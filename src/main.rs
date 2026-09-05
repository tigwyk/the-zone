//! The Zone — M1 (data-driven skeleton).

mod area;
mod render;

use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};

use area::{build_area_grid, build_inventory_grid, Action, ZoneData};
use render::{render_grid, TileGrid};

#[derive(Resource, Default)]
struct MenuSelection(usize);

#[derive(Resource, Default)]
struct MessageLine(String);

#[derive(Resource)]
struct CurrentArea(String);

impl FromWorld for CurrentArea {
    fn from_world(world: &mut World) -> Self {
        let start = world.resource::<ZoneData>().start.clone();
        CurrentArea(start)
    }
}

#[derive(States, Default, Debug, Hash, PartialEq, Eq, Clone, Copy)]
enum GameState {
    #[default]
    Area,
    Inventory,
}

const LETTER_KEYS: [(KeyCode, char); 26] = [
    (KeyCode::KeyA, 'A'),
    (KeyCode::KeyB, 'B'),
    (KeyCode::KeyC, 'C'),
    (KeyCode::KeyD, 'D'),
    (KeyCode::KeyE, 'E'),
    (KeyCode::KeyF, 'F'),
    (KeyCode::KeyG, 'G'),
    (KeyCode::KeyH, 'H'),
    (KeyCode::KeyI, 'I'),
    (KeyCode::KeyJ, 'J'),
    (KeyCode::KeyK, 'K'),
    (KeyCode::KeyL, 'L'),
    (KeyCode::KeyM, 'M'),
    (KeyCode::KeyN, 'N'),
    (KeyCode::KeyO, 'O'),
    (KeyCode::KeyP, 'P'),
    (KeyCode::KeyQ, 'Q'),
    (KeyCode::KeyR, 'R'),
    (KeyCode::KeyS, 'S'),
    (KeyCode::KeyT, 'T'),
    (KeyCode::KeyU, 'U'),
    (KeyCode::KeyV, 'V'),
    (KeyCode::KeyW, 'W'),
    (KeyCode::KeyX, 'X'),
    (KeyCode::KeyY, 'Y'),
    (KeyCode::KeyZ, 'Z'),
];

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "The Zone — M1".into(),
                resolution: WindowResolution::new(1280, 720).with_scale_factor_override(1.0),
                present_mode: PresentMode::AutoVsync,
                ..default()
            }),
            ..default()
        }))
        .init_resource::<MenuSelection>()
        .init_resource::<MessageLine>()
        .init_resource::<ZoneData>()
        .init_resource::<CurrentArea>()
        .init_resource::<TileGrid>()
        .init_state::<GameState>()
        .add_systems(Startup, setup)
        .add_systems(OnEnter(GameState::Area), enter_area)
        .add_systems(OnEnter(GameState::Inventory), enter_inventory)
        .add_systems(Update, (menu_input, render_grid).chain().run_if(in_state(GameState::Area)))
        .add_systems(Update, (inventory_input, render_grid).chain().run_if(in_state(GameState::Inventory)))
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn enter_area(
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    area: Res<CurrentArea>,
    sel: Res<MenuSelection>,
    message: Res<MessageLine>,
) {
    build_area_grid(&mut grid, &zone, &area.0, sel.0, &message.0);
}

fn enter_inventory(mut grid: ResMut<TileGrid>) {
    build_inventory_grid(&mut grid);
}

fn menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut sel: ResMut<MenuSelection>,
    mut area: ResMut<CurrentArea>,
    mut message: ResMut<MessageLine>,
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Tab) {
        next_state.set(GameState::Inventory);
        return;
    }

    let data = &zone.areas[area.0.as_str()];
    let n = data.menu.len();
    let mut changed = false;

    if keys.just_pressed(KeyCode::ArrowUp) {
        sel.0 = (sel.0 + n - 1) % n;
        changed = true;
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        sel.0 = (sel.0 + 1) % n;
        changed = true;
    }
    if keys.just_pressed(KeyCode::Enter) {
        let action = data.menu[sel.0].1.clone();
        if run_action(&action, &mut area, &mut message) {
            sel.0 = 0;
        }
        changed = true;
    }
    if let Some(letter) = pressed_letter(&keys) {
        if let Some(action) = data.secrets.get(&letter).cloned() {
            if run_action(&action, &mut area, &mut message) {
                sel.0 = 0;
            }
            changed = true;
        }
    }

    if changed {
        build_area_grid(&mut grid, &zone, &area.0, sel.0, &message.0);
    }
}

fn run_action(action: &Action, area: &mut CurrentArea, message: &mut MessageLine) -> bool {
    match action {
        Action::Travel(dest) => {
            area.0 = dest.clone();
            message.0.clear();
            true
        }
        Action::Say(s) => {
            message.0 = s.clone();
            false
        }
    }
}

fn inventory_input(keys: Res<ButtonInput<KeyCode>>, mut next_state: ResMut<NextState<GameState>>) {
    if keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::Tab) {
        next_state.set(GameState::Area);
    }
}

fn pressed_letter(keys: &ButtonInput<KeyCode>) -> Option<char> {
    LETTER_KEYS
        .iter()
        .find(|(k, _)| keys.just_pressed(*k))
        .map(|(_, c)| *c)
}
