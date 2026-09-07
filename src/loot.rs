//! Loot the Zone has been at. A weapon or a suit that comes out of a field or off a
//! corpse carries affixes: a prefix in front of its name, a suffix after it, and how
//! many of each is what rarity means.
//!
//! Consumables and trophies never roll. A medkit is a medkit.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::area::{ItemKind, ZoneData};
use crate::render::PALETTE;
use crate::run::Rng;

// ---- rarity ----

/// How much of the Zone got into it. The tier is the affix budget and nothing else,
/// so a Relic is simply a thing with six rolls on it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Rarity {
    Plain,
    Touched,
    Marked,
    Warped,
    Relic,
}

impl Rarity {
    /// Derived from the rolls on the item, so the two can never disagree.
    pub fn of(affixes: usize) -> Rarity {
        match affixes {
            0 => Rarity::Plain,
            1 => Rarity::Touched,
            2 => Rarity::Marked,
            3..=4 => Rarity::Warped,
            _ => Rarity::Relic,
        }
    }

    pub fn budget(self) -> usize {
        match self {
            Rarity::Plain => 0,
            Rarity::Touched => 1,
            Rarity::Marked => 2,
            Rarity::Warped => 4,
            Rarity::Relic => 6,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Rarity::Plain => "",
            Rarity::Touched => "touched",
            Rarity::Marked => "marked",
            Rarity::Warped => "warped",
            Rarity::Relic => "relic",
        }
    }

    /// Colours already in the palette, in the meanings GDD §11 gives them: the more
    /// the Zone is in a thing, the further it runs from ordinary green.
    pub fn color(self) -> Color {
        match self {
            Rarity::Plain => PALETTE.menu,
            Rarity::Touched => PALETTE.cyan,
            Rarity::Marked => PALETTE.amber,
            Rarity::Warped => PALETTE.fire,
            Rarity::Relic => PALETTE.pale,
        }
    }
}

// ---- affixes ----

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Slot {
    Prefix,
    Suffix,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fits {
    Weapon,
    Armor,
    Any,
}

/// What an affix does. Every one of these has exactly one place in the code that
/// reads it, which is what keeps the list honest.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Effect {
    /// Flat damage on top of the weapon's dice.
    Damage,
    ToHit,
    /// Damage resistance on top of the armour's own.
    Armor,
    /// Widens the crit window, the way an aimed shot does.
    Crit,
    /// One attribute, by index (0 STR, 1 PER, 2 END, 3 CHA, 4 INT, 5 AGI, 6 LCK).
    Attr(usize),
    /// Rads an hour. Negative shields you; positive is what the Zone charges.
    Rads,
}

#[derive(Deserialize, Clone)]
#[serde(rename = "Affix")]
pub(crate) struct AffixData {
    /// A prefix is one word; a suffix is the whole "of the ..." phrase.
    pub name: String,
    pub slot: Slot,
    pub fits: Fits,
    pub effect: Effect,
    /// Inclusive roll range for the magnitude.
    pub range: (i32, i32),
    /// Rubles a point of magnitude is worth.
    pub value: u32,
}

impl AffixData {
    pub(crate) fn suits(&self, kind: ItemKind) -> bool {
        match (self.fits, kind) {
            (Fits::Any, ItemKind::Weapon { .. } | ItemKind::Armor(_)) => true,
            (Fits::Weapon, ItemKind::Weapon { .. }) => true,
            (Fits::Armor, ItemKind::Armor(_)) => true,
            _ => false,
        }
    }
}

/// One affix as it landed on one item.
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub(crate) struct Roll {
    pub affix: String,
    pub magnitude: i32,
}

// ---- an item as it actually exists ----

/// A thing in a pack. Plain items stack; anything the Zone has marked is its own
/// object, because no two of them are the same any more.
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub(crate) struct ItemStack {
    /// Unique per run, so an equipment slot can point at one particular pistol.
    pub uid: u32,
    pub id: String,
    pub count: u32,
    #[serde(default)]
    pub affixes: Vec<Roll>,
}

impl ItemStack {
    pub fn plain(uid: u32, id: &str, count: u32) -> Self {
        ItemStack { uid, id: id.to_string(), count, affixes: Vec::new() }
    }

    pub fn rarity(&self) -> Rarity {
        Rarity::of(self.affixes.len())
    }

    pub fn is_plain(&self) -> bool {
        self.affixes.is_empty()
    }
}

/// `<prefix> <base> <suffix>`, using the first of each. A Relic has more rolls on it
/// than will fit in a name; the rest are read off the item, not out of it.
pub(crate) fn display_name(stack: &ItemStack, zone: &ZoneData) -> String {
    let base = &zone.items[&stack.id].name;
    let pick = |slot: Slot| {
        stack
            .affixes
            .iter()
            .filter_map(|r| zone.affixes.get(&r.affix))
            .find(|a| a.slot == slot)
            .map(|a| a.name.as_str())
    };
    match (pick(Slot::Prefix), pick(Slot::Suffix)) {
        (Some(p), Some(s)) => format!("{p} {base} {s}"),
        (Some(p), None) => format!("{p} {base}"),
        (None, Some(s)) => format!("{base} {s}"),
        (None, None) => base.clone(),
    }
}

/// The total of one effect across an item's rolls.
pub(crate) fn bonus(stack: &ItemStack, zone: &ZoneData, effect: Effect) -> i32 {
    stack
        .affixes
        .iter()
        .filter_map(|r| zone.affixes.get(&r.affix).map(|a| (a, r.magnitude)))
        .filter(|(a, _)| a.effect == effect)
        .map(|(_, m)| m)
        .sum()
}

/// What it is worth before the vendor's markup: the base, plus what the Zone put on it.
pub(crate) fn value(stack: &ItemStack, zone: &ZoneData) -> u32 {
    let base = zone.items[&stack.id].base;
    let extra: u32 = stack
        .affixes
        .iter()
        .filter_map(|r| zone.affixes.get(&r.affix).map(|a| (a, r.magnitude)))
        .map(|(a, m)| a.value * m.unsigned_abs())
        .sum();
    base + extra
}

// ---- rolling ----

// Rarity out of a thousand, so the rare end can stay rare. Depth and luck widen
// each band, but the top one widens slowest: a relic is meant to be a story, not a
// tier-seven expectation. (GDD §4: LCK is crits and loot.)
//
// At tier 0 with average luck that is about 0.5% relic, 3% warped, 11% marked and
// 28% touched; at the bottom of the Zone with good luck, roughly 2 / 8 / 21 / 42.
const RELIC_BASE: i32 = 5;
const WARPED_BASE: i32 = 30;
const MARKED_BASE: i32 = 110;
const TOUCHED_BASE: i32 = 280;
/// How far depth and luck can push it, in the same thousandths.
const PUSH_CAP: i32 = 30;

fn push(tier: u32, luck: i32) -> i32 {
    (tier as i32 * 2 + (luck - 5) * 2).clamp(0, PUSH_CAP)
}

pub(crate) fn roll_rarity(tier: u32, luck: i32, rng: &mut Rng) -> Rarity {
    let push = push(tier, luck);
    let roll = rng.roll(1000) as i32;
    match roll {
        r if r > 1000 - (RELIC_BASE + push / 2) => Rarity::Relic,
        r if r > 1000 - (WARPED_BASE + push * 2) => Rarity::Warped,
        r if r > 1000 - (MARKED_BASE + push * 4) => Rarity::Marked,
        r if r > 1000 - (TOUCHED_BASE + push * 6) => Rarity::Touched,
        _ => Rarity::Plain,
    }
}

/// Rolls one item as it is found. Only gear the Zone can get into rolls affixes;
/// everything else comes out of the ground as itself.
pub(crate) fn roll_item(
    uid: u32,
    id: &str,
    count: u32,
    tier: u32,
    luck: i32,
    zone: &ZoneData,
    rng: &mut Rng,
) -> ItemStack {
    let kind = zone.items[id].kind;
    if !matches!(kind, ItemKind::Weapon { .. } | ItemKind::Armor(_)) {
        return ItemStack::plain(uid, id, count);
    }

    let rarity = roll_rarity(tier, luck, rng);
    let mut affixes: Vec<Roll> = Vec::new();
    // Alternate sides so a Marked reads as prefix-and-suffix, and start on a random
    // one so a single-affix Touched can be either.
    let mut side = if rng.roll(2) == 1 { Slot::Prefix } else { Slot::Suffix };
    for _ in 0..rarity.budget() {
        if let Some(id) = pick_affix(side, kind, &affixes, zone, rng) {
            let range = zone.affixes[&id].range;
            let span = (range.1 - range.0).unsigned_abs() + 1;
            let magnitude = range.0 + rng.roll(span) as i32 - 1;
            affixes.push(Roll { affix: id, magnitude });
        }
        side = if side == Slot::Prefix { Slot::Suffix } else { Slot::Prefix };
    }

    ItemStack { uid, id: id.to_string(), count: 1.max(count.min(1)), affixes }
}

/// An affix of the right side that fits this kind and is not already on it.
fn pick_affix(
    side: Slot,
    kind: ItemKind,
    taken: &[Roll],
    zone: &ZoneData,
    rng: &mut Rng,
) -> Option<String> {
    let mut pool: Vec<&String> = zone
        .affixes
        .iter()
        .filter(|(id, a)| {
            a.slot == side && a.suits(kind) && !taken.iter().any(|r| r.affix == **id)
        })
        .map(|(id, _)| id)
        .collect();
    if pool.is_empty() {
        return None;
    }
    pool.sort_unstable(); // a HashMap has no order, and a seeded run needs one
    Some(pool[rng.roll(pool.len() as u32) as usize - 1].clone())
}

// ---- tests (SPEC §7) ----

#[cfg(test)]
mod tests {
    use super::*;
    use crate::area::load_zone;
    use std::path::Path;

    #[test]
    fn rarity_is_the_number_of_rolls_and_nothing_else() {
        assert_eq!(Rarity::of(0), Rarity::Plain);
        assert_eq!(Rarity::of(1), Rarity::Touched);
        assert_eq!(Rarity::of(2), Rarity::Marked);
        assert_eq!(Rarity::of(4), Rarity::Warped);
        assert_eq!(Rarity::of(6), Rarity::Relic);
        for r in [Rarity::Plain, Rarity::Touched, Rarity::Marked, Rarity::Warped, Rarity::Relic] {
            assert_eq!(Rarity::of(r.budget()), r, "{r:?} does not round-trip");
        }
        assert!(Rarity::Relic > Rarity::Plain);
    }

    #[test]
    fn only_gear_rolls_and_it_rolls_within_its_ranges() {
        let zone = load_zone(Path::new("assets/data"));
        let mut rng = Rng::new(31);

        // A medkit is a medkit however lucky you are.
        for _ in 0..40 {
            let stack = roll_item(1, "medkit", 3, 9, 10, &zone, &mut rng);
            assert!(stack.is_plain());
            assert_eq!(stack.count, 3, "consumables still stack");
        }

        // Gear rolls, and every roll is a real affix inside its own range, on the
        // right side, and never the same affix twice.
        let mut seen_affixed = false;
        for _ in 0..200 {
            let stack = roll_item(2, "shotgun", 1, 6, 10, &zone, &mut rng);
            assert_eq!(stack.count, 1, "an affixed thing is one of a kind");
            assert!(stack.affixes.len() <= Rarity::Relic.budget());
            let mut ids: Vec<&str> = stack.affixes.iter().map(|r| r.affix.as_str()).collect();
            let before = ids.len();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), before, "an affix rolled twice on one item");
            for roll in &stack.affixes {
                let affix = zone.affixes.get(&roll.affix).expect("a real affix");
                assert!(
                    roll.magnitude >= affix.range.0 && roll.magnitude <= affix.range.1,
                    "{} rolled {} outside {:?}",
                    roll.affix,
                    roll.magnitude,
                    affix.range
                );
                assert!(affix.suits(zone.items["shotgun"].kind), "{} on a shotgun", roll.affix);
            }
            seen_affixed |= !stack.is_plain();
        }
        assert!(seen_affixed, "nothing ever rolled");
    }

    /// How many of each rarity fall out of `n` rolls.
    fn spread(tier: u32, luck: i32, n: usize, rng: &mut Rng) -> [usize; 5] {
        let mut counts = [0usize; 5];
        for _ in 0..n {
            counts[match roll_rarity(tier, luck, rng) {
                Rarity::Plain => 0,
                Rarity::Touched => 1,
                Rarity::Marked => 2,
                Rarity::Warped => 3,
                Rarity::Relic => 4,
            }] += 1;
        }
        counts
    }

    #[test]
    fn depth_and_luck_push_the_rarity_up_without_making_relics_ordinary() {
        let mut rng = Rng::new(77);
        const N: usize = 20_000;

        let shallow = spread(0, 5, N, &mut rng);
        let deep = spread(7, 10, N, &mut rng);

        // Deeper and luckier is better across the board.
        assert!(deep[0] < shallow[0], "plain: deep {} vs shallow {}", deep[0], shallow[0]);
        for tier in 1..5 {
            assert!(
                deep[tier] > shallow[tier],
                "tier {tier}: deep {} vs shallow {}",
                deep[tier],
                shallow[tier]
            );
        }

        // But a relic stays a story. This is the guard that caught the first
        // version, where tier three was handing them out several a trip.
        assert!(shallow[4] * 100 < N, "relics are under 1% at the edge: {shallow:?}");
        assert!(deep[4] * 100 < 3 * N, "and under 3% at the bottom: {deep:?}");
        // Most of what you pick up off the ground is still just a gun.
        assert!(shallow[0] > N / 2, "plain is the common case at the edge: {shallow:?}");
        assert!(deep[0] > N / 5, "and never disappears: {deep:?}");
    }

    #[test]
    fn a_name_reads_prefix_base_suffix_and_the_price_follows_the_rolls() {
        let zone = load_zone(Path::new("assets/data"));
        let prefix = zone.affixes.iter().find(|(_, a)| a.slot == Slot::Prefix).unwrap();
        let suffix = zone.affixes.iter().find(|(_, a)| a.slot == Slot::Suffix).unwrap();

        let plain = ItemStack::plain(1, "pistol", 1);
        assert_eq!(display_name(&plain, &zone), "PMm Pistol");
        assert_eq!(value(&plain, &zone), zone.items["pistol"].base);

        let fancy = ItemStack {
            uid: 2,
            id: "pistol".into(),
            count: 1,
            affixes: vec![
                Roll { affix: prefix.0.clone(), magnitude: 2 },
                Roll { affix: suffix.0.clone(), magnitude: 3 },
            ],
        };
        let name = display_name(&fancy, &zone);
        assert!(name.starts_with(&prefix.1.name), "{name}");
        assert!(name.ends_with(&suffix.1.name), "{name}");
        assert!(name.contains("PMm Pistol"), "{name}");
        assert_eq!(fancy.rarity(), Rarity::Marked);
        assert_eq!(
            value(&fancy, &zone),
            zone.items["pistol"].base + prefix.1.value * 2 + suffix.1.value * 3
        );
    }
}

#[cfg(test)]
mod sample {
    use super::*;
    use crate::area::load_zone;
    use std::path::Path;

    #[test]
    #[ignore = "eyeball the loot: cargo test -- --ignored --nocapture"]
    fn what_it_actually_drops() {
        let zone = load_zone(Path::new("assets/data"));
        let mut rng = Rng::new(2026);
        for (tier, label) in [(0, "shallow"), (3, "mid"), (7, "deep")] {
            println!("\n-- {label} (tier {tier}) --");
            for id in ["pistol", "shotgun", "vest", "seva_suit"] {
                for _ in 0..3 {
                    let s = roll_item(1, id, 1, tier, 5, &zone, &mut rng);
                    println!(
                        "  {:<10} {:<46} {} RU",
                        s.rarity().name(),
                        display_name(&s, &zone),
                        value(&s, &zone)
                    );
                }
            }
        }
    }
}
