//! Bridge for browser chrome drawn by egui over an AppKit material.
//!
//! Egui owns layout, hit testing, focus, and accessibility. Its overlay ink
//! is painted into a transparent buffer and displayed as the AppKit glass
//! view's content. The window's original framebuffer stays intact beneath
//! the glass, so AppKit samples the actual page and chrome as they move.

use egui::{Context, InnerResponse, Rect, Ui};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Role {
    Menu,
    Panel,
    Sidebar,
    Dialog,
    Status,
    Field,
}

impl Role {
    pub(crate) fn radius(self) -> u8 {
        match self {
            Self::Menu => 14,
            Self::Panel | Self::Sidebar => 18,
            Self::Dialog => 20,
            Self::Status => 10,
            Self::Field => 12,
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn tint(self, dark: bool) -> objc2::rc::Retained<objc2_app_kit::NSColor> {
        let (white, alpha) = match (dark, self) {
            (true, Self::Menu | Self::Status) => (0.045, 0.92),
            (true, Self::Dialog) => (0.055, 0.88),
            (true, Self::Panel | Self::Sidebar) => (0.065, 0.80),
            (true, Self::Field) => (0.12, 0.62),
            (false, Self::Menu | Self::Status) => (0.90, 0.72),
            (false, Self::Dialog) => (0.89, 0.64),
            (false, Self::Panel | Self::Sidebar) => (0.88, 0.56),
            (false, Self::Field) => (0.95, 0.48),
        };
        objc2_app_kit::NSColor::colorWithWhite_alpha(white, alpha)
    }
}

pub(crate) struct Surface {
    #[cfg(target_os = "macos")]
    capture: Option<macos::Capture>,
}

impl Surface {
    pub(crate) fn begin(ui: &Ui) -> Self {
        let _ = ui;
        Self {
            #[cfg(target_os = "macos")]
            capture: macos::Capture::begin(ui),
        }
    }

    pub(crate) fn is_native(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.capture.is_some()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    pub(crate) fn text_edit<'a>(&self, edit: egui::TextEdit<'a>) -> egui::TextEdit<'a> {
        if self.is_native() {
            edit.background_color(egui::Color32::TRANSPARENT)
        } else {
            edit
        }
    }

    pub(crate) fn finish(self, ui: &Ui, rect: Rect, role: Role) {
        self.finish_on(ui.painter(), rect, role);
    }

    fn finish_on(self, painter: &egui::Painter, rect: Rect, role: Role) {
        let _ = (painter, rect, role);
        #[cfg(target_os = "macos")]
        if let Some(capture) = self.capture {
            capture.finish(painter, rect, role);
        }
    }
}

pub(crate) fn text_field(ui: &mut Ui, edit: egui::TextEdit<'_>) -> egui::text_edit::TextEditOutput {
    let surface = Surface::begin(ui);
    let output = surface.text_edit(edit).show(ui);
    surface.finish(ui, output.response.rect, Role::Field);
    output
}

/// Give a fixed-height panel a scrollable body while leaving its header and
/// actions outside the scrolling region.
pub(crate) fn scroll_body<R>(ui: &mut Ui, rect: Rect, add: impl FnOnce(&mut Ui) -> R) -> R {
    let mut body = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    body.set_clip_rect(ui.clip_rect().intersect(rect));
    egui::ScrollArea::vertical()
        .max_height(rect.height().max(0.0))
        .auto_shrink([false, false])
        .show(&mut body, add)
        .inner
}

pub(crate) fn row_label(ui: &Ui, rect: Rect, label: &str, font: egui::FontId, ink: egui::Color32) {
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), font, ink);
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width().max(0.0));
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(
        egui::pos2(rect.left(), rect.center().y - galley.size().y * 0.5),
        galley,
        ink,
    );
}

const MENU_MARGIN: i8 = 8;

fn menu_style(style: &mut egui::Style) {
    egui::containers::menu::menu_style(style);
    style.spacing.menu_margin = egui::Margin::same(MENU_MARGIN);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.visuals.window_fill = egui::Color32::TRANSPARENT;
    style.visuals.window_stroke = egui::Stroke::NONE;
    style.visuals.popup_shadow = egui::epaint::Shadow::NONE;
    style.visuals.menu_corner_radius = egui::CornerRadius::same(Role::Menu.radius());
    style.wrap_mode = Some(egui::TextWrapMode::Truncate);
}

fn enabled(ctx: &Context) -> bool {
    let _ = ctx;
    #[cfg(target_os = "macos")]
    {
        macos::enabled(ctx)
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// The Area's final response includes the frame and any scrollbars.
#[derive(Default)]
struct MenuSurface(Option<(Surface, egui::Painter, egui::Id, Rect)>);

impl MenuSurface {
    fn draw<R>(&mut self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
        let surface = Surface::begin(ui);
        let result = add(ui);
        if surface.is_native() {
            self.0 = Some((
                surface,
                ui.painter().clone(),
                ui.layer_id().id,
                ui.min_rect().expand(f32::from(MENU_MARGIN)),
            ));
        }
        result
    }

    fn finish(self, ctx: &Context, rect: Option<Rect>) {
        if let Some((surface, painter, id, measured)) = self.0 {
            let rect = rect
                .or_else(|| ctx.read_response(id).map(|response| response.rect))
                .filter(|rect| rect.is_finite() && rect.is_positive())
                .unwrap_or(measured);
            surface.finish_on(&painter, rect, Role::Menu);
        }
    }
}

pub(crate) fn popup<R>(
    popup: egui::Popup<'_>,
    ctx: &Context,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<InnerResponse<R>> {
    if !enabled(ctx) {
        return popup.show(add);
    }
    let mut surface = MenuSurface::default();
    let result = popup.style(menu_style).show(|ui| surface.draw(ui, add));
    surface.finish(ctx, result.as_ref().map(|shown| shown.response.rect));
    result
}

pub(crate) fn context_menu(response: &egui::Response, add: impl FnOnce(&mut Ui)) {
    popup(egui::Popup::context_menu(response), &response.ctx, add);
}

pub(crate) fn submenu(ui: &mut Ui, label: impl Into<egui::WidgetText>, add: impl FnOnce(&mut Ui)) {
    let mut surface = MenuSurface::default();
    ui.menu_button(label.into(), |ui| surface.draw(ui, add));
    surface.finish(ui.ctx(), None);
}

pub(crate) trait ComboBoxGlassExt {
    fn show_glass<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R)
    -> InnerResponse<Option<R>>;
}

impl ComboBoxGlassExt for egui::ComboBox<'_> {
    fn show_glass<R>(
        self,
        ui: &mut Ui,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>> {
        if !enabled(ui.ctx()) {
            return self.show_ui(ui, add);
        }
        let mut surface = MenuSurface::default();
        let response = self
            .popup_style(menu_style.into())
            .show_ui(ui, |ui| surface.draw(ui, add));
        surface.finish(ui.ctx(), None);
        response
    }
}

#[cfg(target_os = "macos")]
pub(crate) use macos::GlassOverlays;

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::{Arc, Mutex};

    use super::Role;
    use egui::{Id, PaintCallbackInfo, Rect, Shape, Ui};
    use egui_glow::CallbackFn;
    use glow::HasContext as _;
    use objc2::rc::Retained;
    use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, define_class, runtime::AnyClass};
    use objc2_app_kit::{
        NSBitmapImageRep, NSDeviceRGBColorSpace, NSGlassEffectContainerView, NSGlassEffectView,
        NSGlassEffectViewStyle, NSImage, NSImageScaling, NSImageView, NSView,
        NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
        NSVisualEffectView, NSWindowOrderingMode,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use objc2_quartz_core::CATransaction;
    use winit::raw_window_handle::HasWindowHandle;

    define_class!(
        #[unsafe(super(NSView))]
        #[thread_kind = MainThreadOnly]
        #[name = "ZervoGlassOverlayHost"]
        #[ivars = ()]
        struct Host;

        impl Host {
            #[unsafe(method(hitTest:))]
            fn hit_test(&self, _point: NSPoint) -> *mut NSView {
                // The window's egui view handles all interaction.
                std::ptr::null_mut()
            }

            #[unsafe(method(isAccessibilityHidden))]
            fn is_accessibility_hidden(&self) -> bool {
                // AccessKit supplies the interactive UI to assistive technology.
                true
            }
        }
    );

    enum Effect {
        Glass(Retained<NSGlassEffectView>),
        Frost(Retained<NSVisualEffectView>),
    }

    impl Role {
        fn frost(self) -> Option<NSVisualEffectMaterial> {
            match self {
                Self::Field => Some(NSVisualEffectMaterial::Popover),
                _ => None,
            }
        }
    }

    impl Effect {
        fn new(role: Role, mtm: MainThreadMarker) -> Option<Self> {
            if let Some(material) = role.frost() {
                let frost =
                    NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), NSRect::ZERO);
                frost.setMaterial(material);
                frost.setBlendingMode(NSVisualEffectBlendingMode::WithinWindow);
                frost.setState(NSVisualEffectState::FollowsWindowActiveState);
                frost.setWantsLayer(true);
                Some(Self::Frost(frost))
            } else {
                let glass = crate::vibrancy::new_glass_view()?;
                glass.setStyle(NSGlassEffectViewStyle::Regular);
                Some(Self::Glass(glass))
            }
        }

        fn view(&self) -> &NSView {
            match self {
                Self::Glass(view) => view,
                Self::Frost(view) => view,
            }
        }

        fn set_radius(&self, radius: f64) {
            match self {
                Self::Glass(view) => {
                    if view.cornerRadius() != radius {
                        view.setCornerRadius(radius);
                    }
                },
                Self::Frost(view) => {
                    if let Some(layer) = view.layer()
                        && layer.cornerRadius() != radius
                    {
                        layer.setCornerRadius(radius);
                        layer.setMasksToBounds(true);
                    }
                },
            }
        }
    }

    struct Material {
        effect: Effect,
        role: Role,
        dark: bool,
    }

    impl Material {
        fn new(role: Role, dark: bool, mtm: MainThreadMarker) -> Option<Self> {
            let effect = Effect::new(role, mtm)?;
            if let Effect::Glass(view) = &effect {
                view.setTintColor(Some(&role.tint(dark)));
            }
            Some(Self { effect, role, dark })
        }

        fn place(&mut self, frame: NSRect, dark: bool, zoom: f64) {
            if dark != self.dark
                && let Effect::Glass(view) = &self.effect
            {
                view.setTintColor(Some(&self.role.tint(dark)));
            }
            self.dark = dark;
            let radius = (f64::from(self.role.radius()) * zoom)
                .min(frame.size.width.min(frame.size.height) / 2.0);
            self.effect.set_radius(radius);
            if self.effect.view().frame() != frame {
                self.effect.view().setFrame(frame);
            }
        }
    }

    struct Layer {
        root: Retained<NSView>,
        material: Material,
        content: Retained<NSView>,
        ink: Retained<NSImageView>,
        decorations: Vec<Material>,
        image_size: Option<(NSSize, usize, usize)>,
        pixels: Vec<u8>,
    }

    pub(crate) struct GlassOverlays {
        content: Retained<NSView>,
        host: Retained<Host>,
        container: Retained<NSGlassEffectContainerView>,
        surfaces: Retained<NSView>,
        sidebar: Option<Layer>,
        sidebar_position: Option<usize>,
        layers: Vec<Option<Layer>>,
        renderer: Arc<Mutex<Renderer>>,
    }

    impl GlassOverlays {
        pub(crate) fn install(window: &impl HasWindowHandle) -> Option<Self> {
            let mtm = MainThreadMarker::new()?;
            crate::vibrancy::glass_class()?;
            let content = crate::vibrancy::content_view(window)?;
            let allocated = Host::alloc(mtm).set_ivars(());
            // SAFETY: Host inherits NSView's designated initializer, with its ivars set.
            let host: Retained<Host> =
                unsafe { objc2::msg_send![super(allocated), initWithFrame: content.frame()] };
            host.setHidden(true);
            host.setClipsToBounds(true);
            let class = AnyClass::get(c"NSGlassEffectContainerView")?;
            // SAFETY: the class exists on this OS and AppKit views are
            // allocated on the main thread.
            let container: Retained<NSGlassEffectContainerView> =
                unsafe { objc2::msg_send![class, new] };
            let surfaces = NSView::initWithFrame(NSView::alloc(mtm), host.bounds());
            container.setContentView(Some(&surfaces));
            host.addSubview(&container);
            Some(Self {
                content,
                host,
                container,
                surfaces,
                sidebar: None,
                sidebar_position: None,
                layers: Vec::new(),
                renderer: Default::default(),
            })
        }

        /// Start a frame and expose its renderer to popup builders during layout.
        pub(crate) fn prepare(&self, ctx: &egui::Context, size: [u32; 2], enabled: bool) -> bool {
            let mut renderer = self.renderer.lock().unwrap();
            renderer.active = 0;
            let ready = enabled && renderer.failed_size != Some(size);
            ctx.data_mut(|data| {
                if ready {
                    data.insert_temp(renderer_id(), self.renderer.clone());
                } else {
                    data.remove::<Arc<Mutex<Renderer>>>(renderer_id());
                }
            });
            ready
        }

        pub(crate) fn present(&mut self, ctx: &egui::Context, dark: bool) {
            // The slide is already timed by egui. Publish the frame and ink
            // together without a second implicit animation in Core Animation.
            CATransaction::begin();
            CATransaction::setDisableActions(true);
            self.update_views(ctx, dark);
            CATransaction::commit();
        }

        fn update_views(&mut self, ctx: &egui::Context, dark: bool) {
            let renderer = self.renderer.clone();
            let mut renderer = renderer.lock().unwrap();
            if std::mem::take(&mut renderer.repaint) {
                ctx.request_repaint();
            }
            let count = renderer.active;
            if count == 0 {
                if !self.host.isHidden() {
                    self.host.setHidden(true);
                }
                return;
            }

            // SAFETY: the retained content view is accessed on the main thread.
            let Some(parent) = (unsafe { self.content.superview() }) else {
                return;
            };
            // SAFETY: the retained overlay host lives on the main thread.
            if unsafe { self.host.superview() }.as_deref() != Some(&*parent) {
                parent.addSubview_positioned_relativeTo(
                    &self.host,
                    NSWindowOrderingMode::Above,
                    Some(&self.content),
                );
            }
            let host_frame = self.content.frame();
            if self.host.frame() != host_frame {
                self.host.setFrame(host_frame);
            }
            let bounds = self.host.bounds();
            if self.container.frame() != bounds {
                self.container.setFrame(bounds);
                self.surfaces.setFrame(self.container.bounds());
            }
            if self.host.isHidden() {
                self.host.setHidden(false);
            }

            let mtm = self.host.mtm();
            let zoom = f64::from(ctx.zoom_factor());
            let mut index = 0;
            let mut sidebar_position = None;
            for frame in &mut renderer.frames[..count] {
                let slot = if frame.role == Role::Sidebar {
                    sidebar_position = Some(index);
                    &mut self.sidebar
                } else {
                    if index == self.layers.len() {
                        self.layers.push(None);
                    }
                    let slot = &mut self.layers[index];
                    index += 1;
                    slot
                };
                if slot
                    .as_ref()
                    .is_none_or(|layer| layer.material.role != frame.role)
                {
                    let Some(layer) = Layer::new(frame.role, dark, mtm) else {
                        continue;
                    };
                    if frame.role != Role::Sidebar {
                        let previous = slot.as_ref().map(|layer| &*layer.root);
                        self.surfaces.addSubview_positioned_relativeTo(
                            &layer.root,
                            NSWindowOrderingMode::Above,
                            previous,
                        );
                    }
                    if let Some(previous) = slot.replace(layer) {
                        previous.root.removeFromSuperview();
                    }
                }
                let layer = slot.as_mut().unwrap();
                if !layer.update(frame, bounds, dark, zoom, mtm) {
                    renderer.failed_size = Some(renderer.size);
                    self.host.setHidden(true);
                    ctx.request_repaint();
                    return;
                }
            }
            for layer in self.layers.iter().skip(index).flatten() {
                layer.root.setHidden(true);
            }
            if let Some(sidebar) = &self.sidebar {
                if let Some(position) = sidebar_position {
                    if self.sidebar_position != sidebar_position {
                        let previous = position
                            .checked_sub(1)
                            .and_then(|index| self.layers[index].as_ref());
                        let next = self.layers.first().and_then(Option::as_ref);
                        let (order, relative) = if let Some(previous) = previous {
                            (NSWindowOrderingMode::Above, Some(previous))
                        } else {
                            (NSWindowOrderingMode::Below, next)
                        };
                        self.surfaces.addSubview_positioned_relativeTo(
                            &sidebar.root,
                            order,
                            relative.map(|layer| &*layer.root),
                        );
                    }
                } else {
                    sidebar.root.setHidden(true);
                }
            }
            self.sidebar_position = sidebar_position;
        }

        pub(crate) fn destroy(&self, gl: &glow::Context) {
            self.renderer.lock().unwrap().destroy(gl);
        }
    }

    impl Layer {
        fn new(role: Role, dark: bool, mtm: MainThreadMarker) -> Option<Self> {
            let root = NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO);
            let material = Material::new(role, dark, mtm)?;
            root.addSubview(material.effect.view());
            let content = NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO);
            match &material.effect {
                Effect::Glass(view) => view.setContentView(Some(&content)),
                Effect::Frost(view) => view.addSubview(&content),
            }
            let ink = NSImageView::initWithFrame(NSImageView::alloc(mtm), NSRect::ZERO);
            ink.setImageScaling(NSImageScaling::ScaleAxesIndependently);
            content.addSubview(&ink);
            Some(Self {
                root,
                material,
                content,
                ink,
                decorations: Vec::new(),
                image_size: None,
                pixels: Vec::new(),
            })
        }

        fn update(
            &mut self,
            frame: &mut Frame,
            bounds: NSRect,
            dark: bool,
            zoom: f64,
            mtm: MainThreadMarker,
        ) -> bool {
            let rect = frame.material_rect();
            let size = NSSize::new(
                f64::from(rect.width()) * zoom,
                f64::from(rect.height()) * zoom,
            );
            // The window owns the left corners. Move the full panel past the
            // edge instead of resizing the visible slice on each frame.
            let bleed = if frame.role == Role::Sidebar {
                f64::from(frame.role.radius()) * zoom + 2.0
            } else {
                0.0
            };
            let local = NSRect::new(
                NSPoint::new(
                    bounds.min().x + f64::from(rect.left()) * zoom - bleed,
                    bounds.max().y - f64::from(rect.bottom()) * zoom,
                ),
                NSSize::new(size.width + bleed, size.height),
            );
            if self.root.frame() != local {
                self.root.setFrame(local);
            }
            let material_frame = NSRect::new(NSPoint::ZERO, local.size);
            self.material.place(material_frame, dark, zoom);
            if self.content.frame() != material_frame {
                self.content.setFrame(material_frame);
            }
            let relative_rect = |part: Rect| {
                NSRect::new(
                    NSPoint::new(
                        f64::from(part.left() - rect.left()) * zoom + bleed,
                        f64::from(rect.bottom() - part.bottom()) * zoom,
                    ),
                    NSSize::new(
                        f64::from(part.width()) * zoom,
                        f64::from(part.height()) * zoom,
                    ),
                )
            };
            let ink_frame = relative_rect(frame.rect);
            if self.ink.frame() != ink_frame {
                self.ink.setFrame(ink_frame);
            }
            let image_size = Some((ink_frame.size, frame.width, frame.height));
            // The compositor tracks the backdrop independently of the ink.
            if self.image_size != image_size || frame.rgba != self.pixels {
                let Some(image) = frame.image(ink_frame.size) else {
                    return false;
                };
                self.ink.setImage(Some(&image));
                self.image_size = image_size;
                std::mem::swap(&mut frame.rgba, &mut self.pixels);
            }
            for (index, &(rect, role)) in frame.decorations.iter().enumerate() {
                if self
                    .decorations
                    .get(index)
                    .is_none_or(|material| material.role != role)
                {
                    let Some(material) = Material::new(role, dark, mtm) else {
                        break;
                    };
                    self.content.addSubview_positioned_relativeTo(
                        material.effect.view(),
                        NSWindowOrderingMode::Below,
                        Some(&self.ink),
                    );
                    if index == self.decorations.len() {
                        self.decorations.push(material);
                    } else {
                        self.decorations[index].effect.view().removeFromSuperview();
                        self.decorations[index] = material;
                    }
                }
                let material = &mut self.decorations[index];
                material.place(relative_rect(rect), dark, zoom);
                if material.effect.view().isHidden() {
                    material.effect.view().setHidden(false);
                }
            }
            for material in self.decorations.iter().skip(frame.decorations.len()) {
                material.effect.view().setHidden(true);
            }
            if self.root.isHidden() {
                self.root.setHidden(false);
            }
            true
        }
    }

    fn renderer_id() -> Id {
        Id::unique("zervo_appkit_glass_overlays")
    }

    pub(super) fn enabled(ctx: &egui::Context) -> bool {
        ctx.data(|data| {
            data.get_temp::<Arc<Mutex<Renderer>>>(renderer_id())
                .is_some()
        })
    }

    pub(super) struct Capture {
        renderer: Arc<Mutex<Renderer>>,
        start: egui::layers::ShapeIdx,
    }

    impl Capture {
        pub(super) fn begin(ui: &Ui) -> Option<Self> {
            if !ui.is_visible() || ui.is_sizing_pass() {
                return None;
            }
            let renderer = ui
                .ctx()
                .data(|data| data.get_temp::<Arc<Mutex<Renderer>>>(renderer_id()))?;
            Some(Self {
                renderer,
                start: ui.painter().add(Shape::Noop),
            })
        }

        pub(super) fn finish(self, painter: &egui::Painter, rect: Rect, role: Role) {
            let ctx = painter.ctx();
            let screen = ctx.content_rect();
            let surface_width = rect.width();
            let clip = match role {
                Role::Field => painter.clip_rect(),
                _ => screen,
            };
            let rect = rect.intersect(screen).intersect(clip);
            if !rect.is_positive() || !rect.is_finite() {
                return;
            }
            let renderer = self.renderer.clone();
            let painter = ctx.layer_painter(painter.layer_id()).with_clip_rect(screen);
            // The marker inherits the same layer and placement as the
            // overlay. The normal egui paint order then determines stacking.
            painter.set(
                self.start,
                egui::PaintCallback {
                    rect,
                    callback: Arc::new(CallbackFn::new(move |info, painter| {
                        renderer.lock().unwrap().begin(info, painter.gl());
                    })),
                },
            );
            painter.add(egui::PaintCallback {
                rect,
                callback: Arc::new(CallbackFn::new(move |info, painter| {
                    self.renderer
                        .lock()
                        .unwrap()
                        .end(info, painter.gl(), role, surface_width);
                })),
            });
        }
    }

    struct Frame {
        rect: Rect,
        role: Role,
        surface_width: f32,
        decorations: Vec<(Rect, Role)>,
        width: usize,
        height: usize,
        rgba: Vec<u8>,
    }

    impl Frame {
        fn material_rect(&self) -> Rect {
            if self.role == Role::Sidebar {
                self.rect.with_min_x(self.rect.right() - self.surface_width)
            } else {
                self.rect
            }
        }

        fn image(&self, size: NSSize) -> Option<Retained<NSImage>> {
            let stride = self.width * 4;
            // SAFETY: a null plane pointer requests AppKit-owned, interleaved
            // RGBA storage with the declared dimensions and stride.
            let bitmap = unsafe {
                NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                    NSBitmapImageRep::alloc(),
                    std::ptr::null_mut(),
                    self.width as isize,
                    self.height as isize,
                    8,
                    4,
                    true,
                    false,
                    NSDeviceRGBColorSpace,
                    stride as isize,
                    32,
                )
            }?;
            let data = bitmap.bitmapData();
            if data.is_null() {
                return None;
            }
            // SAFETY: the bitmap owns stride * height bytes exclusively here.
            let destination = unsafe { std::slice::from_raw_parts_mut(data, self.rgba.len()) };
            for (row, flipped) in destination
                .chunks_exact_mut(stride)
                .zip(self.rgba.rchunks_exact(stride))
            {
                row.copy_from_slice(flipped);
            }
            bitmap.setSize(size);
            let image = NSImage::initWithSize(NSImage::alloc(), size);
            image.addRepresentation(&bitmap);
            Some(image)
        }
    }

    #[cfg(test)]
    #[test]
    fn sidebar_keeps_its_material_width_during_reveal() {
        let mut frame = Frame {
            rect: Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(12.0, 800.0)),
            role: Role::Sidebar,
            surface_width: 248.0,
            decorations: Vec::new(),
            width: 24,
            height: 1600,
            rgba: Vec::new(),
        };
        assert_eq!(frame.material_rect().left(), -236.0);
        assert_eq!(frame.material_rect().right(), 12.0);
        frame.rect = frame.rect.with_max_x(248.0);
        assert_eq!(frame.material_rect(), frame.rect);
    }

    #[derive(Default)]
    struct Renderer {
        target: Option<(glow::Framebuffer, glow::Renderbuffer)>,
        size: [u32; 2],
        previous: Option<(Option<glow::Framebuffer>, Option<glow::Framebuffer>)>,
        nested: usize,
        decorations: Vec<(Rect, Role)>,
        frames: Vec<Frame>,
        active: usize,
        failed_size: Option<[u32; 2]>,
        repaint: bool,
    }

    impl Renderer {
        fn prepare(&mut self, gl: &glow::Context, size: [u32; 2]) -> bool {
            if size.contains(&0) {
                return false;
            }
            if self.size == size && self.target.is_some() {
                return true;
            }
            self.destroy(gl);
            // SAFETY: main-thread renderer with its OpenGL context current.
            // Restore framebuffer and renderbuffer bindings before returning.
            unsafe {
                let Ok(fbo) = gl.create_framebuffer() else {
                    return false;
                };
                let Ok(color) = gl.create_renderbuffer() else {
                    gl.delete_framebuffer(fbo);
                    return false;
                };
                let previous = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
                let previous_color = gl.get_parameter_renderbuffer(glow::RENDERBUFFER_BINDING);
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color));
                gl.renderbuffer_storage(
                    glow::RENDERBUFFER,
                    glow::RGBA8,
                    size[0] as i32,
                    size[1] as i32,
                );
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(fbo));
                gl.framebuffer_renderbuffer(
                    glow::DRAW_FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::RENDERBUFFER,
                    Some(color),
                );
                let complete = gl.check_framebuffer_status(glow::DRAW_FRAMEBUFFER)
                    == glow::FRAMEBUFFER_COMPLETE;
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, previous);
                gl.bind_renderbuffer(glow::RENDERBUFFER, previous_color);
                if !complete {
                    gl.delete_framebuffer(fbo);
                    gl.delete_renderbuffer(color);
                    return false;
                }
                self.target = Some((fbo, color));
            }
            self.size = size;
            true
        }

        fn begin(&mut self, info: PaintCallbackInfo, gl: &glow::Context) {
            if self.previous.is_some() {
                // A material inside a captured overlay shares its foreground
                // buffer. Its AppKit background is placed beneath that ink.
                self.nested += 1;
                return;
            }
            let size = info.screen_size_px;
            if self.failed_size == Some(size) {
                return;
            }
            if !self.prepare(gl, size) {
                self.failed_size = Some(size);
                self.repaint = true;
                return;
            }
            self.failed_size = None;
            let Some((fbo, _)) = self.target else {
                return;
            };
            // SAFETY: this is the current context's complete framebuffer.
            // egui re-establishes its scissor, viewport and blend state after the callback.
            unsafe {
                self.previous = Some((
                    gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING),
                    gl.get_parameter_framebuffer(glow::READ_FRAMEBUFFER_BINDING),
                ));
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                let viewport = info.viewport_in_pixels();
                gl.enable(glow::SCISSOR_TEST);
                gl.scissor(
                    viewport.left_px,
                    viewport.from_bottom_px,
                    viewport.width_px,
                    viewport.height_px,
                );
                gl.clear_color(0.0, 0.0, 0.0, 0.0);
                gl.clear(glow::COLOR_BUFFER_BIT);
            }
        }

        fn end(
            &mut self,
            info: PaintCallbackInfo,
            gl: &glow::Context,
            role: Role,
            surface_width: f32,
        ) {
            if self.nested > 0 {
                self.nested -= 1;
                self.decorations.push((info.viewport, role));
                return;
            }
            let Some((draw, read)) = self.previous.take() else {
                return;
            };
            let viewport = info.viewport_in_pixels();
            let width = viewport.width_px.max(0) as usize;
            let height = viewport.height_px.max(0) as usize;
            let frame = if self.active == self.frames.len() {
                self.frames.push(Frame {
                    rect: Rect::NOTHING,
                    role,
                    surface_width,
                    decorations: Vec::new(),
                    width: 0,
                    height: 0,
                    rgba: Vec::new(),
                });
                self.frames.last_mut().unwrap()
            } else {
                &mut self.frames[self.active]
            };
            frame.rgba.resize(width * height * 4, 0);
            // SAFETY: dimensions are clipped to the window and this buffer
            // has four bytes for each pixel. The app's pack state is restored.
            unsafe {
                let pack = [
                    (glow::PACK_ROW_LENGTH, 0),
                    (glow::PACK_SKIP_ROWS, 0),
                    (glow::PACK_SKIP_PIXELS, 0),
                    (glow::PACK_ALIGNMENT, 4),
                ];
                let previous = pack.map(|(param, value)| {
                    let old = gl.get_parameter_i32(param);
                    gl.pixel_store_i32(param, value);
                    (param, old)
                });
                let previous_pack = gl.get_parameter_buffer(glow::PIXEL_PACK_BUFFER_BINDING);
                gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
                if !frame.rgba.is_empty() {
                    gl.read_pixels(
                        viewport.left_px,
                        viewport.from_bottom_px,
                        viewport.width_px,
                        viewport.height_px,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelPackData::Slice(Some(&mut frame.rgba)),
                    );
                }
                for (param, value) in previous {
                    gl.pixel_store_i32(param, value);
                }
                gl.bind_buffer(glow::PIXEL_PACK_BUFFER, previous_pack);
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, draw);
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, read);
            }
            self.decorations.retain_mut(|(rect, _)| {
                *rect = rect.intersect(info.viewport);
                rect.is_positive()
            });
            std::mem::swap(&mut frame.decorations, &mut self.decorations);
            self.decorations.clear();
            if frame.rgba.is_empty() {
                return;
            }
            let scale = info.pixels_per_point;
            frame.rect = Rect::from_min_size(
                egui::pos2(
                    viewport.left_px as f32 / scale,
                    viewport.top_px as f32 / scale,
                ),
                egui::vec2(width as f32 / scale, height as f32 / scale),
            );
            frame.width = width;
            frame.height = height;
            frame.role = role;
            frame.surface_width = surface_width;
            self.active += 1;
        }

        fn destroy(&mut self, gl: &glow::Context) {
            if let Some((fbo, color)) = self.target.take() {
                // SAFETY: these names belong to the current OpenGL context
                // and are no longer used by paint callbacks.
                unsafe {
                    gl.delete_framebuffer(fbo);
                    gl.delete_renderbuffer(color);
                }
            }
        }
    }
}
