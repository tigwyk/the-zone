//! The Zone — M3 (the Zone breathes).

mod area;
mod render;
mod run;
mod screens;
mod sim;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};

use area::{build_area_grid, Action, VendorStock, ZoneData};
use render::{render_grid, TileGrid};
use run::{Rng, RunState, BACKGROUNDS, REST_COST, REST_RADS, SKILL_NAMES, TAG_COUNT};
use screens::{
    build_creation_grid, build_gameover_grid, build_inventory_grid, build_trade_grid, trade_list,
    trade_one, use_item,
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
        .init_resource::<Creation>()
        .init_resource::<TileGrid>()
        .init_state::<GameState>()
        .add_systems(Startup, enter_creation)
        .add_systems(OnEnter(GameState::CharacterCreation), enter_creation)
        .add_systems(OnEnter(GameState::Area), redraw_area)
        .add_systems(OnEnter(GameState::Inventory), enter_inventory)
        .add_systems(OnEnter(GameState::Trade), enter_trade)
        .add_systems(OnEnter(GameState::GameOver), enter_gameover)
        .add_systems(
            Update,
            (
                creation_input.run_if(in_state(GameState::CharacterCreation)),
                menu_input.run_if(in_state(GameState::Area)),
                inventory_input.run_if(in_state(GameState::Inventory)),
                trade_input.run_if(in_state(GameState::Trade)),
                gameover_input.run_if(in_state(GameState::GameOver)),
            )
                .in_set(GameInput),
        )
}

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "The Zone — M3".into(),
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

fn enter_creation(mut grid: ResMut<TileGrid>, c: Res<Creation>) {
    build_creation_grid(&mut grid, c.phase, c.sel, c.background, &c.tags);
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
                clock.schedule(&run, &mut rng);
                reveal_secrets(&area.0, &mut run, &zone, &mut rng);
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
    zone: Res<'w, ZoneData>,
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

    if check_death(&mut act.run) {
        next_state.set(GameState::GameOver);
    }
    moved.is_some()
}

// ---- inventory ----

fn inventory_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: ResMut<Cursor>,
    mut run: ResMut<RunState>,
    mut message: ResMut<MessageLine>,
    mut grid: ResMut<TileGrid>,
    zone: Res<ZoneData>,
    clock: Res<GameClock>,
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
    }

    impl Sim {
        /// A fresh app on a fixed seed, wound forward to the creation screen.
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, StatesPlugin));
            app.init_resource::<ButtonInput<KeyCode>>();
            add_game(&mut app);
            app.insert_resource(Rng::new(20260905));
            app.update();
            Sim { app }
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
                if (22..27).any(|y| self.row(y) == wanted) {
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
        sim.assert_shows("A quarry rim above black water");
        assert_eq!(sim.run().hp, hp, "a scanned field is crossed unharmed");

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
        sim.choose("Look");
        assert_eq!(sim.state(), GameState::GameOver);
        sim.assert_shows("THE ZONE IS STILL THERE");
        sim.assert_shows("Radiation");
    }
}
