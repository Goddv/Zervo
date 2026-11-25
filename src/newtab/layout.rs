//! Persistent widget geometry. Narrow windows project the same layout into
//! one column without changing the saved coordinates.

use crate::grid::{self, Cell, Span};
use serde::{Deserialize, Serialize};

use super::Widget;

pub(crate) const COLUMNS: u8 = 6;
pub(super) const ROWS: u8 = 64;
pub(super) const ROW_HEIGHT: f32 = 64.0;
pub(super) const MIN_SPAN: Span = Span::new(2, 2);
pub(super) const MAX_HEIGHT: u8 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile {
    pub widget: Widget,
    pub at: Cell,
    pub span: Span,
}

impl Tile {
    const fn new(widget: Widget, col: u8, row: u8, w: u8, h: u8) -> Self {
        Self {
            widget,
            at: Cell::new(col, row),
            span: Span::new(w, h),
        }
    }

    fn fit(mut self) -> Self {
        self.span.w = self.span.w.clamp(MIN_SPAN.w, COLUMNS);
        self.span.h = self.span.h.clamp(MIN_SPAN.h, MAX_HEIGHT);
        self.at = self.at.fit(self.span, COLUMNS, ROWS);
        self
    }

    pub(super) fn position(self) -> grid::Placement {
        (self.at, self.span)
    }
}

pub(crate) const DEFAULT: [Tile; 4] = [
    Tile::new(Widget::Clock, 0, 0, 2, 2),
    Tile::new(Widget::QuickLinks, 2, 0, 4, 2),
    Tile::new(Widget::Recent, 0, 2, 4, 3),
    Tile::new(Widget::Writing, 4, 2, 2, 3),
];

impl Widget {
    fn default_span(self) -> Span {
        DEFAULT
            .iter()
            .find(|tile| tile.widget == self)
            .map_or(Span::new(3, 3), |tile| tile.span)
    }
}

pub(super) fn can_place(tiles: &[Tile], candidate: Tile) -> bool {
    candidate == candidate.fit()
        && tiles.iter().all(|tile| {
            tile.widget == candidate.widget
                || !grid::overlaps(tile.position(), candidate.position())
        })
}

fn find_cell(tiles: &[Tile], span: Span) -> Option<Cell> {
    let occupied: Vec<_> = tiles.iter().map(|tile| tile.position()).collect();
    grid::free_cell(&occupied, span, COLUMNS, ROWS)
}

pub(super) fn add(tiles: &mut Vec<Tile>, widget: Widget) {
    if tiles.iter().any(|tile| tile.widget == widget) {
        return;
    }
    let span = widget.default_span();
    if let Some(at) = find_cell(tiles, span) {
        tiles.push(Tile { widget, at, span });
    }
}

pub(crate) fn from_order(widgets: Vec<Widget>) -> Vec<Tile> {
    if widgets == Widget::DEFAULT {
        return DEFAULT.to_vec();
    }
    let mut tiles = Vec::new();
    let mut col = 0;
    let mut row = 0;
    let mut height = 0;
    for widget in widgets {
        if tiles.iter().any(|tile: &Tile| tile.widget == widget) {
            continue;
        }
        let span = widget.default_span();
        if col + span.w > COLUMNS {
            col = 0;
            row += height;
            height = 0;
        }
        tiles.push(Tile {
            widget,
            at: Cell::new(col, row),
            span,
        });
        col += span.w;
        height = height.max(span.h);
    }
    tiles
}

pub(crate) fn normalize(tiles: &mut Vec<Tile>) {
    let mut valid = Vec::with_capacity(Widget::ALL.len());
    for tile in tiles.drain(..) {
        if valid.iter().any(|other: &Tile| other.widget == tile.widget) {
            continue;
        }
        let mut tile = tile.fit();
        if !can_place(&valid, tile) {
            let Some(at) = find_cell(&valid, tile.span) else {
                continue;
            };
            tile.at = at;
        }
        valid.push(tile);
    }
    *tiles = valid;
}

pub(super) fn narrow(tiles: &mut [Tile]) {
    let mut row = 0;
    for tile in tiles {
        tile.at = Cell::new(0, row);
        tile.span.w = COLUMNS;
        row += tile.span.h;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_repaired_layouts_reserve_each_cell_once() {
        let mut tiles = DEFAULT.to_vec();
        tiles.push(Tile::new(Widget::WorldClocks, 0, 0, 2, 3));
        tiles.push(DEFAULT[0]);
        normalize(&mut tiles);
        assert_eq!(&tiles[..4], &DEFAULT);
        assert_eq!(tiles.len(), 5);
        assert!(tiles.iter().all(|tile| can_place(&tiles, *tile)));
        assert!(!can_place(&tiles, Tile::new(Widget::Clock, 0, 0, 3, 2)));
        assert!(!can_place(&tiles, Tile::new(Widget::Clock, 5, 0, 2, 2)));
    }
}
