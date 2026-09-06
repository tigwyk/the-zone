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
  `Component`s to model game objects until something has many live instances (combat
  enemies in M4 are the first). ECS entities exist for rendering only.
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
  themselves.
- **All prices go through `price(item, vendor, run, buying: bool) -> u32`.**
- Logging is for developers; players see `MessageLine`. Any `info!` describing an
  in-game event is a bug.

## 4. Rendering contract

```rust
struct Glyph { ch: char, fg: Color, bold: bool }
struct TileGrid { w: usize, cells: Vec<Glyph> }   // 80 × 30, row-major
```

- Grid is **80 columns × 30 rows**. Row map is fixed (GDD §3): art 0–17, blank 18,
  description 19–20, blank 21, menu 22–26, message 27, status 28, footer 29.
  Modal screens own rows 0–27 and must not touch 28–29.
- `TileGrid::set` silently ignores out-of-range writes. Build functions may rely on
  that instead of bounds-checking every string.
- `bold` renders as brightness (`bold_color`), not a bold face. Do not add a font.
- **Palette** is one `Palette` const with named colors: `dim, ground, pale, fire,
  smoke, water, amber, cyan, secret, red, grey, desc, menu, menu_sel, status`. Code
  and content reference names. No `Color::srgb(...)` literals outside the palette.
- Default font only. **Art is ASCII 32–126.** The loader rejects anything else.
- Screen origin is derived from the window size and the font's advance, not a magic
  translation. Window: 1280×720, scale factor override 1.0, vsync.

## 5. Data formats

Everything under `assets/data/` is loaded once at startup with `std::fs` and `ron`.
A load error is a panic with the file name and line; do not fall back to defaults.

### 5.1 `areas/<id>.area`

```
<art lines, up to 18 rows, up to 80 columns after marker stripping>
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
| `Jobs(FactionId)` | M5 | job board |
| `Scan`, `ThrowBolt`, `PushThrough`, `TakeArtifact` | M3 | anomaly field verbs |
| `SetFlag(String)` | M3 | quest/secret flag |

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

`ItemKind`: `Heal(i32)`, `Antirad(i32)`, `Weapon(i32)` (damage), `Armor(i32)`
(damage resistance), `Artifact(attr: usize, bonus: i32, rads: i32)` (carried: shifts one
attribute, costs rads every hour), `Light` (cancels the night PER penalty), `Misc`.
A vendor's optional `artifact_markup` (default 1.0) multiplies its own markup on
artifacts only. Vendor `stock` is the starting shelf; the live shelf is
the `VendorStock` resource, so trading does not mutate loaded data.

`Gate`: `None`, `Check(Skill, i32)` (rolled once per run on first entry),
`Flag(String)`, `Rep(FactionId, i32)`.

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

Menus have at most 5 entries. A secret letter must not also be a menu key (menus have
no letter keys, so this is automatic; keep it that way).

### 5.3 Later files

`items.ron`, `vendors.ron`, `npc.ron`, `dialogue.ron`, `quests.ron`, `anomalies.ron`,
`backgrounds.ron` are carved out of `zone.ron` at M5 or when a section exceeds one
screen. Ids are lowercase snake_case strings everywhere; the loader validates that
every referenced id exists and panics on a dangling one.

## 6. Input contract

| Key | Owner |
|---|---|
| Up, Down, Enter | current menu |
| Esc | close overlay; on `Area` does nothing |
| A–Z | `Area` state only: look up the area's secrets table |
| Tab | Inventory toggle |
| F1 Status, F2 Map, F3 Journal, F5 Suspend | footer |

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
- Art: ASCII only, ≤ 80 × 18 including secret letters. Secret letters are uppercase and
  should sit on something that makes sense to press (a door, a glint, a hatch).
- About one secret per three areas. Secrets pay off: a room, an artifact, lore.
- Palette meaning is fixed (GDD §11). Do not use bold cyan for anything but secrets.

## 9. Git

- Commit on `main` per completed PLAN.md step, message in the imperative naming the
  step (`M1.3 footer + Inventory overlay`). Include `Cargo.lock`.
- Do not commit `target/` or suspend/save files. Saves live in the platform data dir,
  not the repo.
- Every commit builds and runs. A commit that panics on startup gets reverted.

## 10. Definition of done for a milestone

1. Every step in the PLAN.md milestone checklist is ticked.
2. `cargo build` clean, `cargo test` green, `cargo run` reaches the Area state.
3. The milestone's acceptance test (named in PLAN.md) exists as a `playthrough` test
   and passes.
4. PLAN.md §0 status paragraph updated; any new `Action` variant added to §5.2 here;
   any new number added to the GDD table it belongs in.
5. Nothing was added that the next milestone did not ask for.
