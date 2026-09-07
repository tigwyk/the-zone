//! Menu-option dialogue (GDD §9). A line can require a skill or a standing, and says
//! so in brackets; an unmet line still shows, greyed, so you learn what you are short of.

use std::collections::HashMap;

use bevy::prelude::*;
use serde::Deserialize;

use crate::area::{Action, ZoneData};
use crate::render::{TileGrid, PALETTE};
use crate::run::{RunState, Skill};
use crate::screens::{draw_chrome, draw_message, hint, title, wrap};
use crate::sim::GameClock;

const TEXT_ROW: usize = 3;
const OPTIONS_ROW: usize = 8;

#[derive(Deserialize, Clone)]
#[serde(rename = "Npc")]
pub(crate) struct NpcData {
    pub name: String,
    pub start: String,
    pub nodes: HashMap<String, Node>,
}

#[derive(Deserialize, Clone)]
#[serde(rename = "Node")]
pub(crate) struct Node {
    pub text: String,
    pub options: Vec<Line>,
}

#[derive(Deserialize, Clone)]
#[serde(rename = "Line")]
pub(crate) struct Line {
    pub text: String,
    /// What the stalker has to be to say this. `ponytail:` GDD §9 also allows a raw
    /// attribute threshold, which no line asks for yet.
    #[serde(default)]
    pub req: Option<Req>,
    #[serde(default)]
    pub action: Option<Action>,
    /// Where the conversation goes next. `None` ends it.
    #[serde(default)]
    pub goto: Option<String>,
}

#[derive(Deserialize, Clone)]
pub(crate) enum Req {
    Skill(Skill, i32),
    Rep(String, i32),
    Flag(String),
    /// The Room's short list is built from the run (GDD §9): what you have, what you
    /// have killed, what you have done for people.
    Rubles(u32),
    Carrying(String),
    Killed(String),
    QuestsDone(usize),
}

impl Req {
    fn met(&self, run: &RunState) -> bool {
        match self {
            Req::Skill(skill, at_least) => run.skills[skill.index()] >= *at_least,
            Req::Rep(faction, at_least) => run.rep_of(faction) >= *at_least,
            Req::Flag(flag) => run.flags.contains(flag),
            Req::Rubles(at_least) => run.rubles >= *at_least,
            Req::Carrying(item) => run.count_of(item) > 0,
            Req::Killed(enemy) => run.kills.contains(enemy),
            Req::QuestsDone(at_least) => run.quests_done.len() >= *at_least,
        }
    }
}

fn met(line: &Line, run: &RunState) -> bool {
    line.req.as_ref().is_none_or(|r| r.met(run))
}

#[derive(Resource, Default)]
pub(crate) struct Dialogue {
    pub npc: String,
    pub node: String,
    pub sel: usize,
}

pub(crate) fn start(npc_id: &str, dialogue: &mut Dialogue, zone: &ZoneData) {
    let npc = &zone.npcs[npc_id];
    *dialogue = Dialogue {
        npc: npc_id.to_string(),
        node: npc.start.clone(),
        sel: 0,
    };
}

fn node<'a>(dialogue: &Dialogue, zone: &'a ZoneData) -> &'a Node {
    &zone.npcs[&dialogue.npc].nodes[&dialogue.node]
}

pub(crate) fn options<'a>(dialogue: &Dialogue, zone: &'a ZoneData) -> &'a [Line] {
    &node(dialogue, zone).options
}

/// What picking the highlighted line did.
pub(crate) struct Taken {
    pub action: Option<Action>,
    pub close: bool,
    pub message: String,
}

pub(crate) fn take(dialogue: &mut Dialogue, run: &RunState, zone: &ZoneData) -> Taken {
    let line = options(dialogue, zone)[dialogue.sel].clone();
    if !met(&line, run) {
        return Taken {
            action: None,
            close: false,
            message: "You are not the one to make that argument.".into(),
        };
    }
    match &line.goto {
        Some(next) => {
            dialogue.node = next.clone();
            dialogue.sel = 0;
            Taken { action: line.action, close: false, message: String::new() }
        }
        None => Taken { action: line.action, close: true, message: String::new() },
    }
}

// ---- the screen ----

pub(crate) fn build_dialogue_grid(
    grid: &mut TileGrid,
    zone: &ZoneData,
    run: &RunState,
    clock: &GameClock,
    dialogue: &Dialogue,
    message: &str,
) {
    grid.clear();
    let npc = &zone.npcs[&dialogue.npc];
    title(grid, &npc.name);

    for (i, line) in wrap(&node(dialogue, zone).text, 76).iter().enumerate() {
        grid.text(2, TEXT_ROW + i, line, PALETTE.pale, false);
    }

    for (i, line) in options(dialogue, zone).iter().enumerate() {
        let selected = i == dialogue.sel;
        let fg = match (met(line, run), selected) {
            (false, _) => PALETTE.dim,
            (true, true) => PALETTE.menu_sel,
            (true, false) => PALETTE.menu,
        };
        grid.text(0, OPTIONS_ROW + i, if selected { "> " } else { "  " }, fg, false);
        grid.text(2, OPTIONS_ROW + i, &line.text, fg, false);
    }

    draw_message(grid, message);
    hint(grid, "Up/Down choose   Enter say it   Esc walk away");
    draw_chrome(grid, run, clock);
}

// ---- tests (SPEC §7) ----

#[cfg(test)]
mod tests {
    use super::*;
    use crate::area::load_zone;
    use std::path::Path;

    fn fixture() -> (ZoneData, RunState, Dialogue) {
        let zone = load_zone(Path::new("assets/data"));
        let run = RunState::roll(0, &[8, 9, 4], &mut crate::run::Rng::new(3));
        let mut dialogue = Dialogue::default();
        start("grisha", &mut dialogue, &zone);
        (zone, run, dialogue)
    }

    #[test]
    fn a_gated_line_shows_but_will_not_be_said() {
        let (zone, mut run, mut dialogue) = fixture();
        // The Barter line is out of reach for a starting Loner.
        let gated = options(&dialogue, &zone)
            .iter()
            .position(|l| l.text.contains("Barter"))
            .expect("grisha has a barter line");
        assert!(!met(&options(&dialogue, &zone)[gated], &run));

        dialogue.sel = gated;
        let taken = take(&mut dialogue, &run, &zone);
        assert!(taken.message.contains("not the one"));
        assert_eq!(dialogue.node, "hello", "a blocked line goes nowhere");

        // Learn to haggle and the same line opens.
        run.skills[Skill::Barter.index()] = 45;
        let taken = take(&mut dialogue, &run, &zone);
        assert!(taken.message.is_empty());
        assert_eq!(dialogue.node, "haggle");
    }

    #[test]
    fn a_line_can_carry_an_action_and_an_ending() {
        let (zone, run, mut dialogue) = fixture();
        // Grisha's rumour has a line that sets a flag and ends the conversation.
        dialogue.node = "rumour".into();
        dialogue.sel = options(&dialogue, &zone)
            .iter()
            .position(|l| l.action.is_some())
            .expect("the rumour sets a flag");
        let taken = take(&mut dialogue, &run, &zone);
        assert!(taken.close, "that line ends it");
        assert!(matches!(taken.action, Some(Action::SetFlag(f)) if f == "heard_the_rumour"));
    }

    #[test]
    fn standing_opens_lines_that_skill_cannot() {
        let (zone, mut run, dialogue) = fixture();
        let line = options(&dialogue, &zone)
            .iter()
            .find(|l| l.text.contains("Loners"))
            .expect("grisha has a rep line");
        assert!(!met(line, &run), "20 standing is not 25");
        run.rep.insert("loners".into(), 30);
        assert!(met(line, &run));
    }
}
