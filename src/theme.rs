//! Theme system: dark and light palettes with an Auto mode that follows the
//! OS appearance (which on macOS follows the day/night cycle when the system
//! is set to Auto). Icon and text colors always derive from the active
//! palette, so glyphs compose correctly on both light and dark chrome.

use egui::{Color32, Context, CornerRadius, Margin, Rect, Shadow, Stroke, TextureId, Vec2, pos2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    /// Follow the system appearance.
    Auto,
    Light,
    Dark,
}

/// User-selectable accent color presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccentColor {
    /// A colour the reader mixed themselves. First in the row, and the only
    /// one that opens a picker rather than simply being applied.
    ///
    /// Kept as it was chosen in both themes. The presets carry a pair each,
    /// tuned so they have contrast on dark chrome and on light; second-guessing
    /// somebody's own colour by lightening it in one theme would be worse than
    /// showing them what they picked.
    Custom(u8, u8, u8),
    Lavender,
    Sky,
    Mint,
    Amber,
    Rose,
    Coral,
    Teal,
    Violet,
    Lime,
    Graphite,
}

impl AccentColor {
    /// The fixed ones, in the order they are offered. `Custom` is not here:
    /// it has no colour until somebody picks one, so the settings page draws
    /// it from whatever they last chose.
    pub const PRESETS: [AccentColor; 10] = [
        AccentColor::Lavender,
        AccentColor::Sky,
        AccentColor::Mint,
        AccentColor::Amber,
        AccentColor::Rose,
        AccentColor::Coral,
        AccentColor::Teal,
        AccentColor::Violet,
        AccentColor::Lime,
        AccentColor::Graphite,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            AccentColor::Custom(..) => "Custom",
            AccentColor::Lavender => "Lavender",
            AccentColor::Sky => "Sky",
            AccentColor::Mint => "Mint",
            AccentColor::Amber => "Amber",
            AccentColor::Rose => "Rose",
            AccentColor::Coral => "Coral",
            AccentColor::Teal => "Teal",
            AccentColor::Violet => "Violet",
            AccentColor::Lime => "Lime",
            AccentColor::Graphite => "Graphite",
        }
    }

    /// The three bytes a custom accent is stored as, or the preset's own
    /// colour in the current theme — what the picker opens on.
    pub fn rgb(&self, dark: bool) -> [u8; 3] {
        let color = self.color(dark);
        [color.r(), color.g(), color.b()]
    }

    /// The accent color, tuned per theme so it has contrast on both.
    pub fn color(&self, dark: bool) -> Color32 {
        match (self, dark) {
            (AccentColor::Lavender, true) => Color32::from_rgb(167, 139, 250),
            (AccentColor::Lavender, false) => Color32::from_rgb(114, 84, 220),
            (AccentColor::Sky, true) => Color32::from_rgb(125, 196, 240),
            (AccentColor::Sky, false) => Color32::from_rgb(38, 122, 190),
            (AccentColor::Mint, true) => Color32::from_rgb(122, 214, 160),
            (AccentColor::Mint, false) => Color32::from_rgb(28, 138, 82),
            (AccentColor::Amber, true) => Color32::from_rgb(240, 187, 120),
            (AccentColor::Amber, false) => Color32::from_rgb(178, 108, 24),
            (AccentColor::Rose, true) => Color32::from_rgb(240, 148, 170),
            (AccentColor::Rose, false) => Color32::from_rgb(188, 62, 96),
            (AccentColor::Coral, true) => Color32::from_rgb(244, 152, 128),
            (AccentColor::Coral, false) => Color32::from_rgb(196, 74, 46),
            (AccentColor::Teal, true) => Color32::from_rgb(112, 204, 202),
            (AccentColor::Teal, false) => Color32::from_rgb(20, 124, 124),
            (AccentColor::Violet, true) => Color32::from_rgb(202, 142, 236),
            (AccentColor::Violet, false) => Color32::from_rgb(138, 60, 190),
            (AccentColor::Lime, true) => Color32::from_rgb(190, 214, 118),
            (AccentColor::Lime, false) => Color32::from_rgb(108, 138, 30),
            (AccentColor::Graphite, true) => Color32::from_rgb(172, 178, 188),
            (AccentColor::Graphite, false) => Color32::from_rgb(84, 92, 104),
            (AccentColor::Custom(red, green, blue), _) => Color32::from_rgb(*red, *green, *blue),
        }
    }
}

/// Linear per-channel mix of two opaque colors.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let inv = 1.0 - t;
    Color32::from_rgb(
        (a.r() as f32 * inv + b.r() as f32 * t) as u8,
        (a.g() as f32 * inv + b.g() as f32 * t) as u8,
        (a.b() as f32 * inv + b.b() as f32 * t) as u8,
    )
}

/// The lightest a dark theme's surface may read, and the darkest a light
/// theme's may, once the page behind has come through it.
///
/// Not the middle: a surface that lands exactly halfway belongs to neither
/// theme. These leave it recognisably on its own side while still letting a
/// good deal of the page through.
const DARKEST_LIGHT: f32 = 0.42;
const LIGHTEST_DARK: f32 = 0.60;

/// The most tint a surface may take on to hold its theme. Past this the blur
/// stops showing through and it is not glass any more.
const THICKEST_TINT: f32 = 0.88;

/// How much better the other ink has to read before a panel abandons the
/// theme's own. Pure hysteresis: without it, text flips as the page scrolls.
const FLIP_MARGIN: f32 = 1.3;

/// WCAG contrast ratio between a text colour and a background of the given
/// brightness, 1..=21.
fn contrast(ink: Color32, brightness: f32) -> f32 {
    let linear = |value: f32| {
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let a = linear(f32::from(luminance_of(ink)) / 255.0);
    let b = linear(brightness.clamp(0.0, 1.0));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Text for a dark background, and the muted shade beside it.
///
/// Muted text has to survive being read off a translucent surface with a
/// photograph behind it, which is a harder job than it had when every card was
/// opaque. It is the caption colour on every list row and every explanatory
/// line in Settings, so when it is too dim it is most of the words in the
/// application.
const LIGHT_INK: (Color32, Color32) = (
    Color32::from_rgb(238, 238, 242),
    Color32::from_rgb(182, 182, 192),
);

/// Text for a light background.
const DARK_INK: (Color32, Color32) = (Color32::from_rgb(20, 20, 24), Color32::from_rgb(78, 78, 88));

/// How many cells across the backdrop's luminance map is.
pub const LUMA_CELLS: usize = 8;

/// Reduce a picture to that map.
pub fn luma_map(image: &egui::ColorImage) -> [u8; LUMA_CELLS * LUMA_CELLS] {
    let mut map = [0_u8; LUMA_CELLS * LUMA_CELLS];
    let [width, height] = image.size;
    if width == 0 || height == 0 {
        return map;
    }
    for cell_y in 0..LUMA_CELLS {
        for cell_x in 0..LUMA_CELLS {
            let x0 = width * cell_x / LUMA_CELLS;
            let x1 = (width * (cell_x + 1) / LUMA_CELLS).max(x0 + 1).min(width);
            let y0 = height * cell_y / LUMA_CELLS;
            let y1 = (height * (cell_y + 1) / LUMA_CELLS).max(y0 + 1).min(height);
            let mut total = 0_u32;
            let mut count = 0_u32;
            // Every fourth pixel: this is an eight-by-eight answer and reading
            // every pixel of a two-hundred-pixel picture to produce it is work
            // nobody sees.
            for y in (y0..y1).step_by(2) {
                for x in (x0..x1).step_by(2) {
                    let pixel = image.pixels[y * width + x];
                    total += u32::from(luminance_of(pixel));
                    count += 1;
                }
            }
            map[cell_y * LUMA_CELLS + cell_x] =
                total.checked_div(count).unwrap_or(0).min(255) as u8;
        }
    }
    map
}

/// Rec. 601 luma, which is what everything deciding between black and white
/// text uses.
fn luminance_of(color: Color32) -> u8 {
    (0.299 * f32::from(color.r()) + 0.587 * f32::from(color.g()) + 0.114 * f32::from(color.b()))
        .clamp(0.0, 255.0) as u8
}

/// A blurred picture uploaded for frosting, carrying the luminance map that
/// was taken from it.
///
/// The two travel together because they are only ever right together: a map
/// read from a different picture than the one on screen would pick text
/// colours for a backdrop that is no longer there.
pub struct Frost {
    texture: egui::TextureHandle,
    luma: [u8; LUMA_CELLS * LUMA_CELLS],
}

impl Frost {
    /// Take the map, then hand the picture to the GPU.
    pub fn upload(
        ctx: &egui::Context,
        name: &str,
        image: egui::ColorImage,
        options: egui::TextureOptions,
    ) -> Self {
        let luma = luma_map(&image);
        Self {
            texture: ctx.load_texture(name, image, options),
            luma,
        }
    }

    /// What to draw with.
    pub fn id(&self) -> TextureId {
        self.texture.id()
    }

    /// How light it is, cell by cell.
    pub fn luma(&self) -> [u8; LUMA_CELLS * LUMA_CELLS] {
        self.luma
    }
}

/// A picture behind the chrome, already blurred, for glass surfaces to frost
/// themselves against.
///
/// egui cannot blur what is behind a shape while it draws it, and nothing here
/// needs it to. The only thing ever behind the chrome is a wallpaper, which is
/// a still image — so it is blurred once, when it is decoded, and the material
/// samples that blurred copy through the same mapping the sharp one is drawn
/// with. What comes out is a real backdrop blur rather than an impression of
/// one, and it costs nothing per frame.
#[derive(Clone, Copy)]
pub struct Backdrop {
    /// A blurred copy of the picture. Small: it is blurred, so there is no
    /// detail left in it to be worth storing at size.
    pub texture: TextureId,
    /// Where the sharp picture is drawn, in screen points.
    pub rect: Rect,
    /// The part of the picture that `rect` shows — the same window the sharp
    /// one uses, so the blur underneath a card lines up with the photograph
    /// beside it.
    pub uv: Rect,
    /// How light the picture is, on an eight-by-eight grid, 0..=255.
    ///
    /// Coarse on purpose: it is used to decide whether text over a patch of it
    /// should be light or dark, and that decision does not get better with
    /// resolution. Sixty-four bytes, so a `Palette` stays cheap to copy.
    pub luma: [u8; LUMA_CELLS * LUMA_CELLS],
    /// How far the picture itself has arrived, 0..=1.
    ///
    /// A wallpaper fades in. While it is doing so the blur under a card has to
    /// fade with it, or a card sits on an opaque frost while the photograph
    /// beside it is still half transparent.
    pub alpha: f32,
}

/// How much of what is behind a surface comes through it.
///
/// Three steps rather than a slider. A surface's job is to hold text up, and
/// most of the range between "a card" and "not there" is a surface that has
/// stopped doing that — so the choice offered is between three that all work
/// rather than a hundred that mostly do not.
///
/// Every material honours this unless it says otherwise with
/// `Material::translucency`, so it is one setting across every theme rather
/// than something each one reinvents.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Translucency {
    /// No translucency at all. Surfaces are their own colour and nothing
    /// comes through them.
    Solid,
    /// What Zervo has always looked like, now applied to everything the
    /// material draws rather than to the window alone.
    Frosted,
}

impl Translucency {
    pub const ALL: [Translucency; 2] = [Translucency::Solid, Translucency::Frosted];

    pub fn label(self) -> &'static str {
        match self {
            Translucency::Solid => "Solid",
            Translucency::Frosted => "Frosted",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Translucency::Solid => {
                "Surfaces are their own colour. The most readable, and the least glass."
            },
            Translucency::Frosted => {
                "What is behind shows through — the page, the chrome, the wallpaper's blur."
            },
        }
    }

    /// What the platform's own backdrop should be at this step.
    ///
    /// The tint is only half of it. macOS's `Sidebar` material is dark and
    /// heavy in its own right, so a window using it looks muted however light
    /// the tint over it is — you can tell something is behind the window
    /// without being able to tell what colour it is. Getting the colours
    /// through means asking the system for a clearer backdrop, not painting
    /// less over a murky one.
    pub fn backdrop(self) -> SystemBackdrop {
        match self {
            Translucency::Solid => SystemBackdrop::Opaque,
            Translucency::Frosted => SystemBackdrop::Frosted,
        }
    }

    /// The tint over the window's own chrome, 0..=1.
    ///
    /// Nearly nothing, because the system's blur is doing the work. This was
    /// one number shared with `surface` for a while, on the reasonable-sounding
    /// grounds that the chrome and the cards should be made of the same stuff.
    /// They are — they sit on the same blur and take the same treatment — but
    /// they are not the same *tint*, and making them so left the window too
    /// murky and the cards too faint at the same time.
    ///
    /// The reference here is Zen's Transparent mod, which is what this was
    /// measured against: its main browser background is `#00000000`, flatly
    /// transparent, and every surface that has to hold text sits on top of
    /// that with a tint of its own — 30% for the toolbar, 33% for a panel, 60%
    /// for the URL bar. The base being clear is the whole effect; the surfaces
    /// being tinted is what keeps it readable.
    pub fn chrome(self) -> f32 {
        match self {
            Translucency::Solid => 1.0,
            Translucency::Frosted => 0.08,
        }
    }

    /// The tint on anything the material draws over that chrome — cards,
    /// menus, the shelf, the new tab page.
    ///
    /// Heavier than the chrome, deliberately and for the same reason the
    /// reference is: these are the surfaces with words on them.
    pub fn surface(self) -> f32 {
        match self {
            Translucency::Solid => 1.0,
            Translucency::Frosted => 0.34,
        }
    }

    /// How far a class's own weight is scaled at this step. Solid takes
    /// everything to opaque; Frosted leaves each class as the material wrote
    /// it, so the hierarchy between them survives the setting.
    pub fn scales(self) -> bool {
        self == Translucency::Solid
    }
}

/// How much the platform's own backdrop should let through, for platforms that
/// have one.
///
/// Named for what is wanted rather than for any one platform's constant, so
/// the mapping to `NSVisualEffectMaterial` — or to whatever Windows and Linux
/// turn out to offer — stays where the platform code is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SystemBackdrop {
    /// No point letting anything through; the tint covers it.
    Opaque,
    /// The clearest frost the platform offers — the colours behind the window
    /// survive it.
    Frosted,
}

/// What kind of surface this is — a class, in the sense a stylesheet means it.
///
/// The call site names the role and the material decides what that role is
/// made of: its tint, its corners, how far it lifts off the page. Changing
/// what a menu looks like everywhere is then one line in a material rather
/// than a search through every place that draws one, and a surface added later
/// picks a class instead of picking numbers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Surface {
    /// Cards: settings sections, the shelf's widgets, the new tab page's.
    /// They sit *in* the chrome and are read against it.
    Card,
    /// Things that float over everything — menus, dropdowns, hover cards,
    /// dialogs. Glass, the same as a card: what is behind one of these is the
    /// system's own blurred backdrop, and a tint heavy enough to be safe
    /// against the worst case throws that away everywhere else.
    Menu,
    /// Something you type into. The heaviest, because a text field is the one
    /// place where what is behind it competes with what you are reading.
    Input,
}

/// The corner-radius tier every rounded thing in Zervo picks from.
///
/// Sizes, not numbers. A card is a card whatever the material thinks a card's
/// corners should look like, so a call site names the tier and the material
/// decides — which is what makes a square-cornered Fluent theme or a heavily
/// rounded Material 3 one a change to six values rather than to eighty call
/// sites.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tier {
    /// Progress tracks, the bar's grabber — things a couple of points tall.
    Hairline,
    /// Small hit targets, up to about twenty-four points.
    Control,
    /// Rows, ghost-button hovers, tab rows. The commonest by far.
    Row,
    /// Ordinary cards: settings sections, widgets, menus, tiles.
    Card,
    /// Page-sized surfaces, and the floating content card.
    Panel,
    /// Search boxes and anything else that reads as a pill.
    Pill,
}

/// What each tier comes to, in points.
#[derive(Clone, Copy)]
pub struct Radii {
    pub hairline: u8,
    pub control: u8,
    pub row: u8,
    pub card: u8,
    pub panel: u8,
    pub pill: u8,
}

impl Radii {
    pub const fn of(&self, tier: Tier) -> u8 {
        match tier {
            Tier::Hairline => self.hairline,
            Tier::Control => self.control,
            Tier::Row => self.row,
            Tier::Card => self.card,
            Tier::Panel => self.panel,
            Tier::Pill => self.pill,
        }
    }
}

/// What Zervo's surfaces are made of.
///
/// A theme is a palette and a material: the palette says what colour things
/// are, the material says how they are built. Every surface the chrome draws
/// asks the material rather than deciding for itself, so a new material is a
/// new look for the whole application and nothing else has to change — the
/// same cards, the same rows, the same dialogs, built to a different recipe.
///
/// This is the seam a Fluent, GTK, Qt or Material 3 theme would be written
/// against. None of the numbers below are glass-specific: a material with
/// `frosts: false`, no sheen, a heavier edge and square corners is a flat
/// desktop toolkit, and the code that draws the chrome does not change.
#[derive(Clone, Copy)]
pub struct Material {
    pub name: &'static str,

    // ── What a surface is filled with
    /// What a [`Surface::Card`] carries at rest, and how much more at full
    /// strength.
    pub fill: f32,
    pub fill_strength: f32,
    /// Whether this material honours the reader's translucency setting.
    ///
    /// True for anything glassy. A material for a toolkit that has no notion
    /// of translucency — a flat GTK or Fluent one — sets this false and its
    /// surfaces stay exactly as it drew them, whatever the setting says.
    pub translucency: bool,
    /// What a floating panel and a text field carry instead of `fill`.
    ///
    /// A panel is the same weight as a card, deliberately. Both sit on a real
    /// blur — a card on the wallpaper's, a panel on the system's, which is
    /// what the window has behind it — and a tint heavy enough to make a panel
    /// legible against the worst case mutes that blur into flat grey in every
    /// other case. A text field is heavier because what you type into it has
    /// to be read against whatever is behind it.
    pub menu_fill: f32,
    pub input_fill: f32,
    /// The same pair over a blurred backdrop, where the fill is a tint on the
    /// blur rather than a substitute for it.
    pub frosted_fill: f32,
    pub frosted_fill_strength: f32,
    /// The white sheen laid over the fill, out of 255, in the dark theme and
    /// the light one. Zero for a material that does not do glassiness.
    pub sheen_dark: f32,
    pub sheen_light: f32,

    // ── Its edge
    /// The hairline along a surface's edge, out of 255, dark and light.
    pub edge_dark: f32,
    pub edge_light: f32,

    // ── How far off the page it sits
    pub lift_dark: f32,
    pub lift_light: f32,
    /// How far a shadow reaches: a fixed part, plus a share of the radius.
    pub shadow_reach: f32,
    pub shadow_reach_per_radius: f32,
    /// The accent halo behind a focused surface, and how far it reaches.
    pub glow: f32,
    pub glow_reach: f32,

    // ── Whether it frosts what is behind it
    pub frosts: bool,
    /// How far a backdrop is blurred before anything is frosted against it,
    /// in pixels of the small copy that is kept for the purpose.
    ///
    /// Pitched to match what the system's own backdrop does to the desktop, so
    /// a card sitting on the wallpaper and the window sitting on the desktop
    /// are blurred to the same degree and read as the same glass.
    pub blur: f32,

    // ── Shape and metrics
    pub radius: Radii,
    /// The height of a settings row, a menu row, a list row.
    pub row_height: f32,
    /// Padding inside a button, and the gap between stacked controls.
    pub control_padding: Vec2,
    pub item_spacing: Vec2,
    /// How long a hover, a selection or a fade takes to settle.
    pub animation: f32,
}

impl Material {
    /// Zervo's own: layered translucency, a hairline edge, a soft shadow, and
    /// a real blur of whatever is behind.
    pub const GLASS: Material = Material {
        name: "Glass",
        fill: 0.55,
        fill_strength: 0.4,
        translucency: true,
        menu_fill: 0.34,
        input_fill: 0.62,
        frosted_fill: 0.58,
        frosted_fill_strength: 0.16,
        sheen_dark: 9.0,
        sheen_light: 24.0,
        edge_dark: 26.0,
        edge_light: 120.0,
        lift_dark: 0.55,
        lift_light: 0.8,
        shadow_reach: 4.0,
        shadow_reach_per_radius: 0.45,
        glow: 0.32,
        glow_reach: 8.0,
        frosts: true,
        blur: 10.8,
        radius: Radii {
            hairline: 2,
            control: 7,
            row: 8,
            card: 10,
            panel: 12,
            pill: 14,
        },
        row_height: 30.0,
        control_padding: Vec2::new(9.0, 5.0),
        item_spacing: Vec2::new(8.0, 5.0),
        // The default 0.1s reads as flicker; glass should settle, not pop.
        animation: 0.14,
    };
}

/// Cross one palette into another.
///
/// `mix` is for opaque tints and drops alpha on the floor; `shadow` carries
/// its alpha, so a theme crossfade needs a blend that keeps it.
pub fn lerp(a: &Palette, b: &Palette, t: f32) -> Palette {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    let blend = |x: Color32, y: Color32| {
        Color32::from_rgba_premultiplied(
            (x.r() as f32 * inv + y.r() as f32 * t) as u8,
            (x.g() as f32 * inv + y.g() as f32 * t) as u8,
            (x.b() as f32 * inv + y.b() as f32 * t) as u8,
            (x.a() as f32 * inv + y.a() as f32 * t) as u8,
        )
    };
    Palette {
        // Switches once, halfway. It picks which recipe a surface uses — a
        // white wash or a dark one — rather than being a color to interpolate.
        dark: if t < 0.5 { a.dark } else { b.dark },
        bg: blend(a.bg, b.bg),
        surface: blend(a.surface, b.surface),
        surface_hover: blend(a.surface_hover, b.surface_hover),
        active: blend(a.active, b.active),
        accent: blend(a.accent, b.accent),
        text: blend(a.text, b.text),
        text_muted: blend(a.text_muted, b.text_muted),
        border: blend(a.border, b.border),
        shadow: blend(a.shadow, b.shadow),
        // Not a colour and not part of the theme, so it does not cross over —
        // it is whatever the setting says, on both sides of the fade.
        translucency: b.translucency,
        backdrop: b.backdrop,
        // Not a colour either. A crossfade between two *materials* would mean
        // interpolating corner radii and metrics, which is a different and
        // much larger idea than fading two palettes into each other.
        material: b.material,
    }
}

impl Palette {
    /// Stamp the reader's translucency setting on. `resolve` has no business
    /// knowing about Settings, so main.rs does this once a frame.
    pub fn with_translucency(mut self, translucency: Translucency) -> Self {
        self.translucency = translucency;
        self
    }

    /// The tint over the window's own chrome.
    pub fn chrome_tint(&self) -> f32 {
        self.translucency.chrome()
    }

    /// The tint on a surface of this class.
    ///
    /// At Solid everything is opaque and the classes collapse into one; below
    /// that each carries the weight the material gave it.
    pub fn tint_for(&self, surface: Surface) -> f32 {
        if self.translucency.scales() {
            return 1.0;
        }
        match surface {
            Surface::Card => self.translucency.surface(),
            Surface::Menu => self.material.menu_fill,
            Surface::Input => self.material.input_fill,
        }
    }

    /// A surface's fill, for the things egui draws itself rather than through
    /// `glass`: its popups, its menus, its tooltips.
    ///
    /// They take their colour from `Visuals` and never see the material, so
    /// without this a combo box's dropdown is an opaque slab in the middle of
    /// a window made of glass — which is exactly how it looked.
    pub fn surface_fill(&self) -> Color32 {
        // A menu: these are the popups egui floats over everything.
        let tint = self.tint_for(Surface::Menu);
        Color32::from_rgba_unmultiplied(
            self.bg.r(),
            self.bg.g(),
            self.bg.b(),
            (tint.clamp(0.0, 1.0) * 255.0) as u8,
        )
    }

    /// The fill behind something you type into.
    ///
    /// Heavier than a surface, because a text field is the one place where
    /// what is behind it competes directly with what you are reading. The
    /// reference makes the same distinction — its panels are a third opaque
    /// and its URL bar is nearly two thirds.
    pub fn input_fill(&self) -> Color32 {
        let tint = self.tint_for(Surface::Input);
        Color32::from_rgba_unmultiplied(
            self.surface.r(),
            self.surface.g(),
            self.surface.b(),
            (tint * 255.0) as u8,
        )
    }

    /// How round a surface of this size is, per the material.
    ///
    /// The way to spell a corner radius. A number written at a call site is a
    /// number a theme cannot change.
    pub fn radius(&self, tier: Tier) -> u8 {
        self.material.radius.of(tier)
    }

    /// Put a blurred backdrop behind every glass surface that sits on it.
    ///
    /// Scoped by the caller rather than global: the wallpaper is behind the
    /// content area and nothing else, so the sidebar must not be frosted
    /// against a picture that is not behind it.
    pub fn with_backdrop(mut self, backdrop: Option<Backdrop>) -> Self {
        self.backdrop = backdrop;
        self
    }

    /// The part of the backdrop that `rect` sits on: where to draw it, and
    /// which part of the picture that is.
    ///
    /// The overlap, not the whole rect. Requiring a surface to be *wholly*
    /// inside the picture is the obvious rule and the wrong one: a card
    /// scrolled half off the top of the page, or carried past the edge under
    /// the pointer, would stop frosting all at once — and since the fill
    /// recipe follows the frost, it would not merely lose its blur, it would
    /// turn into a different material between one frame and the next. Frosting
    /// the overlap degrades continuously instead, and the uv stays in range,
    /// which is what the containment test was protecting against: egui_glow
    /// samples with `CLAMP_TO_EDGE`, so an out-of-range uv smears the edge row
    /// of the picture across the overhang.
    /// How light whatever is behind `rect` will read once this palette's glass
    /// has been laid over it, 0..=1.
    ///
    /// `None` when nothing is behind it, which is the ordinary case for a
    /// surface over the chrome: there the theme already knows the answer.
    pub fn brightness_under(&self, rect: Rect) -> Option<f32> {
        let behind = self.page_brightness_under(rect)?;
        // What the text will actually sit on is the backdrop seen *through* the
        // glass, not the backdrop. A dark card at a third opacity over a white
        // page reads as neither.
        let tint = self.tint_over(rect, self.tint_for(Surface::Menu));
        let own = f32::from(luminance_of(self.bg)) / 255.0;
        Some(behind * (1.0 - tint) + own * tint)
    }

    /// The tint a surface needs so that it still reads as one of this theme's
    /// surfaces, given `wanted` is what the material asked for.
    ///
    /// The material's tint is deliberately thin — it is a wash *on* a blur
    /// rather than a substitute for one. That works until the page behind is
    /// the opposite of the theme: a third of near-black over a white page is
    /// two-thirds white, so a dark-mode menu opened over a white page came out
    /// white. It was still glass, and still blurred, and still wrong — a menu
    /// belongs to the theme first and to the page behind it second.
    ///
    /// So the wash thickens exactly as far as it has to and no further. Over a
    /// page the theme already agrees with, nothing changes at all.
    pub fn tint_over(&self, rect: Rect, wanted: f32) -> f32 {
        if self.translucency != Translucency::Frosted {
            return wanted;
        }
        let Some(behind) = self.page_brightness_under(rect) else {
            return wanted;
        };
        let own = f32::from(luminance_of(self.bg)) / 255.0;
        let target = if self.dark {
            DARKEST_LIGHT
        } else {
            LIGHTEST_DARK
        };
        // Which way the tint has to pull, and how far the tint can pull it.
        let span = own - behind;
        let needed = if self.dark {
            if behind <= target || span >= 0.0 {
                return wanted;
            }
            (behind - target) / -span
        } else {
            if behind >= target || span <= 0.0 {
                return wanted;
            }
            (target - behind) / span
        };
        // Never thinner than the material asked for, and never opaque: past
        // this the blur stops showing through and it is no longer glass, which
        // is the other half of what makes it look right.
        needed.clamp(wanted, THICKEST_TINT)
    }

    /// How light whatever is behind `rect` is, before this palette's glass goes
    /// over it.
    fn page_brightness_under(&self, rect: Rect) -> Option<f32> {
        let backdrop = self.backdrop?;
        let page = backdrop.rect;
        if page.width() <= 0.0 || page.height() <= 0.0 {
            return None;
        }
        let patch = rect.intersect(page);
        if patch.width() <= 0.0 || patch.height() <= 0.0 {
            return None;
        }
        let cell = |value: f32, low: f32, high: f32| {
            let fraction = ((value - low) / (high - low)).clamp(0.0, 1.0);
            ((fraction * LUMA_CELLS as f32) as usize).min(LUMA_CELLS - 1)
        };
        let x0 = cell(patch.min.x, page.min.x, page.max.x);
        let x1 = cell(patch.max.x, page.min.x, page.max.x);
        let y0 = cell(patch.min.y, page.min.y, page.max.y);
        let y1 = cell(patch.max.y, page.min.y, page.max.y);
        let mut total = 0_u32;
        let mut count = 0_u32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                total += u32::from(backdrop.luma[y * LUMA_CELLS + x]);
                count += 1;
            }
        }
        if count == 0 {
            return None;
        }
        Some(f32::from((total / count) as u8) / 255.0 * backdrop.alpha)
    }

    /// This palette, with text chosen for whatever is behind `rect`.
    ///
    /// A floating panel is the one place where the theme cannot know what its
    /// text will land on: the same downloads card sits over a black page one
    /// moment and a white one the next, and in dark mode its pale text
    /// disappears against the second. Ask for this once per panel and the text
    /// inside it follows the page rather than the theme.
    ///
    /// It changes nothing when the panel is over the chrome, and nothing when
    /// the backdrop agrees with the theme — so a panel over a dark page in dark
    /// mode looks exactly as it always did.
    pub fn over(&self, rect: Rect) -> Palette {
        let Some(brightness) = self.brightness_under(rect) else {
            return *self;
        };
        // Which of the two inks actually reads better on it, rather than a
        // guess at where the middle is. The crossover is not at half: pale text
        // on a mid grey is worse than dark text on the same grey, and picking
        // 0.5 left a band of backgrounds where the theme kept the ink that read
        // worse.
        let (theirs, ours) = if self.dark {
            (DARK_INK, LIGHT_INK)
        } else {
            (LIGHT_INK, DARK_INK)
        };
        // The theme keeps its own unless the other is *clearly* better. Text
        // that flips back and forth as a page scrolls past the crossover is
        // worse than text that is slightly less contrasty than it could be, and
        // this value moves as the page behind it moves.
        let take_theirs =
            contrast(theirs.0, brightness) > contrast(ours.0, brightness) * FLIP_MARGIN;
        let (text, muted) = if take_theirs { theirs } else { ours };
        Palette {
            text,
            text_muted: muted,
            ..*self
        }
    }

    pub fn backdrop_under(&self, rect: Rect) -> Option<(TextureId, Rect, Rect)> {
        let backdrop = self.backdrop?;
        let page = backdrop.rect;
        if page.width() <= 0.0 || page.height() <= 0.0 {
            return None;
        }
        // It has to *touch* the picture to frost against it, but once it does,
        // it frosts over all of itself. Returning only the overlap was the
        // careful-looking answer and the wrong one: a hover card anchored to a
        // toolbar button hangs a few points above the page, and frosting only
        // the part below the edge drew a card that was glass at the bottom and
        // flat at the top with a seam across it. The sampler clamps at the
        // edge, so the blur simply continues — which is what the eye expects
        // and what the alternative could not give it.
        if rect.intersect(page).width() <= 0.0 || rect.intersect(page).height() <= 0.0 {
            return None;
        }
        let quad = rect;
        let across = |value: f32, low: f32, high: f32, from: f32, to: f32| {
            from + (to - from) * ((value - low) / (high - low))
        };
        let map = |point: egui::Pos2| {
            pos2(
                across(
                    point.x,
                    page.min.x,
                    page.max.x,
                    backdrop.uv.min.x,
                    backdrop.uv.max.x,
                ),
                across(
                    point.y,
                    page.min.y,
                    page.max.y,
                    backdrop.uv.min.y,
                    backdrop.uv.max.y,
                ),
            )
        };
        Some((
            backdrop.texture,
            quad,
            Rect::from_min_max(map(quad.min), map(quad.max)),
        ))
    }
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 3] = [ThemeMode::Auto, ThemeMode::Light, ThemeMode::Dark];

    pub fn label(&self) -> &'static str {
        match self {
            ThemeMode::Auto => "Auto",
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// Chrome base — window background, panels.
    pub bg: Color32,
    /// Raised surfaces: address pill, cards, inputs.
    pub surface: Color32,
    /// Hover fill for rows and ghost buttons.
    pub surface_hover: Color32,
    /// Active tab fill — accent-tinted surface.
    pub active: Color32,
    pub accent: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub border: Color32,
    /// Shadow color for the floating content card.
    pub shadow: Color32,
    /// How much comes through a surface — the reader's setting, not a colour.
    ///
    /// It lives here because the palette is already handed to every
    /// `glass::shapes` call in the app; the alternative was a parameter on
    /// nine drawing functions and their callers. `resolve` leaves it at 1.0
    /// and main.rs stamps the setting on, the same way `dark` is a fact about
    /// the theme rather than a colour.
    pub translucency: Translucency,
    /// What surfaces are made of: corner radii, fills, edges, shadows, the
    /// lot. See [`Material`].
    pub material: Material,
    /// What is behind the chrome, blurred, if anything is.
    ///
    /// The backdrop, not the frost: `material.frosts` says whether surfaces
    /// frost at all, and this is the thing they frost against.
    ///
    /// Here for the same reason the translucency setting is: frosted glass is
    /// the material every surface in Zervo is made of, so what it frosts has
    /// to reach every surface without nine call sites being edited to pass it.
    /// A caller draws something behind the chrome, hands the palette a blurred
    /// copy of it, and every card, pill and menu drawn on top is frosted
    /// against it — with no change at the call site at all.
    pub backdrop: Option<Backdrop>,
}

// Colour architecture inspired by Zen Browser's public design tokens: the chrome is neutral gray bases
// (dark #1b1b1b, light "paper" #ebebeb/#e2e2e2) with the accent blended in at
// low ratios, so the accent choice retints the whole chrome, not just
// highlights.

/// Corner radius of the floating web-content card, in points.
/// Largest element gets the largest radius in the size-tiered system.
pub const CONTENT_RADIUS: f32 = Material::GLASS.radius.panel as f32;
/// Gap between the chrome panels and the web-content card, in points.
pub const CONTENT_MARGIN: f32 = 8.0;

/// Workspace dot colors, cycled by workspace index. Mid-saturation pastels
/// that read on both light and dark chrome.
pub const WORKSPACE_COLORS: [Color32; 5] = [
    Color32::from_rgb(150, 120, 240),
    Color32::from_rgb(90, 165, 210),
    Color32::from_rgb(100, 180, 135),
    Color32::from_rgb(222, 160, 84),
    Color32::from_rgb(219, 120, 140),
];

pub fn workspace_color(index: usize) -> Color32 {
    WORKSPACE_COLORS[index % WORKSPACE_COLORS.len()]
}

/// The base the new tab page is painted on: deeper than the chrome in the dark
/// theme, airier in the light one. The page is the one surface with nothing
/// behind it, so it can afford to be further from the chrome than a panel can.
pub fn page_base(palette: &Palette) -> Color32 {
    if palette.dark {
        mix(palette.bg, Color32::BLACK, 0.35)
    } else {
        mix(palette.bg, Color32::WHITE, 0.45)
    }
}

/// The veil laid over a photograph, at `amount` of its full strength.
///
/// The same colour as the page itself, so a wallpaper reads as the page seen
/// through frosted glass rather than as a picture with a grey sheet on it.
/// Some veil is always there: a card has to be legible on a snowfield as well
/// as on a night sky, and no photograph is worth an unreadable page.
pub fn page_veil(palette: &Palette, amount: f32) -> Color32 {
    let base = page_base(palette);
    Color32::from_rgba_unmultiplied(
        base.r(),
        base.g(),
        base.b(),
        (amount.clamp(0.0, 1.0) * 255.0) as u8,
    )
}

pub fn resolve(mode: ThemeMode, system_dark: bool, accent: AccentColor) -> Palette {
    let dark = match mode {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::Auto => system_dark,
    };
    let accent_color = accent.color(dark);
    let mut palette = if dark {
        Palette {
            dark: true,
            bg: mix(Color32::from_rgb(27, 27, 27), accent_color, 0.045),
            surface: mix(Color32::from_rgb(38, 38, 38), accent_color, 0.06),
            surface_hover: mix(Color32::from_rgb(51, 51, 51), accent_color, 0.07),
            active: Color32::PLACEHOLDER,
            accent: accent_color,
            text: LIGHT_INK.0,
            text_muted: LIGHT_INK.1,
            border: mix(Color32::from_rgb(60, 60, 62), accent_color, 0.08),
            shadow: Color32::from_rgba_premultiplied(0, 0, 0, 90),
            translucency: Translucency::Solid,
            material: Material::GLASS,
            backdrop: None,
        }
    } else {
        Palette {
            dark: false,
            bg: mix(Color32::from_rgb(235, 235, 235), accent_color, 0.03),
            surface: mix(Color32::from_rgb(226, 226, 226), accent_color, 0.04),
            surface_hover: mix(Color32::from_rgb(213, 213, 213), accent_color, 0.05),
            active: Color32::PLACEHOLDER,
            accent: accent_color,
            text: DARK_INK.0,
            text_muted: DARK_INK.1,
            border: Color32::from_rgb(204, 204, 204),
            shadow: Color32::from_rgba_premultiplied(0, 0, 0, 50),
            translucency: Translucency::Solid,
            material: Material::GLASS,
            backdrop: None,
        }
    };
    // The active-tab tint follows the accent.
    palette.active = mix(palette.bg, palette.accent, if dark { 0.30 } else { 0.24 });
    palette
}

pub fn apply(ctx: &Context, palette: &Palette) {
    let mut style = (*ctx.global_style()).clone();

    // egui's own widgets — buttons, combo boxes, tooltips, the scrollbars —
    // are styled from the material too, so a theme that changes what a control
    // looks like changes the stock ones with it rather than leaving them
    // looking like a different application bolted on.
    let material = &palette.material;
    style.spacing.item_spacing = material.item_spacing;
    style.spacing.button_padding = material.control_padding;
    style.spacing.window_margin = Margin::same(material.radius.row as i8);
    // egui's default 0.1s reads as flicker; a surface should settle, not pop.
    style.animation_time = material.animation;

    let visuals = &mut style.visuals;
    *visuals = if palette.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = palette.bg;
    // egui draws its own popups, menus and tooltips from these, so they have
    // to answer to the material like everything else does.
    visuals.window_fill = palette.surface_fill();
    visuals.extreme_bg_color = palette.input_fill();
    visuals.faint_bg_color = palette.input_fill();
    visuals.hyperlink_color = palette.accent;

    visuals.selection.bg_fill = palette.active;
    visuals.selection.stroke = Stroke::new(1.0_f32, palette.accent);

    // Tooltips and context menus are drawn by egui itself, from these fields
    // rather than from the palette — so left alone they keep egui's defaults:
    // a 6pt radius, a flat gray border, and a shadow offset six points right
    // and ten down whose linear ramp bands. Beside Zervo's own cards, which
    // are 10-12pt with an accent-tinted hairline and a symmetric halo, they
    // read as belonging to a different application.
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.window_stroke = Stroke::new(1.0_f32, palette.border);
    visuals.window_shadow = Shadow {
        offset: [0, 2],
        blur: 14,
        spread: 0,
        color: palette.shadow,
    };
    visuals.popup_shadow = visuals.window_shadow;

    let rounding = CornerRadius::same(material.radius.row);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = rounding;
    }
    // Interactive stock widgets (checkboxes, radios, combo boxes) need a
    // visible outline or an unchecked box renders as nothing.
    visuals.widgets.noninteractive.bg_stroke = Stroke::NONE;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, palette.border);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, palette.text_muted);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, palette.accent);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, palette.border);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, palette.text_muted);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, palette.text_muted);
    visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, palette.text);
    visuals.widgets.hovered.weak_bg_fill = palette.surface_hover;
    visuals.widgets.hovered.bg_fill = palette.surface_hover;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, palette.text);
    visuals.widgets.active.weak_bg_fill = palette.active;
    visuals.widgets.active.bg_fill = palette.active;
    visuals.widgets.open.weak_bg_fill = palette.surface_hover;

    ctx.set_global_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, TextureId, pos2};

    /// A page at 100,100 → 900,700 showing the middle half of a picture.
    fn palette_with_backdrop() -> Palette {
        let mut palette = resolve(ThemeMode::Dark, true, AccentColor::Lavender);
        palette.backdrop = Some(Backdrop {
            texture: TextureId::default(),
            rect: Rect::from_min_max(pos2(100.0, 100.0), pos2(900.0, 700.0)),
            uv: Rect::from_min_max(pos2(0.25, 0.25), pos2(0.75, 0.75)),
            luma: [128; LUMA_CELLS * LUMA_CELLS],
            alpha: 1.0,
        });
        palette
    }

    #[test]
    fn a_surface_on_the_page_frosts_over_all_of_itself() {
        let palette = palette_with_backdrop();
        let card = Rect::from_min_max(pos2(300.0, 250.0), pos2(500.0, 400.0));
        let (_, quad, uv) = palette.backdrop_under(card).expect("card is on the page");
        assert_eq!(
            quad, card,
            "a card wholly on the page frosts over all of it"
        );
        // 300 is a quarter of the way across 100..900, and the picture shows
        // 0.25..0.75, so a quarter of the way in is 0.375.
        assert!((uv.min.x - 0.375).abs() < 1e-5, "uv.min.x was {}", uv.min.x);
        assert!((uv.max.x - 0.5).abs() < 1e-5, "uv.max.x was {}", uv.max.x);
    }

    /// The bug this was written for: a card scrolled half off the top of the
    /// page, or carried past its edge under the pointer, used to stop frosting
    /// altogether — and because the fill recipe followed the frost, it did not
    /// merely lose its blur, it changed material between one frame and the
    /// next.
    ///
    /// Frosting only the overlap fixed the material but left a seam: a hover
    /// card anchored to a toolbar button hangs a few points above the page, so
    /// it drew as glass below the page's top edge and flat above it, with a
    /// line across it where the two met. It frosts over all of itself now, and
    /// the sampler clamps.
    #[test]
    fn a_surface_hanging_off_the_page_still_frosts_over_all_of_itself() {
        let palette = palette_with_backdrop();
        let picture = palette.backdrop.unwrap().uv;
        for card in [
            Rect::from_min_max(pos2(300.0, 40.0), pos2(500.0, 300.0)), // off the top
            Rect::from_min_max(pos2(300.0, 600.0), pos2(500.0, 950.0)), // off the bottom
            Rect::from_min_max(pos2(20.0, 200.0), pos2(300.0, 400.0)), // off the left
            Rect::from_min_max(pos2(800.0, 200.0), pos2(1200.0, 400.0)), // off the right
        ] {
            let (_, quad, uv) = palette
                .backdrop_under(card)
                .expect("part of the card is still on the page");
            assert_eq!(quad, card, "no seam: the whole card frosts");
            // Which puts the uv outside the picture over the overhang. That is
            // the point rather than a defect — egui samples with CLAMP_TO_EDGE,
            // so the picture's edge row carries on across it.
            let escaped = uv.min.x < picture.min.x - 1e-5
                || uv.min.y < picture.min.y - 1e-5
                || uv.max.x > picture.max.x + 1e-5
                || uv.max.y > picture.max.y + 1e-5;
            assert!(
                escaped,
                "uv {uv:?} stayed inside {picture:?}, so it clipped"
            );
        }
    }

    #[test]
    fn the_luminance_map_reads_the_picture() {
        let mut image = egui::ColorImage::filled([32, 32], Color32::BLACK);
        for y in 0..32 {
            for x in 0..16 {
                image.pixels[y * 32 + x] = Color32::WHITE;
            }
        }
        let map = luma_map(&image);
        for row in 0..LUMA_CELLS {
            assert_eq!(map[row * LUMA_CELLS], 255, "left edge is white");
            assert_eq!(
                map[row * LUMA_CELLS + LUMA_CELLS - 1],
                0,
                "right edge is black"
            );
        }
    }

    /// The complaint this answers: a dark-mode menu opened over a white page
    /// came out white. It was frosted, and blurred, and still wrong — a menu
    /// belongs to its theme first and to the page behind it second.
    #[test]
    fn a_menu_over_an_opposite_page_holds_its_theme() {
        for (mode, dark, page) in [
            (ThemeMode::Dark, true, 255_u8),
            (ThemeMode::Light, false, 0_u8),
        ] {
            let mut palette = resolve(mode, dark, AccentColor::Lavender);
            palette.translucency = Translucency::Frosted;
            palette.backdrop = Some(Backdrop {
                luma: [page; LUMA_CELLS * LUMA_CELLS],
                ..palette_with_backdrop().backdrop.unwrap()
            });
            let card = Rect::from_min_max(pos2(300.0, 250.0), pos2(500.0, 400.0));

            let thin = Material::GLASS.frosted_fill;
            let thick = palette.tint_over(card, thin);
            assert!(thick > thin, "{mode:?}: the tint has to thicken: {thick}");
            assert!(thick < 1.0, "{mode:?}: and stay glass: {thick}");

            let seen = palette.brightness_under(card).expect("on the page");
            if dark {
                assert!(seen <= DARKEST_LIGHT + 1e-3, "dark menu read {seen} light");
            } else {
                assert!(seen >= LIGHTEST_DARK - 1e-3, "light menu read {seen} dark");
            }
            // And because it held its theme, the theme's own text still reads
            // on it. The two rules are the same rule from opposite ends.
            assert_eq!(palette.over(card).text, palette.text);
        }
    }

    /// Over a page the theme already agrees with, nothing happens at all — the
    /// material's own number, untouched.
    #[test]
    fn the_tint_is_left_alone_over_an_agreeable_page() {
        let mut palette = palette_with_backdrop();
        palette.translucency = Translucency::Frosted;
        palette.backdrop.as_mut().unwrap().luma = [10; LUMA_CELLS * LUMA_CELLS];
        let card = Rect::from_min_max(pos2(300.0, 250.0), pos2(500.0, 400.0));
        let thin = Material::GLASS.frosted_fill;
        assert_eq!(palette.tint_over(card, thin), thin);
    }

    /// The text rule is the backstop for when the tint cannot hold the theme:
    /// a material whose surfaces are a mid tone has nowhere to pull to, and
    /// that is exactly what a third-party theme is free to be.
    #[test]
    fn the_ink_flips_when_the_tint_cannot_hold_the_theme() {
        let mut palette = palette_with_backdrop();
        palette.translucency = Translucency::Frosted;
        palette.bg = Color32::from_gray(115);
        palette.backdrop.as_mut().unwrap().luma = [255; LUMA_CELLS * LUMA_CELLS];
        let card = Rect::from_min_max(pos2(300.0, 250.0), pos2(500.0, 400.0));

        let seen = palette.brightness_under(card).expect("on the page");
        assert!(
            seen > DARKEST_LIGHT,
            "the premise: this surface cannot get dark enough, {seen}"
        );
        assert_eq!(
            palette.over(card).text,
            DARK_INK.0,
            "so the text has to leave the theme behind instead"
        );
    }

    /// Solid is the step that means what it says. An opaque surface shows
    /// nothing of the page, so the theme already knows what its text lands on
    /// and the page has no vote.
    #[test]
    fn a_solid_surface_keeps_the_theme_s_text() {
        let mut palette = palette_with_backdrop();
        palette.translucency = Translucency::Solid;
        palette.backdrop.as_mut().unwrap().luma = [255; LUMA_CELLS * LUMA_CELLS];
        let card = Rect::from_min_max(pos2(300.0, 250.0), pos2(500.0, 400.0));
        assert_eq!(palette.over(card).text, palette.text);
    }

    /// Only the panels that float over a page get an opinion. Everything drawn
    /// on the chrome keeps the theme's, or the two would disagree along the
    /// edge of the window.
    #[test]
    fn text_is_left_alone_where_there_is_nothing_underneath() {
        let palette = resolve(ThemeMode::Dark, true, AccentColor::Lavender);
        let card = Rect::from_min_max(pos2(300.0, 250.0), pos2(500.0, 400.0));
        assert!(palette.brightness_under(card).is_none());
        assert_eq!(palette.over(card).text, palette.text);
        assert_eq!(palette.over(card).text_muted, palette.text_muted);
    }

    #[test]
    fn a_surface_nowhere_near_the_page_does_not_frost() {
        let palette = palette_with_backdrop();
        let elsewhere = Rect::from_min_max(pos2(0.0, 0.0), pos2(90.0, 90.0));
        assert!(palette.backdrop_under(elsewhere).is_none());
    }

    #[test]
    fn nothing_frosts_without_a_backdrop() {
        let palette = resolve(ThemeMode::Dark, true, AccentColor::Lavender);
        let card = Rect::from_min_max(pos2(300.0, 250.0), pos2(500.0, 400.0));
        assert!(palette.backdrop_under(card).is_none());
    }

    /// The two steps have to be two different things.
    #[test]
    fn the_steps_are_two_different_things() {
        use Translucency::{Frosted, Solid};
        assert_eq!(Solid.chrome(), 1.0, "Solid has to mean an opaque window");
        assert_eq!(Solid.surface(), 1.0, "Solid has to mean opaque surfaces");
        assert!(Frosted.chrome() < Solid.chrome());
        assert!(Frosted.surface() < Solid.surface());
        // The step that matters is not the tint, it is whether the platform is
        // asked for a backdrop at all.
        assert_ne!(Frosted.backdrop(), Solid.backdrop());
    }

    /// The point of the middle step is that you can see what is behind the
    /// window, in the colours it actually has. A tint heavy enough to mute
    /// them passes every ordering check and still fails at the only thing the
    /// setting is for.
    #[test]
    fn frosted_is_actually_frosted() {
        use Translucency::Frosted;
        // The base has to be all but clear or the colours behind the window
        // arrive as a grey suggestion of themselves. Measured against Zen's
        // Transparent mod, whose own base is flatly transparent.
        assert!(
            Frosted.chrome() <= 0.12,
            "a chrome tint of {} washes out whatever is behind it",
            Frosted.chrome()
        );
        // And the surfaces have to be heavier than the base, or the words on
        // them have nothing to sit on.
        assert!(
            Frosted.surface() > Frosted.chrome() * 2.0,
            "surfaces need more tint than the chrome they sit on"
        );
    }

    /// The classes have to stay in their hierarchy. What that hierarchy *is*
    /// took a correction: a panel is glass like a card, not something heavier,
    /// because both of them sit on a blur.
    #[test]
    fn a_panel_is_glass_like_a_card_and_an_input_is_heavier() {
        let palette = resolve(ThemeMode::Dark, true, AccentColor::Lavender)
            .with_translucency(Translucency::Frosted);
        let card = palette.tint_for(Surface::Card);
        let menu = palette.tint_for(Surface::Menu);
        let input = palette.tint_for(Surface::Input);
        // A panel must never be heavier than a card. Both sit on a blur, and
        // the moment a panel outweighs one it stops reading as the same glass
        // — which is what a menu at half opacity looked like beside the new
        // tab page's cards.
        assert!(
            menu <= card,
            "a panel ({menu}) must be no heavier than a card ({card})"
        );
        assert!(
            menu < input,
            "an input ({input}) must outweigh a panel ({menu})"
        );
        assert!(input <= 1.0);
    }

    /// At Solid the classes collapse: everything is opaque and none of the
    /// hierarchy above means anything.
    #[test]
    fn solid_collapses_the_classes() {
        let palette = resolve(ThemeMode::Dark, true, AccentColor::Lavender)
            .with_translucency(Translucency::Solid);
        for class in [Surface::Card, Surface::Menu, Surface::Input] {
            assert_eq!(palette.tint_for(class), 1.0);
        }
    }

    /// Every tier has to resolve to something; a material that forgot one
    /// would give a surface square corners with no other sign.
    #[test]
    fn every_radius_tier_resolves() {
        let radii = Material::GLASS.radius;
        for tier in [
            Tier::Hairline,
            Tier::Control,
            Tier::Row,
            Tier::Card,
            Tier::Panel,
            Tier::Pill,
        ] {
            assert!(radii.of(tier) > 0, "{tier:?} resolved to nothing");
        }
    }
}
