# Screenshots and the lint loop

The gallery renders a story headlessly through the real GUI renderer, writes a
PNG, compares it to a committed tolerance golden and runs the design lint. Those
commands are the loop: **write -> screenshot -> lint -> fix**.

## The commands

```text
cargo run -p guimaker_gallery -- stories
cargo run -p guimaker_gallery -- shot <story> [--size 1280x800] [--scale 1.5] \
    [--theme light|dark|high-contrast] [--out out/x.png] [--check golden.png]
cargo run -p guimaker_gallery -- golden <story> [--size ...] [--scale ...] \
    [--theme ...] [--update --reason "..."] [--out goldens/x.png]
cargo run -p guimaker_gallery -- lint <story> [--sweep] [--size ...] \
    [--scale ...] [--theme ...] [--json]
```

`stories` lists every story; the name in the components page is the value for
`<story>`. `shot` writes the PNG and prints its absolute path. `golden` compares
against the committed file, or rewrites it when `--update` is paired with a
non-empty `--reason`. `lint` prints the design findings and exits non-zero when
a blocking finding exists; `--sweep` runs the ten-width, four-scale sweep;
`--json` prints one JSON object per finding.

Batch commands exist for the whole registry:

```text
cargo run -p guimaker_gallery -- shots --out out/shots
cargo run -p guimaker_gallery -- goldens --reason "why every golden changed"
```

`shots` writes light and dark renders for every story at `--size` and `--scale`;
`goldens` rewrites the same set and appends one line per file to
`goldens/UPDATE_LOG.md`. The batch file names are the story CLI names, so the
two names shared by a recipe and a component (`empty-state`, `command-palette`)
collide: the recipe render wins. Use `shot ... --out <path>` or the golden suite
when those two component images matter. For the two recipe goldens, pass
`--out crates/guimaker_gallery/goldens/<name>-screen-<theme>-1280x800-1.5.png`
so the comparison uses the recipe file.

The physical PNG is `round(logical * scale)` per axis. At `1280x800` and scale
`1.5` it is 1920x1200. `shot`, `golden` and `lint` default to logical size
`1280x800`, scale `1`, theme `dark`; `shots` and `goldens` default to scale
`1.5`.

## Tolerance

A comparison uses `Tolerance { max_channel_delta, max_diff_pixels }`:

| Field | Value |
|---|---|
| `max_channel_delta` | `12` 8-bit steps, per R/G/B/A channel |
| `max_diff_pixels` | `ceil(0.002 * pixels)`, floor `1` (0.2 %) |
| for 1920x1200 | `4608` pixels |

A pixel differs only when its **largest** channel delta exceeds
`max_channel_delta`; the report prints the largest delta, the differing count
and the fraction. A dimension mismatch is an error, never a pass. The golden
test re-renders every committed case (the two boards, the eight recipes
and 50 components, light and dark) and applies the same tolerance; on
a machine without a headless GPU adapter it prints a skip instead of a false
pass.

## The update log

`crates/guimaker_gallery/goldens/UPDATE_LOG.md` records every rewrite as a
timestamp, the file and the reason. `golden --update` and `goldens` refuse an
empty reason. A golden is evidence, not a snapshot to refresh: update it only
after the change is reviewed and the new image is the intended one.

## The loop

1. **Write** the view with tokens and components. Pick the closest recipe
   first; see [design-guide.md](design-guide.md).
2. **Screenshot** it:
   `cargo run -p guimaker_gallery -- shot <story> --out out/check.png`.
   Open the PNG and look at it.
3. **Lint** it:
   `cargo run -p guimaker_gallery -- lint <story> --sweep`.
   Read the finding sentence: it names the path, the numbers, the cause and a
   fix.
4. **Fix** the structure, not the lint. A blocking finding is a layout or
   contrast bug; a warning is a smell worth checking.
5. **Compare** the golden:
   `cargo run -p guimaker_gallery -- golden <story>`.
   If the difference is intentional, update with a real reason; otherwise fix
   the regression.
6. Repeat at the narrow width (`--size 360x800`) and the wide width
   (`--size 1920x1080`) before calling the screen done.

The full command reference, input mapping and render pipeline are in
[guimaker_gallery/gallery.md](../guimaker_gallery/gallery.md). Goldens live in
`crates/guimaker_gallery/goldens/`.
