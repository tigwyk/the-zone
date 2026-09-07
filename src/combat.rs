//! Turn-based, AP-driven combat at close quarters (GDD §8). No grid, no distance:
//! everything is already in reach, and the area art stays up while the combatants
//! are listed under it.

use bevy::prelude::*;

use crate::area::{draw_art, Caliber, EnemyData, ItemKind, ZoneData};
use crate::loot::{self, Effect};
use crate::render::{TileGrid, PALETTE};
use crate::run::{check, check_skill, Outcome, Rng, RunState, Skill, AGI, INT, LCK, PER};
use crate::screens::{draw_chrome, draw_message, row};
use crate::sim::{attr, carrying_light, is_night, Fields, GameClock, NIGHT_PENALTY};

// Fixed row map (SPEC §4), shared with the area screen.
const ENEMY_ROW: usize = 19;
const YOU_ROW: usize = 20;
const MENU_ROW: usize = 22;

// AP costs (GDD §8).
pub(crate) const AP_ATTACK: i32 = 3;
pub(crate) const AP_AIMED: i32 = 6;
pub(crate) const AP_ITEM: i32 = 4;
/// GUNS §3: two points to feed a gun, one if you have handled one before.
pub(crate) const AP_RELOAD: i32 = 2;
pub(crate) const AP_RELOAD_FAST: i32 = 1;
pub(crate) const RELOAD_FAST_SKILL: i32 = 60;
/// However many mods are bolted on, an attack costs at least this (GUNS §1).
const AP_ATTACK_MIN: i32 = 2;

const AIMED_BONUS: i32 = 20;
/// An aimed shot crits on 5, not just on 1 (GDD §8).
const AIMED_CRIT_ON: u32 = 5;
/// How far one point of a crit-range affix widens the window. The bench in
/// balance.rs measured one-for-one, and then two-for-one, as worth about a single
/// percentage point of win rate: base damage is low enough that doubling it
/// occasionally is weak unless it happens often. Five is a real 6-16% crit chance.
const CRIT_PER_POINT: u32 = 5;
/// Bare hands, for a stalker who sold their knife (GDD §8).
const UNARMED: (u32, u32) = (1, 3);
/// GDD §8: an enemy that runs does so under a fifth of its health.
const FLEE_BELOW: i32 = 5;
/// The AP an ordinary thing has, and what each point above it takes off your chance
/// of outrunning it.
const BASE_AP: i32 = 7;
const FASTER_PER_AP: i32 = 8;

#[derive(Resource, Default)]
pub(crate) struct Combat {
    pub active: bool,
    pub enemy: String,
    pub hp: i32,
    pub ap: i32,
    /// Menu cursor, kept here so it survives a trip to the inventory.
    pub sel: usize,
}

/// AP for one turn: 5 + AGI/2 (GDD §4), on the attribute as radiation leaves it.
pub(crate) fn turn_ap(run: &RunState, zone: &ZoneData) -> i32 {
    5 + attr(run, zone, AGI) / 2
}

pub(crate) fn start(
    enemy_id: &str,
    combat: &mut Combat,
    run: &mut RunState,
    zone: &ZoneData,
    rng: &mut Rng,
) -> String {
    let enemy = zone.enemies[enemy_id].clone();
    *combat = Combat {
        active: true,
        enemy: enemy_id.to_string(),
        hp: enemy.hp,
        ap: turn_ap(run, zone),
        sel: 0,
    };

    // GDD §8: a bloodsucker is not there until it is. Spot it or it opens on you,
    // already inside your guard.
    if enemy.ambush {
        let per = attr(run, zone, PER) * 10;
        if !matches!(check(per, 0, rng), Outcome::Success | Outcome::CritSuccess) {
            let opener = enemy_turn(&enemy, combat, run, zone, rng);
            return format!("The air comes apart and something is on you. {opener}");
        }
        return format!("You catch the shimmer a moment early. A {}.", enemy.name);
    }
    format!("A {} comes at you.", enemy.name)
}

// ---- what the player can do right now ----

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Verb {
    Attack,
    Aimed,
    Reload,
    Flee,
}

/// What is in hand: dice, the skill that swings it, and whatever the Zone added to
/// it. Bare hands otherwise.
struct InHand {
    dice: (u32, u32),
    skill: Skill,
    damage: i32,
    to_hit: i32,
    crit: u32,
    /// What one swing or shot costs, after the mods and never under the floor.
    ap: i32,
    /// What it eats, and what is in it. `None` is a blade, and never empty.
    caliber: Option<Caliber>,
    loaded: u32,
    capacity: u32,
    /// What the rounds in it are worth: armour pierced (GUNS §2).
    pierce: i32,
}

fn weapon(run: &RunState, zone: &ZoneData) -> InHand {
    let held = run.weapon.and_then(|uid| run.stack(uid));
    let kind = held.map(|s| zone.items[&s.id].kind);
    let (dice, skill, caliber, mag) = match kind {
        Some(ItemKind::Weapon { dice, skill, ammo, mag }) => (dice, skill, ammo, mag),
        _ => (UNARMED, Skill::Melee, None, 0),
    };
    let of = |effect| held.map_or(0, |s| loot::bonus(s, zone, effect));
    // What is actually chambered. Nothing in the magazine is worth nothing.
    let loaded = held.map_or(0, |s| s.loaded);
    let round = held
        .and_then(|s| s.loaded_with.as_ref())
        .filter(|_| loaded > 0)
        .and_then(|id| zone.items.get(id))
        .map_or((0, 0, 0), |i| match i.kind {
            ItemKind::Ammo { damage, to_hit, pierce, .. } => (damage, to_hit, pierce),
            _ => (0, 0, 0),
        });
    InHand {
        dice,
        skill,
        damage: of(Effect::Damage) + round.0,
        to_hit: of(Effect::ToHit) + round.1,
        crit: of(Effect::Crit).max(0) as u32,
        ap: (AP_ATTACK + of(Effect::ApCost)).max(AP_ATTACK_MIN),
        caliber,
        loaded,
        capacity: (mag as i32 + of(Effect::Mag)).max(1) as u32,
        pierce: round.2,
    }
}

impl InHand {
    /// A gun with nothing in it cannot be fired (GUNS §3). A blade is never empty.
    fn empty(&self) -> bool {
        self.caliber.is_some() && self.loaded == 0
    }
}

/// GUNS §3: two points to feed it, one if you have handled a gun before.
pub(crate) fn reload_ap(run: &RunState) -> i32 {
    if run.skills[Skill::SmallGuns.index()] >= RELOAD_FAST_SKILL {
        AP_RELOAD_FAST
    } else {
        AP_RELOAD
    }
}

/// The rounds in the pack that fit what is in hand, cheapest first, but the type
/// already in the gun ahead of all of them - a reload does not quietly downgrade you.
fn compatible<'a>(
    run: &RunState,
    zone: &'a ZoneData,
    hand: &InHand,
    current: Option<&str>,
) -> Vec<&'a String> {
    let Some(caliber) = hand.caliber else {
        return Vec::new();
    };
    let mut ids: Vec<&String> = zone.ammo_of(caliber).filter(|id| run.count_of(id) > 0).collect();
    if let Some(current) = current {
        ids.sort_by_key(|id| id.as_str() != current);
    }
    ids
}

/// Whether feeding it is possible at all: a gun, room in the magazine, and rounds.
fn can_reload(run: &RunState, zone: &ZoneData) -> bool {
    let hand = weapon(run, zone);
    if hand.caliber.is_none() || hand.loaded >= hand.capacity {
        return false;
    }
    let with = run.weapon.and_then(|uid| run.stack(uid)).and_then(|s| s.loaded_with.clone());
    !compatible(run, zone, &hand, with.as_deref()).is_empty()
}

/// Fills the magazine from the pack. Rounds of another type come out and go back in
/// the pack - this is a text game, not an inventory puzzle (GUNS §3).
pub(crate) fn reload(run: &mut RunState, zone: &ZoneData) -> String {
    let hand = weapon(run, zone);
    let Some(uid) = run.weapon else {
        return "There is nothing in your hands.".into();
    };
    let current = run.stack(uid).and_then(|s| s.loaded_with.clone());
    let Some(id) = compatible(run, zone, &hand, current.as_deref()).first().map(|id| (*id).clone())
    else {
        return "Nothing in the pack fits it.".into();
    };

    if let Some(old) = current.filter(|old| *old != id) {
        let out = hand.loaded;
        if out > 0 {
            run.add_item(&old, out);
            if let Some(stack) = run.stack_mut(uid) {
                stack.loaded = 0;
            }
        }
    }
    let in_gun = run.stack(uid).map_or(0, |s| s.loaded);
    let want = hand.capacity.saturating_sub(in_gun).min(run.count_of(&id));
    if want == 0 {
        return "It is already full.".into();
    }
    run.take_item(&id, want);
    if let Some(stack) = run.stack_mut(uid) {
        stack.loaded = in_gun + want;
        stack.loaded_with = Some(id.clone());
    }
    format!("You feed it {want} of the {}.", zone.items[&id].name)
}

/// The menu for this moment: attack, aim, feed it, or run. Using an item is still
/// the footer’s Inventory, not a fifth verb.
pub(crate) fn menu(combat: &Combat, run: &RunState, zone: &ZoneData) -> Vec<(String, Verb)> {
    let hand = weapon(run, zone);
    let mut menu = Vec::new();
    // An empty gun offers no attack at all: you cannot dry-fire (GUNS §3).
    if !hand.empty() {
        if combat.ap >= hand.ap {
            menu.push((format!("Attack ({} AP)", hand.ap), Verb::Attack));
        }
        if combat.ap >= AP_AIMED {
            menu.push((format!("Aimed Attack ({AP_AIMED} AP)"), Verb::Aimed));
        }
    }
    let ap = reload_ap(run);
    if combat.ap >= ap && can_reload(run, zone) {
        menu.push((format!("Reload ({ap} AP)"), Verb::Reload));
    }
    // Flee costs whatever is left, so it is always on the table.
    menu.push(("Flee".into(), Verb::Flee));
    menu
}

/// The cheapest thing that could be done right now; below it, the turn is over.
/// Flee is not in it - it costs whatever is left, so it would end no turn (GUNS §3).
fn cheapest(run: &RunState, zone: &ZoneData) -> i32 {
    let hand = weapon(run, zone);
    let mut ap = if hand.empty() { i32::MAX } else { hand.ap };
    if can_reload(run, zone) {
        ap = ap.min(reload_ap(run));
    }
    ap
}

// ---- resolving a turn ----

/// Sum of `dice.0` dice of `dice.1` sides. Anomaly damage rolls this raw:
/// the Zone does not care what you are wearing.
pub(crate) fn roll_dice(dice: (u32, u32), rng: &mut Rng) -> i32 {
    (0..dice.0).map(|_| rng.roll(dice.1) as i32).sum()
}

/// GDD §8: damage = dice - armour, and a crit doubles what got *through* the armour
/// rather than ignoring it, so a suit matters against the big hits too. `flat` is
/// what the affixes and the round add, and it doubles with the rest on a crit.
fn damage(dice: (u32, u32), flat: i32, armor: i32, crit: bool, rng: &mut Rng) -> i32 {
    let rolled = roll_dice(dice, rng) + flat;
    let base = (rolled - armor).max(1);
    if crit { base * 2 } else { base }
}

/// What is left of a suit once the round has been through it (GUNS §2). Pierce takes
/// armour off, never below nothing - it cannot make a hit worse than unarmoured.
fn pierced(armor: i32, pierce: i32) -> i32 {
    (armor - pierce).max(0)
}

/// Damage resistance: what the suit is, plus what the Zone put on it.
pub(crate) fn armor_of(run: &RunState, zone: &ZoneData) -> i32 {
    let Some(worn) = run.armor.and_then(|uid| run.stack(uid)) else {
        return 0;
    };
    let base = match zone.items[&worn.id].kind {
        ItemKind::Armor(dr) => dr,
        _ => 0,
    };
    base + loot::bonus(worn, zone, Effect::Armor)
}

fn night_penalty(run: &RunState, zone: &ZoneData) -> i32 {
    if is_night(run.minutes) && !carrying_light(run, zone) {
        NIGHT_PENALTY
    } else {
        0
    }
}

/// Runs one player action, then the enemy's turn if that spent the player's AP.
/// Sets `combat.active = false` when the fight is over.
pub(crate) fn act(
    verb: Verb,
    combat: &mut Combat,
    run: &mut RunState,
    zone: &ZoneData,
    rng: &mut Rng,
) -> String {
    let enemy = zone.enemies[&combat.enemy].clone();

    // GDD §8: a controller takes the turn off you unless you hold onto it.
    if enemy.mind {
        let will = attr(run, zone, INT) * 10;
        if !matches!(check(will, 0, rng), Outcome::Success | Outcome::CritSuccess) {
            combat.ap -= AP_ATTACK;
            let mut lost = format!("The {} is in your head. The moment goes.", enemy.name);
            if let Some(theirs) = end_turn(combat, run, zone, rng) {
                lost = format!("{lost} {theirs}");
            }
            return lost;
        }
    }

    let mut message = match verb {
        Verb::Attack | Verb::Aimed => attack(verb == Verb::Aimed, combat, run, zone, &enemy, rng),
        Verb::Reload => {
            combat.ap -= reload_ap(run);
            reload(run, zone)
        }
        Verb::Flee => {
            combat.ap = 0;
            // GDD §8: a Sneak check, *or* AGI against the fastest thing chasing you.
            let quiet = check_skill(run, Skill::Sneak.index(), 0, 1, rng);
            let legs = check(
                attr(run, zone, AGI) * 10 - (enemy.ap - BASE_AP) * FASTER_PER_AP,
                0,
                rng,
            );
            let away = |o: &Outcome| matches!(o, Outcome::Success | Outcome::CritSuccess);
            if away(&quiet) || away(&legs) {
                combat.active = false;
                return format!("You break contact and lose the {}.", enemy.name);
            }
            "You turn to run and it is still there.".into()
        }
    };

    if combat.hp <= 0 {
        combat.active = false;
        return format!("{message} {}", kill(&enemy, &combat.enemy, run, zone, rng));
    }
    if let Some(theirs) = end_turn(combat, run, zone, rng) {
        message = format!("{message} {theirs}");
    }
    message
}

fn attack(
    aimed: bool,
    combat: &mut Combat,
    run: &mut RunState,
    zone: &ZoneData,
    enemy: &EnemyData,
    rng: &mut Rng,
) -> String {
    let hand = weapon(run, zone);
    combat.ap -= if aimed { AP_AIMED } else { hand.ap };
    // The round leaves the magazine whether or not it hits anything (GUNS §3).
    if hand.caliber.is_some() {
        if let Some(stack) = run.weapon.and_then(|uid| run.stack_mut(uid)) {
            stack.loaded = stack.loaded.saturating_sub(1);
        }
    }

    let modifier = night_penalty(run, zone)
        + hand.to_hit
        + if aimed { AIMED_BONUS } else { 0 };
    let crit_on = if aimed { AIMED_CRIT_ON } else { 1 } + hand.crit * CRIT_PER_POINT;
    let out = check_skill(run, hand.skill.index(), modifier, crit_on, rng);

    match out {
        Outcome::Fail => format!("You miss the {}.", enemy.name),
        Outcome::CritFail => "The shot goes wide and you lose your footing.".into(),
        Outcome::Success | Outcome::CritSuccess => {
            let crit = out == Outcome::CritSuccess;
            let armor = pierced(enemy.armor, hand.pierce);
            let hit = damage(hand.dice, hand.damage, armor, crit, rng);
            combat.hp -= hit;
            if crit {
                format!("You hit the {} clean. {hit} damage.", enemy.name)
            } else {
                format!("You hit the {}. {hit} damage.", enemy.name)
            }
        }
    }
}

fn kill(
    enemy: &EnemyData,
    enemy_id: &str,
    run: &mut RunState,
    zone: &ZoneData,
    rng: &mut Rng,
) -> String {
    run.kills.insert(enemy_id.to_string());
    let luck = attr(run, zone, LCK);
    let mut best: Option<String> = None;
    for (id, n) in &enemy.loot {
        let uid = run.next_uid();
        let stack = loot::roll_item(uid, id, *n, enemy.tier, luck, zone, rng);
        if !stack.is_plain() {
            best = Some(loot::display_name(&stack, zone));
        }
        run.add_stack(stack);
    }
    match best {
        Some(name) => format!("The {} goes down, and leaves a {name}.", enemy.name),
        None => format!("The {} goes down.", enemy.name),
    }
}

/// Ends the player's turn once they can no longer act, runs the enemy's, and hands
/// back a fresh pool of AP. Also called after using an item in the inventory.
pub(crate) fn end_turn(
    combat: &mut Combat,
    run: &mut RunState,
    zone: &ZoneData,
    rng: &mut Rng,
) -> Option<String> {
    if !combat.active || combat.ap >= cheapest(run, zone) {
        return None;
    }
    let enemy = zone.enemies[&combat.enemy].clone();
    let message = enemy_turn(&enemy, combat, run, zone, rng);
    combat.ap = turn_ap(run, zone);
    Some(message)
}

/// The two-state machine from GDD §8: flee when hurt, otherwise attack.
fn enemy_turn(
    enemy: &EnemyData,
    combat: &mut Combat,
    run: &mut RunState,
    zone: &ZoneData,
    rng: &mut Rng,
) -> String {
    let mut ap = enemy.ap;
    let mut said = Vec::new();

    // Flee: hurt badly enough to want out, and the kind of thing that runs.
    if enemy.flees && combat.hp * FLEE_BELOW < enemy.hp {
        // It bolts. Whether it clears is speed: its AP against your AGI (5 is the
        // base attribute), so a quick stalker can still run a wounded thing down.
        let chance = 50
            + (enemy.ap - BASE_AP) * FASTER_PER_AP
            - (attr(run, zone, AGI) - 5) * FASTER_PER_AP;
        if matches!(check(chance, 0, rng), Outcome::Success | Outcome::CritSuccess) {
            combat.active = false;
            return format!("The {} breaks and is gone.", enemy.name);
        }
        return format!("The {} turns to run, but you head it off.", enemy.name);
    }

    // Attack, as often as the AP allows.
    while ap >= AP_ATTACK {
        ap -= AP_ATTACK;
        match check(enemy.skill, 0, rng) {
            Outcome::Fail | Outcome::CritFail => {
                said.push(format!("The {} misses.", enemy.name));
            }
            out => {
                let crit = out == Outcome::CritSuccess;
                let hit = damage(enemy.dice, 0, armor_of(run, zone), crit, rng);
                run.hp -= hit;
                said.push(format!("The {} hits you for {hit}.", enemy.name));
            }
        }
    }

    if said.is_empty() {
        format!("The {} circles.", enemy.name)
    } else {
        said.join(" ")
    }
}

// ---- the screen ----

pub(crate) fn build_combat_grid(
    grid: &mut TileGrid,
    zone: &ZoneData,
    run: &RunState,
    clock: &GameClock,
    fields: &Fields,
    combat: &Combat,
    area_id: &str,
    message: &str,
) {
    grid.clear();
    draw_art(grid, zone, run, fields, area_id);

    let enemy = &zone.enemies[&combat.enemy];
    let line = format!(
        "{:<16} HP {:>3}/{:<3}",
        enemy.name,
        combat.hp.max(0),
        enemy.hp,
    );
    grid.text(0, ENEMY_ROW, &line, PALETTE.red, true);

    let hand = weapon(run, zone);
    let plus = if hand.damage > 0 { format!("+{}", hand.damage) } else { String::new() };
    // A gun says what is left in it; a blade has nothing to say (GUNS §4).
    let ammo = match hand.caliber {
        Some(_) => format!("  AMMO {}/{}", hand.loaded, hand.capacity),
        None => String::new(),
    };
    let mine = format!(
        "{:<16} HP {:>3}/{:<3}  AP {}   {}d{}{plus} {}  DR {}{ammo}",
        "You",
        run.hp.max(0),
        run.max_hp,
        combat.ap,
        hand.dice.0,
        hand.dice.1,
        crate::run::SKILL_NAMES[hand.skill.index()],
        armor_of(run, zone),
    );
    grid.text(0, YOU_ROW, &mine, PALETTE.status, false);

    for (i, (label, _)) in menu(combat, run, zone).iter().enumerate() {
        row(grid, 0, MENU_ROW + i, i == combat.sel, PALETTE.menu, false, label);
    }

    draw_message(grid, message);
    draw_chrome(grid, run, clock);
}

// ---- tests (SPEC §7) ----

#[cfg(test)]
mod tests {
    use super::*;
    use crate::area::load_zone;
    use std::path::Path;

    /// Puts one of `id` in the pack and in the hand, fed if it needs feeding.
    fn equip(run: &mut RunState, zone: &ZoneData, id: &str) {
        run.add_item(id, 1);
        run.weapon = Some(run.items.iter().find(|s| s.id == id).unwrap().uid);
        if let ItemKind::Weapon { ammo: Some(caliber), .. } = zone.items[id].kind {
            let round = zone.ammo_of(caliber).next().expect("a round that fits").clone();
            run.add_item(&round, 40);
            reload(run, zone);
        }
    }

    fn fixture() -> (ZoneData, RunState, Rng, Combat) {
        let zone = load_zone(Path::new("assets/data"));
        let run = RunState::roll(0, &[0, 2, 3], &mut Rng::new(3));
        let mut combat = Combat::default();
        let mut rng = Rng::new(4242);
        let mut run = run;
        start("flesh", &mut combat, &mut run, &zone, &mut rng);
        (zone, run, rng, combat)
    }

    #[test]
    fn a_crit_doubles_what_armour_let_through() {
        let mut rng = Rng::new(1);
        // 4d1 is always 4: a plain hit is 4 - 3 armour = 1, and a crit is that 1 doubled.
        assert_eq!(damage((4, 1), 0, 3, false, &mut rng), 1);
        assert_eq!(damage((4, 1), 0, 3, true, &mut rng), 2);
        // Armour never reduces a hit below 1.
        assert_eq!(damage((1, 1), 0, 99, false, &mut rng), 1);
        // An affix adds before armour, and armour applies before the crit doubles.
        assert_eq!(damage((4, 1), 3, 3, false, &mut rng), 4);
        assert_eq!(damage((4, 1), 3, 3, true, &mut rng), 8);
    }

    #[test]
    fn the_menu_is_just_attack_aim_and_flee() {
        let (zone, run, _, combat) = fixture();

        // Knife in hand: no distance to close and nothing to feed, so three verbs.
        let verbs: Vec<Verb> = menu(&combat, &run, &zone).iter().map(|(_, v)| *v).collect();
        assert_eq!(verbs, vec![Verb::Attack, Verb::Aimed, Verb::Flee]);

        // An empty AP pool hides the swings but keeps the escape hatch.
        let mut combat = combat;
        combat.ap = 0;
        let m = menu(&combat, &run, &zone);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].1, Verb::Flee);
    }

    #[test]
    fn pierce_comes_off_armour_and_floors_at_nothing() {
        // GUNS §2: a round that pierces takes armour off, and can never make a hit
        // worse than an unarmoured one.
        assert_eq!(pierced(3, 2), 1);
        assert_eq!(pierced(3, 0), 3);
        assert_eq!(pierced(1, 3), 0, "pierce cannot turn armour into a bonus");
    }

    #[test]
    fn an_empty_gun_offers_no_attack_and_a_reload() {
        let (zone, mut run, _, mut combat) = fixture();
        equip(&mut run, &zone, "pistol");
        combat.ap = 12;

        let verbs = |run: &RunState| -> Vec<Verb> {
            menu(&combat, run, &zone).iter().map(|(_, v)| *v).collect()
        };
        // Full: nothing to feed, so the menu is the old three.
        assert_eq!(verbs(&run), vec![Verb::Attack, Verb::Aimed, Verb::Flee]);

        // A round down, and topping it up is on the table.
        run.stack_mut(run.weapon.unwrap()).unwrap().loaded -= 1;
        assert_eq!(verbs(&run), vec![Verb::Attack, Verb::Aimed, Verb::Reload, Verb::Flee]);

        // Empty: you cannot dry-fire, and feeding it is the only thing left to do.
        run.stack_mut(run.weapon.unwrap()).unwrap().loaded = 0;
        assert_eq!(verbs(&run), vec![Verb::Reload, Verb::Flee]);

        // Empty with nothing in the pack either: only the way out.
        let round = run.stack(run.weapon.unwrap()).unwrap().loaded_with.clone().unwrap();
        let all = run.count_of(&round);
        run.take_item(&round, all);
        assert_eq!(verbs(&run), vec![Verb::Flee]);

        // A blade is never empty and never asks to be fed.
        equip(&mut run, &zone, "knife");
        assert_eq!(verbs(&run), vec![Verb::Attack, Verb::Aimed, Verb::Flee]);
    }

    #[test]
    fn a_reload_fills_the_magazine_and_never_loses_a_round() {
        let (zone, mut run, _, _) = fixture();
        equip(&mut run, &zone, "pistol"); // 8 in the magazine, 40 rounds bought
        let uid = run.weapon.unwrap();
        let round = run.stack(uid).unwrap().loaded_with.clone().unwrap();
        let total = |run: &RunState| {
            run.count_of(&round) + run.stack(uid).map_or(0, |s| s.loaded)
        };
        assert_eq!(run.stack(uid).unwrap().loaded, 8);
        let held = total(&run);

        // Three shots out, three back in, and nothing has evaporated.
        run.stack_mut(uid).unwrap().loaded = 5;
        let msg = reload(&mut run, &zone);
        assert_eq!(run.stack(uid).unwrap().loaded, 8, "{msg}");
        assert_eq!(total(&run), held - 3, "the rounds came out of the pack");

        // A full magazine takes nothing.
        assert!(reload(&mut run, &zone).starts_with("It is already full"));
        assert_eq!(total(&run), held - 3);

        // Run that type out and the reload falls to whatever else fits - and the
        // five still in the magazine come out and go back in the pack (GUNS §3).
        let left = run.count_of(&round);
        run.take_item(&round, left);
        run.add_item("pistol_ap", 8);
        run.stack_mut(uid).unwrap().loaded = 5;
        reload(&mut run, &zone);
        assert_eq!(run.stack(uid).unwrap().loaded_with.as_deref(), Some("pistol_ap"));
        assert_eq!(run.stack(uid).unwrap().loaded, 8);
        assert_eq!(run.count_of(&round), 5, "the old rounds went back in the pack");
        assert_eq!(run.count_of("pistol_ap"), 0, "and the new ones came out of it");
    }

    #[test]
    fn a_good_shot_reloads_faster() {
        let (_, mut run, _, _) = fixture();
        // GUNS §3: the breakpoint is a real edge, not a curve.
        run.skills[Skill::SmallGuns.index()] = RELOAD_FAST_SKILL - 1;
        assert_eq!(reload_ap(&run), AP_RELOAD);
        run.skills[Skill::SmallGuns.index()] = RELOAD_FAST_SKILL;
        assert_eq!(reload_ap(&run), AP_RELOAD_FAST);
    }

    #[test]
    fn no_stack_of_mods_takes_an_attack_below_two_ap() {
        let (zone, mut run, _, _) = fixture();
        equip(&mut run, &zone, "pistol");
        let uid = run.weapon.unwrap();
        // One brake is 2 AP; three of them would be zero, and zero is a free turn
        // for ever. The floor is what stops that (GUNS §1).
        run.stack_mut(uid).unwrap().mods = vec!["muzzle".into()];
        assert_eq!(weapon(&run, &zone).ap, 2);
        run.stack_mut(uid).unwrap().mods =
            vec!["muzzle".into(), "muzzle".into(), "muzzle".into()];
        assert_eq!(weapon(&run, &zone).ap, 2, "the floor holds");
    }

    #[test]
    fn the_turn_ends_when_nothing_is_affordable() {
        let (zone, mut run, mut rng, mut combat) = fixture();
        equip(&mut run, &zone, "pistol");

        // Two points, an empty gun and nothing to feed it with: the turn has to end
        // rather than sit there with a menu of one impossible verb.
        let uid = run.weapon.unwrap();
        let round = run.stack(uid).unwrap().loaded_with.clone().unwrap();
        let all = run.count_of(&round);
        run.take_item(&round, all);
        run.stack_mut(uid).unwrap().loaded = 0;
        combat.ap = 2;
        assert!(end_turn(&mut combat, &mut run, &zone, &mut rng).is_some());
        assert_eq!(combat.ap, turn_ap(&run, &zone), "a fresh turn came back");

        // With rounds in the pack, that same 2 AP still buys a reload, so the turn
        // is not over yet.
        run.add_item(&round, 8);
        combat.ap = 2;
        assert!(end_turn(&mut combat, &mut run, &zone, &mut rng).is_none());
    }

    #[test]
    fn a_flesh_bites_immediately_with_no_distance_to_close() {
        let (zone, mut run, mut rng, mut combat) = fixture();
        let flesh = zone.enemies["flesh"].clone();

        // No approach turn: the first thing it does is swing.
        let msg = enemy_turn(&flesh, &mut combat, &mut run, &zone, &mut rng);
        assert!(msg.contains("misses") || msg.contains("hits you"), "{msg}");
    }

    #[test]
    fn a_wounded_flesh_runs_and_a_dead_one_drops_loot() {
        let (zone, mut run, mut rng, mut combat) = fixture();
        let flesh = zone.enemies["flesh"].clone();

        // Under a fifth of its health, it wants out and never swings again.
        combat.hp = flesh.hp / FLEE_BELOW - 1; // under a fifth, not at it
        let hp = run.hp;
        let msg = enemy_turn(&flesh, &mut combat, &mut run, &zone, &mut rng);
        assert_eq!(run.hp, hp, "a fleeing flesh does not bite: {msg}");
        assert!(
            msg.contains("breaks and is gone") || msg.contains("head it off"),
            "it runs, one way or the other: {msg}"
        );

        // Killing it hands over the loot, once.
        let (_, mut run, _, _) = fixture();
        assert_eq!(run.count_of(&flesh.loot[0].0), 0);
        let mut rng2 = Rng::new(5);
        kill(&flesh, "flesh", &mut run, &zone, &mut rng2);
        assert!(run.count_of(&flesh.loot[0].0) > 0);
        assert!(run.kills.contains("flesh"), "a kill is remembered for the jobs");
    }

    #[test]
    fn a_bloodsucker_that_is_not_spotted_opens_the_fight_on_top_of_you() {
        let (zone, mut run, mut rng, mut combat) = fixture();
        assert!(zone.enemies["bloodsucker"].ambush);

        // Blind to it: it is already inside your guard and has had its go.
        run.attrs[PER] = 1;
        let msg = start("bloodsucker", &mut combat, &mut run, &zone, &mut rng);
        assert!(msg.contains("The air comes apart"), "{msg}");

        // Sharp enough to see it: the fight opens clean, with your guard up.
        run.attrs[PER] = 10;
        let msg = start("bloodsucker", &mut combat, &mut run, &zone, &mut rng);
        assert!(msg.contains("catch the shimmer"), "{msg}");
    }

    #[test]
    fn a_controller_takes_the_turn_off_a_weak_mind() {
        let (zone, mut run, mut rng, mut combat) = fixture();
        assert!(zone.enemies["controller"].mind);
        let mut run2 = run.clone();
        start("controller", &mut combat, &mut run2, &zone, &mut rng);
        run = run2;
        run.attrs[INT] = 1;
        equip(&mut run, &zone, "pistol");
        run.skills[Skill::SmallGuns.index()] = 100;

        let mut lost = 0;
        for _ in 0..10 {
            combat.hp = 999; // it is not dying today; the point is whose turn it is
            combat.ap = turn_ap(&run, &zone);
            let before = combat.hp;
            let msg = act(Verb::Attack, &mut combat, &mut run, &zone, &mut rng);
            if msg.contains("in your head") {
                lost += 1;
                assert_eq!(combat.hp, before, "a lost action does no damage");
            }
        }
        assert!(lost > 0, "a 10 INT-check should fail sometimes");
    }

    #[test]
    fn a_fight_ends_one_way_or_the_other() {
        let (zone, mut run, mut rng, mut combat) = fixture();
        equip(&mut run, &zone, "pistol");
        run.skills[Skill::SmallGuns.index()] = 90;

        // Drive it like a player would: swing until something gives.
        for _ in 0..80 {
            if !combat.active || run.hp <= 0 {
                break;
            }
            // Swing, feed it when it is empty, run when there is nothing else -
            // the same order a player works down the menu in.
            let m = menu(&combat, &run, &zone);
            let has = |v: Verb| m.iter().any(|(_, x)| *x == v);
            let verb = if has(Verb::Attack) {
                Verb::Attack
            } else if has(Verb::Reload) {
                Verb::Reload
            } else {
                Verb::Flee
            };
            act(verb, &mut combat, &mut run, &zone, &mut rng);
        }
        assert!(!combat.active, "the fight resolved");
        assert!(combat.ap > 0, "a fresh turn is always handed back");
    }
}
