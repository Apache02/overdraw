//! Drawing session state: current tool, color, width and the gesture in progress.
//! Kept free of GTK so the input logic stays easy to follow and test.

use overdraw_core::model::{
    badge_radius_for_width, text_size_for_width, two_point_shape, Shape, ShapeKind,
};
use std::time::{Duration, Instant};

use overdraw_core::{Document, Point, Tool, PALETTE};

pub const MIN_WIDTH: f64 = 1.0;
pub const MAX_WIDTH: f64 = 30.0;
/// Opacity of the background layer under the drawing, offered in the tray menu.
/// 1.0 hides the screen completely, like a whiteboard or blackboard.
pub const BACKGROUND_OPACITIES: [f64; 7] = [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 1.0];
pub const DEFAULT_BACKGROUND_OPACITY: f64 = 0.3;

/// Color of the background layer under the drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Background {
    Black,
    White,
}

impl Background {
    pub const ALL: [Background; 2] = [Background::Black, Background::White];

    pub fn label(self) -> &'static str {
        match self {
            Background::Black => "Black",
            Background::White => "White",
        }
    }

    /// Gray level: 0.0 for black, 1.0 for white.
    pub fn level(self) -> f64 {
        match self {
            Background::Black => 0.0,
            Background::White => 1.0,
        }
    }
}
const ERASER_TOLERANCE: f64 = 6.0;
/// How long a fading shape stays fully visible, then how long it takes to vanish.
pub const FADE_HOLD: Duration = Duration::from_millis(1500);
pub const FADE_OUT: Duration = Duration::from_millis(1000);

/// Opacity of a fading shape of the given age, or `None` once it is gone.
pub fn fade_alpha(age: Duration) -> Option<f64> {
    let fading_for = age.checked_sub(FADE_HOLD).unwrap_or_default();
    (fading_for < FADE_OUT).then(|| 1.0 - fading_for.as_secs_f64() / FADE_OUT.as_secs_f64())
}

/// A shape drawn in fading mode. It is kept out of the document and its undo history.
struct FadingShape {
    shape: Shape,
    born: Instant,
}

struct Drag {
    tool: Tool,
    start: Point,
    current: Point,
    points: Vec<Point>,
    highlighter: bool,
    constrain: bool,
}

pub struct TextEdit {
    pub origin: Point,
    pub text: String,
}

pub struct State {
    pub doc: Document,
    pub tool: Tool,
    pub color: usize,
    pub width: f64,
    pub background: Background,
    /// Opacity of the background layer that covers the screen under the drawing.
    pub background_opacity: f64,
    /// Mouse clicks go through the canvas to the windows under it; the drawing stays.
    pub pass_through: bool,
    /// New shapes fade away after a moment instead of staying.
    pub fading_mode: bool,
    fading: Vec<FadingShape>,
    drag: Option<Drag>,
    pub text: Option<TextEdit>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            doc: Document::new(),
            tool: Tool::Pen,
            color: 0,
            width: 4.0,
            background: Background::Black,
            background_opacity: DEFAULT_BACKGROUND_OPACITY,
            pass_through: false,
            fading_mode: false,
            fading: Vec::new(),
            drag: None,
            text: None,
        }
    }
}

impl State {
    fn shape(&self, kind: ShapeKind, highlighter: bool) -> Shape {
        Shape { kind, color: PALETTE[self.color], width: self.width, highlighter }
    }

    pub fn set_width(&mut self, width: f64) {
        self.width = width.clamp(MIN_WIDTH, MAX_WIDTH);
    }

    /// Grows or shrinks the last drawn shape by `delta` of width, within the width limits.
    pub fn resize_last(&mut self, delta: f64) -> bool {
        let Some(last) = self.doc.shapes().last() else { return false };
        let width = (last.width + delta).clamp(MIN_WIDTH, MAX_WIDTH);
        self.doc.set_last_width(width)
    }

    /// Puts a finished shape on the canvas: into the document, or among the
    /// fading shapes in fading mode.
    fn add_shape(&mut self, shape: Shape) {
        if self.fading_mode {
            self.fading.push(FadingShape { shape, born: Instant::now() });
        } else {
            self.doc.add(shape);
        }
    }

    /// Fading shapes still visible at `now`, with their opacity.
    pub fn fading_shapes(&self, now: Instant) -> impl Iterator<Item = (&Shape, f64)> {
        self.fading
            .iter()
            .filter_map(move |f| Some((&f.shape, fade_alpha(now.duration_since(f.born))?)))
    }

    /// Drops fading shapes that are gone. Returns whether any are left.
    pub fn expire_fading(&mut self, now: Instant) -> bool {
        self.fading.retain(|f| fade_alpha(now.duration_since(f.born)).is_some());
        !self.fading.is_empty()
    }

    pub fn has_fading(&self) -> bool {
        !self.fading.is_empty()
    }

    /// Clears the canvas, fading shapes included.
    pub fn clear(&mut self) {
        self.doc.clear();
        self.fading.clear();
    }

    fn next_badge_number(&self) -> u32 {
        let fading_max = self
            .fading
            .iter()
            .filter_map(|f| match f.shape.kind {
                ShapeKind::Badge { number, .. } => Some(number),
                _ => None,
            })
            .max();
        self.doc.next_badge_number().max(fading_max.map_or(1, |n| n + 1))
    }

    /// Whether the background hides the screen completely.
    pub fn background_is_solid(&self) -> bool {
        self.shown_background_opacity() >= 1.0
    }

    /// Background opacity as drawn. Click-through halves it so the windows being
    /// clicked stay visible, without changing the chosen setting.
    pub fn shown_background_opacity(&self) -> f64 {
        if self.pass_through {
            self.background_opacity / 2.0
        } else {
            self.background_opacity
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn cancel_drag(&mut self) {
        if let Some(drag) = self.drag.take() {
            if drag.tool == Tool::Eraser {
                self.doc.end_erase();
            }
        }
    }

    /// `alt` turns the pen into a temporary highlighter.
    pub fn press(&mut self, p: Point, shift: bool, alt: bool) {
        self.commit_text();
        match self.tool {
            Tool::Text => self.text = Some(TextEdit { origin: p, text: String::new() }),
            Tool::Badge => {
                let kind = ShapeKind::Badge {
                    center: p,
                    number: self.next_badge_number(),
                    radius: badge_radius_for_width(self.width),
                };
                let badge = self.shape(kind, false);
                self.add_shape(badge);
            }
            tool => {
                if tool == Tool::Eraser {
                    self.doc.begin_erase();
                    self.doc.erase_at(p, ERASER_TOLERANCE);
                }
                self.drag = Some(Drag {
                    tool,
                    start: p,
                    current: p,
                    points: vec![p],
                    highlighter: tool == Tool::Highlighter || (tool == Tool::Pen && alt),
                    constrain: shift,
                });
            }
        }
    }

    pub fn motion(&mut self, p: Point, shift: bool) {
        let Some(drag) = self.drag.as_mut() else { return };
        drag.current = p;
        drag.constrain = shift;
        match drag.tool {
            Tool::Pen | Tool::Highlighter => {
                if drag.points.last().is_none_or(|last| last.distance(p) >= 1.0) {
                    drag.points.push(p);
                }
            }
            Tool::Eraser => {
                self.doc.erase_at(p, ERASER_TOLERANCE);
            }
            _ => {}
        }
    }

    pub fn release(&mut self) {
        let Some(drag) = self.drag.take() else { return };
        if drag.tool == Tool::Eraser {
            self.doc.end_erase();
            return;
        }
        let tiny = drag.tool.is_two_point() && drag.start.distance(drag.current) < 2.0;
        if !tiny {
            if let Some(shape) = self.preview_of(drag) {
                self.add_shape(shape);
            }
        }
    }

    fn preview_of(&self, drag: Drag) -> Option<Shape> {
        let kind = match drag.tool {
            Tool::Pen | Tool::Highlighter => ShapeKind::Stroke(drag.points),
            tool => two_point_shape(tool, drag.start, drag.current, drag.constrain)?,
        };
        Some(self.shape(kind, drag.highlighter))
    }

    /// The shape being drawn right now, if any.
    pub fn preview(&self) -> Option<Shape> {
        let d = self.drag.as_ref()?;
        let copy = Drag { points: d.points.clone(), ..*d };
        self.preview_of(copy)
    }

    /// Text being typed, as the shape it will become.
    pub fn text_preview(&self) -> Option<Shape> {
        let edit = self.text.as_ref()?;
        let kind = ShapeKind::Text {
            origin: edit.origin,
            text: edit.text.clone(),
            size: text_size_for_width(self.width),
        };
        Some(self.shape(kind, false))
    }

    pub fn commit_text(&mut self) {
        if let Some(shape) = self.text_preview() {
            self.text = None;
            if matches!(&shape.kind, ShapeKind::Text { text, .. } if !text.trim().is_empty()) {
                self.add_shape(shape);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pen_drag_adds_stroke() {
        let mut s = State::default();
        s.press(Point::new(0.0, 0.0), false, false);
        s.motion(Point::new(5.0, 5.0), false);
        s.motion(Point::new(10.0, 10.0), false);
        s.release();
        assert!(matches!(&s.doc.shapes()[0].kind, ShapeKind::Stroke(p) if p.len() == 3));
    }

    #[test]
    fn alt_pen_is_highlighter() {
        let mut s = State::default();
        s.press(Point::new(0.0, 0.0), false, true);
        s.release();
        assert!(s.doc.shapes()[0].highlighter);
    }

    #[test]
    fn click_without_drag_adds_no_rect() {
        let mut s = State { tool: Tool::Rect, ..State::default() };
        s.press(Point::new(3.0, 3.0), false, false);
        s.release();
        assert!(s.doc.shapes().is_empty());
    }

    #[test]
    fn text_commits_on_next_click() {
        let mut s = State { tool: Tool::Text, ..State::default() };
        s.press(Point::new(0.0, 0.0), false, false);
        s.text.as_mut().unwrap().text.push_str("hi");
        s.press(Point::new(50.0, 50.0), false, false);
        assert_eq!(s.doc.shapes().len(), 1);
        assert!(s.text.is_some(), "second click starts a new text");
        s.commit_text();
        assert_eq!(s.doc.shapes().len(), 1, "empty text is dropped");
    }

    #[test]
    fn resize_last_stays_within_limits() {
        let mut s = State::default();
        assert!(!s.resize_last(2.0), "nothing to resize");
        s.press(Point::new(0.0, 0.0), false, false);
        s.release();
        assert!(s.resize_last(100.0));
        assert_eq!(s.doc.shapes()[0].width, MAX_WIDTH);
        assert!(!s.resize_last(2.0));
        assert_eq!(s.width, 4.0, "the tool width is untouched");
    }

    #[test]
    fn fade_alpha_holds_then_fades() {
        assert_eq!(fade_alpha(Duration::ZERO), Some(1.0));
        assert_eq!(fade_alpha(FADE_HOLD), Some(1.0));
        let half = fade_alpha(FADE_HOLD + FADE_OUT / 2).unwrap();
        assert!((half - 0.5).abs() < 1e-9);
        assert_eq!(fade_alpha(FADE_HOLD + FADE_OUT), None);
    }

    #[test]
    fn fading_mode_keeps_shapes_out_of_the_document() {
        let mut s = State { fading_mode: true, ..State::default() };
        s.press(Point::new(0.0, 0.0), false, false);
        s.release();
        assert!(s.doc.shapes().is_empty());
        let now = Instant::now();
        assert_eq!(s.fading_shapes(now).count(), 1);
        assert!(s.expire_fading(now));
        assert!(!s.expire_fading(now + FADE_HOLD + FADE_OUT));
        assert!(!s.has_fading());
    }

    #[test]
    fn fading_badges_keep_counting() {
        let mut s = State { tool: Tool::Badge, fading_mode: true, ..State::default() };
        s.press(Point::new(0.0, 0.0), false, false);
        s.press(Point::new(40.0, 0.0), false, false);
        let numbers: Vec<u32> = s
            .fading_shapes(Instant::now())
            .filter_map(|(shape, _)| match shape.kind {
                ShapeKind::Badge { number, .. } => Some(number),
                _ => None,
            })
            .collect();
        assert_eq!(numbers, [1, 2]);
    }

    #[test]
    fn pass_through_halves_background_opacity() {
        let mut s = State { background_opacity: 1.0, ..State::default() };
        assert!(s.background_is_solid());
        s.pass_through = true;
        assert_eq!(s.shown_background_opacity(), 0.5);
        assert!(!s.background_is_solid());
        assert_eq!(s.background_opacity, 1.0, "the setting is kept");
        s.background_opacity = 0.5;
        assert_eq!(s.shown_background_opacity(), 0.25);
    }

    #[test]
    fn badges_count_up() {
        let mut s = State { tool: Tool::Badge, ..State::default() };
        s.press(Point::new(0.0, 0.0), false, false);
        s.press(Point::new(40.0, 0.0), false, false);
        assert!(matches!(s.doc.shapes()[1].kind, ShapeKind::Badge { number: 2, .. }));
    }
}
