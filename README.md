# rustfm-proto

One-week prototype answering one question: can a Rust + Slint dual-pane file manager on
Windows 11 beat the Files app (files-community/Files, WinUI 3) on the things that hurt,
namely split-pane cost, scroll smoothness, UI-thread load and crashes on 40,000-entry folders?

The verdict and the measured numbers are in [PROTOTYPE-RESULT.md](PROTOTYPE-RESULT.md).

## Build and run

Stable Rust with the MSVC toolchain (`x86_64-pc-windows-msvc`) and the Visual Studio
Build Tools "Desktop development with C++" workload.

```
cargo build --release
target\release\rustfm-proto.exe C:\traces\bigdir
```

The one argument is the starting folder for the left pane (default `C:\`).

## Keys

| Key | Action |
|---|---|
| F3 | Toggle the second pane (opens on the left pane's folder) |
| Ctrl+Shift+Right | Focus the other pane |
| Double-click, Return | Enter a directory |
| Backspace, Alt+Up | Go to the parent directory |
| Up / Down | Move the cursor |
| Click a column header | Sort by name, size or modified (click again to reverse) |
| Type a path in the path bar and press Return | Navigate |

Drag the bar between the panes to resize them.

## Layout

```
src/
  main.rs          Slint bootstrap, wires model <-> UI, owns the two panes
  fs/enumerate.rs  Worker thread: read_dir -> Vec<Entry> batches of 2000 -> channel
  model.rs         PaneModel: Vec<Entry>, sort state; implements slint::Model
  ui/app.slint     Window, two Pane components, splitter
  ui/pane.slint    Path bar, column header, ListView over the model, status line
  perf.rs          Frame-time ring buffer, UI-thread CPU time, printed to stderr every 5 s
  auto.rs          Scripted driver for unattended runs (RUSTFM_SCRIPT, see its header)
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

Run with `RUSTFM_SCRIPT` set to a scenario (or `@file`) and read stderr. Each 5 s the
`[perf]` line reports render-pass time, frame interval and UI-thread CPU per frame; the
`[enum]`, `[split]`, `[sort]` and `[busy]` lines carry the one-shot timers. Example:

```
set RUSTFM_SCRIPT=sleep 1500; split; wheel 1 -100 16; wheel 0 -100 16; sleep 8000; quit
target\release\rustfm-proto.exe C:\traces\bigdir
```

For a WPR trace run `tools\wpr-gate.ps1` from an elevated PowerShell at the physical
console. Over RDP the Remote Display Adapter presents at 32 Hz and the frame cadence will
sit near 21 fps whatever the app does.
