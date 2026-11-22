//! Virtual-list windowing: the visible rows of a uniform-height list in `O(1)`.
//!
//! The controller is pure arithmetic: no tree, no allocation, no scan. The
//! caller builds only the rows this range names, so a 10 000 000-row list costs
//! the same as a 100-row one. Overscan rows are built above and below the
//! viewport so a fast scroll does not flash empty space.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)] // reason: row, column and pixel values are bounded UI quantities
use std::ops::Range;

/// A uniform-row virtual list window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VirtualList {
    item_count: usize,
    item_height: f32,
    overscan: usize,
}

impl VirtualList {
    /// A list of `item_count` rows, `item_height` logical pixels tall.
    ///
    /// A non-finite or non-positive height is clamped to one pixel so the
    /// arithmetic cannot divide by zero.
    #[must_use]
    pub fn new(item_count: usize, item_height: f32) -> Self {
        Self {
            item_count,
            item_height: if item_height.is_finite() && item_height > 0.0 {
                item_height
            } else {
                1.0
            },
            overscan: 2,
        }
    }

    /// The same list with `overscan` extra rows above and below.
    #[must_use]
    pub fn with_overscan(mut self, overscan: usize) -> Self {
        self.overscan = overscan;
        self
    }

    /// Replaces the row count (a filter changed).
    pub fn set_item_count(&mut self, item_count: usize) {
        self.item_count = item_count;
    }

    /// The row count.
    #[must_use]
    pub fn item_count(&self) -> usize {
        self.item_count
    }

    /// The row height.
    #[must_use]
    pub fn item_height(&self) -> f32 {
        self.item_height
    }

    /// The overscan row count.
    #[must_use]
    pub fn overscan(&self) -> usize {
        self.overscan
    }

    /// The height of the whole list.
    #[must_use]
    pub fn total_height(&self) -> f32 {
        self.item_count as f32 * self.item_height
    }

    /// The rows to build for this viewport, `O(1)` and allocation-free.
    ///
    /// Fills `out` with one range (the overscanned visible window) and returns
    /// the number of rows; an empty list or viewport clears `out` and returns
    /// zero. `out` is reused, so a steady frame does not allocate.
    pub fn visible_range(
        &self,
        offset: f32,
        viewport_height: f32,
        out: &mut Vec<Range<usize>>,
    ) -> usize {
        out.clear();
        let range = self.window(offset, viewport_height);
        if range.is_empty() {
            return 0;
        }
        let count = range.len();
        out.push(range);
        count
    }

    /// The visible window without a buffer.
    #[must_use]
    pub fn window(&self, offset: f32, viewport_height: f32) -> Range<usize> {
        if self.item_count == 0 || !viewport_height.is_finite() || viewport_height <= 0.0 {
            return 0..0;
        }
        let offset = if offset.is_finite() {
            offset.max(0.0)
        } else {
            0.0
        };
        let first = (offset / self.item_height).floor().max(0.0) as usize;
        let first = first.saturating_sub(self.overscan);
        let rows = (viewport_height / self.item_height).ceil().max(0.0) as usize;
        let end = (offset / self.item_height).floor().max(0.0) as usize + rows + 1 + self.overscan;
        let end = end.min(self.item_count).max(first.min(self.item_count));
        first.min(self.item_count)..end
    }

    /// The row under `y` (relative to the viewport) at this offset.
    #[must_use]
    pub fn index_at(&self, y: f32, offset: f32) -> Option<usize> {
        if !y.is_finite() || y < 0.0 {
            return None;
        }
        let absolute = y + offset.max(0.0);
        let index = (absolute / self.item_height).floor() as usize;
        (index < self.item_count).then_some(index)
    }

    /// The offset that reveals `index`, keeping the current offset when the row
    /// is already visible.
    #[must_use]
    pub fn scroll_to(&self, index: usize, current_offset: f32, viewport_height: f32) -> f32 {
        if index >= self.item_count {
            return current_offset;
        }
        let y0 = index as f32 * self.item_height;
        let y1 = y0 + self.item_height;
        if y0 < current_offset {
            return y0;
        }
        if y1 > current_offset + viewport_height {
            return (y1 - viewport_height).max(0.0);
        }
        current_offset
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // reason: exact token arithmetic in tests
    use std::ops::Range;

    use super::VirtualList;

    #[test]
    fn an_empty_list_has_no_window() {
        let list = VirtualList::new(0, 24.0);
        let mut out = Vec::new();
        assert_eq!(list.visible_range(0.0, 500.0, &mut out), 0);
        assert!(out.is_empty());
        assert_eq!(list.window(0.0, 500.0), 0..0);
        assert_eq!(list.index_at(0.0, 0.0), None);
    }

    #[test]
    fn a_viewport_taller_than_the_content_is_clamped() {
        let list = VirtualList::new(3, 24.0).with_overscan(10);
        assert_eq!(list.window(0.0, 500.0), 0..3);
        assert_eq!(list.window(0.0, 48.0), 0..3);
    }

    #[test]
    fn the_window_has_overscan_and_one_extra_row() {
        let list = VirtualList::new(1_000, 24.0).with_overscan(2);
        assert_eq!(list.window(0.0, 240.0), 0..13);
        assert_eq!(list.window(240.0, 240.0), 8..23);
    }

    #[test]
    fn an_offset_past_the_end_yields_an_empty_clamped_window() {
        let list = VirtualList::new(100, 24.0).with_overscan(2);
        let window = list.window(10_000.0, 240.0);
        assert_eq!(window, 100..100);
        assert!(window.is_empty());
    }

    #[test]
    fn index_at_respects_the_offset_and_the_end() {
        let list = VirtualList::new(10, 20.0);
        assert_eq!(list.index_at(0.0, 0.0), Some(0));
        assert_eq!(list.index_at(19.9, 0.0), Some(0));
        assert_eq!(list.index_at(0.0, 20.0), Some(1));
        assert_eq!(list.index_at(0.0, 200.0), None);
        assert_eq!(list.index_at(-1.0, 0.0), None);
    }

    #[test]
    fn scroll_to_reveals_only_when_needed() {
        let list = VirtualList::new(100, 20.0);
        assert_eq!(list.scroll_to(50, 0.0, 100.0), 920.0);
        assert_eq!(list.scroll_to(5, 900.0, 100.0), 100.0);
        assert_eq!(list.scroll_to(50, 900.0, 100.0), 920.0);
        assert_eq!(list.scroll_to(1_000, 900.0, 100.0), 900.0);
    }

    #[test]
    fn a_huge_list_stays_in_bounds() {
        let list = VirtualList::new(10_000_000, 24.0);
        let window: Range<usize> = list.window(239_999_976.0, 900.0);
        assert_eq!(window.start, 9_999_997);
        assert_eq!(window.end, 10_000_000);
        assert_eq!(list.total_height(), 240_000_000.0);
    }

    #[test]
    fn a_bad_height_falls_back_to_one_pixel() {
        let list = VirtualList::new(10, f32::NAN);
        assert_eq!(list.item_height(), 1.0);
        assert_eq!(VirtualList::new(10, 0.0).item_height(), 1.0);
    }

    #[test]
    fn the_output_buffer_is_reused() {
        let list = VirtualList::new(1_000, 24.0);
        let mut out = Vec::with_capacity(4);
        let capacity = out.capacity();
        list.visible_range(0.0, 240.0, &mut out);
        list.visible_range(480.0, 240.0, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out.capacity(), capacity);
    }
}
