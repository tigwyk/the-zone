# The Zone — Design & Build Plan

A text-menu adventure RPG. Rust + Bevy, ASCII/ANSI rendering. Borrows its soul from
Wasteland (the command menu, skill checks), Fallout (AP combat, radiation, retro-
futurism), STALKER (anomalies, artifacts, factions, the Zone itself), Tarkovsky's
*Stalker* (the Zone as a wish-granting, slow, atmospheric place — not just a map), and
Sanctuary RPG (area scenes, contextual menus, secrets hidden in the ASCII art).

Companion docs: [GDD.md](GDD.md) (design and numbers), [SPEC.md](SPEC.md)
(implementation contracts). This file is roadmap and status only.

**Confirmed decisions:** Bevy windowed renderer (no mage-core) · solo protagonist ·
permadeath with meta-progression · area-based scenes with contextual menus and
hidden-letter secrets · **Bevy text pass, not a glyph atlas** (locked in M0, see §4) ·
**menus are arrows+Enter only; letter keys belong to secrets** (see §6).

---

## 0. Where we are (2026-09-05)

**The roadmap is done: M0 through M7 are all in.** The game runs, ends, remembers, and
tests itself.

Eleven modules on Bevy 0.19.1, fully data-driven: `main.rs` (app, states, input),
`render.rs` (glyph, palette, renderer, scanlines), `area.rs` (loaders, scene build),
`run.rs` (the stalker), `screens.rs` (modal screens, the map), `sim.rs` (the Zone acting
on you), `combat.rs` (bands, AP turns), `dialogue.rs` (NPCs), `quest.rs` (jobs and
standing), `meta.rs` (what outlives a run), `audio.rs` (three cues). An 80x30 `TileGrid`
blitted as one `Text2d` with a `TextSpan` per same-colour run. `States {
CharacterCreation, Area, Inventory, Trade, Combat, Dialogue, Jobs, Map, Journal,
Memorial, GameOver }`.

**The run.** Creation rolls a stalker with a Zone nickname, a background and three tag
skills. Eight areas: camp, the bar, the hatch, the road, Duty's Checkpoint 4, the
Whirligig field, the quarry rim and the Room. Four vendors trade through the one
`price()` formula with Barter, standing and specialty markups. The field's tells stay
dull until you Scan, a Bolt buys the next step's answer, and the artifact you lift pays
+1 STR and 4 rads an hour. The clock runs on every action; emissions arrive every 3-5
days with an hour of warning; radiation drags attributes down and kills at 1000. Hidden
letters sit behind skill checks, flags and standing. A Flesh holds the rim, fought over
three range bands on Fallout's AP economy; skills rise by use. Dialogue lines can want a
skill or a standing and say so in brackets; job boards hand out work that settles
itself; F2 maps what you have walked, F3 is the journal.

**The end.** Stand high enough with anyone and a tunnel opens to the Room, which is an
NPC - so its wish list is content, built out of the run by the same requirements every
other dialogue line uses. Six endings, literal or corrupted. Death and endings both go
through `meta::bank`: the memorial takes the name, the days, the cause and the last
thing that happened, a quarter of the standing carries forward, and the suspend file
is deleted. F5 suspends and quits; starting again spends the file.

**The content.** GDD §12's targets are met and the loader asserts them, so content
cannot quietly drift below them: 30 areas (10 reached only through the art), 8 anomaly
fields, 12 artifacts in five families, 42 items, 7 enemies, 4 vendors, 10 speaking NPCs,
15 jobs plus the five-step main chain, 6 endings and 20 lore entries. The Zone runs from
the camp out through the reeds and the farm to Freedom, the Junkyard, the dead town, the
Lab and the antenna, with two routes to the Room. Three more tests keep it honest: every
area is walkable from the start, and no lore, job board or enemy has been written with
nowhere to appear.

**The polish (M7).** Three synthesised cues - a tick for moving, two steps up for
confirming, a thud when something hurts you - played by a system that reads the keyboard
and the stalker's health directly, so nothing had to be plumbed through the game. A CRT
scanline overlay, F4 to turn it off. A palette pass: green is the screen's base state
and the warm and cyan accents are interruptions, with every colour name accounted for.
And a content pass that made the writing rules enforceable - the loader now refuses an
over-long or shouting line - plus `draw_message`, so a message built out of several
events is truncated visibly instead of silently falling off the grid.

**The game tests itself.** `add_game` registers every resource, state and input system;
`main` adds only the window, the renderer, the scanlines and the audio. So `cargo test`
builds the same app headless on `MinimalPlugins`, presses keys into it and reads the
`TileGrid` back as text. Sixty-seven tests green, eighteen of them whole play-throughs.
Tests run on a temp `SaveDir` and never touch the player's own.

Deliberate simplifications, documented in code:
- No `rand` dependency: a six-line seeded xorshift covers d100 (`ponytail:` in `run.rs`).
- No `bevy_kira_audio` either. Bevy's own `AudioPlayer` plays a one-shot at a volume,
  which is the whole requirement; take the dependency when there is an ambience bed to
  mix under it. The `wav` feature is on so the cues decode.
- The scanlines are an overlay sprite, not a post-process shader. Curvature, bloom and
  chromatic aberration would need the real thing.
- The map is generated from `area::exits`, not a hand-drawn `map.area` scene, so it
  cannot hide a letter yet (GDD §5 wants one).
- Quests settle themselves rather than needing a hand-in; escort and deliver need
  followers and NPC-to-NPC routes.
- The Ecologist and Bandit still unlock off the Room and dying of wounds. The Lab and
  the bandits exist now, so the GDD §4 triggers could be wired to them directly.
- No `MainMenu`, and no new run without relaunching: an ending or a death ends the
  process.
- No rep-gated post entry, no roving encounters, no cover, encumbrance or ammo.
- Anomaly damage ignores armour; a scanned field is simply safe to cross.
- Emissions do not swap an area's menu; the hour of warning is the whole mechanic.
- `#!` color-override lines in `.area` files are still not implemented, and no art has
  needed one.

**What is left.** The roadmap and the content targets are both done. What would come
next is depth rather than count: the escort and deliver job shapes, a hand-drawn
`map.area` that can hide a letter, rep-gated post entry, roving encounters, and cover
and encumbrance so the rest of GDD §8's to-hit table has something to hang on.

---

## 1. One-paragraph pitch

You are a stalker entering the Zone — a quarantined wasteland where physics has gone
soft, strange artifacts hold power, and rumor says the center grants your deepest wish.
You move from area to area, each rendered as an ASCII-art scene with a **contextual menu**
of what you can do there; you manage health, radiation, and gear, survive anomalies and
mutants, do jobs for warring factions, and push toward the Room at the center — where
the wish you get may not be the wish you meant. And if you don't read the art closely,
you'll walk right past the Zone's best secrets. Then you die, someone else goes in, and
the Zone is still there.

---

## 2. Design pillars (what makes it *The Zone*, not a generic roguelike)

1. **The Zone is a place, not a backdrop.** Anomalies aren't just traps — they're the
   landscape. The deeper you go, the weirder and more dangerous it gets. Artifacts feel
   forbidden and valuable. Atmosphere over action (Tarkovsky).
2. **The art is the game.** The ASCII art of each area is not decoration — it *is* the
   interface. Secrets, exits, and hidden areas live in the art as **bold cyan letters**
   that are not listed in the menu. You survive by reading the scene, not the options
   (Sanctuary RPG).
3. **Menus are contextual.** There is no global command bar. Each area shows only the
   actions that make sense *there* — a bar offers [Talk · Trade · Drink · Rest · Leave],
   an anomaly field offers [Throw Bolt · Move · Scan · Leave], a bunker offers
   [Search · Hack · Camp · Leave]. A slim persistent footer keeps Inventory / Status /
   Map / Journal / Save.
4. **Skill checks with real stakes.** Wasteland-style roll-under checks that gate
   detection, lockpicking, medicine, speech — and *revealing* hidden letters. Failures
   are interesting, not just misses.
5. **The wish is the point.** Progress and endings revolve around the Room / Wish
   Granter. Choices (and faction allegiance) shape what "granted" means — literally vs.
   corrupted, like the film.
6. **Death is real.** Permadeath — but the Zone remembers: meta-progression (lore,
   unlocked backgrounds, faction standing) carries across runs so a death still moves
   the story forward.
7. **Bleak but readable.** Monospace glyphs, restrained color (radiation green, anomaly
   amber, artifact cyan, secrets in **bold cyan**), CRT-ish feel. Readability beats
   decoration.

---

## 3. Core loop (one run)

1. Roll a stalker (background → attributes → skills → starting gear).
2. Arrive at **base camp** on the Zone's edge. Take a job from a faction contact / trader.
3. Move area to area. Read each area's ASCII art for exits, hazards, and **hidden cyan
   letters** that unlock secret areas/actions. Detect and skirt anomalies (throw a bolt —
   STALKER's signature), loot artifacts, fight or avoid mutants.
4. Manage health / radiation / encumbrance / ammo / rubles.
5. Return to **base camp** — rest, sell artifacts to the vendor, buy supplies, spend rep,
   take the next job; or push deeper toward center.
6. Reach the Room → wish → ending (multiple, based on choices + rep).
7. **Die.** Bank any meta-progress, roll a new stalker, go back in.

---

## 4. Tech stack

**Rust + Bevy 0.19.1** (pinned `=0.19.1`, edition 2024). Bevy gives the loop,
windowing, input, asset pipeline, and a natural path to richer visuals later.

**`mage-core` — dropped.** It is a mini game-loop engine with its own `App` trait — a
competitor to Bevy's `App`, not a plugin — and once Bevy owns the window, fast ASCII is
free.

**Rendering — decided in M0: Bevy's text pass.** One `TileGrid` resource
(`Vec<Glyph>`, `Glyph = char + fg + bold`) rebuilt by `build_grid` whenever the scene
changes, and one `render_grid` system that, **only when the grid is `is_changed()`**,
despawns the screen entity and respawns a `Text2d` with one `TextSpan` per same-color
run (consecutive equal-color glyphs are merged). `ponytail:` full respawn on every
change is O(cells); per-cell spans made a keypress redraw lag, so runs fixed it —
revisit only if we ever animate per-frame.

- **Bold = brighter color** (`bold_color`), not a bold font. Good enough for "the cyan
  letter pops". A real bold face is a later asset drop, not a rewrite.
- **Font:** Bevy's default font is a FiraMono *subset* — monospace, but ASCII only.
  **Art is ASCII-only** (no box-drawing / Unicode). That's a content rule, not a bug to
  fix; it also matches the "ASCII/ANSI" pillar. Ship a full monospace `.ttf` only if a
  scene genuinely needs more glyphs.
- **Grid:** 52×24 today, positioned by a hard-coded `Transform`. M1 sets the canonical
  size to **80×30** (art up to ~80 wide × 18 tall, then 2 desc lines, menu, message
  line, footer) and derives the top-left translation from window size × font advance
  instead of the magic `(-620, 345)`.

**Dependencies (keep minimal, add at the milestone that needs them):**
- `bevy` — now.
- `serde` + `ron` — M1, when areas leave `main.rs`.
- `rand` — **not taken.** A seeded xorshift d100 is six lines in `run.rs`; revisit only
  if we need real distributions.
- `bevy_kira_audio` — M7. Skip `egui`; the menu is a hand-rolled list.

---

## 5. Architecture (Bevy, mostly resources)

This is a menu-driven solo game: there is no world of entities to simulate. **Game state
lives in resources; ECS entities exist only for rendering** (the screen entity and its
spans). Don't reach for `Component`s until something genuinely has many instances.
M4 was expected to be the first real case and was not: one enemy at a time fits in a
`Combat` resource, so entities still exist only for rendering. Revisit if a fight ever
holds several enemies at once.

**Game-flow `States`:** `MainMenu` → `CharacterCreation` → `Area` → (`Dialogue` /
`Combat` / `Trade` / `Inventory` / `Map`) → `GameOver` / `Ending`. `Area` is the main
state; the rest are modal overlays that return to `Area`. M1 introduces the enum with
just `Area` + `Inventory` to prove the overlay pattern.

**Resources:**
- `TileGrid` — the frame's glyph buffer (exists).
- `CurrentArea`, `MenuSelection` (exist).
- `MessageLine` — the last one or two lines of feedback ("You pry open the hatch.").
  Today these go to `info!`; the player never sees them. **M1.**
- `ZoneData` — everything loaded from `assets/` (areas first; items/vendors/etc. as
  each milestone adds them).
- `RunState` — this stalker: stats, skills, inventory, health, radiation, rubles,
  quest flags, discovered areas. One struct, grows per milestone.
- `GameClock` — day/night + emission timer (M3).
- `FactionRep`, `MetaProgress` (M5/M6).

**Systems:** `menu_input` (arrows/Enter → menu action; letter key → secret action),
`render_grid`, then per-milestone: travel, anomaly/radiation, clock, combat, trade.
Keep the `input → mutate resources → build_grid → render` shape; every screen is just a
different `build_grid`.

**File layout:** `main.rs` splits when it hurts, not before. Likely first cut (M1/M2):
`render.rs` (TileGrid + render_grid), `area.rs` (area loading + build_grid for scenes),
`run.rs` (RunState). No `mod` per concept until it exists.

---

## 6. Game systems

### Character
- **Attributes:** STR, PER, END, CHA, INT, AGI, LCK (compact SPECIAL-like).
- **Skills:** Small Guns, Energy Weapons, Melee, Sneak, Medicine, Repair, Lockpick,
  Science, **Stalker lore** (Zone knowledge / anomaly detection / secret-sighting),
  Barter.
- **Checks:** roll-under `skill + modifiers` on d100 (Wasteland feel). Difficulty shifts
  the threshold. 01 = crit success, 00 = crit fail. One `fn check(skill, mod) -> Outcome`
  used by everything; one self-check test for the crit edges.
- **Backgrounds** (starting spread): Loner, ex-Duty, Ecologist, Bandit — each biases
  stats/skills/faction rep. Unlocked ones persist via meta-progression.

### Contextual menus & the hidden letter (the heart)
- Each area renders its ASCII-art scene in the main window (no positional map); a short
  **area description** sits below the art, then a **contextual menu** lists only what
  makes sense there, then the **message line**, then the **footer**.
- **Input split (decided):** the menu is driven by **arrows + Enter only**. **Letter keys
  are reserved for hidden letters.** This kills the "no letter doubles as a menu key"
  bookkeeping entirely — there is nothing to disambiguate. Footer commands use
  non-letter keys (`Tab` inventory, `Esc` back, F-keys for status/map/journal/save;
  pick in M1 and keep them out of a–z).
- A persistent **footer** holds the always-available commands: Inventory · Status · Map ·
  Journal · Save.
- **Hidden letters:** the art may contain glyphs rendered in **bold cyan** — obvious to
  the eye, but *not listed* in the menu. Pressing the key runs a secret action.
  - **Authoring (replaces the position table):** in the `.area` file, wrap the secret
    glyph in braces: `[{D}]`. The loader strips the braces, records `(x, y, 'D')`, and
    the area's RON entry maps `'D' → action`. No separate coordinate table to keep in
    sync with the art; the art is the single source of truth for *where*, the RON for
    *what*. A plain `D` elsewhere in the art stays plain.
  - *Always-visible* secrets: the bold-cyan letter is always on screen once you look.
  - *Gated* secrets: the RON entry carries an optional `gate: PerCheck(40)` /
    `Flag("found_note")`; the letter renders as its plain art character until the gate
    passes, and the key does nothing. (M3.)
  - Each letter is unique within an area; the loader asserts it.

### Exploration
- The Zone is a **network of areas**, not one scrolling grid. Each area is a
  hand-authored `.area` ASCII-art file; the same file renders the scene and declares its
  hidden letters. Moving is a menu choice — visible exits, doors drawn in the art, or a
  hidden letter.
- **The Map** screen is a zoomed-out network of known areas; undiscovered areas are
  hidden until you find them (fog of war lives here, not on a tile grid). The map is
  itself an `.area` file, so it can carry hidden letters too.
- **Anomaly fields** are area scenes with risk menus: detect the anomaly (PER /
  Stalker-lore), throw a bolt, or move through and gamble.
- Day/night and emissions change what a given area's menu offers (e.g. [Shelter] during a
  blowout).

### Base camp (the hub)
- A safe area on the Zone's edge — no anomalies, no combat, low radiation. Your start
  point each run and the place you return to between expeditions.
- Menu verbs: [Talk · Trade · Rest · Jobs · Leave] plus the persistent footer. Rest
  clears radiation and heals for a cost; Jobs is the faction contact / job board.
- **Anchor of meta-progression:** discovered lore, unlocked backgrounds, and faction
  standing surface here between runs (a "memorial" lists fallen stalkers and their notes).
- It's a real place, not a menu: its own ASCII-art scene and its own hidden-letter
  secrets, like any area. (The current camp → hatch pair is this, already.)

### Anomalies & artifacts
- Anomaly types: gravitational (Whirligig), thermal (Burner), electric (Electro),
  chemical (Fruit Punch / acid), space-time. Semi-visible; detection via PER/Stalker-lore.
- Artifacts: powerful stat/gear boosts with a **radiation cost** to carry/use. Sellable —
  the economy driver.

### Vendors & the artifact economy
- A **vendor** is a contextual menu action on an NPC/area (the camp trader's [Trade]).
  Trading opens a modal **Trade** state: [Buy · Sell · Leave], then an item list with
  prices, arrows to select, Enter to confirm.
- **Price** = base × modifiers; buy > sell (a margin). Modifiers stack:
  - **Barter** skill — better margins as it rises.
  - **Faction rep** — friendly vendors discount, hostile ones inflate or refuse.
  - **Vendor specialty** — each vendor's table carries its own mark-up/mark-down.
- **Vendor types** (MVP ships the Trader; the rest are content, not new systems):
  Trader (general, pays best for artifacts) · Doctor (meds) · Technician/Gunrunner
  (weapons, ammo, repairs) · Barkeep (rumors, cheap food).
- **Restock:** stock refreshes on the clock (each day or after an emission), rolling the
  vendor's stock table. `ponytail:` a shared `StockTable` roll is the whole "economy" for
  v1 — no dynamic supply/demand; upgrade path is per-vendor demand modifiers.
- **The loop:** artifacts are the money engine — risk deeper anomalies for better
  artifacts, sell at the trader, spend on supplies.

### Radiation & survival
- Radiation accumulates from anomalies, artifacts, and the deep Zone. Thresholds impose
  stat penalties → death. Medkits/antirad/rest mitigate. Encumbrance + ammo/condition.

### Combat
- Turn-based, **AP-based** (Fallout). Range bands (melee / near / far) and cover as
  modifiers, not a positional grid — the "space" stays in the art.
- MVP: attack / aim / use-item / move-band; accuracy = skill vs. range/cover. One enemy
  type first, then mutants (Flesh → Bloodsucker → Controller → Pseudogiant) and humans.

### Factions, dialogue, quests
- **Loners, Duty, Freedom, Ecologists, Mercs, Bandits, Monolith.** Rep per faction gates
  prices, dialogue, jobs, endings.
- Dialogue: menu-option (not keyword) for MVP — simpler, still supports skill/rep gates.
- Quests: fetch artifact, rescue stalker, scout anomaly field, faction raids, main quest
  toward the Room.

### The ending
- The Room / Wish Granter. Multiple endings driven by rep, artifacts held, and a
  literal-vs-corrupted interpretation of the wish. No single "good" ending. Reaching it
  is a run's goal; the ending you earn feeds the meta-narrative.

### Death & meta-progression (permadeath)
- **Permadeath:** when you die, the run is over and the stalker is gone.
- **Suspend, not save-scum:** allow "save & quit" that resumes the run, but the file is
  deleted on death — no reloading to undo a bad choice.
- **Meta unlocks persist:** discovered lore entries, unlocked backgrounds, and faction
  standing carry across deaths. `ponytail:` this is the minimal meta layer that keeps
  permadeath compatible with a story/endings game without full roguelite buildouts.

---

## 7. Data-driven content

Everything editable without recompiling, under `assets/data/`:
- `areas/<id>.area` — the ASCII scene, with `{X}` markers for hidden letters. The
  description follows the art after a `---` separator, so one file = one area.
- `zone.ron` — **one file** for now: per-area menu + hidden-letter actions + exits.
  Split into `items.ron`, `vendors.ron`, `npc.ron`, `dialogue.ron`, `quests.ron`,
  `anomalies.ron`, `backgrounds.ron` **when a section outgrows a screen**, not before.
- Glyph/color palette as a shared table so areas and renderer agree (incl. the bold-cyan
  secret color). Today it's nine `Color::srgb` locals in `build_grid`; lift them to a
  `Palette` const in M1 and let `.area` files reference names, not RGB.
- Load with `serde` + `ron` via `std::fs` at startup for now; Bevy `AssetLoader`
  only if hot-reload becomes worth its boilerplate.

This is what lets the game grow into "content" instead of "code" after M5.

---

## 8. Roadmap (each milestone is runnable)

- [x] **M0 — Renderer spike.** Bevy window blitting a colored ASCII `TileGrid` with
  bold glyph support + one working menu. Text-pass choice locked. *(Done, 2026-09.)*
- [x] **M1 — Skeleton.** In order:
  1. [x] `git add -A && git commit` (include `Cargo.lock`; it's a binary).
  2. [x] `MessageLine` resource drawn above the footer; replace the `info!` calls.
  3. [x] Persistent footer row + `States { Area, Inventory }` with an empty Inventory
     overlay (`Tab` in, `Esc` out) to prove modal screens.
  4. [x] Move Camp/Hatch into `assets/data/areas/*.area` + `zone.ron`; `{D}` marker
     replaces char-match; loader asserts unique letters. Add `serde`, `ron`.
  5. [x] 80×30 grid, translation derived from window size. Delete the hard-coded
     `KeyCode::KeyD` branch: map any pressed letter → the area's hidden table.
  6. [x] One more anomaly-free area with a visible `Travel` exit, so travel has two
     kinds (menu exit and secret) — that's the M1 acceptance test.
- [x] **M2 — Character, inventory & trade.** Creation screen, stats/skills, `check()`,
  items, equip/use menu, and **a working vendor at the camp** (buy/sell with Barter + rep
  modifiers). Self-checks: price math (buy > sell; Barter/rep shift the margin) and the
  d100 crit edges. *(Done, 2026-09.)*
- [x] **M3 — The Zone breathes.** Anomaly-field areas + bolts + detection, artifacts,
  radiation, day/night + emission; gated hidden letters (skill checks / flags).
  *(Done, 2026-09.)*
- [x] **M4 — Combat.** Turn-based AP combat with range bands, one mutant, XP/loot.
  *(Done, 2026-09.* "XP" is GDD §4's skill-rise-by-use; the GDD rules out an XP table
  and wins on design.*)*
- [x] **M5 — World & story.** Area network + Map screen, NPCs, dialogue, quests,
  factions, more vendors. Split `zone.ron` here. *(Done, 2026-09.)*
- [x] **M6 — The end.** Room/Wish Granter, multiple endings, permadeath + suspend +
  meta saves. *(Done, 2026-09.)*
- [x] **M7 — Polish.** Audio, CRT/scanline shader, palette pass, content pass.
  *(Done, 2026-09. The shader is an overlay sprite; see §0.)*

---

## 9. Non-goals for v1 (YAGNI — deliberately cut)

- No open-world streaming — a discrete hand-authored network of area scenes with
  transitions.
- No free-roam tile walking — navigation is menu-driven (area to area); the "space" is
  in the art, not a physics grid. Combat uses range bands for the same reason.
- No procedural map generation.
- No multiplayer / modding API.
- No physics.
- No complex AI — small state-machine enemies (`ponytail:` upgrade to utility-AI only if
  combat gets boring).
- No mouse-first GUI toolkit (no `egui`); the menu is bespoke and keyboard-first.
- No i18n. No Unicode art (default font is an ASCII subset; keep it that way).
- No companion/party system (solo); a follower can be a post-v1 bolt-on.
- No glyph atlas, no custom text shader, no bold font — until brightness-as-bold
  visibly fails a playtest.
- No Bevy `AssetLoader` / hot reload for RON until editing content without restarting
  becomes the bottleneck.
