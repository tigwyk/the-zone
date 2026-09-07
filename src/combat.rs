//! Turn-based, AP-driven combat at close quarters (GDD §8). No grid, no distance:
//! everything is already in reach, and the area art stays up while the combatants
//! are listed under it.

use bevy::prelude::*;

use crate::area::{draw_art, EnemyData, ItemKind, ZoneData};
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
/// The cheapest thing anyone can do. Below this, the turn is over.
const AP_MIN: i32 = AP_ATTACK;

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
}

fn weapon(run: &RunState, zone: &ZoneData) -> InHand {
    let held = run.weapon.and_then(|uid| run.stack(uid));
    let kind = held.map(|s| zone.items[&s.id].kind);
    let (dice, skill) = match kind {
        Some(ItemKind::Weapon { dice, skill }) => (dice, skill),
        _ => (UNARMED, Skill::Melee),
    };
    let of = |effect| held.map_or(0, |s| loot::bonus(s, zone, effect));
    InHand {
        dice,
        skill,
        damage: of(Effect::Damage),
        to_hit: of(Effect::ToHit),
        crit: of(Effect::Crit).max(0) as u32,
    }
}

/// The menu for this moment: attack, aim, or run. Using an item is the footer's
/// Inventory, not a fourth verb.
pub(crate) fn menu(combat: &Combat) -> Vec<(String, Verb)> {
    let mut menu = Vec::new();
    if combat.ap >= AP_ATTACK {
        menu.push((format!("Attack ({AP_ATTACK} AP)"), Verb::Attack));
    }
    if combat.ap >= AP_AIMED {
        menu.push((format!("Aimed Attack ({AP_AIMED} AP)"), Verb::Aimed));
    }
    // Flee costs whatever is left, so it is always on the table.
    menu.push(("Flee".into(), Verb::Flee));
    menu
}

// ---- resolving a turn ----

/// Sum of `dice.0` dice of `dice.1` sides. Anomaly damage rolls this raw:
/// the Zone does not care what you are wearing.
pub(crate) fn roll_dice(dice: (u32, u32), rng: &mut Rng) -> i32 {
    (0..dice.0).map(|_| rng.roll(dice.1) as i32).sum()
}

/// GDD §8: damage = dice - armour, and a crit doubles what got *through* the armour
/// rather than ignoring it, so a suit matters against the big hits too. `flat` is
/// what the affixes add, and it doubles with the rest on a crit.
fn damage(dice: (u32, u32), flat: i32, armor: i32, crit: bool, rng: &mut Rng) -> i32 {
    let rolled = roll_dice(dice, rng) + flat;
    let base = (rolled - armor).max(1);
    if crit { base * 2 } else { base }
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
    combat.ap -= if aimed { AP_AIMED } else { AP_ATTACK };

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
            let hit = damage(hand.dice, hand.damage, enemy.armor, crit, rng);
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
    if !combat.active || combat.ap >= AP_MIN {
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
    let mine = format!(
        "{:<16} HP {:>3}/{:<3}  AP {}   {}d{}{plus} {}  DR {}",
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

    for (i, (label, _)) in menu(combat).iter().enumerate() {
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

    /// Puts one of `id` in the pack and in the hand.
    fn equip(run: &mut RunState, id: &str) {
        run.add_item(id, 1);
        run.weapon = Some(run.items.iter().find(|s| s.id == id).unwrap().uid);
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
        let (_, _, _, combat) = fixture();

        // No distance to close, so the whole menu is three verbs, in order.
        let verbs: Vec<Verb> = menu(&combat).iter().map(|(_, v)| *v).collect();
        assert_eq!(verbs, vec![Verb::Attack, Verb::Aimed, Verb::Flee]);

        // An empty AP pool hides the swings but keeps the escape hatch.
        let mut combat = combat;
        combat.ap = 0;
        let m = menu(&combat);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].1, Verb::Flee);
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
        equip(&mut run, "pistol");
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
        equip(&mut run, "pistol");
        run.skills[Skill::SmallGuns.index()] = 90;

        // Drive it like a player would: swing until something gives.
        for _ in 0..80 {
            if !combat.active || run.hp <= 0 {
                break;
            }
            let verb = menu(&combat)
                .iter()
                .find(|(_, v)| *v == Verb::Attack)
                .map(|(_, v)| *v)
                .unwrap_or(Verb::Flee);
            act(verb, &mut combat, &mut run, &zone, &mut rng);
        }
        assert!(!combat.active, "the fight resolved");
        assert!(combat.ap > 0, "a fresh turn is always handed back");
    }
}
