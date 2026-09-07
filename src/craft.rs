//! The bench and the forge (GDD §13): spend scavenged parts to make a thing, or
//! spend an artifact to bake its affix into the gear you hold. Every craft is a skill
//! check — the bench rolls Medicine or Repair, the forge rolls Science.

use bevy::prelude::*;

use crate::area::{RecipeData, ZoneData};
use crate::loot::{self, Roll};
use crate::render::{TileGrid, PALETTE};
use crate::run::{check_skill, Outcome, Rng, RunState, SKILL_NAMES};
use crate::screens::{draw_chrome, draw_message, more, title, window};
use crate::sim::GameClock;

const LIST_ROW: usize = 4;
const LIST_ROWS: usize = 16;
const DETAIL_ROW: usize = 22;

/// Runs one recipe: rolls its skill, then applies the outcome. The caller advances
/// the clock by the recipe's `minutes` afterwards, once, like any action.
pub(crate) fn craft(recipe: &RecipeData, run: &mut RunState, zone: &ZoneData, rng: &mut Rng) -> String {
    let out = check_skill(run, recipe.skill.index(), recipe.difficulty, 1, rng);
    if recipe.catalyst.is_some() {
        forge(recipe, run, zone, out, rng)
    } else {
        bench(recipe, run, zone, out)
    }
}

/// None when the recipe can be attempted now; a reason when it cannot.
pub(crate) fn craft_available(zone: &ZoneData, run: &RunState, recipe: &RecipeData) -> Option<&'static str> {
    if recipe.catalyst.is_some() {
        if run.count_of(recipe.catalyst.as_deref().unwrap_or("")) < 1 {
            return Some("no catalyst");
        }
        if held_gear(zone, run, recipe.affix.as_deref().unwrap_or("")).is_none() {
            return Some("hold gear it fits");
        }
        return None;
    }
    for (id, n) in &recipe.inputs {
        if run.count_of(id) < *n {
            return Some("missing parts");
        }
    }
    None
}

// ---- the two crafts ----

/// The bench: the parts go in whatever the roll says, and the yield is the roll.
fn bench(recipe: &RecipeData, run: &mut RunState, zone: &ZoneData, out: Outcome) -> String {
    let (out_id, out_n) = recipe.output.clone().expect("bench recipe has an output");
    for (id, n) in &recipe.inputs {
        run.take_item(id, *n);
    }
    match out {
        Outcome::CritSuccess => {
            run.add_item(&out_id, out_n * 2);
            format!("You work fast and well. You make {} of the {}.", out_n * 2, zone.items[&out_id].name)
        }
        Outcome::Success => {
            run.add_item(&out_id, out_n);
            format!("You make the {}.", zone.items[&out_id].name)
        }
        Outcome::Fail => "You spend the parts and ruin the job.".into(),
        Outcome::CritFail => {
            run.rads = (run.rads + 15).min(1000);
            "The bench bites. You spend the parts and take rads.".into()
        }
    }
}

/// The forge: the catalyst goes in whatever happens; the gear drinks the affix or
/// pays the Zone's price.
fn forge(recipe: &RecipeData, run: &mut RunState, zone: &ZoneData, out: Outcome, rng: &mut Rng) -> String {
    let catalyst = recipe.catalyst.clone().expect("forge has a catalyst");
    let affix_id = recipe.affix.clone().expect("forge has an affix");
    let Some(uid) = held_gear(zone, run, &affix_id) else {
        return "You hold nothing that would take it.".into();
    };
    run.take_item(&catalyst, 1);
    let affix = zone.affixes[&affix_id].clone();
    match out {
        Outcome::CritSuccess => {
            mark(run, uid, Roll { affix: affix_id.clone(), magnitude: affix.range.1 });
            format!("The {} settles at full strength.", zone.affixes[&affix_id].name)
        }
        Outcome::Success => {
            let magnitude = roll_magnitude(&affix.range, rng);
            mark(run, uid, Roll { affix: affix_id.clone(), magnitude });
            format!("The gear drinks it in: {}.", zone.affixes[&affix_id].name)
        }
        Outcome::Fail => "The artifact sputters out. The gear is unchanged.".into(),
        Outcome::CritFail => {
            // The Zone's spite. If it is already hot, there is nothing new to add.
            let already_hot = run.stack(uid).map_or(false, |g| g.affixes.iter().any(|r| r.affix == "hot"));
            if already_hot {
                "The artifact turns on you, but the gear is already hot.".into()
            } else {
                let hot = zone.affixes["hot"].clone();
                let magnitude = roll_magnitude(&hot.range, rng);
                mark(run, uid, Roll { affix: "hot".into(), magnitude });
                "The artifact turns on you. The gear comes out hot.".into()
            }
        }
    }
}

/// The held weapon or armour an affix fits, provided it is not already on it.
fn held_gear(zone: &ZoneData, run: &RunState, affix_id: &str) -> Option<u32> {
    let affix = zone.affixes.get(affix_id)?;
    for uid in [run.weapon, run.armor].into_iter().flatten() {
        if let Some(stack) = run.stack(uid) {
            let kind = zone.items[&stack.id].kind;
            if affix.suits(kind) && !stack.affixes.iter().any(|r| r.affix == affix_id) {
                return Some(uid);
            }
        }
    }
    None
}

/// Adds one affix roll to the held instance, splitting it off its stack first if it
/// is still a stack of several — a marked thing is one of a kind.
fn mark(run: &mut RunState, uid: u32, roll: Roll) {
    let i = run.items.iter().position(|s| s.uid == uid).expect("held gear exists");
    if run.items[i].count > 1 {
        run.items[i].count -= 1;
        // Anything with a count above one is bare (`is_stackable`), so the split
        // carries no mods and no magazine with it.
        let single = loot::ItemStack {
            affixes: vec![roll],
            ..loot::ItemStack::plain(run.next_uid(), &run.items[i].id.clone(), 1)
        };
        if run.weapon == Some(uid) {
            run.weapon = Some(single.uid);
        }
        if run.armor == Some(uid) {
            run.armor = Some(single.uid);
        }
        run.items.push(single);
    } else {
        run.items[i].affixes.push(roll);
    }
}

fn roll_magnitude(range: &(i32, i32), rng: &mut Rng) -> i32 {
    let span = (range.1 - range.0).unsigned_abs() + 1;
    range.0 + rng.roll(span) as i32 - 1
}

// ---- the screen ----

fn craft_line(zone: &ZoneData, recipe: &RecipeData) -> String {
    if let Some((out, _)) = &recipe.output {
        format!("{}  ->  {}", recipe.name, zone.items[out].name)
    } else if let Some(affix) = &recipe.affix {
        format!("{}  ->  {}", recipe.name, zone.affixes[affix].name)
    } else {
        recipe.name.clone()
    }
}

fn craft_detail(zone: &ZoneData, recipe: &RecipeData) -> String {
    let skill = SKILL_NAMES[recipe.skill.index()];
    let cost = format!("{} min", recipe.minutes);
    if let Some((out, _)) = &recipe.output {
        let parts = recipe
            .inputs
            .iter()
            .map(|(id, n)| format!("{} x{n}", zone.items[id].name))
            .collect::<Vec<_>>()
            .join(" + ");
        format!("{parts}  ->  {}   [{skill} · {cost}]", zone.items[out].name)
    } else if let Some(affix) = &recipe.affix {
        let cat = &zone.items[recipe.catalyst.as_deref().unwrap_or("")].name;
        format!("{cat}  ->  {}   [{skill} · {cost}]", zone.affixes[affix].name)
    } else {
        recipe.name.clone()
    }
}

/// Recipes in a stable order, so the cursor and the list agree (a HashMap has none).
pub(crate) fn sorted_recipes(zone: &ZoneData) -> Vec<&RecipeData> {
    let mut list: Vec<&RecipeData> = zone.recipes.values().collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

pub(crate) fn build_craft_grid(
    grid: &mut TileGrid,
    zone: &ZoneData,
    run: &RunState,
    clock: &GameClock,
    sel: usize,
    message: &str,
) {
    grid.clear();
    title(grid, "THE BENCH");
    let list = sorted_recipes(zone);

    let shown = window(sel, list.len(), LIST_ROWS);
    for (i, recipe) in list[shown.clone()].iter().enumerate() {
        let y = LIST_ROW + i;
        let idx = shown.start + i;
        let (fg, marker) = if idx == sel {
            (PALETTE.menu_sel, "> ")
        } else if craft_available(zone, run, recipe).is_none() {
            (PALETTE.menu, "  ")
        } else {
            (PALETTE.grey, "  ")
        };
        grid.text(0, y, marker, fg, false);
        grid.text(2, y, &craft_line(zone, recipe), fg, false);
    }
    more(grid, LIST_ROW + LIST_ROWS, &shown, list.len());

    if let Some(recipe) = list.get(sel) {
        grid.text(2, DETAIL_ROW, &craft_detail(zone, recipe), PALETTE.desc, false);
        if let Some(reason) = craft_available(zone, run, recipe) {
            grid.text(2, DETAIL_ROW + 1, reason, PALETTE.red, false);
        }
    }

    draw_message(grid, message);
    draw_chrome(grid, run, clock);
}

// ---- tests (SPEC §7) ----

#[cfg(test)]
mod tests {
    use super::*;
    use crate::area::load_zone;
    use std::path::Path;

    fn fixture() -> (ZoneData, RunState) {
        let zone = load_zone(Path::new("assets/data"));
        let run = RunState::roll(0, &[4, 5, 7], &mut Rng::new(1)); // Medicine, Repair, Science
        (zone, run)
    }

    #[test]
    fn a_recipe_beats_buying_but_is_not_free() {
        let zone = load_zone(Path::new("assets/data"));
        let mut seen = 0;
        for (id, recipe) in &zone.recipes {
            let Some((out, _)) = &recipe.output else { continue };
            seen += 1;
            let cost: u32 = recipe.inputs.iter().map(|(i, n)| zone.items[i].base * n).sum();
            assert!(
                cost < zone.items[out].base,
                "recipe '{id}' costs {cost} to make {}, so selling beats crafting",
                zone.items[out].base
            );
        }
        assert!(seen >= 1, "at least one bench recipe");
    }

    #[test]
    fn the_bench_spends_parts_and_yields_by_the_roll() {
        let (zone, base) = fixture();
        let mut base = base;
        base.add_item("dog_tail", 1);
        base.add_item("vodka", 1);
        let recipe = &zone.recipes["brew_antirad"];

        let mut run = base.clone();
        bench(recipe, &mut run, &zone, Outcome::CritSuccess);
        assert_eq!(run.count_of("antirad"), 2, "a crit makes two");

        let mut run = base.clone();
        bench(recipe, &mut run, &zone, Outcome::Success);
        assert_eq!(run.count_of("antirad"), 1);

        let mut run = base.clone();
        bench(recipe, &mut run, &zone, Outcome::Fail);
        assert_eq!(run.count_of("antirad"), 0, "a fail wastes the parts");

        let mut run = base.clone();
        bench(recipe, &mut run, &zone, Outcome::CritFail);
        assert_eq!(run.count_of("antirad"), 0);
        assert_eq!(run.rads, 15, "a crit fail bites");
    }

    #[test]
    fn the_forge_bakes_an_affix_or_bites_back() {
        let (zone, mut base) = fixture();
        base.add_item("vest", 1);
        base.armor = base.items.iter().find(|s| s.id == "vest").map(|s| s.uid);
        base.add_item("gravi", 1);
        let recipe = &zone.recipes["cook_whirligig"];

        let mut rng = Rng::new(2);
        let mut run = base.clone();
        forge(recipe, &mut run, &zone, Outcome::Success, &mut rng);
        assert_eq!(run.count_of("gravi"), 0, "the catalyst is spent");
        let vest = run.items.iter().find(|s| s.id == "vest").unwrap();
        assert!(vest.affixes.iter().any(|r| r.affix == "of_the_whirligig"));

        let mut run = base.clone();
        forge(recipe, &mut run, &zone, Outcome::CritFail, &mut rng);
        let vest = run.items.iter().find(|s| s.id == "vest").unwrap();
        assert!(vest.affixes.iter().any(|r| r.affix == "hot"), "a crit fail curses");

        let mut run = base.clone();
        forge(recipe, &mut run, &zone, Outcome::Fail, &mut rng);
        let vest = run.items.iter().find(|s| s.id == "vest").unwrap();
        assert!(vest.affixes.is_empty(), "a fail leaves the gear unchanged");
    }
}
