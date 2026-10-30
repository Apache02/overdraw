//! Drawing session state: current tool, color, width and the gesture in progress.
//! Kept free of GTK so the input logic stays easy to follow and test.

use overdraw_core::model::{
    badge_radius_for_width, text_size_for_width, two_point_shape, Shape, ShapeKind,
};
use overdraw_core::{Document, Point, Tool, PALETTE};

pub const MIN_WIDTH: f64 = 1.0;
pub const MAX_WIDTH: f64 = 30.0;
/// Opacity of the black layer under the drawing, offered in the tray menu.
pub const DIM_LEVELS: [f64; 6] = [0.0, 0.1, 0.2, 0.3, 0.4, 0.5];
pub const DEFAULT_DIM: f64 = 0.3;
const ERASER_TOLERANCE: f64 = 6.0;

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
    /// Opacity of the black layer that dims the screen under the drawing.
    pub dim: f64,
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
            dim: DEFAULT_DIM,
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
                    number: self.doc.next_badge_number(),
                    radius: badge_radius_for_width(self.width),
                };
                let badge = self.shape(kind, false);
                self.doc.add(badge);
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
                self.doc.add(shape);
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
                self.doc.add(shape);
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
    fn badges_count_up() {
        let mut s = State { tool: Tool::Badge, ..State::default() };
        s.press(Point::new(0.0, 0.0), false, false);
        s.press(Point::new(40.0, 0.0), false, false);
        assert!(matches!(s.doc.shapes()[1].kind, ShapeKind::Badge { number: 2, .. }));
    }
}
