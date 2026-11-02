//! GTK3 windows: the transparent overlay, the floating palette and the tray icon.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::gdk::{self, keys::constants as key, EventMask, ModifierType};
use gtk::glib::{self, Propagation};
use gtk::prelude::*;
use overdraw_core::{Point, Tool, PALETTE};

use crate::render;
use crate::state::{Background, State, BACKGROUND_OPACITIES, MAX_WIDTH, MIN_WIDTH};

const WIDTH_STEP: f64 = 2.0;

pub struct Ui {
    app: gtk::Application,
    state: RefCell<State>,
    overlay: gtk::Window,
    area: gtk::DrawingArea,
    palette: gtk::Window,
    tool_buttons: HashMap<Tool, gtk::RadioButton>,
    color_buttons: Vec<gtk::RadioButton>,
    width_scale: gtk::Scale,
    palette_placed: Cell<bool>,
    /// Smooth-scroll distance not yet turned into a width step.
    scroll_rest: Cell<f64>,
    /// Whether the timer that animates fading shapes is running.
    fade_timer: Cell<bool>,
}

fn tool_label(tool: Tool) -> (&'static str, &'static str) {
    match tool {
        Tool::Pen => ("✎", "Pen"),
        Tool::Highlighter => ("▌", "Highlighter"),
        Tool::Line => ("╱", "Line"),
        Tool::Arrow => ("➚", "Arrow"),
        Tool::Rect => ("▭", "Rectangle"),
        Tool::Ellipse => ("◯", "Ellipse"),
        Tool::Text => ("T", "Text"),
        Tool::Badge => ("①", "Numbered badge"),
        Tool::Eraser => ("⌫", "Eraser"),
    }
}

/// Edge length of a tool icon on the palette, in logical pixels.
const TOOL_ICON_SIZE: i32 = 20;

fn tool_icon_svg(tool: Tool) -> &'static str {
    match tool {
        Tool::Pen => include_str!("../../../data/icons/pen.svg"),
        Tool::Highlighter => include_str!("../../../data/icons/highlighter.svg"),
        Tool::Line => include_str!("../../../data/icons/line.svg"),
        Tool::Arrow => include_str!("../../../data/icons/arrow.svg"),
        Tool::Rect => include_str!("../../../data/icons/rectangle.svg"),
        Tool::Ellipse => include_str!("../../../data/icons/ellipse.svg"),
        Tool::Text => include_str!("../../../data/icons/text.svg"),
        Tool::Badge => include_str!("../../../data/icons/badge.svg"),
        Tool::Eraser => include_str!("../../../data/icons/eraser.svg"),
    }
}

const SCREENSHOT_ICON_SVG: &str = include_str!("../../../data/icons/screenshot.svg");
const PASS_THROUGH_ICON_SVG: &str = include_str!("../../../data/icons/pass-through.svg");
const FADE_ICON_SVG: &str = include_str!("../../../data/icons/fade.svg");
/// Redraw interval while fading shapes are on screen.
const FADE_FRAME: Duration = Duration::from_millis(33);
/// Time for the compositor to take the hidden canvas off the screen before capturing.
const SCREENSHOT_DELAY: Duration = Duration::from_millis(300);

/// Rasterizes a palette icon for the dark palette. The SVGs are drawn black on
/// white, so the two colors are swapped: black becomes the glyph color and
/// white becomes the palette background.
fn palette_icon(svg: &str) -> Option<gtk::gdk_pixbuf::Pixbuf> {
    let svg = svg
        .replace("\"white\"", "\"#1c1c1e\"")
        .replace("\"black\"", "\"#ffffff\"");
    svg_pixbuf(&svg, TOOL_ICON_SIZE)
}

/// Puts a palette icon on a button, or `fallback` text if the icon can't be loaded.
fn set_icon(button: &(impl IsA<gtk::Button> + IsA<gtk::Widget>), svg: &str, fallback: &str) {
    match palette_icon(svg) {
        Some(pixbuf) => {
            button.set_image(Some(&gtk::Image::from_pixbuf(Some(&pixbuf))));
            button.set_always_show_image(true);
        }
        None => button.set_label(fallback),
    }
    button.set_can_focus(false);
}

const APP_ICON_SVG: &str = include_str!("../../../data/icons/overdraw-rs.svg");
const STATUS_ICON_SVG: &str = include_str!("../../../data/icons/overdraw-rs-status.svg");
/// Theme icon used when the tray icon file can't be written.
const FALLBACK_STATUS_ICON: &str = "applications-graphics";

fn svg_pixbuf(svg: &str, size: i32) -> Option<gtk::gdk_pixbuf::Pixbuf> {
    let loader = gtk::gdk_pixbuf::PixbufLoader::with_type("svg").ok()?;
    loader.set_size(size, size);
    let written = loader.write(svg.as_bytes());
    let closed = loader.close();
    if let Err(e) = written.and(closed) {
        eprintln!("overdraw: can't load icon: {e}");
        return None;
    }
    loader.pixbuf()
}

/// XAppStatusIcon accepts a file path as well as a theme icon name. The SVG is
/// embedded and written out at startup, so the tray icon works without installing
/// anything into the icon theme.
fn status_icon() -> String {
    let path = glib::user_runtime_dir().join("overdraw").join("overdraw-rs-status.svg");
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, STATUS_ICON_SVG));
    match written {
        Ok(()) => path.to_string_lossy().into_owned(),
        Err(e) => {
            eprintln!("overdraw: can't write tray icon {}: {e}", path.display());
            FALLBACK_STATUS_ICON.to_string()
        }
    }
}

fn hex(c: overdraw_core::Color) -> String {
    let byte = |v: f64| (v * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(c.r), byte(c.g), byte(c.b))
}

fn install_css() {
    let mut css = String::from(
        ".overdraw-palette { background-color: rgba(28,28,30,0.92); border-radius: 12px; }
         .overdraw-palette button { min-width: 30px; min-height: 30px; padding: 0 4px;
             color: #fff; font-size: 16px; background: none; border: none; box-shadow: none;
             border-radius: 8px; }
         .overdraw-palette button:not(.swatch):hover { background-color: rgba(255,255,255,0.12); }
         .overdraw-palette button:checked,
         .overdraw-palette button:not(.swatch):checked:hover { background-color: rgba(255,255,255,0.25); }
         .overdraw-palette .grip { color: rgba(255,255,255,0.5); padding: 0 6px; }
         .overdraw-palette .swatch { min-width: 18px; min-height: 18px; padding: 0;
             border-radius: 50%; border: 2px solid rgba(255,255,255,0.2); }
         .overdraw-palette .swatch:hover { border-color: rgba(255,255,255,0.6); }
         .overdraw-palette .swatch:checked { border-color: #fff; }
         .overdraw-palette scale { min-width: 110px; }\n",
    );
    for (i, c) in PALETTE.iter().enumerate() {
        css.push_str(&format!(
            ".overdraw-palette .swatch-{i}, .overdraw-palette .swatch-{i}:checked \
             {{ background-color: {}; }}\n",
            hex(*c)
        ));
    }
    let provider = gtk::CssProvider::new();
    if let Err(e) = provider.load_from_data(css.as_bytes()) {
        eprintln!("overdraw: bad CSS: {e}");
    }
    if let Some(screen) = gdk::Screen::default() {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

/// Created once and hidden on close, so opening it twice does not stack windows.
fn about_dialog() -> gtk::AboutDialog {
    let dialog = gtk::AboutDialog::builder()
        .program_name("overdraw")
        .version(env!("CARGO_PKG_VERSION"))
        .comments(env!("CARGO_PKG_DESCRIPTION"))
        .website(env!("CARGO_PKG_REPOSITORY"))
        .website_label("Source code on GitHub")
        .license_type(gtk::License::MitX11)
        .authors(["Aleksey Priymachuk"])
        .build();
    dialog.set_keep_above(true);
    if let Some(logo) = svg_pixbuf(APP_ICON_SVG, 128) {
        dialog.set_logo(Some(&logo));
    }
    dialog.connect_response(|d, _| d.hide());
    dialog.connect_delete_event(|d, _| {
        d.hide();
        Propagation::Stop
    });
    dialog
}

fn use_rgba_visual(win: &gtk::Window) {
    if let Some(visual) = WidgetExt::screen(win).and_then(|s| s.rgba_visual()) {
        win.set_visual(Some(&visual));
    } else {
        eprintln!("overdraw: no RGBA visual; is the compositor running?");
    }
}

fn primary_monitor_geometry() -> Option<gdk::Rectangle> {
    let display = gdk::Display::default()?;
    let monitor = display.primary_monitor().or_else(|| display.monitor(0))?;
    Some(monitor.geometry())
}

fn radio(group: Option<&gtk::RadioButton>) -> gtk::RadioButton {
    let b = gtk::RadioButton::new();
    if let Some(g) = group {
        b.join_group(Some(g));
    }
    // Draw as a toggle button instead of a round indicator.
    b.set_mode(false);
    b.set_can_focus(false);
    b
}

impl Ui {
    pub fn new(app: &gtk::Application) -> Rc<Ui> {
        install_css();
        if let Some(icon) = svg_pixbuf(APP_ICON_SVG, 128) {
            gtk::Window::set_default_icon(&icon);
        }

        let overlay = gtk::Window::new(gtk::WindowType::Toplevel);
        overlay.set_application(Some(app));
        overlay.set_title("overdraw");
        overlay.set_decorated(false);
        overlay.set_app_paintable(true);
        overlay.set_keep_above(true);
        overlay.set_skip_taskbar_hint(true);
        overlay.set_skip_pager_hint(true);
        overlay.stick();
        use_rgba_visual(&overlay);

        let area = gtk::DrawingArea::new();
        area.add_events(
            EventMask::BUTTON_PRESS_MASK
                | EventMask::BUTTON_RELEASE_MASK
                | EventMask::POINTER_MOTION_MASK
                | EventMask::SCROLL_MASK
                | EventMask::SMOOTH_SCROLL_MASK,
        );
        overlay.add(&area);

        let palette = gtk::Window::new(gtk::WindowType::Toplevel);
        palette.set_application(Some(app));
        palette.set_title("overdraw palette");
        palette.set_decorated(false);
        palette.set_resizable(false);
        palette.set_keep_above(true);
        palette.set_skip_taskbar_hint(true);
        palette.set_skip_pager_hint(true);
        palette.set_type_hint(gdk::WindowTypeHint::Utility);
        palette.set_transient_for(Some(&overlay));
        palette.stick();
        palette.style_context().add_class("overdraw-palette");
        use_rgba_visual(&palette);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        row.set_margin_top(6);
        row.set_margin_bottom(6);
        row.set_margin_start(4);
        row.set_margin_end(8);

        let grip = gtk::EventBox::new();
        let grip_label = gtk::Label::new(Some("⠿"));
        grip_label.style_context().add_class("grip");
        grip.add(&grip_label);
        grip.set_tooltip_text(Some("Drag to move"));
        row.pack_start(&grip, false, false, 0);

        let mut tool_buttons = HashMap::new();
        let mut first: Option<gtk::RadioButton> = None;
        for tool in Tool::ALL {
            let b = radio(first.as_ref());
            let (glyph, name) = tool_label(tool);
            match palette_icon(tool_icon_svg(tool)) {
                Some(pixbuf) => {
                    b.set_image(Some(&gtk::Image::from_pixbuf(Some(&pixbuf))));
                    b.set_always_show_image(true);
                }
                None => b.set_label(glyph),
            }
            b.set_tooltip_text(Some(&format!("{name} ({})", tool.shortcut().to_ascii_uppercase())));
            row.pack_start(&b, false, false, 0);
            first.get_or_insert_with(|| b.clone());
            tool_buttons.insert(tool, b);
        }

        row.pack_start(&gtk::Separator::new(gtk::Orientation::Vertical), false, false, 6);

        let mut color_buttons: Vec<gtk::RadioButton> = Vec::new();
        for i in 0..PALETTE.len() {
            let b = radio(color_buttons.first());
            b.style_context().add_class("swatch");
            b.style_context().add_class(&format!("swatch-{i}"));
            b.set_valign(gtk::Align::Center);
            b.set_tooltip_text(Some(&format!("Color {}", i + 1)));
            row.pack_start(&b, false, false, 2);
            color_buttons.push(b);
        }

        row.pack_start(&gtk::Separator::new(gtk::Orientation::Vertical), false, false, 6);

        let width_scale =
            gtk::Scale::with_range(gtk::Orientation::Horizontal, MIN_WIDTH, MAX_WIDTH, 1.0);
        width_scale.set_draw_value(false);
        width_scale.set_can_focus(false);
        width_scale.set_tooltip_text(Some("Width ( [ and ] )"));
        row.pack_start(&width_scale, false, false, 0);

        row.pack_start(&gtk::Separator::new(gtk::Orientation::Vertical), false, false, 6);

        let fading = gtk::ToggleButton::new();
        set_icon(&fading, FADE_ICON_SVG, "≈");
        fading.set_tooltip_text(Some("Fading: new drawing disappears after a moment"));
        row.pack_start(&fading, false, false, 0);

        let pass_through = gtk::ToggleButton::new();
        set_icon(&pass_through, PASS_THROUGH_ICON_SVG, "⇱");
        pass_through.set_tooltip_text(Some("Click through: use the windows under the drawing"));
        row.pack_start(&pass_through, false, false, 0);

        let screenshot = gtk::Button::new();
        set_icon(&screenshot, SCREENSHOT_ICON_SVG, "📷");
        screenshot.set_tooltip_text(Some("Screenshot with drawing (S)"));
        row.pack_start(&screenshot, false, false, 0);

        palette.add(&row);

        let ui = Rc::new(Ui {
            app: app.clone(),
            state: RefCell::new(State::default()),
            overlay,
            area,
            palette,
            tool_buttons,
            color_buttons,
            width_scale,
            palette_placed: Cell::new(false),
            scroll_rest: Cell::new(0.0),
            fade_timer: Cell::new(false),
        });
        ui.width_scale.set_value(ui.state.borrow().width);
        Ui::connect(&ui, &grip, &fading, &pass_through, &screenshot);
        ui
    }

    fn connect(
        ui: &Rc<Ui>,
        grip: &gtk::EventBox,
        fading: &gtk::ToggleButton,
        pass_through: &gtk::ToggleButton,
        screenshot: &gtk::Button,
    ) {
        let u = ui.clone();
        screenshot.connect_clicked(move |_| u.take_screenshot());

        let u = ui.clone();
        fading.connect_toggled(move |b| {
            let mut s = u.state.borrow_mut();
            s.commit_text();
            s.fading_mode = b.is_active();
        });

        let u = ui.clone();
        pass_through.connect_toggled(move |b| u.set_pass_through(b.is_active()));

        ui.overlay.connect_realize(|w| {
            let cursor = gdk::Cursor::for_display(&w.display(), gdk::CursorType::Crosshair);
            if let (Some(win), Some(cursor)) = (w.window(), cursor) {
                win.set_cursor(Some(&cursor));
            }
        });

        let u = ui.clone();
        ui.area.connect_draw(move |_, cr| {
            u.draw(cr);
            u.animate_fading();
            Propagation::Stop
        });

        let u = ui.clone();
        ui.area.connect_button_press_event(move |_, ev| {
            if ev.button() == 1 && ev.event_type() == gdk::EventType::ButtonPress {
                let (x, y) = ev.position();
                let m = ev.state();
                u.state.borrow_mut().press(
                    Point::new(x, y),
                    m.contains(ModifierType::SHIFT_MASK),
                    m.contains(ModifierType::MOD1_MASK),
                );
                u.area.queue_draw();
            }
            Propagation::Stop
        });

        let u = ui.clone();
        ui.area.connect_motion_notify_event(move |_, ev| {
            let (x, y) = ev.position();
            let mut s = u.state.borrow_mut();
            if s.is_dragging() {
                s.motion(Point::new(x, y), ev.state().contains(ModifierType::SHIFT_MASK));
                u.area.queue_draw();
            }
            Propagation::Stop
        });

        let u = ui.clone();
        ui.area.connect_scroll_event(move |_, ev| {
            u.on_scroll(ev);
            Propagation::Stop
        });

        let u = ui.clone();
        ui.area.connect_button_release_event(move |_, ev| {
            if ev.button() == 1 {
                u.state.borrow_mut().release();
                u.area.queue_draw();
            }
            Propagation::Stop
        });

        // The palette takes focus when clicked, so both windows share key handling.
        for win in [&ui.overlay, &ui.palette] {
            let u = ui.clone();
            win.connect_key_press_event(move |_, ev| u.on_key(ev));
            win.connect_delete_event(|w, _| {
                w.hide();
                Propagation::Stop
            });
        }

        for (tool, button) in &ui.tool_buttons {
            let (u, tool) = (ui.clone(), *tool);
            button.connect_toggled(move |b| {
                if b.is_active() {
                    let mut s = u.state.borrow_mut();
                    s.commit_text();
                    s.tool = tool;
                    drop(s);
                    u.area.queue_draw();
                }
            });
        }

        for (i, button) in ui.color_buttons.iter().enumerate() {
            let u = ui.clone();
            button.connect_toggled(move |b| {
                if b.is_active() {
                    u.state.borrow_mut().color = i;
                    u.area.queue_draw();
                }
            });
        }

        let u = ui.clone();
        ui.width_scale.connect_value_changed(move |scale| {
            u.state.borrow_mut().set_width(scale.value());
            u.area.queue_draw();
        });

        let palette = ui.palette.clone();
        grip.connect_button_press_event(move |_, ev| {
            if ev.button() == 1 {
                let (x, y) = ev.root();
                palette.begin_move_drag(1, x as i32, y as i32, ev.time());
            }
            Propagation::Stop
        });
    }

    fn draw(&self, cr: &gtk::cairo::Context) {
        let s = self.state.borrow();
        cr.set_operator(gtk::cairo::Operator::Source);
        let level = s.background.level();
        cr.set_source_rgba(level, level, level, s.shown_background_opacity());
        let _ = cr.paint();
        cr.set_operator(gtk::cairo::Operator::Over);

        for shape in s.doc.shapes() {
            render::draw_shape(cr, shape);
        }
        for (shape, alpha) in s.fading_shapes(Instant::now()) {
            render::draw_shape_faded(cr, shape, alpha);
        }
        if let Some(shape) = s.preview() {
            render::draw_shape(cr, &shape);
        }
        if let Some(shape) = s.text_preview() {
            render::draw_text_edit(cr, &shape);
        }
    }

    /// Keeps redrawing while fading shapes are on screen. Started from the draw
    /// handler, so any way a fading shape gets added sets it going.
    fn animate_fading(self: &Rc<Self>) {
        if self.fade_timer.get() || !self.state.borrow().has_fading() {
            return;
        }
        self.fade_timer.set(true);
        let u = self.clone();
        glib::timeout_add_local(FADE_FRAME, move || {
            let left = u.state.borrow_mut().expire_fading(Instant::now());
            u.area.queue_draw();
            if left {
                glib::ControlFlow::Continue
            } else {
                u.fade_timer.set(false);
                glib::ControlFlow::Break
            }
        });
    }

    /// With an empty input shape the canvas gets no mouse events, so clicks reach
    /// the windows under it. The palette is a separate window and keeps working.
    fn set_pass_through(&self, on: bool) {
        {
            let mut s = self.state.borrow_mut();
            s.cancel_drag();
            s.commit_text();
            s.pass_through = on;
        }
        let empty = gtk::cairo::Region::create();
        self.overlay.input_shape_combine_region(on.then_some(&empty));
        self.area.queue_draw();
    }

    /// Scrolling up widens the tool, with Ctrl it widens the last shape instead.
    fn on_scroll(&self, ev: &gdk::EventScroll) {
        let steps = match ev.direction() {
            gdk::ScrollDirection::Up => 1,
            gdk::ScrollDirection::Down => -1,
            gdk::ScrollDirection::Smooth => {
                // Touchpads send fractions of a notch; act once a whole one has built up.
                let total = self.scroll_rest.get() - ev.delta().1;
                let whole = total.trunc();
                self.scroll_rest.set(total - whole);
                whole as i32
            }
            _ => 0,
        };
        if steps == 0 {
            return;
        }
        let delta = steps as f64 * WIDTH_STEP;
        if ev.state().contains(ModifierType::CONTROL_MASK) {
            if self.state.borrow_mut().resize_last(delta) {
                self.area.queue_draw();
            }
        } else {
            self.width_scale.set_value(self.width_scale.value() + delta);
        }
    }

    /// Captures the screen under the canvas with the drawing on top (without the
    /// translucent background), copies it to the clipboard and asks where to save it.
    /// With a solid background the drawing is put on that background instead.
    fn take_screenshot(self: &Rc<Self>) {
        let Some(win) = self.overlay.window().filter(|_| self.is_shown()) else { return };
        let solid = {
            let mut s = self.state.borrow_mut();
            s.cancel_drag();
            s.commit_text();
            s.background_is_solid().then_some(s.background)
        };
        let (_, x, y) = win.origin();
        let (w, h) = (win.width(), win.height());
        if let Some(background) = solid {
            let byte = (background.level() * 255.0) as u32;
            let rgb = gtk::gdk_pixbuf::Colorspace::Rgb;
            let shot = gtk::gdk_pixbuf::Pixbuf::new(rgb, false, 8, w, h)
                .and_then(|bg| {
                    bg.fill(byte << 24 | byte << 16 | byte << 8 | 0xff);
                    render::compose(&bg, w, h, self.state.borrow().doc.shapes())
                })
                .and_then(|s| gdk::pixbuf_get_from_surface(&s, 0, 0, s.width(), s.height()));
            match shot {
                Some(pixbuf) => self.save_screenshot(&pixbuf),
                None => eprintln!("overdraw: can't render the screenshot"),
            }
            return;
        }
        // Both windows are hidden so the capture shows only what is under them.
        self.palette.hide();
        self.overlay.hide();
        let u = self.clone();
        glib::timeout_add_local_once(SCREENSHOT_DELAY, move || {
            let shot = gdk::Screen::default()
                .and_then(|screen| screen.root_window())
                .and_then(|root| root.pixbuf(x, y, w, h))
                .and_then(|bg| render::compose(&bg, w, h, u.state.borrow().doc.shapes()))
                .and_then(|s| gdk::pixbuf_get_from_surface(&s, 0, 0, s.width(), s.height()));
            u.show();
            match shot {
                Some(pixbuf) => u.save_screenshot(&pixbuf),
                None => eprintln!("overdraw: can't capture the screen"),
            }
        });
    }

    /// Copies the screenshot to the clipboard and asks where to save it.
    fn save_screenshot(&self, pixbuf: &gtk::gdk_pixbuf::Pixbuf) {
        gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).set_image(pixbuf);

        let dialog = gtk::FileChooserDialog::with_buttons(
            Some("Save Screenshot"),
            Some(&self.overlay),
            gtk::FileChooserAction::Save,
            &[("_Cancel", gtk::ResponseType::Cancel), ("_Save", gtk::ResponseType::Accept)],
        );
        dialog.set_default_response(gtk::ResponseType::Accept);
        dialog.set_do_overwrite_confirmation(true);
        // The canvas stays on top, so the dialog has to as well.
        dialog.set_keep_above(true);
        let dir = glib::user_special_dir(glib::UserDirectory::Pictures)
            .unwrap_or_else(glib::home_dir);
        dialog.set_current_folder(&dir);
        let stamp = glib::DateTime::now_local()
            .and_then(|t| t.format("%Y-%m-%d %H-%M-%S"))
            .map(|s| s.to_string())
            .unwrap_or_default();
        dialog.set_current_name(&format!("overdraw {stamp}.png"));
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("PNG images"));
        filter.add_mime_type("image/png");
        dialog.add_filter(filter);

        let (pixbuf, overlay) = (pixbuf.clone(), self.overlay.clone());
        dialog.connect_response(move |d, response| {
            let path = d.filename().filter(|_| response == gtk::ResponseType::Accept);
            d.close();
            let Some(path) = path else { return };
            if let Err(e) = pixbuf.savev(&path, "png", &[]) {
                let msg = gtk::MessageDialog::new(
                    Some(&overlay),
                    gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
                    gtk::MessageType::Error,
                    gtk::ButtonsType::Close,
                    &format!("Can't save {}", path.display()),
                );
                msg.set_secondary_text(Some(&e.to_string()));
                msg.set_keep_above(true);
                msg.connect_response(|m, _| m.close());
                msg.show();
            }
        });
        // After show() hands focus back to the canvas, which it also does on idle.
        glib::idle_add_local_once(move || dialog.present());
    }

    fn on_key(self: &Rc<Self>, ev: &gdk::EventKey) -> Propagation {
        let k = ev.keyval();
        let m = ev.state();
        let ctrl = m.contains(ModifierType::CONTROL_MASK);
        let shift = m.contains(ModifierType::SHIFT_MASK);

        // While typing text every key goes into the text.
        {
            let mut s = self.state.borrow_mut();
            if let Some(edit) = s.text.as_mut() {
                if k == key::Escape || k == key::Return || k == key::KP_Enter {
                    s.commit_text();
                } else if k == key::BackSpace {
                    edit.text.pop();
                } else if let Some(c) = k.to_unicode().filter(|c| !c.is_control() && !ctrl) {
                    edit.text.push(c);
                }
                drop(s);
                self.area.queue_draw();
                return Propagation::Stop;
            }
        }

        if k == key::Escape {
            let mut s = self.state.borrow_mut();
            if s.is_dragging() {
                s.cancel_drag();
                drop(s);
                self.area.queue_draw();
            } else {
                drop(s);
                self.hide();
            }
            return Propagation::Stop;
        }

        let lower = k.to_lower().to_unicode();
        if ctrl {
            let mut s = self.state.borrow_mut();
            match lower {
                Some('z') if shift => _ = s.doc.redo(),
                Some('z') => _ = s.doc.undo(),
                Some('y') => _ = s.doc.redo(),
                Some('q') => {
                    drop(s);
                    self.app.quit();
                    return Propagation::Stop;
                }
                _ => return Propagation::Proceed,
            }
            drop(s);
            self.area.queue_draw();
            return Propagation::Stop;
        }

        match lower {
            Some(c) if Tool::from_shortcut(c).is_some() => {
                self.tool_buttons[&Tool::from_shortcut(c).unwrap()].set_active(true);
            }
            Some(d @ '1'..='9') => {
                let i = d as usize - '1' as usize;
                self.color_buttons[i].set_active(true);
            }
            Some('[') => self.width_scale.set_value(self.width_scale.value() - WIDTH_STEP),
            Some(']') => self.width_scale.set_value(self.width_scale.value() + WIDTH_STEP),
            Some('c') => {
                self.state.borrow_mut().clear();
                self.area.queue_draw();
            }
            Some('s') => self.take_screenshot(),
            _ => return Propagation::Proceed,
        }
        Propagation::Stop
    }

    pub fn is_shown(&self) -> bool {
        self.overlay.is_visible()
    }

    pub fn show(&self) {
        self.overlay.show_all();
        self.overlay.fullscreen();
        self.overlay.present();
        self.palette.show_all();
        if !self.palette_placed.replace(true) {
            if let Some(mon) = primary_monitor_geometry() {
                let (_, natural) = self.palette.preferred_size();
                let x = mon.x() + (mon.width() - natural.width) / 2;
                let y = mon.y() + mon.height() - natural.height - 48;
                self.palette.move_(x, y);
            }
        }
        // Give keyboard focus back to the canvas.
        let overlay = self.overlay.clone();
        glib::idle_add_local_once(move || overlay.present());
    }

    pub fn hide(&self) {
        {
            let mut s = self.state.borrow_mut();
            s.cancel_drag();
            s.commit_text();
        }
        self.palette.hide();
        self.overlay.hide();
    }

    pub fn toggle(&self) {
        if self.is_shown() {
            self.hide();
        } else {
            self.show();
        }
    }

    /// Background color, then background opacity, as two radio groups.
    fn background_menu(ui: &Rc<Ui>) -> gtk::Menu {
        let menu = gtk::Menu::new();
        let (current_color, current_opacity) = {
            let s = ui.state.borrow();
            (s.background, s.background_opacity)
        };

        let mut first: Option<gtk::RadioMenuItem> = None;
        for color in Background::ALL {
            let item = gtk::RadioMenuItem::with_label(color.label());
            if let Some(f) = &first {
                item.join_group(Some(f));
            }
            item.set_active(color == current_color);
            let u = ui.clone();
            item.connect_toggled(move |i| {
                if i.is_active() {
                    u.state.borrow_mut().background = color;
                    u.area.queue_draw();
                }
            });
            menu.append(&item);
            first.get_or_insert(item);
        }

        menu.append(&gtk::SeparatorMenuItem::new());

        let mut first: Option<gtk::RadioMenuItem> = None;
        for opacity in BACKGROUND_OPACITIES {
            let label = if opacity == 0.0 {
                "Off".to_string()
            } else if opacity >= 1.0 {
                "Solid".to_string()
            } else {
                format!("{}%", (opacity * 100.0).round())
            };
            let item = gtk::RadioMenuItem::with_label(&label);
            if let Some(f) = &first {
                item.join_group(Some(f));
            }
            item.set_active(opacity == current_opacity);
            let u = ui.clone();
            item.connect_toggled(move |i| {
                if i.is_active() {
                    u.state.borrow_mut().background_opacity = opacity;
                    u.area.queue_draw();
                }
            });
            menu.append(&item);
            first.get_or_insert(item);
        }
        menu
    }

    pub fn install_tray(ui: &Rc<Ui>) -> glib::Object {
        let menu = gtk::Menu::new();
        let draw = gtk::MenuItem::with_label("Draw on Screen");
        let clear = gtk::MenuItem::with_label("Clear");
        let background = gtk::MenuItem::with_label("Background");
        let about = gtk::MenuItem::with_label("About…");
        let quit = gtk::MenuItem::with_label("Quit");
        background.set_submenu(Some(&Ui::background_menu(ui)));
        menu.append(&draw);
        menu.append(&clear);
        menu.append(&background);
        menu.append(&gtk::SeparatorMenuItem::new());
        menu.append(&about);
        menu.append(&quit);
        menu.show_all();

        let u = ui.clone();
        draw.connect_activate(move |_| u.toggle());
        let u = ui.clone();
        clear.connect_activate(move |_| {
            u.state.borrow_mut().clear();
            u.area.queue_draw();
        });
        let about_dialog = about_dialog();
        about.connect_activate(move |_| about_dialog.present());
        let app = ui.app.clone();
        quit.connect_activate(move |_| app.quit());

        // The secondary (right-click) menu is shown by libxapp itself; without a
        // primary menu a left click emits "activate" instead.
        let icon = tray::new_status_icon(
            "overdraw",
            &status_icon(),
            "overdraw — draw on screen",
            &menu,
        );
        let u = ui.clone();
        icon.connect_local("activate", false, move |_| {
            u.toggle();
            None
        });
        icon
    }
}

/// XAppStatusIcon from libxapp: the tray icon Cinnamon's XApp Status Applet shows
/// next to other apps. Without that applet libxapp falls back to GtkStatusIcon.
/// There are no Rust bindings, so the few calls needed are declared here.
mod tray {
    use std::ffi::{c_char, c_void, CString};

    use gtk::glib::translate::{FromGlibPtrFull, ToGlibPtr};
    use gtk::glib::{self, gobject_ffi::GObject};

    // Only the runtime `libxapp.so.1` is needed, not the libxapp-dev symlink.
    #[link(name = "libxapp.so.1", kind = "dylib", modifiers = "+verbatim")]
    extern "C" {
        fn xapp_status_icon_new() -> *mut GObject;
        fn xapp_status_icon_set_name(icon: *mut GObject, name: *const c_char);
        fn xapp_status_icon_set_icon_name(icon: *mut GObject, icon_name: *const c_char);
        fn xapp_status_icon_set_tooltip_text(icon: *mut GObject, text: *const c_char);
        fn xapp_status_icon_set_secondary_menu(icon: *mut GObject, menu: *mut c_void);
    }

    pub fn new_status_icon(
        name: &str,
        icon_name: &str,
        tooltip: &str,
        menu: &gtk::Menu,
    ) -> glib::Object {
        let name = CString::new(name).unwrap();
        let icon_name = CString::new(icon_name).unwrap();
        let tooltip = CString::new(tooltip).unwrap();
        let menu_ptr: *mut gtk::ffi::GtkMenu = menu.to_glib_none().0;
        // SAFETY: plain libxapp C calls on the main thread; the menu is kept
        // alive by the icon (it takes a reference), and the returned object is
        // a new reference that `from_glib_full` takes ownership of.
        unsafe {
            let ptr = xapp_status_icon_new();
            xapp_status_icon_set_name(ptr, name.as_ptr());
            xapp_status_icon_set_icon_name(ptr, icon_name.as_ptr());
            xapp_status_icon_set_tooltip_text(ptr, tooltip.as_ptr());
            xapp_status_icon_set_secondary_menu(ptr, menu_ptr.cast());
            glib::Object::from_glib_full(ptr)
        }
    }
}
