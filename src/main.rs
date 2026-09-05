//! The Zone — M0 renderer spike.
//! A Bevy window blitting a colored ASCII `TileGrid` (the renderer seam) with
//! bold-glyph support, an arrow-key menu, and a hidden-letter area transition.

use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::text::LineBreak;
use bevy::window::{PresentMode, WindowResolution};

// ---------- renderer seam: the frame's glyph buffer ----------

#[derive(Clone, Copy)]
struct Glyph {
    ch: char,
    fg: Color,
    bold: bool,
}

#[derive(Resource)]
struct TileGrid {
    w: usize,
    cells: Vec<Glyph>,
}

impl TileGrid {
    fn new(w: usize, h: usize) -> Self {
        let blank = Glyph {
            ch: ' ',
            fg: Color::srgb(0.25, 0.3, 0.25),
            bold: false,
        };
        Self {
            w,
            cells: vec![blank; w * h],
        }
    }

    fn set(&mut self, x: usize, y: usize, g: Glyph) {
        if x < self.w && y * self.w + x < self.cells.len() {
            self.cells[y * self.w + x] = g;
        }
    }
}

#[derive(Resource, Default)]
struct MenuSelection(usize);

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum AreaId {
    #[default]
    Camp,
    Hatch,
}

#[derive(Resource, Default)]
struct CurrentArea(AreaId);

#[derive(Component)]
struct AsciiScreen;

const GRID_W: usize = 52;
const GRID_H: usize = 24;

const CAMP_MENU: &[&str] = &["Travel", "Look", "Talk", "Inventory"];
const HATCH_MENU: &[&str] = &["Search", "Rest", "Return"];

const CAMP_ART: &[&str] = &[
    "              .     *",
    "      .      ( )        *",
    "          ( moon )    .",
    "    .        .     .",
    "",
    "        /\\            /\\",
    "       /  \\          /  \\",
    "      /    \\        /    \\",
    "     /      \\      /      \\",
    "    ~~~    ~~~    ~~~    ~~~",
    "",
    "              ^",
    "             ^^^",
    "              |",
    "            [D]",
];

const CAMP_DESC: &[&str] = &[
    "A camp on the Zone's edge. A fire pops,",
    "a lantern gutters. Something glints below.",
];

const HATCH_ART: &[&str] = &[
    "      ______________________",
    "     /                      \\",
    "    |   A cramped bunker.    |",
    "    |   Shelves, a cot,      |",
    "    |   a radio hisses.      |",
    "    |                        |",
    "    |    (  )   [  ]  [  ]   |",
    "    |    radio   shelves      |",
    "    |                        |",
    "    |        . . .           |",
    "    |       . glint .        |",
    "    |        . . .           |",
    "    |________________________|",
    "",
    "    ~ dust motes in the air ~",
];

const HATCH_DESC: &[&str] = &[
    "A cramped bunker beneath the camp.",
    "A radio hisses. Something glints on a shelf.",
];

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "The Zone — M0".into(),
                resolution: WindowResolution::new(1280, 720).with_scale_factor_override(1.0),
                present_mode: PresentMode::AutoVsync,
                ..default()
            }),
            ..default()
        }))
        .init_resource::<MenuSelection>()
        .init_resource::<CurrentArea>()
        .add_systems(Startup, setup)
        .add_systems(Update, (menu_input, render_grid).chain())
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let mut grid = TileGrid::new(GRID_W, GRID_H);
    build_grid(&mut grid, 0, AreaId::Camp);
    commands.insert_resource(grid);
}

fn menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut sel: ResMut<MenuSelection>,
    mut area: ResMut<CurrentArea>,
    mut grid: ResMut<TileGrid>,
) {
    let menu = scene(area.0).2;
    let n = menu.len();
    let mut changed = false;

    if keys.just_pressed(KeyCode::ArrowUp) {
        sel.0 = (sel.0 + n - 1) % n;
        changed = true;
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        sel.0 = (sel.0 + 1) % n;
        changed = true;
    }
    if keys.just_pressed(KeyCode::Enter) {
        if area.0 == AreaId::Hatch && menu[sel.0] == "Return" {
            area.0 = AreaId::Camp;
            sel.0 = 0;
            info!("You climb back out to the camp.");
        } else {
            info!("selected: {}", menu[sel.0]);
        }
        changed = true;
    }

    // Hidden letter: type it to immediately enter the secret area.
    if area.0 == AreaId::Camp && keys.just_pressed(KeyCode::KeyD) {
        area.0 = AreaId::Hatch;
        sel.0 = 0;
        info!("You pry open the hatch and climb down.");
        changed = true;
    }

    if changed {
        build_grid(&mut grid, sel.0, area.0);
    }
}

fn render_grid(
    mut commands: Commands,
    grid: Res<TileGrid>,
    screens: Query<Entity, With<AsciiScreen>>,
) {
    if !grid.is_changed() {
        return;
    }
    for e in &screens {
        commands.entity(e).despawn(); // recursive: also drops glyph spans
    }
    spawn_screen(&mut commands, &grid);
}

fn spawn_screen(commands: &mut Commands, grid: &TileGrid) {
    let font = TextFont {
        font_size: FontSize::Px(20.0),
        ..default()
    };
    let layout = TextLayout::new(Justify::Left, LineBreak::NoWrap);

    commands
        .spawn((
            Text2d::new(""),
            font.clone(),
            layout,
            Anchor::TOP_LEFT,
            Transform::from_translation(Vec3::new(-620.0, 345.0, 0.0)),
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

// (art, description, menu, hidden-letter char) for an area.
fn scene(area: AreaId) -> (&'static [&'static str], &'static [&'static str], &'static [&'static str], Option<char>) {
    match area {
        AreaId::Camp => (CAMP_ART, CAMP_DESC, CAMP_MENU, Some('D')),
        AreaId::Hatch => (HATCH_ART, HATCH_DESC, HATCH_MENU, None),
    }
}

fn build_grid(grid: &mut TileGrid, sel: usize, area: AreaId) {
    let dim = Color::srgb(0.30, 0.36, 0.30);
    let ground = Color::srgb(0.55, 0.70, 0.55);
    let pale = Color::srgb(0.82, 0.82, 0.70);
    let fire = Color::srgb(1.00, 0.55, 0.20);
    let smoke = Color::srgb(0.55, 0.55, 0.55);
    let secret = Color::srgb(0.20, 1.00, 1.00);
    let desc_col = Color::srgb(0.72, 0.72, 0.62);
    let menu = Color::srgb(0.70, 0.75, 0.70);
    let menu_sel = Color::srgb(1.00, 0.95, 0.45);

    for c in grid.cells.iter_mut() {
        *c = Glyph {
            ch: ' ',
            fg: dim,
            bold: false,
        };
    }

    let (art, desc, menu_items, secret_ch) = scene(area);

    // Main window: an ASCII-art scene of the area (no positional map).
    for (y, line) in art.iter().enumerate() {
        for (x, ch) in line.chars().enumerate() {
            let mut g = Glyph {
                ch,
                fg: ground,
                bold: false,
            };
            if secret_ch == Some(ch) {
                g.fg = secret;
                g.bold = true;
            } else {
                match ch {
                    ' ' => g.fg = dim,
                    '*' | '(' | ')' => g.fg = pale,
                    '^' => g.fg = fire,
                    '~' => g.fg = smoke,
                    _ => {}
                }
            }
            grid.set(x, y, g);
        }
    }

    // Area description, below the art.
    let desc_y = art.len() + 1;
    for (dy, line) in desc.iter().enumerate() {
        for (x, ch) in line.chars().enumerate() {
            grid.set(x, desc_y + dy, Glyph { ch, fg: desc_col, bold: false });
        }
    }

    // Contextual menu, below the description.
    let menu_y = desc_y + desc.len() + 1;
    for (i, label) in menu_items.iter().enumerate() {
        let y = menu_y + i;
        let fg = if i == sel { menu_sel } else { menu };
        let prefix = if i == sel { "> " } else { "  " };
        for (dx, ch) in prefix.chars().enumerate() {
            grid.set(dx, y, Glyph { ch, fg, bold: false });
        }
        for (dx, ch) in label.chars().enumerate() {
            grid.set(2 + dx, y, Glyph { ch, fg, bold: false });
        }
    }

    let hint = "[arrows] select  [enter] confirm";
    for (x, ch) in hint.chars().enumerate() {
        grid.set(x, GRID_H - 1, Glyph { ch, fg: menu, bold: false });
    }
}
