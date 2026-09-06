//! Jobs: the board you take them from, the journal you track them in, and the
//! standing they move (GDD §9).

use bevy::prelude::*;
use serde::Deserialize;

use crate::area::ZoneData;
use crate::render::{TileGrid, PALETTE};
use crate::run::RunState;
use crate::screens::{draw_chrome, draw_message, hint, more, title, window, wrap};
use crate::sim::GameClock;

const LIST_ROW: usize = 4;

/// GDD §9 names five shapes. These three ride on state the run already keeps.
/// `ponytail:` escort and deliver need followers and NPC-to-NPC routes; they are
/// content-blocked, not design-blocked.
#[derive(Deserialize, Clone)]
pub(crate) enum Goal {
    /// Carry the item back. Handing it over spends it.
    Have(String),
    Reach(String),
    Kill(String),
}

#[derive(Deserialize, Clone)]
#[serde(rename = "Quest")]
pub(crate) struct QuestData {
    pub name: String,
    pub faction: String,
    pub text: String,
    pub goal: Goal,
    pub rubles: u32,
    pub rep: i32,
    /// The job that has to be settled first. This is what makes a chain a chain
    /// (GDD §9: the main quest is five jobs that end with a route to the centre).
    #[serde(default)]
    pub requires: Option<String>,
}

/// Which board is open, and where the cursor is on it.
#[derive(Resource, Default)]
pub(crate) struct Board {
    pub faction: String,
    pub sel: usize,
}

/// Rep move, with the rival spill from GDD §9: helping one side moves the ones it
/// hates the other way, at half rate.
pub(crate) fn adjust_rep(run: &mut RunState, zone: &ZoneData, faction: &str, delta: i32) {
    let now = run.rep_of(faction);
    run.rep.insert(faction.to_string(), (now + delta).clamp(-100, 100));
    let Some(data) = zone.factions.get(faction) else {
        return;
    };
    for rival in &data.rivals {
        let now = run.rep_of(rival);
        run.rep.insert(rival.clone(), (now - delta / 2).clamp(-100, 100));
    }
}

/// The jobs a faction is offering: not taken, not already done.
pub(crate) fn offered<'a>(zone: &'a ZoneData, run: &RunState, faction: &str) -> Vec<&'a str> {
    let mut ids: Vec<&str> = zone
        .quests
        .iter()
        .filter(|(id, q)| {
            q.faction == faction
                && !run.quests_taken.contains(*id)
                && !run.quests_done.contains(*id)
                && q.requires.as_ref().is_none_or(|r| run.quests_done.contains(r))
        })
        .map(|(id, _)| id.as_str())
        .collect();
    ids.sort_unstable(); // HashMap order is not an order
    ids
}

pub(crate) fn taken<'a>(zone: &'a ZoneData, run: &RunState) -> Vec<&'a str> {
    let mut ids: Vec<&str> = zone
        .quests
        .keys()
        .filter(|id| run.quests_taken.contains(*id))
        .map(|id| id.as_str())
        .collect();
    ids.sort_unstable();
    ids
}

fn done(goal: &Goal, run: &RunState) -> bool {
    match goal {
        Goal::Have(item) => run.items.iter().any(|(id, n)| id == item && *n > 0),
        Goal::Reach(area) => run.discovered.contains(area),
        Goal::Kill(enemy) => run.kills.contains(enemy),
    }
}

/// Pays out every taken job whose goal is now met. Called after each action, so a
/// job settles the moment you satisfy it rather than needing a walk back.
/// `ponytail:` no hand-in step; add one when an NPC needs to react to it.
pub(crate) fn settle(run: &mut RunState, zone: &ZoneData) -> String {
    let ready: Vec<String> = taken(zone, run)
        .into_iter()
        .filter(|id| done(&zone.quests[*id].goal, run))
        .map(|id| id.to_string())
        .collect();

    let mut said = Vec::new();
    for id in ready {
        let quest = zone.quests[&id].clone();
        if let Goal::Have(item) = &quest.goal {
            run.take_item(item, 1); // handed over
        }
        run.quests_taken.remove(&id);
        run.quests_done.insert(id);
        run.rubles += quest.rubles;
        adjust_rep(run, zone, &quest.faction, quest.rep);
        said.push(format!("Job done: {}. {} RU.", quest.name, quest.rubles));
    }
    said.join(" ")
}

// ---- the screens ----

/// How many rows a job or lore list gets before it scrolls.
const ROWS: usize = 12;

/// Shared list renderer: a label a row, and the highlighted one explains itself
/// underneath. Used for job boards and for the journal, which mixes jobs and lore.
fn list(grid: &mut TileGrid, entries: &[(String, String)], sel: usize, empty: &str) {
    if entries.is_empty() {
        grid.text(2, LIST_ROW, empty, PALETTE.desc, false);
        return;
    }
    let shown = window(sel, entries.len(), ROWS);
    for (line_no, i) in shown.clone().enumerate() {
        let fg = if i == sel { PALETTE.menu_sel } else { PALETTE.menu };
        grid.text(0, LIST_ROW + line_no, if i == sel { "> " } else { "  " }, fg, false);
        grid.text(2, LIST_ROW + line_no, &entries[i].0, fg, false);
    }
    more(grid, LIST_ROW + ROWS, &shown, entries.len());

    let text = &entries[sel.min(entries.len() - 1)].1;
    for (i, line) in wrap(text, 76).iter().enumerate() {
        grid.text(2, LIST_ROW + ROWS + 2 + i, line, PALETTE.desc, false);
    }
}

fn job_row(zone: &ZoneData, id: &str) -> (String, String) {
    let quest = &zone.quests[id];
    (
        format!("{:<32}{:>6} RU   {}", quest.name, quest.rubles, quest.faction),
        quest.text.clone(),
    )
}

/// Everything the journal lists: the work you are carrying, then the things the Zone
/// has told you. One list, so one cursor reads both.
pub(crate) fn journal_entries(zone: &ZoneData, run: &RunState) -> Vec<(String, String)> {
    let mut entries: Vec<(String, String)> =
        taken(zone, run).iter().map(|id| job_row(zone, id)).collect();
    let mut lore: Vec<&String> = run.lore.iter().collect();
    lore.sort_unstable();
    for id in lore {
        if let Some(entry) = zone.lore.get(id) {
            entries.push((format!("[lore] {}", entry.title), entry.text.clone()));
        }
    }
    entries
}

pub(crate) fn build_board_grid(
    grid: &mut TileGrid,
    zone: &ZoneData,
    run: &RunState,
    clock: &GameClock,
    board: &Board,
    message: &str,
) {
    grid.clear();
    let name = zone
        .factions
        .get(&board.faction)
        .map_or(board.faction.as_str(), |f| f.name.as_str());
    title(grid, &format!("JOB BOARD - {name}"));
    grid.text(
        crate::screens::PANEL_COL,
        0,
        &format!("standing {:+}", run.rep_of(&board.faction)),
        PALETTE.status,
        false,
    );

    let ids = offered(zone, run, &board.faction);
    let entries: Vec<(String, String)> = ids.iter().map(|id| job_row(zone, id)).collect();
    list(grid, &entries, board.sel, "Nothing on the board today.");
    draw_message(grid, message);
    hint(grid, "Up/Down choose   Enter take the job   Esc leave the board");
    draw_chrome(grid, run, clock);
}

pub(crate) fn build_journal_grid(
    grid: &mut TileGrid,
    zone: &ZoneData,
    run: &RunState,
    clock: &GameClock,
    sel: usize,
    message: &str,
) {
    grid.clear();
    title(grid, "JOURNAL");

    let entries = journal_entries(zone, run);
    list(grid, &entries, sel, "Nothing carried, and nothing learned yet.");

    // Standing, which is the other half of what a journal is for.
    let mut y = 2;
    let mut factions: Vec<(&String, &i32)> = run.rep.iter().collect();
    factions.sort_unstable();
    for (id, rep) in factions {
        let name = zone.factions.get(id).map_or(id.as_str(), |f| f.name.as_str());
        grid.text(
            crate::screens::PANEL_COL,
            y,
            &format!("{name:<14}{rep:+}"),
            PALETTE.status,
            false,
        );
        y += 1;
    }
    let tally = format!(
        "{} settled   {} lore",
        run.quests_done.len(),
        run.lore.len()
    );
    grid.text(crate::screens::PANEL_COL, y + 1, &tally, PALETTE.desc, false);

    draw_message(grid, message);
    hint(grid, "Up/Down read   Esc back");
    draw_chrome(grid, run, clock);
}

// ---- tests (SPEC §7) ----

#[cfg(test)]
mod tests {
    use super::*;
    use crate::area::load_zone;
    use std::path::Path;

    fn fixture() -> (ZoneData, RunState) {
        let zone = load_zone(Path::new("assets/data"));
        (zone, RunState::roll(0, &[8, 9, 4], &mut crate::run::Rng::new(3)))
    }

    #[test]
    fn helping_one_faction_costs_you_with_its_rivals() {
        let (zone, mut run) = fixture();
        // Loners hate bandits, so Loner work makes enemies at half the rate.
        adjust_rep(&mut run, &zone, "loners", 20);
        assert_eq!(run.rep_of("loners"), 40); // a Loner starts at 20
        assert_eq!(run.rep_of("bandits"), -10);

        // Duty hates two factions at once, and standing never leaves the range.
        adjust_rep(&mut run, &zone, "duty", 40);
        assert_eq!(run.rep_of("freedom"), -20);
        assert_eq!(run.rep_of("monolith"), -20);
        for _ in 0..20 {
            adjust_rep(&mut run, &zone, "duty", 40);
        }
        assert_eq!(run.rep_of("duty"), 100);
        assert_eq!(run.rep_of("freedom"), -100);
    }

    #[test]
    fn a_job_settles_when_its_goal_is_met_and_pays_once() {
        let (zone, mut run) = fixture();
        run.quests_taken.insert("grishas_pot".into());
        let purse = run.rubles;

        assert!(settle(&mut run, &zone).is_empty(), "no eye, no pay");

        run.add_item("flesh_eye", 1);
        let msg = settle(&mut run, &zone);
        assert!(msg.contains("An eye for the pot"), "{msg}");
        assert_eq!(run.rubles, purse + zone.quests["grishas_pot"].rubles);
        assert!(!run.items.iter().any(|(i, _)| i == "flesh_eye"), "you handed it over");
        assert!(run.quests_done.contains("grishas_pot"));
        assert_eq!(run.rep_of("loners"), 30);

        // Settling again pays nothing, even with another eye in the pack.
        run.add_item("flesh_eye", 1);
        let purse = run.rubles;
        assert!(settle(&mut run, &zone).is_empty());
        assert_eq!(run.rubles, purse);
    }

    #[test]
    fn a_chain_hands_out_one_link_at_a_time() {
        let (zone, mut run) = fixture();
        // The main chain is five jobs, and only the first is on the board.
        let chain: Vec<&str> = {
            let mut ids: Vec<&str> = zone
                .quests
                .iter()
                .filter(|(_, q)| q.requires.is_some())
                .map(|(id, _)| id.as_str())
                .collect();
            ids.sort_unstable();
            ids
        };
        assert!(!chain.is_empty(), "there is a chain to walk");
        for id in &chain {
            let quest = &zone.quests[*id];
            let needed = quest.requires.clone().unwrap();
            assert!(
                !offered(&zone, &run, &quest.faction).contains(id),
                "{id} showed up before {needed} was done"
            );
            run.quests_done.insert(needed);
            assert!(
                offered(&zone, &run, &quest.faction).contains(id),
                "{id} did not open once its prerequisite was settled"
            );
        }
    }

    #[test]
    fn the_journal_carries_lore_alongside_the_work() {
        let (zone, mut run) = fixture();
        assert!(journal_entries(&zone, &run).is_empty());

        run.quests_taken.insert("grishas_pot".into());
        let id = zone.lore.keys().next().expect("there is lore").clone();
        run.lore.insert(id.clone());

        let entries = journal_entries(&zone, &run);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].0.contains("An eye for the pot"));
        assert!(entries[1].0.starts_with("[lore]"), "{:?}", entries[1].0);
        assert_eq!(entries[1].1, zone.lore[&id].text);
    }

    #[test]
    fn the_board_only_offers_what_you_have_not_taken_or_done() {
        let (zone, mut run) = fixture();
        let all = offered(&zone, &run, "loners");
        assert!(all.contains(&"grishas_pot") && all.contains(&"walk_the_rim"));
        assert!(!all.contains(&"cull"), "that is Duty work");

        run.quests_taken.insert("grishas_pot".into());
        assert!(!offered(&zone, &run, "loners").contains(&"grishas_pot"));
        assert!(taken(&zone, &run).contains(&"grishas_pot"));

        run.quests_taken.clear();
        run.quests_done.insert("grishas_pot".into());
        assert!(!offered(&zone, &run, "loners").contains(&"grishas_pot"));
    }

    #[test]
    fn the_other_two_goal_shapes_read_the_run() {
        let (zone, mut run) = fixture();
        run.quests_taken.insert("walk_the_rim".into());
        run.quests_taken.insert("cull".into());
        assert!(settle(&mut run, &zone).is_empty());

        run.discovered.insert("quarry".into());
        run.kills.insert("flesh".into());
        let msg = settle(&mut run, &zone);
        assert!(msg.contains("Walk the rim") && msg.contains("Cull the rim"), "{msg}");
        assert_eq!(run.rep_of("duty"), 15);
    }
}
