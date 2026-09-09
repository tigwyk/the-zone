//! The Zone — M7 (polish).

#[cfg(test)]
mod balance;
mod area;
mod audio;
mod combat;
mod craft;
mod dialogue;
mod liquid;
mod loot;
mod meta;
mod quest;
mod render;
mod run;
mod screens;
mod sim;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowCloseRequested, WindowResolution};

use area::{build_area_grid, Action, VendorStock, ZoneData};
use combat::{build_combat_grid, Combat};
use craft::{build_craft_grid, craft, craft_available, sorted_recipes};
use liquid::{spill, Liquid, Puddles};
use dialogue::{build_dialogue_grid, Dialogue};
use meta::{build_memorial_grid, MetaProgress, SaveDir};
use quest::{build_board_grid, build_journal_grid, Board};
use render::{render_grid, spawn_scanlines, toggle_scanlines, Glitch, TileGrid};
use run::{Rng, RunState, BACKGROUNDS, REST_COST, REST_RADS, SKILL_NAMES, TAG_COUNT};
use screens::{
    build_creation_grid, build_gameover_grid, build_inventory_grid, build_main_menu_grid,
    build_map_grid, build_trade_grid, known_areas, pull_mod, trade_list, trade_one, use_item,
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
    MainMenu,
    CharacterCreation,
    Area,
    Inventory,
    Trade,
    Crafting,
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
        .init_resource::<Puddles>()
        .init_resource::<Combat>()
        .init_resource::<Dialogue>()
        .init_resource::<Board>()
        .init_resource::<SaveDir>()
        .init_resource::<MetaProgress>()
        .init_resource::<Creation>()
        .init_resource::<TileGrid>()
        .add_message::<WindowCloseRequested>()
        .init_state::<GameState>()
        .add_systems(Startup, begin)
        .add_systems(OnEnter(GameState::MainMenu), enter_main_menu)
        .add_systems(OnEnter(GameState::CharacterCreation), enter_creation)
        .add_systems(OnEnter(GameState::Area), redraw_area)
        .add_systems(OnEnter(GameState::Inventory), enter_inventory)
        .add_systems(OnEnter(GameState::Trade), enter_trade)
        .add_systems(OnEnter(GameState::Crafting), enter_craft)
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
                main_menu_input.run_if(in_state(GameState::MainMenu)),
                creation_input.run_if(in_state(GameState::CharacterCreation)),
                menu_input.run_if(in_state(GameState::Area)),
                inventory_input.run_if(in_state(GameState::Inventory)),
                trade_input.run_if(in_state(GameState::Trade)),
                craft_input.run_if(in_state(GameState::Crafting)),
                combat_input.run_if(in_state(GameState::Combat)),
                dialogue_input.run_if(in_state(GameState::Dialogue)),
                board_input.run_if(in_state(GameState::Jobs)),
                map_input.run_if(in_state(GameState::Map)),
                journal_input.run_if(in_state(GameState::Journal)),
                memorial_input.run_if(in_state(GameState::Memorial)),
                gameover_input.run_if(in_state(GameState::GameOver)),
                save_on_exit,
            )
                .in_set(GameInput),
        )
}

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "The Zone".into(),
            // 1280×720 logical, with the native scale factor respected: the fixed
            // grid renders at the same readable size on a 1× monitor and a Retina
            // display, instead of a quarter-sized window (the old 1.0 override).
            resolution: WindowResolution::new(1280, 720),
            present_mode: PresentMode::AutoVsync,
            ..default()
        }),
        ..default()
    }))
    .add_plugins(bevy_term::TermWindowPlugin {
        width: render::GRID_W as u16,
        height: render::GRID_H as u16,
        ..default()
    });
    // The window, the renderer and the polish. None of it is in `add_game`, so the
    // play-through tests still run headless.
    add_game(&mut app)
        .init_resource::<Glitch>()
        .add_systems(Startup, (spawn_scanlines, audio::load_cues))
        .add_systems(
            Update,
            (drive_glitch, render_grid, toggle_scanlines, audio::play_cues).after(GameInput),
        )
        .run();
}

/// The glitch is presentation, so it lives here and not in `add_game`: it reads the
/// current area and paints the HUD with interference where an anomaly is there to
/// sell. A scanned field glitches half as hard — you have seen it coming.
fn drive_glitch(
    mut glitch: ResMut<Glitch>,
    area: Res<CurrentArea>,
    zone: Res<ZoneData>,
    fields: Res<Fields>,
) {
    let here = area.0.as_str();
    glitch.level = zone.areas[here].anomaly.as_ref().map_or(0.0, |a| {
        let base = (a.danger as f32 / 250.0).clamp(0.0, 1.0);
        if fields.get(here).scanned { base * 0.5 } else { base }
    });
}

// ---- character creation ----

fn enter_creation(
    mut grid: ResMut<TileGrid>,
    mut c: ResMut<Creation>,
    meta: Res<MetaProgress>,
    message: Res<MessageLine>,
) {
    // Every "New Game" starts back at the top of the roll, not where the last one
    // left the cursor.
    *c = Creation::default();
    build_creation_grid(&mut grid, &meta, c.phase, c.sel, c.background, &c.tags, &message.0);
}

/// First frame: read the memorial and draw the main menu. Picking the run back up
/// is the menu's job now, because reading the suspend file spends it (GDD §10).
fn begin(
    dir: Res<SaveDir>,
    mut meta: ResMut<MetaProgress>,
    mut cursor: ResMut<Cursor>,
    grid: ResMut<TileGrid>,
    message: Res<MessageLine>,
) {
    *meta = MetaProgress::load(&dir);
    cursor.0 = 0;
    enter_main_menu(cursor, grid, dir, message);
}

// ---- the main menu ----

const CONTINUE: &str = "Continue";
const NEW_GAME: &str = "New Game";
const QUIT: &str = "Quit";

/// Continue is only offered when there is something to continue.
fn menu_entries(dir: &SaveDir) -> Vec<&'static str> {
    let mut entries = Vec::new();
    if meta::has_suspend(dir) {
        entries.push(CONTINUE);
    }
    entries.push(NEW_GAME);
    entries.push(QUIT);
    entries
}

fn enter_main_menu(
    mut cursor: ResMut<Cursor>,
    mut grid: ResMut<TileGrid>,
    dir: Res<SaveDir>,
    message: Res<MessageLine>,
) {
    cursor.0 = 0;
    build_main_menu_grid(&mut grid, &menu_entries(&dir), 0, &message.0);
}

fn main_menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: ResMut<Cursor>,
    mut grid: ResMut<TileGrid>,
    mut act: Act,
    mut next_state: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let entries = menu_entries(&act.dir);
    let mut changed = arrows(&keys, &mut cursor.0, entries.len());

    if keys.just_pressed(KeyCode::Enter) {
        match entries[cursor.0.min(entries.len() - 1)] {
            CONTINUE => {
                // The file can be there and still be unreadable - an older build
                // wrote it. That is a fresh start, not a crash.
                if let Some(save) = meta::resume(&act.dir) {
                    *act.run = save.run;
                    act.area.0 = save.area;
                    *act.clock = save.clock;
                    act.fields.0 = save.fields;
                    act.stock.0 = save.stock;
                    act.puddles.0 = save.puddles;
                    *act.rng = save.rng;
                    act.message.0 = "You pick up where you put it down.".into();
                    next_state.set(GameState::Area);
                    return;
                }
                act.message.0 = "That run is gone. The Zone kept it.".into();
            }
            NEW_GAME => {
                next_state.set(GameState::CharacterCreation);
                return;
            }
            _ => {
                exit.write(AppExit::Success);
                return;
            }
        }
        changed = true;
    }

    if changed {
        build_main_menu_grid(&mut grid, &menu_entries(&act.dir), cursor.0, &act.message.0);
    }
}

fn creation_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut c: ResMut<Creation>,
    mut menu_sel: ResMut<MenuSelection>,
    mut grid: ResMut<TileGrid>,
    mut act: Act,
    mut next_state: ResMut<NextState<GameState>>,
) {
    let n = if c.phase == 0 { BACKGROUNDS.len() } else { SKILL_NAMES.len() };
    let mut changed = arrows(&keys, &mut c.sel, n);

    if keys.just_pressed(KeyCode::Enter) {
        if c.phase == 0 {
            if !act.meta.unlocked(c.sel) {
                act.message.0 = "Nobody with that history has come back yet.".into();
                build_creation_grid(&mut grid, &act.meta, c.phase, c.sel, c.background, &c.tags, &act.message.0);
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
                *act.run = RunState::roll(c.background, &c.tags, &mut act.rng);
                // A fresh stalker starts clean: back at the camp with the Zone
                // re-rolled around them. Only the standing and lore the last one
                // banked carry over (GDD §10).
                act.area.0 = act.zone.start.clone();
                menu_sel.0 = 0;
                *act.fields = Fields::default();
                *act.puddles = Puddles::seed(&act.zone);
                *act.stock = VendorStock(
                    act.zone
                        .vendors
                        .iter()
                        .map(|(id, v)| (id.clone(), area::shelf(v)))
                        .collect(),
                );
                *act.combat = Combat::default();
                *act.dialogue = Dialogue::default();
                *act.board = Board::default();
                *act.trade_ui = TradeUi::default();
                // A quarter of the last stalker's standing came with you (GDD §10).
                for (faction, carried) in &act.meta.rep {
                    let now = act.run.rep_of(faction);
                    act.run.rep.insert(faction.clone(), (now + carried).clamp(-100, 100));
                }
                let here = act.area.0.clone();
                act.clock.schedule(&act.run, &mut act.rng);
                reveal_secrets(&here, &mut act.run, &act.zone, &mut act.rng);
                act.run.discovered.insert(here);
                act.message.0 = "You sign the ledger and walk in.".into();
                next_state.set(GameState::Area);
                return;
            }
        }
        changed = true;
    }

    if changed {
        build_creation_grid(&mut grid, &act.meta, c.phase, c.sel, c.background, &c.tags, &act.message.0);
    }
}

// ---- area ----

fn redraw_area(
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    run: Res<RunState>,
    clock: Res<GameClock>,
    fields: Res<Fields>,
    puddles: Res<Puddles>,
    area: Res<CurrentArea>,
    sel: Res<MenuSelection>,
    message: Res<MessageLine>,
) {
    build_area_grid(&mut grid, &zone, &run, &clock, &fields, &puddles, &area.0, sel.0, &message.0);
}

fn enter_craft(
    mut cursor: ResMut<Cursor>,
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    run: Res<RunState>,
    clock: Res<GameClock>,
    message: Res<MessageLine>,
) {
    cursor.0 = 0;
    build_craft_grid(&mut grid, &zone, &run, &clock, cursor.0, &message.0);
}

fn craft_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: ResMut<Cursor>,
    mut grid: ResMut<TileGrid>,
    mut act: Act,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        act.message.0.clear();
        next_state.set(GameState::Area);
        return;
    }

    let list = sorted_recipes(&act.zone);
    let n = list.len();
    let mut changed = arrows(&keys, &mut cursor.0, n);
    if n > 0 {
        if keys.just_pressed(KeyCode::Enter) {
            let recipe = list[cursor.0.min(n - 1)];
            let here = act.area.0.clone();
            match craft_available(&act.zone, &act.run, recipe) {
                Some(reason) => act.message.0 = format!("You cannot: {reason}."),
                None => {
                    act.message.0 = craft(recipe, &mut act.run, &act.zone, &mut act.rng);
                    // The bench costs time, and an emission does not wait for it.
                    let tail = advance(
                        recipe.minutes,
                        &here,
                        &mut act.run,
                        &act.zone,
                        &mut act.clock,
                        &mut act.fields,
                        &mut act.stock,
                        &mut act.rng,
                    );
                    if !tail.is_empty() {
                        act.message.0 = format!("{} {tail}", act.message.0);
                    }
                }
            }
            changed = true;
        }
    }

    if check_death(&mut act.run) {
        let cause = act.run.death.clone().unwrap_or_default();
        let message = act.message.0.clone();
        meta::bank(&mut act.meta, &act.run, &act.dir, &cause, &message);
        next_state.set(GameState::GameOver);
        return;
    }

    if changed {
        build_craft_grid(&mut grid, &act.zone, &act.run, &act.clock, cursor.0, &act.message.0);
    }
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
    puddles: ResMut<'w, Puddles>,
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

/// The bookmark for the run as it stands.
fn suspend_run(act: &Act) -> bool {
    meta::suspend(
        &act.dir,
        &act.run,
        &act.area.0,
        &act.clock,
        &act.fields,
        &act.stock,
        &act.puddles,
        &act.rng,
    )
}

/// `ponytail:` this listens for the close request rather than `AppExit`, because the
/// winit runner tears the app down without running another `Last` once the exit has
/// been written - a `Last` system reading `AppExit` never fires on the X.
fn save_on_exit(
    mut closes: MessageReader<WindowCloseRequested>,
    state: Res<State<GameState>>,
    act: Act,
) {
    if closes.read().next().is_some()
        && !matches!(
            state.get(),
            GameState::MainMenu
                | GameState::CharacterCreation
                | GameState::Combat
                | GameState::GameOver
        )
    {
        suspend_run(&act);
    }
}

fn menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut sel: ResMut<MenuSelection>,
    mut grid: ResMut<TileGrid>,
    mut act: Act,
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
    let here = act.area.0.clone();
    let menu: Vec<Action> = visible_menu(&act.zone.areas[here.as_str()], &act.run, &act.fields, &here)
        .iter()
        .map(|(_, a)| a.clone())
        .collect();
    let n = menu.len();
    let mut changed = arrows(&keys, &mut sel.0, n);

    if keys.just_pressed(KeyCode::F5) {
        // GDD §10: put the run down. Leaving by any other door writes it too
        // (`save_on_exit`), so this is only the reassurance that it is written.
        act.message.0 = if suspend_run(&act) {
            "Written down. Close the window when you like.".into()
        } else {
            "The Zone will not let you put it down here.".into()
        };
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
        let n = visible_menu(&act.zone.areas[act.area.0.as_str()], &act.run, &act.fields, &act.area.0).len();
        sel.0 = sel.0.min(n.saturating_sub(1));
        build_area_grid(
            &mut grid,
            &act.zone,
            &act.run,
            &act.clock,
            &act.fields,
            &act.puddles,
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
        Action::Craft => {
            act.message.0.clear();
            next_state.set(GameState::Crafting);
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
        Action::Give(item, count) => {
            let key = sim::took_key(&here, item);
            act.message.0 = if act.run.flags.insert(key) {
                // A cache rolls like anything else the Zone has been at, against
                // how deep the place is and how lucky you are.
                let tier = act.zone.areas[here.as_str()].tier;
                let luck = sim::attr(&act.run, &act.zone, run::LCK);
                let uid = act.run.next_uid();
                let stack =
                    loot::roll_item(uid, item, *count, tier, luck, &act.zone, &mut act.rng);
                let name = loot::display_name(&stack, &act.zone);
                let rarity = stack.rarity();
                act.run.add_stack(stack);
                if rarity == loot::Rarity::Plain {
                    format!("You come away with the {name}.")
                } else {
                    format!("You come away with the {name}. The Zone has been at it.")
                }
            } else {
                "You have already had that out of here.".into()
            };
        }
        Action::Spend(item, count) => {
            act.message.0 = if act.run.take_item(item, *count) {
                format!("You hand over the {}.", act.zone.items[item].name)
            } else {
                "You do not have that to give.".into()
            };
        }
        Action::Lore(entry) => {
            let lore = &act.zone.lore[entry];
            act.message.0 = if act.run.lore.insert(entry.clone()) {
                format!("You turn up something: {}. It is in the journal.", lore.title)
            } else {
                format!("{} is already in the journal.", lore.title)
            };
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
                act.message.0 =
                    combat::start(&enemy, combat, &mut act.run, &act.zone, &mut act.rng);
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
    mut act: Act,
    mut next_state: ResMut<NextState<GameState>>,
) {
    // SPEC §6: no Esc out of a fight. Tab still reaches the pack, at 4 AP an item.
    if keys.just_pressed(KeyCode::Tab) {
        next_state.set(GameState::Inventory);
        return;
    }

    let menu = combat::menu(&act.combat, &act.run, &act.zone);
    let n = menu.len();
    let mut changed = arrows(&keys, &mut act.combat.sel, n);

    if keys.just_pressed(KeyCode::Enter) {
        let verb = menu[act.combat.sel.min(n - 1)].1;
        act.message.0 =
            combat::act(verb, &mut act.combat, &mut act.run, &act.zone, &mut act.rng);
        // A kill stays behind: the enemy's blood pools on the floor it died on.
        if !act.combat.active && act.combat.hp <= 0 {
            let (here, blood) = (
                act.area.0.clone(),
                act.zone.enemies[&act.combat.enemy].hp as u32,
            );
            spill(&mut act.puddles, &here, Liquid::Blood, blood);
        }
        changed = true;
    }

    if !changed {
        return;
    }
    if check_death(&mut act.run) {
        // A death in a fight is still a death: the memorial gets the name and the
        // run is spent, exactly as dying anywhere else does (GDD §10). The log's
        // last word is the death itself, so the memorial note reads in context.
        let cause = act.run.death.clone().unwrap_or_default();
        act.message.0 = if act.message.0.is_empty() {
            "You die.".into()
        } else {
            format!("{} You die.", act.message.0.trim_end())
        };
        meta::bank(&mut act.meta, &act.run, &act.dir, &cause, &act.message.0);
        next_state.set(GameState::GameOver);
        return;
    }
    if !act.combat.active {
        next_state.set(GameState::Area);
        return;
    }
    // The menu shrinks as the AP runs low; keep the cursor on something real.
    let n = combat::menu(&act.combat, &act.run, &act.zone).len();
    act.combat.sel = act.combat.sel.min(n.saturating_sub(1));
    build_combat_grid(
        &mut grid,
        &act.zone,
        &act.run,
        &act.clock,
        &act.fields,
        &act.combat,
        &act.area.0,
        &act.message.0,
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
    let mut changed = arrows(&keys, &mut act.dialogue.sel, n);
    if keys.just_pressed(KeyCode::Enter) {
        let taken = dialogue::take(&mut act.dialogue, &act.run, &act.zone);
        if let Some(flag) = &taken.flag {
            act.run.flags.insert(flag.clone());
        }
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
    mut act: Act,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        act.message.0.clear();
        next_state.set(GameState::Area);
        return;
    }

    let offered = quest::offered(&act.zone, &act.run, &act.board.faction);
    let n = offered.len();
    let mut changed = arrows(&keys, &mut act.board.sel, n);
    if n > 0 {
        if keys.just_pressed(KeyCode::Enter) {
            let id = offered[act.board.sel.min(n - 1)].to_string();
            act.message.0 = format!("You take the job: {}.", act.zone.quests[&id].name);
            act.run.quests_taken.insert(id);
            act.board.sel = 0;
            changed = true;
        }
    }

    if changed {
        build_board_grid(&mut grid, &act.zone, &act.run, &act.clock, &act.board, &act.message.0);
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
    let mut changed = arrows(&keys, &mut cursor.0, n);
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

    let n = quest::journal_entries(&zone, &run).len();
    if arrows(&keys, &mut cursor.0, n) {
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
    if arrows(&keys, &mut cursor.0, n) {
        build_memorial_grid(&mut grid, &meta, cursor.0, &message.0);
    }
}

// ---- inventory ----

fn inventory_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: ResMut<Cursor>,
    mut grid: ResMut<TileGrid>,
    mut act: Act,
    mut next_state: ResMut<NextState<GameState>>,
) {
    let back = if act.combat.active { GameState::Combat } else { GameState::Area };
    if keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::Tab) {
        if !act.combat.active {
            act.message.0.clear();
        }
        next_state.set(back);
        return;
    }

    let n = act.run.items.len();
    let mut changed = arrows(&keys, &mut cursor.0, n);
    if n > 0 {
        // GUNS §1: unscrewing the last thing you screwed on, at a Repair check.
        // Never in a fight - a fight is not a workbench.
        if keys.just_pressed(KeyCode::Backspace) && !act.combat.active {
            let (msg, _) = pull_mod(&act.zone, &mut act.run, cursor.0, &mut act.rng);
            act.message.0 = msg;
            changed = true;
        }
        if keys.just_pressed(KeyCode::Enter) {
            // GDD §8: rummaging in a fight costs 4 AP, and may hand the turn over.
            if act.combat.active && act.combat.ap < combat::AP_ITEM {
                act.message.0 = "No AP left for that.".into();
            } else {
                let (msg, acted) = use_item(&act.zone, &mut act.run, cursor.0, &mut act.rng);
                act.message.0 = msg;
                cursor.0 = cursor.0.min(act.run.items.len().saturating_sub(1));
                if act.combat.active && acted {
                    act.combat.ap -= combat::AP_ITEM;
                    let theirs =
                        combat::end_turn(&mut act.combat, &mut act.run, &act.zone, &mut act.rng);
                    if let Some(theirs) = theirs {
                        act.message.0 = format!("{} {theirs}", act.message.0);
                    }
                }
            }
            changed = true;
        }
    }

    if changed {
        build_inventory_grid(&mut grid, &act.zone, &act.run, &act.clock, cursor.0, &act.message.0);
    }
}

// ---- trade ----

fn trade_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: ResMut<Cursor>,
    mut grid: ResMut<TileGrid>,
    mut act: Act,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        act.message.0.clear();
        next_state.set(GameState::Area);
        return;
    }

    let mut changed = false;
    if keys.just_pressed(KeyCode::ArrowLeft) && !act.trade_ui.buying {
        act.trade_ui.buying = true;
        cursor.0 = 0;
        changed = true;
    }
    if keys.just_pressed(KeyCode::ArrowRight) && act.trade_ui.buying {
        act.trade_ui.buying = false;
        cursor.0 = 0;
        changed = true;
    }

    let (vendor, buying) = (act.trade_ui.vendor.clone(), act.trade_ui.buying);
    let n = trade_list(&act.stock, &act.run, &vendor, buying).len();
    changed |= arrows(&keys, &mut cursor.0, n);
    if n > 0 {
        if keys.just_pressed(KeyCode::Enter) {
            act.message.0 = trade_one(
                &act.zone,
                &mut act.stock,
                &mut act.run,
                &vendor,
                buying,
                cursor.0,
            );
            let n = trade_list(&act.stock, &act.run, &vendor, buying).len();
            cursor.0 = cursor.0.min(n.saturating_sub(1));
            changed = true;
        }
    }

    if changed {
        build_trade_grid(
            &mut grid,
            &act.zone,
            &act.stock,
            &act.run,
            &act.clock,
            &vendor,
            buying,
            cursor.0,
            &act.message.0,
        );
    }
}

// ---- the end of a run ----

fn enter_gameover(
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    run: Res<RunState>,
) {
    let cause = run.death.clone().unwrap_or_default();
    build_gameover_grid(&mut grid, &zone, &run, &cause);
}

/// An ending is not the exit: it wraps back to the menu. The run itself is spent
/// (`meta::bank` deleted the suspend), so the menu has nothing to continue.
fn gameover_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut message: ResMut<MessageLine>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        message.0.clear();
        next_state.set(GameState::MainMenu);
    }
}

/// Up/down through a list of `n` entries, wrapping at both ends. Returns true if
/// the cursor moved, which is every screen's cue to redraw. An empty list never
/// moves, so no caller has to guard the modulo.
fn arrows(keys: &ButtonInput<KeyCode>, sel: &mut usize, n: usize) -> bool {
    if n == 0 {
        return false;
    }
    let step = match (
        keys.just_pressed(KeyCode::ArrowUp),
        keys.just_pressed(KeyCode::ArrowDown),
    ) {
        (true, false) => n - 1,
        (false, true) => 1,
        _ => return false,
    };
    *sel = (*sel + step) % n;
    true
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
    use crate::area::ItemKind;
    use crate::render::{GRID_H, GRID_W};
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

        /// Puts the main-menu cursor on `label` and confirms it.
        fn main_menu(&mut self, label: &str) -> &mut Self {
            assert_eq!(self.state(), GameState::MainMenu, "not on the menu:
{}", self.screen());
            self.highlight(label).press(KeyCode::Enter)
        }

        /// As `new`, but on a named save directory, so a test can watch a stalker
        /// die and then check what the next one inherits. Never the player's own.
        fn with_saves(tag: &str) -> Self {
            let saves = std::env::temp_dir().join(format!("the-zone-test-{tag}"));
            let _ = std::fs::remove_dir_all(&saves);
            let mut sim = Sim::reopen(saves);
            sim.main_menu("New Game");
            sim
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
            (0..GRID_H).map(|y| self.row(y) + "\n").collect()
        }

        /// Writes the current grid (char, colour, bold per cell) to `target/screens`
        /// for the release screenshot renderer. A dev tool, not a test - see
        /// `dump_screens_for_release`.
        fn dump(&self, name: &str) {
            let grid = self.app.world().resource::<TileGrid>();
            let mut out = String::new();
            for (i, cell) in grid.cells.iter().enumerate() {
                let (x, y) = (i % GRID_W, i / GRID_W);
                let c = cell.fg.to_srgba();
                let r = (c.red * 255.0).round() as u8;
                let g = (c.green * 255.0).round() as u8;
                let b = (c.blue * 255.0).round() as u8;
                out.push_str(&format!(
                    "{x} {y} {} {r} {g} {b} {}\n",
                    cell.ch as u32,
                    cell.bold as u8
                ));
            }
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/screens");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("{name}.grid")), out).unwrap();
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
            // A stalker who buys a gun buys rounds for it, so the harness does too
            // (GUNS §7.0). Fed through the game’s own reload, never by hand.
            self.app.world_mut().resource_scope(|world, mut run: Mut<RunState>| {
                let zone = world.resource::<ZoneData>();
                run.add_item(item, 1);
                run.weapon = run.items.iter().find(|s| s.id == item).map(|s| s.uid);
                run.add_item("vest", 1);
                run.armor = run.items.iter().find(|s| s.id == "vest").map(|s| s.uid);
                run.skills[Skill::SmallGuns.index()] = 90;
                run.skills[Skill::Melee.index()] = 90;
                if let ItemKind::Weapon { ammo: Some(caliber), .. } = zone.items[item].kind {
                    let round = zone.ammo_of(caliber).next().expect("a round that fits").clone();
                    run.add_item(&round, 40);
                    combat::reload(&mut run, zone);
                }
            });
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

        /// Puts the cursor on a list row without confirming it - Enter on the thing
        /// in your hands would stow it again.
        fn highlight(&mut self, name: &str) -> &mut Self {
            for _ in 0..26 {
                if (4..22).any(|y| self.row(y).starts_with(&format!("> {name}"))) {
                    return self;
                }
                self.press(KeyCode::ArrowDown);
            }
            panic!("no row `{name}` on:\n{}", self.screen());
        }

        /// Picks a row out of any list screen - inventory, board, map, dialogue -
        /// by the text it starts with, and confirms it.
        fn choose_listed(&mut self, name: &str) -> &mut Self {
            for _ in 0..26 {
                if (4..22).any(|y| self.row(y).starts_with(&format!("> {name}"))) {
                    return self.press(KeyCode::Enter);
                }
                self.press(KeyCode::ArrowDown);
            }
            panic!("no row `{name}` on:\n{}", self.screen());
        }

        /// Plays the fight out: swing until something gives. Panics rather than
        /// looping forever if combat will not resolve.
        fn fight(&mut self) -> &mut Self {
            for _ in 0..60 {
                if self.state() != GameState::Combat {
                    return self;
                }
                if (22..27).any(|y| self.row(y).contains("Attack (")) {
                    self.choose("Attack");
                } else {
                    self.choose("Flee");
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

    /// Release tool: drives the game to the screens worth showing on the store page
    /// and dumps each grid to `target/screens` for the PNG renderer. Ignored because
    /// it writes files instead of asserting.
    #[test]
    #[ignore = "release tool: dumps store-page screenshots"]
    fn dump_screens_for_release() {
        let saves = std::env::temp_dir().join("the-zone-test-dump");
        let _ = std::fs::remove_dir_all(&saves);
        let mut sim = Sim::reopen(saves); // on the main menu, before New Game
        sim.dump("01-main-menu");
        sim.main_menu("New Game");
        sim.roll_a_stalker();
        sim.dump("02-camp");
        sim.press(KeyCode::Tab);
        sim.dump("03-inventory");
        sim.press(KeyCode::Escape);
        sim.press(KeyCode::F2);
        sim.dump("04-map");
        sim.press(KeyCode::Escape);
        sim.press(KeyCode::F3);
        sim.dump("05-journal");
        sim.press(KeyCode::Escape);
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
        sim.assert_shows("A fire burns at the camp's heart");
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
        sim.arm_with("rifle");
        sim.fight();
        assert_eq!(sim.state(), GameState::Area);
        assert!(
            sim.run().count_of("flesh_eye") > 0,
            "a dead Flesh pays for the trip:\n{}",
            sim.screen()
        );
        sim.assert_shows("A quarry rim above black water");

        // The flag set back in the hatch is what lights S here.
        assert!(sim.run().is_revealed("quarry", 'S'));
        sim.press(KeyCode::KeyS);
        sim.assert_shows("Army Medkit");
        assert!(sim.run().count_of("army_medkit") > 0);
        // A crate is emptied once, not once a visit.
        sim.press(KeyCode::KeyS);
        sim.assert_shows("already had that");
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
    fn a_fight_is_fought_in_action_points() {
        let mut sim = Sim::new();
        sim.roll_a_stalker();
        sim.arm_with("pistol");
        sim.walk_to_the_rim();

        // A fight opens with a full turn of AP (GDD §4: 5 + AGI/2), already in reach.
        assert_eq!(sim.state(), GameState::Combat);
        let ap = sim.combat().ap;
        assert_eq!(ap, 5 + sim.run().attrs[run::AGI] / 2);
        sim.assert_shows("Flesh");
        sim.assert_shows("AP 7");
        assert!(!sim.shows("Close In"), "no distance to close");

        // Shooting costs 3 of those points, and the menu never offers movement.
        sim.choose("Attack");
        assert_eq!(sim.combat().ap, ap - combat::AP_ATTACK);

        // Two shots leave a single point, which is not enough to swing again but is
        // enough to top the magazine up: a good shot reloads for 1 AP (GUNS §3).
        sim.choose("Attack");
        assert_eq!(sim.combat().ap, 1);
        sim.assert_shows("Reload (1 AP)");

        // Spending the last of it hands the turn over, and the Flesh bites straight
        // away - there is no approach to burn its AP on.
        sim.choose("Reload");
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
    fn a_kill_leaves_blood_pooled_on_the_floor() {
        let mut sim = Sim::new();
        sim.roll_a_stalker();
        sim.arm_with("rifle");
        sim.walk_to_the_rim(); // the Flesh
        assert_eq!(sim.state(), GameState::Combat);
        sim.fight();
        assert_eq!(sim.state(), GameState::Area);

        // The Flesh carried 26 hit points; that much blood pools on the quarry floor.
        sim.assert_shows("A pool of blood");
    }

    #[test]
    fn the_bench_turns_parts_into_gear() {
        let mut sim = Sim::new();
        sim.roll_a_stalker();
        sim.press(KeyCode::KeyD); // into the hatch, where the bench lives
        sim.assert_shows("The bench");

        sim.run_mut().add_item("dog_tail", 1);
        sim.run_mut().add_item("vodka", 1);
        sim.run_mut().skills[Skill::Medicine.index()] = 100;

        sim.choose("The bench");
        assert_eq!(sim.state(), GameState::Crafting);
        sim.assert_shows("THE BENCH");
        sim.choose_listed("Brew Antirad");

        assert_eq!(sim.run().count_of("dog_tail"), 0, "the tail went in");
        assert!(sim.run().count_of("antirad") >= 1, "the brew came out");
    }

    #[test]
    fn the_forge_bakes_an_affix_into_held_gear_in_play() {
        let mut sim = Sim::new();
        sim.roll_a_stalker();
        sim.press(KeyCode::KeyD);

        {
            let mut run = sim.run_mut();
            run.add_item("vest", 1);
            run.add_item("gravi", 1);
            run.skills[Skill::Science.index()] = 100;
            run.armor = run.items.iter().find(|s| s.id == "vest").map(|s| s.uid);
        }

        sim.choose("The bench");
        assert_eq!(sim.state(), GameState::Crafting);
        sim.choose_listed("Cook a Whirligig");

        assert_eq!(sim.run().count_of("gravi"), 0, "the artifact went in");
        let vest = sim.run().items.iter().find(|s| s.id == "vest").unwrap();
        assert_eq!(vest.affixes.len(), 1, "one affix baked in, for good or ill");
    }

    #[test]
    fn a_fight_can_kill_you() {
        let mut sim = Sim::new();
        sim.roll_a_stalker();
        sim.walk_to_the_rim();
        assert_eq!(sim.state(), GameState::Combat);

        // Unarmed, at one hit point, against a Flesh. This ends one way.
        sim.run_mut().hp = 1;
        let name = sim.run().name.clone();
        for _ in 0..40 {
            if sim.state() != GameState::Combat {
                break;
            }
            sim.choose("Attack");
        }
        assert_eq!(sim.state(), GameState::GameOver);
        sim.assert_shows("THE ZONE IS STILL THERE");
        // Dying in a fight still writes the memorial entry, name and all (GDD §10).
        assert_eq!(sim.meta().memorial.len(), 1, "a combat death is still a death");
        assert_eq!(sim.meta().memorial[0].name, name);
        // And the note closes on the death, so it reads as the last log entry.
        assert!(
            sim.meta().memorial[0].note.ends_with("You die."),
            "the note should close on the death:\n{}",
            sim.meta().memorial[0].note
        );
    }

    #[test]
    fn a_marked_weapon_reads_out_and_actually_hits_harder() {
        use loot::{Effect, ItemStack, Rarity, Roll};

        let mut sim = Sim::with_saves("affix");
        sim.roll_a_stalker();

        // A shotgun the Zone has been at twice: one prefix, one suffix.
        {
            let mut run = sim.run_mut();
            let uid = run.next_uid();
            run.add_stack(ItemStack {
                affixes: vec![
                    Roll { affix: "heavy".into(), magnitude: 3 },
                    Roll { affix: "of_the_steady_hand".into(), magnitude: 10 },
                ],
                ..ItemStack::plain(uid, "shotgun", 1)
            });
            run.weapon = Some(uid);
        }

        // The pack shows the built name, the rarity, and what each roll does.
        sim.press(KeyCode::Tab);
        assert_eq!(sim.state(), GameState::Inventory);
        // The list column truncates the long built name; the full one is read out
        // below the list.
        sim.choose_listed("Heavy Pump Shotgun of the");
        sim.assert_shows("Heavy Pump Shotgun of the Steady Hand");
        sim.assert_shows("marked");
        sim.assert_shows("+3 damage");
        sim.assert_shows("+10 to hit");
        sim.press(KeyCode::Escape);

        let stack = sim.run().items.iter().find(|s| s.id == "shotgun").unwrap().clone();
        assert_eq!(stack.rarity(), Rarity::Marked);
        let zone = sim.app.world().resource::<ZoneData>();
        assert_eq!(loot::bonus(&stack, zone, Effect::Damage), 3);
        assert_eq!(loot::bonus(&stack, zone, Effect::ToHit), 10);
        // And it is worth more than the plain gun, because of what is on it.
        assert!(loot::value(&stack, zone) > zone.items["shotgun"].base);
    }

    #[test]
    fn what_is_worn_moves_the_numbers_it_says_it_does() {
        use loot::{ItemStack, Roll};

        let mut sim = Sim::with_saves("worn");
        sim.roll_a_stalker();

        let before_per = {
            let (run, zone) = (sim.run(), sim.app.world().resource::<ZoneData>());
            sim::attr(run, zone, run::PER)
        };

        // A sealed, clean suit: rad shielding on one roll, PER on the other.
        {
            let mut run = sim.run_mut();
            let uid = run.next_uid();
            run.add_stack(ItemStack {
                affixes: vec![
                    Roll { affix: "sealed".into(), magnitude: -6 },
                    Roll { affix: "clean".into(), magnitude: 2 },
                ],
                ..ItemStack::plain(uid, "vest", 1)
            });
            run.armor = Some(uid);
            // An artifact that would otherwise cost four rads an hour.
            run.add_item("gravi", 1);
        }

        let (run, zone) = (sim.run(), sim.app.world().resource::<ZoneData>());
        assert_eq!(sim::attr(run, zone, run::PER), before_per + 2, "worn PER counts");
        assert_eq!(
            sim::artifact_rads_per_hour(run, zone),
            0,
            "six points of shielding covers a four-rad artifact, and does not go below nought"
        );
        // Neither of those rolls is an armour roll, so damage resistance is still
        // exactly the vest: an effect only moves the number it names.
        assert_eq!(combat::armor_of(run, zone), 7);
    }

    #[test]
    fn the_deep_zone_is_walkable_and_the_fields_pay() {
        let mut sim = Sim::with_saves("deep");
        sim.roll_a_stalker();
        sim.arm_with("rifle");
        sim.run_mut().skills[Skill::StalkerLore.index()] = 95;

        // South out of the camp, through the reeds, past whatever lives in them.
        sim.choose("Travel");
        sim.choose("South, into the reeds");
        if sim.state() == GameState::Combat {
            sim.fight();
        }
        sim.assert_shows("Reed flats south of the road");

        // East into the acid, read it, and cross on the line you found.
        sim.choose("Wade east");
        sim.assert_shows("Low ground east of the reeds");
        // A plain scan opens the way; only a crit turns up the artifact with it
        // (GDD §6), so what is pinned here is the crossing, not the prize.
        let hp = sim.run().hp;
        sim.choose("Scan");
        sim.choose("Push Through");
        assert_eq!(sim.run().hp, hp, "a field you have read is crossed unharmed");
        if sim.state() == GameState::Combat {
            sim.fight();
        }
        sim.assert_shows("A camp of tents in the southern woods");

        // Freedom runs its own board, and it is not the Loners' board.
        sim.choose("Job board");
        sim.assert_shows("JOB BOARD - Freedom");
        sim.assert_shows("Eyes on the antenna");
        sim.press(KeyCode::Escape);

        // On to the junkyard, and the dead town beyond it.
        sim.choose("The junkyard road");
        if sim.state() == GameState::Combat {
            sim.fight();
        }
        sim.choose("The dead town road");
        sim.assert_shows("A street of empty houses");
        assert!(sim.run().discovered.len() >= 6, "the map is filling in");
    }

    #[test]
    fn the_main_chain_hands_out_one_step_at_a_time() {
        let mut sim = Sim::with_saves("chain");
        sim.roll_a_stalker();
        sim.choose("The bar");
        sim.choose("Job board");

        // Only the first link is on the board; the rest are behind it (GDD §9).
        sim.assert_shows("The way in, first: ask");
        assert!(!sim.shows("The way in, second"), "{}", sim.screen());
        sim.choose_listed("The way in, first: ask");
        assert!(sim.run().quests_taken.contains("way_1"));
        sim.press(KeyCode::Escape);

        // Settle it by standing in the dead town, and the next link opens.
        sim.run_mut().discovered.insert("dead_town".into());
        sim.choose("Outside"); // any action settles what is finished
        assert!(sim.run().quests_done.contains("way_1"), "{}", sim.screen());

        sim.choose("The bar");
        sim.choose("Job board");
        sim.assert_shows("The way in, second: pay");
    }

    #[test]
    fn the_hermit_takes_one_bottle_and_only_one() {
        let mut sim = Sim::with_saves("hermit");
        sim.roll_a_stalker();

        // Stand in front of the hermit with two bottles and the pay job taken.
        sim.run_mut().add_item("vodka", 2);
        sim.run_mut().quests_taken.insert("way_2".into());
        sim.app.world_mut().resource_mut::<CurrentArea>().0 = "dead_town".into();
        sim.press(KeyCode::ArrowDown); // redraw the new area before choosing from it

        sim.choose("Talk to the hermit");
        assert_eq!(sim.state(), GameState::Dialogue);
        sim.choose_listed("[carrying vodka] I brought something.");

        // One bottle leaves the pack, the job pays, and the line is spent.
        assert_eq!(sim.run().count_of("vodka"), 1, "{}", sim.screen());
        assert!(sim.run().flags.contains("paid_hermit"));
        assert!(sim.run().quests_done.contains("way_2"), "{}", sim.screen());
        sim.assert_shows("You hand over the Vodka");

        // Back on the greeting the line no longer pays: a second press spends nothing.
        sim.press(KeyCode::Escape);
        assert_eq!(sim.state(), GameState::Area);
        sim.choose("Talk to the hermit");
        sim.choose_listed("[carrying vodka] I brought something.");
        assert_eq!(sim.run().count_of("vodka"), 1, "a spent line does not spend twice");
        sim.assert_shows("not the one to make that argument");
    }

    #[test]
    fn lore_is_found_once_and_kept_in_the_journal() {
        let mut sim = Sim::with_saves("lore");
        sim.roll_a_stalker();
        sim.choose("Travel");

        assert!(sim.run().lore.is_empty());
        sim.choose("Read the ground");
        sim.assert_shows("You turn up something");
        assert!(sim.run().lore.contains("whirligigs"));

        // It reads back in the journal, alongside the work.
        sim.press(KeyCode::F3);
        assert_eq!(sim.state(), GameState::Journal);
        sim.assert_shows("[lore] Reading a whirligig");
        sim.assert_shows("The grass leans in");
        sim.press(KeyCode::Escape);

        // Found once: the row is spent, so it leaves the menu and frees the space.
        assert!(!sim.shows("Read the ground"), "{}", sim.screen());
        assert_eq!(sim.run().lore.len(), 1);
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
    fn an_ending_wraps_back_to_the_menu_and_offers_no_continue() {
        let mut sim = Sim::with_saves("ending-menu");
        sim.roll_a_stalker();
        sim.walk_to_the_room();
        sim.choose("Step into the light");
        sim.choose_listed("Make me whole");
        assert_eq!(sim.state(), GameState::GameOver);
        // The summary screen reads back what the run got done.
        sim.assert_shows("THE TALLY");
        sim.assert_shows("Things killed");
        sim.assert_shows("Places walked");

        // An ending is not the exit: Esc goes back to the menu, not to the desktop.
        sim.press(KeyCode::Escape);
        assert_eq!(sim.state(), GameState::MainMenu);
        sim.assert_shows("New Game");
        assert!(
            !sim.shows("Continue"),
            "the run is spent, so there is nothing to continue:\n{}",
            sim.screen()
        );

        // And the next run starts clean at the camp, not at the Room.
        sim.main_menu("New Game");
        sim.roll_a_stalker();
        assert_eq!(sim.state(), GameState::Area);
        sim.assert_shows("A fire burns at the camp's heart");
    }

    #[test]
    fn a_dead_stalker_leaves_something_for_the_next_one() {
        let saves = std::env::temp_dir().join("the-zone-test-permadeath");
        let _ = std::fs::remove_dir_all(&saves);

        let mut sim = Sim::reopen(saves.clone());
        sim.main_menu("New Game");
        sim.roll_a_stalker();
        let name = sim.run().name.clone();
        sim.run_mut().rep.insert("loners".into(), 80);
        sim.run_mut().rads = sim::RAD_DEATH;
        sim.choose("Travel");
        assert_eq!(sim.state(), GameState::GameOver);
        sim.assert_shows(&name);

        // Start the game again. The Zone remembers, at a quarter rate (GDD 10).
        let mut next = Sim::reopen(saves);
        next.main_menu("New Game");
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
    fn closing_the_window_puts_the_run_down_too() {
        let saves = std::env::temp_dir().join("the-zone-test-quit");
        let _ = std::fs::remove_dir_all(&saves);

        let mut sim = Sim::reopen(saves.clone());
        sim.main_menu("New Game");
        sim.roll_a_stalker();
        sim.choose("Travel");
        sim.run_mut().rubles = 777;
        // No F5, just the X on the window.
        sim.app.world_mut().write_message(WindowCloseRequested {
            window: Entity::PLACEHOLDER,
        });
        sim.app.update();

        let mut back = Sim::reopen(saves);
        back.main_menu("Continue");
        assert_eq!(back.state(), GameState::Area, "the run was still there");
        assert_eq!(back.run().rubles, 777);
    }

    #[test]
    fn the_main_menu_only_offers_a_run_there_is_something_to_pick_up() {
        let saves = std::env::temp_dir().join("the-zone-test-mainmenu");
        let _ = std::fs::remove_dir_all(&saves);

        // Nothing saved yet: New Game and Quit, and no false promise of a run.
        let mut sim = Sim::reopen(saves.clone());
        assert_eq!(sim.state(), GameState::MainMenu);
        sim.assert_shows("THE ZONE");
        sim.assert_shows("New Game");
        assert!(!sim.shows("Continue"), "no run to continue:
{}", sim.screen());
        sim.main_menu("New Game");
        assert_eq!(sim.state(), GameState::CharacterCreation);

        // Put one down, and the menu offers it back instead of starting over.
        sim.roll_a_stalker();
        sim.choose("Travel");
        sim.run_mut().rubles = 99;
        sim.press(KeyCode::F5);

        let mut back = Sim::reopen(saves);
        back.assert_shows("Continue");
        back.main_menu("New Game"); // and taking a new one leaves it alone
        assert_eq!(back.state(), GameState::CharacterCreation);
    }

    #[test]
    fn suspending_puts_the_run_down_and_picking_it_up_spends_the_file() {
        let saves = std::env::temp_dir().join("the-zone-test-suspend");
        let _ = std::fs::remove_dir_all(&saves);

        let mut sim = Sim::reopen(saves.clone());
        sim.main_menu("New Game");
        sim.roll_a_stalker();
        sim.choose("Travel"); // out to the road, so there is something to restore
        sim.run_mut().rubles = 1234;
        let name = sim.run().name.clone();
        let minutes = sim.run().minutes;
        sim.press(KeyCode::F5);

        // Starting again drops you straight back where you stood.
        let mut back = Sim::reopen(saves.clone());
        back.main_menu("Continue");
        assert_eq!(back.state(), GameState::Area);
        back.assert_shows("A dirt road between the camp and the wastes");
        assert_eq!(back.run().rubles, 1234);
        assert_eq!(back.run().name, name);
        assert_eq!(back.run().minutes, minutes);
        assert!(back.run().discovered.contains("road"));

        // The file is spent. There is no reload (GDD 10).
        // No suspend file, so the menu does not offer to pick anything up.
        let mut again = Sim::reopen(saves);
        assert!(!again.shows("Continue"), "nothing left to continue:
{}", again.screen());
        again.main_menu("New Game");
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

    /// GUNS §7.5: buy a gun and rounds for it, fit a sight, and take it to the rim.
    /// Everything asserted here is what the player can see on the grid.
    #[test]
    fn a_gun_is_bought_fed_fitted_and_run_dry() {
        let mut sim = Sim::new();
        sim.roll_a_stalker();

        // Off the trader's shelf: the pistol, and ten rounds in one press.
        sim.run_mut().rubles = 4000; // a pistol is 1238 at Barter 25
        sim.choose("Talk to Sidorovich");
        sim.choose_listed("Show me the shelf.");
        assert_eq!(sim.state(), GameState::Trade);
        sim.choose_listed("PMm Pistol");
        sim.choose_listed("9mm Ball");
        sim.assert_shows("You buy 10 of the 9mm Ball");
        assert_eq!(sim.run().count_of("pistol_round"), 10);
        sim.press(KeyCode::Escape);

        // Osip sells the sight; this stalker already has one in the pack.
        sim.run_mut().add_item("scope", 1);

        // Ready the pistol, feed it, and fit the sight to it - all through the pack.
        sim.press(KeyCode::Tab);
        assert_eq!(sim.state(), GameState::Inventory);
        sim.choose_listed("PMm Pistol");
        sim.assert_shows("You ready the PMm Pistol");
        sim.choose_listed("9mm Ball");
        sim.assert_shows("You feed it 8 of the 9mm Ball");
        sim.choose_listed("Telescopic Sight");
        sim.assert_shows("You fit the Telescopic Sight to the PMm Pistol");

        // The pack reads out what is on it and what is in it.
        sim.highlight("PMm Pistol");
        sim.assert_shows("+10 to hit  (Telescopic Sight)");
        sim.assert_shows("Loaded  8/8  9mm Ball");
        sim.press(KeyCode::Tab);

        // Take it to the Flesh on the rim. The fight says what is left in it.
        sim.run_mut().skills[Skill::SmallGuns.index()] = 30; // misses, so it runs dry
        sim.walk_to_the_rim();
        assert_eq!(sim.state(), GameState::Combat);
        sim.assert_shows("AMMO 8/8");

        // Eight shots later there is nothing to fire and the menu says so.
        for _ in 0..40 {
            if sim.state() != GameState::Combat || !sim.shows("Attack (") {
                break;
            }
            sim.choose("Attack");
        }
        if sim.state() == GameState::Combat {
            sim.assert_shows("AMMO 0/8");
            assert!(!sim.shows("Attack ("), "an empty gun offers no attack");
            sim.assert_shows("Reload (2 AP)");

            // Feeding it puts the attack back on the menu.
            sim.choose("Reload");
            sim.assert_shows("AMMO 2/8");
            sim.assert_shows("Attack (3 AP)");
        }
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

        sim.choose("Talk to Sidorovich");
        assert_eq!(sim.state(), GameState::Dialogue);
        sim.choose_listed("Show me the shelf.");
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
    fn wielding_reads_out_instead_of_running_under_the_panel() {
        let mut sim = Sim::new();
        sim.roll_a_stalker();
        sim.arm_with("knife");
        sim.press(KeyCode::Tab);
        assert_eq!(sim.state(), GameState::Inventory);
        // The "[wielded]" marker used to run under the right-hand panel and get cut
        // to "[wiel" - the full marker must sit in the list column.
        sim.assert_shows("[wielded]");
        sim.assert_gutter_clear();
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
