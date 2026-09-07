# Guns — mods, ammunition and reloading

A design + implementation spec for one piece of work (**M12**; M11 is crafting). Same split as the rest
of the repo: design decisions read like [GDD.md](GDD.md), contracts like
[SPEC.md](SPEC.md).

**Built, 2026-09-07.** GDD §6–§8, SPEC §5–§6 and PLAN §0/§8 now carry it, so this file
is kept only as the record of what was decided and what the bench said about it. Where
the build differs from the spec, the difference is marked **[built]** below.

---

## 0. The one-line version

**Affixes are what the Zone did to a gun. Mods are what a person did to it. Ammunition
is what you feed it, and the magazine runs out mid-fight.**

Three additions, each landing on machinery that already exists:

| Addition | Rides on |
|---|---|
| Weapon mods | `loot::bonus` + `Effect`, the same summing that affixes use |
| Ammunition types | the to-hit / damage / armour terms already in `combat::attack` |
| Reloading | the AP economy already in `combat::menu` and `end_turn` |

No new module. `loot.rs` grows mods, `combat.rs` grows reload, `screens.rs` grows the
lines that show them, and the rest is content.

---

## 1. Weapon mods

### What a mod is

An item. It sits in `items.ron` beside the guns, is bought from the gunrunner, drops
off bandits, and is carried in the pack until it is fitted to something.

```ron
"scope": Item(name: "Telescopic Sight", base: 900,
              kind: Mod(effect: ToHit, value: 10, fits: Weapon)),
```

A mod's magnitude is **fixed by its row**. It does not roll. That is the design line
that keeps mods from being a second rarity system: the Zone rolls, a workshop does not.

### Fitting

- Enter on a mod in the inventory fits it to what is equipped and matches its `fits`
  (weapon first, then armour). Nothing equipped that it fits → "There is nothing in use
  that the Telescopic Sight goes on."
- **Three slots** on anything (`MOD_SLOTS = 3`), and no mod twice on one item.
- **[built]** Fitting is **free**, not 10 minutes. Advancing the clock needs the
  clock, the fields and the vendor stock plumbed into the inventory system for a
  ten-minute tick nobody would feel; putting a suit on is free for the same reason.
- Cannot be done in a fight. A fight is not a workbench.
- Fitting consumes the mod item and pushes its id onto the target's `mods`.

### Removing

**[built]** **Backspace** on the item pulls the mod that went on last — the detail
block lists them in fitting order, so it is the bottom one. Enter on a fitted mod
would have needed a second cursor level inside the inventory for a rarely-used verb;
one key and one branch does the job:

```
Repair check, normal difficulty (check_skill, so it teaches)
  success  → the mod goes back in the pack
  failure  → the mod is destroyed; the weapon is unharmed
```

That is the whole risk model. No durability, no jamming, no broken guns — see §6.

This is also the first real job the Repair skill has had.

### The mod list (10 rows of content)

| id | name | effect | value | fits | base ₽ |
|---|---|---|---|---|---|
| `grip` | Rubber Grip | ToHit | +4 | Weapon | 350 |
| `laser` | Laser Sight | ToHit | +6 | Weapon | 500 |
| `scope` | Telescopic Sight | ToHit | +10 | Weapon | 900 |
| `bayonet` | Bayonet | Damage | +1 | Weapon | 300 |
| `barrel` | Match Barrel | Damage | +2 | Weapon | 1100 |
| `trigger` | Tuned Trigger | Crit | +1 | Weapon | 800 |
| `mag_well` | Extended Magazine | Mag | +4 | Weapon | 600 |
| `muzzle` | Muzzle Brake | ApCost | −1 | Weapon | 1600 |
| `plate` | Ceramic Plate | Armor | +2 | Armor | 1200 |
| `liner` | Lead Liner | Rads | −3 | Armor | 900 |

Eight of the ten reuse an `Effect` that already has a read site. Two are new (§4).

`Crit +1` is worth 5 percentage points of crit window (`CRIT_PER_POINT`), so the tuned
trigger is deliberately the same size as a `hairline` roll.

`ApCost −1` takes an attack from 3 AP to 2, which is three swings on a 7-AP turn
instead of two. It is priced as the most expensive mod in the list and it is the one
thing here that must be measured before it ships (§7).

---

## 2. Ammunition

### Calibers

A gun names a caliber and a magazine; melee weapons name neither:

```ron
"pistol": Item(name: "PMm Pistol", base: 900,
               kind: Weapon(dice: (2, 6), skill: SmallGuns, ammo: Some(Pistol), mag: 8)),
"knife":  Item(name: "Hunting Knife", base: 150,
               kind: Weapon(dice: (1, 8), skill: Melee)),
```

`ammo` and `mag` are `#[serde(default)]`, so the three melee rows do not change at all.
`Caliber` is an **enum** (`Pistol | Rifle | Shell`), not an id string, so `ItemKind`
stays `Copy` and nothing downstream of `zone.items[&id].kind` has to change. Same
precedent as `Skill`.

| Weapon | caliber | mag |
|---|---|---|
| PMm Pistol | Pistol | 8 |
| Heavy Revolver | Pistol | 6 |
| Sawn-off | Shell | 2 |
| Pump Shotgun | Shell | 5 |
| Abakan Rifle | Rifle | 20 |
| Marksman Rifle | Rifle | 10 |

The sawn-off is where the mechanic bites, and that is on purpose: it is the cheap gun,
and its price is that you spend a turn in three feeding it.

**[built] The sawn-off is 3d6, not 3d4.** At 3d4 it lost the first Flesh fight it is
meant to win — 37%, against the 50% GDD §8 requires — because a two-shell magazine
halves how often it fires. The bench measured the alternatives: a four-shell magazine
alone did not fix it (44%), and 3d6 did. So the cheap gun hits harder than the pistol
and shoots half as often, which is a better answer than a sawn-off that holds four.

### Types (9 rows of content)

```ron
"pistol_ap": Item(name: "9mm Armour-Piercing", base: 25,
                  kind: Ammo(caliber: Pistol, damage: 0, to_hit: 0, pierce: 2)),
```

| id | name | caliber | dmg | to hit | pierce | ₽/round |
|---|---|---|---|---|---|---|
| `pistol_surplus` | 9mm Surplus | Pistol | 0 | −10 | 0 | 4 |
| `pistol_round` | 9mm Ball | Pistol | 0 | 0 | 0 | 10 |
| `pistol_ap` | 9mm Armour-Piercing | Pistol | 0 | 0 | 2 | 25 |
| `shell_birdshot` | 12g Birdshot | Shell | −1 | +5 | 0 | 6 |
| `shell_buck` | 12g Buckshot | Shell | 0 | 0 | 0 | 12 |
| `shell_slug` | 12g Slug | Shell | +2 | −5 | 1 | 30 |
| `rifle_surplus` | 5.45 Surplus | Rifle | 0 | −10 | 0 | 6 |
| `rifle_round` | 5.45 Ball | Rifle | 0 | 0 | 0 | 14 |
| `rifle_ap` | 5.45 Armour-Piercing | Rifle | +1 | 0 | 3 | 35 |

Three knobs, all of which land on terms `combat::attack` already computes:

- `to_hit` folds into the existing modifier sum, beside the affix and night terms.
- `damage` folds into `InHand::damage`, the flat term that already doubles on a crit.
- `pierce` comes off the enemy's armour before `damage()` subtracts it, floored at 0.
  It is the only genuinely new arithmetic: one `.max(0)` in one function.

Pierce is the interesting choice, because armour is what stops the deep roster:
buckshot is fine on a dog and useless on a pseudogiant, and slugs cost five times what
birdshot does. **Surplus is the poverty option** — it is what you burn when you are
saving for a rifle, and the −10 is what that costs you.

### Economy

**[built]** Rounds change hands **ten at a time** (`AMMO_LOT`), or filling a magazine
is twenty key presses at the counter. They are still priced singly.

Ammunition is the first sink GDD §7 lists that had nothing behind it. A fight is
roughly six to ten shots, so ball ammo runs 60–140 ₽ a fight against a Flesh Eye worth
400 ₽. That margin is the point: a stalker who misses a lot loses money killing things.
Osip the gunrunner (markup 1.2) stocks calibers and mods; the camp Trader stocks
surplus and ball only.

---

## 3. Reloading

### The magazine

`ItemStack` carries what is in the gun: `loaded: u32` and `loaded_with: Option<String>`
(the ammo item's id). A gun found or bought comes up **empty**.

### In a fight

The combat menu gains a fourth verb:

| Action | AP |
|---|---|
| Attack | 3 (2 with a muzzle brake) |
| Aimed Attack | 6 |
| **Reload** | **2, or 1 at Small Guns ≥ 60** |
| Flee | all |

- **Attack and Aimed vanish when the magazine is empty.** You cannot dry-fire; the menu
  simply does not offer it, the same way it already hides an aimed shot you cannot
  afford.
- **Reload appears** when a gun is in hand, the magazine is not full, compatible ammo is
  in the pack, and the AP covers it.
- Reload fills the magazine from the pack with `loaded_with`; if that type is gone, it
  takes the cheapest compatible type there is. Leftover rounds from a part-empty
  magazine go back in the pack — this is a text game, not an inventory puzzle.
- **Changing ammo type mid-fight is the inventory**, which already costs 4 AP. So the
  quick reload gives you more of the same, and switching to slugs because the thing in
  front of you turned out to be armoured costs you double. That is the trade.

**`AP_MIN` stops being a constant.** (`combat::cheapest`.) The turn currently ends at `ap < AP_ATTACK`; with a
1–2 AP reload, that would strand a player who has 2 AP and an empty gun. It becomes the
cheapest action actually available this moment (the attack cost, or the reload cost when
a reload is possible). Flee is excluded — it costs whatever is left, so it is always
affordable and would end no turn.

### Out of a fight

Enter on an **ammo stack** in the inventory loads the wielded gun with that type, free
and instant. That is also how you choose which type you are carrying into the fight,
and it reuses `use_item`'s existing "did anything happen" return, so combat charges its
4 AP when it is done from inside a fight.

### The skill line

Small Guns ≥ 60 takes the reload from 2 AP to 1. One threshold, one const
(`RELOAD_FAST_SKILL`), sitting next to the AP costs with its GDD reference — the same
shape as every other number in `combat.rs`. It is a real breakpoint at exactly the skill
where GDD §8 already says the aimed shot starts to beat two swings, so a gun stalker
crossing 60 feels two things change at once.

---

## 4. Code contract

### New `Effect` variants (two, each with exactly one read site)

| Variant | Read in | Meaning |
|---|---|---|
| `ApCost` | `combat::weapon` → `InHand::ap_cost` | shifts the cost of an attack; floored at `AP_ATTACK_MIN = 2` |
| `Mag` | `combat::mag_size` | rounds on top of the weapon's own magazine |

Both are read once, into a field, and the menu and the resolver read the field — SPEC
§5.4's "one place per effect" rule holds.

### New `ItemKind` variants

```rust
Mod { effect: Effect, value: i32, fits: Fits },     // Fits is loot::Fits, already there
Ammo { caliber: Caliber, damage: i32, to_hit: i32, pierce: i32 },
```

`ItemKind::Weapon` gains `ammo: Option<Caliber>` and `mag: u32`, both defaulted.

### `ItemStack`

```rust
#[serde(default)] pub mods: Vec<String>,          // item ids, in fitting order
#[serde(default)] pub loaded: u32,
#[serde(default)] pub loaded_with: Option<String>,
```

`loot::bonus` sums affix rolls **and** mods, so every existing reader of `bonus` picks
mods up for free — `combat::weapon`, `combat::armor_of`, `sim::worn_bonus`,
`sim::attr`, `sim::artifact_rads_per_hour`. That is the whole reason mods are shaped
like this.

`loot::value` adds each fitted mod's base at full price. Rarity is untouched:
`Rarity::of(affixes.len())` never sees `mods`, so a plain scoped pistol is still plain
and still renders green.

**The one trap.** `add_stack` and `take_item` currently key merging off `is_plain()`.
A loaded or modded gun must not merge into a bare one, so those two callers move to a
new `is_stackable()` (`affixes.is_empty() && mods.is_empty() && loaded == 0`);
`is_plain` stays what it is, the rarity question, and keeps its display callers. Grep
both before touching either.

### Screens

- Inventory detail block: the affix lines it already prints, then one line per fitted
  mod (`+10 to hit  (Telescopic Sight)`), then `Loaded  5/8  9mm Ball` for a gun.
- Combat YOU row gains `AMMO 5/8` when a gun is in hand.
- The inventory hint line gains "Enter fit or pull a mod".

### Loader

Validates each `Ammo`/`Mod` id like every other id, and asserts that every caliber a
weapon names has at least one ammo row (SPEC §5's cross-file check, one more line).

---

## 5. Content deltas

| File | Change |
|---|---|
| `items.ron` | +9 ammo, +10 mods; 6 weapon rows gain `ammo`/`mag` |
| `vendors.ron` | Osip stocks calibers and mods; Trader stocks surplus and ball |
| `enemies.ron` | Bandit drops the ammo it was shooting at you; deep roster drops mods |
| `run.rs` | starting kit gains **10 rounds of 9mm surplus** — worthless without a gun, which is the joke |

Item count 42 → 61. The GDD §12 targets are floors, so nothing breaks.

---

## 6. Deliberately not built

Each of these is one sentence because that is all it should take to say no.

- **No durability, jamming, or condition.** The one failure state is the botched Repair
  check that eats a mod. Guns in the Zone work.
- **No per-weapon mod slots.** Three on everything until a playtest says the sniper
  should differ from the sawn-off.
- **No ammo weight.** There is no encumbrance in the game to hang it on.
- **No burst fire, no chambered round, no partial-magazine bookkeeping.** One shot per
  attack, leftovers go back in the pack.
- **No energy-weapon ammo.** There are no energy weapons; the skill exists and the
  items do not.
- **No mods on artifacts.** An artifact is not a machine.

---

## 7. How it gets proved (SPEC §10)

Nothing here ships on an armchair number. SPEC §10.3 says anything a player will feel
is measured, and every number in §1–§3 is one of those. Four layers.

### 7.0 The thing that breaks first  — *and it did*

**Every gun loadout in `balance.rs` today holds an empty gun.** The moment magazines
exist, `the_first_fight_is_hard_with_a_knife_and_fair_with_the_hatch_gun`,
`the_rifle_earns_its_price_over_the_revolver` and `a_better_weapon_wins_more_often` all
go to a 0% win rate, and so does every ignored report. That is the first thing step 2
of §8 has to answer, and it answers it in the harness, not in the asserts:

> `Loadout::weapon()` loads a **full magazine plus a box (30) of the caliber's ball
> ammunition** by default. `.ammo(id, rounds)` overrides both the type and the count;
> `.ammo(id, 0)` is the dry gun, which is its own experiment.

**[built]** The spec said "two spares", and two spares of a sawn-off is six shells —
the bench measured that as the gun losing a fight it should win because it ran out,
which measures the wrong thing. A box is what a player actually carries.

All four predicted failures happened on the first run: three guard tests and one
play-through went to zero the moment magazines existed.

A player who buys a gun buys ammunition with it, so the default models the player. The
existing seven guard tests then keep their meaning and only shift their numbers.

**Rule for re-baselining: when a guard fails, the content moves, not the assert.** If
the sawn-off stops winning its Flesh fight because a 2-shell magazine costs it a turn in
three, the fix is the magazine or the ammunition, not `0.5`. The one exception is a
threshold this spec is deliberately changing, and there is exactly one — see 7.2.

### 7.1 Harness changes (`balance.rs`)

| Piece | Change |
|---|---|
| `Loadout` | `+ mods: Vec<String>`, `+ ammo: Option<(String, u32)>`; builders `.mods(&["scope", "muzzle"])` and `.ammo("pistol_ap", 24)` |
| `Loadout::build` | fits the mods on the built stack, fills the magazine, puts the spare rounds in the pack — through `run.add_stack` and the real fitting function, never by hand |
| `Policy` | `+ Dry`: never reload, so the cost of running out can be measured against the cost of the reload |
| `choose()` | `Verb::Reload` when Attack is gone and Reload is offered; `Policy::Dry` skips it and falls to Flee |
| `Summary` | `+ shots: f32`, `+ ammo_spent: f32` (rubles), `+ reloads: f32`, and `ru_per_kill()` |
| `report()` | two more columns: `shots` and `₽/kill` |

`one_fight` needs no other change: it counts a round by the AP pool being handed back,
and a reload spends AP like everything else. It still drives the real `combat::act`,
which is the rule that keeps the bench honest (SPEC §7) — the ammunition is spent by the
game's own code, and the bench only reads the pack afterwards to price it.

### 7.2 New reports (`#[ignore]`, run with the bench)

| Report | Answers |
|---|---|
| `ammunition_types` | ball / surplus / AP across the roster: where does pierce start paying, and where is surplus just cheaper? Run against `blind_dog` (armour 0) and `pseudogiant` (the wall). |
| `mods_on_one_weapon` | each mod alone on a pistol at skill 55, then in threes — the same shape as the existing `affixes_on_one_weapon`, so the two lists can be read against each other |
| `the_magazine` | every gun at its own magazine size, `Fast` against `Dry`: what a reload is worth, and whether the sawn-off's two shells are a decision or a punishment |
| `the_muzzle_brake` | 2-AP attacks against the whole roster, beside 3-AP, at skills 40 / 60 / 85 |

The last one is a **gate, not a report.** GDD §8 records that dropping the attack from 4
AP to 3 changed what the game is, and that splitting the discount was measured and
rejected. `ApCost −1` is the same lever pulled again. If the brake flattens the skill
curve the aimed shot depends on — if 2-AP swinging beats aiming at 85 the way it beats
it at 40 — the mod becomes `Crit +1` at 800 ₽ and §1's table gets edited. **That is the
one threshold this spec is allowed to move, and only on the bench's evidence.**

The GDD §8 seven-enemy win table is then **re-measured and rewritten** from
`the_roster_against_a_fair_loadout`, because ammunition makes every row in it a lie.

### 7.3 New guards (not ignored, run every `cargo test`)

Beside the existing seven, in the same style — cheap, seeded, one claim each:

1. `mods_pay_for_themselves` — a scoped, braked pistol beats a bare one against the
   Flesh, and ends it in fewer rounds. (The `a_better_weapon_wins_more_often` shape,
   for mods.)
2. `armour_piercing_earns_its_price` — AP rounds beat ball against the pseudogiant, and
   **lose to ball on `ru_per_kill()` against the blind dog**. Both halves, or the answer
   is just "buy the expensive one".
3. `reloading_beats_running_dry` — the same sawn-off loadout on `Fast` and on `Dry`:
   fewer deaths with the reload, and the gap has to be visible (not one fight in six
   hundred), or the 2 AP is a tax nobody would pay.
4. `the_magazine_does_not_decide_the_fight` — a rifle (20 rounds) and a sawn-off (2)
   both still win their intended fights. This is the guard that catches a magazine
   number tuned into a wall.
5. `ammunition_is_a_sink_not_a_tax` — rubles of ammunition per kill against a Flesh
   stays well under what the Flesh Eye sells for. GDD §7's margin, asserted.
6. `the_kit_ammo_is_useless_without_a_gun` — the ten surplus rounds in the starting kit
   change nothing about the knife fight. Cheap, and it catches a caliber wired wrong.

Plus the seven existing guards, re-baselined per 7.0 and kept.

### 7.4 Unit tests (SPEC §7: one per non-trivial function)

`combat.rs`
- `pierce_comes_off_armour_and_floors_at_zero` — 3 pierce against 1 armour is 0, not −2.
- `an_empty_gun_offers_no_attack_and_a_reload` — `menu()` at `loaded: 0`.
- `a_reload_fills_the_magazine_and_returns_the_leftovers` — pack count conserved.
- `a_good_shot_reloads_faster` — 2 AP at Small Guns 59, 1 AP at 60.
- `an_attack_never_costs_less_than_two_ap` — two braked mods do not stack to 1.
- `the_turn_ends_when_nothing_is_affordable` — 2 AP, empty gun, no ammo: the turn ends
  rather than hanging. This is the `AP_MIN` change, and it is the one that would ship a
  softlock.

`loot.rs`
- `bonus_sums_affixes_and_mods_together`, and `value` adds the mod bases.
- `mods_do_not_change_rarity` — a scoped plain pistol is `Rarity::Plain`.
- `a_loaded_gun_does_not_stack_with_a_bare_one` — the `is_stackable` trap from §4.

`screens.rs`
- `fitting_then_pulling_a_mod_returns_it` (Repair high) and `a_botched_pull_eats_it`
  (Repair low) — seeded, both outcomes.
- `three_mods_is_the_limit_and_never_the_same_one_twice`.

`area.rs` (loader)
- `a_caliber_with_no_ammunition_is_a_load_error` and an unknown mod id panics — the
  SPEC §5 cross-file check, tested the way the existing dangling-id checks are.

### 7.5 Play-through (SPEC §10.2)

One new test in `mod playthrough`, driven through `Sim` like the other twenty: buy a
pistol and two boxes of ball off Osip, load it from the inventory, fit the scope, walk
to the quarry, run the magazine dry on the Flesh, reload, finish it. Assert on what the
grid says, not on internals — `AMMO 0/8`, the Reload verb appearing, the Attack verb
gone, the scope on the item's detail block.

---

## 8. Order of work

Each step builds, runs and tests green on its own (SPEC §9). Tests are written in the
step that creates the behaviour, not swept up at the end.

1. **Data.** `Caliber`, the two `ItemKind` variants, the two `Effect` variants, the
   `ItemStack` fields, `is_stackable`. Content rows. Loader validation. Nothing
   player-visible. → the `loot.rs` and `area.rs` unit tests (7.4).
2. **Shooting.** Ammunition in `combat::attack`: to-hit, damage, pierce, a round spent
   per shot, Attack hidden at empty. **`Loadout` gains its default magazine in the same
   commit** (7.0), and the seven existing guards are re-baselined here — this is the
   step where the bench either stays honest or stops meaning anything. → the pierce and
   empty-magazine tests.
3. **Reloading.** The Reload verb, the AP costs, the skill breakpoint, the `AP_MIN`
   change. → the four reload unit tests, and the `reloading_beats_running_dry` guard.
4. **Mods.** Fit, pull, the Repair check, the inventory lines. → the `screens.rs` tests
   and `mods_pay_for_themselves`.
5. **The bench.** The harness changes (7.1), the four new reports (7.2), the muzzle-brake
   gate resolved either way, the rest of the new guards (7.3), and the GDD §8 table
   re-measured and rewritten.
6. **The play-through** (7.5), then the docs fold into GDD/SPEC/PLAN and this file goes
   away.

Step 5 is not optional polish. Steps 1–4 can be made to build and pass without it, and
that is exactly the state SPEC §10.3 exists to refuse: a magazine size, a pierce value
and an AP discount are all numbers a player feels, and none of them is known until the
bench says so.
