//! Plain 2D geometry helpers used by the model and the renderer.

use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance(self, other: Point) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}

/// Distance from `p` to the segment `a`–`b`.
pub fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len_sq = dx * dx + dy * dy;
    if len_sq == 0.0 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0);
    p.distance(Point::new(a.x + t * dx, a.y + t * dy))
}

/// Snaps `b` so that the segment `a`–`b` lies on a multiple of 45°.
pub fn constrain_45(a: Point, b: Point) -> Point {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy);
    if len == 0.0 {
        return b;
    }
    let step = PI / 4.0;
    let angle = (dy.atan2(dx) / step).round() * step;
    Point::new(a.x + len * angle.cos(), a.y + len * angle.sin())
}

/// Moves `b` so that the box spanned by `a` and `b` is a square,
/// keeping the drag direction.
pub fn constrain_square(a: Point, b: Point) -> Point {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let side = dx.abs().max(dy.abs());
    Point::new(a.x + side * sign(dx), a.y + side * sign(dy))
}

fn sign(v: f64) -> f64 {
    if v < 0.0 {
        -1.0
    } else {
        1.0
    }
}

/// Normalised rectangle spanned by two corners: (x, y, width, height).
pub fn rect_from_corners(a: Point, b: Point) -> (f64, f64, f64, f64) {
    (a.x.min(b.x), a.y.min(b.y), (a.x - b.x).abs(), (a.y - b.y).abs())
}

/// The two end points of an arrow head at `tip`, pointing away from `tail`.
pub fn arrow_head(tail: Point, tip: Point, length: f64) -> (Point, Point) {
    let angle = (tip.y - tail.y).atan2(tip.x - tail.x);
    let spread = PI / 7.0;
    let wing = |a: f64| Point::new(tip.x - length * a.cos(), tip.y - length * a.sin());
    (wing(angle - spread), wing(angle + spread))
}

/// Polygon approximating the ellipse inscribed in the box `a`–`b`.
pub fn ellipse_polygon(a: Point, b: Point, segments: usize) -> Vec<Point> {
    let (x, y, w, h) = rect_from_corners(a, b);
    let (cx, cy, rx, ry) = (x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0);
    (0..=segments)
        .map(|i| {
            let t = 2.0 * PI * i as f64 / segments as f64;
            Point::new(cx + rx * t.cos(), cy + ry * t.sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-9
    }

    #[test]
    fn segment_distance() {
        let (a, b) = (Point::new(0.0, 0.0), Point::new(10.0, 0.0));
        assert_eq!(distance_to_segment(Point::new(5.0, 3.0), a, b), 3.0);
        assert_eq!(distance_to_segment(Point::new(-4.0, 3.0), a, b), 5.0);
        assert_eq!(distance_to_segment(Point::new(1.0, 1.0), a, a), 2f64.sqrt());
    }

    #[test]
    fn snaps_to_45_degrees() {
        let a = Point::new(0.0, 0.0);
        let b = constrain_45(a, Point::new(10.0, 1.0));
        assert!(close(b, Point::new(10.0f64.hypot(1.0), 0.0)));
        let d = constrain_45(a, Point::new(10.0, 9.0));
        assert!((d.x - d.y).abs() < 1e-9);
    }

    #[test]
    fn square_keeps_direction() {
        let a = Point::new(10.0, 10.0);
        assert_eq!(constrain_square(a, Point::new(0.0, 30.0)), Point::new(-10.0, 30.0));
        assert_eq!(constrain_square(a, Point::new(15.0, 12.0)), Point::new(15.0, 15.0));
    }

    #[test]
    fn rect_is_normalised() {
        let r = rect_from_corners(Point::new(5.0, 8.0), Point::new(1.0, 2.0));
        assert_eq!(r, (1.0, 2.0, 4.0, 6.0));
    }

    #[test]
    fn arrow_head_is_behind_tip() {
        let (l, r) = arrow_head(Point::new(0.0, 0.0), Point::new(10.0, 0.0), 4.0);
        assert!(l.x < 10.0 && r.x < 10.0);
        assert!((l.y + r.y).abs() < 1e-9);
    }
}
