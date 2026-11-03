//! Rubik's Cube — Bevy scene compiled to WebAssembly.
//!
//! The engine owns the 3D cube, pointer interaction, the move engine,
//! solver and trainer. The page (index.html + ui/) owns every control, the
//! timer display, stats and settings, and talks to the engine through
//! `bridge`.

mod bridge;
mod cube;
mod game;
mod input;
mod notation;
mod persist;
mod scramble;
mod solver;
mod trainer;

use bevy::prelude::*;
use bevy::winit::{UpdateMode, WinitSettings};
use std::time::Duration;

fn main() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    let size = persist::load_size();
    let settings = game::Settings::from_storage();

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
        // Render on demand: the engine sleeps while nothing moves (battery on
        // phones) and is woken by input, animation, or a page command.
        .insert_resource(WinitSettings {
            focused_mode: UpdateMode::reactive(Duration::from_secs(2)),
            unfocused_mode: UpdateMode::reactive_low_power(Duration::from_secs(10)),
        })
        .insert_resource(ClearColor(settings.clear_color()))
        .insert_resource(Msaa::Sample4)
        .insert_resource(settings)
        .insert_resource(game::CubeSize(size))
        .init_resource::<input::OrbitCamera>()
        .init_resource::<input::PointerState>()
        .init_resource::<game::MoveQueue>()
        .init_resource::<game::MoveHistory>()
        .init_resource::<game::RedoStack>()
        .init_resource::<game::Game>()
        .init_resource::<game::Player>()
        .init_resource::<game::Pending>()
        .init_resource::<game::ActiveRotation>()
        .init_resource::<solver::SolverContext>()
        .init_resource::<trainer::Trainer>()
        .add_systems(Startup, (game::setup, game::restore_state, game::install_waker).chain())
        .add_systems(
            Update,
            (
                game::handle_commands,
                input::sync_canvas_resolution,
                input::pointer_input,
                input::camera_zoom,
                input::update_camera_transform,
                game::rebuild_cube,
                game::apply_pending,
                game::run_solver,
                game::advance_player,
                game::process_rotation,
                game::update_phase,
                game::apply_settings,
                game::persist_state,
                game::emit_state,
                game::keep_awake,
            )
                .chain(),
        )
        .run();
}
