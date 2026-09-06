# The Zone

Rust + Bevy 0.19.1 text-menu adventure RPG. Before editing anything, read:

- [SPEC.md](SPEC.md) — mandatory contracts for code, data formats, input, git.
- [GDD.md](GDD.md) — what the game is; all numbers and rules live there.
- [PLAN.md](PLAN.md) — status and roadmap; **§0 is where things actually stand**, and
  §8 is what has been built and what is still open.

The roadmap (M0–M7) and the GDD §12 content targets are all done, plus loot with
rarity and a combat bench on top. Nobody has played it yet; §0 says what that leaves
unanswered.

Build: `cargo build` · run: `cargo run` · test: `cargo test` (77 tests, 20 of them
headless play-throughs). Balance reports: `cargo test --release -- --ignored
--nocapture balance`. Ponytail mode is on: shortest working diff, no speculative
abstractions.
