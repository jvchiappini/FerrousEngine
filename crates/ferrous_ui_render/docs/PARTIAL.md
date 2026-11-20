# Persistent UI target with damaged-rect copy

Prompt 18 P5, item 3 asks one question with numbers: **does rendering the UI
once into a persistent off-screen target and copying only the damaged
rectangle to the present target beat a full redraw?**

This note is the experiment's contract, the wiring plan, and the criterion used
to keep or reject the idea. Implementation: `crates/ferrous_ui_render/src/persist.rs`
(`PersistentUiTarget`). Measurement: `crates/ferrous_renderer/examples/partial_redraw.rs`.

## What it is

`PersistentUiTarget` owns one **single-sample** colour texture and view at the
surface format and size. It does not render anything itself; the GUI pass
attaches to `PersistentUiTarget::view()` exactly as it attaches to the surface
today.

| Method | Contract |
|---|---|
| `ensure(device, width, height, format) -> bool` | Creates the texture on first use and recreates it on a size or format change. Returns `true` when it recreated, which also invalidates the whole target. A zero width/height frees the texture and returns `false`; nothing panics. |
| `invalidate(rect)` | Adds a pixel rectangle (top-left, Y-down). Rounded **out** (floor origin, ceil far edge) and clipped to the target. Zero-sized, negative, NaN or fully off-target rectangles are dropped. |
| `invalidate_all()` | Marks the whole target (first frame, resize, format change). |
| `take_damage() -> Vec<Rect>` | Merges overlapping **and edge-touching** rectangles, clips, drains the accumulator, and returns them in pixels. The merged set stays available through `pending_damage()` for `copy_damage`. |
| `damage_area() -> u64` | Pixel area covered by the last `take_damage` result, i.e. the area `copy_damage` will move. `0` before the first call. |
| `damage_fraction(width, height) -> f32` | `damage_area() / (width * height)`, clamped to `0.0..=1.0`; `0.0` for an empty window. Feed it to `frame_trace::record_ui_damage`. |
| `copy_damage(encoder, dst_texture, dst_view) -> DamageCopy` | One `CommandEncoder::copy_texture_to_texture` per damage rect (origin, extent, depth 1). Reports rect count and bytes copied. |
| `view()`, `size()`, `format()`, `full_bytes()` | Inspection; `view()` is `None` until `ensure` succeeds. |

Allocation: steady state is allocation-free. The target keeps three private
vectors (accumulated damage, merge scratch, merged result) that are cleared in
place and retain capacity. The `Vec<Rect>` returned by `take_damage` is the only
per-call allocation; callers that only need the copy can read `pending_damage()`.

Copy alignment: `copy_texture_to_texture` copies **texels**, not bytes. Origin
`x`/`y` do **not** need to be multiples of four, and no 256-byte row padding is
involved (that rule applies to `copy_texture_to_buffer` / `copy_buffer_to_texture`).
Each copied row is the texture's own bytes-per-row (`width * block_copy_size`).
The source texture carries `COPY_SRC`; the destination must carry `COPY_DST`.

## Damaged-rect contract: who fills it, how conservative

1. The painter knows what it repainted. In the engine today the only damage
   source is `GuiBatch::damage_union` (`crates/ferrous_ui_render/src/gui_batch.rs:25`,
   accumulated in `add_damage_rect` at `gui_batch.rs:569-585`) â€” **one**
   union rectangle for the whole UI, not a list.
2. Whoever owns the UI calls `invalidate` with that rectangle (or `invalidate_all`
   when it cannot bound the change).
3. `take_damage` merges and clips. Bounds are always conservative: the copy can
   cover a few pixels more than were drawn, never fewer. A partially covered
   pixel is copied whole.

Consequences:

- A caller that does not track damage must call `invalidate_all()`. That turns
  the path into a full-target copy plus the GUI repaint: strictly worse than
  drawing straight to the surface (the copy is pure extra bandwidth).
- The first frame after creation/resize starts fully invalidated, so the
  persistent texture is seeded by a `Clear` + a full draw before any partial
  copy is trusted.

## When it wins, when it cannot

The persistent path only changes the **UI layer's** cost. It wins when the
damaged area is a small fraction of the window **and** the UI layer repaint is a
meaningful part of the frame. It cannot win when:

- **The damage is the whole window.** The 2-D editor (Motioner) repaints its
  whole chrome every frame, so its damage is the window until P6 introduces a
  real per-panel damage list. A full-window copy adds bandwidth and removes
  nothing.
- **The UI changes globally on every frame** (a blinking caret that forces a
  union across the panel, an animated background, a full-screen gradient): the
  union rectangle is the window again.
- **There is no single-sample present target to copy into.** The engine's 3-D
  path renders the GUI into the 4Ã— MSAA scene attachment
  (`renderer_core.rs:4158-4173`); `copy_texture_to_texture` cannot copy a
  single-sample texture into an MSAA one. Blitting into MSAA or resolving first
  would need an extra pass (see Risks).
- **The copy itself is slow on the target hardware.** The example measures the
  copy so this is not assumed: on tile/integrated GPUs a full-window copy can
  cost as much as the repaint it replaces. That is exactly what the numbers
  decide.

The synthetic measurement bounds the win: it compares the same per-pixel
fragment work at full resolution versus in a 320Ã—180 damage rect plus the copy.
Real UI also pays CPU collection and vertex upload, which the persistent path
does **not** reduce â€” it only reduces fragment/fill work and the number of
pixels written. So the measured speedup is an **upper bound** for a real UI at
that damage size.

## Measurement

```text
cargo run --release -p ferrous_renderer --example partial_redraw
cargo run --release -p ferrous_renderer --example partial_redraw -- --4k
```

For each size (1600Ã—900, and 3840Ã—2160 when the device limit allows or `--4k`
is passed) the example:

- renders a clear + a heavy full-screen fragment pass into a full-size target
  (the full-redraw baseline, `W*H*4` bytes);
- renders the same pass scissored to a 320Ã—180 rect into `PersistentUiTarget`
  and `copy_damage`s that rect to the present target (`dw*dh*4` bytes written,
  the same amount copied);
- reports median GPU ms per path (`TIMESTAMP_QUERY`), the bytes, the speedup and
  an estimated break-even damage fraction.

The 4K case allocates ~33 MB for the present target, ~33 MB for the full target
and ~33 MB for the persistent texture; all are dropped before the next size
starts, so only one resolution is resident. Without `TIMESTAMP_QUERY` the
example prints `unsupported` and exits. Copy timing needs
`TIMESTAMP_QUERY_INSIDE_ENCODERS`; when missing, the copy is still encoded (the
bytes are real) and its ms are printed as `unsupported`.

### Expected numbers (formula, fill-bound)

Let `A` be the damaged pixel area, `W*H` the window, `r` the UI fill rate
(maintained pixels/ms) and `c` the copy rate (bytes/ms). With `bpp = 4`:

```text
full_ms    â‰ˆ (W*H) / r
partial_ms â‰ˆ A / r + (A * bpp) / c
wins when  A / (W*H) < r / (r + bpp*c)        (= the printed break-even)
```

For a 320Ã—180 rect: 1600Ã—900 is 4.0 % of the window, 3840Ã—2160 is 0.69 %. A
fill-bound fragment loop should therefore show roughly a 25Ã— win at 1600Ã—900 and
~145Ã— at 4K *before* the copy is subtracted; the copy is the term that decides
whether the win survives on a given GPU.

## Wiring plan (not applied; `gui_renderer.rs` / `renderer_core.rs` are owned by other lanes)

Current flow, Flat2D fast path (`crates/ferrous_renderer/src/renderer_core.rs:3773-3811`):

1. `gpu_timings.pass("GUI Render Pass")` at 3777.
2. `ui_pass.prepare(device, queue, packet)` at 3779-3780 (uploads quads/text).
3. `ui_pass.execute_timed(...)` at 3783-3792, which forwards to
   `GuiRenderer::render_with_timestamps` (`gui_renderer.rs:1042-1214`) with
   `target_view`, `LoadOp::Clear(clear)` (Flat2D) or `LoadOp::Load` (3-D), and
   the timestamp writes. The GUI pass draws every segment inside
   `batch.damage_union` at `gui_renderer.rs:1079-1213`.
4. `gpu_timings.end_frame(encoder)` at 3808 (and 3768 for the other 2-D path,
   4254 for the 3-D path).

Proposed wiring for the Flat2D path (single-sample surface target):

1. **Own a target.** Add `persist: PersistentUiTarget` next to `ui_pass` in
   `Renderer` (`renderer_core.rs:895` region) and in `UiPass`
   (`passes/ui_pass.rs:16-22`) if the pass is to manage it.
2. **Ensure per frame, before the GUI pass.** In `avatar`/`render`, after the
   output config is known:
   `if persist.ensure(&device, width, height, format) { /* recreated */ }`.
   `true` means "seed": this frame must clear the persistent texture and draw
   the whole batch once. `surface configuration width/height/format` are the
   same values `GuiRenderer::update_output_config` already receives
   (`gui_renderer.rs` around 482-487).
3. **Render into `persist.view()`, not `target_view`.** Replace the
   `GuiRenderer::render_with_timestamps` call at `renderer_core.rs:3783` with
   one whose target is `persist.view()`:
   - frame 0 / after recreate: `LoadOp::Clear(clear)` and a full draw;
   - steady state: `LoadOp::Load` and draw **only** the damaged region. The GUI
     renderer already honours `batch.damage_union` as the base scissor, so the
     same batch can be drawn against the persistent target without changing
     the GUI code; the damage rect passed to `invalidate` is that union.
4. **Damage source.** `persist.invalidate(batch.damage_union)` when it is
   `Some`, `persist.invalidate_all()` when it is `None` (unbounded change). Today
   Motioner's answer is `invalidate_all()` every frame because it repaints the
   whole chrome.
5. **Copy after the GUI pass, before frame end.** Build the damage with
   `let _ = persist.take_damage();` (or read `persist.pending_damage()`), then call
   `persist.copy_damage(encoder, target_texture, target_view)` **after**
   `ui_pass.execute_timed` returns (so the render pass has ended) and **before**
   `gpu_timings.end_frame(encoder)` at `renderer_core.rs:3808`. `target_view`
   must be a single-sample view of a texture created with `COPY_DST`; if the
   swapchain texture does not advertise `COPY_DST`, the surface configuration
   must add it (or the copy must target an intermediate single-sample texture
   that is then blitted to the swapchain).
6. **3-D path.** At `renderer_core.rs:4158-4173` the GUI pass uses
   `scene_view`/`scene_rt` (4Ã— MSAA). A persistent single-sample texture cannot
   be copied into an MSAA attachment; the options are (a) keep the 3-D GUI on
   its current full redraw, or (b) add a full-screen composite pass that samples
   the persistent texture into the MSAA target, which gives up the copy win.
   Either way, do not wire the copy into the MSAA path as-is.

`resize`/format changes are handled by `ensure` returning `true`: clear and draw
fully once, and the next frame is partial again. If a resize happens in the
middle of a frame, treat it as `invalidate_all`.

## Risks

- **Surface usage.** The swapchain texture must have `COPY_DST` for the copy to
  be legal; many surface configs only request `RENDER_ATTACHMENT`. Budget an
  extra intermediate texture + blit if the platform refuses.
- **MSAA.** The 3-D GUI path is MSAA (`renderer_core.rs:4158-4173`); the
  persistent texture is single-sample by design. See step 6.
- **Extra memory.** At 4K the persistent target is ~33 MB. That is acceptable for
  a 2-D editor, measurable against the P3 memory budgets, and freed by
  `ensure(0,0)` / drop.
- **Stale pixels.** If a damage source under-reports, the error persists across
  frames (a full redraw self-heals every frame). `invalidate_all` on any doubt
  is the safety valve; the rect API rounds out, not in.
- **Damage modelling is the real work.** `GuiBatch::damage_union` is a single
  union, so the engine UI's damage collapses to one rectangle as soon as two
  distant widgets change. Per-node damage lists are a P6 change, not this one.

## Keep / reject criterion

Keep the type and wire it into the Flat2D path only if, at the damage size the
editor actually produces (until P6: the whole window), the measured persistent
path is faster **and** the product wants the extra ~33 MB at 4K. At the current
whole-window damage the answer is expected to be **reject the wiring**, while
keeping `PersistentUiTarget` plus the example as the instrument for P6, when a
real per-panel damage source exists. "Reject" is a valid result of this item
provided the measurement above and the break-even formula are recorded.

Decision record: fill in `out/reports/PHASE-18-P5-REPORT.md` with the two size
tables (full ms, partial draw ms, copy ms, break-even %), the adapter name and
`timestamp_period`, and state keep/reject against this criterion.
