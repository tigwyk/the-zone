//! Renderer seam: the glyph buffer, palette, and the single text-pass renderer.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::Anchor;
use bevy::text::LineBreak;

// ---------- palette (SPEC §4) ----------

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
    /// Presentation-only: the magenta fringe the glitch effect paints over the HUD.
    pub glitch: Color,
    pub red: Color,
    pub blood: Color,
    pub acid: Color,
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
    glitch: Color::srgb(1.00, 0.30, 0.90),
    water: Color::srgb(0.28, 0.52, 0.72),
    // Red is damage and warning. Grey is dead.
    red: Color::srgb(1.00, 0.32, 0.26),
    // Dried blood, distinct from the salmon `red` used for live warnings.
    blood: Color::srgb(0.85, 0.05, 0.06),
    // Acid: the green of something that eats the floor it sits on.
    acid: Color::srgb(0.50, 1.00, 0.12),
    smoke: Color::srgb(0.52, 0.54, 0.52),
    grey: Color::srgb(0.46, 0.48, 0.46),
};

// Fixed grid (SPEC §4). 120 columns makes the grid native 16:9 with 33 rows.
pub(crate) const GRID_W: usize = 120;
pub(crate) const GRID_H: usize = 33;

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

/// How hard the Zone is interfering with the display right now: 0 is a clean
/// screen, 1 is almost unreadable. Set by game state (an anomaly field glitches the
/// HUD) and read by `render_grid`, so the corruption never touches the `TileGrid`
/// that game logic and the headless tests read.
#[derive(Resource)]
pub(crate) struct Glitch {
    pub level: f32,
    last_tick: u64,
}

impl Default for Glitch {
    fn default() -> Self {
        Glitch { level: 0.0, last_tick: u64::MAX }
    }
}

/// ASCII that reads as "broken" when it lands in the middle of a word.
const GLITCH_CHARS: &[u8] = b"#%&@*+=/\\|<>!?~^;:.,";

/// A deterministic 0..1 per cell, so the same (x, y, tick) always glitches the same
/// way, and the whole screen re-rolls together when `tick` advances.
fn glitch_noise(x: usize, y: usize, tick: u64) -> f32 {
    let mut h = tick
        ^ (x as u64).wrapping_mul(0x9E3779B97F4A7C15)
        ^ (y as u64).wrapping_mul(0xC2B2AE3D27D4EB4F);
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D049BB133111EB);
    h ^= h >> 31;
    (h & 0xFFFFFF) as f32 / 16777216.0
}

/// Corrupts one glyph: swaps in a broken letter, walks the letter one step over
/// (bit rot), or repaints it a wrong colour. Bold glyphs — secrets and titles — are
/// left alone, so a revealed letter stays recognisably bold cyan.
fn glitch_glyph(g: Glyph, level: f32, noise: f32) -> Glyph {
    if g.bold || noise >= level {
        return g;
    }
    let pick = noise / level; // 0..1 within the corrupted band
    let mut out = g;
    if pick < 0.6 {
        let chars = GLITCH_CHARS;
        out.ch = chars[(pick * chars.len() as f32) as usize] as char;
    } else if pick < 0.8 {
        out.ch = shift_char(g.ch);
    } else {
        let colors = [PALETTE.glitch, PALETTE.cyan, PALETTE.red, PALETTE.amber];
        out.fg = colors[(pick * colors.len() as f32) as usize];
    }
    out
}

/// One-codepoint "bit rot": the letter walks one step right in ASCII.
fn shift_char(ch: char) -> char {
    let c = ch as u32;
    if (0x21..0x7E).contains(&c) {
        char::from_u32(c + 1).unwrap_or(ch)
    } else {
        ch
    }
}

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
    mut glitch: ResMut<Glitch>,
    time: Res<Time>,
    screens: Query<Entity, With<AsciiScreen>>,
    windows: Query<&Window>,
) {
    // Redraw when the game logic did, or on a glitch tick so the corruption
    // flickers while the level is above zero (a clean screen never re-renders).
    let tick = (time.elapsed_secs() * 6.0) as u64;
    if !grid.is_changed() && (glitch.level <= 0.0 || tick == glitch.last_tick) {
        return;
    }
    glitch.last_tick = tick;
    for e in &screens {
        commands.entity(e).despawn(); // recursive: also drops glyph spans
    }
    let win = windows.single().expect("primary window");
    spawn_screen(&mut commands, &grid, win, glitch.level, tick);
}

fn spawn_screen(commands: &mut Commands, grid: &TileGrid, win: &Window, level: f32, tick: u64) {
    // The default font (Fira Mono) is monospace with a 0.6-em advance and Bevy's
    // default 1.2-em line height. Scale the font so the 120×33 grid fills the window
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
                    let cell = grid.cells[y * grid.w + x];
                    let g = glitch_glyph(cell, level, glitch_noise(x, y, tick));
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

    #[test]
    fn the_glitch_is_off_when_calm_and_never_touches_bold() {
        let plain = Glyph { ch: 'A', fg: PALETTE.ground, bold: false };

        // No glitch: every cell passes through untouched, whatever the noise.
        assert_eq!(glitch_glyph(plain, 0.0, 0.0).ch, 'A');
        assert_eq!(glitch_glyph(plain, 0.0, 0.5).fg, PALETTE.ground);

        // A bold glyph (a secret or a title) is never corrupted, even at full tilt.
        let secret = Glyph { ch: 'D', fg: PALETTE.secret, bold: true };
        let out = glitch_glyph(secret, 1.0, 0.0);
        assert_eq!(out.ch, 'D');
        assert_eq!(out.fg, PALETTE.secret);

        // At full glitch a non-bold cell under the level comes out broken or recoloured.
        let glitched = glitch_glyph(plain, 1.0, 0.1);
        assert!(
            glitched.ch != 'A' || glitched.fg != PALETTE.ground,
            "a full-glitch cell must change"
        );

        // Deterministic: the same (x, y, tick) reads the same, and the noise stays in range.
        assert_eq!(glitch_noise(7, 9, 3), glitch_noise(7, 9, 3));
        assert!((0.0..1.0).contains(&glitch_noise(7, 9, 3)));
    }
}
