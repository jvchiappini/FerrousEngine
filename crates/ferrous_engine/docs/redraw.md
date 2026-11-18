# Redraw policy: continuous or on demand

An application tells the runner when it needs frames. The choice is an
`AppConfig` field:

| Policy | Behaviour | Use it for |
|---|---|---|
| `RedrawPolicy::Continuous` | Draw at `target_fps` (or uncapped when `None`). The historical loop. | Games, demos and anything that animates every frame. |
| `RedrawPolicy::OnDemand` | Draw only when there is a reason. Idle apps present no frames, submit no GPU work and consume no CPU. | Editors, tools and desktop apps. |

`Continuous` is the default so existing apps keep working. An editor opts
in once:

```rust,ignore
App::new(MyEditor)
    .with_mode(AppMode::Flat2D)
    .with_redraw_policy(RedrawPolicy::OnDemand)
    .run();
```

## What schedules a frame in `OnDemand`

A frame is requested when any of these happens:

1. **Input** arrives: a key, a mouse button, the wheel, a resize, a scale or
   theme change, a file drag. One event batch produces one frame.
2. The app calls **`ctx.request_redraw()`** from `setup`, `update` or
   `draw_ui`.
3. A **scheduled wake** fires. The app calls **`ctx.redraw_at(instant)`** with
   an absolute `std::time::Instant`.
4. A **worker** calls **`waker.wake()`** on the handle obtained from
   `ctx.waker()`.
5. The OS asks for a repaint (the window was uncovered) or the window is
   resized.

When none of these is pending the loop sleeps in `ControlFlow::Wait` (or
`ControlFlow::WaitUntil` for the next wake) and the process is idle.

## Animating

There is no clock tick. An animation asks for the next frame from every frame
it wants to draw:

```rust,ignore
fn update(&mut self, ctx: &mut AppContext) {
    if self.transport.is_playing() {
        self.advance(ctx.time.delta);
        ctx.request_redraw(); // keep the animation running
    }
}
```

The display's vsync (`Fifo` present mode) paces those frames; there is no
need for a `target_fps` cap. `Continuous` apps keep the older
`idle_timeout`/`idle_fps` throttle; both fields are ignored in `OnDemand`.

## Timers without a ticking loop

Use `redraw_at` for anything that must happen at a known instant:

```rust,ignore
// Show the tooltip 450 ms after the pointer settled.
ctx.redraw_at(self.hover_since + Duration::from_millis(450));

// Write the debounced document 300 ms after the last edit.
ctx.redraw_at(self.last_edit + Duration::from_millis(300));
```

The wake presents exactly one frame at (or after) that instant. Calling it
several times keeps the earliest deadline. It is a one-shot: an app that is
still animating calls `request_redraw()` from that frame.

## Waking from a worker

`setup` receives a `Waker`. Clone it into every background job that produces
something the UI must install (a font atlas, a decoded image, a video frame,
the audio device opening, a control request):

```rust,ignore
fn setup(&mut self, ctx: &mut AppContext) {
    let waker = ctx.waker().clone();
    self.assets.start(waker); // the worker calls waker.wake() when done
}

fn update(&mut self, ctx: &mut AppContext) {
    if self.assets.poll() {
        ctx.request_redraw(); // the frame that installs the result
    }
}
```

`Waker::wake()` is `Send + Sync`, cheap, and a no-op once the loop is gone,
so a worker that finishes after the window closed never panics. A worker that
finishes while the app sleeps is the reason the app wakes up: without the
waker an async result would wait for the next mouse move.

## Idle workers

A worker owned by an on-demand app must block on a channel (or a timeout on a
condition variable), not poll at frame rate. The worker wakes the UI with the
`Waker` when it has something; the UI does not wake the worker on a clock.
The one exception is a file watcher, which by nature observes the filesystem
and sleeps between stats.

## Measuring

`FERROUS_TRACE_FRAMES=1` reports the stage medians and the frame count. In
`OnDemand`, a static window over ten seconds must report zero frames; the
memory report and the GPU timestamps stay empty because nothing was
submitted. `ferrous_core::frame_trace` measures only real event callbacks:
`RedrawRequested` carries the frame itself, so it is intentionally not
counted as event handling.
