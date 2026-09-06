//! Turn-based, AP-driven combat over three range bands (GDD §8). No grid: the area
//! art stays up and the combatants are listed under it.

use bevy::prelude::*;

use crate::area::{draw_art, EnemyData, ItemKind, ZoneData};
use crate::render::{TileGrid, PALETTE};
use crate::run::{check, check_skill, Outcome, Rng, RunState, Skill, AGI};
use crate::screens::draw_chrome;
use crate::sim::{attr, carrying_light, is_night, Fields, GameClock, NIGHT_PENALTY};

// Fixed row map (SPEC §4), shared with the area screen.
const ENEMY_ROW: usize = 19;
const YOU_ROW: usize = 20;
const MENU_ROW: usize = 22;
const MESSAGE_ROW: usize = 27;

// AP costs (GDD §8).
pub(crate) const AP_ATTACK: i32 = 4;
pub(crate) const AP_AIMED: i32 = 6;
pub(crate) const AP_MOVE: i32 = 3;
pub(crate) const AP_ITEM: i32 = 4;
/// The cheapest thing anyone can do. Below this, the turn is over.
const AP_MIN: i32 = AP_MOVE;

const AIMED_BONUS: i32 = 20;
/// An aimed shot crits on 5, not just on 1 (GDD §8).
const AIMED_CRIT_ON: u32 = 5;
const FAR_PENALTY: i32 = -20;
/// Bare hands, for a stalker who sold their knife (GDD §8).
const UNARMED: (u32, u32) = (1, 3);
/// GDD §8: an enemy that runs does so under a fifth of its health.
const FLEE_BELOW: i32 = 5;

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Band {
    #[default]
    Far,
    Near,
    Melee,
}

impl Band {
    fn name(self) -> &'static str {
        match self {
            Band::Far => "far",
            Band::Near => "near",
            Band::Melee => "melee",
        }
    }

    /// To-hit modifier for shooting across this band (GDD §8).
    fn to_hit(self) -> i32 {
        match self {
            Band::Far => FAR_PENALTY,
            Band::Near | Band::Melee => 0,
        }
    }

    fn closer(self) -> Band {
        match self {
            Band::Far => Band::Near,
            _ => Band::Melee,
        }
    }

    fn farther(self) -> Band {
        match self {
            Band::Melee => Band::Near,
            _ => Band::Far,
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct Combat {
    pub active: bool,
    pub enemy: String,
    pub hp: i32,
    pub band: Band,
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
    run: &RunState,
    zone: &ZoneData,
) -> String {
    let enemy = &zone.enemies[enemy_id];
    *combat = Combat {
        active: true,
        enemy: enemy_id.to_string(),
        hp: enemy.hp,
        band: Band::Far,
        ap: turn_ap(run, zone),
        sel: 0,
    };
    format!("A {} comes at you out of the ground.", enemy.name)
}

// ---- what the player can do right now ----

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Verb {
    Attack,
    Aimed,
    CloseIn,
    FallBack,
    Flee,
}

/// The weapon in hand: its dice and the skill that swings it. Bare hands otherwise.
fn weapon(run: &RunState, zone: &ZoneData) -> ((u32, u32), Skill) {
    match run.weapon.as_ref().map(|id| zone.items[id].kind) {
        Some(ItemKind::Weapon { dice, skill }) => (dice, skill),
        _ => (UNARMED, Skill::Melee),
    }
}

/// GDD §8: melee weapons only reach at the melee band.
fn can_reach(skill: Skill, band: Band) -> bool {
    skill != Skill::Melee || band == Band::Melee
}

/// The menu for this moment. Never more than five, so it fits rows 22-26; using an
/// item is the footer's Inventory, not a sixth verb.
pub(crate) fn menu(combat: &Combat, run: &RunState, zone: &ZoneData) -> Vec<(String, Verb)> {
    let (_, skill) = weapon(run, zone);
    let reaches = can_reach(skill, combat.band);
    let mut menu = Vec::new();
    if reaches && combat.ap >= AP_ATTACK {
        menu.push((format!("Attack ({AP_ATTACK} AP)"), Verb::Attack));
    }
    if reaches && combat.ap >= AP_AIMED {
        menu.push((format!("Aimed Attack ({AP_AIMED} AP)"), Verb::Aimed));
    }
    if combat.band != Band::Melee && combat.ap >= AP_MOVE {
        menu.push((format!("Close In ({AP_MOVE} AP)"), Verb::CloseIn));
    }
    if combat.band != Band::Far && combat.ap >= AP_MOVE {
        menu.push((format!("Fall Back ({AP_MOVE} AP)"), Verb::FallBack));
    }
    // Flee costs whatever is left, so it is always on the table.
    menu.push(("Flee".into(), Verb::Flee));
    menu
}

// ---- resolving a turn ----

fn roll_dice(dice: (u32, u32), rng: &mut Rng) -> i32 {
    (0..dice.0).map(|_| rng.roll(dice.1) as i32).sum()
}

/// GDD §8: damage = dice - armour, and a crit doubles it and ignores armour.
fn damage(dice: (u32, u32), armor: i32, crit: bool, rng: &mut Rng) -> i32 {
    let rolled = roll_dice(dice, rng);
    if crit {
        (rolled * 2).max(1)
    } else {
        (rolled - armor).max(1)
    }
}

fn armor_of(run: &RunState, zone: &ZoneData) -> i32 {
    match run.armor.as_ref().map(|id| zone.items[id].kind) {
        Some(ItemKind::Armor(dr)) => dr,
        _ => 0,
    }
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
    let mut message = match verb {
        Verb::Attack | Verb::Aimed => attack(verb == Verb::Aimed, combat, run, zone, &enemy, rng),
        Verb::CloseIn => {
            combat.ap -= AP_MOVE;
            combat.band = combat.band.closer();
            format!("You close to {}.", combat.band.name())
        }
        Verb::FallBack => {
            combat.ap -= AP_MOVE;
            combat.band = combat.band.farther();
            format!("You give ground to {}.", combat.band.name())
        }
        Verb::Flee => {
            combat.ap = 0;
            // GDD §8: fleeing is a Sneak check and costs the whole turn.
            let out = check_skill(run, Skill::Sneak.index(), combat.band.to_hit(), 1, rng);
            if matches!(out, Outcome::Success | Outcome::CritSuccess) {
                combat.active = false;
                return format!("You break contact and lose the {}.", enemy.name);
            }
            "You turn to run and it is still there.".into()
        }
    };

    if combat.hp <= 0 {
        combat.active = false;
        return format!("{message} {}", kill(&enemy, run));
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
    let (dice, skill) = weapon(run, zone);
    combat.ap -= if aimed { AP_AIMED } else { AP_ATTACK };

    let modifier = combat.band.to_hit()
        + night_penalty(run, zone)
        + if aimed { AIMED_BONUS } else { 0 };
    let crit_on = if aimed { AIMED_CRIT_ON } else { 1 };
    let out = check_skill(run, skill.index(), modifier, crit_on, rng);

    match out {
        Outcome::Fail => format!("You miss the {}.", enemy.name),
        Outcome::CritFail => "The shot goes wide and you lose your footing.".into(),
        Outcome::Success | Outcome::CritSuccess => {
            let crit = out == Outcome::CritSuccess;
            let hit = damage(dice, enemy.armor, crit, rng);
            combat.hp -= hit;
            if crit {
                format!("You hit the {} clean. {hit} damage.", enemy.name)
            } else {
                format!("You hit the {}. {hit} damage.", enemy.name)
            }
        }
    }
}

fn kill(enemy: &EnemyData, run: &mut RunState) -> String {
    let mut taken = Vec::new();
    for (id, n) in &enemy.loot {
        run.add_item(id, *n);
        taken.push(id.clone());
    }
    if taken.is_empty() {
        format!("The {} goes down.", enemy.name)
    } else {
        format!("The {} goes down. You cut something loose.", enemy.name)
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

/// The three-state machine from GDD §8: flee, approach, attack.
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
        // One band per turn, and only away from far does it actually get clear -
        // so a wounded thing can still be caught and finished.
        if combat.band == Band::Far {
            combat.active = false;
            return format!("The {} breaks and is gone.", enemy.name);
        }
        combat.band = combat.band.farther();
        return format!("The {} backs away to {}, bleeding.", enemy.name, combat.band.name());
    }

    // Approach: something that can only bite has to reach you first.
    while ap >= AP_MOVE && enemy.melee_only && combat.band != Band::Melee {
        combat.band = combat.band.closer();
        ap -= AP_MOVE;
        said.push(format!("The {} closes to {}.", enemy.name, combat.band.name()));
    }

    // Attack, as often as the AP allows.
    while ap >= AP_ATTACK {
        ap -= AP_ATTACK;
        if enemy.melee_only && combat.band != Band::Melee {
            break;
        }
        let modifier = combat.band.to_hit();
        match check(enemy.skill, modifier, rng) {
            Outcome::Fail | Outcome::CritFail => {
                said.push(format!("The {} misses.", enemy.name));
            }
            out => {
                let crit = out == Outcome::CritSuccess;
                let hit = damage(enemy.dice, armor_of(run, zone), crit, rng);
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
        "{:<16} HP {:>3}/{:<3}  {}",
        enemy.name,
        combat.hp.max(0),
        enemy.hp,
        combat.band.name()
    );
    grid.text(0, ENEMY_ROW, &line, PALETTE.red, true);

    let (dice, skill) = weapon(run, zone);
    let mine = format!(
        "{:<16} HP {:>3}/{:<3}  AP {}   {}d{} {}",
        "You",
        run.hp.max(0),
        run.max_hp,
        combat.ap,
        dice.0,
        dice.1,
        crate::run::SKILL_NAMES[skill.index()]
    );
    grid.text(0, YOU_ROW, &mine, PALETTE.status, false);

    for (i, (label, _)) in menu(combat, run, zone).iter().enumerate() {
        let fg = if i == combat.sel { PALETTE.menu_sel } else { PALETTE.menu };
        grid.text(0, MENU_ROW + i, if i == combat.sel { "> " } else { "  " }, fg, false);
        grid.text(2, MENU_ROW + i, label, fg, false);
    }

    grid.text(0, MESSAGE_ROW, message, PALETTE.desc, false);
    draw_chrome(grid, run, clock);
}

// ---- tests (SPEC §7) ----

#[cfg(test)]
mod tests {
    use super::*;
    use crate::area::load_zone;
    use std::path::Path;

    fn fixture() -> (ZoneData, RunState, Rng, Combat) {
        let zone = load_zone(Path::new("assets/data"));
        let run = RunState::roll(0, &[0, 2, 3], &mut Rng::new(3));
        let mut combat = Combat::default();
        start("flesh", &mut combat, &run, &zone);
        (zone, run, Rng::new(4242), combat)
    }

    #[test]
    fn bands_carry_the_gdd_to_hit_table() {
        assert_eq!(Band::Far.to_hit(), -20);
        assert_eq!(Band::Near.to_hit(), 0);
        assert_eq!(Band::Melee.to_hit(), 0);
        assert_eq!(Band::Far.closer(), Band::Near);
        assert_eq!(Band::Near.closer(), Band::Melee);
        assert_eq!(Band::Melee.closer(), Band::Melee, "melee is as close as it gets");
        assert_eq!(Band::Melee.farther(), Band::Near);
        assert_eq!(Band::Far.farther(), Band::Far);
    }

    #[test]
    fn a_crit_doubles_the_dice_and_ignores_armour() {
        let mut rng = Rng::new(1);
        // 4d1 is always 4: plain hit is 4 - 3 armour = 1, a crit is 8 through it.
        assert_eq!(damage((4, 1), 3, false, &mut rng), 1);
        assert_eq!(damage((4, 1), 3, true, &mut rng), 8);
        // Armour never reduces a hit below 1.
        assert_eq!(damage((1, 1), 99, false, &mut rng), 1);
    }

    #[test]
    fn a_melee_weapon_only_reaches_at_the_melee_band() {
        let (zone, mut run, _, mut combat) = fixture();
        run.weapon = Some("knife".into());

        combat.band = Band::Far;
        let far: Vec<Verb> = menu(&combat, &run, &zone).iter().map(|(_, v)| *v).collect();
        assert!(!far.contains(&Verb::Attack), "no swinging a knife across a field");
        assert!(far.contains(&Verb::CloseIn));

        combat.band = Band::Melee;
        let close: Vec<Verb> = menu(&combat, &run, &zone).iter().map(|(_, v)| *v).collect();
        assert!(close.contains(&Verb::Attack) && close.contains(&Verb::Aimed));
        assert!(!close.contains(&Verb::CloseIn), "already there");

        // A pistol reaches from anywhere, and the menu never outgrows its five rows.
        run.weapon = Some("pistol".into());
        for band in [Band::Far, Band::Near, Band::Melee] {
            combat.band = band;
            let m = menu(&combat, &run, &zone);
            assert!(m.iter().any(|(_, v)| *v == Verb::Attack));
            assert!(m.len() <= 5, "{band:?} gave {} entries", m.len());
        }
    }

    #[test]
    fn a_mutant_closes_the_distance_before_it_can_bite() {
        let (zone, mut run, mut rng, mut combat) = fixture();
        let flesh = zone.enemies["flesh"].clone();
        assert!(flesh.melee_only);
        assert_eq!(combat.band, Band::Far);

        let hp = run.hp;
        let msg = enemy_turn(&flesh, &mut combat, &mut run, &zone, &mut rng);
        assert_eq!(combat.band, Band::Melee, "{msg}");
        assert_eq!(run.hp, hp, "it spent the turn running, not biting");

        // Now that it is on top of you it can actually land something.
        let mut bitten = false;
        for _ in 0..12 {
            enemy_turn(&flesh, &mut combat, &mut run, &zone, &mut rng);
            if run.hp < hp {
                bitten = true;
                break;
            }
        }
        assert!(bitten, "a Flesh at melee has to connect eventually");
    }

    #[test]
    fn a_wounded_flesh_runs_and_a_dead_one_drops_loot() {
        let (zone, mut run, mut rng, mut combat) = fixture();
        let flesh = zone.enemies["flesh"].clone();

        // Under a fifth of its health, it wants out, and from far it is gone.
        combat.band = Band::Melee;
        combat.hp = flesh.hp / FLEE_BELOW - 1; // under a fifth, not at it
        enemy_turn(&flesh, &mut combat, &mut run, &zone, &mut rng);
        assert_eq!(combat.band, Band::Near, "one band a turn, so it can be caught");
        enemy_turn(&flesh, &mut combat, &mut run, &zone, &mut rng);
        assert_eq!(combat.band, Band::Far);
        assert!(combat.active, "still catchable at far");
        enemy_turn(&flesh, &mut combat, &mut run, &zone, &mut rng);
        assert!(!combat.active, "a turn later it is gone");

        // Killing it hands over the loot, once.
        let (_, mut run, _, _) = fixture();
        assert!(!run.items.iter().any(|(i, _)| *i == flesh.loot[0].0));
        kill(&flesh, &mut run);
        assert_eq!(
            run.items.iter().find(|(i, _)| *i == flesh.loot[0].0).unwrap().1,
            flesh.loot[0].1
        );
    }

    #[test]
    fn a_fight_ends_one_way_or_the_other() {
        let (zone, mut run, mut rng, mut combat) = fixture();
        run.weapon = Some("pistol".into());
        run.skills[Skill::SmallGuns.index()] = 90;

        // Drive it like a player would: close, then shoot until something gives.
        for _ in 0..80 {
            if !combat.active || run.hp <= 0 {
                break;
            }
            let m = menu(&combat, &run, &zone);
            let verb = m
                .iter()
                .find(|(_, v)| *v == Verb::Attack)
                .map(|(_, v)| *v)
                .unwrap_or(Verb::CloseIn);
            act(verb, &mut combat, &mut run, &zone, &mut rng);
        }
        assert!(!combat.active, "the fight resolved");
        assert!(combat.ap > 0, "a fresh turn is always handed back");
    }
}
