use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use std::collections::VecDeque;
use std::f32::consts::{FRAC_PI_2, PI};

const CUBIE_SIZE: f32 = 0.92;
const STICKER_SIZE: f32 = 0.78;
const FACE_OFFSET: f32 = CUBIE_SIZE / 2.0 + 0.005;
const ROTATION_DURATION: f32 = 0.18;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Rubik's Cube".into(),
                canvas: Some("#bevy".to_owned()),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: false,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin)
        .insert_resource(ClearColor(Color::srgb(0.1, 0.1, 0.18)))
        .insert_resource(OrbitCamera {
            rotation: Quat::from_euler(EulerRot::YXZ, 0.5, 0.35, 0.0),
            radius: 10.0,
            dragging: false,
        })
        .insert_resource(MoveQueue::default())
        .insert_resource(MoveHistory::default())
        .insert_resource(ActiveRotation::default())
        .add_systems(Startup, setup)
        .add_systems(Update, (
            camera_orbit,
            camera_zoom,
            keyboard_input,
            process_rotation,
            egui_ui,
        ))
        .run();
}

// ── Camera ────────────────────────────────────────────────────────────────────

#[derive(Resource)]
struct OrbitCamera {
    rotation: Quat,
    radius: f32,
    dragging: bool,
}

fn camera_orbit(
    mut state: ResMut<OrbitCamera>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut motion: EventReader<MouseMotion>,
    mut query: Query<&mut Transform, With<Camera3d>>,
    mut egui_ctx: EguiContexts,
) {
    // Nu roti camera daca mouse-ul e pe panoul egui
    if egui_ctx.ctx_mut().is_using_pointer() || egui_ctx.ctx_mut().wants_pointer_input() {
        state.dragging = false;
        motion.clear();
        return;
    }

    if mouse_button.just_pressed(MouseButton::Left) {
        state.dragging = true;
    }
    if mouse_button.just_released(MouseButton::Left) {
        state.dragging = false;
    }

    if state.dragging {
        for ev in motion.read() {
            let sensitivity = 0.006;
            let up = state.rotation * Vec3::Y;
            let yaw_sign = if up.y >= 0.0 { -1.0 } else { 1.0 };
            let yaw = Quat::from_rotation_y(yaw_sign * ev.delta.x * sensitivity);
            state.rotation = yaw * state.rotation;
            let right = state.rotation * Vec3::X;
            let pitch = Quat::from_axis_angle(right, ev.delta.y * sensitivity);
            state.rotation = pitch * state.rotation;
            state.rotation = state.rotation.normalize();
        }
    } else {
        motion.clear();
    }

    if let Ok(mut transform) = query.get_single_mut() {
        let position = state.rotation * Vec3::new(0.0, 0.0, state.radius);
        transform.translation = position;
        transform.look_at(Vec3::ZERO, state.rotation * Vec3::Y);
    }
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

#[derive(Resource, Default)]
pub struct MoveQueue(pub VecDeque<CubeMove>);

#[derive(Resource, Default)]
pub struct MoveHistory(pub Vec<CubeMove>);

pub struct RotationState {
    pub cube_move: CubeMove,
    pub elapsed: f32,
    pub duration: f32,
    pub entities: Vec<Entity>,
    pub initial_transforms: Vec<Transform>,
}

#[derive(Resource, Default)]
pub struct ActiveRotation(pub Option<RotationState>);

// ── Keyboard input ────────────────────────────────────────────────────────────

fn keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut queue: ResMut<MoveQueue>,
    mut history: ResMut<MoveHistory>,
    mut egui_ctx: EguiContexts,
) {
    // Nu captura taste daca egui are focus
    if egui_ctx.ctx_mut().wants_keyboard_input() { return; }

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

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
            queue.0.push_back(mv);
            history.0.push(mv);
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
    mut cubie_query: Query<(Entity, &mut GridPos, &mut Transform)>,
    time: Res<Time>,
) {
    let mut finished = false;

    if let Some(state) = active.0.as_mut() {
        state.elapsed += time.delta_seconds();
        let t = (state.elapsed / state.duration).min(1.0);
        let q = Quat::from_axis_angle(state.cube_move.rotation_axis,
                                      state.cube_move.angle * smoothstep(t));
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
    } else if active.0.is_none() {
        if let Some(mv) = move_queue.0.pop_front() {
            let mut entities = Vec::new();
            let mut initial_transforms = Vec::new();
            for (entity, gp, tf) in cubie_query.iter() {
                let layer_val = match mv.layer_axis { 0 => gp.x, 1 => gp.y, _ => gp.z };
                if layer_val == mv.layer_value {
                    entities.push(entity);
                    initial_transforms.push(*tf);
                }
            }
            active.0 = Some(RotationState {
                cube_move: mv,
                elapsed: 0.0,
                duration: ROTATION_DURATION,
                entities,
                initial_transforms,
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

fn egui_ui(
    mut contexts: EguiContexts,
    mut history: ResMut<MoveHistory>,
    mut queue: ResMut<MoveQueue>,
    time: Res<Time>,
) {
    let ctx = contexts.ctx_mut();

    // 1. Hint taste — sus-stanga
    egui::Area::new("hints".into())
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(12.0, 12.0))
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(
                    "Fete: R L U D F B  |  Felii: M E S  |  Shift = prime\nDrag = rotire vedere  |  Scroll = zoom"
                )
                .size(12.0)
                .color(egui::Color32::from_rgba_unmultiplied(220, 220, 220, 120))
            );
        });

    // 2. Panou butoane — ancorat fix sub textul de mai sus
    egui::Area::new("controls_area".into())
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(12.0, 60.0)) // 60px mai jos fata de colt
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Buton Scramble
                if ui.button(egui::RichText::new("🎲 SCRAMBLE").size(16.0).color(egui::Color32::from_rgb(180, 180, 255))).clicked() {
                    queue.0.clear();
                    history.0.clear();
                    let seed = (time.elapsed_seconds() * 100_000.0) as u64;
                    let mut rng = Rng::new(seed);
                    let moves = [
                        CubeMove::r(), CubeMove::ri(), CubeMove::l(), CubeMove::li(),
                        CubeMove::u(), CubeMove::ui(), CubeMove::d(), CubeMove::di(),
                        CubeMove::f(), CubeMove::fi(), CubeMove::b(), CubeMove::bi(),
                    ];
                    for _ in 0..20 {
                        let mv = moves[rng.range(12)];
                        queue.0.push_back(mv);
                        history.0.push(mv);
                    }
                }

                ui.add_space(10.0);

                // Buton Solve
                if ui.button(egui::RichText::new("✔ SOLVE").size(16.0).color(egui::Color32::from_rgb(150, 255, 180))).clicked() && !history.0.is_empty() {
                    queue.0.clear();
                    let solution: Vec<CubeMove> = history.0.drain(..).rev()
                        .map(|m| m.inverse())
                        .collect();
                    queue.0.extend(solution);
                }
            });
        });
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
