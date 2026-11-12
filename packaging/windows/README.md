# Windows installer art

`tauri.windows.conf.json` points the NSIS installer at these bitmaps:

| File | Size | Where it shows |
| --- | --- | --- |
| `installer-sidebar.bmp` | 164×314 | Welcome and Finish pages |
| `installer-header.bmp` | 150×57 | top-right of every other page |

The `.svg` files next to them are the designed sources (the mark is `assets/logo.svg`, inlined). NSIS reads only 24-bit BMP: after editing an SVG, render it at exactly that size (e.g. `rsvg-convert` then `magick <png> -alpha off BMP3:<bmp>`) and commit both.

`installer-*.bmp` belong to the plain NSIS installer (`atlas desktop`, `atlas update` and scripts run it silently, so people rarely see them). What people see is `Atlas-Setup.exe` (`crates/atlas-setup`), whose window is drawn in code from `assets/logo-animated.svg`; `installer-hooks.nsh` keeps its uninstaller registered. See `docs/release.md`, "Windows installer".
