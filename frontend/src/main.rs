mod solver;

use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use solver::SolverContext;
use std::collections::VecDeque;
use std::f32::consts::{FRAC_PI_2, PI};

const CUBIE_SIZE: f32 = 0.92;
const STICKER_SIZE: f32 = 0.78;
const FACE_OFFSET: f32 = CUBIE_SIZE / 2.0 + 0.005;
const ROTATION_DURATION: f32 = 0.18;
// Cand asteapta multe mutari (scramble/solve), animatia accelereaza.
const ROTATION_DURATION_FAST: f32 = 0.07;
// Drag live: prag de la care se alege stratul si sensibilitatea unghiului.
const DRAG_LOCK_THRESHOLD: f32 = 10.0;
const DRAG_ANGLE_PER_PIXEL: f32 = FRAC_PI_2 / 130.0;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Rubik's Cube".into(),
                canvas: Some("#bevy".to_owned()),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: true,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin)
        .insert_resource(ClearColor(Color::srgb(0.1, 0.1, 0.18)))
        .insert_resource(OrbitCamera {
            rotation: Quat::from_euler(EulerRot::YXZ, 0.5, 0.35, 0.0),
            radius: 10.0,
        })
        .insert_resource(PointerState::default())
        .insert_resource(MoveQueue::default())
        .insert_resource(MoveHistory::default())
        .insert_resource(RedoStack::default())
        .insert_resource(GameStats::default())
        .insert_resource(SolverContext::default())
        .insert_resource(ActiveRotation::default())
        .add_systems(Startup, (setup, restore_state).chain())
        .add_systems(Update, (
            pointer_input,
            update_camera_transform,
            camera_zoom,
            keyboard_input,
            process_rotation,
            run_solver,
            update_game_phase,
            persist_state,
            egui_ui,
        ))
        .run();
}

// ── Camera ────────────────────────────────────────────────────────────────────

#[derive(Resource)]
struct OrbitCamera {
    rotation: Quat,
    radius: f32,
}

#[derive(Resource, Default)]
struct PointerState {
    drag: Option<DragData>,
    manual: Option<ManualRotation>,
    prev_pinch_distance: Option<f32>,
}

struct DragData {
    start_screen: Vec2,
    kind: DragKind,
}

#[derive(Clone, Copy)]
enum DragKind {
    Camera,
    Face,
}

/// Rotatie de strat condusa de deget/mouse: stratul urmareste drag-ul in timp
/// real si face snap la multiplu de 90° la ridicare.
struct ManualRotation {
    axis: Vec3,
    layer_axis: u8,
    layer_value: i32,
    entities: Vec<Entity>,
    initial_transforms: Vec<Transform>,
    /// Directia pe ecran care corespunde unghiului pozitiv in jurul axei.
    tangent_screen: Vec2,
    angle: f32,
}

fn apply_rotation_delta(state: &mut OrbitCamera, delta: Vec2) {
    if delta == Vec2::ZERO { return; }
    let sensitivity = 0.006;
    let up = state.rotation * Vec3::Y;
    let yaw_sign = if up.y >= 0.0 { -1.0 } else { 1.0 };
    let yaw = Quat::from_rotation_y(yaw_sign * delta.x * sensitivity);
    state.rotation = yaw * state.rotation;
    let right = state.rotation * Vec3::X;
    let pitch = Quat::from_axis_angle(right, delta.y * sensitivity);
    state.rotation = pitch * state.rotation;
    state.rotation = state.rotation.normalize();
}

// Sistemele Bevy aduna firesc multe resurse; limita clippy nu ajuta aici.
#[allow(clippy::too_many_arguments)]
fn pointer_input(
    mut pointer: ResMut<PointerState>,
    mut cam_state: ResMut<OrbitCamera>,
    mut active_rot: ResMut<ActiveRotation>,
    mut history: ResMut<MoveHistory>,
    mut redo: ResMut<RedoStack>,
    mut stats: ResMut<GameStats>,
    time: Res<Time>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: EventReader<MouseMotion>,
    touches: Res<Touches>,
    mut cubie_q: Query<(Entity, &GridPos, &mut Transform)>,
    mut egui_ctx: EguiContexts,
) {
    let egui_active = egui_ctx.ctx_mut().is_using_pointer() || egui_ctx.ctx_mut().wants_pointer_input();

    if egui_active {
        finish_manual(&mut pointer, &mut active_rot, &mut history, &mut redo, &mut stats, time.elapsed_seconds_f64());
        pointer.drag = None;
        pointer.prev_pinch_distance = None;
        mouse_motion.clear();
        return;
    }

    let Ok((camera, cam_transform)) = camera_q.get_single() else { return; };
    let Ok(window) = windows.get_single() else { return; };

    // 2+ fingers → pinch zoom; rotatia manuala in curs face snap si se incheie.
    let touch_positions: Vec<Vec2> = touches.iter().map(|t| t.position()).collect();
    if touch_positions.len() >= 2 {
        finish_manual(&mut pointer, &mut active_rot, &mut history, &mut redo, &mut stats, time.elapsed_seconds_f64());
        let d = touch_positions[0].distance(touch_positions[1]);
        if let Some(prev) = pointer.prev_pinch_distance {
            cam_state.radius = (cam_state.radius - (d - prev) * 0.03).clamp(3.5, 30.0);
        }
        pointer.prev_pinch_distance = Some(d);
        pointer.drag = None;
        mouse_motion.clear();
        return;
    }

    // Unified single pointer: touch wins over mouse when present.
    let touch_pos = touch_positions.first().copied();

    // Pinch tocmai s-a incheiat: degetul ramas preia camera fara re-apasare.
    if pointer.prev_pinch_distance.take().is_some() {
        if let Some(p) = touch_pos {
            pointer.drag = Some(DragData { start_screen: p, kind: DragKind::Camera });
        }
    }

    let touch_delta: Vec2 = touches.iter().next().map(|t| t.delta()).unwrap_or(Vec2::ZERO);
    let touch_just_pressed = touches.iter_just_pressed().next().map(|t| t.position());
    let touch_ended = touches.iter_just_released().next().is_some()
        || touches.iter_just_canceled().next().is_some();

    let mouse_down = mouse_button.pressed(MouseButton::Left);
    let mouse_just_pressed = mouse_button
        .just_pressed(MouseButton::Left)
        .then(|| window.cursor_position())
        .flatten();
    let mouse_just_released = mouse_button.just_released(MouseButton::Left);

    let pressed_now = touch_just_pressed.or(mouse_just_pressed);
    let pos_now: Option<Vec2> = touch_pos.or_else(|| if mouse_down { window.cursor_position() } else { None });
    let delta_now: Vec2 = if touch_pos.is_some() {
        touch_delta
    } else if mouse_down {
        mouse_motion.read().fold(Vec2::ZERO, |acc, ev| acc + ev.delta)
    } else {
        mouse_motion.clear();
        Vec2::ZERO
    };
    let released = touch_ended || mouse_just_released;

    // Start a new drag: sticker → rotatie de strat, altfel camera.
    if let Some(start_pos) = pressed_now {
        finish_manual(&mut pointer, &mut active_rot, &mut history, &mut redo, &mut stats, time.elapsed_seconds_f64());
        let kind = match raycast_cubie(camera, cam_transform, start_pos, &cubie_q) {
            Some(_) => DragKind::Face,
            None => DragKind::Camera,
        };
        pointer.drag = Some(DragData { start_screen: start_pos, kind });
    }

    // Update ongoing drag.
    if let Some(drag) = pointer.drag.as_ref() {
        let start = drag.start_screen;
        match drag.kind {
            DragKind::Camera => apply_rotation_delta(&mut cam_state, delta_now),
            DragKind::Face => {
                // Alege stratul dupa un mic prag, doar cand nicio animatie nu
                // ruleaza (stratul trebuie sa fie asezat ca sa-i capturam starea).
                if pointer.manual.is_none() && active_rot.0.is_none() {
                    if let Some(now) = pos_now {
                        let total = now - start;
                        if total.length() > DRAG_LOCK_THRESHOLD {
                            pointer.manual = begin_manual(camera, cam_transform, start, total, &cubie_q);
                        }
                    }
                }
                // Stratul urmareste pointerul in timp real.
                if let Some(m) = pointer.manual.as_mut() {
                    if let Some(now) = pos_now {
                        m.angle = ((now - start).dot(m.tangent_screen) * DRAG_ANGLE_PER_PIXEL).clamp(-PI, PI);
                        let q = Quat::from_axis_angle(m.axis, m.angle);
                        for (i, &entity) in m.entities.iter().enumerate() {
                            if let Ok((_, _, mut tf)) = cubie_q.get_mut(entity) {
                                tf.translation = q * m.initial_transforms[i].translation;
                                tf.rotation    = q * m.initial_transforms[i].rotation;
                            }
                        }
                    }
                }
            }
        }
    }

    if released {
        finish_manual(&mut pointer, &mut active_rot, &mut history, &mut redo, &mut stats, time.elapsed_seconds_f64());
        pointer.drag = None;
    }
}

/// Porneste o rotatie manuala: alege axa din planul fetei lovite care se
/// aliniaza cel mai bine cu directia drag-ului si captureaza stratul.
fn begin_manual(
    camera: &Camera,
    cam_transform: &GlobalTransform,
    start_screen: Vec2,
    drag_screen: Vec2,
    cubie_q: &Query<(Entity, &GridPos, &mut Transform)>,
) -> Option<ManualRotation> {
    let (grid_pos, hit_world, normal) = raycast_cubie(camera, cam_transform, start_screen, cubie_q)?;
    let hit_screen = camera.world_to_viewport(cam_transform, hit_world)?;
    let axes = [Vec3::X, Vec3::Y, Vec3::Z];
    let layer_values = [grid_pos.x, grid_pos.y, grid_pos.z];

    let mut best: Option<(f32, usize, Vec2)> = None;
    for (i, axis) in axes.iter().enumerate() {
        // Ca la cubul fizic: un drag pe o fata actioneaza doar straturile din
        // planul ei, nu rotatia in jurul normalei (aia se face din fetele vecine).
        if axis.dot(normal).abs() > 0.5 { continue; }
        let tangent_world = axis.cross(hit_world);
        if tangent_world.length_squared() < 1e-4 { continue; }
        let probe = hit_world + tangent_world.normalize() * 0.2;
        let Some(probe_screen) = camera.world_to_viewport(cam_transform, probe) else { continue; };
        let tangent_screen = probe_screen - hit_screen;
        if tangent_screen.length_squared() < 1.0 { continue; }
        let tangent_unit = tangent_screen.normalize();
        let score = drag_screen.dot(tangent_unit).abs();
        if best.as_ref().is_none_or(|(b, _, _)| score > *b) {
            best = Some((score, i, tangent_unit));
        }
    }
    let (_, axis_idx, tangent_screen) = best?;

    let mut entities = Vec::new();
    let mut initial_transforms = Vec::new();
    for (entity, gp, tf) in cubie_q.iter() {
        let layer_val = match axis_idx { 0 => gp.x, 1 => gp.y, _ => gp.z };
        if layer_val == layer_values[axis_idx] {
            entities.push(entity);
            initial_transforms.push(*tf);
        }
    }
    Some(ManualRotation {
        axis: axes[axis_idx],
        layer_axis: axis_idx as u8,
        layer_value: layer_values[axis_idx],
        entities,
        initial_transforms,
        tangent_screen,
        angle: 0.0,
    })
}

/// Incheie rotatia manuala: snap la cel mai apropiat multiplu de 90°, animat
/// din unghiul curent; mutarea rezultata (daca nu e nula) intra in history.
fn finish_manual(
    pointer: &mut PointerState,
    active_rot: &mut ActiveRotation,
    history: &mut MoveHistory,
    redo: &mut RedoStack,
    stats: &mut GameStats,
    now: f64,
) {
    let Some(m) = pointer.manual.take() else { return; };
    let quarter_turns = (m.angle / FRAC_PI_2).round() as i32;
    let target = quarter_turns as f32 * FRAC_PI_2;
    let mv = CubeMove {
        rotation_axis: m.axis,
        layer_axis: m.layer_axis,
        layer_value: m.layer_value,
        angle: target,
    };
    if quarter_turns != 0 {
        history.0.push(mv);
        redo.0.clear();
        note_recorded_move(stats, now);
    }
    let remaining = (target - m.angle).abs() / FRAC_PI_2;
    active_rot.0 = Some(RotationState {
        cube_move: mv,
        elapsed: 0.0,
        duration: (ROTATION_DURATION * remaining).clamp(0.04, ROTATION_DURATION),
        entities: m.entities,
        initial_transforms: m.initial_transforms,
        start_angle: m.angle,
    });
}

fn update_camera_transform(
    state: Res<OrbitCamera>,
    mut query: Query<&mut Transform, With<Camera3d>>,
) {
    if let Ok(mut transform) = query.get_single_mut() {
        let position = state.rotation * Vec3::new(0.0, 0.0, state.radius);
        transform.translation = position;
        transform.look_at(Vec3::ZERO, state.rotation * Vec3::Y);
    }
}

fn raycast_cubie(
    camera: &Camera,
    cam_transform: &GlobalTransform,
    screen_pos: Vec2,
    cubie_q: &Query<(Entity, &GridPos, &mut Transform)>,
) -> Option<(GridPos, Vec3, Vec3)> {
    let ray = camera.viewport_to_world(cam_transform, screen_pos)?;
    let origin = ray.origin;
    let dir: Vec3 = ray.direction.as_vec3();
    let half = CUBIE_SIZE / 2.0;

    let mut best: Option<(f32, GridPos, Vec3, Vec3)> = None;
    for (_entity, grid_pos, transform) in cubie_q.iter() {
        let inv = transform.compute_matrix().inverse();
        let local_origin = inv.transform_point3(origin);
        let local_dir = inv.transform_vector3(dir);
        if let Some((t, entry_axis)) = ray_aabb_intersect(local_origin, local_dir, Vec3::splat(-half), Vec3::splat(half)) {
            if t > 0.0 && best.as_ref().is_none_or(|(bt, ..)| t < *bt) {
                let mut local_normal = Vec3::ZERO;
                local_normal[entry_axis] = if local_dir[entry_axis] > 0.0 { -1.0 } else { 1.0 };
                let world_normal = (transform.rotation * local_normal).normalize();
                best = Some((t, *grid_pos, origin + dir * t, world_normal));
            }
        }
    }
    best.map(|(_, gp, hw, n)| (gp, hw, n))
}

fn ray_aabb_intersect(origin: Vec3, dir: Vec3, aabb_min: Vec3, aabb_max: Vec3) -> Option<(f32, usize)> {
    let mut tmin = f32::NEG_INFINITY;
    let mut tmax = f32::INFINITY;
    let mut entry_axis = 0usize;
    for i in 0..3 {
        let o = origin[i];
        let d = dir[i];
        if d.abs() < 1e-8 {
            if o < aabb_min[i] || o > aabb_max[i] { return None; }
        } else {
            let t1 = (aabb_min[i] - o) / d;
            let t2 = (aabb_max[i] - o) / d;
            let (tn, tf) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
            if tn > tmin { tmin = tn; entry_axis = i; }
            if tf < tmax { tmax = tf; }
            if tmin > tmax { return None; }
        }
    }
    if tmax < 0.0 { return None; }
    Some(if tmin > 0.0 { (tmin, entry_axis) } else { (tmax, entry_axis) })
}

fn camera_zoom(
    mut state: ResMut<OrbitCamera>,
    mut scroll: EventReader<MouseWheel>,
) {
    for ev in scroll.read() {
        let delta = match ev.unit {
            MouseScrollUnit::Line  => ev.y * 0.5,
            MouseScrollUnit::Pixel => ev.y * 0.005,
        };
        state.radius -= delta;
        state.radius = state.radius.clamp(3.5, 30.0);
    }
}

// ── Cube moves ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
pub struct CubeMove {
    pub rotation_axis: Vec3,
    pub layer_axis: u8,
    pub layer_value: i32,
    pub angle: f32,
}

impl CubeMove {
    // Fete exterioare
    fn r()  -> Self { Self { rotation_axis: Vec3::X, layer_axis: 0, layer_value:  1, angle: -FRAC_PI_2 } }
    fn ri() -> Self { Self { rotation_axis: Vec3::X, layer_axis: 0, layer_value:  1, angle:  FRAC_PI_2 } }
    fn l()  -> Self { Self { rotation_axis: Vec3::X, layer_axis: 0, layer_value: -1, angle:  FRAC_PI_2 } }
    fn li() -> Self { Self { rotation_axis: Vec3::X, layer_axis: 0, layer_value: -1, angle: -FRAC_PI_2 } }
    fn u()  -> Self { Self { rotation_axis: Vec3::Y, layer_axis: 1, layer_value:  1, angle: -FRAC_PI_2 } }
    fn ui() -> Self { Self { rotation_axis: Vec3::Y, layer_axis: 1, layer_value:  1, angle:  FRAC_PI_2 } }
    fn d()  -> Self { Self { rotation_axis: Vec3::Y, layer_axis: 1, layer_value: -1, angle:  FRAC_PI_2 } }
    fn di() -> Self { Self { rotation_axis: Vec3::Y, layer_axis: 1, layer_value: -1, angle: -FRAC_PI_2 } }
    fn f()  -> Self { Self { rotation_axis: Vec3::Z, layer_axis: 2, layer_value:  1, angle: -FRAC_PI_2 } }
    fn fi() -> Self { Self { rotation_axis: Vec3::Z, layer_axis: 2, layer_value:  1, angle:  FRAC_PI_2 } }
    fn b()  -> Self { Self { rotation_axis: Vec3::Z, layer_axis: 2, layer_value: -1, angle:  FRAC_PI_2 } }
    fn bi() -> Self { Self { rotation_axis: Vec3::Z, layer_axis: 2, layer_value: -1, angle: -FRAC_PI_2 } }

    // Felii din mijloc (Slice moves)
    // M: Middle, aceeasi directie ca L (+X)
    fn m()  -> Self { Self { rotation_axis: Vec3::X, layer_axis: 0, layer_value: 0, angle:  FRAC_PI_2 } }
    fn mi() -> Self { Self { rotation_axis: Vec3::X, layer_axis: 0, layer_value: 0, angle: -FRAC_PI_2 } }
    // E: Equator, aceeasi directie ca D (+Y)
    fn e()  -> Self { Self { rotation_axis: Vec3::Y, layer_axis: 1, layer_value: 0, angle:  FRAC_PI_2 } }
    fn ei() -> Self { Self { rotation_axis: Vec3::Y, layer_axis: 1, layer_value: 0, angle: -FRAC_PI_2 } }
    // S: Standing, aceeasi directie ca F (-Z)
    fn s()  -> Self { Self { rotation_axis: Vec3::Z, layer_axis: 2, layer_value: 0, angle: -FRAC_PI_2 } }
    fn si() -> Self { Self { rotation_axis: Vec3::Z, layer_axis: 2, layer_value: 0, angle:  FRAC_PI_2 } }

    fn inverse(self) -> Self {
        Self { angle: -self.angle, ..self }
    }
}

/// Coada de mutari; bool = se inregistreaza in history la executie.
/// Mutarile din Solve nu se inregistreaza, altfel "rezolvarea" s-ar anula singura.
#[derive(Resource, Default)]
pub struct MoveQueue(pub VecDeque<(CubeMove, bool)>);

#[derive(Resource, Default)]
pub struct MoveHistory(pub Vec<CubeMove>);

/// Mutari anulate cu Undo, gata de re-executat. Orice mutare noua il goleste.
#[derive(Resource, Default)]
pub struct RedoStack(pub Vec<CubeMove>);

// ── Game stats (cronometru + contor) ─────────────────────────────────────────

#[derive(Default, Clone, Copy)]
enum Phase {
    /// Joaca libera: fara cronometru.
    #[default]
    Idle,
    /// Scramble-ul se executa; mutarile lui nu se numara.
    Scrambling,
    /// Scramble terminat; cronometrul porneste la prima mutare.
    Ready,
    Running,
    Solved { time: f64, is_best: bool },
}

#[derive(Resource, Default)]
struct GameStats {
    phase: Phase,
    start_time: f64,
    moves: u32,
    best_time: Option<f64>,
}

fn note_recorded_move(stats: &mut GameStats, now: f64) {
    match stats.phase {
        Phase::Ready => {
            stats.phase = Phase::Running;
            stats.start_time = now;
            stats.moves = 1;
        }
        Phase::Running => stats.moves += 1,
        _ => {}
    }
}

fn fmt_time(t: f64) -> String {
    if t < 60.0 {
        format!("{:.2}s", t)
    } else {
        format!("{}:{:04.1}", (t / 60.0) as u32, t % 60.0)
    }
}

pub struct RotationState {
    pub cube_move: CubeMove,
    pub elapsed: f32,
    pub duration: f32,
    pub entities: Vec<Entity>,
    pub initial_transforms: Vec<Transform>,
    /// Unghiul de pornire — nenul cand animatia continua un drag manual.
    pub start_angle: f32,
}

#[derive(Resource, Default)]
pub struct ActiveRotation(pub Option<RotationState>);

// ── Persistenta (localStorage) ────────────────────────────────────────────────

const STORE_HISTORY: &str = "cube.history";
const STORE_REDO: &str = "cube.redo";
const STORE_BEST: &str = "cube.best";

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

#[cfg(target_arch = "wasm32")]
fn storage_set(key: &str, value: &str) {
    if let Some(s) = local_storage() {
        let _ = s.set_item(key, value);
    }
}

#[cfg(target_arch = "wasm32")]
fn storage_get(key: &str) -> Option<String> {
    local_storage()?.get_item(key).ok().flatten()
}

#[cfg(not(target_arch = "wasm32"))]
fn storage_set(_key: &str, _value: &str) {}

#[cfg(not(target_arch = "wasm32"))]
fn storage_get(_key: &str) -> Option<String> {
    None
}

/// O mutare = 3 cifre: axa (0-2), stratul (+1 → 0-2), sferturi de tura (+2 → 0-4).
fn encode_moves(moves: &[CubeMove]) -> String {
    moves
        .iter()
        .map(|m| {
            let quarters = (m.angle / FRAC_PI_2).round() as i32;
            format!("{}{}{}", m.layer_axis, m.layer_value + 1, quarters + 2)
        })
        .collect()
}

fn decode_moves(s: &str) -> Option<Vec<CubeMove>> {
    let bytes = s.as_bytes();
    if !bytes.len().is_multiple_of(3) {
        return None;
    }
    let axes = [Vec3::X, Vec3::Y, Vec3::Z];
    bytes
        .chunks(3)
        .map(|c| {
            let axis = (c[0] as char).to_digit(10)?;
            let value = (c[1] as char).to_digit(10)? as i32 - 1;
            let quarters = (c[2] as char).to_digit(10)? as i32 - 2;
            if axis > 2 || !(-1..=1).contains(&value) || quarters == 0 || !(-2..=2).contains(&quarters) {
                return None;
            }
            Some(CubeMove {
                rotation_axis: axes[axis as usize],
                layer_axis: axis as u8,
                layer_value: value,
                angle: quarters as f32 * FRAC_PI_2,
            })
        })
        .collect()
}

/// La pornire: reconstruieste cubul aplicand instant istoricul salvat, ca un
/// refresh sa nu piarda nici starea si nici capacitatea Solve-ului de a o desface.
fn restore_state(
    mut history: ResMut<MoveHistory>,
    mut redo: ResMut<RedoStack>,
    mut stats: ResMut<GameStats>,
    mut cubies: Query<(&mut GridPos, &mut Transform)>,
) {
    stats.best_time = storage_get(STORE_BEST).and_then(|s| s.parse().ok());
    if let Some(moves) = storage_get(STORE_REDO).and_then(|s| decode_moves(&s)) {
        redo.0 = moves;
    }
    let Some(moves) = storage_get(STORE_HISTORY).and_then(|s| decode_moves(&s)) else {
        return;
    };
    for mv in &moves {
        let q = Quat::from_axis_angle(mv.rotation_axis, mv.angle);
        for (mut gp, mut tf) in cubies.iter_mut() {
            let layer_val = match mv.layer_axis { 0 => gp.x, 1 => gp.y, _ => gp.z };
            if layer_val != mv.layer_value { continue; }
            let (nx, ny, nz) = rotate_grid_pos(gp.x, gp.y, gp.z, mv.rotation_axis, mv.angle);
            gp.x = nx; gp.y = ny; gp.z = nz;
            tf.translation = Vec3::new(nx as f32, ny as f32, nz as f32);
            tf.rotation = snap_rotation(q * tf.rotation);
        }
    }
    history.0 = moves;
}

fn persist_state(history: Res<MoveHistory>, redo: Res<RedoStack>) {
    if history.is_changed() || redo.is_changed() {
        storage_set(STORE_HISTORY, &encode_moves(&history.0));
        storage_set(STORE_REDO, &encode_moves(&redo.0));
    }
}

fn collect_cubies(cubies: &Query<(&GridPos, &Transform)>) -> Vec<(IVec3, Quat)> {
    cubies
        .iter()
        .map(|(gp, tf)| (IVec3::new(gp.x, gp.y, gp.z), tf.rotation))
        .collect()
}

/// Tranzitiile de faza care depind de "cubul s-a asezat": scramble terminat si
/// detectarea starii rezolvate. Detectia foloseste facelets (fete uniforme),
/// nu compararea orientarilor — centrele se pot rasuci invizibil pe loc.
fn update_game_phase(
    queue: Res<MoveQueue>,
    active: Res<ActiveRotation>,
    pointer: Res<PointerState>,
    mut stats: ResMut<GameStats>,
    time: Res<Time>,
    cubies: Query<(&GridPos, &Transform)>,
) {
    let settled = queue.0.is_empty() && active.0.is_none() && pointer.manual.is_none();
    if !settled { return; }

    match stats.phase {
        Phase::Scrambling => stats.phase = Phase::Ready,
        Phase::Running => {
            if solver::is_solved(&collect_cubies(&cubies)) {
                let t = time.elapsed_seconds_f64() - stats.start_time;
                let is_best = stats.best_time.is_none_or(|b| t < b);
                if is_best {
                    stats.best_time = Some(t);
                    storage_set(STORE_BEST, &format!("{t}"));
                }
                stats.phase = Phase::Solved { time: t, is_best };
            }
        }
        _ => {}
    }
}

/// Executa rezolvarea Kociemba ceruta de butonul SOLVE. Ruleaza la un frame
/// dupa click (UI-ul apuca sa arate starea de lucru), asteapta cubul asezat,
/// si genereaza tabelele la prima folosire.
fn run_solver(
    mut ctx: ResMut<SolverContext>,
    mut queue: ResMut<MoveQueue>,
    mut history: ResMut<MoveHistory>,
    mut redo: ResMut<RedoStack>,
    active: Res<ActiveRotation>,
    pointer: Res<PointerState>,
    cubies: Query<(&GridPos, &Transform)>,
) {
    if !ctx.pending { return; }
    let settled = queue.0.is_empty() && active.0.is_none() && pointer.manual.is_none();
    if !settled { return; }

    if ctx.table.is_none() {
        // Generarea dureaza ~1-2s pe wasm; ramanem pending inca un frame ca
        // eticheta "se pregateste" sa fie deja pe ecran in timpul blocarii.
        ctx.table = Some(kewb::DataTable::default());
        return;
    }
    ctx.pending = false;

    let Some(moves) = solver::solve_scene(ctx.table.as_ref().unwrap(), &collect_cubies(&cubies)) else {
        warn!("solver: starea cubului nu a putut fi citita");
        return;
    };
    // Solutia readuce cubul la rezolvat: istoricul vechi nu mai descrie
    // drumul inapoi, deci se goleste.
    history.0.clear();
    redo.0.clear();
    queue.0.extend(moves.into_iter().map(|m| (m, false)));
}

// ── Keyboard input ────────────────────────────────────────────────────────────

fn undo_move(history: &mut MoveHistory, redo: &mut RedoStack, queue: &mut MoveQueue) {
    if let Some(mv) = history.0.pop() {
        redo.0.push(mv);
        queue.0.push_back((mv.inverse(), false));
    }
}

fn redo_move(redo: &mut RedoStack, queue: &mut MoveQueue) {
    if let Some(mv) = redo.0.pop() {
        queue.0.push_back((mv, true));
    }
}

fn keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut queue: ResMut<MoveQueue>,
    mut history: ResMut<MoveHistory>,
    mut redo: ResMut<RedoStack>,
    pointer: Res<PointerState>,
    mut egui_ctx: EguiContexts,
) {
    // Nu captura taste daca egui are focus
    if egui_ctx.ctx_mut().wants_keyboard_input() { return; }

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
        || keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight);

    if ctrl {
        // Undo/redo doar cu coada goala: inversarea unei mutari inca neexecutate
        // ar aplica mutarile in ordinea gresita.
        if keys.just_pressed(KeyCode::KeyZ) && queue.0.is_empty() && pointer.manual.is_none() {
            if shift {
                redo_move(&mut redo, &mut queue);
            } else {
                undo_move(&mut history, &mut redo, &mut queue);
            }
        }
        return;
    }

    let mappings = [
        (KeyCode::KeyR, CubeMove::r(),  CubeMove::ri()),
        (KeyCode::KeyL, CubeMove::l(),  CubeMove::li()),
        (KeyCode::KeyU, CubeMove::u(),  CubeMove::ui()),
        (KeyCode::KeyD, CubeMove::d(),  CubeMove::di()),
        (KeyCode::KeyF, CubeMove::f(),  CubeMove::fi()),
        (KeyCode::KeyB, CubeMove::b(),  CubeMove::bi()),
        (KeyCode::KeyM, CubeMove::m(),  CubeMove::mi()),
        (KeyCode::KeyE, CubeMove::e(),  CubeMove::ei()),
        (KeyCode::KeyS, CubeMove::s(),  CubeMove::si()),
    ];

    for (key, cw, ccw) in &mappings {
        if keys.just_pressed(*key) {
            let mv = if shift { *ccw } else { *cw };
            queue.0.push_back((mv, true));
            redo.0.clear();
        }
    }
}

// ── Rotation animation ────────────────────────────────────────────────────────

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn rotate_grid_pos(x: i32, y: i32, z: i32, axis: Vec3, angle: f32) -> (i32, i32, i32) {
    let q = Quat::from_axis_angle(axis, angle);
    let v = q * Vec3::new(x as f32, y as f32, z as f32);
    (v.x.round() as i32, v.y.round() as i32, v.z.round() as i32)
}

fn snap_rotation(q: Quat) -> Quat {
    let m = Mat3::from_quat(q.normalize());
    let snap = |v: Vec3| Vec3::new(v.x.round(), v.y.round(), v.z.round());
    Quat::from_mat3(&Mat3::from_cols(
        snap(m.col(0)),
        snap(m.col(1)),
        snap(m.col(2)),
    )).normalize()
}

fn process_rotation(
    mut active: ResMut<ActiveRotation>,
    mut move_queue: ResMut<MoveQueue>,
    mut history: ResMut<MoveHistory>,
    mut stats: ResMut<GameStats>,
    pointer: Res<PointerState>,
    mut cubie_query: Query<(Entity, &mut GridPos, &mut Transform)>,
    time: Res<Time>,
) {
    let mut finished = false;

    if let Some(state) = active.0.as_mut() {
        state.elapsed += time.delta_seconds();
        let t = (state.elapsed / state.duration).min(1.0);
        let angle = state.start_angle + (state.cube_move.angle - state.start_angle) * smoothstep(t);
        let q = Quat::from_axis_angle(state.cube_move.rotation_axis, angle);
        for (i, &entity) in state.entities.iter().enumerate() {
            if let Ok((_, _, mut tf)) = cubie_query.get_mut(entity) {
                tf.translation = q * state.initial_transforms[i].translation;
                tf.rotation    = q * state.initial_transforms[i].rotation;
            }
        }
        finished = t >= 1.0;
    }

    if finished {
        let state = active.0.take().unwrap();
        for &entity in &state.entities {
            if let Ok((_, mut gp, mut tf)) = cubie_query.get_mut(entity) {
                let (nx, ny, nz) = rotate_grid_pos(
                    gp.x, gp.y, gp.z,
                    state.cube_move.rotation_axis,
                    state.cube_move.angle,
                );
                gp.x = nx; gp.y = ny; gp.z = nz;
                tf.translation = Vec3::new(nx as f32, ny as f32, nz as f32);
                tf.rotation    = snap_rotation(tf.rotation);
            }
        }
    } else if active.0.is_none() && pointer.manual.is_none() {
        // Coada asteapta cat timp un strat e tinut in mana (drag manual).
        if let Some((mv, record)) = move_queue.0.pop_front() {
            // History reflecta doar mutari executate: push abia la pornirea rotatiei,
            // ca Solve/Scramble apasate in timpul animatiilor sa nu-l corupa.
            if record {
                history.0.push(mv);
                note_recorded_move(&mut stats, time.elapsed_seconds_f64());
            }
            let mut entities = Vec::new();
            let mut initial_transforms = Vec::new();
            for (entity, gp, tf) in cubie_query.iter() {
                let layer_val = match mv.layer_axis { 0 => gp.x, 1 => gp.y, _ => gp.z };
                if layer_val == mv.layer_value {
                    entities.push(entity);
                    initial_transforms.push(*tf);
                }
            }
            let duration = if move_queue.0.len() >= 5 { ROTATION_DURATION_FAST } else { ROTATION_DURATION };
            active.0 = Some(RotationState {
                cube_move: mv,
                elapsed: 0.0,
                duration,
                entities,
                initial_transforms,
                start_angle: 0.0,
            });
        }
    }
}

// ── egui UI ───────────────────────────────────────────────────────────────────

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self { Self(seed.max(1)) }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn range(&mut self, n: usize) -> usize { (self.next() as usize) % n }
}

/// Scramble aleator din mutari de fete, fara acelasi strat de doua ori la
/// rand (mutarile consecutive pe acelasi strat se anuleaza sau se combina).
fn generate_scramble(seed: u64, count: usize) -> Vec<CubeMove> {
    let mut rng = Rng::new(seed);
    let moves = [
        CubeMove::r(), CubeMove::ri(), CubeMove::l(), CubeMove::li(),
        CubeMove::u(), CubeMove::ui(), CubeMove::d(), CubeMove::di(),
        CubeMove::f(), CubeMove::fi(), CubeMove::b(), CubeMove::bi(),
    ];
    let mut out = Vec::with_capacity(count);
    let mut last_layer: Option<(u8, i32)> = None;
    while out.len() < count {
        let mv = moves[rng.range(12)];
        if last_layer == Some((mv.layer_axis, mv.layer_value)) { continue; }
        last_layer = Some((mv.layer_axis, mv.layer_value));
        out.push(mv);
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn egui_ui(
    mut contexts: EguiContexts,
    mut history: ResMut<MoveHistory>,
    mut queue: ResMut<MoveQueue>,
    mut redo: ResMut<RedoStack>,
    mut stats: ResMut<GameStats>,
    mut solver_ctx: ResMut<SolverContext>,
    pointer: Res<PointerState>,
    time: Res<Time>,
) {
    let ctx = contexts.ctx_mut();

    // Layout compact pe ecrane inguste (telefoane): butoane mari, jos, si
    // hint-uri de gesturi in loc de taste.
    let compact = ctx.screen_rect().width() < 700.0;

    // 1. Hint-uri
    if compact {
        egui::Area::new("hints".into())
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 10.0))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("Trage un sticker = rotesti stratul\nTrage in gol = rotesti cubul  ·  Pinch = zoom")
                            .size(12.0)
                            .color(egui::Color32::from_rgba_unmultiplied(220, 220, 220, 120)),
                    );
                });
            });
    } else {
        egui::Area::new("hints".into())
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(12.0, 12.0))
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Fete: R L U D F B  |  Felii: M E S  |  Shift = prime  |  Ctrl+Z = undo, Ctrl+Shift+Z = redo\nDrag pe sticker = roteste stratul  |  Drag in afara = roteste vederea  |  Scroll / pinch = zoom"
                    )
                    .size(12.0)
                    .color(egui::Color32::from_rgba_unmultiplied(220, 220, 220, 120))
                );
            });
    }

    // 2. HUD cronometru + contor (centrat sus, sub hint-urile de pe mobil).
    let hud_text = match stats.phase {
        Phase::Ready => Some("⏱ cronometrul porneste la prima mutare".to_string()),
        Phase::Running => Some(format!(
            "⏱ {}   ·   {} mutari",
            fmt_time(time.elapsed_seconds_f64() - stats.start_time),
            stats.moves
        )),
        _ => None,
    };
    if let Some(text) = hud_text {
        let hud_y = if compact { 54.0 } else { 12.0 };
        egui::Area::new("hud".into())
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, hud_y))
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(text)
                        .size(17.0)
                        .color(egui::Color32::from_rgba_unmultiplied(235, 235, 245, 200)),
                );
            });
    }

    // 3. Celebrare la rezolvare.
    if let Phase::Solved { time: solve_time, is_best } = stats.phase {
        egui::Area::new("solved".into())
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -20.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(egui::Color32::from_rgba_unmultiplied(23, 24, 31, 240))
                    .rounding(14.0)
                    .inner_margin(egui::Margin::symmetric(30.0, 24.0))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("🎉 Rezolvat!").size(26.0).strong().color(egui::Color32::WHITE));
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new(fmt_time(solve_time)).size(36.0).strong().color(egui::Color32::from_rgb(255, 214, 0)));
                            ui.label(egui::RichText::new(format!("{} mutari", stats.moves)).size(16.0).color(egui::Color32::from_rgb(180, 184, 210)));
                            ui.add_space(4.0);
                            if is_best {
                                ui.label(egui::RichText::new("★ Record personal nou!").size(15.0).color(egui::Color32::from_rgb(255, 214, 0)));
                            } else if let Some(best) = stats.best_time {
                                ui.label(egui::RichText::new(format!("Record personal: {}", fmt_time(best))).size(14.0).color(egui::Color32::from_rgb(150, 255, 180)));
                            }
                            ui.add_space(12.0);
                            if ui.add_sized(egui::vec2(130.0, 42.0), egui::Button::new(egui::RichText::new("OK").size(17.0))).clicked() {
                                stats.phase = Phase::Idle;
                            }
                        });
                    });
            });
    }

    // 4. Panou butoane: jos-centrat pe mobil (tinte de atins mari), sus-stanga pe desktop.
    let (anchor, offset) = if compact {
        (egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -20.0))
    } else {
        (egui::Align2::LEFT_TOP, egui::vec2(12.0, 60.0))
    };
    let btn_size = if compact { egui::vec2(104.0, 52.0) } else { egui::vec2(112.0, 32.0) };
    let small_btn = if compact { egui::vec2(150.0, 44.0) } else { egui::vec2(96.0, 32.0) };
    let txt_size = 15.0;

    // Undo/redo/solve sunt valide doar cu cubul asezat.
    let settled = queue.0.is_empty() && pointer.manual.is_none();
    let can_undo = settled && !history.0.is_empty();
    let can_redo = settled && !redo.0.is_empty();
    let can_solve = settled && !solver_ctx.pending;
    let can_rewind = settled && !history.0.is_empty();

    let mut do_scramble = false;
    let mut do_solve = false;
    let mut do_rewind = false;
    let mut do_undo = false;
    let mut do_redo = false;

    egui::Area::new("controls_area".into())
        .anchor(anchor, offset)
        .show(ctx, |ui| {
            let undo_txt = egui::RichText::new("⏴ UNDO").size(txt_size).color(egui::Color32::from_rgb(255, 214, 130));
            let redo_txt = egui::RichText::new("REDO ⏵").size(txt_size).color(egui::Color32::from_rgb(255, 214, 130));
            let scr_txt  = egui::RichText::new("🎲 SCRAMBLE").size(txt_size).color(egui::Color32::from_rgb(180, 180, 255));
            let slv_label = if solver_ctx.pending { "⏳ SOLVE" } else { "✨ SOLVE" };
            let slv_txt  = egui::RichText::new(slv_label).size(txt_size).color(egui::Color32::from_rgb(150, 255, 180));
            let rwd_txt  = egui::RichText::new("⏴⏴ REWIND").size(txt_size).color(egui::Color32::from_rgb(150, 220, 255));

            if compact {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        do_undo = ui.add_enabled(can_undo, egui::Button::new(undo_txt.clone()).min_size(small_btn)).clicked();
                        ui.add_space(8.0);
                        do_redo = ui.add_enabled(can_redo, egui::Button::new(redo_txt.clone()).min_size(small_btn)).clicked();
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        do_scramble = ui.add_sized(btn_size, egui::Button::new(scr_txt.clone())).clicked();
                        ui.add_space(8.0);
                        do_solve = ui.add_enabled(can_solve, egui::Button::new(slv_txt.clone()).min_size(btn_size)).clicked();
                        ui.add_space(8.0);
                        do_rewind = ui.add_enabled(can_rewind, egui::Button::new(rwd_txt.clone()).min_size(btn_size)).clicked();
                    });
                });
            } else {
                ui.horizontal(|ui| {
                    do_scramble = ui.add_sized(btn_size, egui::Button::new(scr_txt)).clicked();
                    ui.add_space(8.0);
                    do_solve = ui.add_enabled(can_solve, egui::Button::new(slv_txt).min_size(btn_size)).clicked();
                    ui.add_space(8.0);
                    do_rewind = ui.add_enabled(can_rewind, egui::Button::new(rwd_txt).min_size(btn_size)).clicked();
                    ui.add_space(8.0);
                    do_undo = ui.add_enabled(can_undo, egui::Button::new(undo_txt).min_size(small_btn)).clicked();
                    ui.add_space(8.0);
                    do_redo = ui.add_enabled(can_redo, egui::Button::new(redo_txt).min_size(small_btn)).clicked();
                });
            }
        });

    if do_scramble {
        queue.0.clear();
        redo.0.clear();
        solver_ctx.pending = false;
        stats.phase = Phase::Scrambling;
        stats.moves = 0;
        let seed = (time.elapsed_seconds() * 100_000.0) as u64;
        for mv in generate_scramble(seed, 20) {
            queue.0.push_back((mv, true));
        }
    }

    // SOLVE: solutie Kociemba (~20 de mutari) din orice stare; ruleaza in
    // run_solver la frame-ul urmator.
    if do_solve {
        solver_ctx.pending = true;
        stats.phase = Phase::Idle;
    }

    // REWIND: deruleaza istoricul invers — drumul "cubul se desface singur".
    if do_rewind && !history.0.is_empty() {
        queue.0.clear();
        redo.0.clear();
        stats.phase = Phase::Idle;
        let solution: Vec<CubeMove> = history.0.drain(..).rev()
            .map(|m| m.inverse())
            .collect();
        queue.0.extend(solution.into_iter().map(|m| (m, false)));
    }

    if do_undo {
        undo_move(&mut history, &mut redo, &mut queue);
    }
    if do_redo {
        redo_move(&mut redo, &mut queue);
    }
}
// ── Cube components ───────────────────────────────────────────────────────────

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GridPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

fn face_color(face: usize) -> Color {
    match face {
        0 => Color::srgb(1.00, 1.00, 1.00),
        1 => Color::srgb(1.00, 0.84, 0.00),
        2 => Color::srgb(0.72, 0.07, 0.02),
        3 => Color::srgb(1.00, 0.35, 0.00),
        4 => Color::srgb(0.00, 0.27, 0.68),
        5 => Color::srgb(0.00, 0.55, 0.22),
        _ => unreachable!(),
    }
}

// ── Setup ─────────────────────────────────────────────────────────────────────

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(4.5, 3.4, 8.2).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });

    commands.spawn(PointLightBundle {
        point_light: PointLight { intensity: 4_000_000.0, shadows_enabled: true, ..default() },
        transform: Transform::from_xyz(6.0, 10.0, 6.0),
        ..default()
    });
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 1_500_000.0,
            color: Color::srgb(0.8, 0.8, 1.0),
            ..default()
        },
        transform: Transform::from_xyz(-6.0, 2.0, -4.0),
        ..default()
    });

    let cubie_mesh   = meshes.add(Cuboid::new(CUBIE_SIZE, CUBIE_SIZE, CUBIE_SIZE));
    let sticker_mesh = meshes.add(Rectangle::new(STICKER_SIZE, STICKER_SIZE));
    let black_mat    = materials.add(StandardMaterial {
        base_color: Color::srgb(0.04, 0.04, 0.04),
        perceptual_roughness: 0.9,
        ..default()
    });

    let face_defs = [
        ( 1_i32, 0_usize, Vec3::new(0.0,  FACE_OFFSET, 0.0), Quat::from_rotation_x(-FRAC_PI_2)),
        (-1_i32, 1_usize, Vec3::new(0.0, -FACE_OFFSET, 0.0), Quat::from_rotation_x( FRAC_PI_2)),
        ( 1_i32, 2_usize, Vec3::new( FACE_OFFSET, 0.0, 0.0), Quat::from_rotation_y( FRAC_PI_2)),
        (-1_i32, 3_usize, Vec3::new(-FACE_OFFSET, 0.0, 0.0), Quat::from_rotation_y(-FRAC_PI_2)),
        ( 1_i32, 4_usize, Vec3::new(0.0, 0.0,  FACE_OFFSET), Quat::IDENTITY),
        (-1_i32, 5_usize, Vec3::new(0.0, 0.0, -FACE_OFFSET), Quat::from_rotation_y(PI)),
    ];

    for x in -1_i32..=1 {
        for y in -1_i32..=1 {
            for z in -1_i32..=1 {
                if x == 0 && y == 0 && z == 0 { continue; }

                let cubie_id = commands.spawn((
                    PbrBundle {
                        mesh: cubie_mesh.clone(),
                        material: black_mat.clone(),
                        transform: Transform::from_xyz(x as f32, y as f32, z as f32),
                        ..default()
                    },
                    GridPos { x, y, z },
                )).id();

                let axes = [y, y, x, x, z, z];
                for (i, &(sign, face_idx, offset, rotation)) in face_defs.iter().enumerate() {
                    if axes[i] != sign { continue; }
                    let sticker_mat = materials.add(StandardMaterial {
                        base_color: face_color(face_idx),
                        perceptual_roughness: 0.4,
                        double_sided: true,
                        ..default()
                    });
                    commands.spawn(PbrBundle {
                        mesh: sticker_mesh.clone(),
                        material: sticker_mat,
                        transform: Transform { translation: offset, rotation, ..default() },
                        ..default()
                    }).set_parent(cubie_id);
                }
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn all_basic_moves() -> Vec<CubeMove> {
        vec![
            CubeMove::r(), CubeMove::ri(), CubeMove::l(), CubeMove::li(),
            CubeMove::u(), CubeMove::ui(), CubeMove::d(), CubeMove::di(),
            CubeMove::f(), CubeMove::fi(), CubeMove::b(), CubeMove::bi(),
            CubeMove::m(), CubeMove::mi(), CubeMove::e(), CubeMove::ei(),
            CubeMove::s(), CubeMove::si(),
        ]
    }

    #[test]
    fn move_codec_roundtrip() {
        let mut moves = all_basic_moves();
        // si mutari duble (±180°), cum produce drag-ul manual
        moves.push(CubeMove { rotation_axis: Vec3::X, layer_axis: 0, layer_value: 1, angle: PI });
        moves.push(CubeMove { rotation_axis: Vec3::Y, layer_axis: 1, layer_value: 0, angle: -PI });

        let decoded = decode_moves(&encode_moves(&moves)).expect("decode failed");
        assert_eq!(decoded.len(), moves.len());
        for (a, b) in moves.iter().zip(&decoded) {
            assert_eq!(a.layer_axis, b.layer_axis);
            assert_eq!(a.layer_value, b.layer_value);
            assert!((a.angle - b.angle).abs() < 1e-6);
            assert_eq!(a.rotation_axis, b.rotation_axis);
        }
    }

    #[test]
    fn decode_rejects_garbage() {
        assert!(decode_moves("12").is_none(), "lungime gresita");
        assert!(decode_moves("912").is_none(), "axa invalida");
        assert!(decode_moves("092").is_none(), "strat invalid");
        assert!(decode_moves("012").is_none(), "zero sferturi de tura");
        assert!(decode_moves("abc").is_none(), "non-cifre");
        assert!(decode_moves("").map(|v| v.is_empty()).unwrap_or(false), "sirul gol e istoric gol");
    }

    #[test]
    fn four_quarter_turns_are_identity() {
        for mv in all_basic_moves() {
            let mut pos = (1, 1, 1);
            for _ in 0..4 {
                pos = rotate_grid_pos(pos.0, pos.1, pos.2, mv.rotation_axis, mv.angle);
            }
            assert_eq!(pos, (1, 1, 1), "mutarea {mv:?} nu are ordin 4");
        }
    }

    #[test]
    fn move_then_inverse_is_identity() {
        for mv in all_basic_moves() {
            let inv = mv.inverse();
            for start in [(1, 1, 1), (1, 0, -1), (0, 1, -1), (-1, -1, -1)] {
                let p = rotate_grid_pos(start.0, start.1, start.2, mv.rotation_axis, mv.angle);
                let back = rotate_grid_pos(p.0, p.1, p.2, inv.rotation_axis, inv.angle);
                assert_eq!(back, start, "inversul mutarii {mv:?} nu anuleaza");
            }
        }
    }

    #[test]
    fn scramble_never_repeats_layer() {
        for seed in [1_u64, 42, 0xDEAD, 987654321] {
            let s = generate_scramble(seed, 20);
            assert_eq!(s.len(), 20);
            for w in s.windows(2) {
                assert!(
                    (w[0].layer_axis, w[0].layer_value) != (w[1].layer_axis, w[1].layer_value),
                    "strat repetat consecutiv la seed {seed}"
                );
            }
        }
    }

    #[test]
    fn fmt_time_formats() {
        assert_eq!(fmt_time(5.0), "5.00s");
        assert_eq!(fmt_time(12.345), "12.35s");
        assert_eq!(fmt_time(65.43), "1:05.4");
        assert_eq!(fmt_time(3600.0 / 60.0), "1:00.0");
    }
}
