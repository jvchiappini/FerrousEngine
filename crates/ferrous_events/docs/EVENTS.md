# ferrous_events — reference

The crate stops at the platform boundary. It answers three questions for a
consumer: *what happened*, *when*, and *what are the platform's timing rules*.
Everything policy-shaped (what a click means, who gets the event, what is
hovered) lives in `guimaker::input`.

## Events

```text
RawInput
├── PointerMoved      { position, time_ms }
├── PointerButton     { position, button, state, modifiers, time_ms }
├── PointerWheel      { position, delta, kind, modifiers, time_ms }
├── PointerEntered    { position, time_ms }
├── PointerLeft
├── ModifiersChanged(Modifiers)
├── Key               { code, state, repeat, modifiers, text, time_ms }
├── Text(String)                     committed text (typing, paste, IME commit)
├── Ime(Ime)                         Composition / Commit / Cancel
├── Clipboard(Clipboard)             Copy / Cut / Paste(text)
├── FileDrop(FileDrop)               Entered / Hovered / Dropped / Left
└── FocusChanged(bool)
```

- `position` is in **logical** pixels; the host applies DPI and canvas scaling
  before feeding the event.
- `PointerWheel.delta` follows the platform convention: positive `y` scrolls
  up (the wheel moves away from the user). `ScrollKind::Line` is a wheel
  notch (typically 1.0); `Pixel` is a precision touchpad or high-resolution
  wheel.
- `Key.text` is the text the platform produced for that key, when it produced
  any; it is not a substitute for `Text` (IME and paste arrive there).
- `RawInput::time_ms`, `position()` and `modifiers()` return `None` for the
  variants that do not carry the field.

## Platform parameters

`InputConfig::default()` and `platform_default()` return 500 ms for the
double-click window, 4 logical px for the double-click distance and 3 logical
px for the drag threshold. A host that reads the real system values replaces
them; a test can use anything.

## The clock

Nothing in this crate calls `Instant::now()` behind the caller's back.

```rust
use ferrous_events::{Clock, ManualClock, SystemClock};

let mut manual = ManualClock::at(0.0);
manual.advance(16.0);
assert_eq!(manual.now_ms(), 16.0);

let system = SystemClock::new();
assert!(system.now_ms() >= 0.0);
```

A host passes its clock reading into the events (the `time_ms` field) and
into `guimaker::Ui::set_time_ms`. A test uses `ManualClock` and advances it
explicitly, which is what makes the gesture and text-editor tests
deterministic.

## What was removed in phase 4

`EventManager` (hovered/focused/pressed state, hit testing and bubbling) is
gone, together with the linear walk that was marked
`TODO(prompt 15, phase 4)`. The retained UI system (`ferrous_gui::UiSystem`)
now only lays out and paints; its widgets no longer receive pointer or key
events. Routing, gestures, focus and shortcuts are GUIMaker's, fed by the
raw events above.
