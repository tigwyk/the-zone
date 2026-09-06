# The Zone — Implementation Spec

Rules for anyone (human or agent) writing code or content in this repo. Read fully
before editing. [GDD.md](GDD.md) says what the game is; this says how it is built.
[PLAN.md](PLAN.md) says what to build next.

---

## 1. Toolchain

- Rust, edition 2024. `bevy = "=0.19.1"` pinned. Do not bump Bevy without a PLAN.md
  entry; its API shifts every release.
- Build and run: `cargo build`, `cargo run`. Tests: `cargo test`. Nothing else.
- Dependencies are added only at the milestone that needs them (PLAN.md §4). Before
  adding one, check whether `std`, Bevy, or ten lines of code covers it.
- Windows is the dev platform. Paths in code go through `std::path`, never string
  concatenation.

## 2. Repo layout

```
src/main.rs        add_game (states + systems), window/renderer wiring, playthrough
src/render.rs      Glyph, TileGrid, Palette, render_grid          (split in M1)
src/area.rs        .area loader, zone.ron loader, scene build_grid (split in M1)
src/run.rs         RunState, Rng, check(), price()                    (M2)
src/screens.rs     modal screens (creation, inventory, trade) + chrome (M2)
src/sim.rs         clock, radiation, emissions, anomaly fields, gates    (M3)
src/combat.rs      AP turns, the enemy machine, the combat screen          (M4)
src/dialogue.rs    NPCs, menu-option dialogue, the dialogue screen        (M5)
src/quest.rs       jobs, standing, the job board and journal screens      (M5)
src/meta.rs        memorial, unlocks, suspend, endings, the save files    (M6)
src/loot.rs        rarity, affixes, item instances, rolling              (M9)
src/balance.rs     combat bench: loadouts, trials, reports   (test-only, M10)
src/audio.rs       the three cues                                        (M7)
tools/make_sounds.py  synthesises assets/audio; the .wav files are the input
assets/data/       all game content — see §5
PLAN.md GDD.md SPEC.md
```

Split a file when it hurts to scroll, not before. One module per concept that exists.
No `mod.rs` trees, no `prelude`s of our own.

## 3. Architecture contract

**Shape of every frame:** `input → mutate resources → build_grid → render_grid`.
Every screen in the game is a `build_grid`-style function that writes into the one
`TileGrid`. There is no second rendering path.

- **State lives in resources.** `RunState`, `CurrentArea`, `MenuSelection`,
  `MessageLine`, `ZoneData`, `GameClock`, `FactionRep`, `MetaProgress`. Do not add
  `Component`s to model game objects until something has many live instances. Combat
  was expected to be the first case and was not: one enemy at a time fits in a `Combat`
  resource. ECS entities still exist for rendering only.
- **Bevy `States`** drive which input and build systems run:
  `MainMenu, CharacterCreation, Area, Inventory, Trade, Map, Journal, Status, Dialogue,
  Combat, GameOver, Ending`. `Area` is home; every other in-run state returns to it on
  `Esc` unless the GDD says otherwise (Combat has no Esc).
- **`render_grid` runs only when `TileGrid` is changed** (`is_changed()`). Never
  rebuild the grid every frame. If you mutate resources, call the screen's build
  function once at the end of the input system.
- **All randomness goes through one `Rng` resource** seeded at run start, so a run
  can be replayed from its seed. No `rand::thread_rng()` in game logic.
- **All skill checks go through `check(skill, modifiers, &mut rng) -> Outcome`**
  (`Outcome { CritFail, Fail, Success, CritSuccess }`). Callers never roll d100
  themselves. `check_crit(.., crit_on, ..)` widens the crit window (an aimed shot
  crits on 5); `check_skill(&mut run, skill, .., rng)` is the one to call when the
  stalker is practising, because it also lets the skill improve on a success.
- **All prices go through `price(item, vendor, run, buying: bool) -> u32`.**
- **All standing changes go through `quest::adjust_rep`**, which applies GDD §9's
  rival spill. Never write `run.rep` directly outside character creation.
- **A run only ends through `meta::bank`**, which writes the memorial entry, banks a
  quarter of the standing and the lore, works out what the next stalker earned, and
  deletes the suspend file. Death and an ending both go through it.
- **Save files live in `SaveDir`**, a resource defaulting to the platform data
  directory. It is a resource so tests can point it at a temp dir; a test that ends a
  run and does not override it is a bug.
- **The area network is derived, never tabulated.** `area::exits` reads an area's own
  Travel actions, secret exits and anomaly far side. Adding a connection means adding
  the menu entry, and the map follows.
- Logging is for developers; players see `MessageLine`. Any `info!` describing an
  in-game event is a bug.

## 4. Rendering contract

```rust
struct Glyph { ch: char, fg: Color, bold: bool }
struct TileGrid { w: usize, cells: Vec<Glyph> }   // 120 × 33, row-major
```

- Grid is **120 columns × 33 rows** (native 16:9). Row map is fixed (GDD §3):
  art 0–17, blank 18, description 19–20, blank 21, menu 22–26, blank 27, message 28,
  blank 29, status 30, blank 31, footer 32. The art is authored ≤120 columns and drawn
  centered. Modal screens own rows 0–27 and must not touch 28–32.
- `TileGrid::set` silently ignores out-of-range writes. Build functions may rely on
  that instead of bounds-checking every string. The exception is row 27: draw it with
  `screens::draw_message`, which truncates with an ellipsis, because a message built
  out of several events must not lose its tail without saying so.
- **Presentation that needs a window lives in `main`**, never in `add_game`: the
  renderer, the scanline overlay, the HUD-glitch pass and the audio cues are all
  registered there, which is what keeps the play-through tests headless.
- `bold` renders as brightness (`bold_color`), not a bold face. Do not add a font.
- **Palette** is one `Palette` const with named colors: `dim, ground, pale, fire,
  smoke, water, amber, cyan, secret, glitch, red, grey, desc, menu, menu_sel, status`.
  `glitch` is presentation-only (the HUD-interference fringe); content never names it.
  Code and content reference names. No `Color::srgb(...)` literals outside the palette.
- Default font only. **Art is ASCII 32–126.** The loader rejects anything else.
- Screen origin is derived from the window size and the font's advance, not a magic
  translation. Window: 1280×720 logical, vsync; the native scale factor is respected
  so a high-DPI (Retina) display gets a sharp, readable window rather than a
  quarter-sized one.

## 5. Data formats

Everything under `assets/data/` is loaded once at startup with `std::fs` and `ron`.
A load error is a panic with the file name and line; do not fall back to defaults.

### 5.1 `areas/<id>.area`

```
<art lines, up to 18 rows, up to 120 columns after marker stripping>
---
<description, 1–2 lines, ≤ 78 chars each>
```

- A hidden letter is written **inline** as `{X}` where `X` is one uppercase ASCII
  letter. The loader removes the braces, keeps `X` at that column, and records
  `(x, y, 'X')`. The braces displace the rest of that line by two columns; authors
  pad accordingly. A plain `X` elsewhere is plain art.
- Each letter appears at most once per file. Duplicate → load panic.
- Colors: the loader assigns palette names by character class (`~` smoke/water,
  `^` fire, `*` `+` electro, `@` gravity, `.` `:` acid, letters/others ground). A line
  starting with `#!` after the art can override: `#! row=3 col=10..20 color=amber`.
  Add overrides only when the class rule reads wrong.
- The art in the file is exactly what renders. No runtime layout.

### 5.2 `zone.ron` (one file until M5)

```ron
Zone(
    start: "camp",
    areas: {
        "camp": Area(
            name: "Base Camp",
            art: "camp",                        // areas/camp.area
            shelter: true,
            menu: [
                ("Travel", Travel("road")),
                ("Look",   Say("The fire pops. The lantern gutters.")),
                ("Rest",   Rest),
            ],
            secrets: {
                'D': Secret(action: Travel("hatch"), gate: None),
                'R': Secret(action: Say("A note is wedged under the radio."),
                            gate: Some(Check(StalkerLore, -10))),
            },
        ),
    },
)
```

`Action` enum, grown one variant per milestone and listed here when added:

| Variant | Since | Meaning |
|---|---|---|
| `Travel(AreaId)` | M1 | move to area, costs 1 h |
| `Say(String)` | M1 | set `MessageLine` |
| `Rest` | M2 | 8 h, heal, clear rads, costs ₽ at camp |
| `Trade(VendorId)` | M2 | enter Trade state |
| `Talk(NpcId)` | M5 | enter Dialogue state |
| `Jobs(FactionId)` | M5 | enter Jobs state (that faction's board) |
| `Scan`, `ThrowBolt`, `PushThrough`, `TakeArtifact` | M3 | anomaly field verbs |
| `SetFlag(String)` | M3 | quest/secret flag |
| `Memorial` | M6 | show the fallen |
| `Lore(LoreId)` | M8 | turn up a lore entry; kept across runs |
| `Give(ItemId, u32)` | M8 | put something in the pack, once per area per run |
| `End(EndingId)` | M6 | end the run on that ending; nothing follows it |

Items and vendors sit in the same file (SPEC §5.3 carves them out later):

```ron
    items: {
        "medkit": Item(name: "Medkit", base: 250, kind: Heal(25)),
        "pistol": Item(name: "PMm Pistol", base: 900, kind: Weapon(8)),
        "jacket": Item(name: "Leather Jacket", base: 400, kind: Armor(2)),
        "bolt":   Item(name: "Bolt", base: 5, kind: Misc),
    },
    vendors: {
        "trader": Vendor(name: "Sidorovich", faction: "loners", markup: 1.0,
                         stock: [("medkit", 3), ("bolt", 20)]),
    },
```

`ItemKind`: `Heal(i32)`, `Antirad(i32)`, `Weapon(dice: (u32, u32), skill: Skill)`,
`Armor(i32)`
(damage resistance), `Artifact(attr: usize, bonus: i32, rads: i32)` (carried: shifts one
attribute, costs rads every hour), `Light` (cancels the night PER penalty), `Misc`.
A vendor's optional `artifact_markup` (default 1.0) multiplies its own markup on
artifacts only. Vendor `stock` is the starting shelf; the live shelf is
the `VendorStock` resource, so trading does not mutate loaded data.

`Gate`: `None`, `Check(Skill, i32)` (rolled once per run on first entry),
`Flag(String)`, `Rep(FactionId, i32)`, `AnyRep(i32)` (that high with anyone at all).

An area with an anomaly carries a field block, and only such an area may use the
anomaly verbs (the loader checks):

```ron
    anomaly: Some(Anomaly(
        name: "Whirligig", danger: 55, dice: (6, 6),
        artifact: "gravi", beyond: "quarry",
    )),
```

`danger` is the percent chance that `PushThrough` hurts; `dice` is contact damage NdS.
`TakeArtifact` is dropped from the rendered menu until something reveals an artifact.

An area may name one resident, fought once per run on arrival, and enemies are their
own table:

```ron
        "quarry": Area(..., encounter: Some("flesh"), ...),
    enemies: {
        "flesh": Enemy(name: "Flesh", hp: 26, ap: 7, skill: 45, dice: (2, 6),
                       armor: 1, flees: true, loot: [("flesh_eye", 1)]),
    },
```

Menus have at most 5 entries; the combat menu is three (Attack, Aim, Flee), which is why
using an item in a fight is the footer's Inventory (4 AP) rather than a fourth verb. A
secret letter must not also be a menu key (menus have no letter keys, so this is
automatic; keep it that way).

### 5.3 The other data files

Carved out of `zone.ron` at M5. `zone.ron` is now only `start` plus `areas` — the
world. Beside it:

| File | Holds |
|---|---|
| `items.ron` | `{ id: Item(name, base, kind) }` |
| `vendors.ron` | `{ id: Vendor(name, faction, markup, artifact_markup?, stock) }` |
| `enemies.ron` | `{ id: Enemy(...) }` |
| `npcs.ron` | `{ id: Npc(name, faction, start, nodes) }`, dialogue included |
| `quests.ron` | `{ id: Quest(name, faction, text, goal, rubles, rep) }` |
| `factions.ron` | `{ id: Faction(name, rivals) }` |
| `endings.ron` | `{ id: Ending(name, literal, corrupted) }` |
| `lore.ron` | `{ id: Lore(title, text) }` |
| `affixes.ron` | `{ id: Affix(name, slot, fits, effect, range, value) }` |

Anomalies stay inline in the area that has one; backgrounds stay as consts in
`run.rs`; dialogue nests inside its NPC. Split those out when they outgrow a screen,
not before. Ids are lowercase snake_case strings everywhere, and the loader validates
that every referenced id exists — across files — and panics on a dangling one.

A dialogue line may carry a `req`, which is a threshold and not a roll (`Skill(Skill,
i32)`, `Rep(FactionId, i32)`, `Flag(String)`, `Rubles(u32)`, `Killed(EnemyId)`,
`QuestsDone(usize)`). The Room is an NPC, so its wish list is `npcs.ron` content built
out of the run by those same requirements — there is no bespoke wish screen. Write the requirement into the line's
own text in brackets, GDD-style; an unmet line still renders, greyed, and refuses.

A quest `goal` is `Have(ItemId)`, `Reach(AreaId)` or `Kill(EnemyId)`. Goals are
checked after every action and settle themselves — there is no hand-in step yet. A
quest may name `requires: Some(QuestId)`, which keeps it off the board until that one
is settled; that is all a quest chain is.

An enemy may set `ambush` (unseen until it strikes: a PER check, or it opens on you at
melee) or `mind` (a will check each turn, or the action is lost).

### 5.4 Loot

An item in a pack is an `ItemStack`, not an id: `{ uid, id, count, affixes }`. Plain
stacks merge; anything with a roll on it is its own object, because no two of them are
the same any more. Equipment slots hold a **uid**, so they can point at one particular
pistol.

- **Rarity is derived from the number of rolls** (`Rarity::of`), never stored, so the
  two can never disagree: 0 plain, 1 touched, 2 marked, 3-4 warped, 5-6 relic.
- **Only weapons and armour roll.** A medkit is a medkit.
- An affix has a `slot` (`Prefix` one word, `Suffix` the whole "of the ..." phrase), a
  `fits` (`Weapon`, `Armor`, `Any`), one `effect`, an inclusive `range` it rolls its
  magnitude in, and a `value` in rubles per point.
- `effect` is `Damage`, `ToHit`, `Armor`, `Crit`, `Attr(n)` or `Rads`. **Every one has
  exactly one place in the code that reads it** — that is what keeps the list honest.
  Do not add an effect without a hook, and do not read one in two places.
- **Affixes apply when equipped, not when carried** (`sim::worn_bonus`). Artifacts are
  the opposite: they work from the pack and charge rads for it.
- Rarity is rolled out of a thousand and pushed by depth and luck, capped so the top
  band stays rare. An area carries a `tier`, an enemy carries a `tier`, and both feed
  the same roll.
- `cargo test -- --ignored --nocapture` prints a sample of what actually drops. Balance
  is the one thing the assertions cannot judge; that is what the printer is for.

**Content integrity is tested, not hoped for.** `area.rs` asserts the GDD §12 counts,
that every area can be walked to from the start, and that no lore, job board or enemy
has been written with nowhere to appear. Adding content that nothing reaches fails the
build.

## 6. Input contract

| Key | Owner |
|---|---|
| Up, Down, Enter | current menu |
| Esc | close overlay; on `Area` does nothing |
| A–Z | `Area` state only: look up the area's secrets table |
| Tab | Inventory toggle |
| F2 Map, F3 Journal | footer, from `Area` |
| F4 | scanlines on/off, anywhere |
| F5 | suspend and quit, from `Area` only — never out of a fight |

There is no F1 Status: the inventory panel already shows the attributes and skills it
would, so M7 took it out of the footer rather than leaving the footer lying.

Letters are never menu accelerators. Do not add mouse handling.

## 7. Coding rules

- **Ponytail is on.** Shortest working diff; stdlib before crates; no abstraction with
  one implementer; no config for a value that never changes. Mark deliberate corners
  with a `// ponytail:` comment naming the ceiling and the upgrade path.
- Bug fix = root cause. Grep every caller before touching a shared function.
- `unwrap()` is fine on data loaded at startup (it is a content error). It is not
  fine on player input or file IO at runtime.
- No `pub` unless another module uses it. No traits for one type. No generics for
  one instantiation.
- Numbers from GDD tables (thresholds, AP costs, price factors) live as named consts
  next to the function that uses them, with the GDD section in a comment. Do not
  scatter magic numbers.
- **The game runs headless.** `add_game(&mut App)` registers every resource, state and
  input system; `main` adds only the window, the camera and `render_grid` on top. So a
  test builds the same app on `MinimalPlugins + StatesPlugin`, presses keys into
  `ButtonInput<KeyCode>`, and reads the `TileGrid` back as text. Keep it that way: no
  game logic in `main`, nothing in an input system that needs a window.
- **Balance is measured, not guessed.** `balance.rs` drives the real `combat::act`
  loop with a built `Loadout` — a weapon and armour with named affix rolls, a skill, a
  policy, attribute overrides — for as many trials as you like, and prints win rate,
  deaths, break-offs, rounds and health left. Reports are `#[ignore]`d; run them with
  `cargo test --release -- --ignored --nocapture balance`. Every trial builds a fresh
  stalker, because `check_skill` lets a skill climb as it is used and that would drift
  a long run. Seeds are fixed per row, so any number in a report can be reproduced.
  **Do not model the rules a second time in the bench** — it must call the same code
  the game does, or it will measure a game that does not exist.
- Findings that matter get a non-ignored guard beside them, so the balance cannot
  quietly drift back: an affix has to beat a plain weapon, breaking off has to beat
  standing and fighting, and the first Flesh has to lose to the hatch gun and win
  against a knife.
- **Each milestone's acceptance play-through is a test**, in `mod playthrough` at the
  bottom of `main.rs`, driven through the `Sim` harness (`press`, `choose`, `assert_shows`,
  `assert_gutter_clear`). Seed the `Rng` so the run is reproducible. Assert on what the
  player can see, not on internals, wherever both would work.
- Every non-trivial function (a branch, a loop, a formula) gets **one** `#[test]` that
  fails if the logic breaks. Required tests so far: `check()` crit edges, `price()`
  buy > sell and Barter/rep direction, `.area` loader marker parsing and duplicate
  rejection. No fixtures, no test frameworks.
- `cargo build` warnings are errors in spirit: fix them before finishing.

## 8. Content rules

- Descriptions: second person, present tense, ≤ 2 lines, ≤ 78 chars each, no `!`.
- Messages: one line, ≤ 78 chars.
- The loader enforces both: an over-long or shouting `Say`, or a description with an
  exclamation mark in it, is a load panic naming the file.
- Art: ASCII only, ≤ 120 × 18 including secret letters. Secret letters are uppercase and
  should sit on something that makes sense to press (a door, a glint, a hatch).
- About one secret per three areas. Secrets pay off: a room, an artifact, lore.
- Palette meaning is fixed (GDD §11). Do not use bold cyan for anything but secrets.

## 9. Git

- Commit on `main` per completed PLAN.md step, message in the imperative naming the
  step (`M1.3 footer + Inventory overlay`). Include `Cargo.lock`.
- Do not commit `target/` or suspend/save files. Saves live in the platform data dir
  (`SaveDir`), not the repo.
- Every commit builds and runs. A commit that panics on startup gets reverted.

## 10. Definition of done

The roadmap is finished, so this is now the bar for any piece of work, not just a
milestone.

1. `cargo build` clean — **no warnings** — `cargo test` green, `cargo run` reaches the
   Area state.
2. Whatever a player can now do is a `playthrough` test: press the keys, read the
   `TileGrid` back. A feature nobody can reach in a test is a feature nobody has seen.
3. Anything with a number in it that a player will feel — damage, prices, drop rates,
   difficulty — is **measured**, not asserted from the armchair. `balance.rs` for
   combat, the ignored printers in `loot.rs` for drops. A finding worth acting on gets
   a guard test so it cannot drift back.
4. Docs follow the code in the same commit: PLAN.md §0 for what now stands, §8 for what
   it opened or closed; any new `Action`, `Gate`, `Req`, `Effect` or data file in §5
   here; any number a designer would want in the GDD table it belongs in.
5. Nothing speculative. If it has no caller, it does not ship.
