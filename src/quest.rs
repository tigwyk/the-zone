//! Jobs: the board you take them from, the journal you track them in, and the
//! standing they move (GDD §9).

use bevy::prelude::*;
use serde::Deserialize;

use crate::area::ZoneData;
use crate::render::{TileGrid, PALETTE};
use crate::run::RunState;
use crate::screens::{draw_chrome, hint, title, wrap};
use crate::sim::GameClock;

const LIST_ROW: usize = 4;
const MESSAGE_ROW: usize = 27;

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
            q.faction == faction && !run.quests_taken.contains(*id) && !run.quests_done.contains(*id)
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

/// Shared list renderer: one job a row, name and pay.
fn list(grid: &mut TileGrid, zone: &ZoneData, ids: &[&str], sel: usize, empty: &str) {
    if ids.is_empty() {
        grid.text(2, LIST_ROW, empty, PALETTE.desc, false);
        return;
    }
    for (i, id) in ids.iter().enumerate() {
        let quest = &zone.quests[*id];
        let fg = if i == sel { PALETTE.menu_sel } else { PALETTE.menu };
        grid.text(0, LIST_ROW + i, if i == sel { "> " } else { "  " }, fg, false);
        let label = format!("{:<32}{:>6} RU   {}", quest.name, quest.rubles, quest.faction);
        grid.text(2, LIST_ROW + i, &label, fg, false);
    }
    // The highlighted job explains itself underneath.
    for (i, line) in wrap(&zone.quests[ids[sel.min(ids.len() - 1)]].text, 76)
        .iter()
        .enumerate()
    {
        grid.text(2, LIST_ROW + ids.len() + 2 + i, line, PALETTE.desc, false);
    }
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
    list(grid, zone, &ids, board.sel, "Nothing on the board today.");
    grid.text(0, MESSAGE_ROW, message, PALETTE.desc, false);
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

    let ids = taken(zone, run);
    list(grid, zone, &ids, sel, "You are not carrying anyone's work.");

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
    if !run.quests_done.is_empty() {
        let line = format!("{} job(s) settled", run.quests_done.len());
        grid.text(crate::screens::PANEL_COL, y + 1, &line, PALETTE.desc, false);
    }

    grid.text(0, MESSAGE_ROW, message, PALETTE.desc, false);
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
        (zone, RunState::roll(0, &[8, 9, 4]))
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
