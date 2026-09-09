//! Liquids on the floor: water, blood, acid. Authored per-area in `zone.ron` and
//! spilled by fights. Same-kind liquids pool into one puddle, which the area screen
//! reads out in the description section, in the liquid's own colour (GDD §11).

use std::collections::HashMap;

use bevy::prelude::*;

use crate::area::ZoneData;
use crate::render::{TileGrid, GRID_W, PALETTE};

/// The liquids that can sit on a floor. Kept in a fixed order so the readout is
/// stable no matter the order they were spilled in.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Liquid {
    Water,
    Blood,
    Acid,
}

impl Liquid {
    pub(crate) const ALL: [Liquid; 3] = [Liquid::Water, Liquid::Blood, Liquid::Acid];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Liquid::Water => "water",
            Liquid::Blood => "blood",
            Liquid::Acid => "acid",
        }
    }

    pub(crate) fn from_id(id: &str) -> Liquid {
        match id {
            "water" => Liquid::Water,
            "blood" => Liquid::Blood,
            "acid" => Liquid::Acid,
            _ => panic!("unknown liquid '{id}'"),
        }
    }

    pub(crate) fn color(self) -> Color {
        match self {
            Liquid::Water => PALETTE.water,
            Liquid::Blood => PALETTE.blood,
            Liquid::Acid => PALETTE.acid,
        }
    }

    /// The phrase for a pooled amount, sized so a bigger puddle reads bigger.
    pub(crate) fn label(self, amount: u32) -> String {
        let noun = self.id();
        let size = match amount {
            1..=4 => match self {
                Liquid::Water => "A slick of",
                Liquid::Blood => "A smear of",
                Liquid::Acid => "A splash of",
            },
            5..=19 => "A puddle of",
            20..=59 => "A pool of",
            _ => "A wide pool of",
        };
        format!("{size} {noun}")
    }
}

/// Run-scoped: how much of each liquid sits on each area's floor. Authored amounts
/// seed it at startup; combat spills into it; both survive a suspend with the run.
#[derive(Resource)]
pub(crate) struct Puddles(pub HashMap<String, HashMap<String, u32>>);

impl Puddles {
    /// An empty floor, for tests. The game seeds from `zone.ron` via `FromWorld`.
    pub(crate) fn empty() -> Self {
        Puddles(HashMap::new())
    }

    /// Authored liquids seed the run; combat pools into them from here on. A new
    /// stalker starts on the authored floor, not the last one's blood.
    pub(crate) fn seed(zone: &ZoneData) -> Self {
        let mut puddles = Puddles::empty();
        for (id, area) in &zone.areas {
            for (liquid, amount) in &area.liquids {
                spill(&mut puddles, id, Liquid::from_id(liquid), *amount);
            }
        }
        puddles
    }
}

impl FromWorld for Puddles {
    fn from_world(world: &mut World) -> Self {
        let zone = world.resource::<ZoneData>();
        Puddles::seed(zone)
    }
}

/// Drops `amount` of one liquid on an area's floor, pooling it with whatever of the
/// same kind is already there — one bigger puddle, not two smaller ones.
pub(crate) fn spill(puddles: &mut Puddles, area: &str, liquid: Liquid, amount: u32) {
    *puddles
        .0
        .entry(area.to_string())
        .or_default()
        .entry(liquid.id().to_string())
        .or_insert(0) += amount;
}

/// What is pooled in `area`, in `Liquid::ALL` order, zero amounts dropped.
pub(crate) fn pooled(puddles: &Puddles, area: &str) -> Vec<(Liquid, u32)> {
    let Some(here) = puddles.0.get(area) else {
        return Vec::new();
    };
    Liquid::ALL
        .iter()
        .filter_map(|&l| here.get(l.id()).filter(|&&n| n > 0).map(|&n| (l, n)))
        .collect()
}

/// Row 21, the blank below the description (SPEC §4 keeps 18 and 21 clear around
/// it), so the readout sits inside the description block without touching the menu.
const LIQUID_ROW: usize = 21;

/// Draws one centred line naming each pooled liquid, each in its own colour.
pub(crate) fn draw_liquids(grid: &mut TileGrid, puddles: &Puddles, area: &str) {
    let here = pooled(puddles, area);
    if here.is_empty() {
        return;
    }
    let labels: Vec<(String, Color)> = here.iter().map(|&(l, n)| (l.label(n), l.color())).collect();
    // Two spaces between entries, then centre the whole run like the description.
    let total: usize = labels.iter().map(|(s, _)| s.chars().count()).sum::<usize>()
        + 2 * (labels.len() - 1);
    let mut x = GRID_W.saturating_sub(total) / 2;
    for (s, c) in &labels {
        grid.text(x, LIQUID_ROW, s, *c, false);
        x += s.chars().count() + 2;
    }
}

// ---- tests (SPEC §7) ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_liquid_pools_and_different_liquids_stay_apart() {
        let mut p = Puddles::empty();
        spill(&mut p, "quarry", Liquid::Blood, 3);
        spill(&mut p, "quarry", Liquid::Blood, 26);
        spill(&mut p, "quarry", Liquid::Water, 5);
        assert_eq!(
            pooled(&p, "quarry"),
            vec![(Liquid::Water, 5), (Liquid::Blood, 29)]
        );
        assert_eq!(pooled(&p, "camp"), Vec::new());
    }

    #[test]
    fn a_bigger_puddle_reads_bigger() {
        assert_eq!(Liquid::Blood.label(3), "A smear of blood");
        assert_eq!(Liquid::Blood.label(26), "A pool of blood");
        assert_eq!(Liquid::Blood.label(120), "A wide pool of blood");
        assert_eq!(Liquid::Acid.label(3), "A splash of acid");
        assert_eq!(Liquid::Water.label(45), "A pool of water");
    }

    #[test]
    fn the_readout_names_each_pooled_liquid() {
        let mut grid = TileGrid::new(crate::render::GRID_W, crate::render::GRID_H);
        let mut p = Puddles::empty();
        spill(&mut p, "swamp", Liquid::Water, 45);
        spill(&mut p, "swamp", Liquid::Acid, 55);
        draw_liquids(&mut grid, &p, "swamp");
        let row: String = (0..crate::render::GRID_W)
            .map(|x| grid.cells[LIQUID_ROW * crate::render::GRID_W + x].ch)
            .collect();
        assert!(row.contains("A pool of water"), "{row}");
        assert!(row.contains("A pool of acid"), "{row}");
    }
}
