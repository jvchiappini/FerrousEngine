# Backdrop blur

The GUI can blur whatever is painted under a rounded rect (a modal card, a
popover). The pipeline lives in `src/pipelines/blur.rs` and
`assets/shaders/backdrop_blur.wgsl`; the targets and sequencing live in
`GuiRenderer::set_backdrop_source`, `prepare_segments` and
`encode_backdrop_blur`.

## Why it is not a single nine-tap pass

A single separable pass that samples the **full-resolution** scene at taps
spaced by the radius is an under-sampled convolution: a 1 px line or a text
baseline is point-sampled once per tap, so it comes out as several sharp
copies at the tap spacing instead of one smooth peak. The parser cannot tell
that apart from a "look", so it is a rendering bug.

## The chosen kernel

1. **Prefilter.** The scene is box-filtered down a 2x chain (level 1 = 1/2,
   ..., level 5 = 1/32 per axis). Each `fs_downsample` pass averages the four
   source texels under the destination texel, so content above the level's
   Nyquist limit is removed before it can alias.
2. **Level matching.** The requested radius `r` maps to a Gaussian with
   `sigma = r / 2.5` physical pixels. The Gaussian runs at
   `level = floor(log2(sigma))`, clamped to `1..=5`, so its sigma is between
   one and two **destination** texels. That keeps the kernel at most 13 taps.
3. **Separable Gaussian.** `fs_blur` runs twice (horizontal, then vertical)
   with taps exactly one destination texel apart. The weights are computed on
   the CPU for the exact sigma (`BlurParams::gaussian_weights`) and uploaded,
   so the impulse response has the requested width; nothing is approximated by
   a fixed kernel.
4. **Upsample.** The GUI quad samples the final level with a linear sampler, so
   the result is bilinearly upsampled for free.

The full width at half maximum of the result is `2.355 * r / 2.5 = 0.94 * r`,
i.e. within 6 % of the requested radius.

## Cost

`cargo run --release -p guimaker_gallery --example blur_timing` renders the
dialog story at 1600x900 @1.5 with and without the `BackdropBlur` command, 200
submissions per variant, one GPU wait. Measured on the reference machine
(RTX 4050 Laptop):

| run | with blur | without | blur cost |
|---|---|---|---|
| 1 | 0.4805 ms | 0.4054 ms | 0.0751 ms |
| 2 | 0.4269 ms | 0.3959 ms | 0.0310 ms |
| 3 | 0.3885 ms | 0.3298 ms | 0.0587 ms |

Median blur cost **0.059 ms/frame** (upper bound: the blurred variant also
encodes one extra pass). The chain is bounded by the scene size (1/3 extra
texels for the whole chain) and the Gaussian runs at most 13 taps at 1/32
resolution, so the cost is independent of the radius once the level saturates.

## Guards

`crates/guimaker_gallery/tests/mechanics.rs`:

- `the_dialog_card_blurs_the_content_below` compares every interior pixel not
  covered by a child against a CPU reference of the same chain and kernel.
- `the_dialog_backdrop_does_not_sample_the_font_atlas` renders the story with a
  solid atlas and checks the card only moves by the amount the blurred scene
  moved (an engine that sampled the atlas directly would move it much more).
- `a_one_pixel_line_blurs_to_one_peak` draws a 1 px horizontal and a 1 px
  vertical line behind a backdrop card and asserts: one peak, a monotone fall
  to both sides within ±1/255, no secondary peaks, a full width at half maximum
  within 20 % of the radius (16.00 px for a 16 px radius), and the GPU profile
  within 4/255 of the CPU reference.
