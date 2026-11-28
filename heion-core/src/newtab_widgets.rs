//! Persistent placements for the New Tab dashboard.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::grid::{self, Cell, Span};
use crate::l10n::text as tr;

pub const COLUMNS: u8 = 12;
pub const ROWS: u8 = 48;
pub const MAX_HEIGHT: u8 = 5;

/// The standard resize stops, in columns × rows.
pub const SIZES: [(Span, &str); 5] = [
    (Span::new(1, 1), "newtab-size-small"),
    (Span::new(2, 1), "newtab-size-pill"),
    (Span::new(2, 2), "newtab-size-medium"),
    (Span::new(4, 2), "newtab-size-wide"),
    (Span::new(4, 4), "newtab-size-large"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum Kind {
    Clock,
    Recent,
    Pinned,
    Checklist,
    Workspace,
    Custom(u64),
}

impl Kind {
    pub const ALL: [Self; 5] = [
        Self::Clock,
        Self::Recent,
        Self::Pinned,
        Self::Checklist,
        Self::Workspace,
    ];

    pub fn label(self) -> &'static str {
        tr(match self {
            Self::Clock => "newtab-clock",
            Self::Recent => "newtab-recent",
            Self::Pinned => "sidebar-pinned-tabs",
            Self::Checklist => "newtab-checklist",
            Self::Workspace => "newtab-workspace-pulse",
            Self::Custom(_) => "newtab-created-widget",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Widget {
    pub kind: Kind,
    pub cell: Cell,
    pub span: Span,
}

impl Widget {
    fn new(kind: Kind, col: u8, row: u8, width: u8) -> Self {
        Self {
            kind,
            cell: Cell::new(col, row),
            span: Span::new(width, 2),
        }
    }

    fn placement(self) -> grid::Placement {
        (self.cell, self.span)
    }
}

pub fn defaults() -> Vec<Widget> {
    vec![
        Widget::new(Kind::Clock, 0, 0, 4),
        Widget::new(Kind::Recent, 4, 0, 4),
        Widget::new(Kind::Pinned, 8, 0, 4),
        Widget::new(Kind::Checklist, 0, 2, 6),
        Widget::new(Kind::Workspace, 6, 2, 6),
    ]
}

/// Repair saved overlaps and fit the current viewport without writing back.
pub fn arrange(saved: &[Widget], columns: u8) -> Vec<Widget> {
    let mut seen = HashSet::new();
    let mut saved: Vec<_> = saved
        .iter()
        .copied()
        .filter(|widget| seen.insert(widget.kind))
        .collect();
    saved.sort_by_key(|widget| (widget.cell.row, widget.cell.col));
    let mut placed = Vec::with_capacity(saved.len());
    let mut taken = Vec::with_capacity(saved.len());
    for mut widget in saved {
        widget.span.w = widget.span.w.clamp(1, columns);
        widget.span.h = widget.span.h.clamp(1, MAX_HEIGHT);
        widget.cell = widget.cell.fit(widget.span, columns, ROWS);
        if taken
            .iter()
            .any(|other| grid::overlaps(*other, widget.placement()))
        {
            let Some(cell) = grid::free_cell(&taken, widget.span, columns, ROWS) else {
                continue;
            };
            widget.cell = cell;
        }
        taken.push(widget.placement());
        placed.push(widget);
    }
    placed
}

pub fn add(widgets: &mut Vec<Widget>, kind: Kind, columns: u8) -> bool {
    let width = if columns >= 4 { 4 } else { 2 };
    add_with_size(widgets, kind, Span::new(width, 2), columns)
}

pub fn add_with_size(widgets: &mut Vec<Widget>, kind: Kind, span: Span, columns: u8) -> bool {
    if widgets.iter().any(|widget| widget.kind == kind) {
        return false;
    }
    let span = span.fit(columns, ROWS);
    let taken: Vec<_> = widgets.iter().map(|widget| widget.placement()).collect();
    let Some(cell) = grid::free_cell(&taken, span, columns, ROWS) else {
        return false;
    };
    widgets.push(Widget { kind, cell, span });
    true
}

/// Convert the original six-column layout to the square-cell grid. The two
/// generations have separate settings keys so the conversion runs once.
pub fn migrate_wide_cells(mut widgets: Vec<Widget>) -> Vec<Widget> {
    for widget in &mut widgets {
        widget.cell.col = widget.cell.col.saturating_mul(2);
        widget.span.w = widget.span.w.saturating_mul(2);
    }
    arrange(&widgets, COLUMNS)
}

pub fn snap_size(desired: egui::Vec2, columns: u8, rows: u8) -> Option<Span> {
    if columns == 0 || rows == 0 || !desired.x.is_finite() || !desired.y.is_finite() {
        return None;
    }
    let height = desired
        .y
        .round()
        .clamp(1.0, f32::from(rows.min(MAX_HEIGHT))) as u8;
    let min_width = if height > 1 { columns.min(2) } else { 1 };
    let width = desired
        .x
        .round()
        .clamp(f32::from(min_width), f32::from(columns)) as u8;
    Some(Span::new(width, height))
}

/// Move a widget into free space, or swap it with a single neighbour if both
/// will fit. A resize keeps its corner and reflows the cards it displaces.
pub fn reposition(
    widgets: &[Widget],
    index: usize,
    proposed: Widget,
    columns: u8,
    rows: u8,
) -> Option<Vec<Widget>> {
    let original = widgets[index];
    if proposed == original
        || proposed.span.fit(columns, rows) != proposed.span
        || proposed.cell.fit(proposed.span, columns, rows) != proposed.cell
    {
        return None;
    }
    if proposed.span != original.span {
        return resize(widgets, index, proposed, columns, rows);
    }
    let mut collisions = widgets.iter().enumerate().filter(|(other, widget)| {
        *other != index && grid::overlaps(widget.placement(), proposed.placement())
    });
    let mut next = widgets.to_vec();
    if let Some((other, displaced)) = collisions.next() {
        if collisions.next().is_some() {
            return None;
        }
        let swapped = Widget {
            cell: original.cell,
            ..*displaced
        };
        if swapped.cell.fit(swapped.span, columns, rows) != swapped.cell
            || grid::overlaps(swapped.placement(), proposed.placement())
            || widgets.iter().enumerate().any(|(i, widget)| {
                i != index && i != other && grid::overlaps(widget.placement(), swapped.placement())
            })
        {
            return None;
        }
        next[other] = swapped;
    }
    next[index] = proposed;
    Some(next)
}

fn resize(
    widgets: &[Widget],
    index: usize,
    proposed: Widget,
    columns: u8,
    rows: u8,
) -> Option<Vec<Widget>> {
    let mut next = widgets.to_vec();
    next[index] = proposed;
    let conflicts = |other: &Widget| grid::overlaps(proposed.placement(), other.placement());
    let mut taken: Vec<_> = widgets
        .iter()
        .enumerate()
        .filter(|(position, other)| *position != index && !conflicts(other))
        .map(|(_, other)| other.placement())
        .collect();
    taken.push(proposed.placement());
    for (position, other) in widgets.iter().enumerate() {
        if position == index || !conflicts(other) {
            continue;
        }
        let bottom = proposed.cell.row + proposed.span.h;
        let floor = (Cell::new(0, 0), Span::new(columns, bottom.max(1)));
        taken.push(floor);
        let below = grid::free_cell(&taken, other.span, columns, rows);
        taken.pop();
        next[position].cell =
            below.or_else(|| grid::free_cell(&taken, other.span, columns, rows))?;
        taken.push(next[position].placement());
    }
    Some(next)
}

/// Apply a viewport edit to saved positions. A narrow window is a projection
/// of the board, so an edit must preserve the canonical spans and positions of
/// cards that the user did not touch.
pub fn apply_view_edit(saved: &[Widget], before: &[Widget], after: &[Widget]) -> Vec<Widget> {
    let mut result = saved.to_vec();
    result.retain(|tile| {
        after.iter().any(|visible| visible.kind == tile.kind)
            || !before.iter().any(|visible| visible.kind == tile.kind)
    });

    let mut changed = Vec::new();
    for tile in after {
        let previous = before.iter().find(|visible| visible.kind == tile.kind);
        if previous == Some(tile) {
            continue;
        }
        let canonical = result
            .iter()
            .position(|stored| stored.kind == tile.kind)
            .map(|position| result.remove(position));
        let span = match (canonical, previous) {
            (Some(saved), Some(previous)) if previous.span == tile.span => saved.span,
            _ => tile.span,
        };
        changed.push(Widget {
            span,
            cell: tile.cell.fit(span, COLUMNS, ROWS),
            ..*tile
        });
    }
    changed.reverse();
    // Give the user's new positions priority. Keep displacements after the
    // affected row when possible, and retain any tile that cannot fit.
    for target in changed {
        result.insert(0, target);
        if let Some(next) = resize(&result, 0, target, COLUMNS, ROWS) {
            result = next;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_grid_reflows_without_overlaps_or_changes_to_saved_layout() {
        let saved = defaults();
        let narrow = arrange(&saved, 2);
        assert_eq!(narrow.len(), saved.len());
        assert!(narrow.iter().all(|widget| widget.cell.col == 0));
        for (index, widget) in narrow.iter().enumerate() {
            assert!(
                narrow[index + 1..]
                    .iter()
                    .all(|other| { !grid::overlaps(widget.placement(), other.placement()) })
            );
        }
        assert_eq!(arrange(&saved, COLUMNS), saved);
    }

    #[test]
    fn moving_swaps_and_expanding_reflows_the_neighbour() {
        let saved = defaults();
        let moved = Widget {
            cell: saved[1].cell,
            ..saved[0]
        };
        let result = reposition(&saved, 0, moved, COLUMNS, ROWS).unwrap();
        assert_eq!(result[0].cell, saved[1].cell);
        assert_eq!(result[1].cell, saved[0].cell);
        let resized = Widget {
            span: Span::new(5, 2),
            ..saved[0]
        };
        let result = reposition(&saved, 0, resized, COLUMNS, ROWS).unwrap();
        assert_eq!(result[0], resized);
        assert!(result[1].cell.row >= resized.cell.row + resized.span.h);
        assert_eq!(result[2], saved[2]);
    }

    #[test]
    fn removing_a_tile_in_a_narrow_window_keeps_other_cards_in_their_saved_positions() {
        let saved = defaults();
        let before = arrange(&saved, 2);
        let mut after = before.clone();
        after.retain(|tile| tile.kind != Kind::Recent);
        let actual = apply_view_edit(&saved, &before, &after);
        let expected: Vec<_> = saved
            .into_iter()
            .filter(|tile| tile.kind != Kind::Recent)
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn old_layout_and_compact_sizes_keep_their_footprints() {
        let old = [
            Widget::new(Kind::Clock, 0, 0, 2),
            Widget::new(Kind::Recent, 2, 0, 2),
        ];
        let mut migrated = migrate_wide_cells(old.to_vec());
        assert_eq!(migrated, defaults()[..2]);
        let pill = snap_size(egui::vec2(2.1, 1.1), 4, 4).unwrap();
        assert_eq!(pill, Span::new(2, 1));
        let compact = Widget {
            span: pill,
            ..migrated[0]
        };
        migrated = reposition(&migrated, 0, compact, COLUMNS, ROWS).unwrap();
        assert_eq!(arrange(&migrated, COLUMNS)[0], compact);
        assert_eq!(snap_size(egui::vec2(4.0, 4.0), 2, 4), Some(Span::new(2, 4)));
        assert_eq!(snap_size(egui::vec2(5.1, 2.8), 8, 5), Some(Span::new(5, 3)));
    }
}
