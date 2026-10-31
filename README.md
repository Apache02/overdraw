# overdraw

Draw on top of the screen during presentations, demos and screen recordings.
A Linux take on [ScreenInk](https://github.com/volyx/ScreenInk) for X11, written in Rust + GTK3.

## Building

```bash
sudo apt install libgtk-3-dev libxapp1
cargo build --release
cargo test
```

The tray icon uses libxapp's XAppStatusIcon, so it shows up in Cinnamon's
XApp Status Applet next to other apps (libxapp falls back to a classic tray icon
elsewhere). Linux Mint ships `libxapp1`; only the runtime library is needed.

Requires Rust 1.84 or newer. The session must be X11 with a compositor running
(Cinnamon has one; on MATE or XFCE enable compositing in the window manager).

## Running

```bash
./target/release/overdraw            # starts in the tray
./target/release/overdraw --toggle   # show/hide the canvas
```

overdraw runs as a single instance: running `overdraw` again (or
`overdraw --toggle`) just toggles the canvas in the running copy.
Other options: `--show`, `--hide`, `--quit`.

### Global hotkey

Cinnamon: *System Settings → Keyboard → Shortcuts → Add custom shortcut*,
command `overdraw --toggle`, keys e.g. <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>A</kbd>.

### Install and autostart

```bash
install -Dm755 target/release/overdraw ~/.local/bin/overdraw
install -Dm644 data/icons/overdraw-rs.svg ~/.local/share/icons/hicolor/scalable/apps/overdraw-rs.svg
install -Dm644 data/overdraw.desktop ~/.local/share/applications/overdraw.desktop
install -Dm644 data/overdraw.desktop ~/.config/autostart/overdraw.desktop   # autostart
```

## Shortcuts while drawing

| Key | Action |
|---|---|
| P / H / L / A | Pen / Highlighter / Line / Arrow |
| R / O / T / N | Rectangle / Ellipse / Text / Numbered badge |
| E | Eraser (removes whole shapes) |
| 1–9 | Color |
| [ / ] | Thinner / Thicker |
| C | Clear all |
| S | Screenshot with the drawing: copied to the clipboard, then a Save dialog opens |
| Ctrl+Z / Ctrl+Shift+Z (Ctrl+Y) | Undo / Redo |
| Esc | Cancel the current gesture, or hide the canvas |
| Hold Shift | Constrain lines to 45°, shapes to squares and circles |
| Hold Alt with the pen | Temporary highlighter |
| Ctrl+Q | Quit |

Text: click with the Text tool and type; Enter or Esc finishes.

The palette can be dragged by the ⠿ grip on its left.

While the canvas is shown the screen under it is dimmed (30% by default).
Pick the level, or turn it off, under **Dim Screen** in the tray icon's menu.

## Layout

- `crates/overdraw-core` — shapes, geometry, document with undo/redo;
  no GTK dependency, unit-tested.
- `crates/overdraw` — the GTK3 app: transparent always-on-top window (`ui.rs`),
  Cairo rendering (`render.rs`), gesture logic (`state.rs`).

## Not yet implemented (compared to ScreenInk)

Selection tool (move, recolor), curved arrows, auto-fading drawings,
a separate canvas per monitor (the canvas currently covers the
primary monitor), Wayland.

## License

[MIT](LICENSE)
