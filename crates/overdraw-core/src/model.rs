//! Shapes, tools and the document with undo/redo.

use crate::geometry::{
    constrain_45, constrain_square, distance_to_segment, ellipse_polygon, rect_from_corners, Point,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r: r as f64 / 255.0, g: g as f64 / 255.0, b: b as f64 / 255.0 }
    }
}

/// Colors selected with keys 1–9.
pub const PALETTE: [Color; 9] = [
    Color::rgb(0xff, 0x3b, 0x30), // red
    Color::rgb(0xff, 0x95, 0x00), // orange
    Color::rgb(0xff, 0xd6, 0x0a), // yellow
    Color::rgb(0x34, 0xc7, 0x59), // green
    Color::rgb(0x0a, 0x84, 0xff), // blue
    Color::rgb(0xaf, 0x52, 0xde), // purple
    Color::rgb(0xff, 0x2d, 0x91), // pink
    Color::rgb(0xff, 0xff, 0xff), // white
    Color::rgb(0x1c, 0x1c, 0x1e), // black
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Pen,
    Highlighter,
    Line,
    Arrow,
    Rect,
    Ellipse,
    Text,
    Badge,
    Eraser,
}

impl Tool {
    pub const ALL: [Tool; 9] = [
        Tool::Pen,
        Tool::Highlighter,
        Tool::Line,
        Tool::Arrow,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Text,
        Tool::Badge,
        Tool::Eraser,
    ];

    /// Keyboard shortcut for the tool (lowercase letter).
    pub fn shortcut(self) -> char {
        match self {
            Tool::Pen => 'p',
            Tool::Highlighter => 'h',
            Tool::Line => 'l',
            Tool::Arrow => 'a',
            Tool::Rect => 'r',
            Tool::Ellipse => 'o',
            Tool::Text => 't',
            Tool::Badge => 'n',
            Tool::Eraser => 'e',
        }
    }

    pub fn from_shortcut(c: char) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.shortcut() == c)
    }

    /// Whether the tool creates a shape by dragging from one corner to another.
    pub fn is_two_point(self) -> bool {
        matches!(self, Tool::Line | Tool::Arrow | Tool::Rect | Tool::Ellipse)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShapeKind {
    Stroke(Vec<Point>),
    Line(Point, Point),
    Arrow(Point, Point),
    Rect(Point, Point),
    Ellipse(Point, Point),
    Text { origin: Point, text: String, size: f64 },
    Badge { center: Point, number: u32, radius: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub kind: ShapeKind,
    pub color: Color,
    pub width: f64,
    /// Drawn translucent and wide, like a marker.
    pub highlighter: bool,
}

/// Opacity the renderer uses for highlighter strokes.
pub const HIGHLIGHTER_ALPHA: f64 = 0.35;
/// Highlighter strokes are this many times wider than the selected width.
pub const HIGHLIGHTER_SCALE: f64 = 4.0;

impl Shape {
    /// Line width the shape is actually drawn with.
    pub fn stroke_width(&self) -> f64 {
        if self.highlighter {
            self.width * HIGHLIGHTER_SCALE
        } else {
            self.width
        }
    }

    /// Changes the width setting, rescaling text and badges the same way the tools size them.
    pub fn set_width(&mut self, width: f64) {
        self.width = width;
        match &mut self.kind {
            ShapeKind::Text { size, .. } => *size = text_size_for_width(width),
            ShapeKind::Badge { radius, .. } => *radius = badge_radius_for_width(width),
            _ => {}
        }
    }

    /// Whether `p` touches the shape, with `tolerance` extra pixels of slack.
    pub fn hit(&self, p: Point, tolerance: f64) -> bool {
        let reach = tolerance + self.stroke_width() / 2.0;
        let near_polyline = |pts: &[Point]| match pts {
            [] => false,
            [only] => p.distance(*only) <= reach,
            _ => pts.windows(2).any(|w| distance_to_segment(p, w[0], w[1]) <= reach),
        };
        match &self.kind {
            ShapeKind::Stroke(points) => near_polyline(points),
            ShapeKind::Line(a, b) | ShapeKind::Arrow(a, b) => distance_to_segment(p, *a, *b) <= reach,
            ShapeKind::Rect(a, b) => {
                let (x, y, w, h) = rect_from_corners(*a, *b);
                let c = [
                    Point::new(x, y),
                    Point::new(x + w, y),
                    Point::new(x + w, y + h),
                    Point::new(x, y + h),
                    Point::new(x, y),
                ];
                near_polyline(&c)
            }
            ShapeKind::Ellipse(a, b) => near_polyline(&ellipse_polygon(*a, *b, 64)),
            ShapeKind::Text { origin, text, size } => {
                let (w, h) = text_extent(text, *size);
                p.x >= origin.x - tolerance
                    && p.x <= origin.x + w + tolerance
                    && p.y >= origin.y - tolerance
                    && p.y <= origin.y + h + tolerance
            }
            ShapeKind::Badge { center, radius, .. } => p.distance(*center) <= radius + tolerance,
        }
    }
}

/// Rough size of a single-line text; good enough for hit-testing.
pub fn text_extent(text: &str, size: f64) -> (f64, f64) {
    (text.chars().count() as f64 * size * 0.6, size * 1.25)
}

/// Font size used by the text tool for a given width setting.
pub fn text_size_for_width(width: f64) -> f64 {
    14.0 + width * 3.0
}

/// Badge radius used for a given width setting.
pub fn badge_radius_for_width(width: f64) -> f64 {
    12.0 + width * 1.5
}

/// Builds the shape a two-point tool produces for a drag from `a` to `b`.
/// With `constrain`, lines snap to 45° and boxes become squares.
pub fn two_point_shape(tool: Tool, a: Point, b: Point, constrain: bool) -> Option<ShapeKind> {
    let snapped = |f: fn(Point, Point) -> Point| if constrain { f(a, b) } else { b };
    match tool {
        Tool::Line => Some(ShapeKind::Line(a, snapped(constrain_45))),
        Tool::Arrow => Some(ShapeKind::Arrow(a, snapped(constrain_45))),
        Tool::Rect => Some(ShapeKind::Rect(a, snapped(constrain_square))),
        Tool::Ellipse => Some(ShapeKind::Ellipse(a, snapped(constrain_square))),
        _ => None,
    }
}

/// All shapes on one canvas, with snapshot-based undo/redo.
#[derive(Debug, Default)]
pub struct Document {
    shapes: Vec<Shape>,
    undo: Vec<Vec<Shape>>,
    redo: Vec<Vec<Shape>>,
    erase_start: Option<Vec<Shape>>,
    /// Set while consecutive resizes of the last shape share one undo step.
    resizing: bool,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    fn checkpoint(&mut self, before: Vec<Shape>) {
        self.undo.push(before);
        self.redo.clear();
        self.resizing = false;
    }

    pub fn add(&mut self, shape: Shape) {
        self.checkpoint(self.shapes.clone());
        self.shapes.push(shape);
    }

    pub fn clear(&mut self) {
        if !self.shapes.is_empty() {
            let before = std::mem::take(&mut self.shapes);
            self.checkpoint(before);
        }
    }

    /// Starts an eraser drag; everything erased until [`end_erase`] is one undo step.
    ///
    /// [`end_erase`]: Document::end_erase
    pub fn begin_erase(&mut self) {
        self.erase_start = Some(self.shapes.clone());
    }

    /// Removes every shape under `p`. Returns whether anything was removed.
    pub fn erase_at(&mut self, p: Point, tolerance: f64) -> bool {
        let before = self.shapes.len();
        self.shapes.retain(|s| !s.hit(p, tolerance));
        self.shapes.len() != before
    }

    pub fn end_erase(&mut self) {
        if let Some(start) = self.erase_start.take() {
            if start != self.shapes {
                self.checkpoint(start);
            }
        }
    }

    /// Sets the width of the most recently added shape. A run of resizes with
    /// nothing else in between is one undo step.
    pub fn set_last_width(&mut self, width: f64) -> bool {
        match self.shapes.last() {
            Some(last) if last.width != width => {}
            _ => return false,
        }
        if !self.resizing {
            self.checkpoint(self.shapes.clone());
            self.resizing = true;
        }
        if let Some(last) = self.shapes.last_mut() {
            last.set_width(width);
        }
        true
    }

    pub fn undo(&mut self) -> bool {
        self.resizing = false;
        match self.undo.pop() {
            Some(prev) => {
                self.redo.push(std::mem::replace(&mut self.shapes, prev));
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        self.resizing = false;
        match self.redo.pop() {
            Some(next) => {
                self.undo.push(std::mem::replace(&mut self.shapes, next));
                true
            }
            None => false,
        }
    }

    /// Number for the next numbered badge.
    pub fn next_badge_number(&self) -> u32 {
        let max = self
            .shapes
            .iter()
            .filter_map(|s| match s.kind {
                ShapeKind::Badge { number, .. } => Some(number),
                _ => None,
            })
            .max();
        max.unwrap_or(0) + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x: f64) -> Shape {
        Shape {
            kind: ShapeKind::Line(Point::new(x, 0.0), Point::new(x, 100.0)),
            color: PALETTE[0],
            width: 4.0,
            highlighter: false,
        }
    }

    fn badge(number: u32) -> Shape {
        Shape {
            kind: ShapeKind::Badge { center: Point::new(0.0, 0.0), number, radius: 10.0 },
            ..line(0.0)
        }
    }

    #[test]
    fn undo_redo_roundtrip() {
        let mut doc = Document::new();
        doc.add(line(0.0));
        doc.add(line(10.0));
        assert!(doc.undo());
        assert_eq!(doc.shapes(), &[line(0.0)]);
        assert!(doc.redo());
        assert_eq!(doc.shapes().len(), 2);
        assert!(!doc.redo());
    }

    #[test]
    fn new_change_drops_redo() {
        let mut doc = Document::new();
        doc.add(line(0.0));
        doc.undo();
        doc.add(line(5.0));
        assert!(!doc.redo());
        assert_eq!(doc.shapes(), &[line(5.0)]);
    }

    #[test]
    fn clear_is_undoable() {
        let mut doc = Document::new();
        doc.add(line(0.0));
        doc.clear();
        assert!(doc.shapes().is_empty());
        doc.undo();
        assert_eq!(doc.shapes().len(), 1);
    }

    #[test]
    fn erase_drag_is_one_undo_step() {
        let mut doc = Document::new();
        doc.add(line(0.0));
        doc.add(line(50.0));
        doc.begin_erase();
        assert!(doc.erase_at(Point::new(1.0, 50.0), 2.0));
        assert!(doc.erase_at(Point::new(49.0, 50.0), 2.0));
        assert!(!doc.erase_at(Point::new(25.0, 50.0), 2.0));
        doc.end_erase();
        assert!(doc.shapes().is_empty());
        doc.undo();
        assert_eq!(doc.shapes().len(), 2);
    }

    #[test]
    fn erase_miss_adds_no_undo_step() {
        let mut doc = Document::new();
        doc.add(line(0.0));
        doc.begin_erase();
        doc.erase_at(Point::new(500.0, 500.0), 2.0);
        doc.end_erase();
        doc.undo();
        assert!(doc.shapes().is_empty());
    }

    #[test]
    fn badge_numbers_continue_from_max() {
        let mut doc = Document::new();
        assert_eq!(doc.next_badge_number(), 1);
        doc.add(badge(1));
        doc.add(badge(4));
        assert_eq!(doc.next_badge_number(), 5);
    }

    #[test]
    fn resizing_last_shape_is_one_undo_step() {
        let mut doc = Document::new();
        doc.add(line(0.0));
        doc.add(badge(1));
        assert!(doc.set_last_width(6.0));
        assert!(doc.set_last_width(8.0));
        assert!(!doc.set_last_width(8.0), "same width is no change");
        let last = &doc.shapes()[1];
        assert_eq!(last.width, 8.0);
        assert!(matches!(last.kind, ShapeKind::Badge { radius, .. } if radius == badge_radius_for_width(8.0)));
        assert_eq!(doc.shapes()[0].width, 4.0, "only the last shape changes");
        doc.undo();
        assert_eq!(doc.shapes()[1], badge(1));
        doc.undo();
        assert_eq!(doc.shapes().len(), 1);
    }

    #[test]
    fn resizing_after_undo_starts_a_new_step() {
        let mut doc = Document::new();
        doc.add(line(0.0));
        doc.set_last_width(6.0);
        doc.undo();
        doc.set_last_width(10.0);
        doc.undo();
        assert_eq!(doc.shapes(), &[line(0.0)]);
    }

    #[test]
    fn hit_tests() {
        let rect = Shape {
            kind: ShapeKind::Rect(Point::new(0.0, 0.0), Point::new(100.0, 50.0)),
            ..line(0.0)
        };
        assert!(rect.hit(Point::new(50.0, 1.0), 2.0));
        assert!(!rect.hit(Point::new(50.0, 25.0), 2.0), "inside of an outline is empty");

        let ellipse = Shape {
            kind: ShapeKind::Ellipse(Point::new(0.0, 0.0), Point::new(100.0, 50.0)),
            ..line(0.0)
        };
        assert!(ellipse.hit(Point::new(100.0, 25.0), 2.0));
        assert!(!ellipse.hit(Point::new(50.0, 25.0), 2.0));

        let stroke = Shape {
            kind: ShapeKind::Stroke(vec![Point::new(0.0, 0.0), Point::new(10.0, 10.0)]),
            ..line(0.0)
        };
        assert!(stroke.hit(Point::new(5.0, 5.0), 0.0));
        assert!(!stroke.hit(Point::new(10.0, 0.0), 0.0));
    }

    #[test]
    fn two_point_constrained() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(30.0, 10.0);
        assert_eq!(
            two_point_shape(Tool::Rect, a, b, true),
            Some(ShapeKind::Rect(a, Point::new(30.0, 30.0)))
        );
        assert_eq!(two_point_shape(Tool::Pen, a, b, true), None);
    }

    #[test]
    fn shortcuts_roundtrip() {
        for t in Tool::ALL {
            assert_eq!(Tool::from_shortcut(t.shortcut()), Some(t));
        }
    }
}
