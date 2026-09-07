//! A bench for the combat maths. It drives the real `combat::act` loop, so what it
//! measures is what ships — no second model of the rules to drift out of step.
//!
//! Test-only. Run the reports with:
//!   cargo test --release -- --ignored --nocapture balance
//!
//! Everything here is deterministic from a seed, so a number in a report can be
//! reproduced exactly.

use crate::area::{load_zone, Caliber, ItemKind, ZoneData};
use crate::combat::{self, Combat, Verb};
use crate::loot::{self, Effect, ItemStack, Roll};
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
    /// Never feed the gun. The control for what a reload is worth (GUNS §7).
    Dry,
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
    /// Mods fitted to the weapon, by item id.
    pub mods: Vec<String>,
    /// The rounds carried, and how many. Left alone, a gun arrives with a full
    /// magazine and a box of the caliber’s ball ammunition, because a player who
    /// buys a gun buys ammunition with it (GUNS §7.0). A box rather than a couple of
    /// magazines, because two spares of a sawn-off is six shells and the bench
    /// measured that as the sawn-off losing to a Flesh it is meant to beat.
    pub ammo: Option<(String, u32)>,
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
            mods: Vec::new(),
            ammo: None,
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

    /// Mods fitted to the weapon, by item id, in order.
    pub fn mods(mut self, ids: &[&str]) -> Self {
        self.mods = ids.iter().map(|id| id.to_string()).collect();
        self
    }

    /// A particular round, and how many are carried. `.ammo(id, 0)` is the dry gun.
    pub fn ammo(mut self, id: &str, rounds: u32) -> Self {
        self.ammo = Some((id.to_string(), rounds));
        self
    }

    /// A fresh stalker for one trial. Fresh every time, because `check_skill` lets a
    /// skill climb as it is used and that would drift a long run of trials.
    fn build(&self, zone: &ZoneData, rng: &mut Rng) -> RunState {
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
            run.add_stack(ItemStack {
                affixes: affixes.clone(),
                mods: self.mods.clone(),
                ..ItemStack::plain(uid, id, 1)
            });
            let plain = affixes.is_empty() && self.mods.is_empty();
            run.weapon = Some(equipped(&run, id, plain, uid));
            self.load(&mut run, zone);
        }
        if let Some((id, affixes)) = &self.armor {
            let uid = run.next_uid();
            run.add_stack(ItemStack {
                affixes: affixes.clone(),
                ..ItemStack::plain(uid, id, 1)
            });
            run.armor = Some(equipped(&run, id, affixes.is_empty(), uid));
        }
        run
    }

    /// Puts rounds in the pack and fills the magazine from it, through the same
    /// `combat::reload` the player presses - never by writing the magazine by hand.
    fn load(&self, run: &mut RunState, zone: &ZoneData) {
        let uid = run.weapon.expect("a weapon was just equipped");
        let stack = run.stack(uid).expect("just added");
        let ItemKind::Weapon { ammo: Some(caliber), mag, .. } = zone.items[&stack.id].kind else {
            return;
        };
        let capacity = (mag as i32 + loot::bonus(stack, zone, Effect::Mag)).max(1) as u32;
        let (id, rounds) = match &self.ammo {
            Some((id, rounds)) => (id.clone(), *rounds),
            // A magazine, and a box of the ordinary stuff behind it.
            None => (ball(caliber).to_string(), capacity + BOX),
        };
        if rounds == 0 {
            return;
        }
        run.add_item(&id, rounds);
        run.stack_mut(uid).expect("just added").loaded_with = Some(id);
        combat::reload(run, zone);
    }
}

/// What a stalker carries spare: three lots off the shelf (GUNS §2).
const BOX: u32 = 30;

/// The ordinary round for a caliber - what a stalker buys without thinking about it.
fn ball(caliber: Caliber) -> &'static str {
    match caliber {
        Caliber::Pistol => "pistol_round",
        Caliber::Rifle => "rifle_round",
        Caliber::Shell => "shell_buck",
    }
}

/// What the ammunition on this stalker is worth, magazine included. Read before and
/// after a fight, the difference is what the fight cost in rubles (GUNS §7.1).
fn ammo_rubles(run: &RunState, zone: &ZoneData) -> u32 {
    let price = |id: &String, n: u32| match zone.items.get(id) {
        Some(item) if matches!(item.kind, ItemKind::Ammo { .. }) => item.base * n,
        _ => 0,
    };
    let carried: u32 = run.items.iter().map(|s| price(&s.id, s.count)).sum();
    let chambered: u32 = run
        .items
        .iter()
        .filter_map(|s| s.loaded_with.as_ref().map(|id| price(id, s.loaded)))
        .sum();
    carried + chambered
}

/// After `add_stack`, the stack actually in hand: a plain one has merged into the
/// kit's own copy (the kit holds a knife) and keeps the kit's uid; an affixed one
/// is pushed and keeps the uid it was just given.
fn equipped(run: &RunState, id: &str, plain: bool, uid: u32) -> u32 {
    if plain {
        run.items.iter().find(|s| s.id == id).map(|s| s.uid).expect("just added")
    } else {
        uid
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
    /// Shots fired, magazines fed, and what the ammunition cost in rubles.
    shots: u32,
    reloads: u32,
    spent: u32,
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
    let mut run = loadout.build(zone, rng);
    let max_hp = run.max_hp;
    let carried = ammo_rubles(&run, zone);
    let mut combat = Combat::default();
    combat::start(enemy_id, &mut combat, &mut run, zone, rng);

    let (mut shots, mut reloads) = (0, 0);
    let mut rounds = 0;
    while combat.active && run.hp > 0 && rounds < ROUND_CAP {
        let menu = combat::menu(&combat, &run, zone);
        let hurt = run.hp as f32 / max_hp as f32;
        let verb = choose(&menu, loadout.policy, hurt);
        match verb {
            Verb::Attack | Verb::Aimed => shots += 1,
            Verb::Reload => reloads += 1,
            Verb::Flee => {}
        }
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
    Trial {
        end,
        rounds,
        hp_left: run.hp.max(0),
        max_hp,
        shots,
        reloads,
        spent: carried.saturating_sub(ammo_rubles(&run, zone)),
    }
}

/// Aim when the policy asks and the AP allows, otherwise swing, feed it when it is
/// empty, otherwise run. Only `Cautious` runs early; the others stay in so a weapon
/// can be measured to the end, and `Dry` never reloads on purpose (GUNS §7.1).
fn choose(menu: &[(String, Verb)], policy: Policy, hurt: f32) -> Verb {
    let has = |v: Verb| menu.iter().any(|(_, m)| *m == v);
    if policy == Policy::Cautious && hurt < BREAK_OFF_AT && has(Verb::Flee) {
        return Verb::Flee;
    }
    if policy == Policy::Aimed && has(Verb::Aimed) {
        return Verb::Aimed;
    }
    if has(Verb::Attack) {
        return Verb::Attack;
    }
    // Nothing to shoot with: feed it, unless the point of the trial is not to.
    if policy != Policy::Dry && has(Verb::Reload) {
        return Verb::Reload;
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
    /// Shots, reloads and rubles of ammunition, per fight.
    pub shots: f32,
    pub reloads: f32,
    pub spent: f32,
}

impl Summary {
    pub fn win_rate(&self) -> f32 {
        self.killed as f32 / self.trials as f32
    }

    /// What the ammunition cost per kill. The only way to see whether the sink is a
    /// decision or a tax (GUNS §7.2). Nothing killed is nothing earned, not free.
    pub fn ru_per_kill(&self) -> f32 {
        if self.killed == 0 {
            return f32::INFINITY;
        }
        self.spent * self.trials as f32 / self.killed as f32
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
    let (mut shots, mut reloads, mut spent) = (0u64, 0u64, 0u64);
    for _ in 0..trials {
        let trial = one_fight(loadout, enemy_id, zone, &mut rng);
        rounds += trial.rounds as u64;
        shots += trial.shots as u64;
        reloads += trial.reloads as u64;
        spent += trial.spent as u64;
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
    out.shots = shots as f32 / trials as f32;
    out.reloads = reloads as f32 / trials as f32;
    out.spent = spent as f32 / trials as f32;
    out
}

/// Prints one comparison table: every loadout against one enemy.
pub(crate) fn report(title: &str, enemy_id: &str, loadouts: &[Loadout], trials: usize) {
    let zone = load_zone(Path::new("assets/data"));
    println!("\n{title}  ({trials} fights each, vs {})", zone.enemies[enemy_id].name);
    println!(
        "  {:<34}{:>6}{:>7}{:>7}{:>8}{:>8}{:>7}{:>9}",
        "loadout", "win%", "died%", "away%", "rounds", "hp left", "shots", "RU/kill"
    );
    for (i, loadout) in loadouts.iter().enumerate() {
        // A seed per row, fixed, so a row can be reproduced on its own.
        let s = simulate(loadout, enemy_id, trials, 1000 + i as u64, &zone);
        let cost = s.ru_per_kill();
        println!(
            "  {:<34}{:>5.0}%{:>6.0}%{:>6.0}%{:>8.1}{:>7.0}%{:>7.1}{:>9}",
            loadout.name,
            s.win_rate() * 100.0,
            s.died as f32 / trials as f32 * 100.0,
            s.escaped as f32 / trials as f32 * 100.0,
            s.rounds,
            s.hp_left * 100.0,
            s.shots,
            if cost.is_finite() { format!("{cost:.0}") } else { "-".into() },
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

    /// What each round is worth, where armour is and where it is not (GUNS §7.2).
    #[test]
    #[ignore = "balance bench: cargo test --release -- --ignored --nocapture balance"]
    fn ammunition_types() {
        let rounds = |gun: &str, ids: &[&str]| -> Vec<Loadout> {
            ids.iter()
                .map(|id| Loadout {
                    name: format!("{gun} + {id}"),
                    ..Loadout::new("x").weapon(gun, &[]).ammo(id, 60).skill(60)
                })
                .collect()
        };
        let pistol = rounds("pistol", &["pistol_surplus", "pistol_round", "pistol_ap"]);
        let rifle = rounds("rifle", &["rifle_surplus", "rifle_round", "rifle_ap"]);
        // No armour at all, then the wall.
        for enemy in ["blind_dog", "flesh", "pseudogiant"] {
            report("Pistol rounds", enemy, &pistol, TRIALS);
            report("Rifle rounds", enemy, &rifle, TRIALS);
        }
    }

    /// What one mod is worth, held against everything else being equal - the same
    /// shape as `affixes_on_one_weapon`, so the two lists can be read together.
    #[test]
    #[ignore = "balance bench"]
    fn mods_on_one_weapon() {
        let base = || Loadout::new("x").weapon("pistol", &[]).skill(55);
        let mut loadouts = vec![Loadout { name: "pistol, bare".into(), ..base() }];
        for id in ["grip", "laser", "scope", "bayonet", "barrel", "trigger", "mag_well", "muzzle"] {
            loadouts.push(Loadout { name: format!("+ {id}"), ..base().mods(&[id]) });
        }
        loadouts.push(Loadout {
            name: "scope + barrel + trigger".into(),
            ..base().mods(&["scope", "barrel", "trigger"])
        });
        report("Mods on a pistol", "flesh", &loadouts, TRIALS);
        report("Mods on a pistol", "bloodsucker", &loadouts, TRIALS);
    }

    /// What a magazine costs and what feeding it is worth (GUNS §7.2).
    #[test]
    #[ignore = "balance bench"]
    fn the_magazine() {
        let mut loadouts = Vec::new();
        for gun in ["sawn_off", "pistol", "shotgun", "rifle"] {
            for (label, policy, skill) in [
                ("feeds it, skill 40", Policy::Fast, 40),
                ("feeds it, skill 60 (1 AP)", Policy::Fast, 60),
                ("never reloads", Policy::Dry, 40),
            ] {
                loadouts.push(Loadout {
                    name: format!("{gun}, {label}"),
                    ..Loadout::new("x").weapon(gun, &[]).skill(skill).policy(policy)
                });
            }
        }
        report("The magazine", "flesh", &loadouts, TRIALS);
        report("The magazine", "bandit", &loadouts, TRIALS);
    }

    /// The gate, not a report (GUNS §7.2). GDD §8 records that taking an attack from
    /// 4 AP to 3 changed what the game is; the brake is that lever pulled again, and
    /// it does not ship until this table says the skill curve survives it.
    #[test]
    #[ignore = "balance bench"]
    fn the_muzzle_brake() {
        let mut loadouts = Vec::new();
        for skill in [40, 60, 85] {
            for (label, mods, policy) in [
                ("3 AP, swings", vec![], Policy::Fast),
                ("2 AP, swings", vec!["muzzle"], Policy::Fast),
                ("3 AP, aims", vec![], Policy::Aimed),
                ("2 AP, aims", vec!["muzzle"], Policy::Aimed),
            ] {
                loadouts.push(Loadout {
                    name: format!("skill {skill}, {label}"),
                    ..Loadout::new("x").weapon("rifle", &[]).mods(&mods).skill(skill).policy(policy)
                });
            }
        }
        for enemy in ["flesh", "bloodsucker", "pseudogiant"] {
            report("The muzzle brake", enemy, &loadouts, TRIALS);
        }
    }

    /// A mod has to be worth its price, the same way an affix does (GUNS §7.3).
    #[test]
    fn mods_pay_for_themselves() {
        let zone = load_zone(Path::new("assets/data"));
        let trials = 600;
        let bare = Loadout::new("bare").weapon("pistol", &[]).skill(50);
        let fitted = Loadout::new("fitted").weapon("pistol", &[]).mods(&["scope", "barrel"]).skill(50);

        let a = simulate(&bare, "flesh", trials, 7, &zone);
        let b = simulate(&fitted, "flesh", trials, 7, &zone);
        assert!(
            b.win_rate() > a.win_rate(),
            "fitting should pay: bare {:.2} vs fitted {:.2}",
            a.win_rate(),
            b.win_rate()
        );
        assert!(b.rounds < a.rounds, "and should end it sooner");
    }

    /// Pierce has to earn its price where armour is, and lose it where armour is
    /// not - otherwise the answer is just "buy the expensive one" (GUNS §7.3).
    #[test]
    fn armour_piercing_earns_its_price_and_not_everywhere() {
        let zone = load_zone(Path::new("assets/data"));
        let trials = 600;
        let ball = Loadout::new("ball").weapon("rifle", &[]).ammo("rifle_round", 60).skill(70);
        let ap = Loadout::new("ap").weapon("rifle", &[]).ammo("rifle_ap", 60).skill(70);

        // The wall: armour is what stops you, so pierce is what gets through it.
        let a = simulate(&ball, "pseudogiant", trials, 13, &zone);
        let b = simulate(&ap, "pseudogiant", trials, 13, &zone);
        assert!(
            b.win_rate() > a.win_rate(),
            "AP should beat ball on the pseudogiant: {:.2} vs {:.2}",
            b.win_rate(),
            a.win_rate()
        );

        // The dog has no armour to pierce, so the same rounds are only dearer.
        let a = simulate(&ball, "blind_dog", trials, 13, &zone);
        let b = simulate(&ap, "blind_dog", trials, 13, &zone);
        assert!(
            b.ru_per_kill() > a.ru_per_kill(),
            "AP should cost more per dog than ball: {:.0} vs {:.0}",
            b.ru_per_kill(),
            a.ru_per_kill()
        );
    }

    /// The 2 AP has to buy something, or nobody would ever pay it (GUNS §7.3).
    #[test]
    fn reloading_beats_running_dry() {
        let zone = load_zone(Path::new("assets/data"));
        let trials = 600;
        // The sawn-off is where the magazine bites: two shells, then a pause.
        let feeds = Loadout::new("feeds it").weapon("sawn_off", &[]).skill(40);
        let dry = Loadout { policy: Policy::Dry, ..feeds.clone() };

        let a = simulate(&dry, "flesh", trials, 17, &zone);
        let b = simulate(&feeds, "flesh", trials, 17, &zone);
        assert!(
            b.win_rate() > a.win_rate() + 0.15,
            "feeding it has to be worth the AP: dry {:.2} vs fed {:.2}",
            a.win_rate(),
            b.win_rate()
        );
        assert!(b.reloads > 0.5, "a two-shell gun reloads most fights: {:.1}", b.reloads);
    }

    /// A magazine number tuned into a wall is the failure this catches: both guns
    /// still win the fights they are meant to win (GUNS §7.3).
    #[test]
    fn the_magazine_does_not_decide_the_fight() {
        let zone = load_zone(Path::new("assets/data"));
        let trials = 600;
        let sawn_off = Loadout::new("sawn-off").weapon("sawn_off", &[]).skill(40);
        let rifle = Loadout::new("rifle").weapon("rifle", &[]).armor("vest", &[]).skill(70);

        let a = simulate(&sawn_off, "flesh", trials, 19, &zone);
        assert!(a.win_rate() > 0.5, "two shells still take a Flesh: {a:?}");
        let b = simulate(&rifle, "bloodsucker", trials, 19, &zone);
        assert!(b.win_rate() > 0.5, "twenty rounds still take a bloodsucker: {b:?}");
    }

    /// GDD §7: ammunition is a sink, not a tax. A Flesh Eye sells for 400, and the
    /// fight that produced it must not have cost anything like that (GUNS §7.3).
    #[test]
    fn ammunition_is_a_sink_not_a_tax() {
        let zone = load_zone(Path::new("assets/data"));
        let s = simulate(
            &Loadout::new("kitted").weapon("rifle", &[]).armor("vest", &[]).skill(70),
            "flesh",
            600,
            23,
            &zone,
        );
        let eye = zone.items["flesh_eye"].base as f32;
        assert!(s.ru_per_kill() > 0.0, "shooting things costs money: {s:?}");
        assert!(
            s.ru_per_kill() < eye / 3.0,
            "a Flesh must be worth shooting: {:.0} RU of ammunition per {eye} RU eye",
            s.ru_per_kill()
        );
    }

    /// The ten rounds in the starting kit are worth nothing until somebody finds a
    /// pistol. Cheap, and it catches a caliber wired to the wrong gun (GUNS §7.3).
    #[test]
    fn the_kit_ammunition_is_useless_without_a_gun() {
        let zone = load_zone(Path::new("assets/data"));
        let knife = Loadout::new("knife").weapon("knife", &[]).skill(25);
        let s = simulate(&knife, "blind_dog", 600, 5, &zone);
        assert_eq!(s.spent, 0.0, "a knife spends no rounds");
        assert!(s.win_rate() > 0.6, "and still handles the first dog: {s:?}");
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

    /// The rifle costs a thousand more than the heavy revolver and must hit harder,
    /// or it is dead weight on the gunrunner's shelf. Both swing the same skill, so
    /// the only thing that can separate them is the dice.
    #[test]
    fn the_rifle_earns_its_price_over_the_revolver() {
        let zone = load_zone(Path::new("assets/data"));
        let trials = 600;
        let revolver = Loadout::new("revolver").weapon("revolver", &[]).skill(60);
        let rifle = Loadout::new("rifle").weapon("rifle", &[]).skill(60);
        let a = simulate(&revolver, "boar", trials, 7, &zone);
        let b = simulate(&rifle, "boar", trials, 7, &zone);
        assert!(
            b.win_rate() > a.win_rate(),
            "the rifle must beat the revolver against a boar ({:.2} vs {:.2})",
            b.win_rate(),
            a.win_rate()
        );
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

    /// The first hours, measured as a real new player actually arrives: whatever the
    /// kit holds and whatever 600 ₽ buys, at the skill a fresh stalker really has
    /// (25 untagged, 40 tagged) rather than the kitted 60 the other reports assume.
    #[test]
    #[ignore = "balance bench: cargo test --release -- --ignored --nocapture balance"]
    fn the_first_hours() {
        let mut loadouts = Vec::new();
        for skill in [25, 40] {
            for (name, w) in [
                ("bare hands (sold the knife)", None),
                ("knife (in the kit)", Some("knife")),
                ("knife + jacket (550 R)", Some("knife")),
                ("sawn-off (hatch)", Some("sawn_off")),
            ] {
                let mut l = Loadout::new("x").skill(skill);
                if let Some(id) = w {
                    l = l.weapon(id, &[]);
                }
                if name.contains("jacket") {
                    l = l.armor("jacket", &[]);
                }
                loadouts.push(Loadout { name: format!("{name}, skill {skill}"), ..l });
            }
        }
        for enemy in ["blind_dog", "flesh", "bandit"] {
            report("The first hours", enemy, &loadouts, TRIALS);
        }
    }

    /// The whole arsenal, price order, against the whole roster. Reads as the ladder
    /// a player climbs, so anything that does a cheaper thing's job (or fails to earn
    /// its price) sticks out as a flat spot or a step backwards.
    #[test]
    #[ignore = "balance bench: cargo test --release -- --ignored --nocapture balance"]
    fn the_whole_arsenal() {
        let zone = load_zone(Path::new("assets/data"));
        let mut weapons: Vec<&String> = zone
            .items
            .keys()
            .filter(|id| matches!(zone.items[*id].kind, crate::area::ItemKind::Weapon { .. }))
            .collect();
        weapons.sort_by_key(|id| zone.items[*id].base);
        let guns: Vec<Loadout> = weapons
            .iter()
            .map(|id| Loadout {
                name: format!("{id}  {}R", zone.items[*id].base),
                ..Loadout::new("x").weapon(id, &[]).skill(60)
            })
            .collect();
        let mut enemies: Vec<&String> = zone.enemies.keys().collect();
        enemies.sort_unstable();
        for enemy in &enemies {
            report("Arsenal (skill 60, no armour)", enemy, &guns, TRIALS);
        }

        let mut armors: Vec<&String> = zone
            .items
            .keys()
            .filter(|id| matches!(zone.items[*id].kind, crate::area::ItemKind::Armor(_)))
            .collect();
        armors.sort_by_key(|id| zone.items[*id].base);
        let suits: Vec<Loadout> = armors
            .iter()
            .map(|id| Loadout {
                name: format!("rifle + {id}  {}R", zone.items[*id].base),
                ..Loadout::new("x").weapon("rifle", &[]).armor(id, &[]).skill(70)
            })
            .collect();
        for enemy in &enemies {
            report("Armour (rifle, skill 70)", enemy, &suits, TRIALS);
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

    /// The Blind Dog is the first fight a new stalker actually meets, and the knife
    /// they arrive with has to handle it even untagged. Otherwise the opening hour
    /// is a permadeath coin toss instead of a lesson.
    #[test]
    fn the_kit_knife_handles_the_first_dog() {
        let zone = load_zone(Path::new("assets/data"));
        let knife = Loadout::new("knife").weapon("knife", &[]).skill(25);
        let s = simulate(&knife, "blind_dog", 600, 5, &zone);
        assert!(
            s.win_rate() > 0.6,
            "an untagged fresh stalker should beat the first dog: {s:?}"
        );
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
