//! The stalker: `RunState`, backgrounds, the seeded RNG, the d100 `check()`
//! and the one `price()` formula (SPEC §3).

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::loot::ItemStack;

// ---- attributes & skills (GDD §4) ----

pub(crate) const STR: usize = 0;
pub(crate) const PER: usize = 1;
pub(crate) const END: usize = 2;
pub(crate) const CHA: usize = 3;
pub(crate) const INT: usize = 4;
pub(crate) const AGI: usize = 5;
pub(crate) const LCK: usize = 6;

pub(crate) const ATTR_NAMES: [&str; 7] = ["STR", "PER", "END", "CHA", "INT", "AGI", "LCK"];

pub(crate) const SKILL_NAMES: [&str; 10] = [
    "Small Guns",
    "Energy Weapons",
    "Melee",
    "Sneak",
    "Medicine",
    "Repair",
    "Lockpick",
    "Science",
    "Stalker Lore",
    "Barter",
];

/// Governing attribute per skill (GDD §4). Starting skill = 10 + 3 × attribute.
const SKILL_ATTR: [usize; 10] = [AGI, INT, STR, AGI, INT, INT, PER, INT, PER, CHA];

pub(crate) const BARTER: usize = 9;

/// The same ten skills, named for `zone.ron` gates. Order matches `SKILL_NAMES`.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Skill {
    SmallGuns,
    EnergyWeapons,
    Melee,
    Sneak,
    Medicine,
    Repair,
    Lockpick,
    Science,
    StalkerLore,
    Barter,
}

impl Skill {
    pub fn index(self) -> usize {
        self as usize
    }
}

// GDD §4: attributes start at 5, tags give +15, HP = 20 + 3 × END.
const ATTR_BASE: i32 = 5;
const TAG_BONUS: i32 = 15;
pub(crate) const TAG_COUNT: usize = 3;

/// Nobody uses their own name in the Zone. Picked at creation so the memorial has
/// something to carve (GDD §10).
const NAMES: [&str; 12] = [
    "Sparrow", "Grim", "Tinman", "Kettle", "Pike", "Ash",
    "Cricket", "Mole", "Rook", "Fen", "Bricks", "Marsh",
];

// ---- backgrounds (GDD §4) ----

pub(crate) struct Background {
    pub name: &'static str,
    pub blurb: &'static str,
    pub attr: (usize, i32),
    pub skills: &'static [(usize, i32)],
    pub rep: &'static [(&'static str, i32)],
}

// ponytail: all four are selectable now; the Ecologist/Bandit unlock conditions
// need MetaProgress, which lands with permadeath in M6.
pub(crate) const BACKGROUNDS: [Background; 4] = [
    Background {
        name: "Loner",
        blurb: "You came for the money and stayed for the quiet.",
        attr: (END, 1),
        skills: &[(8, 10)],
        rep: &[("loners", 20)],
    },
    Background {
        name: "ex-Duty",
        blurb: "You wore the black and red until the orders stopped making sense.",
        attr: (STR, 1),
        skills: &[(0, 10)],
        rep: &[("duty", 30), ("freedom", -20)],
    },
    Background {
        name: "Ecologist",
        blurb: "You read the Zone in numbers before you ever smelled it.",
        attr: (INT, 1),
        skills: &[(7, 10), (4, 10)],
        rep: &[("ecologists", 30)],
    },
    Background {
        name: "Bandit",
        blurb: "Somebody else's kit paid for your first trip in.",
        attr: (AGI, 1),
        skills: &[(3, 10), (9, 10)],
        rep: &[("bandits", 30), ("loners", -20)],
    },
];

// ---- the run ----

// Starting kit (GDD §7). A knife is the whole opening answer to the first dog
// (GDD §8); a stalker who sells it for rubles is betting they will not need it.
// Rest: 8 h at a sheltered camp.
const START_RUBLES: u32 = 600;
const START_KIT: [(&str, u32); 4] = [("knife", 1), ("medkit", 1), ("bread", 2), ("bolt", 5)];
const START_MINUTES: u32 = 6 * 60;
pub(crate) const REST_MINUTES: u32 = 8 * 60;
pub(crate) const REST_COST: u32 = 50;
pub(crate) const REST_RADS: i32 = 100;

#[derive(Resource, Default, Serialize, Deserialize, Clone)]
pub(crate) struct RunState {
    pub name: String,
    pub background: usize,
    pub attrs: [i32; 7],
    pub skills: [i32; 10],
    pub tags: [bool; 10],
    pub hp: i32,
    pub max_hp: i32,
    pub rads: i32,
    pub rubles: u32,
    pub items: Vec<ItemStack>,
    /// The uid of the stack in hand and the one being worn. A uid rather than an
    /// item id, because two pistols are no longer the same pistol.
    pub weapon: Option<u32>,
    pub armor: Option<u32>,
    next_uid: u32,
    pub rep: HashMap<String, i32>,
    /// Minutes since day 1, 00:00.
    pub minutes: u32,
    /// Quest/secret flags set by `SetFlag`.
    pub flags: HashSet<String>,
    /// Hidden letters this run has seen, keyed `"<area>:<letter>"`.
    pub revealed: HashSet<String>,
    /// Gates already rolled, so a failed `Check` gate stays failed for the run.
    pub gate_rolled: HashSet<String>,
    /// Areas this stalker has stood in - the map is drawn from it (GDD §5).
    pub discovered: HashSet<String>,
    /// Enemy ids killed, for kill jobs.
    pub kills: HashSet<String>,
    /// Lore entries turned up this run.
    pub lore: HashSet<String>,
    pub quests_taken: HashSet<String>,
    pub quests_done: HashSet<String>,
    /// Set once, when the run ends: how it ended, and which ending if it was the Room.
    pub death: Option<String>,
    pub ending: Option<String>,
}

impl RunState {
    pub fn roll(background: usize, tags: &[usize], rng: &mut Rng) -> Self {
        let bg = &BACKGROUNDS[background];
        let mut attrs = [ATTR_BASE; 7];
        attrs[bg.attr.0] += bg.attr.1;

        let mut skills = [0; 10];
        for (i, s) in skills.iter_mut().enumerate() {
            *s = 10 + 3 * attrs[SKILL_ATTR[i]];
        }
        for &(i, bonus) in bg.skills {
            skills[i] += bonus;
        }
        let mut tag_flags = [false; 10];
        for &t in tags {
            skills[t] += TAG_BONUS;
            tag_flags[t] = true;
        }

        let max_hp = 20 + 3 * attrs[END];
        RunState {
            name: NAMES[rng.roll(NAMES.len() as u32) as usize - 1].to_string(),
            background,
            attrs,
            skills,
            tags: tag_flags,
            hp: max_hp,
            max_hp,
            rads: 0,
            rubles: START_RUBLES,
            items: START_KIT
                .iter()
                .enumerate()
                .map(|(i, &(id, n))| ItemStack::plain(i as u32 + 1, id, n))
                .collect(),
            weapon: None,
            armor: None,
            next_uid: START_KIT.len() as u32 + 1,
            rep: bg.rep.iter().map(|&(f, r)| (f.to_string(), r)).collect(),
            minutes: START_MINUTES,
            flags: HashSet::new(),
            revealed: HashSet::new(),
            gate_rolled: HashSet::new(),
            discovered: HashSet::new(),
            kills: HashSet::new(),
            lore: HashSet::new(),
            quests_taken: HashSet::new(),
            quests_done: HashSet::new(),
            death: None,
            ending: None,
        }
    }

    /// Key for the per-run secret sets.
    pub fn secret_key(area_id: &str, letter: char) -> String {
        format!("{area_id}:{letter}")
    }

    pub fn is_revealed(&self, area_id: &str, letter: char) -> bool {
        self.revealed.contains(&Self::secret_key(area_id, letter))
    }

    pub fn day(&self) -> u32 {
        self.minutes / 1440 + 1
    }

    pub fn clock(&self) -> String {
        let m = self.minutes % 1440;
        format!("{:02}:{:02}", m / 60, m % 60)
    }

    pub fn rep_of(&self, faction: &str) -> i32 {
        self.rep.get(faction).copied().unwrap_or(0)
    }

    pub fn next_uid(&mut self) -> u32 {
        self.next_uid += 1;
        self.next_uid
    }

    /// Puts a rolled stack in the pack. Plain things merge with what is already
    /// there; anything the Zone has marked stays its own object.
    pub fn add_stack(&mut self, stack: ItemStack) {
        if stack.is_plain() {
            if let Some(slot) = self
                .items
                .iter_mut()
                .find(|s| s.id == stack.id && s.is_plain())
            {
                slot.count += stack.count;
                return;
            }
        }
        self.items.push(stack);
    }

    /// Plain items by id, which is how content, loot tables and quests speak.
    pub fn add_item(&mut self, id: &str, n: u32) {
        let uid = self.next_uid();
        self.add_stack(ItemStack::plain(uid, id, n));
    }

    pub fn count_of(&self, id: &str) -> u32 {
        self.items.iter().filter(|s| s.id == id).map(|s| s.count).sum()
    }

    pub fn stack(&self, uid: u32) -> Option<&ItemStack> {
        self.items.iter().find(|s| s.uid == uid)
    }

    /// Removes `n` of `id`, plainest first, so handing a job its item does not spend
    /// the good one. Returns false and changes nothing if there are not enough.
    pub fn take_item(&mut self, id: &str, n: u32) -> bool {
        if self.count_of(id) < n {
            return false;
        }
        let mut left = n;
        let mut order: Vec<usize> = (0..self.items.len()).filter(|&i| self.items[i].id == id).collect();
        order.sort_by_key(|&i| self.items[i].affixes.len());
        for i in order {
            if left == 0 {
                break;
            }
            let taken = left.min(self.items[i].count);
            self.items[i].count -= taken;
            left -= taken;
        }
        self.items.retain(|s| {
            let keep = s.count > 0;
            if !keep {
                // Nothing stays equipped once it is gone.
                if self.weapon == Some(s.uid) {
                    self.weapon = None;
                }
                if self.armor == Some(s.uid) {
                    self.armor = None;
                }
            }
            keep
        });
        true
    }

    /// Removes one whole stack by uid, whatever is on it.
    pub fn take_uid(&mut self, uid: u32) -> Option<ItemStack> {
        let i = self.items.iter().position(|s| s.uid == uid)?;
        if self.weapon == Some(uid) {
            self.weapon = None;
        }
        if self.armor == Some(uid) {
            self.armor = None;
        }
        let mut stack = self.items.remove(i);
        if stack.count > 1 {
            stack.count -= 1;
            let one = ItemStack { count: 1, ..stack.clone() };
            self.items.insert(i, stack);
            return Some(one);
        }
        Some(stack)
    }
}

// ---- randomness (SPEC §3: one seeded Rng, no thread_rng) ----

/// xorshift64. `ponytail:` a dice RNG is six lines; pull in `rand` only if we ever
/// need distributions beyond "roll a die".
#[derive(Resource, Serialize, Deserialize, Clone)]
pub(crate) struct Rng {
    state: u64,
    #[allow(dead_code)] // read by the suspend file in M6 so a run can be replayed
    pub seed: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let seed = seed | 1;
        Rng { state: seed, seed }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    pub fn roll(&mut self, sides: u32) -> u32 {
        (self.next_u64() >> 33) as u32 % sides + 1
    }
}

impl Default for Rng {
    fn default() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x5EED);
        Rng::new(nanos)
    }
}

// ---- the check (GDD §4) ----

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum Outcome {
    CritFail,
    Fail,
    Success,
    CritSuccess,
}

// Difficulty modifiers (GDD §4); the first callers are the gated secrets in M3.
#[allow(dead_code)]
pub(crate) const DIFF_EASY: i32 = 20;
#[allow(dead_code)]
pub(crate) const DIFF_HARD: i32 = -20;
#[allow(dead_code)]
pub(crate) const DIFF_VERY_HARD: i32 = -40;

/// The only d100 roll-under in the game.
pub(crate) fn check(skill: i32, modifier: i32, rng: &mut Rng) -> Outcome {
    check_crit(skill, modifier, 1, rng)
}

/// As `check`, but crits on a roll at or under `crit_on` — an aimed shot crits on 5
/// where everything else crits only on 1 (GDD §8).
pub(crate) fn check_crit(skill: i32, modifier: i32, crit_on: u32, rng: &mut Rng) -> Outcome {
    outcome(rng.roll(100), skill, modifier, crit_on)
}

/// Rolls one of the stalker's own skills, and lets it improve on a success (GDD §4).
/// Anything the player is *practising* goes through here rather than `check`.
pub(crate) fn check_skill(
    run: &mut RunState,
    skill: usize,
    modifier: i32,
    crit_on: u32,
    rng: &mut Rng,
) -> Outcome {
    let out = check_crit(run.skills[skill], modifier, crit_on, rng);
    if matches!(out, Outcome::Success | Outcome::CritSuccess) {
        improve(run, skill, rng);
    }
    out
}

/// GDD §4: a success on a skill under 50 improves it 1 time in 4, over 50 1 in 10.
/// A tagged skill rises twice as fast.
fn improve(run: &mut RunState, skill: usize, rng: &mut Rng) {
    let odds = if run.skills[skill] < 50 { 4 } else { 10 };
    let odds = if run.tags[skill] { odds / 2 } else { odds };
    if rng.roll(odds) == 1 {
        run.skills[skill] += 1;
    }
}

fn outcome(roll: u32, skill: i32, modifier: i32, crit_on: u32) -> Outcome {
    match roll {
        r if r <= crit_on => Outcome::CritSuccess,
        100 => Outcome::CritFail,
        r if r as i32 <= skill + modifier => Outcome::Success,
        _ => Outcome::Fail,
    }
}

// ---- prices (GDD §7) ----

/// Rep tiers (GDD §9) -> (buy multiplier, sell multiplier). `None` = refuses to trade.
fn rep_mult(rep: i32) -> Option<(f32, f32)> {
    match rep {
        r if r < -50 => None,
        r if r < -10 => Some((1.25, 0.8)),
        r if r > 75 => Some((0.8, 1.2)),
        r if r > 25 => Some((0.9, 1.1)),
        _ => Some((1.0, 1.0)),
    }
}

/// buy  = base × markup × (1.5 − barter/200) × rep_buy
/// sell = base × markup × (0.5 + barter/400) × rep_sell   (GDD §7)
pub(crate) fn price(base: u32, markup: f32, barter: i32, rep: i32, buying: bool) -> Option<u32> {
    let (rb, rs) = rep_mult(rep)?;
    let barter = barter.clamp(0, 100) as f32;
    let p = if buying {
        base as f32 * markup * (1.5 - barter / 200.0) * rb
    } else {
        base as f32 * markup * (0.5 + barter / 400.0) * rs
    };
    Some((p.round() as u32).max(1))
}

// ---- tests (SPEC §7) ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d100_crit_edges() {
        // 1 always crits up and 100 always crits down, whatever the skill.
        assert_eq!(outcome(1, 0, -40, 1), Outcome::CritSuccess);
        assert_eq!(outcome(100, 100, 20, 1), Outcome::CritFail);
        assert_eq!(outcome(50, 50, 0, 1), Outcome::Success);
        assert_eq!(outcome(51, 50, 0, 1), Outcome::Fail);
        assert_eq!(outcome(51, 50, 20, 1), Outcome::Success);
        // An aimed shot widens the crit window to 5 and nothing else (GDD §8).
        assert_eq!(outcome(5, 50, 0, 5), Outcome::CritSuccess);
        assert_eq!(outcome(6, 50, 0, 5), Outcome::Success);
        assert_eq!(outcome(100, 100, 0, 5), Outcome::CritFail);
    }

    #[test]
    fn skills_rise_by_use_and_tags_rise_faster() {
        // Practise a tagged skill under 50 (1 in 2) and an untagged one (1 in 4);
        // both must climb, and the tagged one must climb faster.
        let mut rng = Rng::new(99);
        let mut run = RunState::roll(0, &[2, 3, 4], &mut rng);
        run.skills[2] = 20;
        run.skills[0] = 20;
        for _ in 0..400 {
            check_skill(&mut run, 2, 100, 1, &mut rng);
            check_skill(&mut run, 0, 100, 1, &mut rng);
        }
        assert!(run.skills[2] > 20 && run.skills[0] > 20, "use raises a skill");
        assert!(
            run.skills[2] - 20 > run.skills[0] - 20,
            "tagged {} vs untagged {}",
            run.skills[2],
            run.skills[0]
        );

        // Only a success teaches. At hopeless odds the only teacher left is the
        // 1-in-100 crit, so the skill barely moves instead of not moving at all.
        let mut run = RunState::roll(0, &[0, 1, 2], &mut rng);
        let before = run.skills[5];
        for _ in 0..200 {
            check_skill(&mut run, 5, -1000, 1, &mut rng);
        }
        assert!(run.skills[5] - before < 3, "failure is not practice");
    }

    #[test]
    fn d100_stays_in_range() {
        let mut rng = Rng::new(12345);
        for _ in 0..1000 {
            let r = rng.roll(100);
            assert!((1..=100).contains(&r));
        }
    }

    #[test]
    fn price_buy_above_sell_and_modifiers_point_the_right_way() {
        let buy = |b, r| price(100, 1.0, b, r, true).unwrap();
        let sell = |b, r| price(100, 1.0, b, r, false).unwrap();

        assert!(buy(0, 0) > sell(0, 0));
        assert!(buy(100, 0) > sell(100, 0));
        assert_eq!((buy(0, 0), sell(0, 0)), (150, 50)); // GDD §7 anchors
        assert_eq!((buy(100, 0), sell(100, 0)), (100, 75));

        // Barter: buying gets cheaper, selling pays better.
        assert!(buy(80, 0) < buy(20, 0));
        assert!(sell(80, 0) > sell(20, 0));

        // Rep: friendlier is cheaper to buy from and pays more.
        assert!(buy(50, 80) < buy(50, 0));
        assert!(sell(50, 80) > sell(50, 0));
        assert!(buy(50, -20) > buy(50, 0));
        assert_eq!(price(100, 1.0, 50, -60, true), None); // hostile refuses
    }

    #[test]
    fn roll_applies_background_tags_and_kit() {
        let r = RunState::roll(0, &[8, 9, 4], &mut Rng::new(1)); // Loner; Lore, Barter, Medicine
        assert_eq!(r.attrs[END], 6);
        assert_eq!(r.max_hp, 38);
        assert_eq!(r.hp, r.max_hp);
        // Stalker Lore: 10 + 3×PER(5) = 25, +10 background, +15 tag.
        assert_eq!(r.skills[8], 50);
        assert_eq!(r.rep_of("loners"), 20);
        assert_eq!(r.day(), 1);
        assert_eq!(r.clock(), "06:00");
        assert!(!r.name.is_empty(), "the memorial needs something to carve");
        // GDD §8: the starting kit is a knife, not bare hands. Selling it is a
        // choice, arriving with it is not.
        assert_eq!(r.count_of("knife"), 1, "a new stalker starts with a knife");
    }

    #[test]
    fn take_item_is_all_or_nothing_and_unequips() {
        let mut r = RunState::roll(0, &[0, 1, 2], &mut Rng::new(1));
        r.add_item("pistol", 1);
        let uid = r.items.iter().find(|s| s.id == "pistol").unwrap().uid;
        r.weapon = Some(uid);
        assert!(!r.take_item("pistol", 2));
        assert!(r.take_item("pistol", 1));
        assert_eq!(r.weapon, None, "what is gone cannot still be in your hand");
        assert_eq!(r.count_of("pistol"), 0);
    }

    #[test]
    fn plain_things_stack_and_marked_things_do_not() {
        use crate::loot::Roll;
        let mut r = RunState::roll(0, &[0, 1, 2], &mut Rng::new(1));
        r.add_item("bolt", 3);
        assert_eq!(r.count_of("bolt"), 8, "five in the kit, three more in");
        assert_eq!(r.items.iter().filter(|s| s.id == "bolt").count(), 1);

        // Two pistols the Zone has been at are two objects, not a stack of two.
        let uid = r.next_uid();
        r.add_stack(ItemStack {
            uid,
            id: "pistol".into(),
            count: 1,
            affixes: vec![Roll { affix: "keen".into(), magnitude: 6 }],
        });
        let uid = r.next_uid();
        r.add_stack(ItemStack {
            uid,
            id: "pistol".into(),
            count: 1,
            affixes: vec![Roll { affix: "heavy".into(), magnitude: 2 }],
        });
        assert_eq!(r.items.iter().filter(|s| s.id == "pistol").count(), 2);

        // A job takes the plain one first, so the good one stays in the pack.
        r.add_item("pistol", 1);
        assert!(r.take_item("pistol", 1));
        assert_eq!(r.items.iter().filter(|s| s.id == "pistol").count(), 2);
        assert!(r.items.iter().all(|s| s.id != "pistol" || !s.is_plain()));
    }
}
