//! Area data, the `.area`/`zone.ron` loaders, and the area scene build function.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use bevy::prelude::*;
use serde::Deserialize;

use crate::dialogue::NpcData;
use crate::liquid::{draw_liquids, Liquid, Puddles};
use crate::loot::{AffixData, ItemStack};
use crate::meta::EndingData;
use crate::quest::QuestData;
use crate::render::{Glyph, TileGrid, GRID_W, PALETTE};
use crate::run::{RunState, Skill};
use crate::screens::{draw_chrome, draw_message, row};
use crate::sim::{visible_menu, Fields, GameClock};

// Fixed row map (SPEC §4): art 0–17, desc 19–20, menu 22–26, gap 27, message 28,
// gap 29, status 30, gap 31, footer 32.
const DESC_ROW: usize = 19;
const MENU_ROW: usize = 22;
pub(crate) const MESSAGE_ROW: usize = 28;

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
    /// What is pooled on the floor when you arrive (SPEC §5.2): (liquid id, amount).
    #[serde(default)]
    liquids: Vec<(String, u32)>,
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
    /// The bench: enter the Crafting screen (GDD §13).
    Craft,
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

/// One thing the bench or the forge can make (GDD §13). A bench recipe spends
/// `inputs` and rolls its skill to make `output`; a forge recipe spends a `catalyst`
/// artifact and bakes `affix` into the held weapon or armour it fits.
#[derive(Deserialize, Clone)]
#[serde(rename = "Recipe")]
pub(crate) struct RecipeData {
    pub name: String,
    pub skill: Skill,
    pub difficulty: i32,
    pub minutes: u32,
    #[serde(default)]
    pub inputs: Vec<(String, u32)>,
    #[serde(default)]
    pub output: Option<(String, u32)>,
    #[serde(default)]
    pub catalyst: Option<String>,
    #[serde(default)]
    pub affix: Option<String>,
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
    pub liquids: Vec<(String, u32)>,
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
    pub recipes: HashMap<String, RecipeData>,
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
        for (lid, _) in &a.liquids {
            assert!(
                Liquid::ALL.iter().any(|l| l.id() == lid.as_str()),
                "zone.ron: area '{id}' names unknown liquid '{lid}'"
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
                liquids: a.liquids,
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
        recipes: read_ron(&data_dir.join("recipes.ron")),
    };
    data.validate_ids();
    data
}

/// One id, in the table that has to hold it. Every check in `validate_ids` is this
/// shape: where the mistake is, what the file says it does, and the id that is not
/// there. Panics naming all three, which is the whole error message an author needs.
fn must<V>(table: &HashMap<String, V>, id: &str, whose: &str, verb: &str) {
    assert!(table.contains_key(id), "{whose} {verb} '{id}'");
}

impl ZoneData {
    /// Every id an action or a vendor names must exist (SPEC §5.3).
    fn validate_ids(&self) {
        let check = |action: &Action, whose: &str| match action {
            Action::Travel(d) => must(&self.areas, d, whose, "travels to unknown area"),
            Action::Trade(v) => must(&self.vendors, v, whose, "trades with unknown vendor"),
            Action::Talk(n) => must(&self.npcs, n, whose, "talks to unknown npc"),
            Action::Jobs(f) => must(&self.factions, f, whose, "opens a board for unknown faction"),
            Action::End(e) => must(&self.endings, e, whose, "ends on unknown ending"),
            Action::Lore(l) => must(&self.lore, l, whose, "turns up unknown lore"),
            Action::Give(i, _) => must(&self.items, i, whose, "hands over unknown item"),
            // SPEC §8: one line, at most 78 characters, and no shouting.
            Action::Say(line) => {
                assert!(
                    line.chars().count() <= 78,
                    "{whose} says a line longer than 78 characters"
                );
                assert!(!line.contains('!'), "{whose} shouts");
            }
            _ => {}
        };
        for (id, area) in &self.areas {
            for (label, action) in &area.menu {
                check(action, &format!("zone.ron: {id}/{label}"));
            }
            for (letter, secret) in &area.secrets {
                check(&secret.action, &format!("zone.ron: {id}/'{letter}'"));
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
            let whose = format!("zone.ron: field '{id}'");
            if let Some(a) = &area.anomaly {
                must(&self.items, &a.artifact, &whose, "hides unknown artifact");
                must(&self.areas, &a.beyond, &whose, "leads to unknown area");
            }
            let whose = format!("zone.ron: area '{id}'");
            if let Some(enemy) = &area.encounter {
                must(&self.enemies, enemy, &whose, "is home to unknown enemy");
            }
        }
        for (id, e) in &self.enemies {
            let whose = format!("zone.ron: enemy '{id}'");
            for (item, _) in &e.loot {
                must(&self.items, item, &whose, "drops unknown item");
            }
        }
        for (id, recipe) in &self.recipes {
            let whose = format!("recipes.ron: '{id}'");
            for (item, _) in &recipe.inputs {
                must(&self.items, item, &whose, "needs unknown item");
            }
            if let Some((item, _)) = &recipe.output {
                must(&self.items, item, &whose, "makes unknown item");
            }
            if let Some(cat) = &recipe.catalyst {
                must(&self.items, cat, &whose, "uses unknown catalyst");
                assert!(
                    matches!(self.items[cat].kind, ItemKind::Artifact { .. }),
                    "{whose} catalyst '{cat}' is not an artifact"
                );
            }
            if let Some(affix) = &recipe.affix {
                must(&self.affixes, affix, &whose, "bakes unknown affix");
            }
            let bench = recipe.output.is_some() && recipe.catalyst.is_none() && recipe.affix.is_none();
            let forge = recipe.output.is_none() && recipe.catalyst.is_some() && recipe.affix.is_some();
            assert!(bench || forge, "{whose} is not a valid bench or forge recipe");
            if let Some(needed) = &recipe.catalyst {
                assert!(recipe.affix.is_some(), "{whose} forge recipe '{needed}' is missing its affix");
            }
            if recipe.affix.is_some() {
                assert!(recipe.catalyst.is_some(), "{whose} affix recipe is missing its catalyst");
            }
        }

        // Everything the story files point at has to exist too (SPEC §5.3).
        for (id, npc) in &self.npcs {
            let whose = format!("npcs.ron: '{id}'");
            must(&npc.nodes, &npc.start, &whose, "starts at missing node");
            for (node_id, node) in &npc.nodes {
                let whose = format!("npcs.ron: '{id}/{node_id}'");
                for line in &node.options {
                    if let Some(goto) = &line.goto {
                        must(&npc.nodes, goto, &whose, "goes to missing node");
                    }
                    if let Some(action) = &line.action {
                        check(action, &whose);
                    }
                }
            }
        }
        for (id, quest) in &self.quests {
            let whose = format!("quests.ron: '{id}'");
            if let Some(needed) = &quest.requires {
                must(&self.quests, needed, &whose, "follows unknown job");
                assert_ne!(needed, id, "{whose} requires itself");
            }
            must(&self.factions, &quest.faction, &whose, "belongs to unknown faction");
            match &quest.goal {
                crate::quest::Goal::Have(i) => must(&self.items, i, &whose, "wants unknown item"),
                crate::quest::Goal::Reach(a) => must(&self.areas, a, &whose, "sends you to unknown area"),
                crate::quest::Goal::Kill(e) => must(&self.enemies, e, &whose, "wants the unknown enemy dead"),
            }
        }
        for (id, faction) in &self.factions {
            let whose = format!("factions.ron: '{id}'");
            for rival in &faction.rivals {
                must(&self.factions, rival, &whose, "hates unknown faction");
            }
        }
        for (id, v) in &self.vendors {
            let whose = format!("vendors.ron: '{id}'");
            must(&self.factions, &v.faction, &whose, "belongs to unknown faction");
            for (item, _) in &v.stock {
                must(&self.items, item, &whose, "stocks unknown item");
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
            stripped.len() <= GRID_W,
            "{}:{}: art line exceeds {GRID_W} columns",
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
    puddles: &Puddles,
    area_id: &str,
    sel: usize,
    message: &str,
) {
    grid.clear();
    draw_art(grid, zone, run, fields, area_id);
    let area = &zone.areas[area_id];

    // Description rows 19-20, centered under the art.
    for (dy, line) in area.desc.iter().enumerate() {
        let x = GRID_W.saturating_sub(line.chars().count()) / 2;
        grid.text(x, DESC_ROW + dy, line, PALETTE.desc, false);
    }
    // What has pooled on the floor, named in its own colour below the description.
    draw_liquids(grid, puddles, area_id);

    // Menu rows 22-26.
    for (i, (label, _)) in visible_menu(area, fields, area_id).iter().enumerate() {
        row(grid, 0, MENU_ROW + i, i == sel, PALETTE.menu, false, label);
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

    // The art is authored at up to 80 columns; center it in the widescreen grid.
    let art_width = area.art.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let x_offset = GRID_W.saturating_sub(art_width) / 2;

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
            grid.set(x + x_offset, y, g);
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
        assert_eq!(zone.areas["camp"].secret_cells, vec![(49, 17, 'D')]);
        assert!(zone.vendors.contains_key("trader"));
        assert!(zone.items.contains_key("medkit"));
    }
}
