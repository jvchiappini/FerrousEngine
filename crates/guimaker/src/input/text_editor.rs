//! The text-editing state machine: pure, deterministic and headless.
//!
//! `TextEditor` holds the text, a caret, a selection anchor, the undo history
//! and the IME composition. Every entry point takes the injected instant
//! (`now_ms`), never reads a clock, and returns an [`EditorOutcome`] telling the
//! host whether the text changed, the field asked to submit, or it reverted.
//!
//! Movement and deletion are grapheme-correct (see [`super::grapheme`]): one
//! backspace removes a whole emoji or a base letter with its combining accent.
//! Caret geometry comes from a cached line layout built from a [`TextMetrics`]
//! implementation: the cache is invalidated only when the text changes, so a
//! keystroke re-measures nothing.
//!
//! ```
//! use guimaker::input::text_editor::{EditorKey, Modifiers, NullClipboard, TextEditor};
//! let mut editor = TextEditor::new("hello");
//! editor.handle_key(EditorKey::Home, Modifiers::none(), 0.0, &mut NullClipboard);
//! assert_eq!(editor.caret(), 0);
//! ```

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)] // reason: row, column and pixel values are bounded UI quantities

use unicode_segmentation::UnicodeSegmentation;

use super::grapheme;

#[allow(clippy::struct_excessive_bools)] // reason: the four platform modifiers are independent
/// A modifier set for the editor (independent of the engine's input types).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// Shift is held.
    pub shift: bool,
    /// Control is held.
    pub control: bool,
    /// Alt is held.
    pub alt: bool,
    /// The platform key is held.
    pub super_key: bool,
}

impl Modifiers {
    /// No modifier.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            shift: false,
            control: false,
            alt: false,
            super_key: false,
        }
    }

    /// A modifier set with Control held.
    #[must_use]
    pub const fn control() -> Self {
        Self {
            shift: false,
            control: true,
            alt: false,
            super_key: false,
        }
    }

    /// A modifier set with Control and Shift held.
    #[must_use]
    pub const fn control_shift() -> Self {
        Self {
            shift: true,
            control: true,
            alt: false,
            super_key: false,
        }
    }

    /// A modifier set with Shift held.
    #[must_use]
    pub const fn shift() -> Self {
        Self {
            shift: true,
            control: false,
            alt: false,
            super_key: false,
        }
    }
}

/// A key the editor understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorKey {
    /// Insert this character.
    Char(char),
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Home (line start).
    Home,
    /// End (line end).
    End,
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// Enter or Return.
    Enter,
    /// Escape.
    Escape,
    /// Tab; the editor ignores it so focus can move.
    Tab,
}

/// What a key did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorOutcome {
    /// The key is not an edit (Tab, an unknown combination).
    Ignored,
    /// The text or the selection changed.
    Changed,
    /// A single-line field submitted (`Enter`).
    Submit,
    /// The editor reverted to the text it was opened with (`Escape`).
    Revert,
}

/// A validation hook result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Validation {
    /// The text is longer than the field's maximum.
    TooLong,
    /// A numeric field contains a non-numeric character.
    NotNumeric,
}

/// The host clipboard.
pub trait Clipboard {
    /// Reads the current clipboard text.
    fn read(&mut self) -> Option<String>;
    /// Writes text to the clipboard.
    fn write(&mut self, text: &str);
}

/// A clipboard that does nothing (tests, headless hosts).
#[derive(Clone, Copy, Debug, Default)]
pub struct NullClipboard;

impl Clipboard for NullClipboard {
    fn read(&mut self) -> Option<String> {
        None
    }

    fn write(&mut self, _text: &str) {}
}

/// Horizontal metrics for the text.
///
/// `advance` must be additive (`advance(a + b) == advance(a) + advance(b)`) so
/// caret arithmetic is `O(graphemes)` without shaping the whole line again.
pub trait TextMetrics {
    /// The advance width of a string, in logical pixels.
    fn advance(&self, text: &str) -> f32;
    /// The line height, in logical pixels.
    fn line_height(&self) -> f32;
}

/// A fixed-width metric, exact for monospace and the test reference.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Monospace {
    /// One character's advance.
    pub char_width: f32,
    /// The line height.
    pub height: f32,
}

impl Monospace {
    /// A monospace metric with a character width (line height 1.4x).
    #[must_use]
    pub fn new(char_width: f32) -> Self {
        Self {
            char_width,
            height: char_width * 1.4,
        }
    }
}

impl TextMetrics for Monospace {
    fn advance(&self, text: &str) -> f32 {
        grapheme::count(text) as f32 * self.char_width
    }

    fn line_height(&self) -> f32 {
        self.height
    }
}

/// One cached line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineInfo {
    /// Byte offset of the line start.
    pub start: usize,
    /// Byte offset one past the last character (the newline excluded).
    pub end: usize,
    /// The line width.
    pub width: f32,
}

#[derive(Clone, Debug)]
struct Edit {
    start: usize,
    removed: String,
    inserted: String,
    at_ms: f64,
    caret_before: usize,
    coalesce: bool,
    closed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Composition {
    start: usize,
    text: String,
}

/// The text-editing state.
#[derive(Clone, Debug)]
pub struct TextEditor {
    text: String,
    opened_with: String,
    caret: usize,
    anchor: Option<usize>,
    preferred_column: Option<f32>,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    single_line: bool,
    max_len: Option<usize>,
    numeric: bool,
    composition: Option<Composition>,
    blink_origin: f64,
    metrics_width: f32,
    lines: Vec<LineInfo>,
    line_cache_text_len: usize,
    line_cache_text: String,
    scroll_x: f32,
    dirty: bool,
}

impl TextEditor {
    /// Builds an editor over `text` with the caret at the end.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let caret = text.len();
        Self {
            opened_with: text.clone(),
            text,
            caret,
            anchor: None,
            preferred_column: None,
            undo: Vec::new(),
            redo: Vec::new(),
            single_line: false,
            max_len: None,
            numeric: false,
            composition: None,
            blink_origin: 0.0,
            metrics_width: 0.0,
            lines: Vec::new(),
            line_cache_text_len: usize::MAX,
            line_cache_text: String::new(),
            scroll_x: 0.0,
            dirty: false,
        }
    }

    /// Whether the field is single-line.
    #[must_use]
    pub fn is_single_line(&self) -> bool {
        self.single_line
    }

    /// Makes the field single-line (no newline is inserted, paste flattens).
    #[must_use]
    pub fn with_single_line(mut self, single_line: bool) -> Self {
        self.single_line = single_line;
        self
    }

    /// Limits the text to `max_len` grapheme clusters.
    #[must_use]
    pub fn with_max_len(mut self, max_len: usize) -> Self {
        self.max_len = Some(max_len);
        self
    }

    /// Filters input to digits, one leading `-` and one `.`.
    #[must_use]
    pub fn with_numeric(mut self, numeric: bool) -> Self {
        self.numeric = numeric;
        self
    }

    /// The text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The caret byte offset (always on a grapheme boundary).
    #[must_use]
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// The selection as `(start, end)`, normalized, when it is not empty.
    #[must_use]
    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        if anchor == self.caret {
            return None;
        }
        Some(if anchor < self.caret {
            (anchor, self.caret)
        } else {
            (self.caret, anchor)
        })
    }

    /// The selected text, when there is a selection.
    #[must_use]
    pub fn selected_text(&self) -> Option<&str> {
        self.selection().map(|(start, end)| &self.text[start..end])
    }

    /// Whether the text differs from what the editor was opened with.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Marks the current text as the clean baseline.
    pub fn mark_clean(&mut self) {
        self.opened_with = self.text.clone();
        self.dirty = false;
    }

    /// The current IME composition, when one is active.
    #[must_use]
    pub fn composition(&self) -> Option<&str> {
        self.composition
            .as_ref()
            .map(|composition| composition.text.as_str())
    }

    /// The number of undo steps available.
    #[must_use]
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// The number of redo steps available.
    #[must_use]
    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// Replaces the text and moves the caret to the end; the old text becomes
    /// the clean baseline and the undo history is cleared.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.caret = self.text.len();
        self.anchor = None;
        self.preferred_column = None;
        self.undo.clear();
        self.redo.clear();
        self.composition = None;
        self.opened_with = self.text.clone();
        self.dirty = false;
        self.invalidate_lines();
    }

    /// Sets the caret, clamped to a grapheme boundary.
    pub fn set_caret(&mut self, byte: usize) {
        self.caret = grapheme::floor_boundary(&self.text, byte);
        self.anchor = None;
        self.preferred_column = None;
    }

    /// Selects everything.
    pub fn select_all(&mut self) -> EditorOutcome {
        if self.text.is_empty() {
            return EditorOutcome::Ignored;
        }
        self.anchor = Some(0);
        self.caret = self.text.len();
        self.preferred_column = None;
        EditorOutcome::Changed
    }

    /// The cached line layout, rebuilt only when the text or the metrics
    /// width changed.
    pub fn lines(&mut self, metrics: &dyn TextMetrics) -> &[LineInfo] {
        let width = metrics.advance("M");
        if self.line_cache_text != self.text
            || (width - self.metrics_width).abs() > f32::EPSILON
            || self.lines.is_empty()
        {
            self.rebuild_lines(metrics);
        }
        &self.lines
    }

    /// The caret's line index, horizontal offset and top, and the line height.
    pub fn caret_rect(&mut self, metrics: &dyn TextMetrics) -> (usize, f32, f32, f32) {
        let line_index = self.line_index(self.caret);
        let line_height = metrics.line_height();
        let (line_start, _line_end) = {
            let lines = self.lines(metrics);
            let line = lines[line_index.min(lines.len().saturating_sub(1))];
            (line.start, line.end)
        };
        let x = metrics.advance(&self.text[line_start..self.caret]);
        let y = line_index as f32 * line_height;
        (line_index, x, y, line_height)
    }

    /// Scrolls horizontally so the caret is visible in `viewport_width`.
    pub fn keep_caret_visible(&mut self, viewport_width: f32, metrics: &dyn TextMetrics) {
        let (_, x, _, _) = self.caret_rect(metrics);
        let margin = metrics.advance("M");
        if x - self.scroll_x < margin {
            self.scroll_x = (x - margin).max(0.0);
        } else if x + margin > self.scroll_x + viewport_width {
            self.scroll_x = x + margin - viewport_width;
        }
        if self.scroll_x < 0.0 {
            self.scroll_x = 0.0;
        }
    }

    /// The horizontal scroll offset.
    #[must_use]
    pub fn scroll_x(&self) -> f32 {
        self.scroll_x
    }

    /// Whether the caret is in its visible half of the blink cycle.
    ///
    /// The cycle is 500 ms on, 500 ms off, reset by every edit.
    #[must_use]
    pub fn caret_visible(&self, now_ms: f64) -> bool {
        let elapsed = (now_ms - self.blink_origin).rem_euclid(1000.0);
        elapsed < 500.0
    }

    /// The validation result for the current text.
    #[must_use]
    pub fn validate(&self) -> Option<Validation> {
        if let Some(max_len) = self.max_len {
            if grapheme::count(&self.text) > max_len {
                return Some(Validation::TooLong);
            }
        }
        if self.numeric && !is_numeric(&self.text) {
            return Some(Validation::NotNumeric);
        }
        None
    }

    /// Copies the selection to the clipboard.
    pub fn copy(&mut self, clipboard: &mut dyn Clipboard) -> EditorOutcome {
        let Some(text) = self.selected_text() else {
            return EditorOutcome::Ignored;
        };
        let text = text.to_string();
        clipboard.write(&text);
        EditorOutcome::Changed
    }

    /// Cuts the selection to the clipboard.
    pub fn cut(&mut self, now_ms: f64, clipboard: &mut dyn Clipboard) -> EditorOutcome {
        let Some((start, end)) = self.selection() else {
            return EditorOutcome::Ignored;
        };
        let text = self.text[start..end].to_string();
        clipboard.write(&text);
        self.replace(start, end, "", now_ms, false);
        EditorOutcome::Changed
    }

    /// Pastes the clipboard, sanitising it for the field kind.
    pub fn paste(&mut self, now_ms: f64, clipboard: &mut dyn Clipboard) -> EditorOutcome {
        let Some(raw) = clipboard.read() else {
            return EditorOutcome::Ignored;
        };
        let sanitised = self.sanitise(&raw);
        if sanitised.is_empty() {
            return EditorOutcome::Ignored;
        }
        let (start, end) = self.selection().unwrap_or((self.caret, self.caret));
        let sanitised = self.clamp_insertion(start, end, &sanitised);
        if sanitised.is_empty() {
            return EditorOutcome::Ignored;
        }
        self.replace(start, end, &sanitised, now_ms, false);
        EditorOutcome::Changed
    }

    /// Undoes the last step.
    pub fn undo(&mut self, now_ms: f64) -> EditorOutcome {
        let Some(edit) = self.undo.pop() else {
            return EditorOutcome::Ignored;
        };
        let _caret = self.replace_raw(edit.start, edit.start + edit.inserted.len(), &edit.removed);
        self.caret = edit.caret_before.min(self.text.len());
        self.anchor = None;
        self.redo.push(edit);
        self.blink_origin = now_ms;
        self.dirty = self.text != self.opened_with;
        EditorOutcome::Changed
    }

    /// Redoes the last undone step.
    pub fn redo(&mut self, now_ms: f64) -> EditorOutcome {
        let Some(edit) = self.redo.pop() else {
            return EditorOutcome::Ignored;
        };
        let caret = self.replace_raw(edit.start, edit.start + edit.removed.len(), &edit.inserted);
        self.caret = caret.min(self.text.len());
        self.anchor = None;
        self.undo.push(edit);
        self.blink_origin = now_ms;
        self.dirty = self.text != self.opened_with;
        EditorOutcome::Changed
    }

    /// Starts a composition at the caret; the composed text is not part of the
    /// document until [`Self::commit_composition`].
    pub fn set_composition(&mut self, text: impl Into<String>) {
        let selection = self.selection();
        let start = selection.map_or(self.caret, |(start, _)| start);
        if let Some((start, end)) = selection {
            self.text.replace_range(start..end, "");
            self.caret = start;
            self.anchor = None;
            self.invalidate_lines();
        }
        self.composition = Some(Composition {
            start,
            text: text.into(),
        });
    }

    /// Commits the composition into the document.
    pub fn commit_composition(&mut self, now_ms: f64) -> EditorOutcome {
        let Some(composition) = self.composition.take() else {
            return EditorOutcome::Ignored;
        };
        if composition.text.is_empty() {
            return EditorOutcome::Ignored;
        }
        let text = self.sanitise(&composition.text);
        let start = composition.start.min(self.text.len());
        self.replace(start, start, &text, now_ms, false);
        EditorOutcome::Changed
    }

    /// Discards the composition.
    pub fn cancel_composition(&mut self) {
        self.composition = None;
    }

    /// Handles one key.
    #[allow(clippy::too_many_lines)] // reason: one match per key keeps the precedence visible
    pub fn handle_key(
        &mut self,
        key: EditorKey,
        modifiers: Modifiers,
        now_ms: f64,
        clipboard: &mut dyn Clipboard,
    ) -> EditorOutcome {
        let control = modifiers.control || modifiers.super_key;
        if control {
            match key {
                EditorKey::Char('a' | 'A') => return self.select_all(),
                EditorKey::Char('c' | 'C') => return self.copy(clipboard),
                EditorKey::Char('x' | 'X') => return self.cut(now_ms, clipboard),
                EditorKey::Char('v' | 'V') => return self.paste(now_ms, clipboard),
                EditorKey::Char('z' | 'Z') if modifiers.shift => return self.redo(now_ms),
                EditorKey::Char('z' | 'Z') => return self.undo(now_ms),
                EditorKey::Char('y' | 'Y') => return self.redo(now_ms),
                EditorKey::Home => {
                    self.move_to(0, modifiers.shift);
                    return EditorOutcome::Changed;
                }
                EditorKey::End => {
                    self.move_to(self.text.len(), modifiers.shift);
                    return EditorOutcome::Changed;
                }
                EditorKey::Left => {
                    self.move_to(
                        grapheme::previous_word(&self.text, self.caret),
                        modifiers.shift,
                    );
                    return EditorOutcome::Changed;
                }
                EditorKey::Right => {
                    self.move_to(grapheme::next_word(&self.text, self.caret), modifiers.shift);
                    return EditorOutcome::Changed;
                }
                EditorKey::Backspace => {
                    let start = grapheme::previous_word(&self.text, self.caret);
                    self.delete_range(start, self.caret, now_ms);
                    return EditorOutcome::Changed;
                }
                EditorKey::Delete => {
                    let mut end = grapheme::next_word(&self.text, self.caret);
                    while end < self.text.len() {
                        let next = grapheme::next(&self.text, end);
                        if self.text[end..next].chars().all(char::is_whitespace) {
                            end = next;
                        } else {
                            break;
                        }
                    }
                    self.delete_range(self.caret, end, now_ms);
                    return EditorOutcome::Changed;
                }
                _ => {}
            }
        }
        match key {
            EditorKey::Char(ch) if !modifiers.alt => {
                self.insert(&ch.to_string(), now_ms);
                EditorOutcome::Changed
            }
            EditorKey::Left => {
                self.move_to(grapheme::previous(&self.text, self.caret), modifiers.shift);
                EditorOutcome::Changed
            }
            EditorKey::Right => {
                self.move_to(grapheme::next(&self.text, self.caret), modifiers.shift);
                EditorOutcome::Changed
            }
            EditorKey::Up => {
                self.vertical(-1, modifiers.shift);
                EditorOutcome::Changed
            }
            EditorKey::Down => {
                self.vertical(1, modifiers.shift);
                EditorOutcome::Changed
            }
            EditorKey::Home => {
                let (start, _) = grapheme::line_at(&self.text, self.caret);
                self.move_to(start, modifiers.shift);
                EditorOutcome::Changed
            }
            EditorKey::End => {
                let (_, end) = grapheme::line_at(&self.text, self.caret);
                self.move_to(end, modifiers.shift);
                EditorOutcome::Changed
            }
            EditorKey::Backspace => {
                self.delete_selection_or(now_ms, -1);
                EditorOutcome::Changed
            }
            EditorKey::Delete => {
                self.delete_selection_or(now_ms, 1);
                EditorOutcome::Changed
            }
            EditorKey::Enter => {
                if self.single_line {
                    EditorOutcome::Submit
                } else {
                    self.insert("\n", now_ms);
                    EditorOutcome::Changed
                }
            }
            EditorKey::Escape => {
                if self.text != self.opened_with || self.anchor.is_some() {
                    self.text = self.opened_with.clone();
                    self.caret = self.text.len();
                    self.anchor = None;
                    self.composition = None;
                    self.undo.clear();
                    self.redo.clear();
                    self.dirty = false;
                    self.invalidate_lines();
                    EditorOutcome::Revert
                } else {
                    EditorOutcome::Ignored
                }
            }
            EditorKey::Tab | EditorKey::Char(_) => EditorOutcome::Ignored,
        }
    }

    /// Places the caret (or the selection, for a double/triple click) from a
    /// pointer press at horizontal offset `x` in the caret's line.
    pub fn pointer_down(
        &mut self,
        x: f32,
        click_count: u8,
        extend: bool,
        metrics: &dyn TextMetrics,
    ) {
        let line = self.line_index(self.caret);
        let byte = self.byte_at_x(line, x, metrics);
        match click_count {
            2 => {
                let (start, end) = grapheme::word_at(&self.text, byte);
                self.anchor = Some(start);
                self.caret = end;
            }
            3 => {
                let (start, end) = grapheme::line_at(&self.text, byte);
                self.anchor = Some(start);
                self.caret = end;
            }
            _ => {
                if extend {
                    if self.anchor.is_none() {
                        self.anchor = Some(self.caret);
                    }
                } else {
                    self.anchor = None;
                }
                self.caret = byte;
            }
        }
        self.preferred_column = None;
    }

    /// Extends the selection to the pointer position while dragging.
    pub fn pointer_drag(&mut self, x: f32, metrics: &dyn TextMetrics) {
        let line = self.line_index(self.caret);
        let byte = self.byte_at_x(line, x, metrics);
        if self.anchor.is_none() {
            self.anchor = Some(self.caret);
        }
        self.caret = byte;
    }

    /// The byte offset nearest to horizontal `x` on `line`.
    #[must_use]
    pub fn byte_at_x(&mut self, line: usize, x: f32, metrics: &dyn TextMetrics) -> usize {
        let (start, end) = {
            let lines = self.lines(metrics);
            let line = line.min(lines.len().saturating_sub(1));
            (lines[line].start, lines[line].end)
        };
        let mut cursor = start;
        let mut best = start;
        let mut best_distance = x.abs();
        while cursor < end {
            let next = grapheme::next(&self.text, cursor);
            let advance = metrics.advance(&self.text[cursor..next]);
            let mid = metrics.advance(&self.text[start..cursor]) + advance * 0.5;
            let distance = (mid - x).abs();
            if distance < best_distance {
                best_distance = distance;
                best = if x >= mid { next } else { cursor };
            }
            cursor = next;
        }
        best
    }

    fn line_index(&self, byte: usize) -> usize {
        self.text[..byte.min(self.text.len())]
            .bytes()
            .filter(|candidate| *candidate == b'\n')
            .count()
    }

    fn invalidate_lines(&mut self) {
        self.line_cache_text_len = usize::MAX;
        self.line_cache_text.clear();
        self.lines.clear();
        self.metrics_width = f32::NAN;
    }

    fn rebuild_lines(&mut self, metrics: &dyn TextMetrics) {
        self.lines.clear();
        self.line_cache_text.clear();
        self.line_cache_text.push_str(&self.text);
        self.line_cache_text_len = self.text.len();
        self.metrics_width = metrics.advance("M");
        let mut start = 0;
        for (index, byte) in self.text.bytes().enumerate() {
            if byte == b'\n' {
                self.lines.push(LineInfo {
                    start,
                    end: index,
                    width: metrics.advance(&self.text[start..index]),
                });
                start = index + 1;
            }
        }
        self.lines.push(LineInfo {
            start,
            end: self.text.len(),
            width: metrics.advance(&self.text[start..]),
        });
    }

    fn move_to(&mut self, byte: usize, extend: bool) {
        let byte = grapheme::floor_boundary(&self.text, byte);
        if extend {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret);
            }
        } else {
            self.anchor = None;
        }
        self.caret = byte;
        self.preferred_column = None;
    }

    fn vertical(&mut self, direction: i32, extend: bool) {
        let line = self.line_index(self.caret) as i32 + direction;
        let line_count = self.line_index(self.text.len()) as i32;
        if line < 0 {
            self.move_to(0, extend);
            return;
        }
        if line > line_count {
            self.move_to(self.text.len(), extend);
            return;
        }
        let column = self.preferred_column.unwrap_or_else(|| {
            let (start, _) = grapheme::line_at(&self.text, self.caret);
            (self.caret - start) as f32
        });
        self.preferred_column = Some(column);
        // Without metrics the preferred column is byte based; the host calls
        // `vertical_metric` when it has real advances.
        let target = self.line_column_byte(line as usize, column);
        if extend {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret);
            }
        } else {
            self.anchor = None;
        }
        self.caret = target;
    }

    fn line_column_byte(&self, line: usize, column: f32) -> usize {
        let mut start = 0;
        for _ in 0..line {
            match self.text[start..].find('\n') {
                Some(index) => start += index + 1,
                None => return self.text.len(),
            }
        }
        let end = self.text[start..]
            .find('\n')
            .map_or(self.text.len(), |index| start + index);
        let mut cursor = start;
        let mut consumed = 0.0;
        while cursor < end {
            let next = grapheme::next(&self.text, cursor);
            consumed += 1.0;
            if consumed > column {
                return if column - (consumed - 1.0) >= 0.5 {
                    next
                } else {
                    cursor
                };
            }
            cursor = next;
        }
        end
    }

    fn insert(&mut self, text: &str, now_ms: f64) {
        let text = self.sanitise(text);
        if text.is_empty() {
            return;
        }
        let (start, end) = self.selection().unwrap_or((self.caret, self.caret));
        let text = self.clamp_insertion(start, end, &text);
        if text.is_empty() {
            return;
        }
        self.replace(start, end, &text, now_ms, true);
    }

    fn clamp_insertion(&self, start: usize, end: usize, text: &str) -> String {
        let Some(max_len) = self.max_len else {
            return text.to_string();
        };
        let used = grapheme::count(&self.text[..start]) + grapheme::count(&self.text[end..]);
        let room = max_len.saturating_sub(used);
        UnicodeSegmentation::graphemes(text, true)
            .take(room)
            .collect()
    }

    fn delete_selection_or(&mut self, now_ms: f64, direction: i32) {
        if let Some((start, end)) = self.selection() {
            self.replace(start, end, "", now_ms, false);
            return;
        }
        if direction < 0 {
            let start = grapheme::previous(&self.text, self.caret);
            self.replace(start, self.caret, "", now_ms, false);
        } else {
            let end = grapheme::next(&self.text, self.caret);
            self.replace(self.caret, end, "", now_ms, false);
        }
    }

    fn delete_range(&mut self, start: usize, end: usize, now_ms: f64) {
        if start < end {
            self.replace(start, end, "", now_ms, false);
        }
    }

    fn replace(&mut self, start: usize, end: usize, inserted: &str, now_ms: f64, coalesce: bool) {
        if start > end || end > self.text.len() {
            return;
        }
        self.redo.clear();
        let removed = self.text[start..end].to_string();
        if removed.is_empty() && inserted.is_empty() {
            return;
        }
        let caret_before = self.caret;
        let merge = coalesce
            && inserted.chars().count() == 1
            && self.undo.last().is_some_and(|last| {
                last.coalesce
                    && !last.closed
                    && last.start + last.inserted.len() == start
                    && now_ms - last.at_ms <= 500.0
                    && end == start
            });
        if merge {
            let last = self.undo.last_mut().expect("checked just above");
            last.inserted.push_str(inserted);
            last.at_ms = now_ms;
            last.closed = inserted
                .chars()
                .next()
                .is_some_and(|ch| !ch.is_alphanumeric());
            self.text.replace_range(start..end, inserted);
        } else {
            self.text.replace_range(start..end, inserted);
            self.undo.push(Edit {
                start,
                removed,
                inserted: inserted.to_string(),
                at_ms: now_ms,
                caret_before,
                coalesce,
                closed: inserted
                    .chars()
                    .next()
                    .is_some_and(|ch| !ch.is_alphanumeric()),
            });
            if self.undo.len() > 256 {
                self.undo.remove(0);
            }
        }
        self.caret = start + inserted.len();
        self.anchor = None;
        self.preferred_column = None;
        self.blink_origin = now_ms;
        self.dirty = self.text != self.opened_with;
        self.invalidate_lines();
    }

    fn replace_raw(&mut self, start: usize, end: usize, inserted: &str) -> usize {
        let end = end.min(self.text.len());
        self.text.replace_range(start..end, inserted);
        self.invalidate_lines();
        start + inserted.len()
    }

    fn sanitise(&self, text: &str) -> String {
        let mut output = String::with_capacity(text.len());
        if self.numeric {
            let has_minus = self.text.contains('-') || text.contains('-');
            let has_point = self.text.contains('.') || text.contains('.');
            for ch in text.chars() {
                match ch {
                    '0'..='9' => output.push(ch),
                    '-' if has_minus && !self.text.contains('-') => output.push(ch),
                    '.' if has_point && !self.text.contains('.') => output.push(ch),
                    _ => {}
                }
            }
            return output;
        }
        for ch in text.chars() {
            if self.single_line && (ch == '\r' || ch == '\n' || ch == '\t') {
                if !output.ends_with(' ') {
                    output.push(' ');
                }
            } else {
                output.push(ch);
            }
        }
        output
    }
}

fn is_numeric(text: &str) -> bool {
    let mut minus = false;
    let mut point = false;
    let mut digits = 0;
    for (index, ch) in text.chars().enumerate() {
        match ch {
            '-' if index == 0 && !minus => minus = true,
            '.' if !point => point = true,
            ch if ch.is_ascii_digit() => digits += 1,
            _ => return false,
        }
    }
    digits > 0 || minus || point
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // reason: exact token arithmetic in tests
    use super::{
        EditorKey, EditorOutcome, Modifiers, Monospace, NullClipboard, TextEditor, TextMetrics,
    };

    struct TestClipboard(Option<String>);

    impl super::Clipboard for TestClipboard {
        fn read(&mut self) -> Option<String> {
            self.0.clone()
        }

        fn write(&mut self, text: &str) {
            self.0 = Some(text.to_string());
        }
    }

    fn editor(text: &str) -> TextEditor {
        TextEditor::new(text)
    }

    fn key(editor: &mut TextEditor, key: EditorKey, modifiers: Modifiers) -> EditorOutcome {
        editor.handle_key(key, modifiers, 0.0, &mut NullClipboard)
    }

    fn key_at(
        editor: &mut TextEditor,
        key: EditorKey,
        modifiers: Modifiers,
        now_ms: f64,
    ) -> EditorOutcome {
        editor.handle_key(key, modifiers, now_ms, &mut NullClipboard)
    }

    #[test]
    fn movement_table() {
        // (text, caret, key, modifiers) -> (caret, selection)
        let rows = [
            ("hello", 0, EditorKey::Right, Modifiers::none(), 1, None),
            ("hello", 5, EditorKey::Left, Modifiers::none(), 4, None),
            (
                "hello world",
                5,
                EditorKey::Left,
                Modifiers::control(),
                0,
                None,
            ),
            (
                "hello world",
                0,
                EditorKey::Right,
                Modifiers::control(),
                5,
                None,
            ),
            ("one two", 0, EditorKey::End, Modifiers::none(), 7, None),
            ("one two", 7, EditorKey::Home, Modifiers::none(), 0, None),
            (
                "one\ntwo",
                5,
                EditorKey::Home,
                Modifiers::control(),
                0,
                None,
            ),
            ("one\ntwo", 1, EditorKey::End, Modifiers::control(), 7, None),
            (
                "hello",
                2,
                EditorKey::Right,
                Modifiers::shift(),
                3,
                Some((2, 3)),
            ),
            (
                "hello",
                2,
                EditorKey::Left,
                Modifiers::shift(),
                1,
                Some((1, 2)),
            ),
        ];
        for (text, caret, input, modifiers, expected_caret, selection) in rows {
            let mut editor = editor(text);
            editor.set_caret(caret);
            key(&mut editor, input, modifiers);
            assert_eq!(editor.caret(), expected_caret, "{text} {input:?}");
            assert_eq!(editor.selection(), selection, "{text} {input:?}");
        }
    }

    #[test]
    fn character_editing_and_selection_replacement() {
        let mut editor = editor("helo");
        editor.set_caret(2);
        key(&mut editor, EditorKey::Char('l'), Modifiers::none());
        assert_eq!(editor.text(), "hello");
        editor.select_all();
        key(&mut editor, EditorKey::Char('x'), Modifiers::none());
        assert_eq!(editor.text(), "x");
        assert_eq!(editor.caret(), 1);
    }

    #[test]
    fn grapheme_deletion_removes_the_whole_cluster() {
        let mut editor = editor("a\u{0301}👩‍👩‍👧‍👦");
        key(&mut editor, EditorKey::Backspace, Modifiers::none());
        assert_eq!(editor.text(), "a\u{0301}");
        key(&mut editor, EditorKey::Backspace, Modifiers::none());
        assert_eq!(editor.text(), "");
    }

    #[test]
    fn word_deletion_with_control() {
        let mut editor = editor("hello big world");
        key(&mut editor, EditorKey::Backspace, Modifiers::control());
        assert_eq!(editor.text(), "hello big ");
        key(&mut editor, EditorKey::Delete, Modifiers::control());
        assert_eq!(editor.text(), "hello big ");
        editor.set_caret(6);
        key(&mut editor, EditorKey::Delete, Modifiers::control());
        assert_eq!(editor.text(), "hello ");
    }

    #[test]
    fn undo_redo_and_typing_coalescing() {
        let mut editor = editor("");
        for (index, ch) in "word".chars().enumerate() {
            key_at(
                &mut editor,
                EditorKey::Char(ch),
                Modifiers::none(),
                index as f64 * 100.0,
            );
        }
        assert_eq!(editor.text(), "word");
        assert_eq!(editor.undo_depth(), 1);
        key_at(&mut editor, EditorKey::Char(' '), Modifiers::none(), 400.0);
        assert_eq!(editor.undo_depth(), 1);
        key_at(&mut editor, EditorKey::Char('x'), Modifiers::none(), 5000.0);
        assert_eq!(editor.undo_depth(), 2);
        key_at(&mut editor, EditorKey::Char('z'), Modifiers::control(), 0.0);
        assert_eq!(editor.text(), "word ");
        key_at(&mut editor, EditorKey::Char('y'), Modifiers::control(), 0.0);
        assert_eq!(editor.text(), "word x");
        key_at(
            &mut editor,
            EditorKey::Char('z'),
            Modifiers::control_shift(),
            0.0,
        );
        assert_eq!(editor.text(), "word x");
    }

    #[test]
    fn clipboard_round_trip_and_paste_sanitising() {
        let mut editor = editor("hello world");
        editor.select_all();
        let mut clipboard = TestClipboard(None);
        assert_eq!(editor.copy(&mut clipboard), EditorOutcome::Changed);
        assert_eq!(clipboard.0.as_deref(), Some("hello world"));
        editor.set_caret(0);
        let mut paste = TestClipboard(Some("line\r\none\t".to_string()));
        let mut single = editor.clone().with_single_line(true);
        single.set_caret(0);
        single.paste(0.0, &mut paste);
        assert_eq!(single.text(), "line one hello world");
        assert_eq!(editor.cut(0.0, &mut clipboard), EditorOutcome::Ignored);
        editor.select_all();
        assert_eq!(editor.cut(0.0, &mut clipboard), EditorOutcome::Changed);
        assert_eq!(editor.text(), "");
    }

    #[test]
    fn ime_composition_commits_or_cancels() {
        let mut editor = editor("ab");
        editor.set_caret(1);
        editor.set_composition("に");
        assert_eq!(editor.text(), "ab");
        assert_eq!(editor.composition(), Some("に"));
        editor.commit_composition(0.0);
        assert_eq!(editor.text(), "aにb");
        assert_eq!(editor.composition(), None);
        editor.set_composition("x");
        editor.cancel_composition();
        assert_eq!(editor.text(), "aにb");
    }

    #[test]
    fn double_and_triple_click_select_words_and_lines() {
        let metrics = Monospace::new(10.0);
        let mut editor = editor("one two\nthree four");
        editor.set_caret(0);
        editor.pointer_down(35.0, 2, false, &metrics);
        assert_eq!(editor.selected_text(), Some("two"));
        editor.pointer_down(10.0, 3, false, &metrics);
        assert_eq!(editor.selected_text(), Some("one two"));
        editor.pointer_down(0.0, 1, false, &metrics);
        assert_eq!(editor.caret(), 0);
        assert_eq!(editor.selection(), None);
    }

    #[test]
    fn shift_click_extends_and_drag_selects() {
        let metrics = Monospace::new(10.0);
        let mut editor = editor("abcdef");
        editor.set_caret(1);
        editor.pointer_down(40.0, 1, true, &metrics);
        assert_eq!(editor.selection(), Some((1, 4)));
        editor.set_caret(0);
        editor.pointer_down(10.0, 1, false, &metrics);
        editor.pointer_drag(50.0, &metrics);
        assert_eq!(editor.selection(), Some((1, 5)));
    }

    #[test]
    fn up_and_down_move_between_lines() {
        let mut editor = editor("ab\ncdef\ngh");
        editor.set_caret(6);
        key(&mut editor, EditorKey::Up, Modifiers::none());
        assert_eq!(editor.caret(), 2);
        key(&mut editor, EditorKey::Down, Modifiers::none());
        assert_eq!(editor.caret(), 6);
        key(&mut editor, EditorKey::Down, Modifiers::none());
        assert_eq!(editor.caret(), 10);
    }

    #[test]
    fn validation_and_limits() {
        let mut editor = editor("").with_max_len(3);
        for ch in "abcd".chars() {
            key(&mut editor, EditorKey::Char(ch), Modifiers::none());
        }
        assert_eq!(editor.text(), "abc");
        assert_eq!(editor.validate(), None);
        let mut numeric = editor_with_numeric();
        for ch in "a-1.2.3".chars() {
            key(&mut numeric, EditorKey::Char(ch), Modifiers::none());
        }
        assert_eq!(numeric.text(), "-1.23");
        assert_eq!(numeric.validate(), None);
    }

    fn editor_with_numeric() -> TextEditor {
        TextEditor::new("")
            .with_single_line(true)
            .with_numeric(true)
    }

    #[test]
    fn enter_submits_a_single_line_and_escape_reverts() {
        let mut editor = editor("a").with_single_line(true);
        assert_eq!(
            key(&mut editor, EditorKey::Enter, Modifiers::none()),
            EditorOutcome::Submit
        );
        key(&mut editor, EditorKey::Char('b'), Modifiers::none());
        assert_eq!(editor.text(), "ab");
        assert_eq!(
            key(&mut editor, EditorKey::Escape, Modifiers::none()),
            EditorOutcome::Revert
        );
        assert_eq!(editor.text(), "a");
        assert_eq!(
            key(&mut editor, EditorKey::Escape, Modifiers::none()),
            EditorOutcome::Ignored
        );
        assert_eq!(
            key(&mut editor, EditorKey::Tab, Modifiers::none()),
            EditorOutcome::Ignored
        );
    }

    #[test]
    fn caret_blinks_and_resets_on_edit() {
        let mut editor = editor("");
        assert!(editor.caret_visible(0.0));
        assert!(!editor.caret_visible(600.0));
        key_at(&mut editor, EditorKey::Char('a'), Modifiers::none(), 1200.0);
        assert!(editor.caret_visible(1200.0));
        assert!(!editor.caret_visible(1800.0));
    }

    #[test]
    fn caret_geometry_and_visibility() {
        let metrics = Monospace::new(10.0);
        let mut editor = editor("hello");
        let (_, x, y, height) = editor.caret_rect(&metrics);
        assert_eq!(x, 50.0);
        assert_eq!(y, 0.0);
        assert_eq!(height, 14.0);
        editor.set_caret(0);
        editor.keep_caret_visible(40.0, &metrics);
        assert_eq!(editor.scroll_x(), 0.0);
        editor.set_caret(5);
        editor.keep_caret_visible(40.0, &metrics);
        assert_eq!(editor.scroll_x(), 20.0);
        assert!((metrics.advance("hello") - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn line_cache_rebuilds_only_when_needed() {
        let metrics = Monospace::new(10.0);
        let mut editor = editor("a\nbb\nccc");
        assert_eq!(editor.lines(&metrics).len(), 3);
        let first = editor.lines(&metrics)[2].width;
        key(&mut editor, EditorKey::Char('!'), Modifiers::none());
        assert_eq!(editor.lines(&metrics).len(), 3);
        assert!(editor.lines(&metrics)[2].width > first);
    }
}
