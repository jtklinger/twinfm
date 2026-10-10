# twinfm

TwinFM, a dual-pane file manager for Windows 11 in Rust and Slint. Renamed from `rustfm-proto` on 2026-10-09; the prototype record below is unchanged.

One-week prototype answering one question: can a Rust + Slint dual-pane file manager on
Windows 11 beat the Files app (files-community/Files, WinUI 3) on the things that hurt,
namely split-pane cost, scroll smoothness, UI-thread load and crashes on 40,000-entry folders?

The verdict and the measured numbers are in [PROTOTYPE-RESULT.md](PROTOTYPE-RESULT.md).

## Build and run

Stable Rust with the MSVC toolchain (`x86_64-pc-windows-msvc`) and the Visual Studio
Build Tools "Desktop development with C++" workload.

```
cargo build --release
target\release\twinfm.exe C:\traces\bigdir
```

The one argument is the starting folder for the left pane (default `C:\`).

## Keys

| Key | Action |
|---|---|
| F3 | Toggle the second pane (opens on the left pane's folder) |
| Ctrl+Shift+Right | Focus the other pane |
| Double-click, Return | Enter a directory |
| Backspace, Alt+Up | Go to the parent directory (from a drive root: This PC, the drive list) |
| Up / Down | Move the cursor and select that row |
| Click, Ctrl+click, Shift+click | Select a row, toggle a row, select a range |
| Shift+Up / Shift+Down | Extend the selection |
| Space | Toggle the selection of the cursor row |
| Ctrl+A | Select all |
| Ctrl+H | Show or hide hidden and system files in the active pane |
| F5, Ctrl+R | Refresh the active pane (keeps cursor, selection and scroll) |
| Click a column header | Sort by name, type, size or modified (click again to reverse) |
| Type a path in the path bar and press Return | Navigate (empty or "This PC" lists the drives) |

Drag the bar between the panes to resize them.

## Layout

```
src/
  main.rs          Slint bootstrap, wires model <-> UI, owns the two panes
  fs/enumerate.rs  Worker thread: read_dir -> Vec<Entry> batches of 2000 -> channel; type names, drive list
  model.rs         PaneModel: full and filtered Vec<Entry>, selection, sort state; implements slint::Model
  ui/app.slint     Window, two Pane components, splitter
  ui/pane.slint    Path bar, column header, ListView over the model, status line
  perf.rs          Frame-time ring buffer, UI-thread CPU time, printed to stderr every 5 s
  auto.rs          Scripted driver for unattended runs (TWINFM_SCRIPT, see its header)
tools/
  wpr-gate.ps1     The gate scenario under Windows Performance Recorder (run elevated)
  shot.ps1         Capture the window to a PNG (works with SLINT_BACKEND=winit-software)
```

Rules the prototype follows, because they are what makes the numbers possible:

- The list model holds plain structs. No per-row handles, callbacks or allocation after
  enumeration.
- Enumeration streams batches of 2,000 entries over a channel; the UI thread appends a
  batch and fires one `row_added` per batch, never per entry.
- Sorting runs on a worker over a snapshot and swaps the whole Vec in one step.
- Only the UI thread touches the Slint model. Workers wake it with
  `slint::invoke_from_event_loop`.

## Measuring

Run with `TWINFM_SCRIPT` set to a scenario (or `@file`) and read stderr. Each 5 s the
`[perf]` line reports render-pass time, frame interval and UI-thread CPU per frame; the
`[enum]`, `[split]`, `[sort]` and `[busy]` lines carry the one-shot timers. Example:

```
set TWINFM_SCRIPT=sleep 1500; split; wheel 1 -100 16; wheel 0 -100 16; sleep 8000; quit
target\release\twinfm.exe C:\traces\bigdir
```

For a WPR trace run `tools\wpr-gate.ps1` from an elevated PowerShell at the physical
console. Over RDP the Remote Display Adapter presents at 32 Hz and the frame cadence will
sit near 21 fps whatever the app does.

## License

The application's own files are MIT (see `LICENSE`). It links a modified Slint, used under
the [Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0](https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md),
whose section 2 permits distributing Slint "as part of an Application, modified or
unmodified" provided the Slint attribution is shown. This README is that attribution
(option (b) of section 2); the `AboutSlint` widget goes into an About dialog once the
product has menus.

The modifications to Slint themselves are the two commits on the `rustfm` branch of
[jtklinger/slint](https://github.com/jtklinger/slint), also carried as `docs/patches/`.
Slint's licensing FAQ states that modifications may be released under any license of the
modifier's choice, so those two commits are offered under MIT like the rest of this
repository; the Slint code around them keeps Slint's own triple license (GPL-3.0-only,
Royalty-free 2.0, or commercial), and the fork as a standalone copy of Slint is
redistributed under Slint's GPL-3.0 option, as section 3 of the Royalty-free license directs.

[![Made with Slint](https://github.com/slint-ui/slint/raw/master/logo/MadeWithSlint-logo-whitebg.png)](https://slint.dev)
