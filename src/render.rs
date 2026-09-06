//! Renderer seam: the glyph buffer, palette, and the single text-pass renderer.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::Anchor;
use bevy::text::LineBreak;

// ---------- palette (SPEC §4) ----------

// `water` is the one name nothing reaches yet: no area art uses a water class.
pub(crate) struct Palette {
    pub dim: Color,
    pub ground: Color,
    pub pale: Color,
    pub fire: Color,
    pub smoke: Color,
    #[allow(dead_code)] // no area art declares a water class yet
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

// A phosphor screen with things burned onto it. Green is the ground state and
// everything else is an interruption, so the accents run warm and stay rare
// (GDD §11). Every colour here is used; nothing is kept "for later".
pub(crate) const PALETTE: Palette = Palette {
    // Green: ground, living, the base state of the screen.
    dim: Color::srgb(0.26, 0.33, 0.27),
    ground: Color::srgb(0.52, 0.72, 0.52),
    status: Color::srgb(0.62, 0.78, 0.62),
    menu: Color::srgb(0.68, 0.78, 0.68),
    // Off-white: prose, which has to read first.
    desc: Color::srgb(0.76, 0.78, 0.68),
    pale: Color::srgb(0.88, 0.88, 0.76),
    // Warm: what is on fire, what is anomalous, what you have picked.
    fire: Color::srgb(1.00, 0.52, 0.16),
    amber: Color::srgb(1.00, 0.74, 0.18),
    menu_sel: Color::srgb(1.00, 0.94, 0.42),
    // Cyan: artifacts. Bold cyan is a secret and is nothing else (GDD §11).
    cyan: Color::srgb(0.32, 0.88, 0.90),
    secret: Color::srgb(0.16, 1.00, 1.00),
    water: Color::srgb(0.28, 0.52, 0.72),
    // Red is damage and warning. Grey is dead.
    red: Color::srgb(1.00, 0.32, 0.26),
    smoke: Color::srgb(0.52, 0.54, 0.52),
    grey: Color::srgb(0.46, 0.48, 0.46),
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

    /// Writes a string starting at (x, y). Off-grid characters are dropped by `set`.
    pub fn text(&mut self, x: usize, y: usize, s: &str, fg: Color, bold: bool) {
        for (i, ch) in s.chars().enumerate() {
            self.set(x + i, y, Glyph { ch, fg, bold });
        }
    }

    pub fn clear(&mut self) {
        let blank = Glyph {
            ch: ' ',
            fg: PALETTE.dim,
            bold: false,
        };
        self.cells.fill(blank);
    }
}

impl FromWorld for TileGrid {
    fn from_world(_world: &mut World) -> Self {
        TileGrid::new(GRID_W, GRID_H)
    }
}

#[derive(Component)]
pub(crate) struct AsciiScreen;

#[derive(Component)]
pub(crate) struct Scanlines;

/// Every third row of the display, darkened. `ponytail:` an overlay sprite, not a
/// post-process shader - it buys the phosphor feel for twenty lines and no WGSL.
/// Curvature, bloom and chromatic aberration would need the real thing.
const SCANLINE_PERIOD: usize = 3;
const SCANLINE_ALPHA: u8 = 30;

/// One pixel wide and as tall as the display: the rows are uniform, so the sprite
/// stretches it across. Every `SCANLINE_PERIOD`-th row carries the darkening.
fn scanline_pixels(height: u32) -> Vec<u8> {
    let mut data = vec![0u8; height as usize * 4];
    for row in 0..height as usize {
        if row % SCANLINE_PERIOD == 0 {
            data[row * 4 + 3] = SCANLINE_ALPHA;
        }
    }
    data
}

/// Draws the CRT overlay over the text. F4 turns it off, because readability beats
/// decoration (GDD §2) and this is the one thing here that can cost some.
pub(crate) fn spawn_scanlines(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window>,
) {
    let Ok(win) = windows.single() else {
        return;
    };
    let height = win.height().max(1.0) as u32;
    let mut image = Image::new(
        Extent3d { width: 1, height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        scanline_pixels(height),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    // Nearest, or the lines blur into a flat tint.
    image.sampler = ImageSampler::nearest();

    commands.spawn((
        Sprite {
            image: images.add(image),
            custom_size: Some(Vec2::new(win.width(), win.height())),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 10.0),
        Scanlines,
    ));
}

pub(crate) fn toggle_scanlines(
    keys: Res<ButtonInput<KeyCode>>,
    mut scanlines: Query<&mut Visibility, With<Scanlines>>,
) {
    if !keys.just_pressed(KeyCode::F4) {
        return;
    }
    for mut visibility in &mut scanlines {
        *visibility = match *visibility {
            Visibility::Hidden => Visibility::Inherited,
            _ => Visibility::Hidden,
        };
    }
}

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
    // The default font (Fira Mono) is monospace with a 0.6-em advance and Bevy's
    // default 1.2-em line height. Scale the font so the 80×30 grid fills the window
    // with one even margin, and centre the block (SPEC §4: the origin comes from the
    // window size and the font's advance).
    let margin = 16.0;
    let em_w = GRID_W as f32 * 0.6; // grid width in em
    let em_h = GRID_H as f32 * 1.2; // grid height in em
    let font_size = ((win.width() - 2.0 * margin) / em_w)
        .min((win.height() - 2.0 * margin) / em_h);

    let font = TextFont {
        font_size: FontSize::Px(font_size),
        ..default()
    };
    let layout = TextLayout::new(Justify::Left, LineBreak::NoWrap);

    // Anchor::TOP_LEFT pins the block's top-left corner to the translation and the
    // text runs right (+x) and down (−y), so this centres the grid.
    let origin = Vec3::new(
        -font_size * em_w / 2.0,
        font_size * em_h / 2.0,
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
            // Merge consecutive same-color glyphs into runs: per-cell spans made a full
            // redraw despawn ~2,400 entities and re-layout them all, lagging keypresses.
            let rows = grid.cells.len() / grid.w;
            let mut run = String::new();
            let mut cur: Option<Color> = None;
            for y in 0..rows {
                for x in 0..grid.w {
                    let g = grid.cells[y * grid.w + x];
                    let color = if g.bold { bold_color(g.fg) } else { g.fg };
                    if cur != Some(color) {
                        if let Some(c) = cur {
                            parent.spawn((
                                TextSpan::new(std::mem::take(&mut run)),
                                font.clone(),
                                TextColor(c),
                            ));
                        }
                        cur = Some(color);
                    }
                    run.push(g.ch);
                }
                if y + 1 < rows {
                    run.push('\n');
                }
            }
            if let Some(c) = cur {
                parent.spawn((TextSpan::new(run), font.clone(), TextColor(c)));
            }
        });
}

// ponytail: ANSI-style "bold = bright". Swap for a real bold font asset when
// the hidden-letter mechanic needs true weight, not just brightness.
fn bold_color(c: Color) -> Color {
    let s = c.to_srgba();
    Color::srgb(s.red * 0.4 + 0.6, s.green * 0.4 + 0.6, s.blue * 0.4 + 0.6)
}

// ---- tests (SPEC §7) ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scanline_overlay_darkens_every_third_row_and_nothing_else() {
        let data = scanline_pixels(9);
        assert_eq!(data.len(), 9 * 4);
        for row in 0..9 {
            let alpha = data[row * 4 + 3];
            let want = if row % SCANLINE_PERIOD == 0 { SCANLINE_ALPHA } else { 0 };
            assert_eq!(alpha, want, "row {row}");
            // Black, so it only ever subtracts light.
            assert_eq!(&data[row * 4..row * 4 + 3], &[0, 0, 0]);
        }
        // Even one row tall it must not panic or run off the buffer.
        assert_eq!(scanline_pixels(1).len(), 4);
    }
}
