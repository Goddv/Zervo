use super::*;

const MAX_ROWS: usize = 7;
const ROW_HEIGHT: f32 = 36.0;
const ROW_GAP: f32 = 5.0;
const CAPTION_HEIGHT: f32 = 24.0;
const FIELD_HEIGHT: f32 = 44.0;
const PADDING: i8 = 14;

#[derive(Debug, Clone, Copy)]
pub enum CommandMode {
    NewTab,
    CurrentTab,
}

pub struct CommandBar {
    mode: CommandMode,
    query: String,
    selection: usize,
    select_all: bool,
    transition: Option<Transition>,
}

struct Transition {
    origin: Rect,
    opened_at: Option<f64>,
    closing: Option<(f64, f32)>,
}

impl Transition {
    fn progress(&mut self, now: f64, duration: f32) -> f32 {
        if duration <= 0.0 {
            return if self.closing.is_some() { 0.0 } else { 1.0 };
        }
        let opened = *self.opened_at.get_or_insert(now);
        let (started, elapsed) = if let Some((closed, from)) = self.closing {
            (from, -((now - closed) as f32))
        } else {
            (0.0, (now - opened) as f32)
        };
        (started + elapsed / duration).clamp(0.0, 1.0)
    }
}

impl CommandBar {
    pub fn new(mode: CommandMode, address: &str) -> Self {
        Self {
            mode,
            query: match mode {
                CommandMode::NewTab => String::new(),
                CommandMode::CurrentTab => address.to_owned(),
            },
            selection: 0,
            select_all: true,
            transition: None,
        }
    }

    pub(crate) fn from_new_tab(origin: Rect) -> Self {
        Self {
            transition: Some(Transition {
                origin,
                opened_at: None,
                closing: None,
            }),
            ..Self::new(CommandMode::CurrentTab, "")
        }
    }

    pub(crate) fn from_home(&self) -> bool {
        self.transition.is_some()
    }
}

struct ResultRow<'a> {
    title: &'a str,
    detail: &'a str,
    icon: Icon,
    action: UiAction,
}

pub(super) fn draw(root: &Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>) -> bool {
    let Some(mut bar) = chrome.browser.command_bar.take() else {
        return false;
    };
    let ctx = root.ctx();
    let palette = chrome.palette;
    let id = Id::unique("zervo_command_bar");
    let window = ctx.content_rect();
    let top = (window.height() * 0.12).clamp(24.0, 56.0);
    let width = (window.width() - 64.0).clamp(1.0, 520.0);
    let now = ctx.input(|input| input.time);
    let duration = palette.material.animation;
    let mut progress = 1.0;
    let mut closing = false;
    let mut area = egui::Modal::default_area(id)
        .anchor(Align2::CENTER_TOP, vec2(0.0, top))
        .fade_in(false);
    let mut field = Rect::from_min_size(
        pos2(
            window.center().x - width * 0.5,
            window.top() + top + f32::from(PADDING) + CAPTION_HEIGHT + ROW_GAP,
        ),
        vec2(width, FIELD_HEIGHT),
    );
    if let Some(transition) = &mut bar.transition {
        progress = transition.progress(now, duration);
        closing = transition.closing.is_some();
        if closing && progress <= 0.0 {
            ctx.request_repaint();
            return false;
        }
        field = lerp_rect(transition.origin, field, crate::glass::ease_out(progress));
    }
    let eased = crate::glass::ease_out(progress);
    let padding = (f32::from(PADDING) * eased).round() as i8;
    let caption_height = CAPTION_HEIGHT * eased;
    let spacing = ROW_GAP * eased;
    if bar.from_home() {
        let margin = f32::from(padding);
        area = egui::Area::new(id)
            .kind(egui::UiKind::Modal)
            .sense(Sense::hover())
            .order(egui::Order::Foreground)
            .fixed_pos(field.min - vec2(margin, margin + caption_height + spacing))
            .constrain(false)
            .fade_in(false);
    }
    let shade = Color32::from_black_alpha(if palette.dark { 105 } else { 53 });
    let width = field.width().max(1.0);
    let modal = egui::Modal::new(id)
        .area(area)
        .backdrop_color(shade.gamma_multiply(eased))
        .frame(Frame::NONE)
        .show(ctx, |ui| {
            let surface = crate::overlay_glass::Surface::begin(ui);
            let mut frame = Frame::NONE.inner_margin(padding);
            if !surface.is_native() {
                frame = frame
                    .fill(palette.surface)
                    .stroke(Stroke::new(1.0_f32, palette.border))
                    .corner_radius(12 + (8.0 * eased).round() as u8)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 14],
                        blur: 38,
                        spread: 0,
                        color: Color32::from_black_alpha(95).gamma_multiply(eased),
                    });
            }
            let card = frame.show(ui, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = spacing;
                if closing {
                    ui.disable();
                }
                let down =
                    ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::ArrowDown));
                let up =
                    ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::ArrowUp));
                let enter =
                    ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Enter));
                let caption = match bar.mode {
                    CommandMode::NewTab => tr("common-new-tab"),
                    CommandMode::CurrentTab => tr("command-current-tab"),
                };
                let (title, _) =
                    ui.allocate_exact_size(vec2(width, caption_height), Sense::hover());
                if eased > 0.35 {
                    let alpha = ((eased - 0.35) / 0.65).clamp(0.0, 1.0);
                    let ink = palette.text_muted.gamma_multiply(alpha);
                    ui.painter().text(
                        title.left_center(),
                        Align2::LEFT_CENTER,
                        caption,
                        FontId::proportional(12.0),
                        ink,
                    );
                    let close = Rect::from_center_size(
                        title.right_center() - vec2(13.0, 0.0),
                        vec2(24.0, caption_height),
                    );
                    let response = ui.interact(close, id.with("close"), Sense::click());
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, true, tr("common-close"))
                    });
                    if response.has_focus() {
                        icons::paint_focus(ui, &palette, close, Tier::Control);
                    }
                    if response.on_hover_text(tr("common-close")).clicked() {
                        ui.close();
                    }
                    icons::draw_icon(
                        ui.painter(),
                        Rect::from_center_size(close.center(), vec2(13.0, 13.0)),
                        Icon::Close,
                        ink,
                    );
                }
                let query_font = FontId::proportional(15.0 + 4.0 * eased);
                let edit = TextEdit::singleline(&mut bar.query)
                    .id(id.with("query"))
                    .font(query_font.clone())
                    .text_color(palette.text)
                    .hint_text(RichText::new(tr("command-search-placeholder")).font(query_font))
                    .align(egui::Align2::LEFT_CENTER)
                    .desired_width(width)
                    .min_size(vec2(0.0, field.height()))
                    .margin(Margin::symmetric(12, 11))
                    .background_color(palette.surface);
                let mut editor = crate::overlay_glass::text_field(ui, edit);
                editor.response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::Role::TextInput,
                        true,
                        tr("command-search-placeholder"),
                    )
                });
                if std::mem::take(&mut bar.select_all) {
                    editor.response.request_focus();
                    editor
                        .state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(bar.query.chars().count()),
                        )));
                    editor.state.store(ctx, editor.response.id);
                }
                if editor.response.changed() {
                    bar.selection = 0;
                }

                let rows = results(bar.query.trim(), bar.mode, chrome);
                ui.add_space(5.0 * eased);
                let max = rows.len();
                bar.selection = match (down, up) {
                    _ if max == 0 => 0,
                    (true, _) => (bar.selection + 1) % max,
                    (_, true) => (bar.selection + max - 1) % max,
                    _ => bar.selection.min(max - 1),
                };
                let mut chosen = None;
                let reveal = ((eased - 0.22) / 0.78).clamp(0.0, 1.0);
                let available = window.bottom() - ui.cursor().top() - f32::from(padding) - 16.0;
                let natural_height =
                    max.max(1) as f32 * ROW_HEIGHT + max.saturating_sub(1) as f32 * spacing;
                let results_height = natural_height.min(available.max(0.0)) * reveal;
                if results_height <= 0.0 {
                    return if enter && editor.response.has_focus() {
                        rows.into_iter().nth(bar.selection).map(|row| row.action)
                    } else {
                        None
                    };
                }
                // An Area starts with its last frame's size. Give the list
                // its own bounds so the folded field cannot cap its height.
                let (bounds, _) =
                    ui.allocate_exact_size(vec2(width, results_height), Sense::hover());
                let mut list = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt("results")
                        .max_rect(bounds)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                list.set_clip_rect(ui.clip_rect().intersect(bounds));
                egui::ScrollArea::vertical()
                    .max_height(results_height)
                    .min_scrolled_height(0.0)
                    .auto_shrink([false, false])
                    .show(&mut list, |ui| {
                        ui.multiply_opacity(reveal);
                        if rows.is_empty() {
                            ui.label(
                                RichText::new(tr("command-no-matches")).color(palette.text_muted),
                            );
                        }
                        for (index, row) in rows.into_iter().enumerate() {
                            let selected = index == bar.selection;
                            let response = result_row(ui, &row, selected, &palette);
                            if selected && (down || up || editor.response.changed()) {
                                response.scroll_to_me(Some(egui::Align::Center));
                            }
                            if response.clicked()
                                || (enter && selected && editor.response.has_focus())
                            {
                                chosen = Some(row.action);
                            }
                        }
                    });
                chosen
            });
            surface.finish(ui, card.response.rect, crate::overlay_glass::Role::Dialog);
            card.inner
        });

    if let Some(action) = modal.inner {
        actions.push(action);
    } else if closing || !modal.should_close() {
        chrome.browser.command_bar = Some(bar);
        if progress < 1.0 || closing {
            ctx.request_repaint();
        }
        return true;
    } else if duration > 0.0
        && progress > 0.0
        && let Some(transition) = &mut bar.transition
    {
        transition.closing = Some((now, progress));
        chrome.browser.command_bar = Some(bar);
        ctx.memory_mut(|memory| memory.surrender_focus(id.with("query")));
        ctx.request_repaint();
        return true;
    }
    ctx.memory_mut(|memory| memory.surrender_focus(id.with("query")));
    ctx.request_repaint();
    false
}

fn results<'a>(query: &'a str, mode: CommandMode, chrome: &'a ChromeContext) -> Vec<ResultRow<'a>> {
    let workspace = chrome.browser.active_workspace;
    let mut results = Vec::with_capacity(MAX_ROWS);
    let command_only = query.starts_with('>');
    let input = query.trim_start_matches('>').trim().to_lowercase();
    let matches = |title: &str, detail: &str| {
        title.to_lowercase().contains(&input) || detail.to_lowercase().contains(&input)
    };
    let open_url = |url: String| match mode {
        CommandMode::NewTab => UiAction::OpenAddress(url),
        CommandMode::CurrentTab => UiAction::Navigate(url),
    };

    if !query.is_empty() && !command_only {
        results.push(ResultRow {
            title: query,
            detail: tr("command-open-or-search"),
            icon: Icon::Search,
            action: open_url(normalize_url(query, chrome.settings.search_engine)),
        });
        let mut addresses = std::collections::HashSet::new();
        for space in &chrome.browser.workspaces {
            for tab in &space.tabs {
                if results.len() >= MAX_ROWS - 1 {
                    break;
                }
                if Some(tab.id) != chrome.browser.active_tab
                    && matches(&tab.title, &tab.url)
                    && addresses.insert(tab.url.as_str())
                {
                    results.push(ResultRow {
                        title: &tab.title,
                        detail: tr("command-switch-tab"),
                        icon: Icon::Browser,
                        action: UiAction::SelectTab(tab.id),
                    });
                }
            }
        }
        for favourite in &chrome.library.favourites {
            if results.len() >= MAX_ROWS - 1 {
                break;
            }
            if matches(&favourite.title, &favourite.url) && addresses.insert(&favourite.url) {
                results.push(ResultRow {
                    title: &favourite.title,
                    detail: tr("common-favourites"),
                    icon: Icon::Star,
                    action: open_url(favourite.url.clone()),
                });
            }
        }
        for visit in chrome.library.history.iter().rev().take(500) {
            if results.len() >= MAX_ROWS - 1 {
                break;
            }
            if matches(&visit.title, &visit.url) && addresses.insert(&visit.url) {
                results.push(ResultRow {
                    title: if visit.title.is_empty() {
                        &visit.url
                    } else {
                        &visit.title
                    },
                    detail: tr("common-history"),
                    icon: Icon::History,
                    action: open_url(visit.url.clone()),
                });
            }
        }
    }
    for (title, icon, action) in [
        (
            tr("command-open-new-tab"),
            Icon::Plus,
            UiAction::NewTab { workspace },
        ),
        (
            tr("command-open-settings"),
            Icon::Gear,
            UiAction::OpenSettings,
        ),
        (
            tr("layout-customize"),
            Icon::Sliders,
            UiAction::CustomizeChrome,
        ),
        (tr("common-history"), Icon::History, UiAction::OpenHistory),
        (
            tr("common-downloads"),
            Icon::Download,
            UiAction::OpenDownloads,
        ),
        (
            tr("chrome-new-workspace"),
            Icon::Stack,
            UiAction::NewWorkspace,
        ),
        (
            tr("command-toggle-compact"),
            Icon::Sidebar,
            UiAction::ToggleSidebar,
        ),
    ] {
        if results.len() < MAX_ROWS && matches(title, "") {
            results.push(ResultRow {
                title,
                detail: "",
                icon,
                action,
            });
        }
    }
    results
}

fn result_row(ui: &mut Ui, row: &ResultRow, selected: bool, palette: &Palette) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, 8, palette.surface_hover);
    }
    if selected {
        ui.painter().rect_filled(
            Rect::from_center_size(pos2(rect.left() + 2.0, rect.center().y), vec2(3.0, 16.0)),
            1,
            palette.accent,
        );
    }
    if response.has_focus() {
        icons::paint_focus(ui, palette, rect, Tier::Control);
    }
    icons::draw_icon(
        ui.painter(),
        Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), vec2(16.0, 16.0)),
        row.icon,
        palette.text_muted,
    );
    let detail = ui.painter().layout_no_wrap(
        row.detail.to_owned(),
        FontId::proportional(11.0),
        palette.text_muted,
    );
    let detail_width = if detail.size().x < rect.width() * 0.4 {
        detail.size().x
    } else {
        0.0
    };
    if detail_width > 0.0 {
        ui.painter().galley(
            pos2(
                rect.right() - detail_width - 10.0,
                rect.center().y - detail.size().y * 0.5,
            ),
            detail,
            palette.text_muted,
        );
    }
    let x = rect.left() + 38.0;
    let mut title = egui::text::LayoutJob::simple_singleline(
        row.title.to_owned(),
        FontId::proportional(13.5),
        palette.text,
    );
    title.wrap = egui::text::TextWrapping::truncate_at_width(
        (rect.right() - x - detail_width - 24.0).max(0.0),
    );
    let title = ui.painter().layout_job(title);
    ui.painter().galley(
        pos2(x, rect.center().y - title.size().y * 0.5),
        title,
        palette.text,
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::ListBoxOption, true, selected, row.title)
    });
    response.on_hover_cursor(CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_morph_reverses_from_its_current_position() {
        let origin = Rect::from_min_size(pos2(50.0, 200.0), vec2(650.0, 54.0));
        let mut bar = CommandBar::from_new_tab(origin);
        let transition = bar.transition.as_mut().unwrap();
        assert_eq!(transition.progress(1.0, 0.25), 0.0);
        let partial = transition.progress(1.125, 0.25);
        assert_eq!(partial, 0.5);
        transition.closing = Some((1.125, partial));
        assert_eq!(transition.progress(1.125, 0.25), partial);
        assert_eq!(transition.progress(1.25, 0.25), 0.0);
        assert_eq!(transition.progress(1.125, 0.0), 0.0);
    }
}
