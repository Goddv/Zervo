//! Persistent placements for the New Tab dashboard.

use serde::{Deserialize, Serialize};

use crate::grid::{self, Cell, Span};
use crate::l10n::text as tr;

pub const COLUMNS: u8 = 12;
pub const ROWS: u8 = 48;
const MAX_SAVED_HEIGHT: u8 = 5;

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
    let mut saved: Vec<_> = Kind::ALL
        .iter()
        .filter_map(|kind| saved.iter().find(|widget| widget.kind == *kind).copied())
        .collect();
    saved.sort_by_key(|widget| (widget.cell.row, widget.cell.col));
    let mut placed = Vec::with_capacity(saved.len());
    let mut taken = Vec::with_capacity(saved.len());
    for mut widget in saved {
        widget.span.w = widget.span.w.clamp(1, columns);
        widget.span.h = widget.span.h.clamp(1, MAX_SAVED_HEIGHT);
        widget.cell = widget.cell.fit(widget.span, columns, ROWS);
        if taken
            .iter()
            .any(|other| grid::overlaps(*other, widget.placement()))
        {
            widget.cell = grid::free_cell(&taken, widget.span, columns, ROWS)
                .expect("five widgets of at most five rows fit in the dashboard");
        }
        taken.push(widget.placement());
        placed.push(widget);
    }
    placed
}

pub fn add(widgets: &mut Vec<Widget>, kind: Kind, columns: u8) -> bool {
    if widgets.iter().any(|widget| widget.kind == kind) {
        return false;
    }
    let width = if columns >= 4 { 4 } else { 2 };
    let mut widget = Widget::new(kind, 0, 0, width);
    let taken: Vec<_> = widgets.iter().map(|widget| widget.placement()).collect();
    let Some(cell) = grid::free_cell(&taken, widget.span, columns, ROWS) else {
        return false;
    };
    widget.cell = cell;
    widgets.push(widget);
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
    SIZES
        .into_iter()
        .map(|(span, _)| span)
        .filter(|span| span.w <= columns && span.h <= rows)
        .min_by(|a, b| {
            let distance = |span: &Span| {
                (egui::vec2(f32::from(span.w), f32::from(span.h)) - desired).length_sq()
            };
            distance(a).total_cmp(&distance(b))
        })
}

/// Move a widget into free space, or swap it with a single neighbour if both
/// will fit. Resizing always keeps the original top-left corner.
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
    let mut collisions = widgets.iter().enumerate().filter(|(other, widget)| {
        *other != index && grid::overlaps(widget.placement(), proposed.placement())
    });
    let mut next = widgets.to_vec();
    if let Some((other, displaced)) = collisions.next() {
        if proposed.span != original.span || collisions.next().is_some() {
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
    fn moving_a_widget_can_swap_but_resizing_cannot_cover_a_neighbour() {
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
        assert!(reposition(&saved, 0, resized, COLUMNS, ROWS).is_none());
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
        assert_eq!(snap_size(egui::vec2(4.0, 4.0), 2, 4), Some(Span::new(2, 2)));
    }
}
