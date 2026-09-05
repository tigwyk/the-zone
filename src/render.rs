//! Renderer seam: the glyph buffer, palette, and the single text-pass renderer.

use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::text::LineBreak;

// ---------- palette (SPEC §4) ----------

// water/amber/cyan/red/grey/pale are referenced by later content (anomalies, artifacts).
#[allow(dead_code)]
pub(crate) struct Palette {
    pub dim: Color,
    pub ground: Color,
    pub pale: Color,
    pub fire: Color,
    pub smoke: Color,
    pub water: Color,
    pub amber: Color,
    pub cyan: Color,
    pub secret: Color,
    pub red: Color,
    pub grey: Color,
    pub desc: Color,
    pub menu: Color,
    pub menu_sel: Color,
    pub status: Color,
}

pub(crate) const PALETTE: Palette = Palette {
    dim: Color::srgb(0.30, 0.36, 0.30),
    ground: Color::srgb(0.55, 0.70, 0.55),
    pale: Color::srgb(0.82, 0.82, 0.70),
    fire: Color::srgb(1.00, 0.55, 0.20),
    smoke: Color::srgb(0.55, 0.55, 0.55),
    water: Color::srgb(0.30, 0.50, 0.70),
    amber: Color::srgb(1.00, 0.75, 0.20),
    cyan: Color::srgb(0.30, 0.90, 0.90),
    secret: Color::srgb(0.20, 1.00, 1.00),
    red: Color::srgb(1.00, 0.30, 0.25),
    grey: Color::srgb(0.50, 0.50, 0.50),
    desc: Color::srgb(0.72, 0.72, 0.62),
    menu: Color::srgb(0.70, 0.75, 0.70),
    menu_sel: Color::srgb(1.00, 0.95, 0.45),
    status: Color::srgb(0.65, 0.75, 0.65),
};

// Fixed grid (SPEC §4).
pub(crate) const GRID_W: usize = 80;
pub(crate) const GRID_H: usize = 30;

#[derive(Clone, Copy)]
pub(crate) struct Glyph {
    pub ch: char,
    pub fg: Color,
    pub bold: bool,
}

#[derive(Resource)]
pub(crate) struct TileGrid {
    pub w: usize,
    pub cells: Vec<Glyph>,
}

impl TileGrid {
    pub fn new(w: usize, h: usize) -> Self {
        let blank = Glyph {
            ch: ' ',
            fg: PALETTE.dim,
            bold: false,
        };
        Self {
            w,
            cells: vec![blank; w * h],
        }
    }

    pub fn set(&mut self, x: usize, y: usize, g: Glyph) {
        if x < self.w && y * self.w + x < self.cells.len() {
            self.cells[y * self.w + x] = g;
        }
    }
}

impl FromWorld for TileGrid {
    fn from_world(_world: &mut World) -> Self {
        TileGrid::new(GRID_W, GRID_H)
    }
}

#[derive(Component)]
pub(crate) struct AsciiScreen;

pub(crate) fn render_grid(
    mut commands: Commands,
    grid: Res<TileGrid>,
    screens: Query<Entity, With<AsciiScreen>>,
    windows: Query<&Window>,
) {
    if !grid.is_changed() {
        return;
    }
    for e in &screens {
        commands.entity(e).despawn(); // recursive: also drops glyph spans
    }
    let win = windows.single().expect("primary window");
    spawn_screen(&mut commands, &grid, win);
}

fn spawn_screen(commands: &mut Commands, grid: &TileGrid, win: &Window) {
    let font = TextFont {
        font_size: FontSize::Px(16.0),
        ..default()
    };
    let layout = TextLayout::new(Justify::Left, LineBreak::NoWrap);

    // Top-left origin derived from the window size (SPEC §4), one glyph of margin.
    let margin = 16.0;
    let origin = Vec3::new(
        -win.width() / 2.0 + margin,
        win.height() / 2.0 - margin,
        0.0,
    );

    commands
        .spawn((
            Text2d::new(""),
            font.clone(),
            layout,
            Anchor::TOP_LEFT,
            Transform::from_translation(origin),
            AsciiScreen,
        ))
        .with_children(|parent| {
            let rows = grid.cells.len() / grid.w;
            for y in 0..rows {
                for x in 0..grid.w {
                    let g = grid.cells[y * grid.w + x];
                    let color = if g.bold { bold_color(g.fg) } else { g.fg };
                    parent.spawn((TextSpan::new(g.ch.to_string()), font.clone(), TextColor(color)));
                }
                if y + 1 < rows {
                    parent.spawn((TextSpan::new("\n"), font.clone()));
                }
            }
        });
}

// ponytail: ANSI-style "bold = bright". Swap for a real bold font asset when
// the hidden-letter mechanic needs true weight, not just brightness.
fn bold_color(c: Color) -> Color {
    let s = c.to_srgba();
    Color::srgb(s.red * 0.4 + 0.6, s.green * 0.4 + 0.6, s.blue * 0.4 + 0.6)
}
