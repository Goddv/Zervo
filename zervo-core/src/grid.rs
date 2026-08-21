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

#[cfg(test)]
mod tests {
    use egui::{pos2, vec2};

    use super::*;

    fn grid(columns: u8, rows: f32) -> Metrics {
        Metrics::new(
            Rect::from_min_size(pos2(10.0, 20.0), vec2(600.0, rows * 80.0 - 12.0)),
            columns,
            68.0,
            12.0,
        )
    }

    /// Two cards either share a cell or they do not, and which one is asked
    /// first cannot change the answer.
    #[test]
    fn overlapping_is_symmetric() {
        let a = (Cell::new(0, 0), Span::new(2, 2));
        let b = (Cell::new(1, 1), Span::new(2, 2));
        assert!(overlaps(a, b));
        assert_eq!(overlaps(a, b), overlaps(b, a));
    }

    /// Sitting next to something is not sitting on it. An off-by-one here puts
    /// a gap between every pair of cards, or refuses to place them at all.
    #[test]
    fn touching_is_not_overlapping() {
        let left = (Cell::new(0, 0), Span::new(2, 1));
        let right = (Cell::new(2, 0), Span::new(2, 1));
        assert!(!overlaps(left, right));
        // One column closer and they do share a cell.
        assert!(overlaps(left, (Cell::new(1, 0), Span::new(2, 1))));

        let above = (Cell::new(0, 0), Span::new(1, 2));
        let below = (Cell::new(0, 2), Span::new(1, 2));
        assert!(!overlaps(above, below));
        assert!(overlaps(above, (Cell::new(0, 1), Span::new(1, 1))));
    }

    /// A span of zero comes out of a settings file somebody edited, and a card
    /// occupying no cells would sit invisibly on top of another.
    #[test]
    fn a_card_always_occupies_something() {
        let nothing = (Cell::new(0, 0), Span::new(0, 0));
        assert!(overlaps(nothing, (Cell::new(0, 0), Span::new(1, 1))));
        assert_eq!(Span::new(0, 0).fit(12, 6), Span::new(1, 1));
    }

    #[test]
    fn a_card_is_pulled_back_until_it_fits() {
        // Wider than the grid: clamped to the grid, at the only column it fits.
        assert_eq!(Span::new(30, 30).fit(12, 6), Span::new(12, 6));
        assert_eq!(
            Cell::new(11, 5).fit(Span::new(2, 2), 12, 6),
            Cell::new(10, 4)
        );
        // Already inside, so nothing moves.
        assert_eq!(Cell::new(3, 1).fit(Span::new(2, 2), 12, 6), Cell::new(3, 1));
    }

    /// Fitting something that already fits must not move it. Anything else and
    /// a card creeps a cell every time the page is drawn.
    #[test]
    fn fitting_twice_is_the_same_as_fitting_once() {
        for col in 0..14 {
            for row in 0..8 {
                let once = Cell::new(col, row).fit(Span::new(3, 2), 12, 6);
                let twice = once.fit(Span::new(3, 2), 12, 6);
                assert_eq!(once, twice, "at ({col}, {row})");
            }
        }
    }

    #[test]
    fn the_first_free_cell_is_found_left_to_right_then_down() {
        assert_eq!(free_cell(&[], Span::new(1, 1), 6, 3), Some(Cell::new(0, 0)));
        let taken = [(Cell::new(0, 0), Span::new(1, 1))];
        assert_eq!(
            free_cell(&taken, Span::new(1, 1), 6, 3),
            Some(Cell::new(1, 0))
        );
        // A full first row sends it to the second.
        let row = [(Cell::new(0, 0), Span::new(6, 1))];
        assert_eq!(
            free_cell(&row, Span::new(1, 1), 6, 3),
            Some(Cell::new(0, 1))
        );
    }

    /// The answer, whatever it is, must be somewhere the card actually fits.
    /// This is the property the callers rely on and the one worth checking
    /// exhaustively rather than by example.
    #[test]
    fn a_free_cell_never_lands_on_anything() {
        let taken = [
            (Cell::new(0, 0), Span::new(2, 2)),
            (Cell::new(4, 0), Span::new(2, 1)),
            (Cell::new(3, 2), Span::new(1, 1)),
        ];
        for w in 1..=3_u8 {
            for h in 1..=3_u8 {
                let span = Span::new(w, h);
                let Some(at) = free_cell(&taken, span, 6, 4) else {
                    continue;
                };
                for other in &taken {
                    assert!(
                        !overlaps((at, span), *other),
                        "{span:?} placed at {at:?} lands on {other:?}"
                    );
                }
                // And inside the grid, not hanging off the end.
                let fitted = span.fit(6, 4);
                assert!(at.col + fitted.w <= 6 && at.row + fitted.h <= 4);
            }
        }
    }

    #[test]
    fn a_full_grid_has_nowhere_left() {
        let full = [(Cell::new(0, 0), Span::new(4, 2))];
        assert_eq!(free_cell(&full, Span::new(1, 1), 4, 2), None);
    }

    /// The round trip every drag depends on: drop a card where the outline says
    /// and it must land in the cell the outline was drawn for. Any error in
    /// `stride`, `origin` or the rounding shows up here.
    #[test]
    fn a_card_lands_in_the_cell_it_was_measured_for() {
        let metrics = grid(12, 6.0);
        let span = Span::new(2, 2);
        for col in 0..=(metrics.columns - span.w) {
            for row in 0..=(metrics.rows.saturating_sub(span.h)) {
                let cell = Cell::new(col, row);
                let landed = metrics.cell_at(metrics.rect(cell, span).min, span);
                assert_eq!(landed, cell, "round trip failed at {cell:?}");
            }
        }
    }

    /// A pointer nearer the next cell than this one rounds to the next.
    #[test]
    fn a_pointer_rounds_to_the_nearest_cell() {
        let metrics = grid(12, 6.0);
        let span = Span::new(1, 1);
        let first = metrics.rect(Cell::new(0, 0), span).min;
        let stride = metrics.cell.x + metrics.gap;
        assert_eq!(metrics.cell_at(first, span), Cell::new(0, 0));
        assert_eq!(
            metrics.cell_at(first + vec2(stride * 0.49, 0.0), span),
            Cell::new(0, 0)
        );
        assert_eq!(
            metrics.cell_at(first + vec2(stride * 0.51, 0.0), span),
            Cell::new(1, 0)
        );
    }

    /// A pointer dragged off the top or the left of the page must still name a
    /// cell on it.
    #[test]
    fn a_pointer_outside_the_grid_is_pulled_onto_it() {
        let metrics = grid(12, 6.0);
        let span = Span::new(2, 2);
        let far = metrics.cell_at(pos2(-4000.0, -4000.0), span);
        assert_eq!(far, Cell::new(0, 0));
        let beyond = metrics.cell_at(pos2(9000.0, 9000.0), span);
        assert!(beyond.col + span.w <= metrics.columns);
        assert!(beyond.row + span.h <= metrics.rows);
    }

    /// A grid with no room, and one with far too much, both have to produce a
    /// usable row count rather than zero or a wrapped `u8`.
    #[test]
    fn the_row_count_stays_between_one_and_two_hundred_and_fifty_five() {
        let flat = Metrics::new(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 0.0)),
            12,
            68.0,
            12.0,
        );
        assert_eq!(flat.rows, 1);
        let enormous = Metrics::new(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 5_000_000.0)),
            12,
            1.0,
            0.0,
        );
        assert_eq!(enormous.rows, 255);
    }

    /// A grid one column wide is legal, and a grid of zero columns is somebody
    /// else's arithmetic error that must not become a division by zero here.
    #[test]
    fn a_grid_always_has_a_column() {
        let none = Metrics::new(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 300.0)),
            0,
            68.0,
            12.0,
        );
        assert_eq!(none.columns, 1);
        assert!(none.cell.x.is_finite() && none.cell.x > 0.0);
    }

    /// What a container sizing itself around the grid asks, and what the grid
    /// actually draws, have to be the same number.
    #[test]
    fn the_height_of_n_rows_is_where_the_nth_row_ends() {
        let metrics = grid(12, 6.0);
        for rows in 1..=metrics.rows {
            let last = metrics.rect(Cell::new(0, rows - 1), Span::new(1, 1));
            let measured = last.max.y - metrics.origin.y;
            assert!(
                (metrics.height_for(rows) - measured).abs() < 0.01,
                "height_for({rows}) said {} but the row ends at {measured}",
                metrics.height_for(rows)
            );
        }
    }

    /// Cells narrow as the window does; the column count does not change. A
    /// grid that dropped columns would pile everything on the right into one
    /// place and the arrangement would not survive a resize.
    #[test]
    fn narrowing_the_window_narrows_the_cells_and_keeps_the_columns() {
        let wide = grid(12, 6.0);
        let narrow = Metrics::new(
            Rect::from_min_size(pos2(10.0, 20.0), vec2(300.0, 468.0)),
            12,
            68.0,
            12.0,
        );
        assert_eq!(wide.columns, narrow.columns);
        assert!(narrow.cell.x < wide.cell.x);
        assert!(narrow.cell.x >= 1.0);
    }
}
