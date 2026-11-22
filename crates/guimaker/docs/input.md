# Input, focus, shortcuts and text editing

`guimaker::input` is the only place that turns raw platform events into UI
state. The engine (`ferrous_events`) supplies events, the platform input
parameters and a clock; GUIMaker owns the gesture state machine, the hit
index, focus, shortcuts, scrolling and the text editor. Nothing here reads
the system clock: the host calls `Ui::set_time_ms(now_ms)` once per frame and
every timed decision uses that instant.

```text
guimaker::input
├── gesture        click / double / triple, long press, drag, hover intent
├── focus          precomputed Tab order, scopes
├── shortcut       chords -> messages, scopes, conflicts, display strings
├── scroll         wheel / touchpad / momentum, one controller per container
├── virtual_list   O(1) visible window for uniform rows
├── tree           flatten, expand/collapse, navigation, type-ahead
├── grapheme       cluster, word and line boundaries
└── text_editor    caret, selection, clipboard, undo/redo, IME
```

## The routing contract

`route` is fed one `InputEvent` at a time (`InputEvent::PointerMoved`,
`PointerDown/Up`, `Wheel`, `KeyDown/Up`, `Text`, `Modifiers`, `PointerLeft`,
`FocusLost`). The frame coalesces a run of pointer moves to the latest before
routing, so a 1 000 Hz mouse costs one route per frame.

| Operation | Cost | Notes |
|---|---|---|
| Hit test | `O(cell)` | uniform grid over interactive rects, rebuilt only on layout/structure change; clips baked in |
| Hover inside the same node | one rect test | last hit and its rect are cached |
| Pointer move between two nodes | `O(cell)` + a `HashMap` state write | measured 28 ns/event at 100, 1 000 and 10 000 nodes |
| Click | move cost + gesture + one message | 42 ns/event on a 10 000-node screen |
| Wheel over a scroll container | ancestor walk to the nearest scroller + `O(1)` controller | 52 ns/event |
| `Tab` / `Shift+Tab` | `O(1)` | precomputed focus array, rebuilt only when the structure changes |
| Shortcut lookup | `O(1)` | one hash per active scope, max three (overlay, focused, global) |
| Virtual-list window | `O(1)` | 9 ns from 100 to 10 000 rows, 13 ns at 10 000 000 |
| Tree flatten | `O(visible)` | 17 µs for 10 000 rows into a reused buffer; the old engine version allocated per row (1.78 ms) |
| Tree navigation | `O(1)` | 7.5 ns per row |

The spatial index and the focus order are rebuilt when the tree's structure
or the layout changes, never per frame. A steady-state pointer event performs
**zero heap allocations**; `tests/alloc.rs` installs a counting allocator and
asserts 0 allocations / 0 bytes over 30 hover changes.

## Gestures

`GestureRecognizer` is a pure state machine over positions and instants. It
reports `Click { count }` (1, 2 or 3), `LongPress`, `DragStart`, `DragMove`,
`DragEnd` and `HoverIntent`. A press that travels past the drag threshold
becomes a drag and cancels the click. The platform double-click time and
distance come from `ferrous_events::InputConfig`; the long-press (500 ms) and
hover-intent (400 ms) delays are GUIMaker tunables. Every transition takes
`now_ms`, so a recorded sequence replays exactly.

`InputState::click_count` carries the count of the last completed click and
`InputState::hover_elapsed_ms` is what a tooltip reads before showing.

## Focus

`FocusOrder` is an array of the enabled, interactive nodes in tree order,
with `.tab_index()` overriding the position the way HTML does (positive
indices first, then the natural order, then negative). Tab and Shift+Tab wrap.
A focus scope (`.push_focus_scope(FocusScope { root, trap })`) restricts the
array to a subtree; dialogs push one and restore focus by popping it.

`Enter`/`Space` activate the focused node's message. `Escape` closes the
topmost overlay, or clears focus when no overlay is open. Arrow navigation
inside lists, menus, grids and radio groups is the relevant controller's job
(`input::tree`, later the list and menu components).

## Shortcuts

`ShortcutRegistry<Msg>` binds a `Chord` (key + modifiers) in a `Scope`
(`Global`, `Focused`, `Overlay`) to a message. Lookup checks `Overlay`, then
`Focused`, then `Global`, so a dialog can shadow `Ctrl+S` without removing the
global binding. `bind` twice on the same chord and scope replaces the message
and records a `Conflict` the design lint can report. `Chord::display(platform)`
produces `Ctrl+Shift+P`, `⌘⇧P` or `Super+Shift+P` for menus and `kbd`.

## Scrolling

`ScrollController` owns one container's offset and velocity: wheel lines
(`line_height` px per notch), touchpad pixels with exponential momentum
(`velocity *= exp(-friction * dt)`), Shift-horizontal mapping, clamping and
`ensure_visible` / `scroll_to_index`. The router finds the nearest scroll
ancestor from the hit node, mirrors the offset into the keyed store and uses
`LayoutEngine::content_size` for the bounds, so layout, paint and hit testing
agree. Scrolling invalidates layout only for that container.

## Virtual lists and trees

`VirtualList` returns the visible window in `O(1)` with overscan and a reused
output buffer; the controller never allocates. `Tree<T>` flattens a nested
model into an arena once, keeps the visible row list up to date on
expand/collapse, and offers keyboard navigation (`Up/Down/Left/Right/Home/End/
PageUp/PageDown/Enter`) plus type-ahead with an 800 ms buffer. Both replaced
the `ScrollView`/`VirtualList`/`TreeView` widgets that were deleted from
`ferrous_ui_core`; those allocated a `Vec` and cloned every label on every
draw. The benches live in `benches/controllers.rs`.

## Text editing

`TextEditor` is a headless state machine used by text fields, text areas,
number fields, search boxes and the command palette. It covers:

- caret movement by grapheme, word (`Ctrl+←/→`), line (`Home/End`), document
  (`Ctrl+Home/End`) and up/down with a preferred column;
- selection with Shift + any movement, `Ctrl+A`, drag, double-click (word),
  triple-click (line) and Shift+click; selection is anchor + caret;
- `Ctrl+C/X/V` through a host `Clipboard` trait, with paste sanitising for
  single-line and numeric fields;
- deletion by grapheme and by word, grapheme-correct for emoji, ZWJ sequences
  and combining marks (see `G-0004`);
- undo/redo (`Ctrl+Z`, `Ctrl+Y`, `Ctrl+Shift+Z`) with typing coalesced into
  word-sized steps (500 ms, closed at whitespace/punctuation, 256 steps);
- IME composition (`set_composition`, `commit_composition`,
  `cancel_composition`) with the composed text outside the document until it
  commits;
- caret blink from `now_ms` (500 ms on / 500 ms off, reset by an edit),
  horizontal scroll that keeps the caret visible, and a cached line layout
  (byte ranges + widths) rebuilt only when the text changes;
- validation (`max_len`, numeric) and `Enter` submit / `Esc` revert outcomes.

Caret geometry comes from a `TextMetrics` implementation whose `advance` is
additive; `Monospace` is the test reference and `guimaker_ferrous` supplies
the real one in phase 5. The test module is a table of
`(text, caret, selection, key, modifiers) -> expected` for every shortcut,
plus pointer sequences, IME, undo coalescing and clipboard cases.

## Tests and benches

- `cargo test -p guimaker input::` — 73 unit tests in the input modules.
- `cargo test -p guimaker --test alloc` — 0 allocations in steady state
  (frames and input).
- `cargo bench -p guimaker --bench input` — routing at 100/1 000/10 000 nodes.
- `cargo bench -p guimaker --bench controllers` — scroll, virtual list, tree.
- Motioner's `scripts/perf_bench.py` feeds at least 30 input instants per
  measured run (`MIN_INPUT_SAMPLES`) and reports p50/p99; a run below that
  prints a FINDING and fails the input-to-present gate.
