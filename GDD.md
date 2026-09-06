# The Zone — Game Design Document

This is the *what*. [SPEC.md](SPEC.md) is the *how* (contracts for code and data).
[PLAN.md](PLAN.md) is the *when* (roadmap and current status). When they disagree, this
document wins on design, SPEC wins on implementation.

---

## 1. Identity

**Genre:** text-menu adventure RPG with permadeath. Single player, keyboard only,
windowed, ASCII art.

**Pitch:** You are a stalker entering the Zone — a quarantined wasteland where physics
has gone soft, artifacts hold power, and rumor says the center grants your deepest
wish. You move area to area through ASCII scenes, choosing from a short contextual menu
of what you can do *here*. Secrets are hidden in the art itself as bold cyan letters
that the menu never lists. You die, someone else goes in, and the Zone is still there.

**Influences, and what each one lends:**

| Source | What we take |
|---|---|
| Wasteland | Command menu, d100 roll-under skill checks, failures that matter |
| Fallout | SPECIAL-like attributes, AP combat, radiation, bleak humor |
| STALKER | Anomalies, bolts, artifacts, factions, emissions, the Zone itself |
| Tarkovsky's *Stalker* | Slowness, dread, the Room, the wish that is not what you meant |
| Sanctuary RPG | Area scenes, contextual menus, secrets hidden in ASCII art |

**Not:** a roguelike with tile movement, an open world, a party game, a shooter.

---

## 2. Pillars

1. **The Zone is a place.** Anomalies are landscape, not traps. Deeper is stranger.
   Atmosphere over action.
2. **The art is the interface.** Reading the scene is how you find exits, hazards and
   secrets. Secrets are **bold cyan letters** in the art, never in the menu.
3. **Menus are contextual.** Each area offers only what makes sense there. No global
   command bar. A persistent footer holds the meta commands.
4. **Checks have stakes.** Roll-under d100. A failed check does something interesting,
   not nothing.
5. **The wish is the point.** The run's goal is the Room. The ending is shaped by who you
   were, not by a menu at the end.
6. **Death is real, and remembered.** Permadeath. Lore, backgrounds and faction standing
   persist across stalkers.
7. **Bleak but readable.** Restrained palette, monospace, short text. Readability beats
   decoration.

---

## 3. The screen

One fixed 80×30 character grid. No scrolling, no mouse.

```
row  0–17  ART          the area scene, up to 80×18, secrets in bold cyan
row  18    (blank)
row  19–20 DESCRIPTION  two lines max, second person, present tense
row  21    (blank)
row  22–26 MENU         up to 5 contextual verbs, "> " marks the selection
row  27    MESSAGE      result of the last action ("You pry open the hatch.")
row  28    STATUS       HP 24/32  RAD 210  AP —  ₽ 1,340  Day 3 06:40  [emission soon]
row  29    FOOTER       Tab Inventory  F1 Status  F2 Map  F3 Journal  F5 Save  Esc Back
```

Modal screens (Inventory, Trade, Map, Dialogue, Combat, Status, Journal) replace rows
0–27 and keep rows 28–29.

**Input:**

| Key | Does |
|---|---|
| Up / Down | move menu selection |
| Enter | confirm |
| Esc | close overlay / back |
| A–Z | trigger that area's hidden letter, if it exists and is revealed |
| Tab, F1, F2, F3, F5 | footer commands |

Letters never drive the menu. That is what keeps hidden letters unambiguous.

---

## 4. The stalker

### Attributes (1–10, start at 5, background shifts them)

STR (carry, melee), PER (spotting anomalies and secrets), END (HP, radiation
resistance), CHA (dialogue, prices), INT (science, skill points), AGI (AP, small
guns), LCK (crits, loot).

### Skills (0–100)

Small Guns (AGI), Energy Weapons (INT), Melee (STR), Sneak (AGI), Medicine (INT),
Repair (INT), Lockpick (PER), Science (INT), **Stalker Lore** (PER), Barter (CHA).

- Starting value = `10 + 3 × governing attribute`, plus background bonuses.
- Choose 3 **tag** skills at creation: +15 each, and they level twice as fast.
- Skills rise by use: a successful check on a skill under 50 has a 1-in-4 chance to add
  +1; over 50, 1-in-10. Tagged skills halve the odds, so they rise twice as fast. Only a
  success teaches, which includes the 01 crit. `ponytail:` no XP table; use-to-improve is the whole system.

### The check

```
roll d100
success  if roll <= skill + modifiers        (modifiers: difficulty, gear, rads)
roll == 1    crit success  (extra effect)
roll == 100  crit fail     (something goes wrong)
```

Difficulty is a modifier: easy +20, normal 0, hard −20, very hard −40. Every check in
the game goes through one function so this table is the only place it lives.

### Derived

- HP = `20 + 3 × END`. 0 = dead.
- AP (combat) = `5 + AGI / 2`.
- Carry limit = `20 + 5 × STR` kg. Over the limit: −2 AP, no Sneak.
- Radiation 0–1000 rads. Thresholds: 200 (−1 END, AGI), 400 (−2 all attributes),
  600 (−3 all, lose 1 HP per hour), 800 (−4 all, 2 HP per hour), 1000 dead.

### Backgrounds

| Background | Shift | Faction rep | Unlocked |
|---|---|---|---|
| Loner | +1 END, +10 Stalker Lore | Loners +20 | always |
| ex-Duty | +1 STR, +10 Small Guns | Duty +30, Freedom −20 | always |
| Ecologist | +1 INT, +10 Science, +10 Medicine | Ecologists +30 | reach the Lab once |
| Bandit | +1 AGI, +10 Sneak, +10 Barter | Bandits +30, Loners −20 | die to bandits once |

---

## 5. Areas

The Zone is a hand-authored **network of areas**, not a grid. Each area is one ASCII
scene, a description, a contextual menu, and zero or more hidden letters. Travel is a
menu choice or a secret letter. There is no positional movement.

### Area types and their typical menu

| Type | Menu | Notes |
|---|---|---|
| Base camp (hub) | Talk · Trade · Rest · Jobs · Leave | safe, memorial, meta anchor |
| Road / ruin | Travel · Look · Search · Camp | may have wandering encounter |
| Anomaly field | Scan · Throw Bolt · Push Through · Leave | see §6 |
| Bunker / lab | Search · Hack · Rest · Leave | lockpick / science gates |
| Faction post | Talk · Trade · Jobs · Leave | rep-gated entry |
| The Room | *(scripted)* | ending |

Menus are data. The verbs above are the default set; an area may add or drop any.
Day/night and emissions can swap a verb (an emission adds **Shelter** to any area with
cover, and removes **Travel**).

### Hidden letters

- Drawn in the art as an uppercase letter rendered **bold cyan**. Not in the menu.
- Pressing the letter runs a secret action: usually travel to a secret area, sometimes
  a scripted event or loot.
- **Always-visible:** shown from the moment the scene renders.
- **Gated:** shown only after a PER or Stalker Lore check (rolled once on first entry
  per run) or when a quest flag is set. Until then the letter renders as ordinary art.
- One letter is used at most once per area. A player who has seen the letter once can
  use it on every later visit that run.
- Roughly one secret per three areas. Secret areas hold the best artifacts and lore.

### The Map (F2)

A zoomed-out ASCII network of areas you have visited or heard about. Undiscovered
areas are simply absent. The map is itself an `.area` scene and can hide letters.
Selecting a known area and pressing Enter travels there if it is adjacent to where you
stand; otherwise it only shows the route.

### Time

- Each action costs time. Travel 1 h, Search 30 min, Rest 8 h, Trade 0, Combat turn
  10 s (does not advance the clock meaningfully).
- Day is 24 h. Night (20:00–05:00): PER checks −10 without a light source; mutants +1
  in encounters; some vendors closed.
- **Emission** every 3–5 days (rolled). One in-game hour of warning on the status row.
  When it hits: anyone not in an area flagged `shelter` takes 400 rads and 50% HP.
  Emissions restock vendors and reshuffle artifact spawns in anomaly fields.

---

## 6. Anomalies and artifacts

| Anomaly | Tell in the art | Damage on contact | Artifact family |
|---|---|---|---|
| Whirligig (gravity) | swirl `@` and bent lines | 6d6 crush | Gravi (carry +, rads +) |
| Burner (thermal) | shimmer `~` | 4d6 fire per turn | Fireball (heal, rads) |
| Electro | crackle `*` `+` | 5d6 shock, drops electronics | Flash (AP +) |
| Fruit Punch (acid) | pool `.` `:` | 3d6 acid, ruins armor | Slime (rad resist) |
| Space-time | nothing visible | teleport to random known area, 200 rads | Compass (reveal map) |

**Anomaly field flow:**

**Numbers:** a field action (scan, bolt, take) costs 10 minutes; pushing through costs
an hour, like any travel. On a scan, PER assists Stalker Lore at **+2 per point above
5**, and the night penalty applies unless you carry a light.

1. **Scan** — Stalker Lore check (PER assists). Success: the anomaly's tell renders in
   amber and the field's safe path is shown as a menu verb. Crit: also reveals one
   artifact. Fail: nothing. Crit fail: you step wrong, take damage.
2. **Throw Bolt** — costs one bolt. No check. Reveals whether the *next* step is safe.
   Bolts are cheap and the signature move; the tension is in running out.
3. **Push Through** — no check, gamble: damage chance equals the field's danger rating
   (30–80%). Reaches whatever is on the far side.
4. **Take artifact** — appears only after one is revealed. Holding it: +rads per hour
   as listed.

**In v1 so far:** the Whirligig field east of the road (danger 55, 6d6) hides a **Gravi**
(+1 STR, 4 rads an hour, base 3000 ₽). A **flashlight** (200 ₽) is the light source.

**Artifacts** give a stat bonus and a radiation cost while carried. They are the money
engine: sell at the trader, or keep and pay in rads. Deeper fields spawn better ones.
Roughly 12 artifacts in v1, 3 tiers.

---

## 7. Economy

Currency: rubles (₽). Sources: artifacts (most), job rewards, loot. Sinks: supplies,
ammo, medical, rest, repair, bribes.

**Price formula** (one function; the M2 self-check tests it):

```
buy  = base × vendor_markup × (1.5 − barter/200) × rep_buy
sell = base × vendor_markup × (0.5 + barter/400) × rep_sell
```

- Barter 0: buy 1.5×, sell 0.5×. Barter 100: buy 1.0×, sell 0.75×. Buy is always
  above sell.
- Rep: hostile refuses to trade; unfriendly buy ×1.25 sell ×0.8; neutral ×1 ×1;
  friendly buy ×0.9 sell ×1.1; allied buy ×0.8 sell ×1.2.
- Vendor markup: trader 1.0 (artifacts sell ×1.5 here), doctor 1.2 on meds, gunrunner
  1.2 on weapons and ammo, barkeep 0.8 on food.

**Restock** on each new day and after every emission: the vendor rolls its stock table
again. Unsold player items vanish from the vendor's list at restock.

**Vendors in v1:** the camp Trader (M2), then Doctor, Gunrunner, Barkeep at faction
posts (M5). They are content rows, not new code.

**Starting kit** (every background): 600 ₽, 1 medkit, 2 loaves, 5 bolts, no weapon or
armour. You arrive at base camp on day 1 at 06:00.

**Rest** at a sheltered area: 8 h, 50 ₽, heals to full and clears 100 rads.

---

## 8. Combat

Turn-based, AP-driven, no grid. Distance is one of three **range bands**: melee, near,
far. The scene art stays on screen; combatants are listed under it.

| Action | AP | Notes |
|---|---|---|
| Attack | 4 | to-hit = weapon skill + modifiers |
| Aimed attack | 6 | +20 to-hit, crits on ≤5 |
| Move band | 3 | closer or farther by one band |
| Use item | 4 | medkit, antirad, grenade |
| Reload | 2 | |
| Flee | all | Sneak check, or AGI vs. fastest enemy |

To-hit modifiers: near 0, far −20, melee weapons only at melee band, target in cover
−20, night −10 without light, over-encumbered −10, rads thresholds as attribute loss.
Damage = weapon dice − armor. Crit ×2 and ignores armor.

Enemies use a three-state machine: **approach** (close band), **attack**, **flee** at
under 20% HP (bandits and Fleshes flee; Bloodsuckers and Controllers do not).

**Roster order of implementation:** Bandit (human, gun), Flesh, Blind Dog, Bloodsucker
(invisible until it attacks: PER check to act first), Controller (forces a will check
each turn or lose the action), Pseudogiant (boss, guards the Room's approach).

**Numbers not in the table above:** bare hands do 1d3 Melee. A fleeing enemy gives up
one band per turn and only gets clear from far, so a wounded thing can still be caught.
A turn ends when you can no longer afford the cheapest action (3 AP), and the menu only
offers what your AP covers. Anomaly damage is the exception to armour: it ignores it.

**In v1 so far:** the **Flesh** on the quarry rim (30 HP, 7 AP, skill 45, 2d6, armour 1,
closes to melee, runs when hurt) drops a **Flesh Eye** worth 400 ₽. Weapons: a hunting
knife (1d8 Melee, 150 ₽) and a PMm pistol (2d6 Small Guns, 900 ₽).

Death ends the run. There is no unconsciousness.

---

## 9. Factions and story

| Faction | Where | Wants | Hates |
|---|---|---|---|
| Loners | camp, roads | to get rich and get out | nobody in particular |
| Duty | north post | contain the Zone | Freedom, Monolith |
| Freedom | south post | open the Zone | Duty |
| Ecologists | the Lab | samples and data | Bandits |
| Mercs | anywhere | money | whoever isn't paying |
| Bandits | the Junkyard | your rubles | Loners, Ecologists |
| Monolith | the center | the Wish Granter | everyone |

Rep is −100 to 100 per faction: hostile < −50, unfriendly < −10, neutral, friendly
> 25, allied > 75. Rep gates prices, dialogue options, post entry, jobs, and endings.
Helping one side moves its rival the other way at half rate.

**Dialogue** is menu-option. Lines can require a skill, attribute or rep threshold and
say so in brackets: `[Barter 40] "Half that, and you throw in the bolts."`

**Quests** come from job boards and NPCs. Five shapes only: fetch artifact, reach area,
kill target, escort, deliver. Main quest: a chain of five jobs that ends with a route to
the center, opened by whichever faction you are highest with.

**Endings** at the Room. The wish is chosen from a short list built from the run
(what you carried, whom you helped, whom you killed). Each has a literal reading and a
corrupted reading; which one you get depends on Monolith rep and a hidden LCK check.
Plus two non-wishes: refuse and walk out (Tarkovsky's ending), or be absorbed by the
Monolith if their rep is highest. Every ending writes a memorial entry.

---

## 10. Death and meta-progression

- Death ends the run. The suspend file is deleted. Death comes from 0 HP or 1000 rads;
  the end screen names the cause and the days survived.
- **Suspend** (F5) saves and quits. Resuming deletes the file. There is no reload.
- **Persists across runs:** discovered lore entries, unlocked backgrounds, faction rep
  at 25% of its final value, the memorial (name, days survived, cause of death, last
  note). The camp scene shows the memorial and lets a new stalker read fallen stalkers'
  notes, which can hint at secrets.
- **Does not persist:** items, rubles, skills, map knowledge, revealed secrets.

---

## 11. Tone and writing

- Second person, present tense. Short sentences. No exclamation marks.
- Descriptions: two lines, 78 characters each, maximum.
- Messages: one line. State what happened, not how the player should feel.
- Humor is dry and rare. Dread is constant and quiet.
- Names are Zone-flavored, not copied: our own anomaly and artifact names may echo
  STALKER's but the text is ours.
- Colors mean things: green is ground and living, amber is anomaly, cyan is artifact,
  **bold cyan is a secret**, red is damage and warning, grey is dead.

---

## 12. Content targets for v1

| Thing | Count |
|---|---|
| Areas | 30 (about 10 secret) |
| Anomaly fields | 8 |
| Artifacts | 12 |
| Items (weapons, armor, meds, misc) | 40 |
| Enemies | 6 |
| Vendors | 4 |
| NPCs with dialogue | 10 |
| Jobs | 15 plus the 5-step main chain |
| Endings | 6 |
| Lore entries | 20 |
