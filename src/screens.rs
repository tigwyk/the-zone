//! Modal screens and the persistent chrome: character creation, inventory, trade.
//! Every one of these is a `build_*_grid` writing into the single `TileGrid` (SPEC §3).

use crate::area::{ItemKind, VendorData, VendorStock, ZoneData, MESSAGE_ROW};
use crate::render::{TileGrid, PALETTE};
use crate::loot::{self, AffixData, Effect, ItemStack, Rarity};
use crate::meta::MetaProgress;
use crate::run::{
    price, Rng, RunState, ATTR_NAMES, BACKGROUNDS, BARTER, SKILL_NAMES, TAG_COUNT,
};
use crate::sim::{artifact_rads_per_hour, attr, is_night, GameClock};

const STATUS_ROW: usize = 28;
const FOOTER_ROW: usize = 29;
const HINT_ROW: usize = 25;
const LIST_ROW: usize = 4;
/// How many rows each long list gets before it starts scrolling.
const PACK_ROWS: usize = 16;
const MAP_ROWS: usize = 18;
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
    // The footer tells the truth: F1 Status was advertised for six milestones and
    // never existed, and the inventory panel already shows everything it would.
    let footer = "Tab Inventory   F2 Map   F3 Journal   F4 Scanlines   F5 Save   Esc Back";
    grid.text(0, FOOTER_ROW, footer, PALETTE.menu, false);
}

pub(crate) fn title(grid: &mut TileGrid, s: &str) {
    grid.text(0, 0, s, PALETTE.menu_sel, true);
}

/// The slice of a long list to draw so the cursor stays on screen. Thirty areas and
/// forty items do not fit in twenty rows, and a list that runs off the grid is the
/// same bug as a message that does.
pub(crate) fn window(sel: usize, total: usize, rows: usize) -> std::ops::Range<usize> {
    if total <= rows {
        return 0..total;
    }
    let start = sel.saturating_sub(rows / 2).min(total - rows);
    start..start + rows
}

/// Drawn at the end of a windowed list when there is more of it above or below.
pub(crate) fn more(grid: &mut TileGrid, y: usize, shown: &std::ops::Range<usize>, total: usize) {
    if total <= shown.len() {
        return;
    }
    let line = format!("({}-{} of {total})", shown.start + 1, shown.end);
    grid.text(2, y, &line, PALETTE.dim, false);
}

/// The message row (SPEC §4). A message is meant to fit in one line; when one has
/// been built out of several - a hit, the reply, and a job settling in the same
/// breath - it says so rather than falling off the edge of the grid.
pub(crate) fn draw_message(grid: &mut TileGrid, message: &str) {
    let width = crate::render::GRID_W;
    if message.chars().count() <= width {
        grid.text(0, MESSAGE_ROW, message, PALETTE.desc, false);
        return;
    }
    let cut: String = message.chars().take(width - 3).collect();
    grid.text(0, MESSAGE_ROW, &format!("{cut}..."), PALETTE.desc, false);
}

pub(crate) fn hint(grid: &mut TileGrid, s: &str) {
    grid.text(0, HINT_ROW, s, PALETTE.dim, false);
}

/// Greedy word wrap. Prose written for the screen is short; this is for the lines
/// that come out of dialogue and job text, which are not.
pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        let line = lines.last_mut().expect("never empty");
        if line.is_empty() {
            line.push_str(word);
        } else if line.chars().count() + 1 + word.chars().count() <= width {
            line.push(' ');
            line.push_str(word);
        } else {
            lines.push(word.to_string());
        }
    }
    lines
}

/// One selectable list row.
fn row(grid: &mut TileGrid, x: usize, y: usize, selected: bool, s: &str) {
    let fg = if selected { PALETTE.menu_sel } else { PALETTE.menu };
    grid.text(x, y, if selected { ">" } else { " " }, fg, false);
    grid.text(x + 2, y, s, fg, false);
}

// ---- character creation ----

pub(crate) fn build_creation_grid(
    grid: &mut TileGrid,
    meta: &MetaProgress,
    phase: usize,
    sel: usize,
    bg: usize,
    tags: &[usize],
    message: &str,
) {
    grid.clear();
    title(grid, "THE ZONE - a new stalker");
    draw_message(grid, message);

    let (preview_bg, preview_tags) = if phase == 0 { (sel, &[][..]) } else { (bg, tags) };
    // A throwaway die, so drawing the preview does not spend the run's own rolls.
    let preview = RunState::roll(preview_bg, preview_tags, &mut Rng::new(1));

    if phase == 0 {
        grid.text(0, 2, "Choose a background:", PALETTE.desc, false);
        for (i, b) in BACKGROUNDS.iter().enumerate() {
            // A locked background still shows: it is something to go and earn.
            if meta.unlocked(i) {
                row(grid, 0, LIST_ROW + i, i == sel, b.name);
            } else {
                let fg = if i == sel { PALETTE.grey } else { PALETTE.dim };
                grid.text(0, LIST_ROW + i, if i == sel { "> " } else { "  " }, fg, false);
                grid.text(2, LIST_ROW + i, &format!("{} (locked)", b.name), fg, false);
            }
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
    let shown = window(sel, run.items.len(), PACK_ROWS);
    for (line_no, i) in shown.clone().enumerate() {
        let stack = &run.items[i];
        let slot = match (run.weapon, run.armor) {
            (Some(w), _) if w == stack.uid => " [wielded]",
            (_, Some(a)) if a == stack.uid => " [worn]",
            _ => "",
        };
        let count = if stack.count > 1 { format!("{:>3}", stack.count) } else { "   ".into() };
        let label = format!("{:<34}{count}{slot}", loot::display_name(stack, zone));
        // The colour is the rarity: how much of the Zone got into it.
        let rarity = stack.rarity();
        let fg = if i == sel { PALETTE.menu_sel } else { rarity.color() };
        grid.text(0, LIST_ROW + line_no, if i == sel { "> " } else { "  " }, fg, false);
        grid.text(2, LIST_ROW + line_no, &label, fg, rarity >= Rarity::Warped);
    }
    more(grid, LIST_ROW + PACK_ROWS, &shown, run.items.len());

    // The highlighted thing, read out: what it is and what the Zone put on it.
    if let Some(stack) = run.items.get(sel) {
        let mut y = LIST_ROW + PACK_ROWS + 2;
        let rarity = stack.rarity();
        let head = if rarity == Rarity::Plain {
            format!("{}   {} RU", loot::display_name(stack, zone), loot::value(stack, zone))
        } else {
            format!(
                "{}   {}   {} RU",
                loot::display_name(stack, zone),
                rarity.name(),
                loot::value(stack, zone)
            )
        };
        grid.text(2, y, &head, rarity.color(), rarity >= Rarity::Warped);
        y += 1;
        for roll in &stack.affixes {
            if let Some(affix) = zone.affixes.get(&roll.affix) {
                grid.text(4, y, &affix_line(affix, roll.magnitude), PALETTE.desc, false);
                y += 1;
            }
        }
    }

    // What the artifacts are costing you, if anything.
    let per_hour = artifact_rads_per_hour(run, zone);
    if per_hour > 0 {
        let line = format!("Artifacts: +{per_hour} RAD every hour");
        grid.text(2, LIST_ROW + PACK_ROWS + 1, &line, PALETTE.amber, false);
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

    draw_message(grid, message);
    hint(grid, "Up/Down select   Enter use or equip   Tab/Esc back");
    draw_chrome(grid, run, clock);
}

/// One roll in words: what it does and by how much.
fn affix_line(affix: &AffixData, magnitude: i32) -> String {
    let what = match affix.effect {
        Effect::Damage => "damage".to_string(),
        Effect::ToHit => "to hit".to_string(),
        Effect::Armor => "damage resistance".to_string(),
        Effect::Crit => "crit range".to_string(),
        Effect::Attr(i) => ATTR_NAMES[i].to_string(),
        Effect::Rads => "rads an hour".to_string(),
    };
    format!("{magnitude:+} {what}")
}

/// Uses or equips one item. Returns the message line, and whether anything actually
/// happened - a medkit at full health costs no AP because it never left the pack.
pub(crate) fn use_item(zone: &ZoneData, run: &mut RunState, index: usize) -> (String, bool) {
    let Some(stack) = run.items.get(index).cloned() else {
        return (String::new(), false);
    };
    let id = stack.id.clone();
    let uid = stack.uid;
    let name = loot::display_name(&stack, zone);
    let item = zone.items[&id].clone();
    match item.kind {
        ItemKind::Heal(n) => {
            let healed = (run.max_hp - run.hp).min(n);
            if healed == 0 {
                return ("You are not hurt.".into(), false);
            }
            run.hp += healed;
            run.take_item(&id, 1);
            (format!("You use the {name}. HP +{healed}."), true)
        }
        ItemKind::Antirad(n) => {
            if run.rads == 0 {
                return ("You are clean.".into(), false);
            }
            let cleared = run.rads.min(n);
            run.rads -= cleared;
            run.take_item(&id, 1);
            (format!("You use the {name}. RAD -{cleared}."), true)
        }
        ItemKind::Weapon { .. } => {
            let off = run.weapon == Some(uid);
            run.weapon = if off { None } else { Some(uid) };
            (format!("You {} the {name}.", if off { "stow" } else { "ready" }), true)
        }
        ItemKind::Armor(_) => {
            let off = run.armor == Some(uid);
            run.armor = if off { None } else { Some(uid) };
            (format!("You {} the {name}.", if off { "take off" } else { "put on" }), true)
        }
        // Artifacts work by being carried; there is nothing to press (GDD §6).
        ItemKind::Artifact { rads, .. } => (
            format!("The {name} hums against your hip. {rads} rads an hour."),
            false,
        ),
        ItemKind::Light => (format!("The {name} is on whenever you carry it."), false),
        ItemKind::Misc => (format!("The {name} is not much use here."), false),
    }
}

// ---- trade ----

/// The list the trade screen is showing: vendor stock when buying, your pack when selling.
pub(crate) fn trade_list<'a>(
    stock: &'a VendorStock,
    run: &'a RunState,
    vendor_id: &str,
    buying: bool,
) -> &'a Vec<ItemStack> {
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
    for (i, stack) in list.iter().enumerate() {
        let item = &zone.items[&stack.id];
        let p = price(
            loot::value(stack, zone),
            markup_for(vendor, item.kind),
            run.skills[BARTER],
            rep,
            buying,
        );
        let p = p.map_or("--".to_string(), |v| v.to_string());
        let count = if stack.count > 1 { format!("{:>3}", stack.count) } else { "   ".into() };
        let label = format!("{:<30}{count}{p:>9} RU", loot::display_name(stack, zone));
        let rarity = stack.rarity();
        let fg = if i == sel { PALETTE.menu_sel } else { rarity.color() };
        grid.text(0, LIST_ROW + i, if i == sel { "> " } else { "  " }, fg, false);
        grid.text(2, LIST_ROW + i, &label, fg, rarity >= Rarity::Warped);
    }

    draw_message(grid, message);
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
    let Some(stack) = list.get(sel).cloned() else {
        return String::new();
    };
    let item = &zone.items[&stack.id];
    let name = loot::display_name(&stack, zone);
    let Some(p) = price(
        loot::value(&stack, zone),
        markup_for(vendor, item.kind),
        run.skills[BARTER],
        rep,
        buying,
    ) else {
        return format!("{} will not trade with you.", vendor.name);
    };

    if buying {
        if run.rubles < p {
            return format!("You cannot afford the {name}.");
        }
        run.rubles -= p;
        // One off the shelf, with whatever is on it, and a uid of its own.
        let uid = run.next_uid();
        let bought = ItemStack { uid, count: 1, ..stack.clone() };
        run.add_stack(bought);
        take_one(stock, vendor_id, sel);
        format!("You buy the {name} for {p} RU.")
    } else {
        let Some(sold) = run.take_uid(stack.uid) else {
            return String::new();
        };
        run.rubles += p;
        let shelf = stock.0.get_mut(vendor_id).expect("vendor stock");
        match shelf.iter_mut().find(|s| s.id == sold.id && s.affixes == sold.affixes) {
            Some(slot) => slot.count += 1,
            None => shelf.push(sold),
        }
        format!("You sell the {name} for {p} RU.")
    }
}

fn take_one(stock: &mut VendorStock, vendor_id: &str, index: usize) {
    let shelf = stock.0.get_mut(vendor_id).expect("vendor stock");
    if index >= shelf.len() {
        return;
    }
    shelf[index].count -= 1;
    if shelf[index].count == 0 {
        shelf.remove(index);
    }
}

// ---- the map (GDD §5) ----

/// Areas you have been to, each with the ways out that you know about.
/// `ponytail:` generated, not a hand-drawn `map.area` scene, so it cannot hide a
/// letter yet. Authoring one is a content job; the mechanic below would not change.
pub(crate) fn known_areas(zone: &ZoneData, run: &RunState) -> Vec<String> {
    let mut ids: Vec<String> = run
        .discovered
        .iter()
        .filter(|id| zone.areas.contains_key(*id))
        .cloned()
        .collect();
    ids.sort_unstable();
    ids
}

pub(crate) fn build_map_grid(
    grid: &mut TileGrid,
    zone: &ZoneData,
    run: &RunState,
    clock: &GameClock,
    here: &str,
    sel: usize,
    message: &str,
) {
    grid.clear();
    title(grid, "THE ZONE - what you know of it");

    let known = known_areas(zone, run);
    let next_door = crate::area::exits(&zone.areas[here]);
    let shown = window(sel, known.len(), MAP_ROWS);
    for (line_no, i) in shown.clone().enumerate() {
        let id = &known[i];
        let area = &zone.areas[id];
        let selected = i == sel;
        let standing = if id == here {
            "you are here"
        } else if next_door.contains(id) {
            "next door"
        } else {
            ""
        };
        let fg = if selected { PALETTE.menu_sel } else { PALETTE.menu };
        grid.text(0, LIST_ROW + line_no, if selected { "> " } else { "  " }, fg, false);
        grid.text(2, LIST_ROW + line_no, &format!("{:<24}{standing}", area.name), fg, false);

        // The network: where this one leads, as far as you have found out.
        let onward: Vec<&str> = crate::area::exits(area)
            .iter()
            .filter(|d| run.discovered.contains(*d))
            .map(|d| zone.areas[d].name.as_str())
            .collect();
        if !onward.is_empty() {
            let line = format!("-> {}", onward.join(", "));
            grid.text(40, LIST_ROW + line_no, &line, PALETTE.dim, false);
        }
    }
    more(grid, LIST_ROW + MAP_ROWS, &shown, known.len());

    draw_message(grid, message);
    hint(grid, "Up/Down read   Enter walk there if it is next door   Esc back");
    draw_chrome(grid, run, clock);
}

// ---- the end of a run ----

/// `ponytail:` the memorial, meta-progress and rolling a new stalker are M6.
/// This screen exists so a dead stalker stops playing.
pub(crate) fn build_gameover_grid(grid: &mut TileGrid, run: &RunState, cause: &str) {
    grid.clear();
    match &run.ending {
        Some(name) => {
            grid.text(0, 4, name, PALETTE.cyan, true);
            grid.text(0, 6, "THE ZONE IS STILL THERE", PALETTE.red, true);
        }
        None => grid.text(0, 6, "THE ZONE IS STILL THERE", PALETTE.red, true),
    }
    for (i, line) in wrap(cause, 76).iter().enumerate() {
        grid.text(0, 8 + i, line, PALETTE.desc, false);
    }
    let epitaph = format!(
        "{}, the {}. {} days in, {} rads, {} RU on you.",
        run.name,
        BACKGROUNDS[run.background].name,
        run.day(),
        run.rads,
        run.rubles
    );
    grid.text(0, 16, &epitaph, PALETTE.grey, false);
    grid.text(0, 18, "The next one will read your name at the camp.", PALETTE.dim, false);
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
                .map(|(id, v)| (id.clone(), crate::area::shelf(v)))
                .collect(),
        );
        (zone, stock, RunState::roll(0, &[8, 9, 4], &mut crate::run::Rng::new(3)))
    }

    #[test]
    fn buying_then_selling_back_loses_money_and_conserves_goods() {
        let (zone, mut stock, mut run) = fixture();
        let before = run.rubles;
        // "bolt" is the cheapest thing on the shelf; find its row in buy mode.
        let sel = stock.0["trader"].iter().position(|s| s.id == "bolt").unwrap();
        let held = run.count_of("bolt");

        trade_one(&zone, &mut stock, &mut run, "trader", true, sel);
        assert_eq!(run.count_of("bolt"), held + 1);
        assert!(run.rubles < before);

        let sel = run.items.iter().position(|s| s.id == "bolt").unwrap();
        trade_one(&zone, &mut stock, &mut run, "trader", false, sel);
        assert_eq!(run.count_of("bolt"), held);
        assert!(run.rubles < before, "the margin is the vendor's cut");
    }

    #[test]
    fn cannot_buy_without_the_rubles() {
        let (zone, mut stock, mut run) = fixture();
        run.rubles = 0;
        let sel = stock.0["trader"].iter().position(|s| s.id == "bolt").unwrap();
        let held = run.count_of("bolt");
        let msg = trade_one(&zone, &mut stock, &mut run, "trader", true, sel);
        assert!(msg.contains("cannot afford"));
        assert_eq!(run.count_of("bolt"), held);
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

        build_creation_grid(&mut grid, &MetaProgress::default(), 0, 3, 0, &[], "");
        build_creation_grid(&mut grid, &MetaProgress::default(), 1, 9, 2, &[8, 9], "");
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
    fn a_long_list_scrolls_to_keep_the_cursor_on_screen() {
        // Short enough to fit: everything, and no scroll note.
        assert_eq!(window(0, 5, 10), 0..5);
        assert_eq!(window(4, 5, 10), 0..5);

        // Longer than the rows: the window follows the cursor and stops at the ends.
        assert_eq!(window(0, 30, 10), 0..10);
        assert_eq!(window(15, 30, 10), 10..20);
        assert_eq!(window(29, 30, 10), 20..30);
        // Never past the end, whatever the cursor claims.
        assert_eq!(window(99, 30, 10), 20..30);
        for sel in 0..30 {
            let w = window(sel, 30, 10);
            assert_eq!(w.len(), 10);
            assert!(w.contains(&sel), "cursor {sel} fell out of {w:?}");
        }
        assert_eq!(window(0, 0, 10), 0..0, "an empty list is not a panic");
    }

    #[test]
    fn a_long_message_says_it_was_cut_instead_of_falling_off_the_grid() {
        use crate::render::{TileGrid, GRID_W};
        let mut grid = TileGrid::new(GRID_W, crate::render::GRID_H);

        // Combat builds a line out of several events, and it can outgrow the row.
        let long = "x".repeat(GRID_W + 40);
        draw_message(&mut grid, &long);
        let row: String = (0..GRID_W)
            .map(|x| grid.cells[MESSAGE_ROW * GRID_W + x].ch)
            .collect();
        assert!(row.ends_with("..."), "{row}");
        assert_eq!(row.chars().count(), GRID_W);

        // A message that fits is left exactly as written. (Every real screen clears
        // the grid before it draws; this stands in for that.)
        grid.clear();
        draw_message(&mut grid, "You pry open the hatch.");
        let row: String = (0..GRID_W)
            .map(|x| grid.cells[MESSAGE_ROW * GRID_W + x].ch)
            .collect();
        assert_eq!(row.trim_end(), "You pry open the hatch.");
    }

    #[test]
    fn heal_is_capped_and_consumes_the_item() {
        let (zone, _, mut run) = fixture();
        run.hp = run.max_hp - 2;
        let i = run.items.iter().position(|s| s.id == "medkit").unwrap();
        let (msg, acted) = use_item(&zone, &mut run, i);
        assert!(acted);
        assert_eq!(run.hp, run.max_hp);
        assert!(msg.contains("HP +2"));
        assert_eq!(run.count_of("medkit"), 0);
    }
}
