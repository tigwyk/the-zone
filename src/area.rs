//! Area data, the `.area`/`zone.ron` loaders, and the area scene build function.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use bevy::prelude::*;
use serde::Deserialize;

use crate::render::{Glyph, TileGrid, PALETTE};
use crate::run::RunState;
use crate::screens::draw_chrome;

// Fixed row map (SPEC §4): art 0–17, desc 19–20, menu 22–26, message 27, status 28, footer 29.
const DESC_ROW: usize = 19;
const MENU_ROW: usize = 22;
pub(crate) const MESSAGE_ROW: usize = 27;

// ---- zone.ron model (SPEC §5.2) ----

#[derive(Deserialize)]
struct Zone {
    start: String,
    areas: HashMap<String, Area>,
    #[serde(default)]
    items: HashMap<String, ItemData>,
    #[serde(default)]
    vendors: HashMap<String, VendorData>,
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
}

// ponytail: gates land with gated secrets (M3); M1 secrets are always visible.
#[derive(Deserialize)]
struct Secret {
    action: Action,
}

#[derive(Deserialize, Clone)]
pub(crate) enum Action {
    Travel(String),
    Say(String),
    Rest,
    Trade(String),
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
    Weapon(i32),
    Armor(i32),
    Misc,
}

#[derive(Deserialize, Clone)]
#[serde(rename = "Vendor")]
pub(crate) struct VendorData {
    pub name: String,
    pub faction: String,
    pub markup: f32,
    pub stock: Vec<(String, u32)>,
}

// name/shelter are read by later milestones (status/map, M3 emissions).
#[allow(dead_code)]
pub(crate) struct AreaData {
    pub name: String,
    pub shelter: bool,
    pub art: Vec<String>,
    pub desc: Vec<String>,
    pub menu: Vec<(String, Action)>,
    pub secrets: HashMap<char, Action>,
    pub secret_cells: Vec<(usize, usize, char)>,
}

#[derive(Resource)]
pub(crate) struct ZoneData {
    pub start: String,
    pub areas: HashMap<String, AreaData>,
    pub items: HashMap<String, ItemData>,
    pub vendors: HashMap<String, VendorData>,
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
pub(crate) struct VendorStock(pub HashMap<String, Vec<(String, u32)>>);

impl FromWorld for VendorStock {
    fn from_world(world: &mut World) -> Self {
        let zone = world.resource::<ZoneData>();
        VendorStock(
            zone.vendors
                .iter()
                .map(|(id, v)| (id.clone(), v.stock.clone()))
                .collect(),
        )
    }
}

// ---- loading ----

pub(crate) fn load_zone(data_dir: &Path) -> ZoneData {
    let zone_path = data_dir.join("zone.ron");
    let text =
        fs::read_to_string(&zone_path).unwrap_or_else(|e| panic!("{}: {e}", zone_path.display()));
    let zone: Zone =
        ron::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", zone_path.display()));

    let mut areas = HashMap::new();
    for (id, a) in zone.areas {
        let art_path = data_dir.join("areas").join(format!("{}.area", a.art));
        let (art, desc, secret_cells) = load_area_file(&art_path);
        let secrets: HashMap<char, Action> =
            a.secrets.into_iter().map(|(k, s)| (k, s.action)).collect();
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
        items: zone.items,
        vendors: zone.vendors,
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
            Action::Say(_) | Action::Rest => {}
        };
        for (id, area) in &self.areas {
            for (label, action) in &area.menu {
                check(action, &format!("{id}/{label}"));
            }
            for (letter, action) in &area.secrets {
                check(action, &format!("{id}/'{letter}'"));
            }
        }
        for (id, v) in &self.vendors {
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
    area_id: &str,
    sel: usize,
    message: &str,
) {
    grid.clear();
    let area = &zone.areas[area_id];

    // Art rows 0–17.
    for (y, line) in area.art.iter().enumerate() {
        for (x, ch) in line.chars().enumerate() {
            let mut g = Glyph {
                ch,
                fg: class_color(ch),
                bold: false,
            };
            if area.secret_cells.iter().any(|&(sx, sy, _)| sx == x && sy == y) {
                g.fg = PALETTE.secret;
                g.bold = true;
            }
            grid.set(x, y, g);
        }
    }

    // Description rows 19–20.
    for (dy, line) in area.desc.iter().enumerate() {
        grid.text(0, DESC_ROW + dy, line, PALETTE.desc, false);
    }

    // Menu rows 22–26.
    for (i, (label, _)) in area.menu.iter().enumerate() {
        let fg = if i == sel { PALETTE.menu_sel } else { PALETTE.menu };
        let prefix = if i == sel { "> " } else { "  " };
        grid.text(0, MENU_ROW + i, prefix, fg, false);
        grid.text(2, MENU_ROW + i, label, fg, false);
    }

    grid.text(0, MESSAGE_ROW, message, PALETTE.desc, false);
    draw_chrome(grid, run);
}

// Character class -> palette color (SPEC §5.1). Anomaly tells render amber.
fn class_color(ch: char) -> Color {
    match ch {
        ' ' => PALETTE.dim,
        '~' => PALETTE.smoke,
        '^' => PALETTE.fire,
        '*' | '+' | '@' | '.' | ':' => PALETTE.amber,
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

    #[test]
    fn loads_zone_data() {
        let zone = load_zone(Path::new("assets/data"));
        assert_eq!(zone.start, "camp");
        assert_eq!(zone.areas.len(), 3);
        assert!(zone.areas["camp"].secrets.contains_key(&'D'));
        assert_eq!(zone.areas["camp"].secret_cells, vec![(13, 14, 'D')]);
        assert!(zone.vendors.contains_key("trader"));
        assert!(zone.items.contains_key("medkit"));
    }
}
