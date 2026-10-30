//! Cairo rendering of shapes.

use gtk::cairo::{Context, FontSlant, FontWeight, LineCap, LineJoin};
use overdraw_core::geometry::{arrow_head, rect_from_corners, Point};
use overdraw_core::model::{Color, Shape, ShapeKind, HIGHLIGHTER_ALPHA};

const FONT: &str = "Sans";
const BADGE_ALPHA: f64 = 0.8;

fn set_color(cr: &Context, c: Color, alpha: f64) {
    cr.set_source_rgba(c.r, c.g, c.b, alpha);
}

fn polyline(cr: &Context, points: &[Point]) {
    if let Some((first, rest)) = points.split_first() {
        cr.move_to(first.x, first.y);
        for p in rest {
            cr.line_to(p.x, p.y);
        }
    }
}

/// Text is drawn with its top-left corner at `origin`.
fn draw_text(cr: &Context, origin: Point, text: &str, size: f64) {
    cr.select_font_face(FONT, FontSlant::Normal, FontWeight::Bold);
    cr.set_font_size(size);
    cr.move_to(origin.x, origin.y + size);
    let _ = cr.show_text(text);
}

pub fn draw_shape(cr: &Context, shape: &Shape) {
    let alpha = if shape.highlighter { HIGHLIGHTER_ALPHA } else { 1.0 };
    let width = shape.stroke_width();
    set_color(cr, shape.color, alpha);
    cr.set_line_width(width);
    cr.set_line_cap(LineCap::Round);
    cr.set_line_join(LineJoin::Round);

    match &shape.kind {
        ShapeKind::Stroke(points) => {
            if let [only] = points.as_slice() {
                cr.arc(only.x, only.y, width / 2.0, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
            } else {
                polyline(cr, points);
                let _ = cr.stroke();
            }
        }
        ShapeKind::Line(a, b) => {
            polyline(cr, &[*a, *b]);
            let _ = cr.stroke();
        }
        ShapeKind::Arrow(a, b) => {
            let (l, r) = arrow_head(*a, *b, (width * 4.0).max(14.0));
            polyline(cr, &[*a, *b]);
            polyline(cr, &[l, *b, r]);
            let _ = cr.stroke();
        }
        ShapeKind::Rect(a, b) => {
            let (x, y, w, h) = rect_from_corners(*a, *b);
            cr.rectangle(x, y, w, h);
            let _ = cr.stroke();
        }
        ShapeKind::Ellipse(a, b) => {
            let (x, y, w, h) = rect_from_corners(*a, *b);
            if w > 0.0 && h > 0.0 {
                let _ = cr.save();
                cr.translate(x + w / 2.0, y + h / 2.0);
                cr.scale(w / 2.0, h / 2.0);
                cr.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
                // Restore before stroking so the line width is not scaled.
                let _ = cr.restore();
                let _ = cr.stroke();
            }
        }
        ShapeKind::Text { origin, text, size } => draw_text(cr, *origin, text, *size),
        ShapeKind::Badge { center, number, radius } => {
            // Drawn as a group so the number does not let the circle show through.
            cr.push_group();
            cr.arc(center.x, center.y, *radius, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
            let c = shape.color;
            let light = 0.299 * c.r + 0.587 * c.g + 0.114 * c.b > 0.6;
            let ink = if light { 0.0 } else { 1.0 };
            cr.set_source_rgb(ink, ink, ink);
            cr.select_font_face(FONT, FontSlant::Normal, FontWeight::Bold);
            cr.set_font_size(radius * 1.1);
            let label = number.to_string();
            if let Ok(ext) = cr.text_extents(&label) {
                cr.move_to(
                    center.x - ext.width() / 2.0 - ext.x_bearing(),
                    center.y - ext.height() / 2.0 - ext.y_bearing(),
                );
                let _ = cr.show_text(&label);
            }
            if cr.pop_group_to_source().is_ok() {
                let _ = cr.paint_with_alpha(BADGE_ALPHA);
            }
        }
    }
}

/// Text being typed, with a caret after it.
pub fn draw_text_edit(cr: &Context, shape: &Shape) {
    draw_shape(cr, shape);
    if let ShapeKind::Text { origin, text, size } = &shape.kind {
        let advance = cr.text_extents(text).map(|e| e.x_advance()).unwrap_or(0.0);
        let x = origin.x + advance + 2.0;
        set_color(cr, shape.color, 1.0);
        cr.set_line_width(2.0);
        polyline(cr, &[Point::new(x, origin.y + size * 0.1), Point::new(x, origin.y + size * 1.2)]);
        let _ = cr.stroke();
    }
}
