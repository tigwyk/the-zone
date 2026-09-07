# The Zone

A text-menu adventure RPG in Rust and Bevy. You play a stalker walking into a
quarantined wasteland where physics has gone soft, artifacts are worth more than your
life, and rumour says the centre grants wishes. You move area to area through ASCII
scenes, picking from a short menu of what makes sense right here. When you die, the run
is over. Someone else goes in, and the Zone is still there.

Three dependencies, no engine editor, no mouse. The whole game renders into one
120 by 33 character grid.

## Running it

```bash
cargo run
```

Build with `cargo build` and test with `cargo test`. The suite is 111 tests, and 25 of
them are whole play-throughs: they build the same app headless, press keys into it, and
read the character grid back as text.

The combat and loot benches are heavier, so they are marked `#[ignore]` and print
rather than assert:

```bash
cargo test --release -- --ignored --nocapture balance
```

That prints win rates, deaths, break-offs, rounds, health left and rubles of
ammunition per kill for every weapon against every enemy in the game.

## What a screen looks like

One fixed grid, no scrolling. Rows never move:

```
row  0-17  ART          the area scene, secrets drawn as bold cyan letters
row  19-20 DESCRIPTION  two lines, second person, present tense
row  22-26 MENU         up to five contextual verbs
row  28    MESSAGE      what just happened
row  30    STATUS       HP 24/32  RAD 210  RU 1340  Day 3 06:40  [emission soon]
row  32    FOOTER       Tab Inventory  F2 Map  F3 Journal  F4 Scanlines  F5 Save
```

Arrow keys move the cursor and Enter confirms. Letter keys are never menu shortcuts,
because letters belong to the secrets: hidden exits are drawn into the art itself as
uppercase letters in bold cyan, and pressing one takes you through. Ten of the thirty
areas have no menu entry anywhere and can only be reached by noticing something in a
picture.

## What is in the game

Thirty hand-authored areas, seven kinds of thing that fight back, twelve artifacts,
four vendors, ten speaking NPCs, twenty jobs and six endings. Skills rise by use rather
than by an XP table. Radiation drags your attributes down and kills you at 1000. Every
three to five days an emission burns the sky, and you get an hour of warning to find
cover.

Combat is turn-based on action points, at close quarters, with no grid and no distance
to close. A turn is 5 + AGI/2 points. Swinging costs 3, aiming costs 6, feeding a gun
costs 2 (or 1 once you know how), and digging through your pack mid-fight costs 4. Guns
take a caliber and hold a magazine, so a sawn-off spends a turn in three reloading and
a rifle rarely notices. Ammunition moves your accuracy, your damage and how much armour
you punch through.

Gear the Zone has been at carries affixes, and how many it carries is its rarity: plain,
touched, marked, warped, relic. Mods are the other half of that. Affixes are what the
Zone did to a thing and mods are what a person did, so a scoped pistol is still a plain
pistol that somebody looked after. At the bench under the camp you can build medicine
and gear out of scavenged parts, and at the forge you can feed an artifact to a weapon
to bake one specific affix into it. Fail that roll and the artifact is gone. Fail it
badly and your weapon comes back cursed.

Death deletes the save. What carries over to the next stalker is the lore you found, a
quarter of your faction standing, the backgrounds you unlocked, and a memorial entry
with your name, your days survived, how you died and the last thing that happened to
you. New stalkers can read those notes at the camp. Some of them are hints.

## Where the code lives

| Path | Holds |
|---|---|
| `src/main.rs` | app wiring, states, input systems, the play-through tests |
| `src/render.rs` | glyphs, palette, the grid renderer, scanlines |
| `src/area.rs` | the `.area` and `zone.ron` loaders, the area screen |
| `src/run.rs` | the stalker, the d100 check, the one price formula |
| `src/screens.rs` | inventory, trade, map, creation, shared chrome |
| `src/sim.rs` | clock, radiation, emissions, anomaly fields |
| `src/combat.rs` | AP turns, reloading, the enemy state machine |
| `src/craft.rs`, `src/liquid.rs` | the bench and forge; liquids pooling on floors |
| `src/loot.rs` | rarity, affixes, mods, item instances |
| `src/quest.rs`, `src/dialogue.rs`, `src/meta.rs` | jobs, NPCs, what outlives a run |
| `src/balance.rs` | the combat bench (test only) |
| `assets/data/` | every piece of content, as RON and `.area` scenes |

Content is data. Areas are text files where the art is exactly what renders, and the
loader checks the whole world at startup: that every id resolves across files, that
every area can be walked to from the start, that no lore entry or enemy has been
written with nowhere to appear, and that no gun eats a caliber nobody sells. A content
mistake fails the build rather than reaching a player.

## Documentation

[GDD.md](GDD.md) is the design and every number in it. [SPEC.md](SPEC.md) is how the
code and data are allowed to work. [PLAN.md](PLAN.md) tracks status, and its section 0
is where things actually stand. [GUNS.md](GUNS.md) is the spec for the weapon system,
kept as a record of what the bench measured.

Numbers that a player can feel are measured rather than argued about. When the bench
disagrees with the design, the design document gets edited.

## Status

Playable start to finish. Nobody has played it yet, so pacing, whether the secrets are
findable without knowing they are there, and whether a run is the right length are all
still open questions.
