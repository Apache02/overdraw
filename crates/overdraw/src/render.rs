//! Cairo rendering of shapes.

use gtk::cairo::{Context, FontSlant, FontWeight, Format, ImageSurface, LineCap, LineJoin};
use gtk::gdk::prelude::GdkContextExt;
use gtk::gdk_pixbuf::Pixbuf;
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

/// Draws a shape at reduced opacity as a whole, so overlapping parts don't show through.
pub fn draw_shape_faded(cr: &Context, shape: &Shape, alpha: f64) {
    cr.push_group();
    draw_shape(cr, shape);
    if cr.pop_group_to_source().is_ok() {
        let _ = cr.paint_with_alpha(alpha);
    }
}

/// Draws `shapes` over a screen capture. `width` × `height` is the logical size the
/// shapes were drawn at; the capture may be larger on a HiDPI screen.
pub fn compose(
    background: &Pixbuf,
    width: i32,
    height: i32,
    shapes: &[Shape],
) -> Option<ImageSurface> {
    let (pw, ph) = (background.width(), background.height());
    let surface = ImageSurface::create(Format::Rgb24, pw, ph).ok()?;
    {
        let cr = Context::new(&surface).ok()?;
        cr.set_source_pixbuf(background, 0.0, 0.0);
        cr.paint().ok()?;
        cr.scale(f64::from(pw) / f64::from(width), f64::from(ph) / f64::from(height));
        for shape in shapes {
            draw_shape(&cr, shape);
        }
    }
    Some(surface)
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

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::gdk_pixbuf::Colorspace;
    use overdraw_core::PALETTE;

    /// RGB of a pixel; Rgb24 surfaces store each one as a native-endian 0xXXRRGGBB word.
    fn pixel(s: &mut ImageSurface, x: i32, y: i32) -> [u8; 3] {
        let stride = s.stride();
        let data = s.data().unwrap();
        let i = (y * stride + x * 4) as usize;
        let word = u32::from_ne_bytes(data[i..i + 4].try_into().unwrap());
        [(word >> 16) as u8, (word >> 8) as u8, word as u8]
    }

    #[test]
    fn compose_draws_shapes_scaled_over_background() {
        // A 2x capture of a 50x50 logical area, with a red line across the middle.
        let bg = Pixbuf::new(Colorspace::Rgb, false, 8, 100, 100).unwrap();
        bg.fill(0x000000ff);
        let line = Shape {
            kind: ShapeKind::Line(Point::new(0.0, 25.0), Point::new(50.0, 25.0)),
            color: PALETTE[0],
            width: 4.0,
            highlighter: false,
        };
        let mut shot = compose(&bg, 50, 50, &[line]).unwrap();
        assert_eq!((shot.width(), shot.height()), (100, 100));
        assert_eq!(pixel(&mut shot, 50, 50), [0xff, 0x3b, 0x30], "line lands at 2x");
        assert_eq!(pixel(&mut shot, 50, 10), [0, 0, 0], "background kept");
    }
}
