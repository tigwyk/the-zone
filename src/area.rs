//! Area data, the `.area`/`zone.ron` loaders, and the area scene build function.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use bevy::prelude::*;
use serde::Deserialize;

use crate::dialogue::NpcData;
use crate::loot::{AffixData, ItemStack};
use crate::meta::EndingData;
use crate::quest::QuestData;
use crate::render::{Glyph, TileGrid, PALETTE};
use crate::run::{RunState, Skill};
use crate::screens::{draw_chrome, draw_message};
use crate::sim::{visible_menu, Fields, GameClock};

// Fixed row map (SPEC §4): art 0–17, desc 19–20, menu 22–26, message 27, status 28, footer 29.
const DESC_ROW: usize = 19;
const MENU_ROW: usize = 22;
pub(crate) const MESSAGE_ROW: usize = 27;

// ---- zone.ron model (SPEC §5.2) ----

#[derive(Deserialize)]
struct Zone {
    start: String,
    areas: HashMap<String, Area>,
}

/// One thing the Zone tells you. Found once, kept for good (GDD §10).
#[derive(Deserialize, Clone)]
#[serde(rename = "Lore")]
pub(crate) struct LoreData {
    pub title: String,
    pub text: String,
}

/// A faction and the ones it cannot stand (GDD §9).
#[derive(Deserialize, Clone)]
#[serde(rename = "Faction")]
pub(crate) struct FactionData {
    pub name: String,
    pub rivals: Vec<String>,
}

/// One thing that fights back (GDD §8).
#[derive(Deserialize, Clone)]
#[serde(rename = "Enemy")]
pub(crate) struct EnemyData {
    pub name: String,
    pub hp: i32,
    pub ap: i32,
    /// To-hit skill, rolled under like any other check.
    pub skill: i32,
    pub dice: (u32, u32),
    pub armor: i32,
    /// A mutant that can only bite has to close the band first.
    #[serde(default)]
    pub melee_only: bool,
    /// GDD §8: bandits and Fleshes run at under 20% HP; bloodsuckers do not.
    #[serde(default)]
    pub flees: bool,
    /// How deep it lives, which is what its loot rolls against.
    #[serde(default)]
    pub tier: u32,
    /// Invisible until it strikes: a PER check to act first, or it opens on you.
    #[serde(default)]
    pub ambush: bool,
    /// Gets into your head: a will check each turn, or the action is lost.
    #[serde(default)]
    pub mind: bool,
    pub loot: Vec<(String, u32)>,
}

#[derive(Deserialize)]
struct Area {
    name: String,
    art: String,
    #[serde(default)]
    shelter: bool,
    menu: Vec<(String, Action)>,
    #[serde(default)]
    secrets: HashMap<char, Secret>,
    #[serde(default)]
    anomaly: Option<AnomalyData>,
    /// How deep this is, which is what anything found here rolls against.
    #[serde(default)]
    tier: u32,
    /// The thing that lives here. Fought once per run, on arrival.
    /// `ponytail:` no roving encounters yet - a `chance` field is the upgrade path.
    #[serde(default)]
    encounter: Option<String>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct Secret {
    pub action: Action,
    #[serde(default)]
    pub gate: Option<Gate>,
}

/// What has to be true before a hidden letter shows itself (SPEC §5.2).
#[derive(Deserialize, Clone)]
pub(crate) enum Gate {
    /// Rolled once per run, the first time you enter the area.
    Check(Skill, i32),
    Flag(String),
    Rep(String, i32),
    /// Standing that high with anyone at all (GDD §9: the route to the centre opens
    /// for whichever faction you are highest with).
    AnyRep(i32),
}

#[derive(Deserialize, Clone)]
pub(crate) enum Action {
    Travel(String),
    Say(String),
    Rest,
    Trade(String),
    Scan,
    ThrowBolt,
    PushThrough,
    TakeArtifact,
    SetFlag(String),
    Talk(String),
    Jobs(String),
    Memorial,
    /// Turn up a lore entry. Found once; it outlives the stalker who found it.
    Lore(String),
    /// Put something in the pack, once. GDD §8: a secret has to pay off, and the
    /// payoff is a room, an artifact, or something you can carry out.
    Give(String, u32),
    /// End the run on this ending. There is nothing after it.
    End(String),
}

/// An anomaly field: the danger of walking in, what it does to you, what it hides.
#[derive(Deserialize, Clone)]
#[serde(rename = "Anomaly")]
pub(crate) struct AnomalyData {
    pub name: String,
    /// Percent chance that pushing through blind hurts (GDD §6: 30-80).
    pub danger: u32,
    /// Contact damage, NdS.
    pub dice: (u32, u32),
    pub artifact: String,
    /// The area on the far side.
    pub beyond: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename = "Item")]
pub(crate) struct ItemData {
    pub name: String,
    pub base: u32,
    pub kind: ItemKind,
}

#[derive(Deserialize, Clone, Copy, PartialEq)]
pub(crate) enum ItemKind {
    Heal(i32),
    Antirad(i32),
    /// Damage dice NdS, and the skill that swings or fires it (GDD §8).
    Weapon { dice: (u32, u32), skill: Skill },
    Armor(i32),
    /// Carried: +bonus to one attribute, +rads every hour (GDD §6).
    Artifact { attr: usize, bonus: i32, rads: i32 },
    /// Kills the night penalty on PER checks (GDD §5).
    Light,
    Misc,
}

#[derive(Deserialize, Clone)]
#[serde(rename = "Vendor")]
pub(crate) struct VendorData {
    pub name: String,
    pub faction: String,
    pub markup: f32,
    /// Vendor specialty (GDD §7): the trader pays 1.5x for artifacts.
    #[serde(default = "one")]
    pub artifact_markup: f32,
    pub stock: Vec<(String, u32)>,
}

fn one() -> f32 {
    1.0
}

// name/shelter are read by later milestones (status/map, M3 emissions).
#[allow(dead_code)]
pub(crate) struct AreaData {
    pub name: String,
    pub shelter: bool,
    pub art: Vec<String>,
    pub desc: Vec<String>,
    pub menu: Vec<(String, Action)>,
    pub secrets: HashMap<char, Secret>,
    pub secret_cells: Vec<(usize, usize, char)>,
    pub anomaly: Option<AnomalyData>,
    pub encounter: Option<String>,
    pub tier: u32,
}

#[derive(Resource)]
pub(crate) struct ZoneData {
    pub start: String,
    pub areas: HashMap<String, AreaData>,
    pub items: HashMap<String, ItemData>,
    pub vendors: HashMap<String, VendorData>,
    pub enemies: HashMap<String, EnemyData>,
    pub npcs: HashMap<String, NpcData>,
    pub quests: HashMap<String, QuestData>,
    pub factions: HashMap<String, FactionData>,
    pub endings: HashMap<String, EndingData>,
    pub lore: HashMap<String, LoreData>,
    pub affixes: HashMap<String, AffixData>,
}

impl FromWorld for ZoneData {
    fn from_world(_world: &mut World) -> Self {
        load_zone(Path::new("assets/data"))
    }
}

/// Live vendor inventories, seeded from `zone.ron` at startup. Separate from
/// `ZoneData` because stock changes as the player trades.
/// `ponytail:` no restock roll yet — the stock table lands with the clock in M3.
#[derive(Resource)]
pub(crate) struct VendorStock(pub HashMap<String, Vec<ItemStack>>);

/// A shelf as it was authored: plain goods, in the order the vendor lists them.
/// `ponytail:` shops do not roll affixes - what the Zone has been at comes out of
/// the Zone, and a restock that rolled would be the place to change that.
pub(crate) fn shelf(vendor: &VendorData) -> Vec<ItemStack> {
    vendor
        .stock
        .iter()
        .enumerate()
        .map(|(i, (id, n))| ItemStack::plain(i as u32, id, *n))
        .collect()
}

impl FromWorld for VendorStock {
    fn from_world(world: &mut World) -> Self {
        let zone = world.resource::<ZoneData>();
        VendorStock(zone.vendors.iter().map(|(id, v)| (id.clone(), shelf(v))).collect())
    }
}

// ---- loading ----

/// Reads one RON file or dies naming it (SPEC §5: a load error is a panic).
fn read_ron<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    ron::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub(crate) fn load_zone(data_dir: &Path) -> ZoneData {
    let zone: Zone = read_ron(&data_dir.join("zone.ron"));

    let mut areas = HashMap::new();
    for (id, a) in zone.areas {
        let art_path = data_dir.join("areas").join(format!("{}.area", a.art));
        let (art, desc, secret_cells) = load_area_file(&art_path);
        let secrets = a.secrets;
        for &(_, _, ch) in &secret_cells {
            assert!(
                secrets.contains_key(&ch),
                "{}: secret letter '{ch}' has no action in zone.ron",
                art_path.display()
            );
        }
        areas.insert(
            id,
            AreaData {
                name: a.name,
                shelter: a.shelter,
                art,
                desc,
                menu: a.menu,
                secrets,
                secret_cells,
                anomaly: a.anomaly,
                encounter: a.encounter,
                tier: a.tier,
            },
        );
    }

    assert!(
        areas.contains_key(&zone.start),
        "zone.ron: start area '{}' not found",
        zone.start
    );
    let data = ZoneData {
        start: zone.start,
        areas,
        items: read_ron(&data_dir.join("items.ron")),
        vendors: read_ron(&data_dir.join("vendors.ron")),
        enemies: read_ron(&data_dir.join("enemies.ron")),
        npcs: read_ron(&data_dir.join("npcs.ron")),
        quests: read_ron(&data_dir.join("quests.ron")),
        factions: read_ron(&data_dir.join("factions.ron")),
        endings: read_ron(&data_dir.join("endings.ron")),
        lore: read_ron(&data_dir.join("lore.ron")),
        affixes: read_ron(&data_dir.join("affixes.ron")),
    };
    data.validate_ids();
    data
}

impl ZoneData {
    /// Every id an action or a vendor names must exist (SPEC §5.3).
    fn validate_ids(&self) {
        let check = |action: &Action, where_: &str| match action {
            Action::Travel(dest) => assert!(
                self.areas.contains_key(dest),
                "zone.ron: {where_} travels to unknown area '{dest}'"
            ),
            Action::Trade(v) => assert!(
                self.vendors.contains_key(v),
                "zone.ron: {where_} trades with unknown vendor '{v}'"
            ),
            Action::Talk(npc) => assert!(
                self.npcs.contains_key(npc),
                "zone.ron: {where_} talks to unknown npc '{npc}'"
            ),
            Action::Jobs(f) => assert!(
                self.factions.contains_key(f),
                "zone.ron: {where_} opens a board for unknown faction '{f}'"
            ),
            Action::End(ending) => assert!(
                self.endings.contains_key(ending),
                "{where_} ends on unknown ending '{ending}'"
            ),
            Action::Lore(entry) => assert!(
                self.lore.contains_key(entry),
                "{where_} turns up unknown lore '{entry}'"
            ),
            Action::Give(item, _) => assert!(
                self.items.contains_key(item),
                "{where_} hands over unknown item '{item}'"
            ),
            // SPEC §8: one line, at most 78 characters, and no shouting.
            Action::Say(line) => {
                assert!(
                    line.chars().count() <= 78,
                    "zone.ron: {where_} says a line longer than 78 characters"
                );
                assert!(!line.contains('!'), "zone.ron: {where_} shouts");
            }
            _ => {}
        };
        for (id, area) in &self.areas {
            for (label, action) in &area.menu {
                check(action, &format!("{id}/{label}"));
            }
            for (letter, secret) in &area.secrets {
                check(&secret.action, &format!("{id}/'{letter}'"));
            }
            // The anomaly verbs only make sense on a field; `perform` relies on it.
            let verbs = area.menu.iter().map(|(_, a)| a).chain(area.secrets.values().map(|s| &s.action));
            for action in verbs {
                if matches!(
                    action,
                    Action::Scan | Action::ThrowBolt | Action::PushThrough | Action::TakeArtifact
                ) {
                    assert!(
                        area.anomaly.is_some(),
                        "zone.ron: area '{id}' uses an anomaly verb but has no anomaly"
                    );
                }
            }
            if let Some(anomaly) = &area.anomaly {
                assert!(
                    self.items.contains_key(&anomaly.artifact),
                    "zone.ron: field '{id}' hides unknown artifact '{}'",
                    anomaly.artifact
                );
                assert!(
                    self.areas.contains_key(&anomaly.beyond),
                    "zone.ron: field '{id}' leads to unknown area '{}'",
                    anomaly.beyond
                );
            }
        }
        for (id, area) in &self.areas {
            if let Some(enemy) = &area.encounter {
                assert!(
                    self.enemies.contains_key(enemy),
                    "zone.ron: area '{id}' is home to unknown enemy '{enemy}'"
                );
            }
        }
        for (id, e) in &self.enemies {
            for (item, _) in &e.loot {
                assert!(
                    self.items.contains_key(item),
                    "zone.ron: enemy '{id}' drops unknown item '{item}'"
                );
            }
        }
        // Everything the story files point at has to exist too (SPEC §5.3).
        for (id, npc) in &self.npcs {
            assert!(
                npc.nodes.contains_key(&npc.start),
                "npcs.ron: '{id}' starts at missing node '{}'",
                npc.start
            );
            for (node_id, node) in &npc.nodes {
                for line in &node.options {
                    if let Some(goto) = &line.goto {
                        assert!(
                            npc.nodes.contains_key(goto),
                            "npcs.ron: '{id}/{node_id}' goes to missing node '{goto}'"
                        );
                    }
                    if let Some(action) = &line.action {
                        check(action, &format!("npc {id}/{node_id}"));
                    }
                }
            }
        }
        for (id, quest) in &self.quests {
            if let Some(needed) = &quest.requires {
                assert!(
                    self.quests.contains_key(needed),
                    "quests.ron: '{id}' follows unknown job '{needed}'"
                );
                assert_ne!(needed, id, "quests.ron: '{id}' requires itself");
            }
            assert!(
                self.factions.contains_key(&quest.faction),
                "quests.ron: '{id}' belongs to unknown faction '{}'",
                quest.faction
            );
            match &quest.goal {
                crate::quest::Goal::Have(item) => assert!(
                    self.items.contains_key(item),
                    "quests.ron: '{id}' wants unknown item '{item}'"
                ),
                crate::quest::Goal::Reach(area) => assert!(
                    self.areas.contains_key(area),
                    "quests.ron: '{id}' sends you to unknown area '{area}'"
                ),
                crate::quest::Goal::Kill(enemy) => assert!(
                    self.enemies.contains_key(enemy),
                    "quests.ron: '{id}' wants unknown enemy '{enemy}' dead"
                ),
            }
        }
        for (id, faction) in &self.factions {
            for rival in &faction.rivals {
                assert!(
                    self.factions.contains_key(rival),
                    "factions.ron: '{id}' hates unknown faction '{rival}'"
                );
            }
        }
        for (id, v) in &self.vendors {
            assert!(
                self.factions.contains_key(&v.faction),
                "vendors.ron: '{id}' belongs to unknown faction '{}'",
                v.faction
            );
            for (item, _) in &v.stock {
                assert!(
                    self.items.contains_key(item),
                    "zone.ron: vendor '{id}' stocks unknown item '{item}'"
                );
            }
        }
    }
}

fn load_area_file(path: &Path) -> (Vec<String>, Vec<String>, Vec<(usize, usize, char)>) {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let lines: Vec<&str> = text.lines().collect();
    let sep = lines
        .iter()
        .position(|l| *l == "---")
        .unwrap_or_else(|| panic!("{}: missing '---' separator", path.display()));
    let art_lines = &lines[..sep];
    let desc_lines = &lines[sep + 1..];

    assert!(art_lines.len() <= 18, "{}: art exceeds 18 rows", path.display());
    assert!(
        desc_lines.len() <= 2,
        "{}: description exceeds 2 lines",
        path.display()
    );

    let mut art = Vec::new();
    let mut secret_cells = Vec::new();
    for (row, line) in art_lines.iter().enumerate() {
        let (stripped, secrets) = strip_markers(line, path, row);
        assert!(
            stripped.len() <= 80,
            "{}:{}: art line exceeds 80 columns",
            path.display(),
            row
        );
        for b in stripped.bytes() {
            assert!(
                (32..=126).contains(&b),
                "{}:{}: non-ASCII in art",
                path.display(),
                row
            );
        }
        for (col, ch) in secrets {
            secret_cells.push((col, row, ch));
        }
        art.push(stripped);
    }

    let desc: Vec<String> = desc_lines
        .iter()
        .map(|l| {
            assert!(
                l.len() <= 78,
                "{}: description line exceeds 78 chars",
                path.display()
            );
            // SPEC §8: dread is quiet. The Zone does not shout.
            assert!(
                !l.contains('!'),
                "{}: no exclamation marks in a description",
                path.display()
            );
            l.to_string()
        })
        .collect();

    let mut seen = HashSet::new();
    for &(_, _, ch) in &secret_cells {
        assert!(
            seen.insert(ch),
            "{}: secret letter '{ch}' appears more than once",
            path.display()
        );
    }

    (art, desc, secret_cells)
}

fn strip_markers(line: &str, path: &Path, row: usize) -> (String, Vec<(usize, char)>) {
    let mut out = String::new();
    let mut secrets = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' {
            assert!(
                i + 2 < chars.len(),
                "{}:{}: unclosed '{{' marker",
                path.display(),
                row
            );
            let x = chars[i + 1];
            assert!(
                x.is_ascii_uppercase(),
                "{}:{}: marker letter must be A-Z",
                path.display(),
                row
            );
            assert!(
                chars[i + 2] == '}',
                "{}:{}: malformed marker",
                path.display(),
                row
            );
            secrets.push((out.len(), x));
            out.push(x);
            i += 3;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    (out, secrets)
}

// ---- build (SPEC §3: input -> mutate -> build -> render) ----

pub(crate) fn build_area_grid(
    grid: &mut TileGrid,
    zone: &ZoneData,
    run: &RunState,
    clock: &GameClock,
    fields: &Fields,
    area_id: &str,
    sel: usize,
    message: &str,
) {
    grid.clear();
    draw_art(grid, zone, run, fields, area_id);
    let area = &zone.areas[area_id];

    // Description rows 19-20.
    for (dy, line) in area.desc.iter().enumerate() {
        grid.text(0, DESC_ROW + dy, line, PALETTE.desc, false);
    }

    // Menu rows 22-26.
    for (i, (label, _)) in visible_menu(area, fields, area_id).iter().enumerate() {
        let fg = if i == sel { PALETTE.menu_sel } else { PALETTE.menu };
        let prefix = if i == sel { "> " } else { "  " };
        grid.text(0, MENU_ROW + i, prefix, fg, false);
        grid.text(2, MENU_ROW + i, label, fg, false);
    }

    draw_message(grid, message);
    draw_chrome(grid, run, clock);
}

/// Everywhere this area leads: menu exits, secret exits, and the far side of an
/// anomaly. The map network is derived from this, so there is no adjacency table.
pub(crate) fn exits(area: &AreaData) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let actions = area
        .menu
        .iter()
        .map(|(_, a)| a)
        .chain(area.secrets.values().map(|s| &s.action));
    for action in actions {
        if let Action::Travel(dest) = action {
            out.push(dest.clone());
        }
    }
    if let Some(anomaly) = &area.anomaly {
        out.push(anomaly.beyond.clone());
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Rows 0-17: the scene itself. Combat draws over the rest of the screen but keeps
/// this, so a fight happens somewhere (GDD §8).
pub(crate) fn draw_art(
    grid: &mut TileGrid,
    zone: &ZoneData,
    run: &RunState,
    fields: &Fields,
    area_id: &str,
) {
    let area = &zone.areas[area_id];
    // An unscanned field keeps its tells dull; Scan is what lights them amber (GDD §6).
    let tells_lit = area.anomaly.is_none() || fields.get(area_id).scanned;

    for (y, line) in area.art.iter().enumerate() {
        for (x, ch) in line.chars().enumerate() {
            let mut g = Glyph {
                ch,
                fg: class_color(ch, tells_lit),
                bold: false,
            };
            // A gated letter renders as ordinary art until it is revealed (GDD §5).
            if let Some(&(_, _, letter)) = area
                .secret_cells
                .iter()
                .find(|&&(sx, sy, _)| sx == x && sy == y)
            {
                if run.is_revealed(area_id, letter) {
                    g.fg = PALETTE.secret;
                    g.bold = true;
                }
            }
            grid.set(x, y, g);
        }
    }
}

// Character class -> palette color (SPEC §5.1). Anomaly tells render amber.
fn class_color(ch: char, tells_lit: bool) -> Color {
    match ch {
        ' ' => PALETTE.dim,
        '~' => PALETTE.smoke,
        '^' => PALETTE.fire,
        '*' | '+' | '@' | '.' | ':' if tells_lit => PALETTE.amber,
        '*' | '+' | '@' | '.' | ':' => PALETTE.grey,
        _ => PALETTE.ground,
    }
}

// ---- tests (SPEC §7) ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_parsing_strips_braces_and_records_position() {
        let (line, secrets) = strip_markers("[{D}]", Path::new("t.area"), 0);
        assert_eq!(line, "[D]");
        assert_eq!(secrets, vec![(1, 'D')]);
    }

    #[test]
    #[should_panic(expected = "appears more than once")]
    fn duplicate_letter_rejected() {
        let dir = std::env::temp_dir().join("the-zone-m1-test");
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("dup.area");
        fs::write(&p, "{D} x\n{D} y\n---\n").unwrap();
        load_area_file(&p);
    }

    /// Every action anywhere in the data, so a coverage check cannot miss one.
    #[cfg(test)]
    fn all_actions(zone: &ZoneData) -> Vec<&Action> {
        let mut out: Vec<&Action> = Vec::new();
        for area in zone.areas.values() {
            out.extend(area.menu.iter().map(|(_, a)| a));
            out.extend(area.secrets.values().map(|s| &s.action));
        }
        for npc in zone.npcs.values() {
            for node in npc.nodes.values() {
                out.extend(node.options.iter().filter_map(|l| l.action.as_ref()));
            }
        }
        out
    }

    #[test]
    fn every_area_can_be_walked_to_from_the_start() {
        // Thirty areas wired by hand: an orphan is a content bug that no other
        // test would ever reach, because no player could either.
        let zone = load_zone(Path::new("assets/data"));
        let mut seen: HashSet<String> = HashSet::from([zone.start.clone()]);
        let mut queue = vec![zone.start.clone()];
        while let Some(id) = queue.pop() {
            for dest in exits(&zone.areas[&id]) {
                if seen.insert(dest.clone()) {
                    queue.push(dest);
                }
            }
        }
        let orphans: Vec<&String> = zone.areas.keys().filter(|id| !seen.contains(*id)).collect();
        assert!(orphans.is_empty(), "unreachable: {orphans:?}");
    }

    #[test]
    fn every_written_thing_is_reachable_in_play() {
        let zone = load_zone(Path::new("assets/data"));
        let actions = all_actions(&zone);

        // Lore nobody can turn up is lore nobody wrote.
        let found: HashSet<&str> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Lore(id) => Some(id.as_str()),
                _ => None,
            })
            .collect();
        let missing: Vec<&String> = zone.lore.keys().filter(|id| !found.contains(id.as_str())).collect();
        assert!(missing.is_empty(), "unreachable lore: {missing:?}");

        // Same for the boards: a faction with jobs and no board is a dead end.
        let boards: HashSet<&str> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Jobs(f) => Some(f.as_str()),
                _ => None,
            })
            .collect();
        for (id, quest) in &zone.quests {
            assert!(
                boards.contains(quest.faction.as_str()),
                "job '{id}' is on a board nobody can open ({})",
                quest.faction
            );
        }

        // And every enemy should live somewhere.
        let homes: HashSet<&str> = zone
            .areas
            .values()
            .filter_map(|a| a.encounter.as_deref())
            .collect();
        let unused: Vec<&String> = zone.enemies.keys().filter(|id| !homes.contains(id.as_str())).collect();
        assert!(unused.is_empty(), "enemies with nowhere to be: {unused:?}");
    }

    #[test]
    fn loads_zone_data() {
        let zone = load_zone(Path::new("assets/data"));
        assert_eq!(zone.start, "camp");
        assert_eq!(zone.areas.len(), 30); // GDD §12
        assert_eq!(zone.endings.len(), 6); // GDD §12
        assert!(zone.npcs.contains_key("room"));
        // GDD §12 targets, so that content drifting below them fails the build.
        assert_eq!(zone.npcs.len(), 10);
        assert_eq!(zone.lore.len(), 20);
        assert!(zone.quests.len() >= 20, "15 jobs and a five-step chain");
        assert!(zone.enemies.len() >= 6);
        assert_eq!(zone.vendors.len(), 4);
        let anomalies = zone.areas.values().filter(|a| a.anomaly.is_some()).count();
        assert_eq!(anomalies, 8, "eight anomaly fields");
        let artifacts = zone
            .items
            .values()
            .filter(|i| matches!(i.kind, ItemKind::Artifact { .. }))
            .count();
        assert_eq!(artifacts, 12);
        assert!(zone.items.len() - artifacts >= 40, "forty items besides");
        // About ten areas you can only reach through the art (GDD §5).
        let secret_targets: std::collections::HashSet<&str> = zone
            .areas
            .values()
            .flat_map(|a| a.secrets.values())
            .filter_map(|s| match &s.action {
                Action::Travel(dest) => Some(dest.as_str()),
                _ => None,
            })
            .collect();
        assert!(secret_targets.len() >= 10, "{secret_targets:?}");
        assert!(zone.npcs.contains_key("grisha"));
        assert!(zone.quests.contains_key("cull"));
        assert_eq!(zone.factions.len(), 7);
        // The network is derived, not tabulated.
        assert_eq!(exits(&zone.areas["field"]), vec!["quarry", "road"]);
        assert!(zone.areas["camp"].secrets.contains_key(&'D'));
        assert!(zone.areas["field"].anomaly.is_some());
        assert_eq!(zone.areas["camp"].secret_cells, vec![(13, 14, 'D')]);
        assert!(zone.vendors.contains_key("trader"));
        assert!(zone.items.contains_key("medkit"));
    }
}
