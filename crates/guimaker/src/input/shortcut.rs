//! The shortcut registry: chords bound to messages, with scopes and conflict
//! detection.
//!
//! A lookup is one hash of a [`Chord`] into the table of the highest active
//! scope (`Overlay` beats `Focused` beats `Global`), so a key press is `O(1)`
//! and allocation-free. The tables are built when a binding is added, never per
//! frame.

use std::collections::HashMap;
use std::fmt;

/// A key a shortcut can name.
///
/// The set is the one a desktop app actually binds; an unknown key is reported
/// as [`ShortcutKey::Other`] so a host can still round-trip it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShortcutKey {
    /// A letter, uppercase, layout independent (`A`..`Z`).
    Letter(u8),
    /// A digit `0`..`9`.
    Digit(u8),
    /// A function key `1`..`24`.
    Function(u8),
    /// Escape.
    Escape,
    /// Enter or Return.
    Enter,
    /// Tab.
    Tab,
    /// Space.
    Space,
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// Insert.
    Insert,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Left arrow.
    ArrowLeft,
    /// Right arrow.
    ArrowRight,
    /// Up arrow.
    ArrowUp,
    /// Down arrow.
    ArrowDown,
    /// Comma.
    Comma,
    /// Period.
    Period,
    /// Slash.
    Slash,
    /// Backslash.
    Backslash,
    /// Semicolon.
    Semicolon,
    /// Quote.
    Quote,
    /// Minus.
    Minus,
    /// Equal.
    Equal,
    /// Bracket left.
    BracketLeft,
    /// Bracket right.
    BracketRight,
    /// Backtick.
    Backquote,
    /// Any other key, kept by its numeric code.
    Other(u32),
}

impl ShortcutKey {
    /// A letter from an ASCII letter (upper- or lowercase), else `None`.
    #[must_use]
    pub fn letter(ch: char) -> Option<Self> {
        let ch = ch.to_ascii_uppercase();
        ch.is_ascii_uppercase().then_some(Self::Letter(ch as u8))
    }

    /// A digit from an ASCII digit.
    #[must_use]
    pub fn digit(ch: char) -> Option<Self> {
        ch.to_digit(10)
            .filter(|value| *value <= 9)
            .map(|value| Self::Digit(value as u8))
    }

    /// A key from a printable character, when it is one this enum names.
    #[must_use]
    pub fn from_char(ch: char) -> Option<Self> {
        if let Some(letter) = Self::letter(ch) {
            return Some(letter);
        }
        if let Some(digit) = Self::digit(ch) {
            return Some(digit);
        }
        Some(match ch {
            ',' => Self::Comma,
            '.' => Self::Period,
            '/' => Self::Slash,
            '\\' => Self::Backslash,
            ';' => Self::Semicolon,
            '\'' => Self::Quote,
            '-' => Self::Minus,
            '=' => Self::Equal,
            '[' => Self::BracketLeft,
            ']' => Self::BracketRight,
            '`' => Self::Backquote,
            ' ' => Self::Space,
            '\t' => Self::Tab,
            '\n' | '\r' => Self::Enter,
            _ => return None,
        })
    }
}

impl fmt::Display for ShortcutKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Letter(letter) => write!(formatter, "{}", *letter as char),
            Self::Digit(digit) => write!(formatter, "{digit}"),
            Self::Function(number) => write!(formatter, "F{number}"),
            Self::Escape => formatter.write_str("Esc"),
            Self::Enter => formatter.write_str("Enter"),
            Self::Tab => formatter.write_str("Tab"),
            Self::Space => formatter.write_str("Space"),
            Self::Backspace => formatter.write_str("Backspace"),
            Self::Delete => formatter.write_str("Del"),
            Self::Insert => formatter.write_str("Ins"),
            Self::Home => formatter.write_str("Home"),
            Self::End => formatter.write_str("End"),
            Self::PageUp => formatter.write_str("PgUp"),
            Self::PageDown => formatter.write_str("PgDn"),
            Self::ArrowLeft => formatter.write_str("Left"),
            Self::ArrowRight => formatter.write_str("Right"),
            Self::ArrowUp => formatter.write_str("Up"),
            Self::ArrowDown => formatter.write_str("Down"),
            Self::Comma => formatter.write_str(","),
            Self::Period => formatter.write_str("."),
            Self::Slash => formatter.write_str("/"),
            Self::Backslash => formatter.write_str("\\"),
            Self::Semicolon => formatter.write_str(";"),
            Self::Quote => formatter.write_str("'"),
            Self::Minus => formatter.write_str("-"),
            Self::Equal => formatter.write_str("="),
            Self::BracketLeft => formatter.write_str("["),
            Self::BracketRight => formatter.write_str("]"),
            Self::Backquote => formatter.write_str("`"),
            Self::Other(code) => write!(formatter, "Key{code}"),
        }
    }
}

#[allow(clippy::struct_excessive_bools)] // reason: the four platform modifiers are independent
/// A key plus the modifiers it requires.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    /// The key.
    pub key: ShortcutKey,
    /// Control is required.
    pub control: bool,
    /// Shift is required.
    pub shift: bool,
    /// Alt is required.
    pub alt: bool,
    /// The platform key is required.
    pub super_key: bool,
}

impl Chord {
    /// A chord with no modifiers.
    #[must_use]
    pub const fn new(key: ShortcutKey) -> Self {
        Self {
            key,
            control: false,
            shift: false,
            alt: false,
            super_key: false,
        }
    }

    /// The same chord with Control required.
    #[must_use]
    pub const fn control(mut self) -> Self {
        self.control = true;
        self
    }

    /// The same chord with Shift required.
    #[must_use]
    pub const fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    /// The same chord with Alt required.
    #[must_use]
    pub const fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    /// The same chord with the platform key required.
    #[must_use]
    pub const fn super_key(mut self) -> Self {
        self.super_key = true;
        self
    }

    /// A chord for a single printable key (`'s'`, `'?'`).
    #[must_use]
    pub fn char(ch: char) -> Option<Self> {
        ShortcutKey::from_char(ch).map(Self::new)
    }

    /// The display string for a platform (`Ctrl+S`, `⌘S`).
    #[must_use]
    pub fn display(self, platform: Platform) -> String {
        let mut parts: Vec<String> = Vec::with_capacity(4);
        if self.control {
            parts.push(
                match platform {
                    Platform::Mac => "⌃",
                    Platform::Windows | Platform::Linux => "Ctrl",
                }
                .to_string(),
            );
        }
        if self.alt {
            parts.push(
                match platform {
                    Platform::Mac => "⌥",
                    Platform::Windows | Platform::Linux => "Alt",
                }
                .to_string(),
            );
        }
        if self.shift {
            parts.push(
                match platform {
                    Platform::Mac => "⇧",
                    Platform::Windows | Platform::Linux => "Shift",
                }
                .to_string(),
            );
        }
        if self.super_key {
            parts.push(
                match platform {
                    Platform::Mac => "⌘",
                    Platform::Windows => "Win",
                    Platform::Linux => "Super",
                }
                .to_string(),
            );
        }
        parts.push(self.key.to_string());
        let separator = match platform {
            Platform::Mac => "",
            Platform::Windows | Platform::Linux => "+",
        };
        parts.join(separator)
    }
}

/// The platform a display string is written for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    /// Windows.
    Windows,
    /// macOS.
    Mac,
    /// Linux.
    Linux,
}

impl Platform {
    /// The platform this build targets.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Linux
        }
    }
}

/// Where a binding applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    /// Always active.
    Global = 0,
    /// Active while a widget has keyboard focus.
    Focused = 1,
    /// Active while any overlay is open.
    Overlay = 2,
}

impl Scope {
    const COUNT: usize = 3;

    const fn index(self) -> usize {
        self as usize
    }
}

/// One registered shortcut.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding<Msg> {
    /// The chord.
    pub chord: Chord,
    /// The scope it applies in.
    pub scope: Scope,
    /// The message it emits.
    pub message: Msg,
    /// An optional human label for menus.
    pub label: Option<String>,
}

/// Two bindings that fight for the same chord in the same scope.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    /// The chord.
    pub chord: Chord,
    /// The scope.
    pub scope: Scope,
    /// The labels (or display strings) of the bindings involved.
    pub bindings: Vec<String>,
}

/// The registry of shortcuts.
#[derive(Clone, Debug)]
pub struct ShortcutRegistry<Msg> {
    bindings: Vec<Binding<Msg>>,
    tables: Vec<HashMap<Chord, usize>>,
    conflicts: Vec<Conflict>,
    platform: Platform,
}

impl<Msg> Default for ShortcutRegistry<Msg> {
    fn default() -> Self {
        Self {
            bindings: Vec::new(),
            tables: (0..Scope::COUNT).map(|_| HashMap::new()).collect(),
            conflicts: Vec::new(),
            platform: Platform::current(),
        }
    }
}

impl<Msg: Clone> ShortcutRegistry<Msg> {
    /// Builds an empty registry for a display platform.
    #[must_use]
    pub fn new(platform: Platform) -> Self {
        Self {
            bindings: Vec::new(),
            tables: (0..Scope::COUNT).map(|_| HashMap::new()).collect(),
            conflicts: Vec::new(),
            platform,
        }
    }

    /// The platform used for display strings.
    #[must_use]
    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// Binds a chord in a scope.
    ///
    /// Re-binding the same chord in the same scope replaces the message but
    /// records a conflict, so the design lint can report a screen that binds
    /// `Ctrl+S` twice.
    pub fn bind(&mut self, chord: Chord, scope: Scope, message: Msg) {
        self.bind_labelled(chord, scope, message, None);
    }

    /// Binds a chord with a menu label.
    pub fn bind_labelled(&mut self, chord: Chord, scope: Scope, message: Msg, label: Option<&str>) {
        let table = &mut self.tables[scope.index()];
        if let Some(existing) = table.get(&chord).copied() {
            let previous = self.bindings[existing]
                .label
                .clone()
                .unwrap_or_else(|| chord.display(self.platform));
            self.conflicts.push(Conflict {
                chord,
                scope,
                bindings: vec![
                    previous,
                    label.map_or_else(|| chord.display(self.platform), ToString::to_string),
                ],
            });
            self.bindings[existing].message = message;
            self.bindings[existing].label = label.map(ToString::to_string);
            return;
        }
        let index = self.bindings.len();
        table.insert(chord, index);
        self.bindings.push(Binding {
            chord,
            scope,
            message,
            label: label.map(ToString::to_string),
        });
    }

    /// Removes every binding in a scope.
    pub fn clear_scope(&mut self, scope: Scope) {
        let table = &mut self.tables[scope.index()];
        let removed: Vec<usize> = table.values().copied().collect();
        table.clear();
        self.bindings = self
            .bindings
            .iter()
            .enumerate()
            .filter(|(index, _)| !removed.contains(index))
            .map(|(_, binding)| binding.clone())
            .collect();
        self.reindex();
    }

    /// The message bound to a chord, honoring the scope priority.
    ///
    /// `focused` says whether a widget currently has focus; `overlay` whether
    /// an overlay is open. A lookup is one hash per active scope, at most
    /// three, and allocates nothing.
    #[must_use]
    pub fn lookup(&self, chord: Chord, focused: bool, overlay: bool) -> Option<&Msg> {
        if overlay {
            if let Some(message) = self.lookup_in(chord, Scope::Overlay) {
                return Some(message);
            }
        }
        if focused {
            if let Some(message) = self.lookup_in(chord, Scope::Focused) {
                return Some(message);
            }
        }
        self.lookup_in(chord, Scope::Global)
    }

    /// The display string of a chord on the registry's platform.
    #[must_use]
    pub fn display(&self, chord: Chord) -> String {
        chord.display(self.platform)
    }

    /// Every recorded conflict.
    #[must_use]
    pub fn conflicts(&self) -> &[Conflict] {
        &self.conflicts
    }

    /// Every binding, in registration order.
    #[must_use]
    pub fn bindings(&self) -> &[Binding<Msg>] {
        &self.bindings
    }

    /// Clears the conflict log (the lint reports it once).
    pub fn clear_conflicts(&mut self) {
        self.conflicts.clear();
    }

    fn lookup_in(&self, chord: Chord, scope: Scope) -> Option<&Msg> {
        self.tables[scope.index()]
            .get(&chord)
            .map(|index| &self.bindings[*index].message)
    }

    fn reindex(&mut self) {
        for table in &mut self.tables {
            table.clear();
        }
        for (index, binding) in self.bindings.iter().enumerate() {
            self.tables[binding.scope.index()].insert(binding.chord, index);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // reason: exact token arithmetic in tests
    use super::{Chord, Platform, Scope, ShortcutKey, ShortcutRegistry};

    #[derive(Clone, Debug, PartialEq)]
    enum Msg {
        Save,
        SaveAs,
        Close,
        Palette,
    }

    fn save() -> Chord {
        Chord::new(ShortcutKey::letter('s').unwrap()).control()
    }

    #[test]
    fn chord_parsing_and_display() {
        assert_eq!(ShortcutKey::from_char('?'), None);
        assert_eq!(ShortcutKey::from_char('K'), Some(ShortcutKey::Letter(b'K')));
        assert_eq!(Chord::char('7').unwrap().key, ShortcutKey::Digit(7));
        assert_eq!(save().display(Platform::Windows), "Ctrl+S");
        assert_eq!(save().display(Platform::Mac), "⌃S");
        assert_eq!(save().shift().display(Platform::Windows), "Ctrl+Shift+S");
    }

    #[test]
    fn lookup_honors_scope_priority() {
        let mut registry = ShortcutRegistry::new(Platform::Windows);
        registry.bind(save(), Scope::Global, Msg::Save);
        registry.bind(save(), Scope::Focused, Msg::SaveAs);
        assert_eq!(registry.lookup(save(), false, false), Some(&Msg::Save));
        assert_eq!(registry.lookup(save(), true, false), Some(&Msg::SaveAs));
        assert_eq!(registry.lookup(save(), true, true), Some(&Msg::SaveAs));
        registry.bind(save(), Scope::Overlay, Msg::Palette);
        assert_eq!(registry.lookup(save(), true, true), Some(&Msg::Palette));
        assert_eq!(registry.lookup(save(), true, false), Some(&Msg::SaveAs));
        assert_eq!(
            registry.lookup(Chord::new(ShortcutKey::Escape), true, true),
            None
        );
    }

    #[test]
    fn duplicate_binding_is_a_conflict() {
        let mut registry = ShortcutRegistry::new(Platform::Windows);
        registry.bind(save(), Scope::Global, Msg::Save);
        registry.bind_labelled(save(), Scope::Global, Msg::SaveAs, Some("Save as"));
        assert_eq!(registry.conflicts().len(), 1);
        assert_eq!(registry.bindings().len(), 1);
        assert_eq!(registry.lookup(save(), false, false), Some(&Msg::SaveAs));
        registry.clear_conflicts();
        assert!(registry.conflicts().is_empty());
    }

    #[test]
    fn clear_scope_keeps_other_scopes() {
        let mut registry = ShortcutRegistry::new(Platform::Windows);
        registry.bind(save(), Scope::Global, Msg::Save);
        registry.bind(Chord::new(ShortcutKey::Escape), Scope::Overlay, Msg::Close);
        registry.clear_scope(Scope::Overlay);
        assert_eq!(registry.bindings().len(), 1);
        assert_eq!(registry.lookup(save(), false, true), Some(&Msg::Save));
        assert_eq!(
            registry.lookup(Chord::new(ShortcutKey::Escape), false, true),
            None
        );
    }
}
