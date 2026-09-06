//! Modal screens and the persistent chrome: character creation, inventory, trade.
//! Every one of these is a `build_*_grid` writing into the single `TileGrid` (SPEC §3).

use crate::area::{ItemKind, VendorData, VendorStock, ZoneData, MESSAGE_ROW};
use crate::render::{TileGrid, PALETTE};
use crate::run::{
    price, RunState, ATTR_NAMES, BACKGROUNDS, BARTER, SKILL_NAMES, TAG_COUNT,
};
use crate::sim::{artifact_rads_per_hour, attr, is_night, GameClock};

const STATUS_ROW: usize = 28;
const FOOTER_ROW: usize = 29;
const HINT_ROW: usize = 25;
const LIST_ROW: usize = 4;
/// Full-width text sits here, clear of the preview column at 44.
const BLURB_ROW: usize = 17;
/// The right-hand column on the creation and inventory screens.
pub(crate) const PANEL_COL: usize = 44;
/// The last row the panel occupies. Below it, text may run the full width.
#[allow(dead_code)] // read by the gutter guard in the playthrough tests
pub(crate) const PANEL_LAST_ROW: usize = 15;

/// Rows 28–29, on every in-run screen (SPEC §4).
pub(crate) fn draw_chrome(grid: &mut TileGrid, run: &RunState, clock: &GameClock) {
    let status = format!(
        "  HP {}/{}  RAD {}  RU {}  Day {} {}",
        run.hp,
        run.max_hp,
        run.rads,
        run.rubles,
        run.day(),
        run.clock()
    );
    grid.text(0, STATUS_ROW, &status, PALETTE.status, false);
    // GDD §3: the status row also carries the dark and the emission warning.
    let mut x = status.chars().count() + 2;
    if is_night(run.minutes) {
        grid.text(x, STATUS_ROW, "[night]", PALETTE.grey, false);
        x += 9;
    }
    if clock.warning(run) {
        grid.text(x, STATUS_ROW, "[emission soon]", PALETTE.red, true);
    }
    let footer = "Tab Inventory   F1 Status   F2 Map   F3 Journal   F5 Save   Esc Back";
    grid.text(0, FOOTER_ROW, footer, PALETTE.menu, false);
}

fn title(grid: &mut TileGrid, s: &str) {
    grid.text(0, 0, s, PALETTE.menu_sel, true);
}

fn hint(grid: &mut TileGrid, s: &str) {
    grid.text(0, HINT_ROW, s, PALETTE.dim, false);
}

/// One selectable list row.
fn row(grid: &mut TileGrid, x: usize, y: usize, selected: bool, s: &str) {
    let fg = if selected { PALETTE.menu_sel } else { PALETTE.menu };
    grid.text(x, y, if selected { ">" } else { " " }, fg, false);
    grid.text(x + 2, y, s, fg, false);
}

// ---- character creation ----

pub(crate) fn build_creation_grid(grid: &mut TileGrid, phase: usize, sel: usize, bg: usize, tags: &[usize]) {
    grid.clear();
    title(grid, "THE ZONE - a new stalker");

    let (preview_bg, preview_tags) = if phase == 0 { (sel, &[][..]) } else { (bg, tags) };
    let preview = RunState::roll(preview_bg, preview_tags);

    if phase == 0 {
        grid.text(0, 2, "Choose a background:", PALETTE.desc, false);
        for (i, b) in BACKGROUNDS.iter().enumerate() {
            row(grid, 0, LIST_ROW + i, i == sel, b.name);
        }
        // Below the preview column, which owns everything from column 44.
        grid.text(0, BLURB_ROW, BACKGROUNDS[sel].blurb, PALETTE.desc, false);
        hint(grid, "Up/Down choose   Enter confirm");
    } else {
        let picked = tags.len();
        let head = format!("Tag {TAG_COUNT} skills, +15 each:  {picked}/{TAG_COUNT}");
        grid.text(0, 2, &head, PALETTE.desc, false);
        grid.text(
            0,
            BLURB_ROW,
            "Tagged skills also rise twice as fast.",
            PALETTE.desc,
            false,
        );
        for (i, name) in SKILL_NAMES.iter().enumerate() {
            let mark = if tags.contains(&i) { "*" } else { " " };
            let label = format!("{mark} {:<16}{:>3}", name, preview.skills[i]);
            row(grid, 0, LIST_ROW + i, i == sel, &label);
        }
        hint(grid, "Up/Down choose   Enter tag or untag   the run starts on the third tag");
    }

    // Live preview of the stalker on the right.
    grid.text(PANEL_COL, 2, BACKGROUNDS[preview_bg].name, PALETTE.menu_sel, true);
    for (i, name) in ATTR_NAMES.iter().enumerate() {
        let line = format!("{name} {}", preview.attrs[i]);
        grid.text(PANEL_COL, LIST_ROW + i, &line, PALETTE.status, false);
    }
    let derived = format!("HP {}   RU {}", preview.max_hp, preview.rubles);
    grid.text(PANEL_COL, LIST_ROW + ATTR_NAMES.len() + 1, &derived, PALETTE.status, false);
    let mut y = LIST_ROW + ATTR_NAMES.len() + 3;
    for (faction, rep) in &preview.rep {
        grid.text(PANEL_COL, y, &format!("{faction} {rep:+}"), PALETTE.desc, false);
        y += 1;
    }
}

// ---- inventory ----

pub(crate) fn build_inventory_grid(
    grid: &mut TileGrid,
    zone: &ZoneData,
    run: &RunState,
    clock: &GameClock,
    sel: usize,
    message: &str,
) {
    grid.clear();
    title(grid, "INVENTORY");

    if run.items.is_empty() {
        grid.text(2, LIST_ROW, "Nothing carried.", PALETTE.desc, false);
    }
    for (i, (id, n)) in run.items.iter().enumerate() {
        let item = &zone.items[id];
        let slot = match (run.weapon.as_deref(), run.armor.as_deref()) {
            (Some(w), _) if w == id => " [wielded]",
            (_, Some(a)) if a == id => " [worn]",
            _ => "",
        };
        let label = format!("{:<20}{:>3}{}", item.name, n, slot);
        row(grid, 0, LIST_ROW + i, i == sel, &label);
    }

    // What the artifacts are costing you, if anything.
    let per_hour = artifact_rads_per_hour(run, zone);
    if per_hour > 0 {
        let line = format!("Artifacts: +{per_hour} RAD every hour");
        grid.text(2, LIST_ROW + run.items.len() + 1, &line, PALETTE.amber, false);
    }

    // Attributes and skills on the right — this is where creation choices show up.
    // Attributes are effective: artifact bonuses in, radiation penalty out (GDD §4, §6).
    grid.text(PANEL_COL, 2, BACKGROUNDS[run.background].name, PALETTE.menu_sel, true);
    for (i, name) in ATTR_NAMES.iter().enumerate() {
        let now = attr(run, zone, i);
        let fg = match now.cmp(&run.attrs[i]) {
            std::cmp::Ordering::Less => PALETTE.red,
            std::cmp::Ordering::Greater => PALETTE.cyan,
            std::cmp::Ordering::Equal => PALETTE.status,
        };
        grid.text(PANEL_COL, LIST_ROW + i, &format!("{name} {now}"), fg, false);
    }
    for (i, name) in SKILL_NAMES.iter().enumerate() {
        let mark = if run.tags[i] { "*" } else { " " };
        let line = format!("{mark} {:<16}{:>3}", name, run.skills[i]);
        grid.text(PANEL_COL + 12, LIST_ROW + i, &line, PALETTE.desc, false);
    }

    grid.text(0, MESSAGE_ROW, message, PALETTE.desc, false);
    hint(grid, "Up/Down select   Enter use or equip   Tab/Esc back");
    draw_chrome(grid, run, clock);
}

/// Uses or equips one item. Returns the message line, and whether anything actually
/// happened - a medkit at full health costs no AP because it never left the pack.
pub(crate) fn use_item(zone: &ZoneData, run: &mut RunState, index: usize) -> (String, bool) {
    let Some((id, _)) = run.items.get(index).cloned() else {
        return (String::new(), false);
    };
    let item = zone.items[&id].clone();
    match item.kind {
        ItemKind::Heal(n) => {
            let healed = (run.max_hp - run.hp).min(n);
            if healed == 0 {
                return ("You are not hurt.".into(), false);
            }
            run.hp += healed;
            run.take_item(&id, 1);
            (format!("You use the {}. HP +{healed}.", item.name), true)
        }
        ItemKind::Antirad(n) => {
            if run.rads == 0 {
                return ("You are clean.".into(), false);
            }
            let cleared = run.rads.min(n);
            run.rads -= cleared;
            run.take_item(&id, 1);
            (format!("You use the {}. RAD -{cleared}.", item.name), true)
        }
        ItemKind::Weapon { .. } => {
            let off = run.weapon.as_deref() == Some(id.as_str());
            run.weapon = if off { None } else { Some(id) };
            (format!("You {} the {}.", if off { "stow" } else { "ready" }, item.name), true)
        }
        ItemKind::Armor(_) => {
            let off = run.armor.as_deref() == Some(id.as_str());
            run.armor = if off { None } else { Some(id) };
            (format!("You {} the {}.", if off { "take off" } else { "put on" }, item.name), true)
        }
        // Artifacts work by being carried; there is nothing to press (GDD §6).
        ItemKind::Artifact { rads, .. } => (
            format!("The {} hums against your hip. {rads} rads an hour.", item.name),
            false,
        ),
        ItemKind::Light => (format!("The {} is on whenever you carry it.", item.name), false),
        ItemKind::Misc => (format!("The {} is not much use here.", item.name), false),
    }
}

// ---- trade ----

/// The list the trade screen is showing: vendor stock when buying, your pack when selling.
pub(crate) fn trade_list<'a>(
    stock: &'a VendorStock,
    run: &'a RunState,
    vendor_id: &str,
    buying: bool,
) -> &'a Vec<(String, u32)> {
    if buying {
        &stock.0[vendor_id]
    } else {
        &run.items
    }
}

/// Vendor specialty (GDD §7): the trader's artifact mark-up rides on top of its own.
fn markup_for(vendor: &VendorData, kind: ItemKind) -> f32 {
    match kind {
        ItemKind::Artifact { .. } => vendor.markup * vendor.artifact_markup,
        _ => vendor.markup,
    }
}

fn rep_word(rep: i32) -> &'static str {
    match rep {
        r if r < -50 => "hostile",
        r if r < -10 => "unfriendly",
        r if r > 75 => "allied",
        r if r > 25 => "friendly",
        _ => "neutral",
    }
}

pub(crate) fn build_trade_grid(
    grid: &mut TileGrid,
    zone: &ZoneData,
    stock: &VendorStock,
    run: &RunState,
    clock: &GameClock,
    vendor_id: &str,
    buying: bool,
    sel: usize,
    message: &str,
) {
    grid.clear();
    let vendor = &zone.vendors[vendor_id];
    let rep = run.rep_of(&vendor.faction);
    title(grid, &format!("TRADE - {}", vendor.name));
    let head = format!(
        "Barter {}   {} {rep:+} ({})",
        run.skills[BARTER],
        vendor.faction,
        rep_word(rep)
    );
    grid.text(PANEL_COL, 0, &head, PALETTE.status, false);

    let (buy_fg, sell_fg) = if buying {
        (PALETTE.menu_sel, PALETTE.menu)
    } else {
        (PALETTE.menu, PALETTE.menu_sel)
    };
    grid.text(0, 2, if buying { "> BUY" } else { "  BUY" }, buy_fg, buying);
    grid.text(10, 2, if buying { "  SELL" } else { "> SELL" }, sell_fg, !buying);

    let list = trade_list(stock, run, vendor_id, buying);
    if list.is_empty() {
        let empty = if buying { "The trader has nothing left." } else { "You have nothing to sell." };
        grid.text(2, LIST_ROW, empty, PALETTE.desc, false);
    }
    for (i, (id, n)) in list.iter().enumerate() {
        let item = &zone.items[id];
        let p = price(
            item.base,
            markup_for(vendor, item.kind),
            run.skills[BARTER],
            rep,
            buying,
        );
        let p = p.map_or("--".to_string(), |v| v.to_string());
        let label = format!("{:<22}{:>3}{:>9} RU", item.name, n, p);
        row(grid, 0, LIST_ROW + i, i == sel, &label);
    }

    grid.text(0, MESSAGE_ROW, message, PALETTE.desc, false);
    hint(grid, "Left/Right buy or sell   Up/Down select   Enter confirm   Esc leave");
    draw_chrome(grid, run, clock);
}

/// Buys or sells one unit of the selected row. Returns the message line.
pub(crate) fn trade_one(
    zone: &ZoneData,
    stock: &mut VendorStock,
    run: &mut RunState,
    vendor_id: &str,
    buying: bool,
    sel: usize,
) -> String {
    let vendor = &zone.vendors[vendor_id];
    let rep = run.rep_of(&vendor.faction);

    let list = trade_list(stock, run, vendor_id, buying);
    let Some((id, _)) = list.get(sel).cloned() else {
        return String::new();
    };
    let item = &zone.items[&id];
    let Some(p) = price(
        item.base,
        markup_for(vendor, item.kind),
        run.skills[BARTER],
        rep,
        buying,
    ) else {
        return format!("{} will not trade with you.", vendor.name);
    };

    if buying {
        if run.rubles < p {
            return format!("You cannot afford the {}.", item.name);
        }
        run.rubles -= p;
        run.add_item(&id, 1);
        take_stock(stock, vendor_id, &id);
        format!("You buy the {} for {p} RU.", item.name)
    } else {
        if !run.take_item(&id, 1) {
            return String::new();
        }
        run.rubles += p;
        let shelf = stock.0.get_mut(vendor_id).expect("vendor stock");
        match shelf.iter_mut().find(|(i, _)| *i == id) {
            Some(slot) => slot.1 += 1,
            None => shelf.push((id.clone(), 1)),
        }
        format!("You sell the {} for {p} RU.", item.name)
    }
}

fn take_stock(stock: &mut VendorStock, vendor_id: &str, id: &str) {
    let shelf = stock.0.get_mut(vendor_id).expect("vendor stock");
    if let Some(i) = shelf.iter().position(|(x, _)| x == id) {
        shelf[i].1 -= 1;
        if shelf[i].1 == 0 {
            shelf.remove(i);
        }
    }
}

// ---- the end of a run ----

/// `ponytail:` the memorial, meta-progress and rolling a new stalker are M6.
/// This screen exists so a dead stalker stops playing.
pub(crate) fn build_gameover_grid(grid: &mut TileGrid, run: &RunState, cause: &str) {
    grid.clear();
    grid.text(0, 6, "THE ZONE IS STILL THERE", PALETTE.red, true);
    grid.text(0, 8, cause, PALETTE.desc, false);
    let epitaph = format!(
        "{}, {} days in, {} rads, {} RU on you.",
        BACKGROUNDS[run.background].name,
        run.day(),
        run.rads,
        run.rubles
    );
    grid.text(0, 10, &epitaph, PALETTE.grey, false);
    hint(grid, "Esc quit");
}

// ---- tests (SPEC §7) ----

#[cfg(test)]
mod tests {
    use super::*;
    use crate::area::load_zone;
    use std::collections::HashMap;
    use std::path::Path;

    fn fixture() -> (ZoneData, VendorStock, RunState) {
        let zone = load_zone(Path::new("assets/data"));
        let stock = VendorStock(
            zone.vendors
                .iter()
                .map(|(id, v)| (id.clone(), v.stock.clone()))
                .collect(),
        );
        (zone, stock, RunState::roll(0, &[8, 9, 4]))
    }

    #[test]
    fn buying_then_selling_back_loses_money_and_conserves_goods() {
        let (zone, mut stock, mut run) = fixture();
        let before = run.rubles;
        // "bolt" is the cheapest thing on the shelf; find its row in buy mode.
        let sel = stock.0["trader"].iter().position(|(i, _)| i == "bolt").unwrap();
        let held = run.items.iter().find(|(i, _)| i == "bolt").map_or(0, |(_, n)| *n);

        trade_one(&zone, &mut stock, &mut run, "trader", true, sel);
        assert_eq!(
            run.items.iter().find(|(i, _)| i == "bolt").unwrap().1,
            held + 1
        );
        assert!(run.rubles < before);

        let sel = run.items.iter().position(|(i, _)| i == "bolt").unwrap();
        trade_one(&zone, &mut stock, &mut run, "trader", false, sel);
        assert_eq!(
            run.items.iter().find(|(i, _)| i == "bolt").unwrap().1,
            held
        );
        assert!(run.rubles < before, "the margin is the vendor's cut");
    }

    #[test]
    fn cannot_buy_without_the_rubles() {
        let (zone, mut stock, mut run) = fixture();
        run.rubles = 0;
        let sel = stock.0["trader"].iter().position(|(i, _)| i == "bolt").unwrap();
        let held = run.items.iter().find(|(i, _)| i == "bolt").map_or(0, |(_, n)| *n);
        let msg = trade_one(&zone, &mut stock, &mut run, "trader", true, sel);
        assert!(msg.contains("cannot afford"));
        assert_eq!(run.items.iter().find(|(i, _)| i == "bolt").unwrap().1, held);
        assert_eq!(run.rubles, 0);
    }

    #[test]
    fn hostile_vendor_refuses() {
        let (zone, mut stock, mut run) = fixture();
        run.rep = HashMap::from([("loners".to_string(), -80)]);
        let msg = trade_one(&zone, &mut stock, &mut run, "trader", true, 0);
        assert!(msg.contains("will not trade"));
    }

    /// Every screen writes into the one grid; this fails if a build function
    /// panics on a lookup or runs off the end of a row.
    #[test]
    fn every_screen_builds() {
        use crate::area::build_area_grid;
        use crate::render::TileGrid;

        let (zone, stock, run) = fixture();
        let mut grid = TileGrid::new(crate::render::GRID_W, crate::render::GRID_H);

        let clock = GameClock::default();
        let mut fields = crate::sim::Fields::default();

        build_creation_grid(&mut grid, 0, 3, 0, &[]);
        build_creation_grid(&mut grid, 1, 9, 2, &[8, 9]);
        for id in zone.areas.keys() {
            build_area_grid(&mut grid, &zone, &run, &clock, &fields, id, 0, "test");
        }
        // Again with the field scanned and its artifact showing, so the extra
        // menu entry and the lit-up anomaly tells are drawn at least once.
        fields.0.insert(
            "field".into(),
            crate::sim::FieldState { scanned: true, artifact: true, ..Default::default() },
        );
        build_area_grid(&mut grid, &zone, &run, &clock, &fields, "field", 4, "test");
        build_gameover_grid(&mut grid, &run, "test");
        build_trade_grid(&mut grid, &zone, &stock, &run, &clock, "trader", true, 0, "test");
        build_trade_grid(&mut grid, &zone, &stock, &run, &clock, "trader", false, 0, "test");
        // Last, so the status row below is the one this screen drew.
        build_inventory_grid(&mut grid, &zone, &run, &clock, 0, "test");

        // The status row is the one thing every in-run screen must carry (SPEC §4).
        let status: String = (0..grid.w).map(|x| grid.cells[STATUS_ROW * grid.w + x].ch).collect();
        assert!(status.contains("HP 38/38"), "{status}");
        assert!(status.contains("Day 1 06:00"), "{status}");
    }

    #[test]
    fn heal_is_capped_and_consumes_the_item() {
        let (zone, _, mut run) = fixture();
        run.hp = run.max_hp - 2;
        let i = run.items.iter().position(|(id, _)| id == "medkit").unwrap();
        let (msg, acted) = use_item(&zone, &mut run, i);
        assert!(acted);
        assert_eq!(run.hp, run.max_hp);
        assert!(msg.contains("HP +2"));
        assert!(!run.items.iter().any(|(id, _)| id == "medkit"));
    }
}
