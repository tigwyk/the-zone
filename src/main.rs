//! The Zone — M2 (character, inventory & trade).

mod area;
mod render;
mod run;
mod screens;

use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};

use area::{build_area_grid, Action, VendorStock, ZoneData};
use render::{render_grid, TileGrid};
use run::{RunState, Rng, BACKGROUNDS, REST_COST, REST_MINUTES, REST_RADS, SKILL_NAMES, TAG_COUNT};
use screens::{
    build_creation_grid, build_inventory_grid, build_trade_grid, trade_list, trade_one, use_item,
};

#[derive(Resource, Default)]
struct MenuSelection(usize);

/// Cursor for whichever modal list is open (inventory or trade).
#[derive(Resource, Default)]
struct Cursor(usize);

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

/// Which vendor the Trade state is showing, and which side of the counter.
#[derive(Resource, Default)]
struct TradeUi {
    vendor: String,
    buying: bool,
}

#[derive(Resource, Default)]
struct Creation {
    phase: usize,
    sel: usize,
    background: usize,
    tags: Vec<usize>,
}

#[derive(States, Default, Debug, Hash, PartialEq, Eq, Clone, Copy)]
enum GameState {
    #[default]
    CharacterCreation,
    Area,
    Inventory,
    Trade,
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
                title: "The Zone — M2".into(),
                resolution: WindowResolution::new(1280, 720).with_scale_factor_override(1.0),
                present_mode: PresentMode::AutoVsync,
                ..default()
            }),
            ..default()
        }))
        .init_resource::<MenuSelection>()
        .init_resource::<Cursor>()
        .init_resource::<MessageLine>()
        .init_resource::<ZoneData>()
        .init_resource::<VendorStock>()
        .init_resource::<CurrentArea>()
        .init_resource::<RunState>()
        .init_resource::<Rng>()
        .init_resource::<TradeUi>()
        .init_resource::<Creation>()
        .init_resource::<TileGrid>()
        .init_state::<GameState>()
        .add_systems(Startup, (setup, enter_creation))
        .add_systems(OnEnter(GameState::CharacterCreation), enter_creation)
        .add_systems(OnEnter(GameState::Area), redraw_area)
        .add_systems(OnEnter(GameState::Inventory), enter_inventory)
        .add_systems(OnEnter(GameState::Trade), enter_trade)
        .add_systems(
            Update,
            (creation_input, render_grid)
                .chain()
                .run_if(in_state(GameState::CharacterCreation)),
        )
        .add_systems(
            Update,
            (menu_input, render_grid)
                .chain()
                .run_if(in_state(GameState::Area)),
        )
        .add_systems(
            Update,
            (inventory_input, render_grid)
                .chain()
                .run_if(in_state(GameState::Inventory)),
        )
        .add_systems(
            Update,
            (trade_input, render_grid)
                .chain()
                .run_if(in_state(GameState::Trade)),
        )
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}

// ---- character creation ----

fn enter_creation(mut grid: ResMut<TileGrid>, c: Res<Creation>) {
    build_creation_grid(&mut grid, c.phase, c.sel, c.background, &c.tags);
}

fn creation_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut c: ResMut<Creation>,
    mut run: ResMut<RunState>,
    mut grid: ResMut<TileGrid>,
    mut message: ResMut<MessageLine>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    let n = if c.phase == 0 { BACKGROUNDS.len() } else { SKILL_NAMES.len() };
    let mut changed = false;

    if keys.just_pressed(KeyCode::ArrowUp) {
        c.sel = (c.sel + n - 1) % n;
        changed = true;
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        c.sel = (c.sel + 1) % n;
        changed = true;
    }
    if keys.just_pressed(KeyCode::Enter) {
        if c.phase == 0 {
            c.background = c.sel;
            c.phase = 1;
            c.sel = 0;
        } else if let Some(i) = c.tags.iter().position(|&t| t == c.sel) {
            c.tags.remove(i);
        } else if c.tags.len() < TAG_COUNT {
            let sel = c.sel;
            c.tags.push(sel);
            if c.tags.len() == TAG_COUNT {
                *run = RunState::roll(c.background, &c.tags);
                message.0 = "You sign the ledger and walk in.".into();
                next_state.set(GameState::Area);
                return;
            }
        }
        changed = true;
    }

    if changed {
        build_creation_grid(&mut grid, c.phase, c.sel, c.background, &c.tags);
    }
}

// ---- area ----

fn redraw_area(
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    run: Res<RunState>,
    area: Res<CurrentArea>,
    sel: Res<MenuSelection>,
    message: Res<MessageLine>,
) {
    build_area_grid(&mut grid, &zone, &run, &area.0, sel.0, &message.0);
}

fn enter_inventory(
    mut cursor: ResMut<Cursor>,
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    run: Res<RunState>,
    message: Res<MessageLine>,
) {
    cursor.0 = 0;
    build_inventory_grid(&mut grid, &zone, &run, 0, &message.0);
}

fn enter_trade(
    mut cursor: ResMut<Cursor>,
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    stock: Res<VendorStock>,
    run: Res<RunState>,
    trade_ui: Res<TradeUi>,
    message: Res<MessageLine>,
) {
    cursor.0 = 0;
    build_trade_grid(
        &mut grid,
        &zone,
        &stock,
        &run,
        &trade_ui.vendor,
        trade_ui.buying,
        0,
        &message.0,
    );
}

fn menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut sel: ResMut<MenuSelection>,
    mut area: ResMut<CurrentArea>,
    mut message: ResMut<MessageLine>,
    mut grid: ResMut<TileGrid>,
    mut run: ResMut<RunState>,
    mut trade_ui: ResMut<TradeUi>,
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
        if run_action(&action, &mut area, &mut message, &mut run, &mut trade_ui, &mut next_state) {
            sel.0 = 0;
        }
        changed = true;
    }
    if let Some(letter) = pressed_letter(&keys) {
        if let Some(action) = data.secrets.get(&letter).cloned() {
            if run_action(&action, &mut area, &mut message, &mut run, &mut trade_ui, &mut next_state) {
                sel.0 = 0;
            }
            changed = true;
        }
    }

    if changed {
        build_area_grid(&mut grid, &zone, &run, &area.0, sel.0, &message.0);
    }
}

/// Runs one menu or secret action. Returns true if the area changed.
fn run_action(
    action: &Action,
    area: &mut CurrentArea,
    message: &mut MessageLine,
    run: &mut RunState,
    trade_ui: &mut TradeUi,
    next_state: &mut NextState<GameState>,
) -> bool {
    match action {
        Action::Travel(dest) => {
            area.0 = dest.clone();
            run.minutes += 60; // GDD §5: travel costs 1 h
            message.0.clear();
            true
        }
        Action::Say(s) => {
            message.0 = s.clone();
            false
        }
        Action::Rest => {
            if run.rubles < REST_COST {
                message.0 = format!("A bunk costs {REST_COST} RU. You are short.");
            } else {
                run.rubles -= REST_COST;
                run.minutes += REST_MINUTES;
                run.hp = run.max_hp;
                run.rads = (run.rads - REST_RADS).max(0);
                message.0 = "You sleep eight hours. The Zone waits.".into();
            }
            false
        }
        Action::Trade(vendor) => {
            trade_ui.vendor = vendor.clone();
            trade_ui.buying = true;
            message.0.clear();
            next_state.set(GameState::Trade);
            false
        }
    }
}

// ---- inventory ----

fn inventory_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: ResMut<Cursor>,
    mut run: ResMut<RunState>,
    mut message: ResMut<MessageLine>,
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::Tab) {
        message.0.clear();
        next_state.set(GameState::Area);
        return;
    }

    let n = run.items.len();
    let mut changed = false;
    if n > 0 {
        if keys.just_pressed(KeyCode::ArrowUp) {
            cursor.0 = (cursor.0 + n - 1) % n;
            changed = true;
        }
        if keys.just_pressed(KeyCode::ArrowDown) {
            cursor.0 = (cursor.0 + 1) % n;
            changed = true;
        }
        if keys.just_pressed(KeyCode::Enter) {
            message.0 = use_item(&zone, &mut run, cursor.0);
            cursor.0 = cursor.0.min(run.items.len().saturating_sub(1));
            changed = true;
        }
    }

    if changed {
        build_inventory_grid(&mut grid, &zone, &run, cursor.0, &message.0);
    }
}

// ---- trade ----

fn trade_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: ResMut<Cursor>,
    mut run: ResMut<RunState>,
    mut stock: ResMut<VendorStock>,
    mut message: ResMut<MessageLine>,
    mut grid: ResMut<TileGrid>,
    mut trade_ui: ResMut<TradeUi>,
    zone: Res<ZoneData>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        message.0.clear();
        next_state.set(GameState::Area);
        return;
    }

    let mut changed = false;
    if keys.just_pressed(KeyCode::ArrowLeft) && !trade_ui.buying {
        trade_ui.buying = true;
        cursor.0 = 0;
        changed = true;
    }
    if keys.just_pressed(KeyCode::ArrowRight) && trade_ui.buying {
        trade_ui.buying = false;
        cursor.0 = 0;
        changed = true;
    }

    let n = trade_list(&stock, &run, &trade_ui.vendor, trade_ui.buying).len();
    if n > 0 {
        if keys.just_pressed(KeyCode::ArrowUp) {
            cursor.0 = (cursor.0 + n - 1) % n;
            changed = true;
        }
        if keys.just_pressed(KeyCode::ArrowDown) {
            cursor.0 = (cursor.0 + 1) % n;
            changed = true;
        }
        if keys.just_pressed(KeyCode::Enter) {
            message.0 = trade_one(
                &zone,
                &mut stock,
                &mut run,
                &trade_ui.vendor,
                trade_ui.buying,
                cursor.0,
            );
            let n = trade_list(&stock, &run, &trade_ui.vendor, trade_ui.buying).len();
            cursor.0 = cursor.0.min(n.saturating_sub(1));
            changed = true;
        }
    }

    if changed {
        build_trade_grid(
            &mut grid,
            &zone,
            &stock,
            &run,
            &trade_ui.vendor,
            trade_ui.buying,
            cursor.0,
            &message.0,
        );
    }
}

fn pressed_letter(keys: &ButtonInput<KeyCode>) -> Option<char> {
    LETTER_KEYS
        .iter()
        .find(|(k, _)| keys.just_pressed(*k))
        .map(|(_, c)| *c)
}
