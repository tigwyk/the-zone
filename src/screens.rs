//! Modal screens and the persistent chrome: character creation, inventory, trade.
//! Every one of these is a `build_*_grid` writing into the single `TileGrid` (SPEC §3).

use crate::area::{Caliber, ItemKind, VendorData, VendorStock, ZoneData, MESSAGE_ROW};
use bevy::prelude::Color;
use crate::render::{TileGrid, PALETTE};
use crate::loot::{self, AffixData, Effect, Fits, ItemStack, Rarity, MOD_SLOTS};
use crate::meta::MetaProgress;
use crate::run::{
    check_skill, price, Outcome, Rng, RunState, Skill, ATTR_NAMES, BACKGROUNDS, BARTER,
    SKILL_NAMES, TAG_COUNT,
};
use crate::sim::{artifact_rads_per_hour, attr, is_night, GameClock};

const STATUS_ROW: usize = 30;
const FOOTER_ROW: usize = 32;
const HINT_ROW: usize = 25;
const LIST_ROW: usize = 4;
/// How many rows each long list gets before it starts scrolling.
const PACK_ROWS: usize = 16;
const MAP_ROWS: usize = 18;
/// Full-width text sits here, clear of the preview column at 84.
const BLURB_ROW: usize = 17;
/// The right-hand column on the creation and inventory screens.
pub(crate) const PANEL_COL: usize = 84;
/// The last row the panel occupies. Below it, text may run the full width.
/// Read by the gutter guard in the playthrough tests, and nowhere else.
#[cfg(test)]
pub(crate) const PANEL_LAST_ROW: usize = 15;

/// Rows 30 and 32, on every in-run screen; 31 is the gap between them (SPEC §4).
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

/// One selectable list row: the marker in the gutter, the label at `x + 2`. `fg` is
/// the colour when this is not the selected row; the selected row is always menu_sel.
/// Every list in the game draws its rows through here.
pub(crate) fn row(grid: &mut TileGrid, x: usize, y: usize, sel: bool, fg: Color, bold: bool, s: &str) {
    let fg = if sel { PALETTE.menu_sel } else { fg };
    grid.text(x, y, if sel { "> " } else { "  " }, fg, false);
    grid.text(x + 2, y, s, fg, bold);
}

// ---- the main menu ----

pub(crate) fn build_main_menu_grid(
    grid: &mut TileGrid,
    entries: &[&str],
    sel: usize,
    message: &str,
) {
    grid.clear();
    title(grid, "THE ZONE");
    grid.text(0, 2, "Nobody comes back the same.", PALETTE.desc, false);
    for (i, entry) in entries.iter().enumerate() {
        row(grid, 0, LIST_ROW + i, i == sel, PALETTE.menu, false, entry);
    }
    draw_message(grid, message);
    hint(grid, "Up/Down choose   Enter confirm");
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
                row(grid, 0, LIST_ROW + i, i == sel, PALETTE.menu, false, b.name);
            } else {
                let fg = if i == sel { PALETTE.grey } else { PALETTE.dim };
                grid.text(0, LIST_ROW + i, if i == sel { "> " } else { "  " }, fg, false);
                grid.text(2, LIST_ROW + i, &format!("{} (locked)", b.name), fg, false);
            }
        }
        // Below the preview column, which owns everything from column 84.
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
            row(grid, 0, LIST_ROW + i, i == sel, PALETTE.menu, false, &label);
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
    // `rep` is a HashMap, so its iteration order is not stable; sort the factions or
    // the preview list reshuffles on every redraw (a locked class re-rolls the preview
    // each Enter, and "bandits +30" / "loners -20" swapped places).
    let mut y = LIST_ROW + ATTR_NAMES.len() + 3;
    let mut factions: Vec<(&String, &i32)> = preview.rep.iter().collect();
    factions.sort_unstable();
    for (faction, rep) in factions {
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
        // The list column ends at the panel gutter; a name that ran long pushed the
        // count and the "[wielded]" marker under the panel. Long affixed names are cut
        // here and read out in full below the list.
        let mut name = loot::display_name(stack, zone);
        if name.chars().count() > 68 {
            let cut: String = name.chars().take(65).collect();
            name = format!("{cut}...");
        }
        let label = format!("{name:<68}{count}{slot}");
        // The colour is the rarity: how much of the Zone got into it.
        let rarity = stack.rarity();
        let bold = rarity >= Rarity::Warped;
        row(grid, 0, LIST_ROW + line_no, i == sel, rarity.color(), bold, &label);
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
        // What somebody bolted to it, newest last - Backspace pulls that one first.
        for id in &stack.mods {
            if let Some(item) = zone.items.get(id) {
                if let ItemKind::Mod { effect, value, .. } = item.kind {
                    let line = format!("{} {}  ({})", plus(value), effect_word(effect), item.name);
                    grid.text(4, y, &line, PALETTE.cyan, false);
                    y += 1;
                }
            }
        }
        // And what is in the magazine (GUNS §3).
        if let ItemKind::Weapon { ammo: Some(_), mag, .. } = zone.items[&stack.id].kind {
            let capacity = (mag as i32 + loot::bonus(stack, zone, Effect::Mag)).max(1);
            let round = match &stack.loaded_with {
                Some(id) if stack.loaded > 0 => zone.items[id].name.as_str(),
                _ => "empty",
            };
            let line = format!("Loaded  {}/{capacity}  {round}", stack.loaded);
            grid.text(4, y, &line, PALETTE.status, false);
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
    hint(grid, "Up/Down select   Enter use, equip or fit   Backspace pull a mod   Tab/Esc back");
    draw_chrome(grid, run, clock);
}

/// One roll in words: what it does and by how much.
fn affix_line(affix: &AffixData, magnitude: i32) -> String {
    format!("{} {}", plus(magnitude), effect_word(affix.effect))
}

fn effect_word(effect: Effect) -> String {
    match effect {
        Effect::Damage => "damage".to_string(),
        Effect::ToHit => "to hit".to_string(),
        Effect::Armor => "damage resistance".to_string(),
        Effect::Crit => "crit range".to_string(),
        Effect::Attr(i) => ATTR_NAMES[i].to_string(),
        Effect::Rads => "rads an hour".to_string(),
        Effect::ApCost => "AP to attack".to_string(),
        Effect::Mag => "rounds in the magazine".to_string(),
    }
}

fn plus(n: i32) -> String {
    format!("{n:+}")
}

/// GDD §13: what a Medicine outcome does to a med's printed amount.
fn med_amount(outcome: Outcome, n: i32) -> i32 {
    match outcome {
        Outcome::CritSuccess => n * 2,
        Outcome::Success => n,
        Outcome::Fail => (n / 2).max(1),
        Outcome::CritFail => 0,
    }
}

/// Using a med is a Medicine check (GDD §13). A crit fail jabs you: the item is still
/// spent, it does nothing, and it costs 10 rads.
fn med_effect(run: &mut RunState, n: i32, rng: &mut Rng) -> (i32, Outcome) {
    let out = check_skill(run, Skill::Medicine.index(), 0, 1, rng);
    if out == Outcome::CritFail {
        run.rads = (run.rads + 10).min(1000);
    }
    (med_amount(out, n), out)
}

/// Uses or equips one item. Returns the message line, and whether anything actually
/// happened - a medkit at full health costs no AP because it never left the pack.
pub(crate) fn use_item(zone: &ZoneData, run: &mut RunState, index: usize, rng: &mut Rng) -> (String, bool) {
    let Some(stack) = run.items.get(index).cloned() else {
        return (String::new(), false);
    };
    let id = stack.id.clone();
    let uid = stack.uid;
    let name = loot::display_name(&stack, zone);
    let item = zone.items[&id].clone();
    match item.kind {
        ItemKind::Heal(n) => {
            let cap = run.max_hp - run.hp;
            if cap == 0 {
                return ("You are not hurt.".into(), false);
            }
            let (amount, out) = med_effect(run, n, rng);
            let healed = cap.min(amount);
            run.hp += healed;
            run.take_item(&id, 1);
            let msg = match out {
                Outcome::CritFail => format!("You fumble the {name}. The needle bites. RAD +10."),
                Outcome::Fail => format!("You use the {name} badly. HP +{healed}."),
                _ => format!("You use the {name}. HP +{healed}."),
            };
            (msg, true)
        }
        ItemKind::Antirad(n) => {
            if run.rads == 0 {
                return ("You are clean.".into(), false);
            }
            let (amount, out) = med_effect(run, n, rng);
            let cleared = run.rads.min(amount);
            run.rads -= cleared;
            run.take_item(&id, 1);
            let msg = match out {
                Outcome::CritFail => format!("You fumble the {name}. RAD +10."),
                _ => format!("You take the {name}. RAD -{cleared}."),
            };
            (msg, true)
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
        // Fitting is what a mod is for; it goes on what you are already using.
        ItemKind::Mod { fits, .. } => fit_mod(zone, run, &id, fits),
        ItemKind::Ammo { caliber, .. } => load_gun(zone, run, &id, caliber),
        ItemKind::Misc => (format!("The {name} is not much use here."), false),
    }
}

/// Fits a mod to whatever is equipped and takes it: the weapon first, then the
/// suit (GUNS §1). Three to an item, and never the same one twice.
fn fit_mod(zone: &ZoneData, run: &mut RunState, id: &str, fits: Fits) -> (String, bool) {
    let name = zone.items[id].name.clone();
    let target = [run.weapon, run.armor]
        .into_iter()
        .flatten()
        .find(|uid| run.stack(*uid).is_some_and(|s| suits(zone, s, fits)));
    let Some(uid) = target else {
        return (format!("There is nothing in use that the {name} goes on."), false);
    };
    let onto = loot::display_name(run.stack(uid).expect("just found"), zone);
    let stack = run.stack_mut(uid).expect("just found");
    if stack.mods.len() >= MOD_SLOTS {
        return (format!("The {onto} has no room for another fitting."), false);
    }
    if stack.mods.iter().any(|m| m == id) {
        return (format!("The {onto} already carries one."), false);
    }
    stack.mods.push(id.to_string());
    run.take_item(id, 1);
    (format!("You fit the {name} to the {onto}."), true)
}

/// Whether a mod of this `fits` belongs on this stack.
fn suits(zone: &ZoneData, stack: &ItemStack, fits: Fits) -> bool {
    match (fits, zone.items[&stack.id].kind) {
        (Fits::Any, _) => true,
        (Fits::Weapon, ItemKind::Weapon { .. }) => true,
        (Fits::Armor, ItemKind::Armor(_)) => true,
        _ => false,
    }
}

/// Loads the gun in hand with this round. Free out of a fight; in one it is the
/// four-AP rummage, which is what makes changing type mid-fight cost (GUNS §3).
fn load_gun(zone: &ZoneData, run: &mut RunState, id: &str, caliber: Caliber) -> (String, bool) {
    let name = zone.items[id].name.clone();
    let held = run.weapon.and_then(|uid| run.stack(uid));
    let takes = held.and_then(|s| match zone.items[&s.id].kind {
        ItemKind::Weapon { ammo, .. } => ammo,
        _ => None,
    });
    if takes != Some(caliber) {
        return (format!("Nothing in your hands takes the {name}."), false);
    }
    // Reload prefers what is already in the gun, so put this type in first.
    if let Some(stack) = run.weapon.and_then(|uid| run.stack_mut(uid)) {
        if stack.loaded == 0 {
            stack.loaded_with = Some(id.to_string());
        }
    }
    let message = crate::combat::reload(run, zone);
    let moved = !message.starts_with("It is already full");
    (message, moved)
}

/// Pulls the last mod off the highlighted item. A Repair check decides whether it
/// comes off whole or comes off in pieces (GUNS §1).
pub(crate) fn pull_mod(
    zone: &ZoneData,
    run: &mut RunState,
    index: usize,
    rng: &mut Rng,
) -> (String, bool) {
    let Some(stack) = run.items.get(index) else {
        return (String::new(), false);
    };
    let (uid, onto) = (stack.uid, loot::display_name(stack, zone));
    let Some(id) = stack.mods.last().cloned() else {
        return (format!("There is nothing fitted to the {onto}."), false);
    };
    let name = zone.items[&id].name.clone();
    let out = check_skill(run, Skill::Repair.index(), 0, 1, rng);
    run.stack_mut(uid).expect("just read").mods.pop();
    if matches!(out, Outcome::Success | Outcome::CritSuccess) {
        run.add_item(&id, 1);
        (format!("You work the {name} free of the {onto}."), true)
    } else {
        (format!("The {name} comes off the {onto} in pieces."), true)
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
    let shown = window(sel, list.len(), PACK_ROWS);
    for (line_no, i) in shown.clone().enumerate() {
        let stack = &list[i];
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
        let label = format!("{:<60}{count}{p:>9} RU", loot::display_name(stack, zone));
        let rarity = stack.rarity();
        let bold = rarity >= Rarity::Warped;
        row(grid, 0, LIST_ROW + line_no, i == sel, rarity.color(), bold, &label);
    }
    more(grid, LIST_ROW + PACK_ROWS, &shown, list.len());

    draw_message(grid, message);
    hint(grid, "Left/Right buy or sell   Up/Down select   Enter confirm   Esc leave");
    draw_chrome(grid, run, clock);
}

/// Buys or sells one unit of the selected row. Returns the message line.
/// Rounds change hands ten at a time. `ponytail:` a flat lot rather than a quantity
/// prompt; add the prompt if anyone ever wants seven of something.
const AMMO_LOT: u32 = 10;

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

    let lot = match item.kind {
        ItemKind::Ammo { .. } => AMMO_LOT.min(stack.count),
        _ => 1,
    };

    if buying {
        let due = p * lot;
        if run.rubles < due {
            return format!("You cannot afford the {name}.");
        }
        run.rubles -= due;
        // Off the shelf, with whatever is on it, and a uid of its own.
        let uid = run.next_uid();
        let bought = ItemStack { uid, count: lot, ..stack.clone() };
        run.add_stack(bought);
        take_n(stock, vendor_id, sel, lot);
        if lot > 1 {
            return format!("You buy {lot} of the {name} for {due} RU.");
        }
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

fn take_n(stock: &mut VendorStock, vendor_id: &str, index: usize, n: u32) {
    let shelf = stock.0.get_mut(vendor_id).expect("vendor stock");
    if index >= shelf.len() {
        return;
    }
    shelf[index].count = shelf[index].count.saturating_sub(n);
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
        let label = format!("{:<24}{standing}", area.name);
        row(grid, 0, LIST_ROW + line_no, selected, PALETTE.menu, false, &label);

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

    /// A pistol in hand, and the mod in the pack, ready to go on it.
    fn with_a_pistol(zone: &ZoneData, run: &mut RunState, mod_id: &str) {
        run.add_item("pistol", 1);
        run.weapon = run.items.iter().find(|s| s.id == "pistol").map(|s| s.uid);
        run.add_item(mod_id, 1);
        let _ = zone;
    }

    fn index_of(run: &RunState, id: &str) -> usize {
        run.items.iter().position(|s| s.id == id).expect("in the pack")
    }

    #[test]
    fn a_mod_goes_on_what_is_in_use_and_only_three_of_them_fit() {
        let (zone, _, mut run) = fixture();
        with_a_pistol(&zone, &mut run, "scope");
        let uid = run.weapon.unwrap();

        let i = index_of(&run, "scope");
        let (msg, acted) = use_item(&zone, &mut run, i, &mut Rng::new(4));
        assert!(acted, "{msg}");
        assert_eq!(run.stack(uid).unwrap().mods, vec!["scope".to_string()]);
        assert_eq!(run.count_of("scope"), 0, "the mod itself is spent");
        assert_eq!(loot::bonus(run.stack(uid).unwrap(), &zone, Effect::ToHit), 10);

        // The same one twice is not twice the scope.
        run.add_item("scope", 1);
        let i = index_of(&run, "scope");
        let (msg, acted) = use_item(&zone, &mut run, i, &mut Rng::new(4));
        assert!(!acted, "{msg}");
        assert!(msg.contains("already carries one"), "{msg}");

        // Three is the limit (GUNS §1).
        for id in ["laser", "grip", "bayonet"] {
            run.add_item(id, 1);
            let i = run.items.iter().position(|s| s.id == id).unwrap();
            use_item(&zone, &mut run, i, &mut Rng::new(4));
        }
        assert_eq!(run.stack(uid).unwrap().mods.len(), MOD_SLOTS);
        assert!(run.count_of("bayonet") > 0, "the fourth never went on");

        // And a mod for a suit does not go on a gun.
        run.add_item("plate", 1);
        let i = index_of(&run, "plate");
        let (msg, acted) = use_item(&zone, &mut run, i, &mut Rng::new(4));
        assert!(!acted, "{msg}");
    }

    #[test]
    fn pulling_a_mod_gets_it_back_or_breaks_it() {
        let (zone, _, mut run) = fixture();
        with_a_pistol(&zone, &mut run, "scope");
        let i = index_of(&run, "scope");
        use_item(&zone, &mut run, i, &mut Rng::new(4));
        let uid = run.weapon.unwrap();
        let sel = run.items.iter().position(|s| s.uid == uid).unwrap();

        // A steady hand gets it off whole (GUNS §1).
        run.skills[Skill::Repair.index()] = 100;
        let (msg, acted) = pull_mod(&zone, &mut run, sel, &mut Rng::new(9));
        assert!(acted, "{msg}");
        assert!(run.stack(uid).unwrap().mods.is_empty());
        assert_eq!(run.count_of("scope"), 1, "it went back in the pack");

        // A bad one destroys it, and the weapon is unharmed either way.
        let i = index_of(&run, "scope");
        use_item(&zone, &mut run, i, &mut Rng::new(4));
        run.skills[Skill::Repair.index()] = 0;
        let (msg, _) = pull_mod(&zone, &mut run, sel, &mut Rng::new(9));
        assert!(msg.contains("in pieces"), "{msg}");
        assert!(run.stack(uid).unwrap().mods.is_empty());
        assert_eq!(run.count_of("scope"), 0, "the mod is gone");
        assert_eq!(run.count_of("pistol"), 1, "the gun is not");

        // Nothing fitted, nothing to pull.
        let (_, acted) = pull_mod(&zone, &mut run, sel, &mut Rng::new(9));
        assert!(!acted);
    }

    #[test]
    fn rounds_load_the_gun_in_hand_and_only_that_gun() {
        let (zone, _, mut run) = fixture();
        run.add_item("pistol", 1);
        run.weapon = run.items.iter().find(|s| s.id == "pistol").map(|s| s.uid);
        let uid = run.weapon.unwrap();

        // The kit carries ten 9mm surplus, and they go in (GUNS §5).
        let i = index_of(&run, "pistol_surplus");
        let (msg, acted) = use_item(&zone, &mut run, i, &mut Rng::new(4));
        assert!(acted, "{msg}");
        assert_eq!(run.stack(uid).unwrap().loaded, 8);
        assert_eq!(run.count_of("pistol_surplus"), 2);

        // Shells do not fit a pistol, whatever else is in the pack.
        run.add_item("shell_buck", 4);
        let i = index_of(&run, "shell_buck");
        let (msg, acted) = use_item(&zone, &mut run, i, &mut Rng::new(4));
        assert!(!acted, "{msg}");
        assert_eq!(run.count_of("shell_buck"), 4);
    }

    #[test]
    fn ammunition_changes_hands_ten_at_a_time() {
        let (zone, mut stock, mut run) = fixture();
        let sel = stock.0["trader"].iter().position(|s| s.id == "pistol_round").unwrap();
        let held = run.count_of("pistol_round");
        let before = run.rubles;
        let msg = trade_one(&zone, &mut stock, &mut run, "trader", true, sel);
        assert_eq!(run.count_of("pistol_round"), held + 10, "{msg}");
        assert!(before - run.rubles >= 10 * zone.items["pistol_round"].base);
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

    /// A shelf longer than the screen scrolls instead of running into the hint
    /// and message rows.
    #[test]
    fn a_long_shelf_windows_instead_of_overwriting_the_hint_and_message() {
        use crate::render::{TileGrid, GRID_W, GRID_H};

        let (zone, _, run) = fixture();
        let mut stock = VendorStock(HashMap::new());
        let shelf: Vec<ItemStack> = (0..40).map(|uid| ItemStack::plain(uid, "bolt", 1)).collect();
        stock.0.insert("trader".to_string(), shelf);

        let mut grid = TileGrid::new(GRID_W, GRID_H);
        build_trade_grid(
            &mut grid,
            &zone,
            &stock,
            &run,
            &GameClock::default(),
            "trader",
            true,
            39,
            "a message",
        );

        let row = |y: usize| (0..GRID_W).map(|x| grid.cells[y * GRID_W + x].ch).collect::<String>();
        assert!(row(HINT_ROW).contains("Left/Right"), "hint row overwritten: {}", row(HINT_ROW));
        assert!(row(MESSAGE_ROW).contains("a message"), "message row overwritten: {}", row(MESSAGE_ROW));
        assert!(row(HINT_ROW - 1).trim().is_empty(), "list leaked past the hint row");
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
        let puddles = crate::liquid::Puddles::empty();

        build_creation_grid(&mut grid, &MetaProgress::default(), 0, 3, 0, &[], "");
        build_creation_grid(&mut grid, &MetaProgress::default(), 1, 9, 2, &[8, 9], "");
        for id in zone.areas.keys() {
            build_area_grid(&mut grid, &zone, &run, &clock, &fields, &puddles, id, 0, "test");
        }
        // Again with the field scanned and its artifact showing, so the extra
        // menu entry and the lit-up anomaly tells are drawn at least once.
        fields.0.insert(
            "field".into(),
            crate::sim::FieldState { scanned: true, artifact: true, ..Default::default() },
        );
        build_area_grid(&mut grid, &zone, &run, &clock, &fields, &puddles, "field", 4, "test");
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

    /// The creation preview draws faction standing from a `HashMap`, so it has to sort
    /// it or the list reshuffles every redraw — hammering Enter on a locked class was
    /// swapping "bandits +30" and "loners -20" place.
    #[test]
    fn the_creation_preview_lists_factions_in_a_stable_order() {
        use crate::render::GRID_W;

        let mut grid = TileGrid::new(GRID_W, crate::render::GRID_H);
        // Background 3 (Bandit) carries bandits +30 and loners -20.
        build_creation_grid(&mut grid, &MetaProgress::default(), 0, 3, 0, &[], "");

        let faction_row = |y: usize| {
            (PANEL_COL..GRID_W)
                .map(|x| grid.cells[y * GRID_W + x].ch)
                .collect::<String>()
                .trim_end()
                .to_string()
        };
        let first = LIST_ROW + ATTR_NAMES.len() + 3;
        assert_eq!(faction_row(first), "bandits +30");
        assert_eq!(faction_row(first + 1), "loners -20");
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
    fn a_med_is_a_medicine_check_and_healing_still_caps() {
        let (zone, _, mut run) = fixture();
        run.skills[Skill::Medicine.index()] = 100;
        run.hp = run.max_hp - 2;
        let i = run.items.iter().position(|s| s.id == "medkit").unwrap();
        let (msg, acted) = use_item(&zone, &mut run, i, &mut Rng::new(7));
        assert!(acted);
        assert_eq!(run.count_of("medkit"), 0, "a med is spent even when fumbled");
        assert!(run.hp <= run.max_hp && run.hp >= run.max_hp - 2, "heal caps at max: {msg}");
    }

    #[test]
    fn medicine_tiers_map_a_printed_amount() {
        assert_eq!(med_amount(Outcome::CritSuccess, 25), 50);
        assert_eq!(med_amount(Outcome::Success, 25), 25);
        assert_eq!(med_amount(Outcome::Fail, 25), 12);
        assert_eq!(med_amount(Outcome::Fail, 1), 1, "a fail still helps a little");
        assert_eq!(med_amount(Outcome::CritFail, 25), 0);
    }
}
