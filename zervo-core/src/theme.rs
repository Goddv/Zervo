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

/// The WCAG ratio body text is held to.
///
/// 4.5 is the AA bar for text at ordinary sizes, which is what a tab title, a
/// settings label and a menu row all are.
const MIN_INK_CONTRAST: f32 = 4.5;

/// Deepen a surface, away from the ink that will sit on it, until that ink
/// reads.
///
/// Turn 5 of the study raises the accent ratio from 0.045 to 0.3 and says
/// plainly what it costs: "All of this is contrast you are spending." It is
/// right — half a pale lavender under the dark theme's own ink comes out at
/// 4.27, under the bar every other run of text in the application is held to.
///
/// The fix this had first was to walk the *ratio* back until it cleared, which
/// keeps the row readable by making it stop being the colour that was asked
/// for: turn the accent all the way up and Candy quietly hands back less
/// accent than it promised, which is the one thing this preset must not do.
///
/// Contrast is measured on luminance, and luminance is the single axis of a
/// colour that is not what "candy" means. So the ratio stays where it was put
/// and the blend moves along that axis instead — darker under pale ink,
/// lighter under dark ink. The hue survives intact and almost all of the
/// saturation with it, and the ratio is bought from the only place that costs
/// the design nothing.
fn readable_over(surface: Color32, ink: Color32, floor: f32) -> Color32 {
    let away = if luminance_of(ink) > 127 {
        Color32::BLACK
    } else {
        Color32::WHITE
    };
    let mut shifted = surface;
    let mut amount = 0.0_f32;
    // Capped: two thirds of the way to black, a tinted row has stopped being a
    // colour with text on it and become a dark row with a hint of one. Past
    // that, giving up and leaving it slightly under the bar is more honest
    // than pretending the accent is still in there.
    while contrast(ink, f32::from(luminance_of(shifted)) / 255.0) < floor && amount < 0.66 {
        amount += 0.02;
        shifted = mix(surface, away, amount);
    }
    shifted
}

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
    /// How far outside its own rectangle this picture will still be frosted
    /// against, in points.
    ///
    /// Zero for the ordinary case, where only what is on the page frosts
    /// against the page. The widget shelf is the exception: it slides out of
    /// the top of the new tab page and sits just above it, and reading it as
    /// part of that page rather than as part of the chrome is what makes it
    /// look like it belongs there. The sampler clamps, so what it gets is the
    /// page's edge carried outward.
    pub reach: f32,
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
    /// The window itself, and the content card once it reaches the window's
    /// edge.
    ///
    /// The rung that was missing. The card borrowed `Panel` and the window was
    /// whatever the platform happened to draw, so the two never agreed except
    /// by luck — and at the point where the card's corner *is* the window's
    /// corner, disagreeing is visible.
    Window,
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
    pub window: u8,
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
            Tier::Window => self.window,
        }
    }

    /// The whole ladder at once, multiplied.
    ///
    /// One number rather than seven, because seven is not a choice anybody
    /// makes — square, a little round, or very round is. The tiers keep their
    /// relationship, so a ladder tuned against itself does not have to be
    /// tuned again to be made rounder.
    pub fn scaled(self, factor: f32) -> Radii {
        let rung = |value: u8| (f32::from(value) * factor).round().clamp(0.0, 255.0) as u8;
        Radii {
            hairline: rung(self.hairline),
            control: rung(self.control),
            row: rung(self.row),
            card: rung(self.card),
            panel: rung(self.panel),
            pill: rung(self.pill),
            window: rung(self.window),
        }
    }
}

/// How a surface's edge is drawn.
///
/// One value, spent three ways. The material carries a single edge strength
/// per theme; this says what shape that strength takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Edge {
    /// A flat hairline, the same all the way round.
    Hairline,
    /// The same value split in two: light along the top, dark along the
    /// bottom. A flat line reads as a border drawn on a shape; two read as the
    /// shape having thickness.
    Bevel,
    /// No edge at all. Workable only where the fill and the shadow are already
    /// carrying the separation on their own.
    None,
}

impl Edge {
    pub const ALL: [Edge; 3] = [Edge::Hairline, Edge::Bevel, Edge::None];

    pub fn label(self) -> &'static str {
        match self {
            Edge::Hairline => "Hairline",
            Edge::Bevel => "Bevel",
            Edge::None => "None",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Edge::Hairline => "A flat 1pt hairline, the same all the way round.",
            Edge::Bevel => {
                "Light along the top, dark along the bottom. Same one value, spent so it \
                 reads as thickness rather than as a border."
            },
            Edge::None => {
                "No edge at all. Only workable when fill and shadow are carrying the \
                 separation on their own."
            },
        }
    }
}

/// How the chrome meets the page.
///
/// Four steps rather than a toggle, because the distance between "a card on a
/// tray" and "one continuous surface" is not one decision: the page has to
/// stop painting a background of its own before closing the gap means
/// anything, and the gap has to close before the chrome can be laid *on* the
/// page rather than beside it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Seam {
    /// The page arrives as an opaque blit, inset by a gap, with a radius of
    /// its own. Two backgrounds that never agree.
    Card,
    /// The page stops painting its own base and lets the window's backdrop
    /// through. Nothing has moved — the card is simply the same glass as the
    /// chrome rather than a different colour of grey.
    Frosted,
    /// The gap closes. Only the window's own corners stay round, and a single
    /// hairline holds the join.
    EdgeToEdge,
    /// The chrome becomes a tint laid on the page. The sidebar floats, the
    /// page scrolls under it, and every surface frosts against the same one
    /// copy.
    OneSurface,
}

impl Seam {
    pub const ALL: [Seam; 4] = [
        Seam::Card,
        Seam::Frosted,
        Seam::EdgeToEdge,
        Seam::OneSurface,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Seam::Card => "Card",
            Seam::Frosted => "Frosted",
            Seam::EdgeToEdge => "Edge to edge",
            Seam::OneSurface => "One surface",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Seam::Card => {
                "An 8pt gap, a 12pt radius and an opaque page base. Two backgrounds that \
                 never agree."
            },
            Seam::Frosted => {
                "The page stops painting its own base and shows the window's backdrop \
                 through it. Layout unchanged."
            },
            Seam::EdgeToEdge => {
                "The gap goes to zero; only the window's own corners stay round. One \
                 hairline holds the join."
            },
            Seam::OneSurface => {
                "The chrome becomes a tint on the page. The sidebar floats, the page \
                 scrolls under it, one backdrop for everything."
            },
        }
    }

    /// Whether the page still paints a background of its own.
    pub fn page_paints_base(self) -> bool {
        self == Seam::Card
    }

    /// Whether the gap between the chrome and the page is forced shut.
    pub fn closes_gap(self) -> bool {
        self >= Seam::EdgeToEdge
    }

    /// Whether the chrome is laid on the page rather than beside it.
    pub fn chrome_floats(self) -> bool {
        self == Seam::OneSurface
    }
}

/// What a hidden sidebar leaves at the window's edge.
///
/// Leaving nothing is the honest description of today: the edge is hot, and
/// the only way to learn that is to find out by accident.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Spine {
    Nothing,
    /// One tick per tab, coloured by its workspace, the active one twice as
    /// long. It fits inside the trigger it advertises, so it is a target
    /// rather than a rumour.
    TabTicks,
    /// Favicons stacked down the edge. Wider, but you can aim at a particular
    /// tab without opening anything.
    Favicons,
}

impl Spine {
    pub const ALL: [Spine; 3] = [Spine::Nothing, Spine::TabTicks, Spine::Favicons];

    pub fn label(self) -> &'static str {
        match self {
            Spine::Nothing => "Nothing",
            Spine::TabTicks => "Tab ticks",
            Spine::Favicons => "Favicons",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Spine::Nothing => {
                "Today's behaviour — a hidden sidebar leaves nothing, so the hot edge is \
                 only findable by accident."
            },
            Spine::TabTicks => {
                "One 3pt tick per tab, coloured by workspace, the active one twice as \
                 long. Fits inside the 14pt trigger it advertises."
            },
            Spine::Favicons => {
                "Favicons stacked down the edge. Costs more width, but you can aim at a \
                 specific tab without opening anything."
            },
        }
    }
}

/// Where the widget shelf is reachable from.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ShelfHome {
    Bar,
    Sidebar,
    /// Wherever the chrome happens to be — the bar, the sidebar, or the panel
    /// full-page mode reveals.
    Wherever,
}

impl ShelfHome {
    pub const ALL: [ShelfHome; 3] = [ShelfHome::Bar, ShelfHome::Sidebar, ShelfHome::Wherever];

    pub fn label(self) -> &'static str {
        match self {
            ShelfHome::Bar => "Bar only",
            ShelfHome::Sidebar => "Sidebar only",
            ShelfHome::Wherever => "Wherever the chrome is",
        }
    }

    /// Whether the shelf should be offered in the expanded sidebar.
    pub fn in_sidebar(self) -> bool {
        matches!(self, ShelfHome::Sidebar | ShelfHome::Wherever)
    }

    /// Whether the shelf should be offered in the navigation bar.
    pub fn in_bar(self) -> bool {
        matches!(self, ShelfHome::Bar | ShelfHome::Wherever)
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
    /// What shape that one value takes: a flat line, a two-tone bevel, or
    /// nothing.
    pub edge: Edge,

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
    /// Zervo's own, and what the browser opens on: layered translucency, a
    /// hairline edge, a soft shadow, and a real blur of whatever is behind.
    ///
    /// It is the base every arrangement is built from as well as the shipped
    /// one, so the fields the settings page does not offer are read from here
    /// whichever preset is chosen. `glow` is zero: the accent lights a surface
    /// only where an arrangement asks it to, and asking is what `Candy` is
    /// for.
    pub const ZERVO: Material = Material {
        name: "Zervo",
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
        edge: Edge::Hairline,
        lift_dark: 0.55,
        lift_light: 0.8,
        shadow_reach: 4.0,
        shadow_reach_per_radius: 0.45,
        glow: 0.0,
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
            window: 16,
        },
        row_height: 30.0,
        control_padding: Vec2::new(9.0, 5.0),
        item_spacing: Vec2::new(8.0, 5.0),
        // The default 0.1s reads as flicker; glass should settle, not pop.
        animation: 0.14,
    };
}

/// What Menu keeps to Card, and what Input keeps to it.
///
/// Derived from [`Material::ZERVO`] rather than written out, so an arrangement
/// at the shipped fill reproduces exactly the three numbers the material has
/// always had. They come to about 0.62 and 1.13, which is what the settings
/// page tells the reader.
const MENU_OF_FILL: f32 = Material::ZERVO.menu_fill / Material::ZERVO.fill;
const INPUT_OF_FILL: f32 = Material::ZERVO.input_fill / Material::ZERVO.fill;
/// The frosted core against the same fill. A tint on a blur can be thinner
/// than a tint standing in for one, and this is how much thinner.
const FROSTED_OF_FILL: f32 = Material::ZERVO.frosted_fill / Material::ZERVO.fill;
/// Light glass needs more lift than dark. The same wash that reads as a sheen
/// over near-black is invisible over near-white, so one slider moves both and
/// the ratio between them is the material's, not the reader's.
const LIGHT_SHEEN_OF_DARK: f32 = Material::ZERVO.sheen_light / Material::ZERVO.sheen_dark;

/// A whole arrangement, under a name.
///
/// The five are THEMING.md's own table — Windows-flat, GTK, Android Material,
/// Liquid Glass — plus the one that ships and the one the study argues for.
/// Most people will press one of these and stop; the controls under them are
/// for the person who does not, and they start from wherever the preset left
/// them rather than from nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Preset {
    /// What shipped before the study: a gap, a flat hairline, and an accent
    /// quiet enough that ten presets produced ten near-identical greys.
    Zervo,
    /// The study's own conclusion. The accent is a light source, the seam is
    /// gone, and the material glows.
    Candy,
    Flat,
    LiquidGlass,
    Material,
}

impl Preset {
    pub const ALL: [Preset; 5] = [
        Preset::Zervo,
        Preset::Candy,
        Preset::Flat,
        Preset::LiquidGlass,
        Preset::Material,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Preset::Zervo => "Zervo",
            Preset::Candy => "Candy",
            Preset::Flat => "Flat",
            Preset::LiquidGlass => "Liquid Glass",
            Preset::Material => "Material",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Preset::Zervo => {
                "The one that is finished. Glass laid on the page, a quiet accent taken \
                 from the space you are in, and every motion on."
            },
            Preset::Candy => "The accent is the light source, the seam is gone, everything glows.",
            Preset::Flat => "Solid, a small radius and no sheen — a flat desktop toolkit.",
            Preset::LiquidGlass => {
                "Frosted with a blur of zero — a material may be translucent without \
                 blurring. Sheen and lift carry it instead."
            },
            Preset::Material => {
                "A generous radius, a longer shadow, no sheen, and glow standing in for \
                 the ripple's resting state."
            },
        }
    }

    /// The arrangement this preset is.
    ///
    /// Deliberately not the theme and not the accent. Both have a section of
    /// their own on the same page, `Auto` is a choice somebody made about
    /// their whole machine, and a *material* has no business overruling
    /// either of them.
    pub fn appearance(self) -> Appearance {
        match self {
            // Zervo's own, and the arrangement the rest of the browser is
            // drawn against. Not a reconstruction of what shipped before the
            // study: it is the one that was tuned by hand afterwards, saved,
            // lived in, and handed back — the chrome laid on the page rather
            // than beside it, the accent quiet and taken from the space you
            // are in, favicons down the spine, and every motion switched on.
            //
            // The colours are still the shipped ones. `candy` is the only
            // field `resolve` reads for them and it has not moved, so every
            // rung of the ladder comes out at the byte it always did.
            Preset::Zervo => Appearance {
                preset: Some(self),
                seam: Seam::OneSurface,
                // Carried rather than zeroed: the seam ignores it, and a
                // reader who steps the seam back down should find the gap they
                // had rather than none.
                gap: 8.0,
                translucency: Translucency::Frosted,
                blur: 10.8,
                fill: 0.55,
                sheen: 9.0,
                edge: Edge::Hairline,
                corners: 1.0,
                glow: 0.0,
                motion: 0.14,
                candy: 0.045,
                workspace_accent: true,
                sweep: true,
                liquid: true,
                pill_progress: true,
                spine: Spine::Favicons,
                shelf: ShelfHome::Wherever,
                align_nav: false,
            },
            Preset::Candy => Appearance {
                preset: Some(self),
                seam: Seam::OneSurface,
                gap: 0.0,
                translucency: Translucency::Frosted,
                blur: 22.0,
                fill: 0.42,
                sheen: 30.0,
                edge: Edge::Bevel,
                corners: 1.35,
                glow: 0.55,
                motion: 0.18,
                candy: 0.3,
                // Turn 5's whole argument, on the preset that makes it: "let
                // the *workspace* pick the colour instead of a global setting.
                // The window becomes the space you are in." Left off, Candy is
                // one lavender room rather than five.
                workspace_accent: true,
                sweep: true,
                liquid: true,
                pill_progress: true,
                spine: Spine::TabTicks,
                shelf: ShelfHome::Wherever,
                align_nav: true,
            },
            Preset::Flat => Appearance {
                preset: Some(self),
                seam: Seam::Card,
                gap: 6.0,
                translucency: Translucency::Solid,
                blur: 0.0,
                fill: 1.0,
                sheen: 0.0,
                edge: Edge::Hairline,
                corners: 0.3,
                glow: 0.0,
                motion: 0.06,
                candy: 0.02,
                workspace_accent: false,
                sweep: false,
                liquid: false,
                pill_progress: false,
                spine: Spine::Nothing,
                shelf: ShelfHome::Bar,
                align_nav: false,
            },
            Preset::LiquidGlass => Appearance {
                preset: Some(self),
                seam: Seam::OneSurface,
                gap: 0.0,
                translucency: Translucency::Frosted,
                blur: 0.0,
                fill: 0.34,
                sheen: 44.0,
                edge: Edge::Bevel,
                corners: 1.6,
                glow: 0.4,
                motion: 0.22,
                candy: 0.12,
                workspace_accent: false,
                sweep: true,
                liquid: true,
                pill_progress: true,
                spine: Spine::TabTicks,
                shelf: ShelfHome::Wherever,
                align_nav: true,
            },
            Preset::Material => Appearance {
                preset: Some(self),
                seam: Seam::EdgeToEdge,
                gap: 0.0,
                translucency: Translucency::Solid,
                blur: 0.0,
                fill: 1.0,
                sheen: 0.0,
                edge: Edge::None,
                corners: 1.8,
                glow: 0.3,
                motion: 0.2,
                candy: 0.1,
                workspace_accent: false,
                sweep: false,
                liquid: true,
                pill_progress: false,
                spine: Spine::Favicons,
                shelf: ShelfHome::Sidebar,
                align_nav: false,
            },
        }
    }
}

/// Every value the reader can set about how Zervo is built, in one place.
///
/// This is the answer to the open question THEMING.md has carried since it was
/// written: a theme was a Rust constant with no file format and no loader. If
/// every value is settable at runtime then the loader already exists, and the
/// file format is whatever this struct serialises to.
///
/// It is not a second [`Material`]. It is the arrangement a material is *built
/// from* — [`Appearance::material`] does the building — plus the handful of
/// chrome decisions that are not about what a surface is made of: where the
/// seam falls, what a hidden sidebar leaves behind, where the shelf lives.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// Which preset this is, or `None` once anything below has been moved.
    pub preset: Option<Preset>,

    // ── Where the chrome ends and the page begins
    pub seam: Seam,
    /// `CONTENT_MARGIN`: the gap around the content, in points. Ignored once
    /// the seam closes it.
    pub gap: f32,

    // ── What a surface is made of
    pub translucency: Translucency,
    /// How far the backdrop is blurred. Zero is translucent without blurring —
    /// glass that refracts nothing.
    pub blur: f32,
    /// A card's own alpha. Menu and Input keep their ratio to it.
    pub fill: f32,
    /// White laid over the fill, out of 255, in the dark theme; the light one
    /// keeps its ratio to it.
    pub sheen: f32,
    pub edge: Edge,
    /// Multiplies the whole radius ladder at once.
    pub corners: f32,
    /// The accent halo behind a focused surface.
    pub glow: f32,

    // ── How it moves
    /// How long a hover, a selection, a morph or a fade takes to settle.
    pub motion: f32,
    /// One pass of light across any surface that changes size.
    pub sweep: bool,
    /// One highlight that travels and stretches, rather than two rows half-lit
    /// for the settle time.
    pub liquid: bool,
    /// Progress inside the address pill, which gives it back the points it
    /// reserves for a spinner.
    pub pill_progress: bool,

    // ── What colour it is
    /// How far the accent is mixed into the chrome. The shipped 0.045 is quiet
    /// enough that ten accents produce ten near-identical greys.
    pub candy: f32,
    /// Take the accent from the active workspace instead of one global choice,
    /// so the window changes colour when you change space.
    pub workspace_accent: bool,

    // ── What shape it is
    pub spine: Spine,
    pub shelf: ShelfHome,
    /// Centre the sidebar's nav row on the window controls. macOS only; there
    /// is nothing to centre on anywhere else.
    pub align_nav: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        // What the browser opens on. The study argues for `Candy`, and the
        // Appearance page is one press away — but the arrangement that ships
        // is the one every other part of this design is drawn against, and a
        // browser should not open on an argument.
        Preset::Zervo.appearance()
    }
}

impl Appearance {
    /// The arrangement that shipped before the study, for anyone who wants it
    /// back. Named rather than spelled out, so there is one copy of it.
    pub fn classic() -> Self {
        Preset::Zervo.appearance()
    }

    /// Mark the arrangement as the reader's own. Every control on the settings
    /// page calls this: once a value has been moved, the row of presets is
    /// describing something that is no longer true.
    pub fn customised(&mut self) {
        self.preset = None;
    }

    /// Where this arrangement sits between the accent ratio that shipped and
    /// the one turn 5 argues for, 0..1.
    ///
    /// The ladder in `resolve` names both ends of every rung and slides
    /// between them. This is that same slide, for the handful of things that
    /// are accent-coloured without being one of those rungs — the sidebar's
    /// own tint, the glow on a workspace dot — so all of it arrives together
    /// rather than each one carrying its own idea of when candy starts.
    pub fn candy_t(&self) -> f32 {
        ((self.candy - SHIPPED_CANDY) / (STUDY_CANDY - SHIPPED_CANDY)).clamp(0.0, 1.0)
    }

    pub fn preset_label(&self) -> &'static str {
        self.preset.map_or("Custom", Preset::label)
    }

    /// Whether two arrangements look the same, whatever they are called.
    ///
    /// The name is not part of the look: an arrangement somebody saved and
    /// then arrived at again by moving sliders is the same arrangement, and a
    /// preset row that could not say so would be showing nothing selected
    /// while the reader is plainly looking at it.
    pub fn same_look(&self, other: &Appearance) -> bool {
        Appearance {
            preset: other.preset,
            ..*self
        } == *other
    }

    /// Whether the card fill is being held back so that Frosted means
    /// something.
    ///
    /// True only for an arrangement that asks for an opaque card and is also
    /// asking to be frosted — Flat and Material, the moment somebody turns the
    /// control on. Worth saying out loud on the settings page, because
    /// otherwise the Fill slider appears to stop responding near the top.
    pub fn frost_is_capped(&self) -> bool {
        self.translucency == Translucency::Frosted && self.fill > THICKEST_TINT
    }

    /// This arrangement as JSON — the file format the settings page writes
    /// out, ready to be pasted back into `settings.json` or handed to somebody
    /// else.
    pub fn as_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// The material this arrangement comes to.
    ///
    /// [`Material::ZERVO`] is the base rather than a blank one. The fields the
    /// settings page does not offer — how much heavier a surface gets at full
    /// strength, how far a shadow reaches, row height, control padding — were
    /// tuned against each other, and there is nothing to be gained by making
    /// each of them a slider nobody moves.
    pub fn material(&self) -> Material {
        let asked = self.fill.clamp(0.0, 1.0);
        // A card may be opaque at Solid — a flat desktop toolkit wants exactly
        // that. At Frosted it may not be, or the Solid/Frosted control is one
        // that does nothing on half the arrangements offered. `tint_over`
        // already refuses to take a surface past this same ceiling, and for
        // the same reason: past it the blur has stopped showing through and it
        // is not glass any more.
        let fill = if self.translucency == Translucency::Frosted {
            asked.min(THICKEST_TINT)
        } else {
            asked
        };
        let sheen = self.sheen.max(0.0);
        let blur = self.blur.max(0.0);
        Material {
            name: self.preset_label(),
            fill,
            menu_fill: fill * MENU_OF_FILL,
            input_fill: (fill * INPUT_OF_FILL).min(1.0),
            frosted_fill: (fill * FROSTED_OF_FILL).min(1.0),
            sheen_dark: sheen,
            sheen_light: (sheen * LIGHT_SHEEN_OF_DARK).min(255.0),
            edge: self.edge,
            glow: self.glow.max(0.0),
            // Translucent and blurred are two different things, and the panel
            // lets them come apart: a material may be see-through and refract
            // nothing at all. `frosts` is what asks for the expensive half.
            frosts: self.translucency == Translucency::Frosted && blur > 0.0,
            blur,
            radius: Material::ZERVO.radius.scaled(self.corners.max(0.0)),
            animation: self.motion.max(0.0),
            ..Material::ZERVO
        }
    }
}

impl Material {
    /// This material as the Rust constant somebody would paste into
    /// `theme.rs`, beside [`Material::ZERVO`].
    ///
    /// THEMING.md has carried one open question since it was written: a theme
    /// is a Rust constant, with no file format and no loader. Half the answer
    /// is that every value is settable at runtime now. This is the other half
    /// — the constant, written back out from whatever the reader arranged, so
    /// a look somebody tuned by hand can be checked in rather than described.
    pub fn as_rust(&self) -> String {
        let Material {
            name,
            fill,
            fill_strength,
            translucency,
            menu_fill,
            input_fill,
            frosted_fill,
            frosted_fill_strength,
            sheen_dark,
            sheen_light,
            edge_dark,
            edge_light,
            edge,
            lift_dark,
            lift_light,
            shadow_reach,
            shadow_reach_per_radius,
            glow,
            glow_reach,
            frosts,
            blur,
            radius,
            row_height,
            control_padding,
            item_spacing,
            animation,
        } = self;
        format!(
            "pub const {}: Material = Material {{\n\
             \x20   name: {name:?},\n\
             \x20   fill: {fill:?},\n\
             \x20   fill_strength: {fill_strength:?},\n\
             \x20   translucency: {translucency:?},\n\
             \x20   menu_fill: {menu_fill:?},\n\
             \x20   input_fill: {input_fill:?},\n\
             \x20   frosted_fill: {frosted_fill:?},\n\
             \x20   frosted_fill_strength: {frosted_fill_strength:?},\n\
             \x20   sheen_dark: {sheen_dark:?},\n\
             \x20   sheen_light: {sheen_light:?},\n\
             \x20   edge_dark: {edge_dark:?},\n\
             \x20   edge_light: {edge_light:?},\n\
             \x20   edge: Edge::{edge:?},\n\
             \x20   lift_dark: {lift_dark:?},\n\
             \x20   lift_light: {lift_light:?},\n\
             \x20   shadow_reach: {shadow_reach:?},\n\
             \x20   shadow_reach_per_radius: {shadow_reach_per_radius:?},\n\
             \x20   glow: {glow:?},\n\
             \x20   glow_reach: {glow_reach:?},\n\
             \x20   frosts: {frosts:?},\n\
             \x20   blur: {blur:?},\n\
             \x20   radius: Radii {{\n\
             \x20       hairline: {},\n\
             \x20       control: {},\n\
             \x20       row: {},\n\
             \x20       card: {},\n\
             \x20       panel: {},\n\
             \x20       pill: {},\n\
             \x20       window: {},\n\
             \x20   }},\n\
             \x20   row_height: {row_height:?},\n\
             \x20   control_padding: Vec2::new({:?}, {:?}),\n\
             \x20   item_spacing: Vec2::new({:?}, {:?}),\n\
             \x20   animation: {animation:?},\n\
             }};\n",
            name.to_uppercase().replace(' ', "_"),
            radius.hairline,
            radius.control,
            radius.row,
            radius.card,
            radius.panel,
            radius.pill,
            radius.window,
            control_padding.x,
            control_padding.y,
            item_spacing.x,
            item_spacing.y,
        )
    }
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
        warm: blend(a.warm, b.warm),
        warning: blend(a.warning, b.warning),
        success: blend(a.success, b.success),
        danger: blend(a.danger, b.danger),
        info: blend(a.info, b.info),
        text: blend(a.text, b.text),
        text_muted: blend(a.text_muted, b.text_muted),
        border: blend(a.border, b.border),
        shadow: blend(a.shadow, b.shadow),
        // Not a colour and not part of the theme, so it does not cross over —
        // it is whatever the setting says, on both sides of the fade.
        translucency: b.translucency,
        fills_window: b.fills_window,
        backdrop: b.backdrop,
        // Not a colour either. A crossfade between two *materials* would mean
        // interpolating corner radii and metrics, which is a different and
        // much larger idea than fading two palettes into each other.
        material: b.material,
        appearance: b.appearance,
    }
}

impl Palette {
    /// Stamp the reader's translucency setting on.
    ///
    /// Both copies of it, so they cannot come apart: the palette's own field
    /// is what every `glass::shapes` call reads, and the arrangement's is what
    /// the material was built from.
    /// Say that the page has the window to itself — full-page mode.
    pub fn filling_window(mut self, fills: bool) -> Self {
        self.fills_window = fills;
        self
    }

    pub fn with_translucency(mut self, translucency: Translucency) -> Self {
        self.translucency = translucency;
        self.appearance.translucency = translucency;
        self.material = self.appearance.material();
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
    ///
    /// All three rungs come off the material now. `Card` used to read the
    /// translucency setting's own 0.34 instead, which happens to be the number
    /// `menu_fill` also carries — so a card and a menu were drawn at exactly
    /// the same weight, and the ladder the material describes had two rungs
    /// rather than three. A menu must never be heavier than a card or it stops
    /// reading as the same glass; being *equal* to one is the same failure,
    /// quietly.
    pub fn tint_for(&self, surface: Surface) -> f32 {
        if self.translucency.scales() {
            return 1.0;
        }
        match surface {
            Surface::Card => self.material.fill,
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

    /// The same, as the four corners egui wants.
    ///
    /// Every `CornerRadius::same(<literal>)` in the application is a corner a
    /// theme cannot reach, and there were enough of them that a row could be
    /// drawn at eleven points with its own hover rectangle at eight. This is
    /// the one-character-longer way to spell it that answers to the ladder.
    pub fn corner(&self, tier: Tier) -> CornerRadius {
        CornerRadius::same(self.radius(tier))
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
        let tint = self.tint_over(rect, self.nominal_tint(Surface::Menu));
        let own = f32::from(luminance_of(self.bg)) / 255.0;
        Some(behind * (1.0 - tint) + own * tint)
    }

    /// How much of a surface is its own colour rather than what is behind it,
    /// before any thickening.
    ///
    /// Two numbers make this and both are easy to miss: the material's fill,
    /// which is the core's own alpha, and the class's tint, which scales the
    /// whole finished surface afterwards. A menu with a 0.58 fill at a 0.34
    /// class tint is a fifth of its own colour, not three fifths — and code
    /// that reads only one of them is wrong by a factor of three.
    pub fn nominal_tint(&self, surface: Surface) -> f32 {
        let material = &self.material;
        (material.frosted_fill + material.frosted_fill_strength) * self.tint_for(surface)
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
    pub fn page_brightness_under(&self, rect: Rect) -> Option<f32> {
        let backdrop = self.backdrop?;
        let page = backdrop.rect;
        if page.width() <= 0.0 || page.height() <= 0.0 {
            return None;
        }
        let patch = rect.intersect(page.expand(backdrop.reach));
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

    /// This palette, with its picture reaching `points` beyond its own edges.
    ///
    /// For surfaces that sit beside the page rather than on it and should read
    /// as part of it anyway. Nothing when there is no picture.
    pub fn reaching(&self, points: f32) -> Palette {
        Palette {
            backdrop: self.backdrop.map(|backdrop| Backdrop {
                reach: backdrop.reach.max(points),
                ..backdrop
            }),
            ..*self
        }
    }
    /// The colour of a chrome pane — the sidebar, the shelf — as against a
    /// card or a menu.
    ///
    /// 5a paints its sidebar `accent(0.14)`: the accent *itself* at low alpha
    /// over the window's own light, with `saturate(1.5)` behind it. That is a
    /// different thing from a grey with some accent mixed into it, and the
    /// difference is most of why the artboard's sidebar reads as a coloured
    /// pane while `surface` at the same ratio reads as grey.
    ///
    /// It cannot simply be `surface`, because `surface` is also every card and
    /// every menu, and those have body text sitting on them at 4.5:1. A pane
    /// holds rows, and a row brings its own fill and its own ink with it.
    /// A modest step past `surface`, not a second helping of it. 5a's own
    /// ladder is `accent(.14)` on the pane and `accent(.34)` on the row inside
    /// it, so the pane has to stay well below the selection or the selected
    /// tab disappears into the sidebar holding it — which is exactly what a
    /// bolder tint here did.
    pub fn pane_tint(&self) -> Color32 {
        mix(self.surface, self.accent, 0.25 * self.appearance.candy_t())
    }

    /// The accent ring the study draws round its lit surfaces, at `alpha` of
    /// the accent — or nothing, on an arrangement that lights nothing.
    ///
    /// 5a rings very nearly everything: `0 0 0 1px accent(.28)` on the address
    /// pill, .24 on the search field, .2 on a chip, .18 on a card. It is the
    /// cheap half of "the accent as a light source" — the ring is the edge of
    /// the glass catching it, and it is what carries the colour onto surfaces
    /// whose own fill has to stay near-neutral to keep their text readable.
    ///
    /// Scaled by the same `glow` that lights the halo behind a surface, so the
    /// two arrive together: an arrangement that asked for no glow gets no
    /// rings either, and Zervo and Flat are untouched by all of this.
    pub fn accent_ring(&self, alpha: f32) -> Option<Color32> {
        let glow = self.appearance.glow;
        // Measured against the study's own glow rather than against one, so
        // Candy — which is the arrangement these alphas were read off — gets
        // them at full strength, and an arrangement that lights less rings
        // proportionally less rather than being scaled down for no reason.
        let strength = (glow / STUDY_GLOW).min(1.0);
        (glow > 0.0 && alpha > 0.0).then(|| self.accent.gamma_multiply(alpha * strength))
    }

    /// The ink that reads on a surface of this exact colour, and the muted
    /// shade beside it.
    ///
    /// [`Palette::over`] asks this of a photograph. This asks it of a colour
    /// the theme mixed itself — which is what an active row, a filled button
    /// or an accent-tinted chip is, and none of them have a picture behind
    /// them to measure.
    ///
    /// 5b names `Palette::over` and `prefers_light_ink` as the two that
    /// "become load-bearing here rather than a backstop" once the accent is
    /// turned up. This is the third of them, for the surfaces the theme itself
    /// made.
    ///
    /// Same hysteresis as `over`, for the same reason: the theme keeps its own
    /// ink unless the other is clearly better, so text does not flip colour
    /// halfway through the animation that carries a row from unselected to
    /// selected.
    pub fn ink_on(&self, surface: Color32) -> (Color32, Color32) {
        let brightness = f32::from(luminance_of(surface)) / 255.0;
        let (theirs, ours) = if self.dark {
            (DARK_INK, LIGHT_INK)
        } else {
            (LIGHT_INK, DARK_INK)
        };
        if contrast(theirs.0, brightness) > contrast(ours.0, brightness) * FLIP_MARGIN {
            theirs
        } else {
            ours
        }
    }

    /// Whether pale text reads better than dark text at `rect`, or `None` when
    /// there is nothing behind it.
    ///
    /// `on_glass` says whether the text sits on one of this theme's surfaces or
    /// straight on the page. It is the whole question: a card holds the theme,
    /// so text on one follows the theme, while text laid directly on a
    /// photograph follows the photograph.
    pub fn prefers_light_ink(&self, rect: Rect, on_glass: bool) -> Option<bool> {
        let brightness = if on_glass {
            self.brightness_under(rect)?
        } else {
            self.page_brightness_under(rect)?
        };
        Some(contrast(LIGHT_INK.0, brightness) >= contrast(DARK_INK.0, brightness))
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
        let within = page.expand(backdrop.reach);
        if rect.intersect(within).width() <= 0.0 || rect.intersect(within).height() <= 0.0 {
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
    /// Something is wrong but recoverable: an unencrypted connection, a
    /// download that failed, a permission a page is asking for.
    ///
    /// A theme colour rather than a literal at the call site, for the same
    /// reason `accent` is one. There were two of these in the tree — a 220,138,
    /// 40 amber mixed into the address pill's warning badge and nothing at all
    /// for its opposite — so 7c's amber chip and 7b's green tick both came out
    /// grey, and the one warning that did exist could not answer to a theme.
    ///
    /// Not the accent: the accent is a preference and this is a fact about the
    /// page. A reader whose accent is amber must still be able to tell a
    /// warning from a highlight.
    pub warning: Color32,
    /// Something finished, or is safe: a download complete, a task done.
    pub success: Color32,
    /// Something is wrong and is *not* recoverable by carrying on: a
    /// certificate that does not match the site it was served for.
    ///
    /// Distinct from `warning` because 7b needs the distinction and says why:
    /// "the one error page that must not be candy". An engine gap is a wait,
    /// and a bad certificate is a decision — the two must not be the same
    /// colour, or the page that wants you to stop looks like the one that
    /// wants you to be patient.
    pub danger: Color32,
    /// Something is merely so: you are offline, four tabs are queued. Not an
    /// error and not the accent either — the accent is a preference, and a
    /// fact about the network is not one.
    pub info: Color32,
    /// The accent's partner round the wheel — the aurora's second lamp.
    ///
    /// Derived rather than set: see [`warm_of`]. It lives on the palette so
    /// that every place that lights something with the accent can reach the
    /// colour beside it without deriving its own and drifting.
    pub warm: Color32,
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
    /// Whether the page has the whole window to itself.
    ///
    /// A fact about the layout rather than about the theme, here for the same
    /// reason `translucency` is: the palette already reaches every drawing
    /// function, and the alternative was a parameter on all of them.
    /// `resolve` leaves it false and main.rs stamps the layout on.
    pub fills_window: bool,
    /// What surfaces are made of: corner radii, fills, edges, shadows, the
    /// lot. See [`Material`].
    pub material: Material,
    /// The arrangement the material was built from, and the handful of chrome
    /// decisions that are not about what a surface is made of — where the seam
    /// falls, what a hidden sidebar leaves behind, where the shelf lives.
    ///
    /// Here rather than in `Settings` for the same reason the material is: the
    /// palette already reaches every drawing function in the application, and
    /// the alternative was another parameter on all of them.
    pub appearance: Appearance,
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

/// The accent ratio the chrome shipped with, and what a candy setting is
/// measured against.
///
/// It is a small number, and that was the complaint: mixed in this quietly,
/// ten accent presets produce ten near-identical greys. The whole ladder below
/// scales off it, so a reader who leaves it alone gets exactly the chrome that
/// always shipped and one who moves it moves every rung together.
const SHIPPED_CANDY: f32 = 0.045;

/// The ratio turn 5 argues for, and the far end of the ladder.
const STUDY_CANDY: f32 = 0.3;

/// The glow the study's own arrangement carries, and what a ring is measured
/// against. Candy sits exactly here; an arrangement that glows less rings less.
const STUDY_GLOW: f32 = 0.55;

/// Where one rung of the accent ladder sits at this candy setting.
///
/// The rungs are named at both ends rather than scaled from one. `shipped` is
/// what the chrome has always mixed; `study` is the ratio turn 5 asks for by
/// name — "0.16 on the base, 0.24 on surfaces, 0.5 on the active tab" — and
/// the setting slides between them, carrying on past the study's end at the
/// same rate for anyone who wants more than it asked for.
///
/// Scaling every rung by `candy / SHIPPED_CANDY` was the first shape of this
/// and it overshoots badly: at the study's own 0.3 it puts 0.30 of the accent
/// in the base and 0.40 in the surfaces, roughly twice what is being asked
/// for. That is not a brighter turn 5, it is a muddier one. The artboards keep
/// their *surfaces* close to neutral and do the colouring with the light
/// behind them — `frame::paint_chrome_aurora` is where the colour comes from,
/// and a base already 40% of the way to the accent leaves it nothing to add.
fn rung(candy: f32, shipped: f32, study: f32) -> f32 {
    let t = (candy.max(0.0) - SHIPPED_CANDY) / (STUDY_CANDY - SHIPPED_CANDY);
    (shipped + (study - shipped) * t).clamp(0.0, 1.0)
}

/// The radius the window's own corners are cut to by the platform, in points.
///
/// Not a theme value, and deliberately not on the [`Radii`] ladder: the window
/// server draws this one and the chrome cannot argue with it. Painting a
/// different radius at the same corner does not replace the platform's, it
/// puts a second arc beside it — which is what "double corners" looks like,
/// and is exactly what a page rounded to `Tier::Window` did against a macOS
/// window rounded to ten.
///
/// There is no API for it on any of the three. macOS has masked windows at
/// 10pt since Big Sur; Windows 11 rounds at 8 device-independent pixels and
/// Windows 10 does not round at all, so the smaller of the two is the safe
/// answer where a square corner is the failure that shows; X11 and Wayland
/// leave it to the compositor and most draw nothing, so the theme keeps its
/// own value there and nothing is being contradicted.
///
/// `None` means "no platform opinion, use the ladder".
pub const fn platform_window_radius() -> Option<f32> {
    #[cfg(target_os = "macos")]
    {
        Some(10.0)
    }
    #[cfg(target_os = "windows")]
    {
        Some(8.0)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// The radius a surface flush with the window's edge must take.
///
/// The platform's, where the platform has one — see
/// [`platform_window_radius`]. It does not answer to the corner scale for the
/// same reason: the reader can make every surface in the chrome rounder, and
/// the window is not one of the chrome's surfaces.
pub fn window_radius(palette: &Palette) -> f32 {
    platform_window_radius().unwrap_or_else(|| f32::from(palette.radius(Tier::Window)))
}

/// Corner radius of the web-content card, in points.
///
/// Two tiers rather than one. While the card is inset it is a panel among
/// panels and rounds like one; once it reaches the window's edge its corner
/// *is* the window's corner, and it has to be the window's radius or the two
/// disagree by four points in the one place where nobody can miss it.
pub fn content_radius(palette: &Palette) -> f32 {
    if palette.fills_window || palette.appearance.seam.closes_gap() {
        window_radius(palette)
    } else {
        f32::from(palette.radius(Tier::Panel))
    }
}

/// Gap between the chrome panels and the web-content card, in points.
///
/// Nothing in full-page mode: there are no chrome panels to be apart from, so
/// a gap there is a margin around the whole window rather than a seam between
/// two things — an inset page with the desktop showing through the strip
/// outside it. Every preset carries a gap of its own and three of them are
/// non-zero, so this was a border round the page in three arrangements out of
/// five and nothing in the layout said why.
pub fn content_margin(palette: &Palette) -> f32 {
    if palette.fills_window || palette.appearance.seam.closes_gap() {
        0.0
    } else {
        palette.appearance.gap.max(0.0)
    }
}

/// Which of the card's corners are round.
///
/// Once the gap closes the card is flush against the sidebar on its left and
/// against the window everywhere else, so the two corners touching the sidebar
/// square off and only the window's own stay round. That is also two fewer
/// corners to erase out of the framebuffer every frame, which is the rare
/// change that costs less than what it replaces.
pub fn content_corners(palette: &Palette) -> CornerRadius {
    let radius = content_radius(palette) as u8;
    // In full-page mode all four of the page's corners *are* the window's, so
    // all four take the window's radius. The seam's own rule below is about a
    // page flush against a sidebar, and there is no sidebar here.
    if palette.fills_window {
        return CornerRadius::same(radius);
    }
    if palette.appearance.seam.closes_gap() {
        CornerRadius {
            nw: 0,
            sw: 0,
            ne: radius,
            se: radius,
        }
    } else {
        CornerRadius::same(radius)
    }
}

/// The same four corners as points, in the order the framebuffer eraser and
/// the mask fans walk them: north-west, north-east, south-east, south-west.
pub fn content_corner_radii(palette: &Palette) -> [f32; 4] {
    let corners = content_corners(palette);
    [
        f32::from(corners.nw),
        f32::from(corners.ne),
        f32::from(corners.se),
        f32::from(corners.sw),
    ]
}

/// Workspace dot colors, cycled by workspace index. Mid-saturation pastels
/// that read on both light and dark chrome.
pub const WORKSPACE_COLORS: [Color32; 5] = [
    Color32::from_rgb(150, 120, 240),
    Color32::from_rgb(90, 165, 210),
    Color32::from_rgb(100, 180, 135),
    Color32::from_rgb(222, 160, 84),
    Color32::from_rgb(219, 120, 140),
];

/// The second light in each space, from the study's own table.
///
/// Turn 5's aurora is not one colour: every workspace carries a `warm` beside
/// its `rgb`, and the second of the three radials is that one. One hue at
/// three sizes reads as a gradient; two hues read as a room with two lamps in
/// it, and that is the whole of the difference between the artboard and a
/// tinted rectangle.
pub const WORKSPACE_WARMS: [Color32; 5] = [
    Color32::from_rgb(200, 120, 240),
    Color32::from_rgb(70, 200, 200),
    Color32::from_rgb(170, 200, 90),
    Color32::from_rgb(230, 110, 90),
    Color32::from_rgb(150, 90, 220),
];

/// The partner of any accent at all, not just the five.
///
/// The spaces get the study's own pairings. Anything else — the ten accent
/// presets, or a colour somebody mixed on the Appearance page — is rotated a
/// fifth of the way toward the warm end of the wheel, round whichever side is
/// shorter, which is an approximation of what those five pairs are doing by
/// eye. It is stated as an approximation because it is one: Nightshift's pair
/// goes the other way, and no single rule reproduces all five.
pub fn warm_of(accent: Color32) -> Color32 {
    if let Some(index) = WORKSPACE_COLORS.iter().position(|colour| *colour == accent) {
        return WORKSPACE_WARMS[index];
    }
    let mut hsva = egui::ecolor::Hsva::from(accent);
    // Orange, as the warm end.
    let mut delta = 0.06 - hsva.h;
    if delta > 0.5 {
        delta -= 1.0;
    } else if delta < -0.5 {
        delta += 1.0;
    }
    hsva.h = (hsva.h + delta * 0.22).rem_euclid(1.0);
    hsva.s = (hsva.s * 1.05).min(1.0);
    Color32::from(hsva)
}

pub fn workspace_color(index: usize) -> Color32 {
    WORKSPACE_COLORS[index % WORKSPACE_COLORS.len()]
}

/// The same, rotated so that the first workspace takes the colour somebody
/// chose for it and the ones after it carry on round the ring.
///
/// Rotating rather than storing a colour per workspace: the point of the list
/// is that two spaces never look alike, and a free choice per space is a way
/// to end up with two blues. This keeps the guarantee and still lets the first
/// one — the only one that exists at first run — be picked.
pub fn workspace_color_from(index: usize, first: usize) -> Color32 {
    workspace_color(index + first)
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

/// The least anything the page paints over is ever veiled.
///
/// Below this the header controls and the credit line stop being readable on a
/// bright picture — and it is also exactly enough to keep a card legible once
/// the page has stopped painting a base of its own, which is why the seam
/// reaches for the same number rather than inventing a second one.
pub const MIN_VEIL: f32 = 0.15;

/// What the page lays down under itself, given where the seam falls.
///
/// At [`Seam::Card`] it is an opaque base with a colour of its own — which is
/// precisely what makes the seam visible, because that colour and the chrome's
/// are two greys that never quite agree. Past that the page stops painting a
/// background at all and lays down only the veil that keeps a card readable,
/// so what shows through is the window's own backdrop: the same one thing the
/// chrome is a tint on.
pub fn page_ground(palette: &Palette) -> Color32 {
    if palette.appearance.seam.page_paints_base() {
        page_base(palette)
    } else {
        page_veil(palette, MIN_VEIL)
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

pub fn resolve(
    mode: ThemeMode,
    system_dark: bool,
    accent: AccentColor,
    appearance: &Appearance,
) -> Palette {
    let dark = match mode {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::Auto => system_dark,
    };
    let accent_color = accent.color(dark);
    let material = appearance.material();
    // How far the accent reaches into each rung of the chrome. At the shipped
    // ratio every one of these comes out at the byte the chrome has always
    // been; at the study's, at the ratio the study names.
    let candy = appearance.candy;
    let mut palette = if dark {
        Palette {
            dark: true,
            bg: mix(
                Color32::from_rgb(27, 27, 27),
                accent_color,
                rung(candy, 0.045, 0.16),
            ),
            surface: mix(
                Color32::from_rgb(38, 38, 38),
                accent_color,
                rung(candy, 0.06, 0.24),
            ),
            surface_hover: mix(
                Color32::from_rgb(51, 51, 51),
                accent_color,
                rung(candy, 0.07, 0.28),
            ),
            active: Color32::PLACEHOLDER,
            accent: accent_color,
            warm: warm_of(accent_color),
            warning: Color32::from_rgb(240, 187, 120),
            success: Color32::from_rgb(122, 214, 160),
            danger: Color32::from_rgb(240, 148, 170),
            info: Color32::from_rgb(125, 196, 240),
            text: LIGHT_INK.0,
            text_muted: LIGHT_INK.1,
            border: mix(
                Color32::from_rgb(60, 60, 62),
                accent_color,
                rung(candy, 0.08, 0.32),
            ),
            shadow: Color32::from_rgba_premultiplied(0, 0, 0, 90),
            translucency: appearance.translucency,
            fills_window: false,
            material,
            appearance: *appearance,
            backdrop: None,
        }
    } else {
        Palette {
            dark: false,
            bg: mix(
                Color32::from_rgb(235, 235, 235),
                accent_color,
                rung(candy, 0.03, 0.10),
            ),
            surface: mix(
                Color32::from_rgb(226, 226, 226),
                accent_color,
                rung(candy, 0.04, 0.16),
            ),
            surface_hover: mix(
                Color32::from_rgb(213, 213, 213),
                accent_color,
                rung(candy, 0.05, 0.19),
            ),
            active: Color32::PLACEHOLDER,
            accent: accent_color,
            warm: warm_of(accent_color),
            // Darker in the light theme, or an amber chip on white is a pale
            // smudge — the same reason every other rung has two values.
            warning: Color32::from_rgb(176, 108, 20),
            success: Color32::from_rgb(28, 132, 82),
            danger: Color32::from_rgb(186, 42, 78),
            info: Color32::from_rgb(30, 110, 176),
            text: DARK_INK.0,
            text_muted: DARK_INK.1,
            // The one rung that starts at nothing, because the light border
            // always has. It only picks the accent up once the reader asks for
            // more candy than shipped.
            border: mix(
                Color32::from_rgb(204, 204, 204),
                accent_color,
                rung(candy, 0.0, 0.18),
            ),
            shadow: Color32::from_rgba_premultiplied(0, 0, 0, 50),
            translucency: appearance.translucency,
            fills_window: false,
            material,
            appearance: *appearance,
            backdrop: None,
        }
    };
    // The active-tab tint follows the accent, and reaches Turn 5's half at the
    // candy the study asks for. Not past it: at more than half its own colour
    // the selected row has stopped being a tinted surface and become a swatch.
    let wanted = rung(candy, if dark { 0.30 } else { 0.24 }, 0.5);
    // The full ratio, kept. Which of the two inks lands on the row is decided
    // from the colour that was actually asked for, and then that colour is
    // deepened until the ink reads on it — rather than the ratio being the
    // thing that gives way.
    let asked = mix(palette.bg, palette.accent, wanted);
    palette.active = readable_over(asked, palette.ink_on(asked).0, MIN_INK_CONTRAST);
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
    // Every hint in the application, and the only way to set one.
    //
    // `TextEdit::hint_text` takes a `RichText` and then throws its colour away:
    // egui overwrites it with `weak_text_color()`, and says so in a comment
    // apologising for it ("Sucks, since it means users won't be able to
    // override it"). Left at the default that is the text colour at 0.6, which
    // over a frosted pill on a lit page is a hint you cannot read — and it is
    // the *only* thing an empty search field says.
    //
    // Set here rather than worked around at the two call sites, because it is
    // a theme colour: it is the muted tier, which is what a hint has always
    // been, and setting it once means a field added later gets it too.
    visuals.weak_text_color = Some(palette.text_muted);
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
        let mut palette = resolve(
            ThemeMode::Dark,
            true,
            AccentColor::Lavender,
            &Appearance::classic(),
        );
        palette.backdrop = Some(Backdrop {
            texture: TextureId::default(),
            rect: Rect::from_min_max(pos2(100.0, 100.0), pos2(900.0, 700.0)),
            uv: Rect::from_min_max(pos2(0.25, 0.25), pos2(0.75, 0.75)),
            luma: [128; LUMA_CELLS * LUMA_CELLS],
            reach: 0.0,
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
            let mut palette = resolve(mode, dark, AccentColor::Lavender, &Appearance::classic());
            palette.translucency = Translucency::Frosted;
            palette.backdrop = Some(Backdrop {
                luma: [page; LUMA_CELLS * LUMA_CELLS],
                ..palette_with_backdrop().backdrop.unwrap()
            });
            let card = Rect::from_min_max(pos2(300.0, 250.0), pos2(500.0, 400.0));

            let thin = Material::ZERVO.frosted_fill;
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
        let thin = Material::ZERVO.frosted_fill;
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

    /// Both factors, or the number is wrong by a factor of three.
    #[test]
    fn a_surface_tint_counts_the_class_as_well_as_the_fill() {
        let palette = palette_with_backdrop().with_translucency(Translucency::Frosted);
        let fill = Material::ZERVO.frosted_fill + Material::ZERVO.frosted_fill_strength;
        for surface in [Surface::Card, Surface::Menu, Surface::Input] {
            let nominal = palette.nominal_tint(surface);
            assert!(
                nominal < fill,
                "{surface:?}: the class tint has to be in there, {nominal} vs {fill}"
            );
            assert!(
                (nominal - fill * palette.tint_for(surface)).abs() < 1e-6,
                "{surface:?}: it is the product of the two"
            );
        }
    }

    /// A panel that hangs off the page still frosts against it, once it has
    /// been given reach — and one far beyond that reach still does not, or a
    /// menu at the other end of the window would frost against a smear.
    #[test]
    fn reach_lets_a_panel_off_the_page_frost_against_it() {
        let palette = palette_with_backdrop();
        let page = palette.backdrop.unwrap().rect;
        // Wholly above the page, as a hover card is when the shelf is open.
        let card = Rect::from_min_max(pos2(300.0, 20.0), pos2(600.0, 80.0));
        assert!(
            palette.backdrop_under(card).is_none(),
            "without reach it is not on the page"
        );

        let reaching = palette.reaching(200.0);
        let (_, quad, _) = reaching
            .backdrop_under(card)
            .expect("within reach of the page");
        assert_eq!(
            quad, card,
            "and frosts over all of itself, as anything does"
        );

        let far = Rect::from_min_max(pos2(300.0, -400.0), pos2(600.0, -340.0));
        assert!(
            reaching.backdrop_under(far).is_none(),
            "reach is a distance, not a licence"
        );
        assert!(page.min.y > 0.0, "the page is not at the top of the window");
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
        let palette = resolve(
            ThemeMode::Dark,
            true,
            AccentColor::Lavender,
            &Appearance::classic(),
        );
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
        let palette = resolve(
            ThemeMode::Dark,
            true,
            AccentColor::Lavender,
            &Appearance::classic(),
        );
        let card = Rect::from_min_max(pos2(300.0, 250.0), pos2(500.0, 400.0));
        assert!(palette.backdrop_under(card).is_none());
    }

    /// The two steps have to be two different things.
    #[test]
    fn the_steps_are_two_different_things() {
        use Translucency::{Frosted, Solid};
        assert_eq!(Solid.chrome(), 1.0, "Solid has to mean an opaque window");
        assert!(Frosted.chrome() < Solid.chrome());
        let at = |translucency| {
            resolve(
                ThemeMode::Dark,
                true,
                AccentColor::Lavender,
                &Appearance::classic(),
            )
            .with_translucency(translucency)
            .tint_for(Surface::Card)
        };
        assert_eq!(at(Solid), 1.0, "Solid has to mean opaque surfaces");
        assert!(at(Frosted) < at(Solid));
        // The step that matters is not the tint, it is whether the platform is
        // asked for a backdrop at all.
        assert_ne!(Frosted.backdrop(), Solid.backdrop());
    }

    /// The Solid/Frosted control has to do something on every arrangement,
    /// including the two that ship opaque. An arrangement whose card fill is
    /// 1.0 would otherwise offer a switch that changes nothing.
    #[test]
    fn every_preset_can_be_frosted() {
        for preset in Preset::ALL {
            let mut appearance = preset.appearance();
            appearance.translucency = Translucency::Solid;
            let solid = resolve(ThemeMode::Dark, true, AccentColor::Lavender, &appearance)
                .tint_for(Surface::Card);
            appearance.translucency = Translucency::Frosted;
            let frosted = resolve(ThemeMode::Dark, true, AccentColor::Lavender, &appearance)
                .tint_for(Surface::Card);
            assert_eq!(solid, 1.0, "{}: Solid means opaque", preset.label());
            assert!(
                frosted < 1.0,
                "{}: Frosted has to let something through, got {frosted}",
                preset.label()
            );
        }
    }

    /// The arrangement that shipped has to still resolve to the colours that
    /// shipped. It is the fallback the whole preset row exists to preserve,
    /// and a fallback that is nearly right is not one.
    #[test]
    fn the_classic_arrangement_resolves_to_what_shipped() {
        let classic = Appearance::classic();
        for accent in AccentColor::PRESETS {
            for (mode, dark) in [(ThemeMode::Dark, true), (ThemeMode::Light, false)] {
                let palette = resolve(mode, dark, accent, &classic);
                let colour = accent.color(dark);
                // The arithmetic as it was written before any of this, spelled
                // out rather than referenced, so the two cannot drift together
                // into being wrong in the same direction.
                let (bg, surface, hover, active) = if dark {
                    (
                        mix(Color32::from_rgb(27, 27, 27), colour, 0.045),
                        mix(Color32::from_rgb(38, 38, 38), colour, 0.06),
                        mix(Color32::from_rgb(51, 51, 51), colour, 0.07),
                        0.30,
                    )
                } else {
                    (
                        mix(Color32::from_rgb(235, 235, 235), colour, 0.03),
                        mix(Color32::from_rgb(226, 226, 226), colour, 0.04),
                        mix(Color32::from_rgb(213, 213, 213), colour, 0.05),
                        0.24,
                    )
                };
                assert_eq!(palette.bg, bg, "{accent:?} {mode:?} bg");
                assert_eq!(palette.surface, surface, "{accent:?} {mode:?} surface");
                assert_eq!(palette.surface_hover, hover, "{accent:?} {mode:?} hover");
                assert_eq!(
                    palette.active,
                    mix(bg, colour, active),
                    "{accent:?} {mode:?} active"
                );
                if !dark {
                    assert_eq!(
                        palette.border,
                        Color32::from_rgb(204, 204, 204),
                        "{accent:?}: the light border took accent it never used to"
                    );
                }
            }
        }
        for (mode, dark) in [(ThemeMode::Dark, true), (ThemeMode::Light, false)] {
            let palette = resolve(mode, dark, AccentColor::Lavender, &classic);
            // And the material it builds is the one the constant describes.
            assert_eq!(palette.material.fill, Material::ZERVO.fill);
            assert!((palette.material.menu_fill - Material::ZERVO.menu_fill).abs() < 1e-6);
            assert!((palette.material.input_fill - Material::ZERVO.input_fill).abs() < 1e-6);
            assert_eq!(palette.material.blur, Material::ZERVO.blur);
            assert_eq!(palette.material.animation, Material::ZERVO.animation);
            assert_eq!(palette.material.edge, Edge::Hairline);
            assert_eq!(palette.radius(Tier::Card), Material::ZERVO.radius.card);
        }
    }

    /// Composite `over` onto `under` at `alpha`, and report the WCAG ratio of
    /// `ink` against the result.
    fn ratio_on(ink: Color32, under: Color32, over: Color32, alpha: f32) -> f32 {
        let blend = mix(under, over, alpha.clamp(0.0, 1.0));
        contrast(ink, f32::from(luminance_of(blend)) / 255.0)
    }

    /// Every arrangement, in both themes, with every accent, has to stay
    /// readable.
    ///
    /// Turn 5 of the study raises the accent ratio from 0.045 to 0.3 and says
    /// so plainly: "All of this is contrast you are spending." It names the
    /// two workspace colours it expects to be worst. This is that warning made
    /// into something that fails a build instead of a screenshot — because the
    /// combination that breaks is a *product* of preset, theme and accent, and
    /// nobody is going to look at all hundred of them.
    ///
    /// The thresholds are WCAG's, one step down for the muted tier: body text
    /// at 4.5, and the caption colour beside it at 3.0, which is the large-text
    /// bar and is what that tier is used for.
    #[test]
    #[expect(
        clippy::print_stdout,
        reason = "the headroom is the useful half of this test — which combination is \
                  closest to the line is what somebody moving a ratio needs to know, and \
                  it is not a failure so it cannot be said in an assertion"
    )]
    fn every_arrangement_stays_readable() {
        let mut worst = (f32::INFINITY, String::new());
        for preset in Preset::ALL {
            for (mode, dark) in [(ThemeMode::Dark, true), (ThemeMode::Light, false)] {
                // The presets' own accents, plus the workspace colours, which
                // become the accent outright when the reader asks the space to
                // pick it.
                let accents = AccentColor::PRESETS.into_iter().chain(
                    WORKSPACE_COLORS
                        .into_iter()
                        .map(|c| AccentColor::Custom(c.r(), c.g(), c.b())),
                );
                for accent in accents {
                    let palette = resolve(mode, dark, accent, &preset.appearance());
                    let card = palette.tint_for(Surface::Card);
                    let checks = [
                        (
                            "text on the chrome",
                            palette.text,
                            palette.bg,
                            palette.bg,
                            1.0,
                            4.5,
                        ),
                        (
                            "text on a card",
                            palette.text,
                            palette.bg,
                            palette.surface,
                            card,
                            4.5,
                        ),
                        (
                            // Whichever ink the row itself would use, which is
                            // the point of `Palette::ink_on` — asserting on
                            // `palette.text` here would test a colour no call
                            // site draws on a selected row.
                            "text on the selected row",
                            palette.ink_on(palette.active).0,
                            palette.bg,
                            palette.active,
                            1.0,
                            4.5,
                        ),
                        (
                            "muted text on the chrome",
                            palette.text_muted,
                            palette.bg,
                            palette.bg,
                            1.0,
                            3.0,
                        ),
                        // The two semantic colours are icon tints and short
                        // labels, held to WCAG's non-text bar: below 3.0 an
                        // amber warning badge beside grey text is a decoration
                        // rather than a warning.
                        (
                            "a warning on the chrome",
                            palette.warning,
                            palette.bg,
                            palette.bg,
                            1.0,
                            3.0,
                        ),
                        (
                            "a success on the chrome",
                            palette.success,
                            palette.bg,
                            palette.bg,
                            1.0,
                            3.0,
                        ),
                        (
                            "a danger on the chrome",
                            palette.danger,
                            palette.bg,
                            palette.bg,
                            1.0,
                            3.0,
                        ),
                        (
                            "information on the chrome",
                            palette.info,
                            palette.bg,
                            palette.bg,
                            1.0,
                            3.0,
                        ),
                    ];
                    for (what, ink, under, over, alpha, floor) in checks {
                        let ratio = ratio_on(ink, under, over, alpha);
                        if ratio < worst.0 {
                            worst = (
                                ratio,
                                format!("{} / {mode:?} / {accent:?}: {what}", preset.label()),
                            );
                        }
                        assert!(
                            ratio >= floor,
                            "{} / {mode:?} / {accent:?}: {what} is {ratio:.2}, under {floor}",
                            preset.label()
                        );
                    }
                }
            }
        }
        // Printed rather than asserted on: it is the headroom, and knowing
        // which combination is closest to the line is the useful part when
        // somebody moves a ratio.
        println!("closest to the line: {} at {:.2}", worst.1, worst.0);
    }

    /// Full-page mode has no chrome to be apart from, so the page takes the
    /// window: no gap in any arrangement, and the window's own corner on all
    /// four sides rather than the two the closed seam leaves round.
    #[test]
    fn a_page_with_the_window_to_itself_has_no_gap_in_any_arrangement() {
        for preset in Preset::ALL {
            let framed = resolve(
                ThemeMode::Dark,
                true,
                AccentColor::Lavender,
                &preset.appearance(),
            );
            let whole = framed.filling_window(true);

            assert_eq!(
                content_margin(&whole),
                0.0,
                "{} keeps a gap in full page",
                preset.label()
            );
            let corners = content_corners(&whole);
            let radius = window_radius(&whole) as u8;
            assert_eq!(
                corners,
                CornerRadius::same(radius),
                "{}'s full-page corners are not the window's",
                preset.label()
            );
            // And the framed case is untouched: three of the five carry a gap
            // and this must not have quietly removed it.
            if !framed.appearance.seam.closes_gap() {
                assert_eq!(
                    content_margin(&framed),
                    framed.appearance.gap,
                    "{} lost its gap outside full page",
                    preset.label()
                );
            }
        }
    }

    /// The window's corner is the platform's, and does not answer to the corner
    /// scale — painting a different radius at the same corner does not replace
    /// the platform's arc, it puts a second one beside it.
    #[test]
    fn the_windows_corner_does_not_move_with_the_scale() {
        let Some(native) = platform_window_radius() else {
            // Nothing to hold it to; the ladder is the only answer here.
            return;
        };
        let mut widest = f32::NEG_INFINITY;
        let mut narrowest = f32::INFINITY;
        for scale in [0.3_f32, 1.0, 1.8] {
            let mut appearance = Preset::Zervo.appearance();
            appearance.corners = scale;
            let palette = resolve(ThemeMode::Dark, true, AccentColor::Lavender, &appearance);
            let radius = window_radius(&palette);
            assert_eq!(radius, native, "the corner scale moved the window's corner");
            widest = widest.max(radius);
            narrowest = narrowest.min(radius);
        }
        assert_eq!(widest, narrowest);
    }

    /// The corner scale has to move the whole ladder and keep its order, or a
    /// pill ends up rounder than the window it is in.
    #[test]
    fn the_corner_scale_keeps_the_ladder_in_order() {
        for scale in [0.0_f32, 0.3, 1.0, 1.35, 1.8, 2.0] {
            let radii = Material::ZERVO.radius.scaled(scale);
            let rungs = [
                radii.hairline,
                radii.control,
                radii.row,
                radii.card,
                radii.panel,
                radii.pill,
                radii.window,
            ];
            for pair in rungs.windows(2) {
                assert!(
                    pair[0] <= pair[1],
                    "at x{scale} the ladder went backwards: {rungs:?}"
                );
            }
        }
        assert_eq!(
            Material::ZERVO.radius.scaled(1.0).card,
            Material::ZERVO.radius.card
        );
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
        let card = resolve(
            ThemeMode::Dark,
            true,
            AccentColor::Lavender,
            &Appearance::classic(),
        )
        .with_translucency(Frosted)
        .tint_for(Surface::Card);
        assert!(
            card > Frosted.chrome() * 2.0,
            "surfaces need more tint than the chrome they sit on"
        );
    }

    /// The classes have to stay in their hierarchy. What that hierarchy *is*
    /// took a correction: a panel is glass like a card, not something heavier,
    /// because both of them sit on a blur.
    #[test]
    fn a_panel_is_glass_like_a_card_and_an_input_is_heavier() {
        let palette = resolve(
            ThemeMode::Dark,
            true,
            AccentColor::Lavender,
            &Appearance::classic(),
        )
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
        let palette = resolve(
            ThemeMode::Dark,
            true,
            AccentColor::Lavender,
            &Appearance::classic(),
        )
        .with_translucency(Translucency::Solid);
        for class in [Surface::Card, Surface::Menu, Surface::Input] {
            assert_eq!(palette.tint_for(class), 1.0);
        }
    }

    /// Every tier has to resolve to something; a material that forgot one
    /// would give a surface square corners with no other sign.
    #[test]
    fn every_radius_tier_resolves() {
        let radii = Material::ZERVO.radius;
        for tier in [
            Tier::Hairline,
            Tier::Control,
            Tier::Row,
            Tier::Card,
            Tier::Panel,
            Tier::Pill,
            Tier::Window,
        ] {
            assert!(radii.of(tier) > 0, "{tier:?} resolved to nothing");
        }
    }
}
