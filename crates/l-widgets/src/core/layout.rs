//! Pure grid layout engine. No UI, no I/O — fully testable.
//!
//! Coordinates are integer cells; (0,0) is top-left. Widgets occupy
//! `w x h` cells starting at `pos`.

use l_core::models::{GridPos, GridSize, PlacedWidget, Surface};

/// A resolved cell rect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRect {
    /// Left column.
    pub x: u32,
    /// Top row.
    pub y: u32,
    /// Width in cells.
    pub w: u32,
    /// Height in cells.
    pub h: u32,
}

impl CellRect {
    /// Whether two rects overlap.
    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        self.x < other.x + other.w
            && self.x + self.w > other.x
            && self.y < other.y + other.h
            && self.y + self.h > other.y
    }

    /// Clamp into `cols` columns.
    #[must_use]
    pub fn clamped(self, cols: u32) -> Self {
        let w = self.w.min(cols.max(1));
        Self {
            x: self.x.min(cols.saturating_sub(w)),
            w,
            ..self
        }
    }
}

/// Convert a placed widget to a cell rect clamped to `cols`.
#[must_use]
pub fn rect_of(w: &PlacedWidget, cols: u32) -> CellRect {
    CellRect {
        x: w.pos.x,
        y: w.pos.y,
        w: w.size.w,
        h: w.size.h,
    }
    .clamped(cols)
}

/// Whether `candidate` overlaps any widget in `others` (excluding `exclude`).
#[must_use]
pub fn collides(
    candidate: CellRect,
    others: &[PlacedWidget],
    cols: u32,
    exclude: Option<l_core::models::WidgetInstanceId>,
) -> bool {
    others
        .iter()
        .any(|w| Some(w.id) != exclude && rect_of(w, cols).overlaps(candidate))
}

/// Resolve positions for a set of widgets on one surface, eliminating
/// overlaps deterministically by pushing colliding widgets down.
///
/// Input order is preserved in output order. The function never fails; it
/// always produces an in-bounds, overlap-free arrangement (given enough rows).
#[must_use]
pub fn resolve(mut widgets: Vec<PlacedWidget>, cols: u32, surface: Surface) -> Vec<PlacedWidget> {
    let cols = cols.max(1);
    // Sort by current position for determinism, stable.
    widgets.sort_by_key(|w| (w.pos.y, w.pos.x));
    let mut placed: Vec<PlacedWidget> = Vec::with_capacity(widgets.len());

    for mut w in widgets {
        if w.surface != surface {
            placed.push(w);
            continue;
        }
        let mut rect = rect_of(&w, cols);
        // Push down until no collision (bounded to avoid infinite loops).
        for _ in 0..10_000 {
            let hit = placed
                .iter()
                .any(|p| p.surface == surface && rect_of(p, cols).overlaps(rect));
            if !hit {
                break;
            }
            rect.y += 1;
        }
        w.pos.x = rect.x;
        w.pos.y = rect.y;
        placed.push(w);
    }
    placed
}

/// Attempt to move `id` to `pos` on `surface`. Returns the new position if the
/// move is legal (in bounds, no collision); `None` if rejected.
#[must_use]
pub fn try_move(
    widgets: &[PlacedWidget],
    id: l_core::models::WidgetInstanceId,
    pos: GridPos,
    size: GridSize,
    cols: u32,
    surface: Surface,
) -> Option<GridPos> {
    let candidate = CellRect {
        x: pos.x,
        y: pos.y,
        w: size.w,
        h: size.h,
    }
    .clamped(cols);
    let same_surface: Vec<PlacedWidget> = widgets
        .iter()
        .filter(|w| w.surface == surface)
        .cloned()
        .collect();
    if collides(candidate, &same_surface, cols, Some(id)) {
        return None;
    }
    Some(GridPos {
        x: candidate.x,
        y: candidate.y,
    })
}

/// Attempt to resize `id` to `size` keeping top-left fixed.
#[must_use]
pub fn try_resize(
    widgets: &[PlacedWidget],
    id: l_core::models::WidgetInstanceId,
    size: GridSize,
    min: GridSize,
    max: Option<GridSize>,
    cols: u32,
    surface: Surface,
) -> Option<GridSize> {
    let w = widgets.iter().find(|w| w.id == id)?;
    let mut new_w = size.w.max(min.w);
    let mut new_h = size.h.max(min.h);
    if let Some(m) = max {
        new_w = new_w.min(m.w);
        new_h = new_h.min(m.h);
    }
    new_w = new_w.min(cols.max(1));
    let candidate = CellRect {
        x: w.pos.x,
        y: w.pos.y,
        w: new_w,
        h: new_h,
    }
    .clamped(cols);
    let same_surface: Vec<PlacedWidget> = widgets
        .iter()
        .filter(|w| w.surface == surface)
        .cloned()
        .collect();
    if collides(candidate, &same_surface, cols, Some(id)) {
        return None;
    }
    Some(GridSize::new(candidate.w, candidate.h))
}

/// Find the first free position for a widget of `size` (scans row-major).
#[must_use]
pub fn first_free(
    widgets: &[PlacedWidget],
    size: GridSize,
    cols: u32,
    surface: Surface,
) -> GridPos {
    let cols = cols.max(1);
    // Only widgets on the same surface participate in collision.
    let surface_widgets: Vec<PlacedWidget> = widgets
        .iter()
        .filter(|w| w.surface == surface)
        .cloned()
        .collect();
    for y in 0..10_000u32 {
        for x in 0..cols {
            let candidate = CellRect {
                x,
                y,
                w: size.w.min(cols),
                h: size.h,
            };
            if !collides(candidate, &surface_widgets, cols, None) {
                return GridPos { x, y };
            }
        }
    }
    GridPos { x: 0, y: 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use l_core::models::WidgetInstanceId;

    fn widget(x: u32, y: u32, w: u32, h: u32) -> PlacedWidget {
        PlacedWidget {
            id: WidgetInstanceId::new_v4(),
            widget_type: "t".into(),
            name: String::new(),
            enabled: true,
            pos: GridPos { x, y },
            size: GridSize::new(w, h),
            config: serde_json::json!({}),
            surface: Surface::Main,
        }
    }

    #[test]
    fn resolve_removes_overlap() {
        let a = widget(0, 0, 4, 3);
        let b = widget(0, 0, 4, 3); // exact overlap
        let out = resolve(vec![a.clone(), b], 12, Surface::Main);
        let ra = rect_of(&out[0], 12);
        let rb = rect_of(&out[1], 12);
        assert!(!ra.overlaps(rb));
    }

    #[test]
    fn try_move_rejects_collision() {
        let a = widget(0, 0, 4, 3);
        let b = widget(4, 0, 4, 3);
        let ws = vec![a.clone(), b.clone()];
        assert!(try_move(&ws, b.id, GridPos { x: 0, y: 0 }, b.size, 12, Surface::Main).is_none());
        assert!(try_move(&ws, b.id, GridPos { x: 8, y: 0 }, b.size, 12, Surface::Main).is_some());
    }

    #[test]
    fn clamped_keeps_in_bounds() {
        let r = CellRect {
            x: 10,
            y: 0,
            w: 6,
            h: 2,
        }
        .clamped(12);
        assert_eq!(r.x, 6);
        assert_eq!(r.w, 6);
    }

    #[test]
    fn first_free_finds_spot() {
        let a = widget(0, 0, 12, 2); // full-width row
        let pos = first_free(&[a], GridSize::new(4, 2), 12, Surface::Main);
        assert_eq!(pos.y, 2);
    }
}
