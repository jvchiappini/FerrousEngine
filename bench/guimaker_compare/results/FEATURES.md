# Feature table (Phase 9B)

`measured` means the start-screen app exercised the row in this run. `library`
means the capability comes from the library's own implementation/tests, but the
start screen (Latin text, no IME, one monitor) did not exercise it. A blank cell
was not checked.

| Feature | GUIMaker | egui 0.29 | iced 0.13 | Slint 1.7 | ImGui 0.32 (imgui-rs 0.12) | WebView2 |
|---|---|---|---|---|---|---|
| CJK with no setup | yes (composite fallback atlas; `fallback_draw` tests) | no (bundled font lacks CJK; needs a font) | no (Fira Sans) | system fallback (fontdb) | no (glyph ranges must be loaded) | yes (Chromium/system fonts) |
| Emoji with no setup | yes (fallback atlas) | yes (bundled emoji font) | no | system fallback | no | yes |
| Popups outside the window | yes (layered popup windows, G-0010) | yes (viewports, separate OS windows) | yes (`window::open`) | yes (`PopupWindow`) | no by default (single viewport) | yes (browser windows) |
| Idle CPU | measured 1.220 % (6 s) / 0.376 % (30 s) | measured 0.326 % / 0.054 % | measured 1.008 % / 0.163 % | measured 0.000 % / 0.109 % | measured 1.340 % / 0.217 % | not built |
| Idle frames | measured 3 (first, timer, close) | measured 5 | measured 1 | measured 1 | measured 1 | not built |
| Per-monitor DPI | yes (`on_scale_factor_changed`, popup own scale) | yes (`pixels_per_point` per viewport) | yes (winit) | yes (`ScaleFactor`) | partial (`DisplayFramebufferScale` is global) | yes |
| IME | yes (`TextEditor` IME) | yes (winit IME) | yes (winit IME) | yes (Slint IME) | partial (`io.set_ime`) | yes |

Notes:

- The start screen has read-only fields, so **typing** is not a meaningful row
  for it; the hover/key sweeps measure pointer and key dispatch, not text
  editing. A text-input variant is a follow-up.
- ImGui's CJK/emoji require loading a font with explicit glyph ranges; its
  popups are in-viewport unless the multi-viewport branch is enabled.
- WebView2 was not built: `wry 0.57` + `tao 0.38` fail Cargo's `gdk-sys`
  `links = "gdk-3"` conflict during resolution (a Linux-only dependency graph
  issue). It needs a different version pairing or a winit-based rewrite.
