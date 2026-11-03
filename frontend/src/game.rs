//! Session logic: the move queue and animation, history / undo / redo,
//! timed-solve phases, the sequence player, page commands and the state that
//! is mirrored to the page.

use crate::bridge::{self, json_array, json_str, Json, PlayerOp, UiCommand};
use crate::cube::{
    apply_move_instant, max_coord, rotate_grid, snap_rotation, spawn_cube, CubeAssets, CubeMove, GridPos,
    BACKGROUND_COUNT, LAYER_ALL, SCHEMES,
};
use crate::input::{OrbitCamera, PointerState};
use crate::notation::{format_alg, format_move, parse_alg, parse_alg_limited};
use crate::persist;
use crate::scramble;
use crate::solver::{self, SolverContext};
use crate::trainer::{self, Category, Trainer, ALGS};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::RequestRedraw;
use std::collections::VecDeque;

const ROTATION_DURATION: f32 = 0.18;
const ROTATION_DURATION_FAST: f32 = 0.07;
const ROTATION_DURATION_TURBO: f32 = 0.03;
/// Cubes up to this size animate their scramble; bigger ones apply it instantly.
const ANIMATED_SCRAMBLE_MAX: i32 = 7;

// ── Resources ─────────────────────────────────────────────────────────────────

/// Cube size (n x n x n), 2..=20.
#[derive(Resource)]
pub struct CubeSize(pub i32);

/// Move queue; bool = user move (recorded into history when it starts).
#[derive(Resource, Default)]
pub struct MoveQueue(pub VecDeque<(CubeMove, bool)>);

/// Moves from the solved state to the current one, in order.
#[derive(Resource, Default)]
pub struct MoveHistory(pub Vec<CubeMove>);

/// Moves undone with Undo, ready to be re-executed. Any new move clears it.
#[derive(Resource, Default)]
pub struct RedoStack(pub Vec<CubeMove>);

pub struct RotationState {
    pub mv: CubeMove,
    pub elapsed: f32,
    pub duration: f32,
    pub entities: Vec<Entity>,
    pub initial_transforms: Vec<Transform>,
    /// Nonzero when the animation continues a manual drag.
    pub start_angle: f32,
}

#[derive(Resource, Default)]
pub struct ActiveRotation(pub Option<RotationState>);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Phase {
    /// Free play: no timer.
    #[default]
    Idle,
    /// The scramble is animating; its moves aren't counted.
    Scrambling,
    /// Scrambled; the timer starts on the first turn (inspection runs in the page).
    Ready,
    Running,
    Solved,
}

impl Phase {
    fn code(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Scrambling => "scrambling",
            Phase::Ready => "ready",
            Phase::Running => "running",
            Phase::Solved => "solved",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Free,
    Scramble,
    Daily(u32),
    Trainer(usize),
    /// Watching a recorded solve of that day's challenge.
    Replay(u32),
}

impl Mode {
    fn code(self) -> String {
        match self {
            Mode::Free => "free".into(),
            Mode::Scramble => "scramble".into(),
            Mode::Daily(d) => format!("daily:{d}"),
            Mode::Trainer(i) => format!("trainer:{i}"),
            Mode::Replay(d) => format!("replay:{d}"),
        }
    }
}

#[derive(Resource, Default)]
pub struct Game {
    pub phase: Phase,
    pub mode: Mode,
    /// Turns counted in the current timed solve (rotations excluded).
    pub moves: u32,
    /// Real time of the first and of the latest counted turn.
    pub start: f64,
    pub last_move: f64,
    /// Notation of the current scramble / setup, for display and sharing.
    pub scramble: String,
    /// Set when a move lands, so solved-detection only runs when needed.
    pub check_solved: bool,
    /// Page asked for a full re-announce (catalog + state).
    pub announce: bool,
    /// Every turn physically made since the scramble finished (rotations,
    /// undos and redos included, in order). Replaying the scramble and then
    /// this log must solve the cube: it's the proof a daily result is sent
    /// with, checked by the leaderboard server.
    pub solution: Vec<CubeMove>,
    pub solution_overflow: bool,
}

/// Longest solution log kept; past this the proof is dropped (no ranked
/// submission), since no real solve needs that many turns.
pub const MAX_SOLUTION_LOG: usize = 2000;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PlayerKind {
    /// Playing the moves leads back to solved (solver, rewind, "show me").
    /// History is kept in sync by popping as moves play.
    #[default]
    Solution,
    /// Applying an algorithm to the current state; moves extend history.
    Apply,
}

/// Step-by-step playback of a move sequence, with play/pause/prev/next.
#[derive(Resource, Default)]
pub struct Player {
    pub active: bool,
    pub playing: bool,
    pub kind: PlayerKind,
    pub moves: Vec<CubeMove>,
    pub idx: usize,
    pub title: String,
    pub version: u32,
}

/// Scene operations that need a settled cube of the right size.
pub enum SceneOp {
    Reset,
    Scramble { moves: Vec<CubeMove>, daily: Option<u32> },
    ScrambleInstant { moves: Vec<CubeMove> },
    Setup { moves: Vec<CubeMove>, ready: bool },
    Trainer { case: usize, auf: bool },
    Replay { day: u32, scramble: Vec<CubeMove>, solution: Vec<CubeMove> },
}

#[derive(Resource, Default)]
pub struct Pending(pub Option<SceneOp>);

/// Engine-side settings (the page owns the rest and forwards these).
#[derive(Resource, Clone, PartialEq)]
pub struct Settings {
    pub speed: f32,
    pub scheme: usize,
    pub background: usize,
    pub shadows: bool,
    pub dpr_cap: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { speed: 1.0, scheme: 0, background: 0, shadows: true, dpr_cap: 2.0 }
    }
}

impl Settings {
    /// `key=value;key=value` with every value range-checked; unknown keys
    /// and bad values are ignored.
    pub fn apply_str(&mut self, s: &str) {
        for pair in s.split(';').take(16) {
            let Some((k, v)) = pair.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "speed" => {
                    if let Some(x) = v.parse::<f32>().ok().filter(|x| x.is_finite() && (0.25..=4.0).contains(x)) {
                        self.speed = x;
                    }
                }
                "scheme" => {
                    if let Some(x) = v.parse::<usize>().ok().filter(|&x| x < SCHEMES.len()) {
                        self.scheme = x;
                    }
                }
                "bg" => {
                    if let Some(x) = v.parse::<usize>().ok().filter(|&x| x < BACKGROUND_COUNT) {
                        self.background = x;
                    }
                }
                "shadows" => self.shadows = v == "1",
                "dpr" => {
                    if let Some(x) = v.parse::<f32>().ok().filter(|x| x.is_finite() && (0.75..=3.0).contains(x)) {
                        self.dpr_cap = x;
                    }
                }
                _ => {}
            }
        }
    }

    pub fn to_engine_string(&self) -> String {
        format!(
            "speed={};scheme={};bg={};shadows={};dpr={}",
            self.speed, self.scheme, self.background, self.shadows as u8, self.dpr_cap
        )
    }

    pub fn from_storage() -> Self {
        let mut s = Self::default();
        if let Some(saved) = persist::get(persist::KEY_ENGINE) {
            s.apply_str(&saved);
        }
        s
    }
}

/// Marks the light that casts shadows.
#[derive(Component)]
pub struct ShadowLight;

/// Point-light shadows render a cubemap over every cubie; past 7x7 that's
/// too expensive for phones.
fn shadows_ok(n: i32, settings: &Settings) -> bool {
    settings.shadows && n <= 7
}

/// Number of surface cubies of an n-cube.
pub fn surface_count(n: i32) -> usize {
    let inner = (n - 2).max(0);
    (n * n * n - inner * inner * inner) as usize
}

// ── The bundle most systems work on ───────────────────────────────────────────

#[derive(SystemParam)]
pub struct Session<'w> {
    pub size: ResMut<'w, CubeSize>,
    pub queue: ResMut<'w, MoveQueue>,
    pub history: ResMut<'w, MoveHistory>,
    pub redo: ResMut<'w, RedoStack>,
    pub game: ResMut<'w, Game>,
    pub player: ResMut<'w, Player>,
    pub active: ResMut<'w, ActiveRotation>,
    pub solver: ResMut<'w, SolverContext>,
    pub trainer: ResMut<'w, Trainer>,
    pub pending: ResMut<'w, Pending>,
    pub pointer: ResMut<'w, PointerState>,
    pub real: Res<'w, Time<Real>>,
}

impl Session<'_> {
    pub fn settled(&self) -> bool {
        self.queue.0.is_empty() && self.active.0.is_none() && self.pointer.manual.is_none()
    }

    /// Real (wall-clock) time: unlike virtual time it isn't clamped when a
    /// frame stalls or the tab is in the background, so solve times are honest.
    pub fn now(&self) -> f64 {
        self.real.elapsed_seconds_f64()
    }

    pub fn push_history(&mut self, mv: CubeMove) {
        self.history.0.push(mv);
        let len = self.history.0.len();
        if len > persist::MAX_HISTORY {
            self.history.0.drain(..len - persist::MAX_HISTORY);
        }
    }

    /// Timer bookkeeping for a turn the user made. Whole-cube rotations are
    /// free (allowed during inspection, not counted).
    pub fn note_move(&mut self, mv: &CubeMove) {
        if mv.layer == LAYER_ALL {
            return;
        }
        let now = self.now();
        match self.game.phase {
            Phase::Ready => {
                self.game.phase = Phase::Running;
                self.game.start = now;
                self.game.last_move = now;
                self.game.moves = 1;
                bridge::emit("event", &Json::new().str("type", "start").done());
            }
            Phase::Running => {
                self.game.moves += 1;
                self.game.last_move = now;
            }
            _ => {}
        }
    }

    /// A user move that's committed right away (manual drag release).
    pub fn record_user_move(&mut self, mv: CubeMove) {
        self.close_player();
        self.push_history(mv);
        self.redo.0.clear();
        self.note_move(&mv);
        bridge::emit("event", &Json::new().str("type", "turn").done());
    }

    pub fn close_player(&mut self) {
        if self.player.active {
            self.player.active = false;
            self.player.playing = false;
            self.player.moves.clear();
            self.player.idx = 0;
        }
    }

    fn start_player(&mut self, moves: Vec<CubeMove>, kind: PlayerKind, title: &str, autoplay: bool) {
        self.redo.0.clear();
        self.game.phase = Phase::Idle;
        self.player.active = true;
        self.player.playing = autoplay && !moves.is_empty();
        self.player.kind = kind;
        self.player.moves = moves;
        self.player.idx = 0;
        self.player.title = title.to_owned();
        self.player.version = self.player.version.wrapping_add(1);
    }

    fn player_step(&mut self, forward: bool) {
        if !self.player.active || !self.settled() {
            return;
        }
        if forward {
            let Some(&mv) = self.player.moves.get(self.player.idx) else { return };
            self.player.idx += 1;
            match self.player.kind {
                PlayerKind::Solution => {
                    self.history.0.pop();
                }
                PlayerKind::Apply => self.push_history(mv),
            }
            self.queue.0.push_back((mv, false));
        } else {
            if self.player.idx == 0 {
                return;
            }
            self.player.idx -= 1;
            let mv = self.player.moves[self.player.idx];
            match self.player.kind {
                PlayerKind::Solution => self.push_history(mv.inverse()),
                PlayerKind::Apply => {
                    self.history.0.pop();
                }
            }
            self.queue.0.push_back((mv.inverse(), false));
        }
    }

    /// Plays the history backwards — the "cube undoes itself" path, and SOLVE
    /// on cubes the two-phase solver doesn't cover.
    fn rewind(&mut self, autoplay: bool) {
        if self.history.0.is_empty() || !self.settled() {
            return;
        }
        let moves: Vec<CubeMove> = self.history.0.iter().rev().map(|m| m.inverse()).collect();
        self.close_player();
        self.start_player(moves, PlayerKind::Solution, "rewind", autoplay);
    }

    fn error(&self, cmd: &str, code: &str) {
        bridge::emit("event", &Json::new().str("type", "error").str("cmd", cmd).str("code", code).done());
    }

    fn parse(&self, cmd: &str, alg: &str) -> Option<Vec<CubeMove>> {
        match parse_alg(alg, self.size.0) {
            Ok(m) if !m.is_empty() => Some(m),
            Ok(_) => {
                self.error(cmd, "empty");
                None
            }
            Err(e) => {
                self.error(cmd, &e.code());
                None
            }
        }
    }
}

fn random_seed() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static FALLBACK: AtomicU64 = AtomicU64::new(0x5EED_CAFE_F00D_D00D);
    let mut b = [0u8; 8];
    if getrandom::fill(&mut b).is_err() {
        // Scrambles don't need to be unpredictable, only varied.
        return FALLBACK.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    }
    u64::from_le_bytes(b)
}

// ── Startup ───────────────────────────────────────────────────────────────────

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    size: Res<CubeSize>,
    settings: Res<Settings>,
) {
    // Lights ride along with the camera, so whatever side the player orbits
    // to, the three visible faces read as three distinct shades: key light
    // from above-left of the viewer, a cool fill from the right.
    commands
        .spawn(Camera3dBundle {
            transform: Transform::from_xyz(4.5, 3.4, 8.2).looking_at(Vec3::ZERO, Vec3::Y),
            // LUT-free tonemapping: lets the build drop the KTX2/zstd stack.
            tonemapping: bevy::core_pipeline::tonemapping::Tonemapping::AcesFitted,
            // A touch brighter than Blender's default (9.7): stickers should
            // read as bright plastic, not as a dim studio shot.
            exposure: bevy::render::camera::Exposure { ev100: 9.4 },
            ..default()
        })
        .with_children(|camera| {
            camera.spawn((
                DirectionalLightBundle {
                    directional_light: DirectionalLight {
                        illuminance: 6_000.0,
                        shadows_enabled: shadows_ok(size.0, &settings),
                        ..default()
                    },
                    transform: Transform::IDENTITY.looking_to(Vec3::new(0.05, -0.55, -1.0), Vec3::Y),
                    // WebGL2 gets a single cascade: keep it tight around the
                    // cube (shadows are only on up to 7x7) for crisp edges.
                    cascade_shadow_config: bevy::pbr::CascadeShadowConfigBuilder {
                        num_cascades: 1,
                        minimum_distance: 0.1,
                        first_cascade_far_bound: 45.0,
                        maximum_distance: 45.0,
                        overlap_proportion: 0.0,
                    }
                    .build(),
                    ..default()
                },
                ShadowLight,
            ));
            camera.spawn(DirectionalLightBundle {
                directional_light: DirectionalLight {
                    illuminance: 2_600.0,
                    color: Color::srgb(0.85, 0.9, 1.0),
                    ..default()
                },
                transform: Transform::IDENTITY.looking_to(Vec3::new(-0.8, -0.2, -0.6), Vec3::Y),
                ..default()
            });
        });
    commands.insert_resource(AmbientLight { color: Color::WHITE, brightness: 420.0 });

    let assets = CubeAssets::new(&mut meshes, &mut materials, settings.scheme);
    spawn_cube(&mut commands, &assets, size.0);
    commands.insert_resource(assets);
}

/// Rebuilds the cube from the saved history, so a reload loses nothing.
pub fn restore_state(mut s: Session, mut cubies: Query<(&mut GridPos, &mut Transform)>) {
    let (history, redo) = persist::load_moves(s.size.0);
    for mv in &history {
        apply_move_instant(&mut cubies, mv);
    }
    s.history.0 = history;
    s.redo.0 = redo;
}

#[cfg(target_arch = "wasm32")]
pub fn install_waker(proxy: Option<NonSend<bevy::winit::EventLoopProxy<bevy::winit::WakeUp>>>) {
    if let Some(p) = proxy {
        bridge::set_waker((*p).clone());
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn install_waker() {}

// ── Page commands ─────────────────────────────────────────────────────────────

pub fn handle_commands(mut s: Session, mut settings: ResMut<Settings>, mut cam: ResMut<OrbitCamera>) {
    for cmd in bridge::drain() {
        match cmd {
            UiCommand::Hello => s.game.announce = true,
            UiCommand::Size(n) => {
                if n != s.size.0 {
                    s.size.0 = n;
                }
            }
            UiCommand::Scramble { seed, daily } => {
                if daily.is_some() && s.size.0 != 3 {
                    s.size.0 = 3;
                }
                let moves = scramble::generate(seed.unwrap_or_else(random_seed), s.size.0);
                s.close_player();
                s.queue.0.clear();
                s.solver.pending = false;
                s.pending.0 = Some(SceneOp::Scramble { moves, daily });
            }
            UiCommand::ScrambleAlg(text) => {
                if let Some(moves) = s.parse("scramble_alg", &text) {
                    s.close_player();
                    s.pending.0 = Some(SceneOp::ScrambleInstant { moves });
                }
            }
            UiCommand::Replay { day, solution } => {
                // Recorded solves are 3x3, logged by this engine (at most
                // MAX_SOLUTION_LOG turns) and checked by the server.
                match parse_alg_limited(&solution, 3, bridge::MAX_REPLAY_LEN, MAX_SOLUTION_LOG) {
                    Ok(solution) if !solution.is_empty() => {
                        if s.size.0 != 3 {
                            s.size.0 = 3;
                        }
                        let scramble = scramble::generate(bridge::daily_seed(day), 3);
                        s.close_player();
                        s.queue.0.clear();
                        s.solver.pending = false;
                        s.pending.0 = Some(SceneOp::Replay { day, scramble, solution });
                    }
                    Ok(_) => s.error("replay", "empty"),
                    Err(e) => s.error("replay", &e.code()),
                }
            }
            UiCommand::Solve { autoplay } => {
                if s.size.0 == 3 {
                    s.close_player();
                    s.solver.pending = true;
                    s.solver.autoplay = autoplay;
                    s.game.phase = Phase::Idle;
                } else {
                    s.rewind(autoplay);
                }
            }
            UiCommand::Rewind => s.rewind(true),
            UiCommand::Undo => {
                if s.settled() && !s.player.active {
                    if let Some(mv) = s.history.0.pop() {
                        s.redo.0.push(mv);
                        let inv = mv.inverse();
                        s.note_move(&inv);
                        s.queue.0.push_back((inv, false));
                    }
                }
            }
            UiCommand::Redo => {
                if s.settled() && !s.player.active {
                    if let Some(mv) = s.redo.0.pop() {
                        s.queue.0.push_back((mv, true));
                    }
                }
            }
            UiCommand::Reset => {
                s.close_player();
                s.queue.0.clear();
                s.pending.0 = Some(SceneOp::Reset);
            }
            UiCommand::Move(tok) => {
                // Keyboard / keypad input: ignored while a scramble animates.
                if s.game.phase == Phase::Scrambling || s.queue.0.len() > 32 {
                    continue;
                }
                if let Some(moves) = s.parse("move", &tok) {
                    s.redo.0.clear();
                    for mv in moves.into_iter().take(8) {
                        s.queue.0.push_back((mv, true));
                    }
                }
            }
            UiCommand::PlayAlg(text) => {
                if let Some(moves) = s.parse("play_alg", &text) {
                    if s.settled() {
                        s.close_player();
                        s.start_player(moves, PlayerKind::Apply, "alg", true);
                    }
                }
            }
            UiCommand::Setup { alg, inverse } => {
                if let Some(moves) = s.parse(if inverse { "setup_inverse" } else { "setup" }, &alg) {
                    let moves = if inverse { trainer::inverse_alg(&moves) } else { moves };
                    s.close_player();
                    s.queue.0.clear();
                    s.pending.0 = Some(SceneOp::Setup { moves, ready: inverse });
                }
            }
            UiCommand::Trainer { case, auf } => {
                if s.size.0 != 3 {
                    s.size.0 = 3;
                }
                s.close_player();
                s.queue.0.clear();
                s.pending.0 = Some(SceneOp::Trainer { case, auf });
            }
            UiCommand::Player(op) => match op {
                PlayerOp::Play => {
                    if s.player.active {
                        if s.player.idx >= s.player.moves.len() {
                            // Replay from the start: step back to the beginning first.
                            while s.player.idx > 0 && s.queue.0.len() < 4096 {
                                s.player.idx -= 1;
                                let mv = s.player.moves[s.player.idx];
                                match s.player.kind {
                                    PlayerKind::Solution => s.push_history(mv.inverse()),
                                    PlayerKind::Apply => {
                                        s.history.0.pop();
                                    }
                                }
                                s.queue.0.push_back((mv.inverse(), false));
                            }
                        }
                        s.player.playing = true;
                    }
                }
                PlayerOp::Pause => s.player.playing = false,
                PlayerOp::Next => {
                    s.player.playing = false;
                    s.player_step(true);
                }
                PlayerOp::Prev => {
                    s.player.playing = false;
                    s.player_step(false);
                }
                PlayerOp::Start => {
                    s.player.playing = false;
                    while s.player.active && s.player.idx > 0 {
                        // Queue the whole way back in one go.
                        s.player.idx -= 1;
                        let mv = s.player.moves[s.player.idx];
                        match s.player.kind {
                            PlayerKind::Solution => s.push_history(mv.inverse()),
                            PlayerKind::Apply => {
                                s.history.0.pop();
                            }
                        }
                        s.queue.0.push_back((mv.inverse(), false));
                    }
                }
                PlayerOp::End => {
                    s.player.playing = false;
                    while s.player.active && s.player.idx < s.player.moves.len() {
                        let mv = s.player.moves[s.player.idx];
                        s.player.idx += 1;
                        match s.player.kind {
                            PlayerKind::Solution => {
                                s.history.0.pop();
                            }
                            PlayerKind::Apply => s.push_history(mv),
                        }
                        s.queue.0.push_back((mv, false));
                    }
                }
                PlayerOp::Close => s.close_player(),
            },
            UiCommand::ViewReset => cam.reset(),
            UiCommand::View(top, bottom) => cam.inset = Vec2::new(top, bottom),
            UiCommand::Export => {
                // Share links stay short: past a few hundred moves, don't bother.
                let alg = if s.history.0.len() <= 400 { format_alg(&s.history.0, s.size.0) } else { String::new() };
                let ev = Json::new()
                    .str("type", "export")
                    .str("alg", &alg)
                    .num("n", s.size.0)
                    .num("len", s.history.0.len())
                    .done();
                bridge::emit("event", &ev);
            }
            UiCommand::Settings(text) => {
                let mut next = settings.clone();
                next.apply_str(&text);
                if next != *settings {
                    persist::set(persist::KEY_ENGINE, &next.to_engine_string());
                    *settings = next;
                }
            }
        }
    }
}

// ── Scene maintenance ─────────────────────────────────────────────────────────

/// When the size changes: rebuild the scene and reset the session.
#[allow(clippy::too_many_arguments)]
pub fn rebuild_cube(
    mut s: Session,
    mut commands: Commands,
    assets: Res<CubeAssets>,
    settings: Res<Settings>,
    cubies: Query<Entity, With<GridPos>>,
    mut cam: ResMut<OrbitCamera>,
    mut shadow_light: Query<&mut DirectionalLight, With<ShadowLight>>,
) {
    if !s.size.is_changed() || s.size.is_added() {
        return;
    }
    for entity in &cubies {
        commands.entity(entity).despawn_recursive();
    }
    spawn_cube(&mut commands, &assets, s.size.0);
    if let Ok(mut light) = shadow_light.get_single_mut() {
        light.shadows_enabled = shadows_ok(s.size.0, &settings);
    }
    s.queue.0.clear();
    s.history.0.clear();
    s.redo.0.clear();
    s.active.0 = None;
    s.pointer.manual = None;
    s.pointer.drag = None;
    s.solver.pending = false;
    s.close_player();
    s.trainer.current = None;
    s.game.phase = Phase::Idle;
    s.game.mode = Mode::Free;
    s.game.scramble.clear();
    cam.zoom = 1.0;
    persist::set(persist::KEY_SIZE, &s.size.0.to_string());
}

/// Puts every cubie back home: its home position follows from its
/// orientation (home = q⁻¹ · pos). Instant, no respawn.
fn reset_instant(cubies: &mut Query<(&mut GridPos, &mut Transform)>) {
    for (mut gp, mut tf) in cubies.iter_mut() {
        let home = (tf.rotation.conjugate() * gp.0.as_vec3()).round().as_ivec3();
        gp.0 = home;
        tf.translation = home.as_vec3() * 0.5;
        tf.rotation = Quat::IDENTITY;
    }
}

/// Runs a queued [`SceneOp`] once the cube is settled and has the right size.
pub fn apply_pending(mut s: Session, mut cubies: Query<(&mut GridPos, &mut Transform)>) {
    if s.pending.0.is_none() || !s.settled() || cubies.iter().count() != surface_count(s.size.0) {
        return;
    }
    if matches!(s.pending.0, Some(SceneOp::Trainer { .. } | SceneOp::Replay { .. })) && s.size.0 != 3 {
        return;
    }
    let Some(op) = s.pending.0.take() else { return };
    let n = s.size.0;

    reset_instant(&mut cubies);
    s.history.0.clear();
    s.redo.0.clear();
    s.trainer.current = None;
    s.game.phase = Phase::Idle;
    s.game.mode = Mode::Free;
    s.game.moves = 0;
    s.game.scramble.clear();

    let mut ready = false;
    match op {
        SceneOp::Reset => {}
        SceneOp::Scramble { moves, daily } => {
            s.game.mode = daily.map(Mode::Daily).unwrap_or(Mode::Scramble);
            s.game.scramble = format_alg(&moves, n);
            if n <= ANIMATED_SCRAMBLE_MAX {
                s.game.phase = Phase::Scrambling;
                s.queue.0.extend(moves.into_iter().map(|m| (m, true)));
            } else {
                for mv in &moves {
                    apply_move_instant(&mut cubies, mv);
                    s.push_history(*mv);
                }
                ready = true;
            }
        }
        SceneOp::ScrambleInstant { moves } => {
            s.game.mode = Mode::Scramble;
            s.game.scramble = format_alg(&moves, n);
            for mv in &moves {
                apply_move_instant(&mut cubies, mv);
                s.push_history(*mv);
            }
            ready = true;
        }
        SceneOp::Setup { moves, ready: arm } => {
            s.game.scramble = format_alg(&moves, n);
            for mv in &moves {
                apply_move_instant(&mut cubies, mv);
                s.push_history(*mv);
            }
            if arm {
                s.game.mode = Mode::Scramble;
                ready = true;
            }
        }
        SceneOp::Trainer { case, auf } => {
            let alg = trainer::alg_moves(case);
            let pattern = ALGS[case].category == Category::Pattern;
            let mut setup = if pattern { alg } else { trainer::inverse_alg(&alg) };
            if auf && !pattern {
                let quarters = [-1i8, 1, 2][(random_seed() % 3) as usize];
                setup.push(CubeMove::new(1, max_coord(3), 1, quarters));
            }
            for mv in &setup {
                apply_move_instant(&mut cubies, mv);
                s.push_history(*mv);
            }
            s.trainer.current = Some(case);
            s.game.mode = Mode::Trainer(case);
            s.game.scramble = format_alg(&setup, 3);
            ready = !pattern;
        }
        SceneOp::Replay { day, scramble, solution } => {
            for mv in &scramble {
                apply_move_instant(&mut cubies, mv);
                s.push_history(*mv);
            }
            s.game.scramble = format_alg(&scramble, 3);
            s.start_player(solution, PlayerKind::Apply, "replay", true);
            s.game.mode = Mode::Replay(day);
        }
    }
    if ready {
        mark_ready(&mut s.game);
    }
}

/// Scrambled and settled: the timer arms and the solution log starts.
fn mark_ready(game: &mut Game) {
    game.phase = Phase::Ready;
    game.solution.clear();
    game.solution_overflow = false;
    bridge::emit("event", &Json::new().str("type", "ready").str("mode", &game.mode.code()).done());
}

/// Runs the two-phase solve requested by SOLVE (3x3 only). Table generation
/// (~1 s on wasm, once per session) waits one frame so the page can paint
/// its "working" state first.
pub fn run_solver(mut s: Session, cubies: Query<(&GridPos, &Transform)>) {
    if !s.solver.pending {
        return;
    }
    if s.size.0 != 3 {
        s.solver.pending = false;
        return;
    }
    if !s.settled() {
        return;
    }
    if s.solver.table.is_none() {
        if !s.solver.armed {
            s.solver.armed = true;
            return;
        }
        s.solver.table = Some(kewb::DataTable::default());
        return;
    }
    s.solver.pending = false;
    let scene: Vec<(IVec3, Quat)> = cubies.iter().map(|(gp, tf)| (gp.0 / 2, tf.rotation)).collect();
    let Some(moves) = solver::solve_scene(s.solver.table.as_ref().unwrap(), &scene) else {
        s.error("solve", "unreadable");
        return;
    };
    if moves.is_empty() {
        s.error("solve", "already_solved");
        return;
    }
    // The solution defines the way back; history becomes its inverse so
    // undo / rewind stay consistent while it plays.
    s.history.0 = moves.iter().rev().map(|m| m.inverse()).collect();
    let autoplay = s.solver.autoplay;
    s.start_player(moves, PlayerKind::Solution, "solver", autoplay);
}

pub fn advance_player(mut s: Session) {
    if !s.player.active || !s.player.playing || !s.settled() {
        return;
    }
    if s.player.idx >= s.player.moves.len() {
        s.player.playing = false;
        return;
    }
    s.player_step(true);
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

pub fn process_rotation(
    mut s: Session,
    settings: Res<Settings>,
    mut cubie_query: Query<(Entity, &mut GridPos, &mut Transform)>,
    time: Res<Time>,
) {
    let mut finished = false;
    if let Some(state) = s.active.0.as_mut() {
        // Clamp: after an idle stretch the first frame's delta is large.
        state.elapsed += time.delta_seconds().min(0.05);
        let t = (state.elapsed / state.duration).min(1.0);
        let angle = state.start_angle + (state.mv.angle() - state.start_angle) * smoothstep(t);
        let q = Quat::from_axis_angle(state.mv.rotation_axis(), angle);
        for (i, &entity) in state.entities.iter().enumerate() {
            if let Ok((_, _, mut tf)) = cubie_query.get_mut(entity) {
                tf.translation = q * state.initial_transforms[i].translation;
                tf.rotation = q * state.initial_transforms[i].rotation;
            }
        }
        finished = t >= 1.0;
    }

    if finished {
        let state = s.active.0.take().unwrap();
        for &entity in &state.entities {
            if let Ok((_, mut gp, mut tf)) = cubie_query.get_mut(entity) {
                gp.0 = rotate_grid(gp.0, state.mv.axis, state.mv.quarters);
                tf.translation = gp.0.as_vec3() * 0.5;
                tf.rotation = snap_rotation(tf.rotation);
            }
        }
        s.game.check_solved = true;
        // Log the turn for the solve proof. Scramble moves land while the
        // phase is still Scrambling, solver playback runs in Idle: neither
        // is logged. A drag released back to 0° isn't a turn.
        if matches!(s.game.phase, Phase::Ready | Phase::Running) && state.mv.quarters != 0 && !s.game.solution_overflow
        {
            if s.game.solution.len() < MAX_SOLUTION_LOG {
                s.game.solution.push(state.mv);
            } else {
                s.game.solution.clear();
                s.game.solution_overflow = true;
            }
        }
    } else if s.active.0.is_none() && s.pointer.manual.is_none() {
        let Some((mv, record)) = s.queue.0.pop_front() else { return };
        if record {
            // History only reflects moves that actually started.
            if s.game.phase != Phase::Scrambling {
                s.close_player();
            }
            s.push_history(mv);
            s.note_move(&mv);
        }
        let mut entities = Vec::new();
        let mut initial_transforms = Vec::new();
        for (entity, gp, tf) in cubie_query.iter() {
            if mv.affects(gp.0[mv.axis as usize]) {
                entities.push(entity);
                initial_transforms.push(*tf);
            }
        }
        let backlog = s.queue.0.len() + s.player.moves.len().saturating_sub(s.player.idx);
        let base = match backlog {
            40.. => ROTATION_DURATION_TURBO,
            8..=39 if s.game.phase == Phase::Scrambling || !s.player.active => ROTATION_DURATION_FAST,
            _ if s.player.active => ROTATION_DURATION * 1.35,
            _ => ROTATION_DURATION,
        };
        let duration = (if mv.quarters.abs() == 2 { base * 1.3 } else { base }) / settings.speed;
        if s.game.phase != Phase::Scrambling {
            bridge::emit("event", &Json::new().str("type", "turn").done());
        }
        s.active.0 = Some(RotationState { mv, elapsed: 0.0, duration, entities, initial_transforms, start_angle: 0.0 });
    }
}

/// Phase transitions that need a settled cube: scramble finished, and
/// solved detection (on facelets, so invisible center twists don't matter).
pub fn update_phase(mut s: Session, cubies: Query<(&GridPos, &Transform)>) {
    if !s.settled() {
        return;
    }
    match s.game.phase {
        Phase::Scrambling => {
            mark_ready(&mut s.game);
        }
        Phase::Running if s.game.check_solved => {
            let scene: Vec<(IVec3, Quat)> = cubies.iter().map(|(gp, tf)| (gp.0, tf.rotation)).collect();
            if solver::is_solved(&scene, max_coord(s.size.0)) {
                s.game.phase = Phase::Solved;
                let ms = ((s.game.last_move - s.game.start) * 1000.0).max(0.0);
                let ev = Json::new()
                    .str("type", "solved")
                    .float("ms", ms)
                    .num("moves", s.game.moves)
                    .num("n", s.size.0)
                    .str("mode", &s.game.mode.code())
                    .str("scramble", &s.game.scramble)
                    .str(
                        "sol",
                        &if s.game.solution_overflow { String::new() } else { format_alg(&s.game.solution, s.size.0) },
                    )
                    .done();
                bridge::emit("event", &ev);
            }
        }
        _ => {}
    }
    s.game.check_solved = false;
}

pub fn apply_settings(
    settings: Res<Settings>,
    size: Res<CubeSize>,
    assets: Res<CubeAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut shadow_light: Query<&mut DirectionalLight, With<ShadowLight>>,
    mut last: Local<Option<Settings>>,
) {
    if last.as_ref() == Some(&*settings) {
        return;
    }
    if last.as_ref().map(|l| l.scheme) != Some(settings.scheme) {
        assets.set_scheme(&mut materials, settings.scheme);
    }
    if let Ok(mut light) = shadow_light.get_single_mut() {
        light.shadows_enabled = shadows_ok(size.0, &settings);
    }
    *last = Some(settings.clone());
}

/// Throttled persistence: at most a few writes per second during activity.
pub fn persist_state(s: Session, mut dirty: Local<bool>, mut last_write: Local<f64>) {
    if s.history.is_changed() || s.redo.is_changed() {
        *dirty = true;
    }
    let now = s.now();
    if *dirty && now - *last_write > 0.4 {
        persist::set(persist::KEY_HISTORY, &persist::encode(&s.history.0));
        persist::set(persist::KEY_REDO, &persist::encode(&s.redo.0));
        *dirty = false;
        *last_write = now;
    }
}

/// While anything moves, keep frames coming (the engine otherwise sleeps).
pub fn keep_awake(s: Session, mut redraw: EventWriter<RequestRedraw>) {
    let busy = !s.settled()
        || s.solver.pending
        || s.pending.0.is_some()
        || (s.player.active && s.player.playing)
        || s.game.phase == Phase::Scrambling
        || s.size.is_changed();
    if busy {
        redraw.send(RequestRedraw);
    }
}

// ── State mirrored to the page ────────────────────────────────────────────────

fn catalog_json() -> String {
    let algs = json_array(
        ALGS.iter().map(|a| Json::new().str("name", a.name).str("cat", a.category.code()).str("moves", a.moves).done()),
    );
    Json::new().raw("algs", &algs).done()
}

pub fn emit_state(mut s: Session, mut last: Local<String>, mut last_player: Local<u32>) {
    if s.game.announce || last.is_empty() {
        bridge::emit("catalog", &catalog_json());
        last.clear();
        *last_player = s.player.version.wrapping_sub(1);
        s.game.announce = false;
    }
    if s.player.version != *last_player {
        *last_player = s.player.version;
        let n = s.size.0;
        let labels = json_array(s.player.moves.iter().map(|m| json_str(&format_move(m, n))));
        let ev = Json::new()
            .num("v", s.player.version)
            .str("title", &s.player.title)
            .str("kind", if s.player.kind == PlayerKind::Apply { "apply" } else { "solution" })
            .raw("moves", &labels)
            .done();
        bridge::emit("player", &ev);
    }

    let settled = s.settled();
    let n = s.size.0;
    let hist = s.history.0.len();
    let player = Json::new()
        .bool("active", s.player.active)
        .bool("playing", s.player.playing)
        .num("idx", s.player.idx)
        .num("len", s.player.moves.len())
        .num("v", s.player.version)
        .done();
    let state = Json::new()
        .num("n", n)
        .str("phase", s.game.phase.code())
        .str("mode", &s.game.mode.code())
        .num("moves", s.game.moves)
        .num("hist", hist)
        .num("redo", s.redo.0.len())
        .bool("busy", !settled)
        .bool("solving", s.solver.pending)
        .bool("canUndo", settled && hist > 0 && !s.player.active)
        .bool("canRedo", settled && !s.redo.0.is_empty() && !s.player.active)
        .bool("canSolve", !s.solver.pending && (n == 3 || hist > 0))
        .bool("canRewind", settled && hist > 0)
        .str("scramble", &s.game.scramble)
        .num("trainer", s.trainer.current.map(|i| i as i64).unwrap_or(-1))
        .raw("player", &player)
        .done();
    if *last != state {
        bridge::emit("state", &state);
        *last = state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::apply_to;

    #[test]
    fn surface_counts() {
        assert_eq!(surface_count(2), 8);
        assert_eq!(surface_count(3), 26);
        assert_eq!(surface_count(4), 56);
        assert_eq!(surface_count(20), 2168);
    }

    #[test]
    fn settings_are_range_checked() {
        let mut s = Settings::default();
        s.apply_str("speed=9;scheme=99;bg=-1;dpr=NaN;shadows=0;evil=1");
        assert_eq!(s.speed, 1.0);
        assert_eq!(s.scheme, 0);
        assert_eq!(s.background, 0);
        assert_eq!(s.dpr_cap, 2.0);
        assert!(!s.shadows);
        s.apply_str("speed=2.5;scheme=2;bg=3;dpr=1.5;shadows=1");
        let mut t = Settings::default();
        t.apply_str(&s.to_engine_string());
        assert!(t == s, "engine string roundtrip");
    }

    #[test]
    fn reset_math_finds_home() {
        // A cubie moved by R then x returns home via q⁻¹·pos.
        let mut p = IVec3::new(2, 2, 2);
        let mut r = Quat::IDENTITY;
        for mv in parse_alg("R x U'", 3).unwrap() {
            apply_to(&mut p, &mut r, &mv);
        }
        let home = (r.conjugate() * p.as_vec3()).round().as_ivec3();
        assert_eq!(home, IVec3::new(2, 2, 2));
    }
}
