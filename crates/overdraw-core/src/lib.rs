//! Toolkit-independent core of overdraw: geometry, shapes and the document.

pub mod geometry;
pub mod model;

pub use geometry::Point;
pub use model::{Color, Document, Shape, ShapeKind, Tool, PALETTE};
