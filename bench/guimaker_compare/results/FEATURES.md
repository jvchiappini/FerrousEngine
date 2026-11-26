# Feature table (Phase 9C)

`measured` means the bench exercised it. `library` means the capability comes
from the library/its tests; the start screen (Latin text, one monitor) did not
exercise it. The bench now has the **same editable text input and hoverable
button** in every app.

| Feature | GUIMaker | egui 0.29 | iced 0.13 | Slint 1.7 | ImGui 0.32 (imgui-rs 0.12) | WebView2 (wry + winit) |
|---|---|---|---|---|---|---|
| CJK with no setup | yes (composite fallback atlas; `fallback_draw`) | no (needs a font) | no (Fira Sans) | system fallback (fontdb) | no (glyph ranges) | yes (Chromium/system) |
| Emoji with no setup | yes (fallback atlas) | yes (bundled emoji) | no | system fallback | no | yes |
| Popups outside the window | yes (layered popup windows, G-0010) | yes (viewports) | yes (`window::open`) | yes (`PopupWindow`) | no by default | yes (browser windows) |
| Idle CPU (measured) | 0.681 % / 0.218 % | 0.000 / 0.055 % | 0.977 / 0.327 % | 0.342 / 0.055 % | 0.977 / 0.272 % | 0.347 / 0.274 % |
| Idle frames (measured) | **1** | 4 | 1 | 1 | 1 | 346 |
| Per-monitor DPI | yes (`on_scale_factor_changed`, popup scale) | yes (`pixels_per_point`) | yes (winit) | yes (`ScaleFactor`) | partial (`DisplayFramebufferScale`) | yes |
| IME | yes (`TextEditor`) | yes (winit) | yes (winit) | yes | partial (`io.set_ime`) | yes |
| Editable bench input | yes (field + `GuiApp::on_text`) | yes (`TextEdit`) | yes (`text_input`) | yes (`TextInput`) | yes (`InputText`) | yes (`<input>`) |
| Button hover | yes (hover background) | yes | yes | yes | yes | yes (CSS) |

Notes:

- WebView2 now builds: `wry 0.57` with `default-features = false,
  features = ["os-webview"]` and **winit** instead of tao; the `gdk-sys`
  `links` conflict is gone. It spawns Chromium child processes, so its RSS
  (24.8 MB) is not comparable with the in-process libraries.
- The tracking of the bench strip in the apps: input x=40..360, button
  x=380..520, surface centre at `window.logical_height - 58`.
- Typing: the native text inputs received 50 characters in the harness; the
  GUIMaker field focus did not land (see `MEASURED.md`).
