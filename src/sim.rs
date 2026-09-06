//! The Zone acting on you: the clock, radiation, emissions, and anomaly fields.
//! `RunState` holds the stalker; this module holds what the place does to them.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::area::{Action, AnomalyData, AreaData, Gate, ItemKind, VendorStock, ZoneData};
use crate::run::{check, Outcome, Rng, RunState, PER};

// ---- the clock (GDD §5) ----

pub(crate) const MINUTES_PER_DAY: u32 = 1440;
const NIGHT_FROM: u32 = 20;
const NIGHT_TO: u32 = 5;
/// PER checks are 10 harder at night without a light source (GDD §5).
pub(crate) const NIGHT_PENALTY: i32 = -10;

/// Time cost of one action (GDD §5). Trade and talk are free.
pub(crate) fn action_minutes(action: &Action) -> u32 {
    match action {
        Action::Travel(_) | Action::PushThrough => 60,
        Action::Rest => crate::run::REST_MINUTES,
        // GDD §5: a careful look at a field is 10 minutes.
        Action::Scan | Action::ThrowBolt | Action::TakeArtifact => 10,
        Action::Say(_) | Action::Trade(_) | Action::SetFlag(_) => 0,
    }
}

pub(crate) fn is_night(minutes: u32) -> bool {
    let hour = (minutes % MINUTES_PER_DAY) / 60;
    hour >= NIGHT_FROM || hour < NIGHT_TO
}

// ---- emissions (GDD §5) ----

const EMISSION_MIN_DAYS: u32 = 3;
const EMISSION_DAY_SPREAD: u32 = 3; // 3..5 days
/// One in-game hour of warning on the status row.
pub(crate) const EMISSION_WARNING: u32 = 60;
const EMISSION_RADS: i32 = 400;

/// Minutes at which the next emission hits. Rolled at run start and after each one.
#[derive(Resource, Default)]
pub(crate) struct GameClock {
    pub next_emission: u32,
}

impl GameClock {
    pub fn schedule(&mut self, run: &RunState, rng: &mut Rng) {
        let days = EMISSION_MIN_DAYS + rng.roll(EMISSION_DAY_SPREAD) - 1;
        self.next_emission = run.minutes + days * MINUTES_PER_DAY;
    }

    pub fn warning(&self, run: &RunState) -> bool {
        self.next_emission.saturating_sub(run.minutes) <= EMISSION_WARNING
    }
}

// ---- radiation (GDD §4) ----

pub(crate) const RAD_DEATH: i32 = 1000;

/// Attribute penalty from accumulated rads: -1 at 200, -2 at 400, -3 at 600, -4 at 800.
pub(crate) fn rad_penalty(rads: i32) -> i32 {
    (rads / 200).clamp(0, 4)
}

/// HP lost per hour to radiation sickness: 1 from 600 rads, 2 from 800.
fn rad_hp_per_hour(rads: i32) -> i32 {
    match rads {
        r if r >= 800 => 2,
        r if r >= 600 => 1,
        _ => 0,
    }
}

// ---- artifacts (GDD §6) ----

/// Every artifact in the pack, as (attribute, bonus, rads per hour).
fn carried_artifacts<'a>(
    run: &'a RunState,
    zone: &'a ZoneData,
) -> impl Iterator<Item = (usize, i32, i32)> + 'a {
    run.items.iter().filter_map(move |(id, n)| {
        match zone.items.get(id).map(|i| i.kind) {
            Some(ItemKind::Artifact { attr, bonus, rads }) => {
                Some((attr, bonus * *n as i32, rads * *n as i32))
            }
            _ => None,
        }
    })
}

pub(crate) fn artifact_rads_per_hour(run: &RunState, zone: &ZoneData) -> i32 {
    carried_artifacts(run, zone).map(|(_, _, rads)| rads).sum()
}

/// Attribute after artifact bonuses and radiation penalty. Never drops below 1.
pub(crate) fn attr(run: &RunState, zone: &ZoneData, index: usize) -> i32 {
    let bonus: i32 = carried_artifacts(run, zone)
        .filter(|&(a, _, _)| a == index)
        .map(|(_, b, _)| b)
        .sum();
    (run.attrs[index] + bonus - rad_penalty(run.rads)).max(1)
}

fn carrying_light(run: &RunState, zone: &ZoneData) -> bool {
    run.items
        .iter()
        .any(|(id, _)| matches!(zone.items.get(id).map(|i| i.kind), Some(ItemKind::Light)))
}

// ---- passing time ----

/// Advances the clock and applies everything that happens per hour: artifact rads,
/// radiation sickness, and any emission that falls inside the span. Returns the
/// message the player should see, if the Zone did something worth saying.
pub(crate) fn advance(
    minutes: u32,
    area_id: &str,
    run: &mut RunState,
    zone: &ZoneData,
    clock: &mut GameClock,
    fields: &mut Fields,
    stock: &mut VendorStock,
    rng: &mut Rng,
) -> String {
    let start = run.minutes;
    run.minutes += minutes;

    let hours = (run.minutes / 60).saturating_sub(start / 60);
    if hours > 0 {
        let per_hour = artifact_rads_per_hour(run, zone);
        run.rads += per_hour * hours as i32;
        for _ in 0..hours {
            run.hp -= rad_hp_per_hour(run.rads);
        }
    }

    let mut message = String::new();
    if start < clock.next_emission && run.minutes >= clock.next_emission {
        message = emission(area_id, run, zone, clock, fields, stock, rng);
    }
    message
}

fn emission(
    area_id: &str,
    run: &mut RunState,
    zone: &ZoneData,
    clock: &mut GameClock,
    fields: &mut Fields,
    stock: &mut VendorStock,
    rng: &mut Rng,
) -> String {
    let sheltered = zone.areas[area_id].shelter;
    let message = if sheltered {
        "The sky burns. You wait it out under cover.".to_string()
    } else {
        run.rads += EMISSION_RADS;
        run.hp -= run.max_hp / 2;
        "The sky burns and the air cooks you. You should have found cover.".to_string()
    };

    // An emission restocks the vendors and reshuffles the fields (GDD §5).
    for (id, vendor) in &zone.vendors {
        stock.0.insert(id.clone(), vendor.stock.clone());
    }
    fields.0.clear();
    clock.schedule(run, rng);
    message
}

/// Sets `run.death` if this stalker is finished. Returns true once the run is over.
pub(crate) fn check_death(run: &mut RunState) -> bool {
    if run.death.is_some() {
        return true;
    }
    if run.rads >= RAD_DEATH {
        run.death = Some("Radiation. You glow, and then you stop.".into());
    } else if run.hp <= 0 {
        run.death = Some("Your wounds finish what the Zone started.".into());
    }
    run.death.is_some()
}

// ---- anomaly fields (GDD §6) ----

/// What this run knows about one field. Cleared by an emission.
#[derive(Default, Clone)]
pub(crate) struct FieldState {
    pub scanned: bool,
    /// An artifact is visible and can be taken.
    pub artifact: bool,
    pub taken: bool,
    /// A thrown bolt has already resolved the next step: `Some(true)` = safe.
    pub bolt_safe: Option<bool>,
}

#[derive(Resource, Default)]
pub(crate) struct Fields(pub HashMap<String, FieldState>);

impl Fields {
    pub fn get(&self, area_id: &str) -> FieldState {
        self.0.get(area_id).cloned().unwrap_or_default()
    }

    fn entry(&mut self, area_id: &str) -> &mut FieldState {
        self.0.entry(area_id.to_string()).or_default()
    }
}

/// The menu as it should render here and now. `Take Artifact` only exists once
/// something has been revealed (GDD §6).
pub(crate) fn visible_menu<'a>(
    area: &'a AreaData,
    fields: &Fields,
    area_id: &str,
) -> Vec<&'a (String, Action)> {
    let revealed = fields.get(area_id).artifact;
    area.menu
        .iter()
        .filter(|(_, action)| !matches!(action, Action::TakeArtifact) || revealed)
        .collect()
}

fn roll_damage(dice: (u32, u32), rng: &mut Rng) -> i32 {
    // ponytail: anomaly damage ignores armour — the Zone does not care what you wear.
    (0..dice.0).map(|_| rng.roll(dice.1) as i32).sum()
}

/// Stalker Lore, assisted by PER, harder at night without a light (GDD §5, §6).
fn scan_modifier(run: &RunState, zone: &ZoneData) -> i32 {
    // GDD §6: PER assists Stalker Lore at +2 per point above the 5 everyone starts with.
    let per = 2 * (attr(run, zone, PER) - 5);
    let night = if is_night(run.minutes) && !carrying_light(run, zone) {
        NIGHT_PENALTY
    } else {
        0
    };
    per + night
}

pub(crate) fn scan(
    area_id: &str,
    anomaly: &AnomalyData,
    run: &mut RunState,
    zone: &ZoneData,
    fields: &mut Fields,
    rng: &mut Rng,
) -> String {
    let skill = run.skills[crate::run::Skill::StalkerLore.index()];
    let outcome = check(skill, scan_modifier(run, zone), rng);
    let taken = fields.get(area_id).taken;
    let state = fields.entry(area_id);
    match outcome {
        Outcome::CritSuccess => {
            state.scanned = true;
            state.artifact = !taken;
            format!("You read the {} whole. Something glints in it.", anomaly.name)
        }
        Outcome::Success => {
            state.scanned = true;
            format!("The {} shows itself. You can see a way through.", anomaly.name)
        }
        Outcome::Fail => "You cannot make sense of the ground here.".into(),
        Outcome::CritFail => {
            let damage = roll_damage(anomaly.dice, rng);
            run.hp -= damage;
            format!("You step wrong reading it. {damage} damage.")
        }
    }
}

pub(crate) fn throw_bolt(
    area_id: &str,
    anomaly: &AnomalyData,
    run: &mut RunState,
    fields: &mut Fields,
    rng: &mut Rng,
) -> String {
    if !run.take_item("bolt", 1) {
        return "You are out of bolts.".into();
    }
    // No check — the bolt resolves the next step's gamble in advance (GDD §6).
    let safe = rng.roll(100) > anomaly.danger;
    fields.entry(area_id).bolt_safe = Some(safe);
    if safe {
        "The bolt arcs over and lands quiet. The way is clear.".into()
    } else {
        "The bolt vanishes with a crack. Not that way.".into()
    }
}

/// Returns the message and, on success, the area on the far side.
pub(crate) fn push_through(
    area_id: &str,
    anomaly: &AnomalyData,
    run: &mut RunState,
    fields: &mut Fields,
    rng: &mut Rng,
) -> (String, Option<String>) {
    let state = fields.get(area_id);
    let known_safe = state.scanned || state.bolt_safe == Some(true);
    fields.entry(area_id).bolt_safe = None;

    if known_safe {
        return (
            format!("You walk the line you found through the {}.", anomaly.name),
            Some(anomaly.beyond.clone()),
        );
    }
    if rng.roll(100) <= anomaly.danger {
        let damage = roll_damage(anomaly.dice, rng);
        run.hp -= damage;
        (
            format!("The {} catches you. {damage} damage.", anomaly.name),
            Some(anomaly.beyond.clone()),
        )
    } else {
        (
            format!("You cross the {} on luck alone.", anomaly.name),
            Some(anomaly.beyond.clone()),
        )
    }
}

pub(crate) fn take_artifact(
    area_id: &str,
    anomaly: &AnomalyData,
    run: &mut RunState,
    zone: &ZoneData,
    fields: &mut Fields,
) -> String {
    if fields.get(area_id).taken {
        return "There is nothing left in it now.".into();
    }
    let state = fields.entry(area_id);
    state.artifact = false;
    state.taken = true;
    run.add_item(&anomaly.artifact, 1);
    let name = &zone.items[&anomaly.artifact].name;
    format!("You lift the {name} clear. It is warm, and it is counting.")
}

// ---- gated hidden letters (GDD §5) ----

/// Works out which of an area's letters show themselves. `Check` gates roll once
/// per run, the first time you stand here; flag and rep gates are re-read on entry.
pub(crate) fn reveal_secrets(area_id: &str, run: &mut RunState, zone: &ZoneData, rng: &mut Rng) {
    let Some(area) = zone.areas.get(area_id) else {
        return;
    };
    for (&letter, secret) in &area.secrets {
        let key = RunState::secret_key(area_id, letter);
        let show = match &secret.gate {
            None => true,
            Some(Gate::Flag(flag)) => run.flags.contains(flag),
            Some(Gate::Rep(faction, at_least)) => run.rep_of(faction) >= *at_least,
            Some(Gate::Check(skill, modifier)) => {
                if !run.gate_rolled.insert(key.clone()) {
                    continue; // already rolled this run, one way or the other
                }
                matches!(
                    check(run.skills[skill.index()], *modifier, rng),
                    Outcome::Success | Outcome::CritSuccess
                )
            }
        };
        if show {
            run.revealed.insert(key);
        }
    }
}

// ---- tests (SPEC §7) ----

#[cfg(test)]
mod tests {
    use super::*;
    use crate::area::load_zone;
    use std::path::Path;

    fn fixture() -> (ZoneData, RunState, Rng, Fields, GameClock, VendorStock) {
        let zone = load_zone(Path::new("assets/data"));
        let stock = VendorStock(
            zone.vendors
                .iter()
                .map(|(id, v)| (id.clone(), v.stock.clone()))
                .collect(),
        );
        let run = RunState::roll(0, &[8, 9, 4]);
        (zone, run, Rng::new(7), Fields::default(), GameClock::default(), stock)
    }

    #[test]
    fn radiation_thresholds_match_the_table() {
        // GDD §4: -1 at 200, -2 at 400, -3 at 600, -4 at 800, dead at 1000.
        assert_eq!(rad_penalty(199), 0);
        assert_eq!(rad_penalty(200), 1);
        assert_eq!(rad_penalty(799), 3);
        assert_eq!(rad_penalty(999), 4);
        assert_eq!(rad_hp_per_hour(599), 0);
        assert_eq!(rad_hp_per_hour(600), 1);
        assert_eq!(rad_hp_per_hour(800), 2);

        let mut run = RunState::roll(0, &[0, 1, 2]);
        run.rads = RAD_DEATH;
        assert!(check_death(&mut run));
    }

    #[test]
    fn night_is_the_hours_the_gdd_says() {
        assert!(!is_night(12 * 60));
        assert!(is_night(20 * 60));
        assert!(is_night(3 * 60));
        assert!(!is_night(5 * 60));
    }

    #[test]
    fn carried_artifacts_cost_rads_by_the_hour_and_pay_an_attribute() {
        let (zone, mut run, mut rng, mut fields, mut clock, mut stock) = fixture();
        let base_str = attr(&run, &zone, crate::run::STR);
        run.add_item("gravi", 1);
        assert_eq!(attr(&run, &zone, crate::run::STR), base_str + 1);

        let per_hour = artifact_rads_per_hour(&run, &zone);
        assert!(per_hour > 0);
        clock.next_emission = u32::MAX;
        advance(120, "camp", &mut run, &zone, &mut clock, &mut fields, &mut stock, &mut rng);
        assert_eq!(run.rads, per_hour * 2);
    }

    #[test]
    fn an_emission_spares_shelter_and_resets_the_zone() {
        let (zone, mut run, mut rng, mut fields, mut clock, mut stock) = fixture();
        fields.entry("field").scanned = true;
        stock.0.get_mut("trader").unwrap().clear();

        clock.next_emission = run.minutes + 30;
        let msg = advance(60, "camp", &mut run, &zone, &mut clock, &mut fields, &mut stock, &mut rng);
        assert!(msg.contains("wait it out"), "{msg}");
        assert_eq!(run.rads, 0);
        assert_eq!(run.hp, run.max_hp);
        assert!(!fields.get("field").scanned, "an emission reshuffles the fields");
        assert!(!stock.0["trader"].is_empty(), "an emission restocks the vendors");
        assert!(clock.next_emission > run.minutes, "the next one is scheduled");

        // Caught in the open, it costs 400 rads and half your health.
        clock.next_emission = run.minutes + 30;
        advance(60, "road", &mut run, &zone, &mut clock, &mut fields, &mut stock, &mut rng);
        assert_eq!(run.rads, EMISSION_RADS);
        assert_eq!(run.hp, run.max_hp - run.max_hp / 2);
    }

    #[test]
    fn a_bolt_costs_a_bolt_and_a_scan_opens_the_way() {
        let (zone, mut run, mut rng, mut fields, _, _) = fixture();
        let anomaly = zone.areas["field"].anomaly.clone().unwrap();

        let bolts = run.items.iter().find(|(i, _)| i == "bolt").unwrap().1;
        throw_bolt("field", &anomaly, &mut run, &mut fields, &mut rng);
        assert_eq!(run.items.iter().find(|(i, _)| i == "bolt").unwrap().1, bolts - 1);
        assert!(fields.get("field").bolt_safe.is_some());

        run.take_item("bolt", bolts - 1);
        assert!(throw_bolt("field", &anomaly, &mut run, &mut fields, &mut rng).contains("out of bolts"));

        // A scanned field is crossed without a roll, and the bolt reading is spent.
        fields.entry("field").scanned = true;
        let hp = run.hp;
        let (msg, dest) = push_through("field", &anomaly, &mut run, &mut fields, &mut rng);
        assert_eq!(dest.as_deref(), Some(anomaly.beyond.as_str()));
        assert_eq!(run.hp, hp, "{msg}");
        assert_eq!(fields.get("field").bolt_safe, None);
    }

    #[test]
    fn an_artifact_can_only_be_taken_once() {
        let (zone, mut run, _, mut fields, _, _) = fixture();
        let anomaly = zone.areas["field"].anomaly.clone().unwrap();
        fields.entry("field").artifact = true;
        take_artifact("field", &anomaly, &mut run, &zone, &mut fields);
        assert_eq!(run.items.iter().find(|(i, _)| *i == anomaly.artifact).unwrap().1, 1);
        assert!(fields.get("field").taken);
        assert!(!fields.get("field").artifact, "the menu entry goes away with it");

        // A second attempt finds an empty field, and the menu no longer offers it.
        assert!(take_artifact("field", &anomaly, &mut run, &zone, &mut fields).contains("nothing left"));
        assert_eq!(run.items.iter().find(|(i, _)| *i == anomaly.artifact).unwrap().1, 1);
        let menu = visible_menu(&zone.areas["field"], &fields, "field");
        assert!(!menu.iter().any(|(_, a)| matches!(a, Action::TakeArtifact)));
    }

    #[test]
    fn ungated_letters_show_and_flag_gates_wait_for_the_flag() {
        let (zone, mut run, mut rng, _, _, _) = fixture();
        reveal_secrets("camp", &mut run, &zone, &mut rng);
        assert!(run.is_revealed("camp", 'D'), "the camp hatch is ungated");

        reveal_secrets("quarry", &mut run, &zone, &mut rng);
        assert!(!run.is_revealed("quarry", 'S'), "no flag, no letter");

        run.flags.insert("read_the_log".into());
        reveal_secrets("quarry", &mut run, &zone, &mut rng);
        assert!(run.is_revealed("quarry", 'S'));
    }

    #[test]
    fn a_failed_check_gate_is_not_rolled_twice() {
        let (zone, mut run, mut rng, _, _, _) = fixture();
        run.skills[crate::run::Skill::StalkerLore.index()] = 0; // cannot pass, barring a crit
        for _ in 0..20 {
            reveal_secrets("field", &mut run, &zone, &mut rng);
        }
        assert_eq!(run.gate_rolled.len(), 1, "the gate is rolled once per run");
    }
}
