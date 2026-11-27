//! Pull-to-refresh for the active browser page.

use std::time::{Duration, Instant};

use egui::{LayerId, Order, Rect, Stroke, Ui, pos2, vec2};
use servo::{JSValue, WebViewId};
use winit::event::TouchPhase;

use crate::gestures::GestureAction;
use crate::icons::{self, Icon};
use crate::theme::Palette;
use crate::{RunningApp, WakerEvent};

pub(crate) struct Pending {
    stroke: Instant,
    page: WebViewId,
    url: url::Url,
    at_top: bool,
    released: Option<Instant>,
}

impl RunningApp {
    pub(crate) fn update_pull_refresh(
        &mut self,
        phase: TouchPhase,
        previous_stroke: Option<Instant>,
        previous_progress: f32,
        eligible: bool,
        scale: f32,
    ) {
        if !eligible || self.settings.gestures.down != GestureAction::Reload {
            self.cancel_pull_refresh();
            return;
        }
        let stroke = self.swipe.stroke_start();
        match phase {
            TouchPhase::Started if stroke.is_none() => self.cancel_pull_refresh(),
            TouchPhase::Started | TouchPhase::Moved => {
                let progress = self.swipe.pull_progress();
                if let Some(stroke) = stroke
                    && Some(stroke) != previous_stroke
                    && progress > 0.0
                {
                    self.check_pull_start(stroke, scale);
                }
                if self.pull_refresh.is_some() && progress != previous_progress {
                    self.state.window.request_redraw();
                }
            },
            TouchPhase::Ended => {
                if let Some(pull) = &mut self.pull_refresh
                    && Some(pull.stroke) == previous_stroke
                    && previous_progress >= 1.0
                {
                    pull.released = Some(Instant::now());
                    self.reload_if_pulled();
                    self.state.window.request_redraw();
                } else {
                    self.cancel_pull_refresh();
                }
            },
            TouchPhase::Cancelled => self.cancel_pull_refresh(),
        }
    }

    fn check_pull_start(&mut self, stroke: Instant, scale: f32) {
        let Some(webview) = self.state.active_webview() else {
            return;
        };
        let (Some(position), Some(url)) = (self.cursor, webview.url()) else {
            return;
        };
        let point = self.webview_point(position, scale);
        let zoom = webview.page_zoom() * webview.hidpi_scale_factor().get() * webview.pinch_zoom();
        let (x, y) = (point.x / zoom, point.y / zoom);
        self.pull_refresh = Some(Pending {
            stroke,
            page: webview.id(),
            url,
            at_top: false,
            released: None,
        });
        // Probe before forwarding the first downward wheel event. A scroll
        // that reaches the top in the middle of a stroke remains a scroll.
        let script = format!(
            r#"(() => {{
            const root = document.scrollingElement;
            if (!root || window.scrollY > 1 || root.scrollTop > 1) return false;
            for (let node = document.elementFromPoint({x}, {y}); node; node = node.parentElement) {{
                if (node.tagName === "IFRAME" || node.tagName === "FRAME") return false;
                const style = getComputedStyle(node);
                if (style.overscrollBehaviorY === "contain" || style.overscrollBehaviorY === "none") return false;
                if (node !== root && node.scrollTop > 1 && /^(auto|scroll|overlay)$/.test(style.overflowY)) return false;
            }}
            return true;
        }})()"#
        );
        let proxy = self.animation_waker.0.clone();
        webview.evaluate_javascript(script, move |result| {
            let at_top = matches!(result, Ok(JSValue::Boolean(true)));
            let _ = proxy.send_event(WakerEvent::PullRefreshChecked { stroke, at_top });
        });
    }

    pub(crate) fn resolve_pull_refresh(&mut self, stroke: Instant, at_top: bool) {
        let Some(pull) = &mut self.pull_refresh else {
            return;
        };
        if pull.stroke != stroke {
            return;
        }
        if !at_top {
            self.cancel_pull_refresh();
            return;
        }
        pull.at_top = true;
        self.reload_if_pulled();
        self.state.window.request_redraw();
    }

    fn reload_if_pulled(&mut self) {
        let Some(pull) = &self.pull_refresh else {
            return;
        };
        let Some(released) = pull.released.filter(|_| pull.at_top) else {
            return;
        };
        let matches_page = self
            .state
            .active_webview()
            .is_some_and(|view| view.id() == pull.page && view.url().as_ref() == Some(&pull.url));
        self.pull_refresh = None;
        if matches_page
            && released.elapsed() < Duration::from_millis(500)
            && self.settings.gestures.enabled
            && self.settings.gestures.down == GestureAction::Reload
        {
            self.apply_action(crate::ui::UiAction::Reload);
        }
    }

    pub(crate) fn cancel_pull_refresh(&mut self) {
        if self.pull_refresh.take().is_some() {
            self.state.window.request_redraw();
        }
    }

    pub(crate) fn visible_pull_progress(&self) -> f32 {
        let Some(pull) = &self.pull_refresh else {
            return 0.0;
        };
        if pull.at_top
            && pull.released.is_none()
            && self.settings.gestures.enabled
            && self.settings.gestures.down == GestureAction::Reload
            && !self.controls_open
            && !egui::Popup::is_any_open(&self.egui_glow.egui_ctx)
            && self.state.active_webview().is_some_and(|view| {
                view.id() == pull.page && view.url().as_ref() == Some(&pull.url)
            })
        {
            self.swipe.pull_progress()
        } else {
            0.0
        }
    }
}

pub(crate) fn paint(ui: &Ui, page: Rect, palette: &Palette, progress: f32) {
    if progress <= 0.0 {
        return;
    }
    let center = page.center_top() + vec2(0.0, 20.0 + 30.0 * progress);
    let painter = ui
        .layer_painter(LayerId::new(
            Order::Foreground,
            egui::Id::unique("pull_refresh"),
        ))
        .with_clip_rect(page);
    painter.circle_filled(center, 18.0, palette.surface_fill());
    painter.circle_stroke(center, 18.0, Stroke::new(1.0, palette.border));
    let points = (0..=24)
        .map(|step| {
            let angle = std::f32::consts::TAU * progress * step as f32 / 24.0;
            center + vec2(angle.sin(), -angle.cos()) * 16.0
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(2.0, palette.accent)));
    icons::draw_icon(
        &painter,
        Rect::from_center_size(pos2(center.x, center.y), vec2(17.0, 17.0)),
        Icon::Reload,
        palette.text,
    );
}
