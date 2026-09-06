//! The Zone — M6 (the end).

mod area;
mod combat;
mod dialogue;
mod meta;
mod quest;
mod render;
mod run;
mod screens;
mod sim;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};

use area::{build_area_grid, Action, VendorStock, ZoneData};
use combat::{build_combat_grid, Combat};
use dialogue::{build_dialogue_grid, Dialogue};
use meta::{build_memorial_grid, MetaProgress, SaveDir};
use quest::{build_board_grid, build_journal_grid, Board};
use render::{render_grid, TileGrid};
use run::{Rng, RunState, BACKGROUNDS, REST_COST, REST_RADS, SKILL_NAMES, TAG_COUNT};
use screens::{
    build_creation_grid, build_gameover_grid, build_inventory_grid, build_map_grid,
    build_trade_grid, known_areas, trade_list, trade_one, use_item,
};
use sim::{
    action_minutes, advance, check_death, push_through, reveal_secrets, scan, take_artifact,
    throw_bolt, visible_menu, Fields, GameClock,
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
    Combat,
    Dialogue,
    Jobs,
    Map,
    Journal,
    Memorial,
    GameOver,
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

/// Everything except the window and the renderer. `main` adds those on top; the
/// headless playthrough tests at the bottom of this file drive exactly this.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
struct GameInput;

fn add_game(app: &mut App) -> &mut App {
    app.init_resource::<MenuSelection>()
        .init_resource::<Cursor>()
        .init_resource::<MessageLine>()
        .init_resource::<ZoneData>()
        .init_resource::<VendorStock>()
        .init_resource::<CurrentArea>()
        .init_resource::<RunState>()
        .init_resource::<Rng>()
        .init_resource::<TradeUi>()
        .init_resource::<GameClock>()
        .init_resource::<Fields>()
        .init_resource::<Combat>()
        .init_resource::<Dialogue>()
        .init_resource::<Board>()
        .init_resource::<SaveDir>()
        .init_resource::<MetaProgress>()
        .init_resource::<Creation>()
        .init_resource::<TileGrid>()
        .init_state::<GameState>()
        .add_systems(Startup, begin)
        .add_systems(OnEnter(GameState::CharacterCreation), enter_creation)
        .add_systems(OnEnter(GameState::Area), redraw_area)
        .add_systems(OnEnter(GameState::Inventory), enter_inventory)
        .add_systems(OnEnter(GameState::Trade), enter_trade)
        .add_systems(OnEnter(GameState::Combat), redraw_combat)
        .add_systems(OnEnter(GameState::Dialogue), redraw_dialogue)
        .add_systems(OnEnter(GameState::Jobs), redraw_board)
        .add_systems(OnEnter(GameState::Map), enter_map)
        .add_systems(OnEnter(GameState::Journal), enter_journal)
        .add_systems(OnEnter(GameState::Memorial), enter_memorial)
        .add_systems(OnEnter(GameState::GameOver), enter_gameover)
        .add_systems(
            Update,
            (
                creation_input.run_if(in_state(GameState::CharacterCreation)),
                menu_input.run_if(in_state(GameState::Area)),
                inventory_input.run_if(in_state(GameState::Inventory)),
                trade_input.run_if(in_state(GameState::Trade)),
                combat_input.run_if(in_state(GameState::Combat)),
                dialogue_input.run_if(in_state(GameState::Dialogue)),
                board_input.run_if(in_state(GameState::Jobs)),
                map_input.run_if(in_state(GameState::Map)),
                journal_input.run_if(in_state(GameState::Journal)),
                memorial_input.run_if(in_state(GameState::Memorial)),
                gameover_input.run_if(in_state(GameState::GameOver)),
            )
                .in_set(GameInput),
        )
}

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "The Zone — M6".into(),
            resolution: WindowResolution::new(1280, 720).with_scale_factor_override(1.0),
            present_mode: PresentMode::AutoVsync,
            ..default()
        }),
        ..default()
    }));
    add_game(&mut app)
        .add_systems(Startup, setup)
        .add_systems(Update, render_grid.after(GameInput))
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}

// ---- character creation ----

fn enter_creation(
    mut grid: ResMut<TileGrid>,
    c: Res<Creation>,
    meta: Res<MetaProgress>,
    message: Res<MessageLine>,
) {
    build_creation_grid(&mut grid, &meta, c.phase, c.sel, c.background, &c.tags, &message.0);
}

/// First frame: read the memorial, and pick up a suspended run if there is one.
/// Reading the suspend file spends it, so there is exactly one way back in (GDD §10).
#[allow(clippy::too_many_arguments)]
fn begin(
    dir: Res<SaveDir>,
    mut meta: ResMut<MetaProgress>,
    mut run: ResMut<RunState>,
    mut area: ResMut<CurrentArea>,
    mut clock: ResMut<GameClock>,
    mut fields: ResMut<Fields>,
    mut stock: ResMut<VendorStock>,
    mut rng: ResMut<Rng>,
    mut message: ResMut<MessageLine>,
    mut next_state: ResMut<NextState<GameState>>,
    grid: ResMut<TileGrid>,
    c: Res<Creation>,
) {
    *meta = MetaProgress::load(&dir);
    if let Some(save) = meta::resume(&dir) {
        *run = save.run;
        area.0 = save.area;
        *clock = save.clock;
        fields.0 = save.fields;
        stock.0 = save.stock;
        *rng = save.rng;
        message.0 = "You pick up where you put it down.".into();
        next_state.set(GameState::Area);
        return;
    }
    enter_creation(grid, c, meta.into(), message.into());
}

fn creation_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut c: ResMut<Creation>,
    mut run: ResMut<RunState>,
    mut grid: ResMut<TileGrid>,
    mut message: ResMut<MessageLine>,
    mut clock: ResMut<GameClock>,
    mut rng: ResMut<Rng>,
    zone: Res<ZoneData>,
    area: Res<CurrentArea>,
    meta: Res<MetaProgress>,
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
            if !meta.unlocked(c.sel) {
                message.0 = "Nobody with that history has come back yet.".into();
                build_creation_grid(&mut grid, &meta, c.phase, c.sel, c.background, &c.tags, &message.0);
                return;
            }
            c.background = c.sel;
            c.phase = 1;
            c.sel = 0;
        } else if let Some(i) = c.tags.iter().position(|&t| t == c.sel) {
            c.tags.remove(i);
        } else if c.tags.len() < TAG_COUNT {
            let sel = c.sel;
            c.tags.push(sel);
            if c.tags.len() == TAG_COUNT {
                *run = RunState::roll(c.background, &c.tags, &mut rng);
                // A quarter of the last stalker's standing came with you (GDD §10).
                for (faction, carried) in &meta.rep {
                    let now = run.rep_of(faction);
                    run.rep.insert(faction.clone(), (now + carried).clamp(-100, 100));
                }
                clock.schedule(&run, &mut rng);
                reveal_secrets(&area.0, &mut run, &zone, &mut rng);
                run.discovered.insert(area.0.clone());
                message.0 = "You sign the ledger and walk in.".into();
                next_state.set(GameState::Area);
                return;
            }
        }
        changed = true;
    }

    if changed {
        build_creation_grid(&mut grid, &meta, c.phase, c.sel, c.background, &c.tags, &message.0);
    }
}

// ---- area ----

fn redraw_area(
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    run: Res<RunState>,
    clock: Res<GameClock>,
    fields: Res<Fields>,
    area: Res<CurrentArea>,
    sel: Res<MenuSelection>,
    message: Res<MessageLine>,
) {
    build_area_grid(&mut grid, &zone, &run, &clock, &fields, &area.0, sel.0, &message.0);
}

fn enter_inventory(
    mut cursor: ResMut<Cursor>,
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    run: Res<RunState>,
    clock: Res<GameClock>,
    message: Res<MessageLine>,
) {
    cursor.0 = 0;
    build_inventory_grid(&mut grid, &zone, &run, &clock, 0, &message.0);
}

fn enter_trade(
    mut cursor: ResMut<Cursor>,
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    stock: Res<VendorStock>,
    run: Res<RunState>,
    clock: Res<GameClock>,
    trade_ui: Res<TradeUi>,
    message: Res<MessageLine>,
) {
    cursor.0 = 0;
    build_trade_grid(
        &mut grid,
        &zone,
        &stock,
        &run,
        &clock,
        &trade_ui.vendor,
        trade_ui.buying,
        0,
        &message.0,
    );
}

/// Everything an area action can touch. Bundled because `perform` needs all of it.
#[derive(SystemParam)]
struct Act<'w> {
    area: ResMut<'w, CurrentArea>,
    message: ResMut<'w, MessageLine>,
    run: ResMut<'w, RunState>,
    clock: ResMut<'w, GameClock>,
    fields: ResMut<'w, Fields>,
    stock: ResMut<'w, VendorStock>,
    rng: ResMut<'w, Rng>,
    trade_ui: ResMut<'w, TradeUi>,
    combat: ResMut<'w, Combat>,
    dialogue: ResMut<'w, Dialogue>,
    board: ResMut<'w, Board>,
    meta: ResMut<'w, MetaProgress>,
    zone: Res<'w, ZoneData>,
    dir: Res<'w, SaveDir>,
}

fn menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut sel: ResMut<MenuSelection>,
    mut grid: ResMut<TileGrid>,
    mut act: Act,
    mut exit: MessageWriter<AppExit>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Tab) {
        next_state.set(GameState::Inventory);
        return;
    }
    if keys.just_pressed(KeyCode::F2) {
        next_state.set(GameState::Map);
        return;
    }
    if keys.just_pressed(KeyCode::F3) {
        next_state.set(GameState::Journal);
        return;
    }
    if keys.just_pressed(KeyCode::F5) {
        // GDD §10: save and quit. Not reachable from a fight, which has no way out.
        let saved = meta::suspend(
            &act.dir,
            &act.run,
            &act.area.0,
            &act.clock,
            &act.fields,
            &act.stock,
            &act.rng,
        );
        if saved {
            exit.write(AppExit::Success);
        } else {
            act.message.0 = "The Zone will not let you put it down here.".into();
        }
        return;
    }

    let here = act.area.0.clone();
    let menu: Vec<Action> = visible_menu(&act.zone.areas[here.as_str()], &act.fields, &here)
        .iter()
        .map(|(_, a)| a.clone())
        .collect();
    let n = menu.len();
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
        if perform(&menu[sel.0], &mut act, &mut next_state) {
            sel.0 = 0;
        }
        changed = true;
    }
    if let Some(letter) = pressed_letter(&keys) {
        // Only a revealed letter answers to its key (GDD §5).
        let secret = act.zone.areas[here.as_str()]
            .secrets
            .get(&letter)
            .filter(|_| act.run.is_revealed(&here, letter))
            .map(|s| s.action.clone());
        if let Some(action) = secret {
            if perform(&action, &mut act, &mut next_state) {
                sel.0 = 0;
            }
            changed = true;
        }
    }

    if changed {
        // The menu can shrink under the cursor: Take Artifact comes and goes.
        let n = visible_menu(&act.zone.areas[act.area.0.as_str()], &act.fields, &act.area.0).len();
        sel.0 = sel.0.min(n.saturating_sub(1));
        build_area_grid(
            &mut grid,
            &act.zone,
            &act.run,
            &act.clock,
            &act.fields,
            &act.area.0,
            sel.0,
            &act.message.0,
        );
    }
}

/// Runs one menu or secret action, then lets the clock catch up with it.
/// Returns true if the area changed.
fn perform(action: &Action, act: &mut Act, next_state: &mut NextState<GameState>) -> bool {
    let here = act.area.0.clone();
    // The loader guarantees an anomaly verb only ever sits on a field.
    let anomaly = act.zone.areas[here.as_str()].anomaly.clone();
    let mut moved = None;

    match action {
        Action::Travel(dest) => {
            moved = Some(dest.clone());
            act.message.0.clear();
        }
        Action::Say(s) => act.message.0 = s.clone(),
        Action::Rest => {
            if act.run.rubles < REST_COST {
                act.message.0 = format!("A bunk costs {REST_COST} RU. You are short.");
                return false;
            }
            act.run.rubles -= REST_COST;
            act.run.hp = act.run.max_hp;
            act.run.rads = (act.run.rads - REST_RADS).max(0);
            act.message.0 = "You sleep eight hours. The Zone waits.".into();
        }
        Action::Trade(vendor) => {
            act.trade_ui.vendor = vendor.clone();
            act.trade_ui.buying = true;
            act.message.0.clear();
            next_state.set(GameState::Trade);
        }
        Action::SetFlag(flag) => {
            act.run.flags.insert(flag.clone());
            act.message.0 = "You read it twice, and keep it.".into();
        }
        Action::Talk(npc) => {
            dialogue::start(npc, &mut act.dialogue, &act.zone);
            act.message.0.clear();
            next_state.set(GameState::Dialogue);
        }
        Action::Jobs(faction) => {
            act.board.faction = faction.clone();
            act.board.sel = 0;
            act.message.0.clear();
            next_state.set(GameState::Jobs);
        }
        Action::Memorial => {
            act.message.0.clear();
            next_state.set(GameState::Memorial);
        }
        Action::End(id) => {
            // The Room grants it, and that is the end of this stalker (GDD §9).
            let ending = act.zone.endings[id].clone();
            let (name, text) = meta::resolve(&ending, &act.run, &act.zone, &mut act.rng);
            let meta = &mut *act.meta;
            meta::bank(meta, &act.run, &act.dir, &name, &text);
            act.run.death = Some(text);
            act.run.ending = Some(name);
            next_state.set(GameState::GameOver);
            return false;
        }
        Action::Scan => {
            let a = anomaly.as_ref().expect("Scan outside a field");
            act.message.0 = scan(&here, a, &mut act.run, &act.zone, &mut act.fields, &mut act.rng);
        }
        Action::ThrowBolt => {
            let a = anomaly.as_ref().expect("ThrowBolt outside a field");
            act.message.0 = throw_bolt(&here, a, &mut act.run, &mut act.fields, &mut act.rng);
        }
        Action::PushThrough => {
            let a = anomaly.as_ref().expect("PushThrough outside a field");
            let (msg, dest) = push_through(&here, a, &mut act.run, &mut act.fields, &mut act.rng);
            act.message.0 = msg;
            moved = dest;
        }
        Action::TakeArtifact => {
            let a = anomaly.as_ref().expect("TakeArtifact outside a field");
            act.message.0 = take_artifact(&here, a, &mut act.run, &act.zone, &mut act.fields);
        }
    }

    // The clock runs where you were standing, so an emission catches you there.
    let minutes = action_minutes(action);
    if minutes > 0 {
        let msg = advance(
            minutes,
            &here,
            &mut act.run,
            &act.zone,
            &mut act.clock,
            &mut act.fields,
            &mut act.stock,
            &mut act.rng,
        );
        if !msg.is_empty() {
            act.message.0 = msg;
        }
    }

    if let Some(dest) = &moved {
        act.area.0 = dest.clone();
    }
    let now = act.area.0.clone();
    reveal_secrets(&now, &mut act.run, &act.zone, &mut act.rng);
    act.run.discovered.insert(now.clone());
    let settled = quest::settle(&mut act.run, &act.zone);
    if !settled.is_empty() {
        act.message.0 = format!("{} {settled}", act.message.0);
    }

    if check_death(&mut act.run) {
        let cause = act.run.death.clone().unwrap_or_default();
        let meta = &mut *act.meta;
        meta::bank(meta, &act.run, &act.dir, &cause, &act.message.0);
        next_state.set(GameState::GameOver);
        return moved.is_some();
    }
    // Something may live here. It gets one go at you per run.
    if moved.is_some() {
        if let Some(enemy) = act.zone.areas[now.as_str()].encounter.clone() {
            // One resident, one fight per run.
            if act.run.flags.insert(format!("met:{now}")) {
                let combat = &mut *act.combat;
                act.message.0 = combat::start(&enemy, combat, &act.run, &act.zone);
                next_state.set(GameState::Combat);
            }
        }
    }
    moved.is_some()
}

// ---- combat ----

fn redraw_combat(
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    run: Res<RunState>,
    clock: Res<GameClock>,
    fields: Res<Fields>,
    combat: Res<Combat>,
    area: Res<CurrentArea>,
    message: Res<MessageLine>,
) {
    build_combat_grid(
        &mut grid,
        &zone,
        &run,
        &clock,
        &fields,
        &combat,
        &area.0,
        &message.0,
    );
}

fn combat_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut grid: ResMut<TileGrid>,
    mut combat: ResMut<Combat>,
    mut run: ResMut<RunState>,
    mut message: ResMut<MessageLine>,
    mut rng: ResMut<Rng>,
    zone: Res<ZoneData>,
    clock: Res<GameClock>,
    fields: Res<Fields>,
    area: Res<CurrentArea>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    // SPEC §6: no Esc out of a fight. Tab still reaches the pack, at 4 AP an item.
    if keys.just_pressed(KeyCode::Tab) {
        next_state.set(GameState::Inventory);
        return;
    }

    let menu = combat::menu(&combat, &run, &zone);
    let n = menu.len();
    let mut changed = false;

    if keys.just_pressed(KeyCode::ArrowUp) {
        combat.sel = (combat.sel + n - 1) % n;
        changed = true;
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        combat.sel = (combat.sel + 1) % n;
        changed = true;
    }
    if keys.just_pressed(KeyCode::Enter) {
        let verb = menu[combat.sel.min(n - 1)].1;
        message.0 = combat::act(verb, &mut combat, &mut run, &zone, &mut rng);
        changed = true;
    }

    if !changed {
        return;
    }
    if check_death(&mut run) {
        next_state.set(GameState::GameOver);
        return;
    }
    if !combat.active {
        next_state.set(GameState::Area);
        return;
    }
    // The menu shrinks as the bands change; keep the cursor on something real.
    let n = combat::menu(&combat, &run, &zone).len();
    combat.sel = combat.sel.min(n.saturating_sub(1));
    build_combat_grid(
        &mut grid,
        &zone,
        &run,
        &clock,
        &fields,
        &combat,
        &area.0,
        &message.0,
    );
}

// ---- dialogue, boards, the map and the journal ----

fn redraw_dialogue(
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    run: Res<RunState>,
    clock: Res<GameClock>,
    dialogue: Res<Dialogue>,
    message: Res<MessageLine>,
) {
    build_dialogue_grid(&mut grid, &zone, &run, &clock, &dialogue, &message.0);
}

fn dialogue_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut grid: ResMut<TileGrid>,
    mut act: Act,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        act.message.0.clear();
        next_state.set(GameState::Area);
        return;
    }

    let n = dialogue::options(&act.dialogue, &act.zone).len();
    let mut changed = false;
    if keys.just_pressed(KeyCode::ArrowUp) {
        act.dialogue.sel = (act.dialogue.sel + n - 1) % n;
        changed = true;
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        act.dialogue.sel = (act.dialogue.sel + 1) % n;
        changed = true;
    }
    if keys.just_pressed(KeyCode::Enter) {
        let taken = dialogue::take(&mut act.dialogue, &act.run, &act.zone);
        act.message.0 = taken.message;
        // Set the fallback first: an action may well send us somewhere better.
        if taken.close {
            next_state.set(GameState::Area);
        }
        if let Some(action) = taken.action {
            perform(&action, &mut act, &mut next_state);
        }
        changed = true;
    }

    if changed {
        build_dialogue_grid(
            &mut grid,
            &act.zone,
            &act.run,
            &act.clock,
            &act.dialogue,
            &act.message.0,
        );
    }
}

fn redraw_board(
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    run: Res<RunState>,
    clock: Res<GameClock>,
    board: Res<Board>,
    message: Res<MessageLine>,
) {
    build_board_grid(&mut grid, &zone, &run, &clock, &board, &message.0);
}

fn board_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut grid: ResMut<TileGrid>,
    mut board: ResMut<Board>,
    mut run: ResMut<RunState>,
    mut message: ResMut<MessageLine>,
    zone: Res<ZoneData>,
    clock: Res<GameClock>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        message.0.clear();
        next_state.set(GameState::Area);
        return;
    }

    let offered = quest::offered(&zone, &run, &board.faction);
    let n = offered.len();
    let mut changed = false;
    if n > 0 {
        if keys.just_pressed(KeyCode::ArrowUp) {
            board.sel = (board.sel + n - 1) % n;
            changed = true;
        }
        if keys.just_pressed(KeyCode::ArrowDown) {
            board.sel = (board.sel + 1) % n;
            changed = true;
        }
        if keys.just_pressed(KeyCode::Enter) {
            let id = offered[board.sel.min(n - 1)].to_string();
            message.0 = format!("You take the job: {}.", zone.quests[&id].name);
            run.quests_taken.insert(id);
            board.sel = 0;
            changed = true;
        }
    }

    if changed {
        build_board_grid(&mut grid, &zone, &run, &clock, &board, &message.0);
    }
}

fn enter_map(
    mut cursor: ResMut<Cursor>,
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    run: Res<RunState>,
    clock: Res<GameClock>,
    area: Res<CurrentArea>,
    message: Res<MessageLine>,
) {
    // Open the map on where you are standing.
    cursor.0 = known_areas(&zone, &run)
        .iter()
        .position(|id| *id == area.0)
        .unwrap_or(0);
    build_map_grid(&mut grid, &zone, &run, &clock, &area.0, cursor.0, &message.0);
}

fn map_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: ResMut<Cursor>,
    mut grid: ResMut<TileGrid>,
    mut act: Act,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::F2) {
        act.message.0.clear();
        next_state.set(GameState::Area);
        return;
    }

    let known = known_areas(&act.zone, &act.run);
    let n = known.len();
    let mut changed = false;
    if keys.just_pressed(KeyCode::ArrowUp) {
        cursor.0 = (cursor.0 + n - 1) % n;
        changed = true;
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        cursor.0 = (cursor.0 + 1) % n;
        changed = true;
    }
    if keys.just_pressed(KeyCode::Enter) {
        let here = act.area.0.clone();
        let dest = known[cursor.0.min(n - 1)].clone();
        if dest == here {
            act.message.0 = "You are already standing there.".into();
        } else if area::exits(&act.zone.areas[here.as_str()]).contains(&dest) {
            // GDD 5: only somewhere next door can be walked to from the map.
            next_state.set(GameState::Area);
            perform(&Action::Travel(dest), &mut act, &mut next_state);
            return;
        } else {
            act.message.0 = format!(
                "{} is not next to here. You would have to walk it.",
                act.zone.areas[&dest].name
            );
        }
        changed = true;
    }

    if changed {
        build_map_grid(
            &mut grid,
            &act.zone,
            &act.run,
            &act.clock,
            &act.area.0,
            cursor.0,
            &act.message.0,
        );
    }
}

fn enter_journal(
    mut cursor: ResMut<Cursor>,
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    run: Res<RunState>,
    clock: Res<GameClock>,
    message: Res<MessageLine>,
) {
    cursor.0 = 0;
    build_journal_grid(&mut grid, &zone, &run, &clock, 0, &message.0);
}

fn journal_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: ResMut<Cursor>,
    mut grid: ResMut<TileGrid>,
    mut message: ResMut<MessageLine>,
    zone: Res<ZoneData>,
    run: Res<RunState>,
    clock: Res<GameClock>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::F3) {
        message.0.clear();
        next_state.set(GameState::Area);
        return;
    }

    let n = quest::taken(&zone, &run).len();
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
    }
    if changed {
        build_journal_grid(&mut grid, &zone, &run, &clock, cursor.0, &message.0);
    }
}

fn enter_memorial(
    mut cursor: ResMut<Cursor>,
    mut grid: ResMut<TileGrid>,
    meta: Res<MetaProgress>,
    message: Res<MessageLine>,
) {
    cursor.0 = 0;
    build_memorial_grid(&mut grid, &meta, 0, &message.0);
}

fn memorial_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: ResMut<Cursor>,
    mut grid: ResMut<TileGrid>,
    mut message: ResMut<MessageLine>,
    meta: Res<MetaProgress>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        message.0.clear();
        next_state.set(GameState::Area);
        return;
    }

    let n = meta.memorial.len();
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
    }
    if changed {
        build_memorial_grid(&mut grid, &meta, cursor.0, &message.0);
    }
}

// ---- inventory ----

fn inventory_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: ResMut<Cursor>,
    mut run: ResMut<RunState>,
    mut message: ResMut<MessageLine>,
    mut grid: ResMut<TileGrid>,
    mut combat: ResMut<Combat>,
    mut rng: ResMut<Rng>,
    zone: Res<ZoneData>,
    clock: Res<GameClock>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    let back = if combat.active { GameState::Combat } else { GameState::Area };
    if keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::Tab) {
        if !combat.active {
            message.0.clear();
        }
        next_state.set(back);
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
            // GDD §8: rummaging in a fight costs 4 AP, and may hand the turn over.
            if combat.active && combat.ap < combat::AP_ITEM {
                message.0 = "No AP left for that.".into();
            } else {
                let (msg, acted) = use_item(&zone, &mut run, cursor.0);
                message.0 = msg;
                cursor.0 = cursor.0.min(run.items.len().saturating_sub(1));
                if combat.active && acted {
                    combat.ap -= combat::AP_ITEM;
                    if let Some(theirs) = combat::end_turn(&mut combat, &mut run, &zone, &mut rng) {
                        message.0 = format!("{} {theirs}", message.0);
                    }
                }
            }
            changed = true;
        }
    }

    if changed {
        build_inventory_grid(&mut grid, &zone, &run, &clock, cursor.0, &message.0);
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
    clock: Res<GameClock>,
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
            &clock,
            &trade_ui.vendor,
            trade_ui.buying,
            cursor.0,
            &message.0,
        );
    }
}

// ---- the end of a run ----

fn enter_gameover(mut grid: ResMut<TileGrid>, run: Res<RunState>) {
    let cause = run.death.clone().unwrap_or_default();
    build_gameover_grid(&mut grid, &run, &cause);
}

fn gameover_input(keys: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

fn pressed_letter(keys: &ButtonInput<KeyCode>) -> Option<char> {
    LETTER_KEYS
        .iter()
        .find(|(k, _)| keys.just_pressed(*k))
        .map(|(_, c)| *c)
}

// ---- headless playthrough (SPEC §7, §10.3) ----
//
// The game is `add_game` plus a window and a renderer. Drop those two and the same
// app runs headless, so a test can press keys into it and read the TileGrid back.
// This is the acceptance play-through the milestones ask for, run by `cargo test`.

#[cfg(test)]
mod playthrough {
    use super::*;
    use crate::render::GRID_W;
    use crate::run::Skill;
    use bevy::state::app::StatesPlugin;

    struct Sim {
        app: App,
        #[allow(dead_code)] // held so the temp save directory outlives the app
        saves: std::path::PathBuf,
    }

    impl Sim {
        /// A fresh app on a fixed seed, wound forward to the creation screen.
        fn new() -> Self {
            Sim::with_saves(&format!("run-{}", std::process::id()))
        }

        /// As `new`, but on a named save directory, so a test can watch a stalker
        /// die and then check what the next one inherits. Never the player's own.
        fn with_saves(tag: &str) -> Self {
            let saves = std::env::temp_dir().join(format!("the-zone-test-{tag}"));
            let _ = std::fs::remove_dir_all(&saves);
            Sim::reopen(saves)
        }

        /// Opens a fresh app over an existing save directory, the way starting the
        /// game again would.
        fn reopen(saves: std::path::PathBuf) -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, StatesPlugin));
            app.init_resource::<ButtonInput<KeyCode>>();
            add_game(&mut app);
            app.insert_resource(Rng::new(20260905));
            app.insert_resource(SaveDir(saves.clone()));
            app.update();
            Sim { app, saves }
        }

        /// One keypress, then an idle frame so the state transition it asked for
        /// lands and the new screen is built before anything is read back.
        fn press(&mut self, key: KeyCode) -> &mut Self {
            self.app
                .world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
            self.app.update();
            // `press` only sets just_pressed on a key that was up, so let it up again.
            let mut keys = self.app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(key);
            keys.clear();
            self.app.update();
            self
        }

        fn row(&self, y: usize) -> String {
            let grid = self.app.world().resource::<TileGrid>();
            (0..GRID_W)
                .map(|x| grid.cells[y * GRID_W + x].ch)
                .collect::<String>()
                .trim_end()
                .to_string()
        }

        fn screen(&self) -> String {
            (0..30).map(|y| self.row(y) + "\n").collect()
        }

        fn shows(&self, text: &str) -> bool {
            self.screen().contains(text)
        }

        fn assert_shows(&self, text: &str) {
            assert!(self.shows(text), "expected `{text}` on:\n{}", self.screen());
        }

        /// The right-hand panel starts at `PANEL_COL`, so the column before it is a
        /// gutter. Anything in it means left-hand text has run under the panel and
        /// is being overwritten by it — how `rises twiLoner fast` got shipped.
        fn assert_gutter_clear(&self) {
            let grid = self.app.world().resource::<TileGrid>();
            for y in 0..=screens::PANEL_LAST_ROW {
                let ch = grid.cells[y * GRID_W + screens::PANEL_COL - 1].ch;
                assert_eq!(ch, ' ', "row {y} runs into the panel:\n{}", self.screen());
            }
        }

        /// Moves the menu cursor onto `label` and confirms it. Panics if the label
        /// is not in the menu, which is what makes this a test and not a click macro.
        fn choose(&mut self, label: &str) -> &mut Self {
            let wanted = format!("> {label}");
            for _ in 0..6 {
                if (22..27).any(|y| self.row(y).starts_with(&wanted)) {
                    return self.press(KeyCode::Enter);
                }
                self.press(KeyCode::ArrowDown);
            }
            panic!("no menu entry `{label}` on:\n{}", self.screen());
        }

        fn state(&self) -> GameState {
            *self.app.world().resource::<State<GameState>>().get()
        }

        fn run(&self) -> &RunState {
            self.app.world().resource::<RunState>()
        }

        fn run_mut(&mut self) -> Mut<'_, RunState> {
            self.app.world_mut().resource_mut::<RunState>()
        }

        /// Hands the stalker a weapon they can actually use, and the skill to use it.
        fn arm_with(&mut self, item: &str) -> &mut Self {
            let mut run = self.run_mut();
            run.add_item(item, 1);
            run.weapon = Some(item.into());
            run.add_item("vest", 1);
            run.armor = Some("vest".into());
            run.skills[Skill::SmallGuns.index()] = 90;
            run.skills[Skill::Melee.index()] = 90;
            self
        }

        fn combat(&self) -> &Combat {
            self.app.world().resource::<Combat>()
        }

        /// Camp to the quarry rim, the long way, through the anomaly field.
        fn walk_to_the_rim(&mut self) -> &mut Self {
            self.choose("Travel");
            self.choose("Cut toward the field");
            self.run_mut().skills[Skill::StalkerLore.index()] = 95;
            self.choose("Scan");
            self.choose("Push Through")
        }

        /// Picks a row out of any list screen - inventory, board, map, dialogue -
        /// by the text it starts with, and confirms it.
        fn choose_listed(&mut self, name: &str) -> &mut Self {
            for _ in 0..14 {
                if (4..18).any(|y| self.row(y).starts_with(&format!("> {name}"))) {
                    return self.press(KeyCode::Enter);
                }
                self.press(KeyCode::ArrowDown);
            }
            panic!("no row `{name}` on:\n{}", self.screen());
        }

        /// Plays the fight out: shoot when the weapon reaches, otherwise close.
        /// Panics rather than looping forever if combat will not resolve.
        fn fight(&mut self) -> &mut Self {
            for _ in 0..60 {
                if self.state() != GameState::Combat {
                    return self;
                }
                // Shoot when the AP and the range allow, otherwise reposition.
                if (22..27).any(|y| self.row(y).contains("Attack (")) {
                    self.choose("Attack");
                } else if (22..27).any(|y| self.row(y).contains("Close In")) {
                    self.choose("Close In");
                } else {
                    self.choose("Fall Back");
                }
            }
            panic!("the fight never ended:\n{}", self.screen());
        }

        fn meta(&self) -> &MetaProgress {
            self.app.world().resource::<MetaProgress>()
        }

        /// Walks the whole route in: camp, road, field, across it, then the tunnel
        /// the Room opens for anyone who stands high enough with somebody.
        fn walk_to_the_room(&mut self) -> &mut Self {
            self.arm_with("pistol");
            self.walk_to_the_rim();
            if self.state() == GameState::Combat {
                self.fight();
            }
            self.run_mut().rep.insert("loners".into(), 60);
            // Standing is read on entry, so step out and back to light the letter.
            self.choose("Back through the field");
            self.choose("Push Through");
            if self.state() == GameState::Combat {
                self.fight();
            }
            assert!(self.run().is_revealed("quarry", 'T'), "the tunnel:\n{}", self.screen());
            self.press(KeyCode::KeyT)
        }

        /// Background 0 (Loner), the first three skills tagged.
        fn roll_a_stalker(&mut self) -> &mut Self {
            self.press(KeyCode::Enter) // take the Loner
                .press(KeyCode::Enter) // tag Small Guns
                .press(KeyCode::ArrowDown)
                .press(KeyCode::Enter) // tag Energy Weapons
                .press(KeyCode::ArrowDown)
                .press(KeyCode::Enter) // tag Melee, and the run starts
        }
    }

    #[test]
    fn creation_rolls_a_stalker_and_drops_them_at_the_camp() {
        let mut sim = Sim::new();
        sim.assert_shows("THE ZONE - a new stalker");
        sim.assert_shows("Loner");
        sim.assert_gutter_clear();
        sim.press(KeyCode::Enter); // into the tag-skills phase
        sim.assert_shows("Tag 3 skills");
        sim.assert_gutter_clear();

        let mut sim = Sim::new();
        sim.roll_a_stalker();
        assert_eq!(sim.state(), GameState::Area);
        sim.assert_shows("A camp on the Zone");
        sim.assert_shows("Travel");
        sim.assert_shows("HP 38/38");
        sim.assert_shows("Day 1 06:00");
        assert!(sim.run().tags[0] && sim.run().tags[1] && sim.run().tags[2]);
    }

    #[test]
    fn the_hidden_letter_opens_the_hatch_and_the_log_unlocks_the_quarry_crate() {
        let mut sim = Sim::new();
        sim.roll_a_stalker();

        // The camp's D is ungated, so it is lit from the moment you arrive.
        assert!(sim.run().is_revealed("camp", 'D'));
        sim.press(KeyCode::KeyD);
        sim.assert_shows("A cramped bunker beneath the camp");

        // The quarry crate is gated on a flag you can only pick up down here.
        assert!(!sim.run().flags.contains("read_the_log"));
        sim.choose("Read the log");
        assert!(sim.run().flags.contains("read_the_log"));

        sim.choose("Return");
        sim.choose("Travel"); // camp -> road
        sim.assert_shows("A dirt road between the camp and the wastes");
        sim.choose("Cut toward the field");
        sim.assert_shows("Bent air over a scorched clearing");

        // Read the field before walking into it, then cross on the line you found.
        sim.run_mut().skills[Skill::StalkerLore.index()] = 95;
        let hp = sim.run().hp;
        sim.choose("Scan");
        sim.choose("Push Through");
        assert_eq!(sim.run().hp, hp, "a scanned field is crossed unharmed");

        // Something lives on the rim. Shoot it, and it drops something worth money.
        assert_eq!(sim.state(), GameState::Combat);
        sim.assert_shows("A Flesh comes at you");
        sim.arm_with("pistol");
        sim.fight();
        assert_eq!(sim.state(), GameState::Area);
        assert!(
            sim.run().items.iter().any(|(i, _)| i == "flesh_eye"),
            "a dead Flesh pays for the trip:\n{}",
            sim.screen()
        );
        sim.assert_shows("A quarry rim above black water");

        // The flag set back in the hatch is what lights S here.
        assert!(sim.run().is_revealed("quarry", 'S'));
        sim.press(KeyCode::KeyS);
        sim.assert_shows("The log was right");
    }

    #[test]
    fn an_unrevealed_letter_is_just_art() {
        let mut sim = Sim::new();
        sim.roll_a_stalker();
        sim.choose("Travel"); // camp -> road
        sim.choose("Cut toward the field");

        // G is behind a Stalker Lore check the starting Loner is unlikely to pass;
        // whichever way this seed rolled it, the key must agree with the letter.
        let revealed = sim.run().is_revealed("field", 'G');
        let before = sim.run().items.clone();
        sim.press(KeyCode::KeyG);
        assert_eq!(
            sim.run().items != before,
            revealed,
            "the key works exactly when the letter is lit"
        );
    }

    #[test]
    fn a_fight_is_fought_in_action_points_across_the_bands() {
        let mut sim = Sim::new();
        sim.roll_a_stalker();
        sim.arm_with("pistol");
        sim.walk_to_the_rim();

        // A fight opens at far, with a full turn of AP (GDD §4: 5 + AGI/2).
        assert_eq!(sim.state(), GameState::Combat);
        let ap = sim.combat().ap;
        assert_eq!(ap, 5 + sim.run().attrs[run::AGI] / 2);
        sim.assert_shows("Flesh");
        sim.assert_shows("far");
        sim.assert_shows("AP 7");

        // A pistol reaches across the field, and shooting costs 4 of those points.
        // The turn is not over: 3 AP still buys a move.
        sim.choose("Attack");
        assert_eq!(sim.combat().ap, ap - combat::AP_ATTACK);
        sim.choose("Close In");

        // Spending the last of it hands the turn over, and a Flesh can only bite,
        // so it closes the rest of the distance itself.
        assert_eq!(sim.combat().band, combat::Band::Melee, "you closed, then it did");
        assert_eq!(sim.combat().ap, ap, "a fresh turn comes back");

        // Rummaging mid-fight is the footer's Inventory, and it is not free.
        sim.run_mut().hp = 10;
        sim.press(KeyCode::Tab);
        assert_eq!(sim.state(), GameState::Inventory);
        sim.choose_listed("Medkit");
        assert!(sim.run().hp > 10, "the medkit went in");
        assert_eq!(sim.combat().ap, ap - combat::AP_ITEM, "and it cost 4 AP");
        sim.press(KeyCode::Tab);
        assert_eq!(sim.state(), GameState::Combat, "Tab goes back to the fight");

        // There is no walking away from a fight with Esc (SPEC §6).
        sim.press(KeyCode::Escape);
        assert_eq!(sim.state(), GameState::Combat);

        sim.fight();
        assert_eq!(sim.state(), GameState::Area);
        assert!(!sim.combat().active);
    }

    #[test]
    fn a_fight_can_kill_you() {
        let mut sim = Sim::new();
        sim.roll_a_stalker();
        sim.walk_to_the_rim();
        assert_eq!(sim.state(), GameState::Combat);

        // Unarmed, at one hit point, against a Flesh. This ends one way.
        sim.run_mut().hp = 1;
        for _ in 0..40 {
            if sim.state() != GameState::Combat {
                break;
            }
            sim.choose("Close In");
        }
        assert_eq!(sim.state(), GameState::GameOver);
        sim.assert_shows("THE ZONE IS STILL THERE");
    }

    #[test]
    fn the_room_is_reached_by_standing_and_answers_what_the_run_built() {
        let mut sim = Sim::with_saves("room");
        sim.roll_a_stalker();
        sim.walk_to_the_room();
        sim.assert_shows("Light comes off the far wall");

        sim.choose("Step into the light");
        assert_eq!(sim.state(), GameState::Dialogue);
        sim.assert_shows("The light does not flicker");

        // The short list is built from the run (GDD 9). This one killed a Flesh and
        // is carrying the pay for it, so those wishes are open; the Monolith one is
        // not, because this stalker has never listened to them.
        sim.assert_shows("[killed a Flesh]");
        sim.assert_shows("Make me whole");
        sim.choose_listed("[Monolith 50]");
        sim.assert_shows("not the one to make that argument");

        // Asking ends the run, whichever way the Zone reads it.
        sim.choose_listed("Make me whole");
        assert_eq!(sim.state(), GameState::GameOver);
        sim.assert_shows("The wish to be whole");
        sim.assert_shows("THE ZONE IS STILL THERE");
        assert_eq!(sim.meta().memorial.len(), 1, "every ending writes a memorial entry");
        assert!(sim.meta().unlocked(2), "reaching the centre earns the Ecologist");
    }

    #[test]
    fn a_dead_stalker_leaves_something_for_the_next_one() {
        let saves = std::env::temp_dir().join("the-zone-test-permadeath");
        let _ = std::fs::remove_dir_all(&saves);

        let mut sim = Sim::reopen(saves.clone());
        sim.roll_a_stalker();
        let name = sim.run().name.clone();
        sim.run_mut().rep.insert("loners".into(), 80);
        sim.run_mut().rads = sim::RAD_DEATH;
        sim.choose("Travel");
        assert_eq!(sim.state(), GameState::GameOver);
        sim.assert_shows(&name);

        // Start the game again. The Zone remembers, at a quarter rate (GDD 10).
        let mut next = Sim::reopen(saves);
        assert_eq!(next.meta().memorial.len(), 1);
        assert_eq!(next.meta().rep["loners"], 20);
        next.roll_a_stalker();
        // 20 from being a Loner, plus the 20 the last one left behind.
        assert_eq!(next.run().rep_of("loners"), 40);
        assert_ne!(next.run().name, "", "the new one has their own name");

        // And the camp carries the memorial, so you can read who went before.
        next.choose("The memorial");
        assert_eq!(next.state(), GameState::Memorial);
        next.assert_shows("THE MEMORIAL");
        next.assert_shows(&name);
        next.assert_shows("Radiation");
        next.press(KeyCode::Escape);
        assert_eq!(next.state(), GameState::Area);
    }

    #[test]
    fn a_locked_background_stays_locked_until_it_is_earned() {
        let mut sim = Sim::with_saves("locked");
        sim.assert_shows("Ecologist (locked)");
        // Down twice to the Ecologist, then try to take it.
        sim.press(KeyCode::ArrowDown).press(KeyCode::ArrowDown);
        sim.press(KeyCode::Enter);
        assert_eq!(sim.state(), GameState::CharacterCreation, "still choosing");
        sim.assert_shows("Nobody with that history has come back yet");
    }

    #[test]
    fn suspending_puts_the_run_down_and_picking_it_up_spends_the_file() {
        let saves = std::env::temp_dir().join("the-zone-test-suspend");
        let _ = std::fs::remove_dir_all(&saves);

        let mut sim = Sim::reopen(saves.clone());
        sim.roll_a_stalker();
        sim.choose("Travel"); // out to the road, so there is something to restore
        sim.run_mut().rubles = 1234;
        let name = sim.run().name.clone();
        let minutes = sim.run().minutes;
        sim.press(KeyCode::F5);

        // Starting again drops you straight back where you stood.
        let mut back = Sim::reopen(saves.clone());
        assert_eq!(back.state(), GameState::Area);
        back.assert_shows("A dirt road between the camp and the wastes");
        assert_eq!(back.run().rubles, 1234);
        assert_eq!(back.run().name, name);
        assert_eq!(back.run().minutes, minutes);
        assert!(back.run().discovered.contains("road"));

        // The file is spent. There is no reload (GDD 10).
        let again = Sim::reopen(saves);
        assert_eq!(again.state(), GameState::CharacterCreation);
    }

    #[test]
    fn grisha_will_not_hear_an_argument_you_cannot_make() {
        let mut sim = Sim::new();
        sim.roll_a_stalker();
        sim.choose("The bar");
        sim.assert_shows("A shed with a plank for a bar");

        sim.choose("Talk to Grisha");
        assert_eq!(sim.state(), GameState::Dialogue);
        sim.assert_shows("Everyone who walks here wants something");

        // The gated line is on screen - that is the point of the brackets - but a
        // starting Loner cannot say it.
        sim.assert_shows("[Barter 45]");
        sim.choose_listed("[Barter 45]");
        assert_eq!(sim.state(), GameState::Dialogue, "a blocked line goes nowhere");
        sim.assert_shows("not the one to make that argument");

        // Learn to haggle and the same line opens, and carries you to his shelf.
        sim.run_mut().skills[Skill::Barter.index()] = 45;
        sim.choose_listed("[Barter 45]");
        sim.assert_shows("For you, cost");
        sim.choose_listed("Let me see the shelf.");
        assert_eq!(sim.state(), GameState::Trade);
        sim.assert_shows("TRADE - Grisha");
        sim.assert_shows("Vodka");

        sim.press(KeyCode::Escape);
        assert_eq!(sim.state(), GameState::Area);
    }

    #[test]
    fn a_job_off_the_board_pays_when_you_do_it() {
        let mut sim = Sim::new();
        sim.roll_a_stalker();
        sim.choose("The bar");
        sim.choose("Job board");
        assert_eq!(sim.state(), GameState::Jobs);
        sim.assert_shows("JOB BOARD - Loners");
        sim.assert_shows("An eye for the pot");

        sim.choose_listed("Walk the rim");
        assert!(sim.run().quests_taken.contains("walk_the_rim"));
        sim.press(KeyCode::Escape);
        assert_eq!(sim.state(), GameState::Area);

        // The journal is where a taken job lives, alongside your standing.
        sim.press(KeyCode::F3);
        assert_eq!(sim.state(), GameState::Journal);
        sim.assert_shows("Walk the rim");
        sim.assert_shows("Loners");
        sim.press(KeyCode::Escape);

        // Do the job. It settles the moment the goal is met.
        let purse = sim.run().rubles;
        let standing = sim.run().rep_of("loners");
        sim.choose("Outside");
        sim.arm_with("pistol");
        sim.walk_to_the_rim();
        assert!(sim.run().quests_done.contains("walk_the_rim"), "{}", sim.screen());
        assert_eq!(sim.run().rubles, purse + 400);
        assert!(sim.run().rep_of("loners") > standing);
        // Helping the Loners costs you with the people they hate (GDD 9).
        assert!(sim.run().rep_of("bandits") < 0);
    }

    #[test]
    fn the_map_walks_you_next_door_and_no_further() {
        let mut sim = Sim::new();
        sim.roll_a_stalker();
        sim.choose("Travel");
        sim.choose("Cut toward the field");

        sim.press(KeyCode::F2);
        assert_eq!(sim.state(), GameState::Map);
        sim.assert_shows("you are here");
        // Only what you have stood in is on it (GDD 5).
        sim.assert_shows("The Whirligig Field");
        assert!(!sim.shows("Checkpoint 4"), "you have never been north:\n{}", sim.screen());

        // The camp is known, but it is two areas away.
        sim.choose_listed("Base Camp");
        assert_eq!(sim.state(), GameState::Map);
        sim.assert_shows("not next to here");

        // The road is next door, so the map walks you there.
        sim.choose_listed("The Road");
        assert_eq!(sim.state(), GameState::Area);
        sim.assert_shows("A dirt road between the camp and the wastes");
    }

    #[test]
    fn the_new_posts_carry_their_own_vendors_and_boards() {
        let mut sim = Sim::new();
        sim.roll_a_stalker();
        sim.choose("Travel");
        sim.choose("North, to the checkpoint");
        sim.assert_shows("A checkpoint of sandbags");

        // Duty runs its own board, and its own quartermaster.
        sim.choose("Quartermaster");
        assert_eq!(sim.state(), GameState::Trade);
        sim.assert_shows("TRADE - Quartermaster Osip");
        sim.assert_shows("Abakan Rifle");
        sim.press(KeyCode::Escape);

        sim.choose("Job board");
        sim.assert_shows("JOB BOARD - Duty");
        sim.assert_shows("Cull the rim");
        assert!(!sim.shows("An eye for the pot"), "that is Loner work");
        sim.press(KeyCode::Escape);

        // Osip's own gated line needs Duty standing, which a Loner has none of.
        sim.choose("Talk to Osip");
        sim.assert_shows("Duty holds this road");
        sim.choose_listed("[Duty 25]");
        sim.assert_shows("not the one to make that argument");
    }

    #[test]
    fn the_counter_takes_your_rubles_and_the_pack_remembers() {
        let mut sim = Sim::new();
        sim.roll_a_stalker();
        let purse = sim.run().rubles;

        sim.choose("Trade");
        assert_eq!(sim.state(), GameState::Trade);
        sim.assert_shows("TRADE - Sidorovich");
        sim.assert_shows("Medkit");

        sim.press(KeyCode::Enter); // buy whatever is on the top row
        assert!(sim.run().rubles < purse, "buying costs money");
        sim.assert_shows("You buy the");

        sim.press(KeyCode::Escape);
        assert_eq!(sim.state(), GameState::Area);

        sim.press(KeyCode::Tab);
        assert_eq!(sim.state(), GameState::Inventory);
        sim.assert_shows("INVENTORY");
        sim.assert_shows("Medkit");
        sim.assert_gutter_clear();
        sim.press(KeyCode::Escape);
        assert_eq!(sim.state(), GameState::Area);
    }

    #[test]
    fn resting_costs_time_and_rubles_and_a_dead_stalker_stops_playing() {
        let mut sim = Sim::new();
        sim.roll_a_stalker();

        let purse = sim.run().rubles;
        sim.run_mut().rads = 250;
        sim.choose("Rest");
        assert_eq!(sim.run().rubles, purse - run::REST_COST);
        assert_eq!(sim.run().rads, 150);
        sim.assert_shows("Day 1 14:00");

        // Radiation kills at 1000, and the next action is the one that finds out.
        sim.run_mut().rads = sim::RAD_DEATH;
        sim.choose("Travel");
        assert_eq!(sim.state(), GameState::GameOver);
        sim.assert_shows("THE ZONE IS STILL THERE");
        sim.assert_shows("Radiation");
    }
}
