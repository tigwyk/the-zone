//! A bench for the combat maths. It drives the real `combat::act` loop, so what it
//! measures is what ships — no second model of the rules to drift out of step.
//!
//! Test-only. Run the reports with:
//!   cargo test --release -- --ignored --nocapture balance
//!
//! Everything here is deterministic from a seed, so a number in a report can be
//! reproduced exactly.

use crate::area::{load_zone, ZoneData};
use crate::combat::{self, Combat, Verb};
use crate::loot::{ItemStack, Roll};
use crate::run::{Rng, RunState, Skill};
use std::path::Path;

/// How the stalker plays it. Comparing these two is how you find out whether the
/// extra 2 AP for an aimed shot is worth it against a given target.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Policy {
    /// Swing as often as the AP allows.
    Fast,
    /// Aim whenever it is affordable, otherwise swing.
    Aimed,
    /// Swing, but break off once it is clearly going badly. This is what a player
    /// actually does, and without it a bench overstates how lethal a thing is.
    Cautious,
}

/// Where a cautious stalker decides it is not their day.
const BREAK_OFF_AT: f32 = 0.35;

/// One stalker, built a particular way. Only what changes a fight is here.
#[derive(Clone)]
pub(crate) struct Loadout {
    pub name: String,
    pub weapon: Option<(String, Vec<Roll>)>,
    pub armor: Option<(String, Vec<Roll>)>,
    pub skill: i32,
    pub policy: Policy,
    /// Attribute overrides, by index. Left alone, everything is the starting 5.
    pub attrs: Vec<(usize, i32)>,
}

impl Loadout {
    pub fn new(name: &str) -> Self {
        Loadout {
            name: name.to_string(),
            weapon: None,
            armor: None,
            skill: 60,
            policy: Policy::Fast,
            attrs: Vec::new(),
        }
    }

    /// A weapon with a specific set of rolls on it, as `(affix id, magnitude)`.
    pub fn weapon(mut self, id: &str, affixes: &[(&str, i32)]) -> Self {
        self.weapon = Some((id.to_string(), rolls(affixes)));
        self
    }

    pub fn armor(mut self, id: &str, affixes: &[(&str, i32)]) -> Self {
        self.armor = Some((id.to_string(), rolls(affixes)));
        self
    }

    pub fn skill(mut self, skill: i32) -> Self {
        self.skill = skill;
        self
    }

    pub fn policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    pub fn attr(mut self, index: usize, value: i32) -> Self {
        self.attrs.push((index, value));
        self
    }

    /// A fresh stalker for one trial. Fresh every time, because `check_skill` lets a
    /// skill climb as it is used and that would drift a long run of trials.
    fn build(&self, rng: &mut Rng) -> RunState {
        let mut run = RunState::roll(0, &[0, 2, 3], rng);
        for &(index, value) in &self.attrs {
            run.attrs[index] = value;
        }
        run.max_hp = 20 + 3 * run.attrs[crate::run::END];
        run.hp = run.max_hp;
        for skill in [Skill::SmallGuns, Skill::Melee, Skill::EnergyWeapons] {
            run.skills[skill.index()] = self.skill;
        }
        if let Some((id, affixes)) = &self.weapon {
            let uid = run.next_uid();
            run.add_stack(ItemStack { uid, id: id.clone(), count: 1, affixes: affixes.clone() });
            run.weapon = Some(uid);
        }
        if let Some((id, affixes)) = &self.armor {
            let uid = run.next_uid();
            run.add_stack(ItemStack { uid, id: id.clone(), count: 1, affixes: affixes.clone() });
            run.armor = Some(uid);
        }
        run
    }
}

fn rolls(affixes: &[(&str, i32)]) -> Vec<Roll> {
    affixes
        .iter()
        .map(|&(affix, magnitude)| Roll { affix: affix.to_string(), magnitude })
        .collect()
}

/// How one fight ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum End {
    Killed,
    Died,
    /// The enemy broke off, or the fight ran past the cap.
    Escaped,
}

struct Trial {
    end: End,
    rounds: u32,
    hp_left: i32,
    max_hp: i32,
}

/// No fight should take this long; if one does, something is wrong with the policy
/// or with the numbers, and that is worth knowing rather than hanging on.
const ROUND_CAP: u32 = 60;

fn one_fight(
    loadout: &Loadout,
    enemy_id: &str,
    zone: &ZoneData,
    rng: &mut Rng,
) -> Trial {
    let mut run = loadout.build(rng);
    let max_hp = run.max_hp;
    let mut combat = Combat::default();
    combat::start(enemy_id, &mut combat, &mut run, zone, rng);

    let mut rounds = 0;
    while combat.active && run.hp > 0 && rounds < ROUND_CAP {
        let menu = combat::menu(&combat, &run, zone);
        let hurt = run.hp as f32 / max_hp as f32;
        let verb = choose(&menu, loadout.policy, hurt);
        let before = combat.ap;
        combat::act(verb, &mut combat, &mut run, zone, rng);
        // Every verb costs AP, so the pool can only fail to go down if `end_turn`
        // handed back a fresh one. Testing for *greater* misses the case where the
        // reset lands on exactly what it started at - an aimed shot at full AP does
        // that every time, and it read as zero rounds a fight.
        if combat.ap >= before {
            rounds += 1;
        }
    }

    let end = if run.hp <= 0 {
        End::Died
    } else if combat.hp <= 0 {
        End::Killed
    } else {
        End::Escaped // one of you broke off, or the cap ran out
    };
    Trial { end, rounds, hp_left: run.hp.max(0), max_hp }
}

/// Shoot if the range and the AP allow, otherwise close, otherwise give ground.
/// Only `Cautious` runs; the others stay in so a weapon can be measured to the end.
fn choose(menu: &[(String, Verb)], policy: Policy, hurt: f32) -> Verb {
    let has = |v: Verb| menu.iter().any(|(_, m)| *m == v);
    if policy == Policy::Cautious && hurt < BREAK_OFF_AT && has(Verb::Flee) {
        return Verb::Flee;
    }
    if policy == Policy::Aimed && has(Verb::Aimed) {
        return Verb::Aimed;
    }
    for verb in [Verb::Attack, Verb::Aimed, Verb::CloseIn, Verb::FallBack] {
        if has(verb) {
            return verb;
        }
    }
    Verb::Flee
}

#[derive(Default, Clone, Copy)]
pub(crate) struct Summary {
    pub trials: usize,
    pub killed: usize,
    pub died: usize,
    pub escaped: usize,
    pub rounds: f32,
    /// Health left, as a fraction of the maximum, over the fights that were won.
    pub hp_left: f32,
}

impl Summary {
    pub fn win_rate(&self) -> f32 {
        self.killed as f32 / self.trials as f32
    }
}

pub(crate) fn simulate(
    loadout: &Loadout,
    enemy_id: &str,
    trials: usize,
    seed: u64,
    zone: &ZoneData,
) -> Summary {
    let mut rng = Rng::new(seed);
    let mut out = Summary { trials, ..Default::default() };
    let mut rounds = 0u64;
    let mut hp_fraction = 0.0f32;
    for _ in 0..trials {
        let trial = one_fight(loadout, enemy_id, zone, &mut rng);
        rounds += trial.rounds as u64;
        match trial.end {
            End::Killed => {
                out.killed += 1;
                hp_fraction += trial.hp_left as f32 / trial.max_hp as f32;
            }
            End::Died => out.died += 1,
            End::Escaped => out.escaped += 1,
        }
    }
    out.rounds = rounds as f32 / trials as f32;
    out.hp_left = if out.killed > 0 { hp_fraction / out.killed as f32 } else { 0.0 };
    out
}

/// Prints one comparison table: every loadout against one enemy.
pub(crate) fn report(title: &str, enemy_id: &str, loadouts: &[Loadout], trials: usize) {
    let zone = load_zone(Path::new("assets/data"));
    println!("\n{title}  ({trials} fights each, vs {})", zone.enemies[enemy_id].name);
    println!(
        "  {:<34}{:>6}{:>7}{:>7}{:>8}{:>8}",
        "loadout", "win%", "died%", "away%", "rounds", "hp left"
    );
    for (i, loadout) in loadouts.iter().enumerate() {
        // A seed per row, fixed, so a row can be reproduced on its own.
        let s = simulate(loadout, enemy_id, trials, 1000 + i as u64, &zone);
        println!(
            "  {:<34}{:>5.0}%{:>6.0}%{:>6.0}%{:>8.1}{:>7.0}%",
            loadout.name,
            s.win_rate() * 100.0,
            s.died as f32 / trials as f32 * 100.0,
            s.escaped as f32 / trials as f32 * 100.0,
            s.rounds,
            s.hp_left * 100.0,
        );
        let _ = &s;
    }
}

// ---- the reports ----

#[cfg(test)]
mod reports {
    use super::*;

    const TRIALS: usize = 3000;

    /// What one affix is actually worth, held against everything else being equal.
    #[test]
    #[ignore = "balance bench: cargo test --release -- --ignored --nocapture balance"]
    fn affixes_on_one_weapon() {
        let base = || Loadout::new("x").weapon("pistol", &[]).skill(55);
        let loadouts = vec![
            Loadout { name: "pistol, plain".into(), ..base() },
            Loadout {
                name: "+3 damage (Heavy)".into(),
                ..base().weapon("pistol", &[("heavy", 3)])
            },
            Loadout {
                name: "+12 to hit (Keen)".into(),
                ..base().weapon("pistol", &[("keen", 12)])
            },
            Loadout {
                name: "+2 crit range (Hairline)".into(),
                ..base().weapon("pistol", &[("hairline", 2)])
            },
            Loadout {
                name: "marked: +3 dmg, +12 hit".into(),
                ..base().weapon("pistol", &[("heavy", 3), ("keen", 12)])
            },
            Loadout {
                name: "relic-ish: dmg, hit, crit".into(),
                ..base().weapon(
                    "pistol",
                    &[("heavy", 4), ("scorched", 6), ("keen", 12), ("hairline", 3)],
                )
            },
        ];
        report("Affixes on a pistol", "flesh", &loadouts, TRIALS);
        report("Affixes on a pistol", "bloodsucker", &loadouts, TRIALS);
    }

    /// Is the aimed shot worth its two extra points, and when.
    #[test]
    #[ignore = "balance bench"]
    fn aiming_against_swinging() {
        let mut loadouts = Vec::new();
        for skill in [40, 60, 85] {
            for (label, policy) in [("fast", Policy::Fast), ("aimed", Policy::Aimed)] {
                loadouts.push(Loadout {
                    name: format!("skill {skill}, {label}"),
                    ..Loadout::new("x").weapon("rifle", &[]).skill(skill).policy(policy)
                });
            }
        }
        // AP is 5 + AGI/2 (GDD §4), so a quick stalker gets whole extra actions.
        for agi in [5, 10] {
            loadouts.push(Loadout {
                name: format!("skill 60, fast, AGI {agi}"),
                ..Loadout::new("x")
                    .weapon("rifle", &[])
                    .skill(60)
                    .attr(crate::run::AGI, agi)
            });
        }
        report("Aim or swing", "flesh", &loadouts, TRIALS);
        report("Aim or swing", "pseudogiant", &loadouts, TRIALS);
    }

    /// What the roster actually feels like to a stalker who is kitted for it.
    #[test]
    #[ignore = "balance bench"]
    fn the_roster_against_a_fair_loadout() {
        let zone = load_zone(Path::new("assets/data"));
        let starting = Loadout::new("starting kit: knife, no armour")
            .weapon("knife", &[])
            .skill(40);
        let armed = Loadout::new("found the hatch: sawn-off")
            .weapon("sawn_off", &[])
            .skill(40);
        let kitted = Loadout::new("kitted: rifle, vest")
            .weapon("rifle", &[])
            .armor("vest", &[])
            .skill(70);
        let deep = Loadout::new("deep: marked sniper, sealed suit")
            .weapon("sniper", &[("heavy", 3), ("keen", 12)])
            .armor("seva_suit", &[("plated", 3)])
            .skill(90);

        let cautious = Loadout::new("kitted, breaks off when hurt")
            .weapon("rifle", &[])
            .armor("vest", &[])
            .skill(70)
            .policy(Policy::Cautious);

        let mut ids: Vec<&String> = zone.enemies.keys().collect();
        ids.sort_unstable();
        for id in ids {
            report(
                &format!("Roster check: {}", zone.enemies[id].name),
                id,
                &[
                    starting.clone(),
                    armed.clone(),
                    kitted.clone(),
                    cautious.clone(),
                    deep.clone(),
                ],
                TRIALS,
            );
        }
    }

    /// The guard that stays on: an affix has to be worth carrying, and a fight has
    /// to be winnable and losable. Cheap enough to run every time.
    #[test]
    fn a_better_weapon_wins_more_often() {
        let zone = load_zone(Path::new("assets/data"));
        let trials = 600;
        let plain = Loadout::new("plain").weapon("pistol", &[]).skill(50);
        let marked = Loadout::new("marked")
            .weapon("pistol", &[("heavy", 3), ("keen", 12)])
            .skill(50);

        let a = simulate(&plain, "flesh", trials, 7, &zone);
        let b = simulate(&marked, "flesh", trials, 7, &zone);
        assert!(
            b.win_rate() > a.win_rate(),
            "rolls should pay: plain {:.2} vs marked {:.2}",
            a.win_rate(),
            b.win_rate()
        );
        assert!(b.rounds < a.rounds, "and should end it sooner");

        // A fight has to be able to go either way, or none of this means anything.
        assert!(a.killed > 0 && a.died > 0, "the plain pistol fight is not a coin toss: {a:?}",);
    }

    /// Breaking off has to be a real way out, not a formality. Half of GDD §8's
    /// flee rule was missing and the bench found it: a kitted stalker who decided
    /// to run from a pseudogiant still died three times in five.
    #[test]
    fn running_away_is_a_real_option() {
        let zone = load_zone(Path::new("assets/data"));
        let trials = 600;
        let kitted = Loadout::new("kitted")
            .weapon("rifle", &[])
            .armor("vest", &[])
            .skill(70);
        let cautious = Loadout { policy: Policy::Cautious, ..kitted.clone() };

        for enemy in ["pseudogiant", "controller", "bloodsucker"] {
            let stay = simulate(&kitted, enemy, trials, 11, &zone);
            let run_for_it = simulate(&cautious, enemy, trials, 11, &zone);
            assert!(
                run_for_it.died < stay.died,
                "{enemy}: breaking off did not help ({} vs {} deaths)",
                run_for_it.died,
                stay.died
            );
            assert!(
                run_for_it.escaped * 2 > run_for_it.died,
                "{enemy}: more stalkers die trying to run than get away: {run_for_it:?}"
            );
        }
    }

    /// The first thing a new stalker is likely to meet. A knife is meant to be a bad
    /// answer to it; the weapon behind the camp's hidden letter is meant to be a
    /// workable one. If either stops being true the early game has drifted.
    #[test]
    fn the_first_fight_is_hard_with_a_knife_and_fair_with_the_hatch_gun() {
        let zone = load_zone(Path::new("assets/data"));
        let trials = 600;
        let knife = Loadout::new("knife").weapon("knife", &[]).skill(40);
        let found = Loadout::new("sawn-off").weapon("sawn_off", &[]).skill(40);

        let a = simulate(&knife, "flesh", trials, 3, &zone);
        let b = simulate(&found, "flesh", trials, 3, &zone);
        assert!(a.win_rate() < 0.5, "a knife should lose to a Flesh: {a:?}");
        assert!(b.win_rate() > 0.5, "the hatch gun should win it: {b:?}");
    }
}

impl std::fmt::Debug for Summary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} trials: {} killed, {} died, {} away, {:.1} rounds",
            self.trials, self.killed, self.died, self.escaped, self.rounds
        )
    }
}
