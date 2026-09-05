//! Area data, the `.area`/`zone.ron` loaders, and the scene build functions.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use bevy::prelude::*;
use serde::Deserialize;

use crate::render::{Glyph, TileGrid, PALETTE};

// Fixed row map (SPEC §4): art 0–17, desc 19–20, menu 22–26, message 27, status 28, footer 29.
const DESC_ROW: usize = 19;
const MENU_ROW: usize = 22;
const MESSAGE_ROW: usize = 27;
const STATUS_ROW: usize = 28;
const FOOTER_ROW: usize = 29;

// ---- zone.ron model (SPEC §5.2) ----

#[derive(Deserialize)]
struct Zone {
    start: String,
    areas: HashMap<String, Area>,
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
}

impl FromWorld for ZoneData {
    fn from_world(_world: &mut World) -> Self {
        load_zone(Path::new("assets/data"))
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
    ZoneData {
        start: zone.start,
        areas,
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
    area_id: &str,
    sel: usize,
    message: &str,
) {
    for c in grid.cells.iter_mut() {
        *c = Glyph {
            ch: ' ',
            fg: PALETTE.dim,
            bold: false,
        };
    }

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
        for (x, ch) in line.chars().enumerate() {
            grid.set(x, DESC_ROW + dy, Glyph { ch, fg: PALETTE.desc, bold: false });
        }
    }

    // Menu rows 22–26.
    for (i, (label, _)) in area.menu.iter().enumerate() {
        let y = MENU_ROW + i;
        let fg = if i == sel { PALETTE.menu_sel } else { PALETTE.menu };
        let prefix = if i == sel { "> " } else { "  " };
        for (dx, ch) in prefix.chars().enumerate() {
            grid.set(dx, y, Glyph { ch, fg, bold: false });
        }
        for (dx, ch) in label.chars().enumerate() {
            grid.set(2 + dx, y, Glyph { ch, fg, bold: false });
        }
    }

    // Message row 27.
    for (x, ch) in message.chars().enumerate() {
        grid.set(x, MESSAGE_ROW, Glyph { ch, fg: PALETTE.desc, bold: false });
    }

    draw_chrome(grid);
}

pub(crate) fn build_inventory_grid(grid: &mut TileGrid) {
    for c in grid.cells.iter_mut() {
        *c = Glyph {
            ch: ' ',
            fg: PALETTE.dim,
            bold: false,
        };
    }
    for (x, ch) in "INVENTORY".chars().enumerate() {
        grid.set(x, 0, Glyph { ch, fg: PALETTE.menu_sel, bold: true });
    }
    for (x, ch) in "Nothing carried.".chars().enumerate() {
        grid.set(x, 2, Glyph { ch, fg: PALETTE.desc, bold: false });
    }
    draw_chrome(grid);
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

fn draw_chrome(grid: &mut TileGrid) {
    // Status row 28 — placeholder until RunState lands (M2).
    let status = "  HP --  RAD --  AP --  Day --";
    for (x, ch) in status.chars().enumerate() {
        grid.set(x, STATUS_ROW, Glyph { ch, fg: PALETTE.status, bold: false });
    }
    // Footer row 29.
    let footer = "Tab Inventory   F1 Status   F2 Map   F3 Journal   F5 Save   Esc Back";
    for (x, ch) in footer.chars().enumerate() {
        grid.set(x, FOOTER_ROW, Glyph { ch, fg: PALETTE.menu, bold: false });
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
    }
}
