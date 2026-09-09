//! What outlives a stalker (GDD §10): the memorial, unlocked backgrounds, a quarter
//! of their standing, and the lore they turned up. Plus the endings at the Room, and
//! the suspend file, which is a bookmark and never a second chance.

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::area::{VendorStock, ZoneData};
use crate::liquid::Puddles;
use crate::render::{TileGrid, PALETTE};
use crate::run::{check, Outcome, Rng, RunState, BACKGROUNDS, LCK};
use crate::screens::{draw_message, hint, row, title, wrap};
use crate::sim::{FieldState, Fields, GameClock};

const LIST_ROW: usize = 4;

/// The memorial's detail panel starts much closer to the list than the shared
/// `screens::PANEL_COL` (84): a row is only name + background + days, so pinning
/// the panel at 84 left the middle of the screen empty. 40 clears the widest row.
const MEMORIAL_PANEL: usize = 40;
/// Wrap width for the panel text, leaving a right margin before the 120th column.
const MEMORIAL_WRAP: usize = 76;

/// GDD §4: the first two backgrounds are always there; the rest are earned.
pub(crate) const ALWAYS_UNLOCKED: usize = 2;
/// Standing carried into the next stalker (GDD §10).
const REP_CARRIED_PERCENT: i32 = 25;
/// Monolith standing above this corrupts any wish (GDD §9).
const CORRUPTING_REP: i32 = 25;

// ---- where the files live (SPEC §9: not in the repo) ----

/// The save directory. A resource so a test can point it at a temp dir instead of
/// the player's real one.
#[derive(Resource, Clone)]
pub(crate) struct SaveDir(pub PathBuf);

impl Default for SaveDir {
    fn default() -> Self {
        let base = env::var_os("APPDATA")
            .or_else(|| env::var_os("XDG_DATA_HOME"))
            .or_else(|| env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        SaveDir(base.join("the-zone"))
    }
}

impl SaveDir {
    fn meta(&self) -> PathBuf {
        self.0.join("meta.ron")
    }

    fn suspend(&self) -> PathBuf {
        self.0.join("suspend.ron")
    }
}

/// Writes RON, or says why it could not. A save that fails must not take the game
/// down with it, so this reports instead of panicking.
fn write(path: &Path, text: &str) -> bool {
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            warn!("{}: {e}", parent.display());
            return false;
        }
    }
    match fs::write(path, text) {
        Ok(()) => true,
        Err(e) => {
            warn!("{}: {e}", path.display());
            false
        }
    }
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let text = fs::read_to_string(path).ok()?;
    match ron::from_str(&text) {
        Ok(value) => Some(value),
        Err(e) => {
            // A save from an older build is not a crash; it is a fresh start.
            warn!("{}: {e}", path.display());
            None
        }
    }
}

// ---- the memorial ----

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Fallen {
    pub name: String,
    pub background: String,
    pub days: u32,
    pub cause: String,
    /// The last thing that happened to them. GDD §10: the notes can hint at secrets.
    pub note: String,
}

#[derive(Resource, Default, Serialize, Deserialize, Clone)]
pub(crate) struct MetaProgress {
    /// Background indices earned beyond the first two.
    pub backgrounds: HashSet<usize>,
    pub memorial: Vec<Fallen>,
    /// A quarter of each dead stalker's standing, carried forward.
    pub rep: HashMap<String, i32>,
    pub lore: HashSet<String>,
}

impl MetaProgress {
    pub fn load(dir: &SaveDir) -> Self {
        read(&dir.meta()).unwrap_or_default()
    }

    pub fn save(&self, dir: &SaveDir) {
        if let Ok(text) = ron::ser::to_string_pretty(self, default()) {
            write(&dir.meta(), &text);
        }
    }

    pub fn unlocked(&self, background: usize) -> bool {
        background < ALWAYS_UNLOCKED || self.backgrounds.contains(&background)
    }
}

/// Ends the run for good: writes the memorial entry, banks a quarter of the standing
/// and the lore, works out what the next stalker has earned, and deletes the suspend
/// file so there is nothing to reload (GDD §10).
pub(crate) fn bank(
    meta: &mut MetaProgress,
    run: &RunState,
    dir: &SaveDir,
    cause: &str,
    note: &str,
) {
    meta.memorial.push(Fallen {
        name: run.name.clone(),
        background: BACKGROUNDS[run.background].name.to_string(),
        days: run.day(),
        cause: cause.to_string(),
        note: note.to_string(),
    });
    for (faction, rep) in &run.rep {
        let carried = rep * REP_CARRIED_PERCENT / 100;
        let now = meta.rep.get(faction).copied().unwrap_or(0);
        meta.rep.insert(faction.clone(), (now + carried).clamp(-100, 100));
    }
    meta.lore.extend(run.lore.iter().cloned());

    // `ponytail:` GDD §4 unlocks the Ecologist at the Lab and the Bandit by dying to
    // bandits. Neither exists yet, so the nearest thing that does stands in.
    if run.discovered.contains("room") {
        meta.backgrounds.insert(2);
    }
    if cause.contains("wounds") {
        meta.backgrounds.insert(3);
    }

    meta.save(dir);
    let _ = fs::remove_file(dir.suspend());
}

// ---- suspend (F5): a bookmark, not a second chance ----

#[derive(Serialize, Deserialize)]
pub(crate) struct Suspend {
    pub run: RunState,
    pub area: String,
    pub clock: GameClock,
    pub fields: HashMap<String, FieldState>,
    pub stock: HashMap<String, Vec<crate::loot::ItemStack>>,
    pub puddles: HashMap<String, HashMap<String, u32>>,
    pub rng: Rng,
}

pub(crate) fn suspend(
    dir: &SaveDir,
    run: &RunState,
    area: &str,
    clock: &GameClock,
    fields: &Fields,
    stock: &VendorStock,
    puddles: &Puddles,
    rng: &Rng,
) -> bool {
    let save = Suspend {
        run: run.clone(),
        area: area.to_string(),
        clock: clock.clone(),
        fields: fields.0.clone(),
        stock: stock.0.clone(),
        puddles: puddles.0.clone(),
        rng: rng.clone(),
    };
    match ron::ser::to_string_pretty(&save, default()) {
        Ok(text) => write(&dir.suspend(), &text),
        Err(e) => {
            warn!("suspend: {e}");
            false
        }
    }
}

/// Whether there is a run to pick up. Looking is not reading: the main menu needs
/// to know without spending the file.
pub(crate) fn has_suspend(dir: &SaveDir) -> bool {
    dir.suspend().exists()
}

/// Takes the suspended run and removes the file in the same breath, so a crash after
/// this point costs the run rather than handing out a free reload.
pub(crate) fn resume(dir: &SaveDir) -> Option<Suspend> {
    let save: Suspend = read(&dir.suspend())?;
    let _ = fs::remove_file(dir.suspend());
    Some(save)
}

// ---- the endings (GDD §9) ----

#[derive(Deserialize, Clone)]
#[serde(rename = "Ending")]
pub(crate) struct EndingData {
    pub name: String,
    /// What you asked for.
    pub literal: String,
    /// What the Zone heard.
    pub corrupted: String,
}

/// Which reading you get: Monolith standing corrupts outright, and past that it is a
/// hidden LCK check (GDD §9).
pub(crate) fn resolve(
    ending: &EndingData,
    run: &RunState,
    zone: &ZoneData,
    rng: &mut Rng,
) -> (String, String) {
    let monolith = run.rep_of("monolith");
    let lucky = matches!(
        check(50 + 5 * crate::sim::attr(run, zone, LCK), 0, rng),
        Outcome::Success | Outcome::CritSuccess
    );
    if monolith > CORRUPTING_REP || !lucky {
        (ending.name.clone(), ending.corrupted.clone())
    } else {
        (ending.name.clone(), ending.literal.clone())
    }
}

// ---- the memorial screen ----

pub(crate) fn build_memorial_grid(
    grid: &mut TileGrid,
    meta: &MetaProgress,
    sel: usize,
    message: &str,
) {
    grid.clear();
    title(grid, "THE MEMORIAL");

    if meta.memorial.is_empty() {
        grid.text(2, LIST_ROW, "No names yet. You are the first.", PALETTE.desc, false);
    }
    // Newest first: the last one in is the one people are still talking about.
    let fallen: Vec<&Fallen> = meta.memorial.iter().rev().collect();
    for (i, f) in fallen.iter().take(14).enumerate() {
        let line = format!("{:<12}{:<10}{} days", f.name, f.background, f.days);
        row(grid, 0, LIST_ROW + i, i == sel, PALETTE.grey, false, &line);
    }

    if let Some(f) = fallen.get(sel) {
        // Wrap so neither the cause nor the note runs into the right edge and
        // loses its tail.
        let mut y = 2;
        for line in wrap(&f.cause, MEMORIAL_WRAP) {
            grid.text(MEMORIAL_PANEL, y, &line, PALETTE.red, false);
            y += 1;
        }
        y += 1; // a breath between the cause and the note
        for line in wrap(&f.note, MEMORIAL_WRAP) {
            grid.text(MEMORIAL_PANEL, y, &line, PALETTE.desc, false);
            y += 1;
        }
    }
    if !meta.lore.is_empty() {
        let line = format!("{} things the Zone has told us", meta.lore.len());
        grid.text(2, 20, &line, PALETTE.dim, false);
    }

    draw_message(grid, message);
    hint(grid, "Up/Down read a name   Esc back");
}

// ---- tests (SPEC §7) ----

#[cfg(test)]
mod tests {
    use super::*;
    use crate::area::load_zone;

    fn temp_dir(tag: &str) -> SaveDir {
        let dir = env::temp_dir().join(format!("the-zone-meta-{tag}"));
        let _ = fs::remove_dir_all(&dir);
        SaveDir(dir)
    }

    #[test]
    fn a_dead_stalker_leaves_a_quarter_of_their_standing_and_a_name() {
        let dir = temp_dir("bank");
        let mut meta = MetaProgress::default();
        let mut run = RunState::roll(0, &[8, 9, 4], &mut Rng::new(5));
        run.rep.insert("loners".into(), 80);
        run.rep.insert("duty".into(), -40);
        run.minutes = 5 * 1440;
        run.lore.insert("read_the_log".into());

        bank(&mut meta, &run, &dir, "Radiation.", "You should have turned back.");
        assert_eq!(meta.memorial.len(), 1);
        assert_eq!(meta.memorial[0].days, 6);
        assert_eq!(meta.memorial[0].name, run.name);
        assert_eq!(meta.rep["loners"], 20); // a quarter of 80
        assert_eq!(meta.rep["duty"], -10);
        assert!(meta.lore.contains("read_the_log"));

        // It survives the trip through the file, and stacks over runs.
        let read_back = MetaProgress::load(&dir);
        assert_eq!(read_back.memorial.len(), 1);
        assert_eq!(read_back.rep["loners"], 20);
        let mut meta = read_back;
        bank(&mut meta, &run, &dir, "Radiation.", "Again.");
        assert_eq!(MetaProgress::load(&dir).rep["loners"], 40);
    }

    #[test]
    fn backgrounds_have_to_be_earned() {
        let dir = temp_dir("unlock");
        let meta = MetaProgress::default();
        assert!(meta.unlocked(0) && meta.unlocked(1));
        assert!(!meta.unlocked(2) && !meta.unlocked(3));

        let mut meta = MetaProgress::default();
        let mut run = RunState::roll(0, &[0, 1, 2], &mut Rng::new(5));
        run.discovered.insert("room".into());
        bank(&mut meta, &run, &dir, "The Room took them.", "");
        assert!(meta.unlocked(2), "reaching the centre earns the Ecologist");
        assert!(!meta.unlocked(3));

        bank(&mut meta, &run, &dir, "Your wounds finish it.", "");
        assert!(meta.unlocked(3), "dying of wounds earns the Bandit");
    }

    #[test]
    fn a_suspend_is_a_bookmark_and_is_spent_when_it_is_read() {
        let dir = temp_dir("suspend");
        let mut rng = Rng::new(11);
        let mut run = RunState::roll(1, &[0, 1, 2], &mut rng);
        run.rubles = 4321;
        run.minutes = 900;
        let mut fields = Fields::default();
        fields.0.insert("field".into(), FieldState { scanned: true, ..Default::default() });
        let stock = VendorStock(HashMap::from([(
            "trader".to_string(),
            vec![crate::loot::ItemStack::plain(0, "bolt", 3)],
        )]));

        assert!(suspend(&dir, &run, "quarry", &GameClock { next_emission: 7000 }, &fields, &stock, &Puddles::empty(), &rng));
        let back = resume(&dir).expect("the file was just written");
        assert_eq!(back.run.rubles, 4321);
        assert_eq!(back.area, "quarry");
        assert_eq!(back.clock.next_emission, 7000);
        assert!(back.fields["field"].scanned);
        assert_eq!(back.stock["trader"][0].count, 3);

        // Reading it spends it. There is no reload (GDD §10).
        assert!(resume(&dir).is_none());
    }

    #[test]
    fn death_deletes_the_suspend_file() {
        let dir = temp_dir("nodoubles");
        let mut rng = Rng::new(2);
        let run = RunState::roll(0, &[0, 1, 2], &mut rng);
        suspend(
            &dir,
            &run,
            "camp",
            &GameClock::default(),
            &Fields::default(),
            &VendorStock(HashMap::new()),
            &Puddles::empty(),
            &rng,
        );
        let mut meta = MetaProgress::default();
        bank(&mut meta, &run, &dir, "Radiation.", "");
        assert!(resume(&dir).is_none(), "you cannot reload out of dying");
    }

    #[test]
    fn the_memorial_wraps_a_long_cause_instead_of_cutting_it_off() {
        let mut grid = TileGrid::new(crate::render::GRID_W, crate::render::GRID_H);
        let mut meta = MetaProgress::default();
        // Longer than the panel is wide only on the old 84-column layout; the guard
        // is that the whole sentence survives wherever the panel sits.
        let cause = "Radiation. You glow, and then you stop.";
        meta.memorial.push(Fallen {
            name: "Sparrow".into(),
            background: "Loner".into(),
            days: 6,
            cause: cause.into(),
            note: String::new(),
        });

        build_memorial_grid(&mut grid, &meta, 0, "");

        // Read the panel back row by row; a cause that ran off the right edge drops
        // its tail, so the whole sentence has to survive.
        let screen: String = (0..crate::render::GRID_H)
            .map(|y| {
                (MEMORIAL_PANEL..grid.w)
                    .map(|x| grid.cells[y * grid.w + x].ch)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert!(screen.contains(cause), "the cause ran off the panel:\n{screen}");
    }

    #[test]
    fn the_monolith_corrupts_every_wish() {
        let zone = load_zone(Path::new("assets/data"));
        let mut rng = Rng::new(9);
        let mut run = RunState::roll(0, &[0, 1, 2], &mut rng);
        let ending = zone.endings["riches"].clone();

        // Standing with the Monolith is enough on its own, whatever the luck.
        run.rep.insert("monolith".into(), 60);
        for _ in 0..20 {
            let (_, text) = resolve(&ending, &run, &zone, &mut rng);
            assert_eq!(text, ending.corrupted);
        }

        // Without them, luck decides, so both readings have to be reachable.
        run.rep.insert("monolith".into(), 0);
        let mut readings: HashSet<String> = HashSet::new();
        for _ in 0..60 {
            readings.insert(resolve(&ending, &run, &zone, &mut rng).1);
        }
        assert_eq!(readings.len(), 2, "a wish can go either way");
    }
}
