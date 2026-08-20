//! Cards on a grid: where one sits, what it collides with, and which cell the
//! pointer is over.
//!
//! Two things in Zervo arrange cards this way — the shelf a taller navigation
//! bar uncovers, and the new tab page — and they want the same four answers,
//! so the answers live here rather than twice. What they do *not* share is the
//! shape of the grid: the shelf is a few fixed rows deep, the page is as deep
//! as the window allows. That part stays with each caller.

use egui::{Pos2, Rect, Vec2, pos2, vec2};
use serde::{Deserialize, Serialize};

/// How many cells a card takes, across and down.
///
/// Zero is meaningless, so both are pulled up to one wherever they are used
/// rather than trusted: these come back from a settings file anyone can edit.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Span {
    pub w: u8,
    pub h: u8,
}

impl Span {
    pub const fn new(w: u8, h: u8) -> Self {
        Span { w, h }
    }

    /// Clamped to a grid of this shape, so a card wider than the grid still
    /// has somewhere to go instead of hanging off the end.
    pub fn fit(self, columns: u8, rows: u8) -> Self {
        Span {
            w: self.w.clamp(1, columns.max(1)),
            h: self.h.clamp(1, rows.max(1)),
        }
    }
}

/// Where a card's top-left corner sits.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Cell {
    pub col: u8,
    pub row: u8,
}

impl Cell {
    pub const fn new(col: u8, row: u8) -> Self {
        Cell { col, row }
    }

    /// Pulled back onto the grid until a card of `span` fits entirely inside.
    pub fn fit(self, span: Span, columns: u8, rows: u8) -> Self {
        let span = span.fit(columns, rows);
        Cell {
            col: self.col.min(columns.saturating_sub(span.w)),
            row: self.row.min(rows.saturating_sub(span.h)),
        }
    }
}

/// A card as the grid sees it: a corner and a footprint.
pub type Placement = (Cell, Span);

/// Do two placed cards share any cell?
pub fn overlaps(a: Placement, b: Placement) -> bool {
    let (a_at, a_span) = a;
    let (b_at, b_span) = b;
    a_at.col < b_at.col.saturating_add(b_span.w.max(1))
        && b_at.col < a_at.col.saturating_add(a_span.w.max(1))
        && a_at.row < b_at.row.saturating_add(b_span.h.max(1))
        && b_at.row < a_at.row.saturating_add(a_span.h.max(1))
}

/// The first cell a card of `span` fits in, scanning left to right and top to
/// bottom, or `None` when there is nowhere left. Callers that must place a
/// card regardless fall back to the origin and let the drop land on top.
pub fn free_cell(taken: &[Placement], span: Span, columns: u8, rows: u8) -> Option<Cell> {
    let span = span.fit(columns, rows);
    for row in 0..=rows.saturating_sub(span.h) {
        'next: for col in 0..=columns.saturating_sub(span.w) {
            let candidate = (Cell::new(col, row), span);
            for other in taken {
                if overlaps(candidate, *other) {
                    continue 'next;
                }
            }
            return Some(Cell::new(col, row));
        }
    }
    None
}

/// The shape of a grid laid over a rectangle.
#[derive(Clone, Copy)]
pub struct Metrics {
    pub columns: u8,
    pub rows: u8,
    /// One cell, in points.
    pub cell: Vec2,
    pub gap: f32,
    /// The top-left corner of cell (0, 0).
    pub origin: Pos2,
}

impl Metrics {
    /// A grid of `columns` columns across `area`, with rows of a fixed height.
    ///
    /// The column count never changes with the window: cells narrow instead,
    /// which is what keeps a card at column six at column six in any window.
    /// A grid that dropped columns as it narrowed would pile everything on the
    /// right into the same place, and the arrangement would not survive one
    /// resize.
    pub fn new(area: Rect, columns: u8, row_height: f32, gap: f32) -> Self {
        let columns = columns.max(1);
        let cell_width = ((area.width() - (columns - 1) as f32 * gap) / columns as f32).max(1.0);
        let rows =
            (((area.height() + gap) / (row_height + gap)).floor() as i32).clamp(1, 255) as u8;
        Metrics {
            columns,
            rows,
            cell: vec2(cell_width, row_height),
            gap,
            origin: area.min,
        }
    }

    /// One step from a cell to the next, corner to corner.
    fn stride(&self) -> Vec2 {
        self.cell + Vec2::splat(self.gap)
    }

    /// What a card of `span` measures.
    pub fn size(&self, span: Span) -> Vec2 {
        let span = span.fit(self.columns, self.rows);
        vec2(
            span.w as f32 * self.cell.x + (span.w - 1) as f32 * self.gap,
            span.h as f32 * self.cell.y + (span.h - 1) as f32 * self.gap,
        )
    }

    /// Where a card of `span` placed at `at` lands.
    pub fn rect(&self, at: Cell, span: Span) -> Rect {
        let at = at.fit(span, self.columns, self.rows);
        let stride = self.stride();
        Rect::from_min_size(
            pos2(
                self.origin.x + at.col as f32 * stride.x,
                self.origin.y + at.row as f32 * stride.y,
            ),
            self.size(span),
        )
    }

    /// The cell a card of `span` would take if its top-left corner were at
    /// `pos`, rounded to the nearest and clamped onto the grid.
    pub fn cell_at(&self, pos: Pos2, span: Span) -> Cell {
        let stride = self.stride();
        let col = ((pos.x - self.origin.x) / stride.x).round();
        let row = ((pos.y - self.origin.y) / stride.y).round();
        Cell::new(col.clamp(0.0, 255.0) as u8, row.clamp(0.0, 255.0) as u8).fit(
            span,
            self.columns,
            self.rows,
        )
    }

    /// The height `rows` whole rows come to — what a caller sizing a container
    /// around the grid needs, rather than guessing at it.
    pub fn height_for(&self, rows: u8) -> f32 {
        let rows = rows.max(1) as f32;
        rows * self.cell.y + (rows - 1.0) * self.gap
    }
}
