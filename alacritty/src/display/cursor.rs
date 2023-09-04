//! Convert a cursor into an iterator of rects.
use alacritty_terminal::index::Point;

use alacritty_terminal::ansi::CursorShape;
use alacritty_terminal::term::color::Rgb;

use crate::display::content::RenderableCursor;
use crate::display::SizeInfo;
use crate::renderer::quads::{QuadPoint, RenderQuad};

/// Trait for conversion into the iterator.
pub trait IntoRects {
    /// Consume the cursor for an iterator of rects.
    fn quads(&mut self, size_info: &SizeInfo, thickness: f32) -> RenderQuad;
}

impl IntoRects for RenderableCursor {
    fn quads(&mut self, size_info: &SizeInfo, thickness: f32) -> RenderQuad {
        self.data.update(self.shape(), size_info, thickness, self.point(), self.is_wide());

        RenderQuad::new(self.data.positions, self.color(), 1.0)
    }
}

/// Cursor rect iterator.
#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct CursorRects {
    vels: [f32; 4],
    pub positions: [QuadPoint; 4],
    index: usize,
}

impl CursorRects {
    fn update(
        &mut self,
        shape: CursorShape,
        size_info: &SizeInfo,
        thickness: f32,
        point: Point<usize>,
        is_wide: bool,
    ) {
        let x = point.column.0 as f32 * size_info.cell_width() + size_info.padding_x();
        let y = point.line as f32 * size_info.cell_height() + size_info.padding_y();

        let mut width = size_info.cell_width();
        let height = size_info.cell_height();

        let thickness = (thickness * width).round().max(1.);

        if is_wide {
            width *= 2.;
        }

        let target = match shape {
            CursorShape::Beam => beam(x, y, height, thickness),
            CursorShape::Underline => underline(x, y, width, height, thickness),
            //CursorShape::HollowBlock => hollow(x, y, width, height, thickness),
            _ => beam(x, y, height, width),
        };

        for i in 0..4 {
            let diffx = target[i].x - self.positions[i].x;
            let diffy = target[i].y - self.positions[i].y;
            let dist_sq = diffx * diffx + diffy * diffy;

            let dist_diag_sq =
                size_info.width() * size_info.width() + size_info.height() * size_info.height();

            self.vels[i] *= 0.90;
            self.vels[i] += (dist_sq / dist_diag_sq) * 0.1;

            self.vels[i] = self.vels[i].clamp(0.2, 1.0);

            self.positions[i].x *= 1.0 - self.vels[i];
            self.positions[i].y *= 1.0 - self.vels[i];
            self.positions[i].x += target[i].x * (self.vels[i]);
            self.positions[i].y += target[i].y * (self.vels[i]);
        }
    }
}

impl From<RenderQuad> for CursorRects {
    fn from(rect: RenderQuad) -> Self {
        Self { positions: Default::default(), vels: Default::default(), index: 0 }
    }
}

/// Create an iterator yielding a single beam rect.
fn beam(x: f32, y: f32, height: f32, thickness: f32) -> [QuadPoint; 4] {
    [
        QuadPoint { x, y },
        QuadPoint { x: x + thickness, y },
        QuadPoint { x: x + thickness, y: y + height },
        QuadPoint { x, y: y + height },
    ]
}

/// Create an iterator yielding a single underline rect.
fn underline(x: f32, y: f32, width: f32, height: f32, thickness: f32) -> [QuadPoint; 4] {
    let y = y + height - thickness;
    [
        QuadPoint { x, y },
        QuadPoint { x: x + width, y },
        QuadPoint { x: x + width, y: y + thickness },
        QuadPoint { x, y: y + thickness },
    ]
}

// Create an iterator yielding a rect for each side of the hollow block cursor.
//fn hollow(x: f32, y: f32, width: f32, height: f32, thickness: f32, color: Rgb) -> CursorRects {
//    let top_line = RenderRect::new(x, y, width, thickness, color, 1.);
//
//    let vertical_y = y + thickness;
//    let vertical_height = height - 2. * thickness;
//    let left_line = RenderRect::new(x, vertical_y, thickness, vertical_height, color, 1.);
//
//    let bottom_y = y + height - thickness;
//    let bottom_line = RenderRect::new(x, bottom_y, width, thickness, color, 1.);
//
//    let right_x = x + width - thickness;
//    let right_line = RenderRect::new(right_x, vertical_y, thickness, vertical_height, color, 1.);
//
//    CursorRects {
//        rects: [Some(top_line), Some(bottom_line), Some(left_line), Some(right_line)],
//        index: 0,
//    }
//}
