# ferrous_events

Raw input at the operating-system boundary.

The crate describes what the platform delivered and when: pointer moves,
button transitions, wheel and touchpad deltas, keys with their text, IME
compositions, clipboard payloads, file drops, the modifier state and window
focus. It also carries the platform double-click time and distance and an
injected `Clock`.

It deliberately contains **no routing, hit testing, hover/focus state or
gesture recognition**. GUIMaker owns that (`guimaker::input`), and the legacy
retained UI system only lays out and paints. This is the phase 4 ownership
split of prompt 15 and the reason `ferrous_events` no longer depends on
`ferrous_ui_core`.

## The surface

| Item | Purpose |
|---|---|
| `RawInput` | One tagged platform event (`PointerMoved`, `PointerButton`, `PointerWheel`, `PointerLeft`, `PointerEntered`, `ModifiersChanged`, `Key`, `Text`, `Ime`, `Clipboard`, `FileDrop`, `FocusChanged`) |
| `Modifiers` | The four platform modifier keys |
| `PointerButton`, `ButtonState`, `ScrollKind` | Button identity, transition and wheel-unit vocabulary |
| `Ime`, `Clipboard`, `FileDrop` | Text composition, clipboard operation and drag payloads |
| `KeyCode` | Re-export of `winit`'s layout-independent physical key |
| `InputConfig` | Double-click time (500 ms), double-click distance (4 px), drag threshold (3 px) |
| `Clock`, `SystemClock`, `ManualClock` | The injected time source |
| `RawInput::time_ms`, `::position`, `::modifiers` | Accessors for the fields a recognizer needs |

`ManualClock` is what tests use: every stamped event plus a clock the test
moves by hand makes a recorded sequence replay byte for byte.

## What belongs elsewhere

- **Hit testing and routing** — `guimaker::input` (uniform-grid index, hover,
  press, capture, focus).
- **Gestures** — `guimaker::input::gesture` (click counts, long press, drag,
  hover intent).
- **Shortcuts, focus order, scroll momentum, text editing** —
  `guimaker::input`.
- **Engine event conversion** — the runner turns `winit` events into
  `RawInput`; the mapping lives with the runner, not here.

See `EVENTS.md` for the field-by-field reference and the clock contract.
