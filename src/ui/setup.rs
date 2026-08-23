//! First run.
//!
//! Four questions and about a minute. No account, no import wizard, no
//! telemetry consent — every answer writes one field that already exists in
//! `Settings`, so this is the same code path as the Appearance page rather
//! than a parallel one, and anything answered here can be answered again
//! later in the same words.
//!
//! ## Why the limitations screen is the point
//!
//! Most onboarding sells. This one sets expectations, because the alternative
//! is worse for exactly the reader it is trying to keep: somebody who meets a
//! black video player and no explanation concludes the browser is broken, and
//! somebody who was told concludes the engine is young. Same fact, opposite
//! outcome — and only one of the two comes back.
//!
//! ## Why the material comes first
//!
//! It is the one question whose answer you can see while you are answering
//! it. Each card is drawn with a palette resolved from that preset, so the
//! choice is made by looking rather than by reading five descriptions of
//! glass.

use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Frame, Id, Rect, RichText, Sense, Stroke,
    TextEdit, Ui, pos2, vec2,
};

use crate::glass::{self, Glass};
use crate::icons::{self, Icon};
use crate::settings::Layout;
use crate::theme::{self, Palette, Preset, Surface, Tier};

use super::{ChromeContext, UiAction};

/// The steps, in order. The first and last are not questions, which is why the
/// counter says four rather than six.
const STEPS: u8 = 6;
/// A full-window change is allowed to take longer than a hover. The chrome's
/// own settle time would be a cut at this size.
const SLIDE: f32 = 0.42;
/// How far a panel is offset when it is one step away.
const SHIFT: f32 = 60.0;

/// Where the setup has got to, and which way it is going.
fn step(ctx: &egui::Context) -> f32 {
    ctx.animate_value_with_time(Id::new("zervo_setup_at"), target(ctx) as f32, SLIDE)
}

fn target(ctx: &egui::Context) -> u8 {
    ctx.data(|data| data.get_temp::<u8>(Id::new("zervo_setup_step")))
        .or_else(crate::shot::setup_step)
        .unwrap_or(0)
        .min(STEPS - 1)
}

fn go(ctx: &egui::Context, to: u8) {
    ctx.data_mut(|data| data.insert_temp(Id::new("zervo_setup_step"), to.min(STEPS - 1)));
}

/// The whole setup, or nothing at all.
///
/// Returns whether it took the frame. It takes the frame completely: no
/// chrome, no page, nothing behind it to click on by accident.
pub(crate) fn draw(root: &mut Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>) -> bool {
    if chrome.settings.seen_setup {
        return false;
    }
    let ctx = root.ctx().clone();
    let window = ctx.content_rect();
    let palette = chrome.palette;
    let accent = theme::workspace_color_from(0, chrome.settings.space_colour);

    // ── The ground. Retints toward the chosen space as soon as it is chosen,
    // so step two's answer is visible behind step three.
    let painter = root.painter();
    painter.rect_filled(window, CornerRadius::ZERO, palette.bg);
    for step in 0..24 {
        let t = step as f32 / 23.0;
        painter.rect_filled(
            Rect::from_min_max(
                pos2(window.min.x, window.min.y + window.height() * t),
                pos2(
                    window.max.x,
                    window.min.y + window.height() * (t + 1.0 / 23.0),
                ),
            ),
            CornerRadius::ZERO,
            theme::mix(
                theme::mix(palette.bg, accent, 0.30),
                theme::mix(palette.bg, Color32::BLACK, 0.25),
                t,
            ),
        );
    }

    let at = step(&ctx);
    let now = target(&ctx);
    // Only the neighbours are drawn. A panel two steps away is off screen and
    // has nothing to contribute but widget ids.
    for index in 0..STEPS {
        let offset = index as f32 - at;
        if offset.abs() >= 1.0 {
            continue;
        }
        let here = index == now;
        // Sixty points, not six hundred. A panel that crosses most of the
        // window on its way past reads as a slide *show*; the design moves
        // each one a short distance and lets the opacity do the rest, which
        // is why it can afford 0.42s without feeling slow.
        let body = Rect::from_min_max(
            pos2(window.min.x, window.min.y),
            pos2(window.max.x, window.max.y - 78.0),
        )
        .translate(vec2(-offset * SHIFT, 0.0));
        let mut ui = root.new_child(
            egui::UiBuilder::new()
                .max_rect(body)
                .layout(egui::Layout::top_down(egui::Align::Center)),
        );
        ui.multiply_opacity((1.0 - offset.abs()).clamp(0.0, 1.0));
        if !here {
            // Halfway past is not something to answer.
            ui.disable();
        }
        panel(&mut ui, chrome, actions, index, body, accent);
    }

    footer(root, chrome, actions, window, now, accent);
    true
}

fn panel(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    index: u8,
    body: Rect,
    accent: Color32,
) {
    match index {
        0 => welcome(ui, chrome, body, accent),
        1 => material(ui, chrome, actions, body),
        2 => space(ui, chrome, actions, body, accent),
        3 => layout(ui, chrome, actions, body),
        4 => limits(ui, chrome, body),
        _ => ready(ui, chrome, actions, body, accent),
    }
}

/// The kicker, title and subtitle every question shares.
fn heading(ui: &mut Ui, palette: &Palette, accent: Color32, kicker: &str, title: &str, sub: &str) {
    ui.label(
        RichText::new(kicker.to_uppercase())
            .size(10.5)
            .strong()
            .color(accent),
    );
    ui.add_space(14.0);
    ui.label(RichText::new(title).size(34.0).color(palette.text));
    ui.add_space(12.0);
    ui.allocate_ui_with_layout(
        vec2(520.0, 0.0),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.label(RichText::new(sub).size(13.5).color(palette.text_muted));
        },
    );
    ui.add_space(26.0);
}

fn welcome(ui: &mut Ui, chrome: &ChromeContext, body: Rect, accent: Color32) {
    let palette = chrome.palette;
    ui.add_space((body.height() - 330.0).max(0.0) * 0.5);
    let (icon, _) = ui.allocate_exact_size(vec2(112.0, 112.0), Sense::hover());
    ui.painter().rect_filled(
        icon,
        CornerRadius::same(palette.radius(Tier::Window) * 2),
        theme::mix(palette.bg, accent, 0.35),
    );
    super::draw_zervo_mark(ui.painter(), icon.center(), 52.0, palette.text);
    ui.add_space(30.0);
    ui.label(RichText::new("Zervo").size(52.0).color(palette.text));
    ui.add_space(16.0);
    ui.allocate_ui_with_layout(
        vec2(440.0, 0.0),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.label(
                RichText::new(
                    "A calm, workspace-oriented browser on Servo — an independent engine, \
                     written in Rust. No Chromium underneath, and no Gecko.",
                )
                .size(15.0)
                .color(palette.text_muted),
            );
        },
    );
}

fn material(ui: &mut Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>, body: Rect) {
    let palette = chrome.palette;
    ui.add_space((body.height() - 340.0).max(0.0) * 0.5);
    heading(
        ui,
        &palette,
        theme::workspace_color_from(0, chrome.settings.space_colour),
        "Step 1 of 4",
        "Pick how it should feel",
        "This is one material, and every surface in Zervo is drawn through it. Zervo's own \
         is the one that is finished and settled — the other four came out of a design \
         study and are worth your time. Nothing here is a decision you are stuck with: \
         every field is on the Appearance page afterwards, one at a time.",
    );
    ui.horizontal_wrapped(|ui| {
        // Centred by hand: `horizontal_wrapped` starts at the left edge, and a
        // row of five cards hugging one side of a window this wide reads as a
        // mistake rather than as a choice.
        let width = 150.0 * 5.0 + ui.spacing().item_spacing.x * 4.0;
        ui.add_space(((ui.available_width() - width) * 0.5).max(0.0));
        for preset in Preset::ALL {
            let chosen = chrome.settings.appearance.preset == Some(preset);
            if preset_card(ui, &palette, preset, chosen).clicked() && !chosen {
                chrome.settings.appearance = preset.appearance();
                actions.push(UiAction::SettingsChanged);
            }
        }
    });
}

/// One preset, drawn as itself.
///
/// The swatch is a palette resolved from that preset rather than a picture of
/// one, so the row is five materials and not five descriptions of glass.
fn preset_card(ui: &mut Ui, palette: &Palette, preset: Preset, chosen: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(150.0, 132.0), Sense::click());
    let lit = glass::ease_out(ui.ctx().animate_bool_with_time(
        response.id,
        chosen,
        palette.material.animation,
    ));
    glass::paint(
        ui.painter(),
        rect.translate(vec2(0.0, -6.0 * lit)),
        palette,
        Glass::tier(Tier::Card)
            .strength(0.55 + 0.45 * lit)
            .glow(palette.appearance.glow * lit),
    );
    let rect = rect.translate(vec2(0.0, -6.0 * lit));

    let mine = theme::resolve(
        chrome_theme(palette),
        palette.dark,
        theme::AccentColor::Lavender,
        &preset.appearance(),
    );
    let swatch = Rect::from_min_size(rect.min + vec2(12.0, 19.0), vec2(126.0, 56.0));
    preset_swatch(ui, &mine, swatch);

    ui.painter().text(
        pos2(rect.center().x, rect.min.y + 90.0),
        Align2::CENTER_CENTER,
        preset.label(),
        FontId::proportional(13.0),
        palette.text,
    );
    ui.painter().text(
        pos2(rect.center().x, rect.min.y + 110.0),
        Align2::CENTER_CENTER,
        preset_blurb(preset),
        FontId::proportional(10.5),
        palette.text_muted,
    );
    // Said on the card as well as in the paragraph above it. Somebody meeting
    // five equally-confident options has no way to tell which one the rest of
    // the browser was actually built and tested against, and finding out by
    // choosing wrong is a poor first ten minutes.
    if preset == Preset::Zervo {
        let tag = Rect::from_center_size(pos2(rect.center().x, rect.min.y + 8.0), vec2(78.0, 15.0));
        ui.painter().rect_filled(
            tag,
            CornerRadius::same((tag.height() * 0.5) as u8),
            palette.accent.gamma_multiply(0.22),
        );
        ui.painter().text(
            tag.center(),
            Align2::CENTER_CENTER,
            "RECOMMENDED",
            FontId::proportional(8.0),
            theme::mix(palette.text, palette.accent, 0.5),
        );
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// What each preset is, in four words. The long form is its `note`, which the
/// Appearance page has room for and a 150pt card does not.
fn preset_blurb(preset: Preset) -> &'static str {
    match preset {
        Preset::Zervo => "Quiet, settled, finished",
        Preset::Candy => "Frosted, lit, colourful",
        Preset::Flat => "No blur, no sheen, square",
        Preset::LiquidGlass => "Translucent, unblurred",
        Preset::Material => "Generous corners, no glass",
    }
}

fn chrome_theme(palette: &Palette) -> theme::ThemeMode {
    if palette.dark {
        theme::ThemeMode::Dark
    } else {
        theme::ThemeMode::Light
    }
}

/// A card, an input and a strip of chrome, small. Between them they show the
/// fill, the sheen, the edge and the corners, which is the whole of what
/// separates one preset from another.
fn preset_swatch(ui: &Ui, palette: &Palette, rect: Rect) {
    let painter = ui.painter().with_clip_rect(rect);
    let radius = CornerRadius::same(palette.radius(Tier::Card));
    painter.rect_filled(rect, radius, theme::mix(palette.bg, palette.accent, 0.45));
    let chrome = Rect::from_min_max(rect.min, pos2(rect.min.x + 34.0, rect.max.y));
    painter.rect_filled(chrome, radius, palette.bg.gamma_multiply(0.85));
    glass::paint(
        &painter,
        Rect::from_min_size(chrome.min + vec2(4.0, 6.0), vec2(26.0, 9.0)),
        palette,
        Glass::of(Surface::Input).no_shadow(),
    );
    glass::paint(
        &painter,
        Rect::from_min_size(chrome.min + vec2(4.0, 19.0), vec2(26.0, 9.0)),
        palette,
        Glass::tier(Tier::Row)
            .tint(palette.active)
            .glow(palette.appearance.glow)
            .no_shadow(),
    );
    glass::paint(
        &painter,
        Rect::from_min_size(
            pos2(chrome.max.x + 8.0, rect.min.y + 12.0),
            vec2(74.0, 20.0),
        ),
        palette,
        Glass::of(Surface::Card),
    );
    glass::paint(
        &painter,
        Rect::from_min_size(
            pos2(chrome.max.x + 8.0, rect.min.y + 38.0),
            vec2(74.0, 14.0),
        ),
        palette,
        Glass::of(Surface::Input),
    );
}

fn space(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    body: Rect,
    accent: Color32,
) {
    let palette = chrome.palette;
    ui.add_space((body.height() - 300.0).max(0.0) * 0.5);
    heading(
        ui,
        &palette,
        accent,
        "Step 2 of 4",
        "Name your first space",
        "Tabs live in named spaces. Its colour can become the window's colour, so you can \
         tell where you are before you read anything.",
    );
    ui.allocate_ui_with_layout(
        vec2(320.0, 0.0),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            let field = Rect::from_min_size(ui.cursor().min, vec2(320.0, 40.0));
            glass::paint(ui.painter(), field, &palette, Glass::of(Surface::Input));
            let mut editor = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(field.shrink2(vec2(14.0, 0.0)))
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            if editor
                .add(
                    TextEdit::singleline(&mut chrome.settings.first_space)
                        .frame(Frame::NONE)
                        .font(FontId::proportional(15.0))
                        .text_color(palette.text)
                        .hint_text(RichText::new("Home").color(palette.text_muted))
                        .desired_width(f32::INFINITY),
                )
                .lost_focus()
            {
                actions.push(UiAction::SettingsChanged);
            }
            ui.advance_cursor_after_rect(field);
            ui.add_space(18.0);

            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - 5.0 * 34.0) * 0.5).max(0.0));
                for index in 0..theme::WORKSPACE_COLORS.len() {
                    let chosen = chrome.settings.space_colour == index;
                    let (dot, response) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
                    let lit = glass::ease_out(ui.ctx().animate_bool_with_time(
                        response.id,
                        chosen,
                        palette.material.animation,
                    ));
                    let colour = theme::workspace_color(index);
                    if lit > 0.0 {
                        ui.painter().add(glass::shadow(
                            Rect::from_center_size(dot.center(), vec2(22.0, 22.0)),
                            11.0,
                            colour.gamma_multiply(0.7 * lit),
                            10.0,
                            glass::Inner::Under,
                        ));
                    }
                    ui.painter()
                        .circle_filled(dot.center(), 9.0 + 2.0 * lit, colour);
                    if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                        chrome.settings.space_colour = index;
                        actions.push(UiAction::SettingsChanged);
                    }
                }
            });
            ui.add_space(14.0);
            ui.label(
                RichText::new("You can add more later, and drag tabs between them.")
                    .size(11.5)
                    .color(palette.text_muted),
            );
        },
    );
}

fn layout(ui: &mut Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>, body: Rect) {
    let palette = chrome.palette;
    ui.add_space((body.height() - 300.0).max(0.0) * 0.5);
    heading(
        ui,
        &palette,
        theme::workspace_color_from(0, chrome.settings.space_colour),
        "Step 3 of 4",
        "Where should the chrome live?",
        "All three are always available — ⌘S switches between them at any time. This is \
         just where you start.",
    );
    ui.horizontal(|ui| {
        let width = 170.0 * 3.0 + ui.spacing().item_spacing.x * 2.0;
        ui.add_space(((ui.available_width() - width) * 0.5).max(0.0));
        for option in Layout::ALL {
            let chosen = chrome.settings.layout == option;
            let (rect, response) = ui.allocate_exact_size(vec2(170.0, 130.0), Sense::click());
            let lit = glass::ease_out(ui.ctx().animate_bool_with_time(
                response.id,
                chosen,
                palette.material.animation,
            ));
            glass::paint(
                ui.painter(),
                rect,
                &palette,
                Glass::tier(Tier::Card)
                    .strength(0.55 + 0.45 * lit)
                    .glow(palette.appearance.glow * lit),
            );
            layout_sketch(
                ui,
                &palette,
                Rect::from_min_size(rect.min + vec2(14.0, 14.0), vec2(142.0, 62.0)),
                option,
            );
            ui.painter().text(
                pos2(rect.center().x, rect.min.y + 92.0),
                Align2::CENTER_CENTER,
                option.label(),
                FontId::proportional(13.0),
                palette.text,
            );
            ui.painter().text(
                pos2(rect.center().x, rect.min.y + 111.0),
                Align2::CENTER_CENTER,
                option.note(),
                FontId::proportional(10.5),
                palette.text_muted,
            );
            if response.on_hover_cursor(CursorIcon::PointingHand).clicked() && !chosen {
                chrome.settings.layout = option;
                actions.push(UiAction::SettingsChanged);
            }
        }
    });
}

/// Three rectangles that say where the chrome is. Anything more detailed at
/// this size is a picture of a browser rather than a diagram of one.
fn layout_sketch(ui: &Ui, palette: &Palette, rect: Rect, option: Layout) {
    let painter = ui.painter();
    let radius = CornerRadius::same(palette.radius(Tier::Control));
    painter.rect_filled(rect, radius, palette.bg.gamma_multiply(0.6));
    let ink = palette.text_muted.gamma_multiply(0.55);
    match option {
        Layout::Sidebar => painter.rect_filled(
            Rect::from_min_max(rect.min, pos2(rect.min.x + 40.0, rect.max.y)),
            radius,
            ink,
        ),
        Layout::Bar => painter.rect_filled(
            Rect::from_min_max(rect.min, pos2(rect.max.x, rect.min.y + 14.0)),
            radius,
            ink,
        ),
        Layout::FullPage => painter.rect_filled(
            Rect::from_min_size(rect.min + vec2(3.0, 12.0), vec2(3.0, rect.height() - 24.0)),
            palette.corner(Tier::Hairline),
            palette.accent,
        ),
    };
}

fn limits(ui: &mut Ui, chrome: &ChromeContext, body: Rect) {
    let palette = chrome.palette;
    ui.add_space((body.height() - 330.0).max(0.0) * 0.5);
    heading(
        ui,
        &palette,
        theme::workspace_color_from(0, chrome.settings.space_colour),
        "Step 4 of 4",
        "Before you start",
        "Servo is young. These are real, they are the engine rather than Zervo, and you \
         will meet them.",
    );
    let rows: [(Icon, &str, &str); 4] = [
        (
            Icon::Music,
            "Streaming video will not play",
            "No Media Source Extensions. Local and progressive video is fine.",
        ),
        (
            Icon::Extensions,
            "No extensions",
            "The engine has none, so neither does the button.",
        ),
        (
            Icon::Lock,
            "Passwords cannot fill themselves in",
            "Kept in your keychain, but the engine gives no hook for a submitted form.",
        ),
        (
            Icon::Warning,
            "Some sites just break",
            "Missing APIs, or a refused user agent. Zervo will tell you which.",
        ),
    ];
    ui.allocate_ui_with_layout(
        vec2(560.0, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            for (icon, title, body) in rows {
                let (rect, _) = ui.allocate_exact_size(vec2(560.0, 54.0), Sense::hover());
                glass::paint(
                    ui.painter(),
                    rect,
                    &palette,
                    Glass::tier(Tier::Card).strength(0.5).no_shadow(),
                );
                icons::draw_icon(
                    ui.painter(),
                    Rect::from_center_size(
                        pos2(rect.min.x + 22.0, rect.center().y),
                        vec2(15.0, 15.0),
                    ),
                    icon,
                    palette.text_muted,
                );
                ui.painter().text(
                    pos2(rect.min.x + 42.0, rect.center().y - 9.0),
                    Align2::LEFT_CENTER,
                    title,
                    FontId::proportional(13.0),
                    palette.text,
                );
                ui.painter().text(
                    pos2(rect.min.x + 42.0, rect.center().y + 9.0),
                    Align2::LEFT_CENTER,
                    body,
                    FontId::proportional(11.5),
                    palette.text_muted,
                );
                ui.add_space(8.0);
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Keeping another browser around is sensible. Zervo will offer to hand a \
                     page over rather than pretend.",
                )
                .size(11.5)
                .color(palette.text_muted),
            );
        },
    );
}

fn ready(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    body: Rect,
    accent: Color32,
) {
    let palette = chrome.palette;
    let name = if chrome.settings.first_space.trim().is_empty() {
        "Home".to_owned()
    } else {
        chrome.settings.first_space.trim().to_owned()
    };
    ui.add_space((body.height() - 260.0).max(0.0) * 0.5);
    let (mark, _) = ui.allocate_exact_size(vec2(60.0, 60.0), Sense::hover());
    ui.painter().rect_filled(
        mark,
        CornerRadius::same(palette.radius(Tier::Window)),
        theme::mix(palette.bg, accent, 0.45),
    );
    icons::draw_icon(
        ui.painter(),
        Rect::from_center_size(mark.center(), vec2(26.0, 26.0)),
        Icon::Check,
        palette.text,
    );
    ui.add_space(24.0);
    ui.label(
        RichText::new(format!("{name} is ready"))
            .size(34.0)
            .color(palette.text),
    );
    ui.add_space(12.0);
    ui.allocate_ui_with_layout(
        vec2(520.0, 0.0),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.label(
                RichText::new(format!(
                    "{} material, the {}, and a colour. Everything here is in Settings → \
                     Appearance if you change your mind.",
                    chrome.settings.appearance.preset_label(),
                    chrome.settings.layout.label().to_lowercase(),
                ))
                .size(13.5)
                .color(palette.text_muted),
            );
        },
    );
    ui.add_space(28.0);
    ui.horizontal(|ui| {
        ui.add_space(((ui.available_width() - 300.0) * 0.5).max(0.0));
        if pill(ui, &palette, "Start over", false).clicked() {
            go(ui.ctx(), 0);
        }
        ui.add_space(10.0);
        if pill(ui, &palette, "Start browsing", true).clicked() {
            finish(chrome, actions);
        }
    });
}

/// The setup's own button. Not `widgets::` — nothing else in the application
/// has a button this size, and giving it one would be inventing a control for
/// a screen that is seen once.
fn pill(ui: &mut Ui, palette: &Palette, label: &str, filled: bool) -> egui::Response {
    let text =
        ui.painter()
            .layout_no_wrap(label.to_owned(), FontId::proportional(14.0), palette.text);
    let (rect, response) = ui.allocate_exact_size(vec2(text.size().x + 46.0, 42.0), Sense::click());
    let hover = glass::ease_out(ui.ctx().animate_bool_with_time(
        response.id,
        response.hovered(),
        0.12,
    ));
    if filled {
        glass::paint(
            ui.painter(),
            rect,
            palette,
            Glass::tier(Tier::Pill)
                .tint(palette.active)
                .strength(0.85 + 0.15 * hover)
                .glow(0.4 + 0.3 * hover),
        );
    } else {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(palette.radius(Tier::Pill)),
            Stroke::new(1.0_f32, palette.border),
            egui::StrokeKind::Inside,
        );
        if hover > 0.0 {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(palette.radius(Tier::Pill)),
                palette.surface_hover.gamma_multiply(hover * 0.7),
            );
        }
    }
    ui.painter().galley(
        pos2(
            rect.center().x - text.size().x * 0.5,
            rect.center().y - text.size().y * 0.5,
        ),
        text,
        palette.text,
    );
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Back, the dots, and forward.
fn footer(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    window: Rect,
    now: u8,
    accent: Color32,
) {
    let palette = chrome.palette;
    let bar = Rect::from_min_max(pos2(window.min.x, window.max.y - 78.0), window.max);
    let mut ui = root.new_child(egui::UiBuilder::new().max_rect(bar));

    // The dots, always — they say how long this is, which is most of what
    // makes a reader willing to start it.
    for index in 0..STEPS {
        let on = index == now;
        let width = if on { 18.0 } else { 6.0 };
        let x = bar.center().x - (STEPS as f32 * 12.0) / 2.0 + index as f32 * 12.0;
        let dot = Rect::from_center_size(pos2(x, bar.center().y), vec2(width, 6.0));
        let response = ui.interact(dot, Id::new("zervo_setup_dot").with(index), Sense::click());
        ui.painter().rect_filled(
            dot,
            // Half the dot's own height. A step marker is a capsule that
            // stretches when it is the current one; the ladder has nothing to
            // say about it.
            CornerRadius::same((dot.height() * 0.5) as u8),
            if on {
                accent
            } else {
                palette.text_muted.gamma_multiply(0.35)
            },
        );
        if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            go(&ui.ctx().clone(), index);
        }
    }

    // The welcome screen has one button and it is in the middle of the panel;
    // a Back and a Next under it would be two ways to do the same nothing.
    if now == 0 {
        let mut middle = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_center_size(
                    pos2(bar.center().x, bar.min.y - 46.0),
                    vec2(bar.width(), 44.0),
                ))
                .layout(egui::Layout::top_down(egui::Align::Center)),
        );
        if pill(&mut middle, &palette, "Set it up", true).clicked() {
            go(&ui.ctx().clone(), 1);
        }
        middle.label(
            RichText::new("Four questions. About a minute.")
                .size(11.5)
                .color(palette.text_muted),
        );
        return;
    }

    let mut left = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(
                pos2(bar.min.x + 26.0, bar.center().y - 21.0),
                vec2(200.0, 42.0),
            ))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    if pill(&mut left, &palette, "Back", false).clicked() {
        go(&ui.ctx().clone(), now.saturating_sub(1));
    }

    if now + 1 < STEPS {
        let mut right = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_size(
                    pos2(bar.max.x - 226.0, bar.center().y - 21.0),
                    vec2(200.0, 42.0),
                ))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        let last = now + 2 == STEPS;
        if pill(
            &mut right,
            &palette,
            if last { "Finish" } else { "Next" },
            true,
        )
        .clicked()
        {
            go(&ui.ctx().clone(), now + 1);
        }
    }
    let _ = actions;
}

/// Done. The one field that is not a preference: it is how the browser knows
/// not to ask again.
fn finish(chrome: &mut ChromeContext, actions: &mut Vec<UiAction>) {
    chrome.settings.seen_setup = true;
    let name = chrome.settings.first_space.trim().to_owned();
    chrome.settings.first_space = if name.is_empty() {
        "Home".to_owned()
    } else {
        name
    };
    if let Some(first) = chrome.browser.workspaces.first_mut() {
        first.name = chrome.settings.first_space.clone();
    }
    actions.push(UiAction::SettingsChanged);
}
