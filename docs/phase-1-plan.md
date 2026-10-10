# Phase 1: from fast list to daily driver

Written 2026-10-09, after week one closed the accessibility gate (see `week-one-plan.md`) and
Phase 0 put the build on a pinned Slint fork with CI. Phase 1's goal: **Jeremy can use TwinFM
instead of the Files app for a normal day.** That means the list has to carry real Windows
entries (attributes, types, drives), act on them (file operations, the shell context menu,
drag and drop) and sit inside a shell that feels like a file manager, all without giving back
the numbers the prototype earned: 40,000 entries enumerated in about 50 ms, and the main
thread 16.7% busy while both panes scroll with accessibility on.

Six milestones, in order:

1. **M1, entry model for real use.** Win32 attributes, entry kinds, a Type column, the
   hidden/system filter, multi-row selection, refresh, the drive list.
2. **M2, icons.** Per-type shell icons, fetched and cached off the UI thread.
3. **M3, file operations.** Copy, move, rename, delete and new folder through
   `IFileOperation`, so progress, conflicts, undo and the Recycle Bin behave as in Explorer.
4. **M4, shell context menu.** `IContextMenu` on the selection, hosted from the list.
5. **M5, drag and drop.** Between the panes and to and from Explorer and other apps
   (`IDropTarget` / `DoDragDrop`).
6. **M6, Nemo-style shell.** Toolbar, places sidebar, breadcrumb path bar, tabs, settings, and
   the About dialog that carries the Slint attribution.

**The gate is re-run after every milestone**: `tools/wpr-gate.ps1` elevated at the console,
default features, Narrator off, the same split-and-scroll scenario on `C:\traces\bigdir`. A
milestone is not closed until its run is recorded here next to the week-one baseline
(16.7% busy, accessibility 198 ms, render 948 ms, ListView 357 ms). A regression past 20% busy
blocks the next milestone until it is understood.

The architecture rules from the prototype hold for every milestone: only the UI thread
touches a Slint model; workers wake it with `slint::invoke_from_event_loop`; entries are plain
structs, enumeration is batched with one `row_added` per batch, sort and filter run on a worker
over a snapshot and swap the whole Vec; `row_data` only clones; all display text is formatted
on the worker.

## M1: entry model for real use

### What landed (2026-10-09)

- **Win32 attributes.** `fs/enumerate.rs` reads `MetadataExt::file_attributes()` from
  `DirEntry::metadata()`, which on Windows is served from the FindNextFile data: no extra
  syscall, and a reparse point reports its own attributes (it is not followed). `Entry` keeps
  the raw `attrs: u32` with `is_hidden()`, `is_system()` and `is_dir()` helpers. A side effect:
  junctions and directory symlinks now open like directories; before, std's `is_dir()` said
  false for them.
- **`Kind`** (`Directory`, `File`, `Drive`, `Reparse`) on `Entry`, exposed to Slint as `kind:
  int` in `Row`. Reparse wins over directory for display (a `\u{21aa}` glyph instead of the
  folder triangle; drives get `\u{25a3}`); a reparse directory still navigates on open.
- **Type column.** `AssocQueryStringW(ASSOCF_NOTRUNCATE, ASSOCSTR_FRIENDLYDOCNAME, ".ext")`,
  cached by lowercase extension in a map that lives for one enumeration run on its worker.
  Directories read "File folder", names without an extension "File", unregistered extensions
  "EXT File". Header cell and sort (`SortCol::Type = 3`, type text then natural name).
  Widths: Name flexible, Type 120 px, Size 90 px, Modified 140 px.
- **Hidden/system filter.** `PaneModel` keeps the full listing as `all` and the filtered,
  sorted view as `shown`; `row_count`/`row_data` read `shown`. Per-pane `show_hidden`
  (default off), toggled by **Ctrl+H** for the active pane (Nemo's binding); the status line
  reads "N items (K hidden), enumerated in X ms" while entries are filtered out.
- **Selection.** A `BTreeSet<usize>` of indices into `shown` plus an anchor, UI thread only.
  Click selects only that row and moves the cursor; Ctrl+click toggles; Shift+click selects
  anchor to row; Ctrl+A selects all; Space toggles the cursor row; Shift+Up/Down extend;
  Home/End jump to the first and last row. Arrowing scrolls the list to keep the cursor in view.
  Only rows whose membership changed get `row_changed`. Selection clears on navigate.
  `accessible-item-selected` now reflects selection membership, not the cursor.
  `PaneModel::selected_names()` is there for M3 to M5.
- **Refresh**, **F5** and **Ctrl+R**: re-enumerates the current folder and restores the cursor
  row and the selection by name where the names still exist, and the list's scroll offset
  (`content-y`, through new `get-scroll`/`set-scroll` functions) where the cursor still
  exists. Directory watching is not in M1.
- **Drive list.** An empty path or "This PC" (any case) lists `GetLogicalDrives()` on the
  enumeration worker, through the same `Msg` path: name "C:\", kind Drive, type
  "Label (C:)" when the volume has a label or else the `GetDriveTypeW` name ("Local Disk",
  "Removable Disk", "Network Drive", "CD Drive", "RAM Disk"), size column "N GB free",
  modified blank. Opening a drive navigates to it; going up from a drive root lands on
  This PC.
- **Driver.** `src/auto.rs` gained `key` names `CtrlH`, `CtrlR`, `F5`, `CtrlA`, `Space`,
  `ShiftUp`, `ShiftDown`, and the steps `click`, `ctrlclick`, `shiftclick <pane> <row>`.
  stderr has new `[select]`, `[hidden]` and `[refresh]` lines, and `[sort]` reports the
  restored cursor and selection count.
- **Tests**, 5 to 15: attribute decoding from a temp dir with hidden and system files set by
  `SetFileAttributesW`; type names and their cache; kind from attributes; the filter pass
  with hidden/system off and on, and per-batch filtering; sort by Type; selection gestures;
  selection mirroring and clear-on-navigate; restore by name over a relisting and a re-sort;
  refresh staging; the drive list contains C:\ as a Drive with free space.
- Every `Weak<App>` site in `main.rs` and `auto.rs` now upgrades with an early return;
  no `unwrap` on the window handle remains.

### Decisions

- F5 is refresh, Nemo's binding (Ctrl+R too). Copy and move to the other pane will use
  Shift+F5 and Shift+F6 in M3, because F5 cannot be both.
- Selection lookup: membership is mirrored into a `selected: bool` on each `Entry` in
  `shown`, beside the `BTreeSet<usize>`, so `row_data` stays a plain clone with no set lookup.
  The set drives ranges, `selected_names()` and the changed-row diff; `PaneModel::select` and
  the pass swap are the only writers, and they keep the two in step.
- Filter during enumeration: each batch moves into `all`, and `shown` is extended with clones
  of the entries that pass the filter (an iterator filter, no temporary Vec), with one
  `row_added` per batch for the visible part. After Done one worker pass filters and sorts a
  snapshot of `all`, as the sort did before.
- One worker pass does filter, sort and restore. Cursor and selection are captured by name
  when the pass is requested, so they follow their files across a re-sort, a Ctrl+H toggle or
  a refresh. If the selection changed while the pass was on the worker (a `sel_version`
  counter), the UI thread re-derives it from the current selection at swap time.
- Refresh re-enumerates in place: batches are staged and the old rows stay on screen until
  the new listing is filtered and sorted, so F5 on a 40k folder does not flash empty.
- Plain Up/Down move the cursor and select only that row (Explorer and Nemo behaviour). The
  cursor is drawn as an outline so it stays visible on selected rows; rows select on mouse
  down.
- Ctrl+H's one switch covers hidden and system entries alike; Explorer's separate "protected
  operating system files" switch can come with settings in M6.
- The Slint `Row` field is `type-text` (`type_text` in Rust), not `type`, to keep a Rust
  keyword out of the generated code.
- Network drives skip the label and free-space queries, which can stall for seconds on a
  disconnected share; their type reads "Network Drive" and the size column stays blank.
- Startup puts keyboard focus in the left list, so keys work before the first click (they
  did not before: focus sat on the window's outer scope).
- Modified times are formatted without syscalls: `LocalZone` reads the zone's bias and
  "week N of month" DST rules from `GetTimeZoneInformation` once per enumeration, and each
  entry is integer arithmetic (Hinnant's civil-date algorithm) plus a hand-written
  `YYYY-MM-DD HH:MM` straight into the SharedString. The per-entry `FileTimeToSystemTime` +
  `SystemTimeToTzSpecificLocalTime` + `format!` cost about 1.5 us; the 40k worker went from
  95 to 134 ms to 55 to 75 ms (formatting stubbed out: 52 to 64 ms). Tests sweep 2024 to 2027
  every 7 minutes, plus each transition, against the Win32 path for a US, a no-DST and a
  southern-hemisphere zone, with no mismatches. Absolute-date rules (`wYear != 0`) still go
  through Win32.

### Deviations

- `type-text` instead of `type` in `Row` (above).
- Selection keeps both the set and a mirrored bool rather than one of the two.

### Deferred

- Arrow keys still do not scroll the list to keep the cursor in view (unchanged from the
  prototype); Shift+Down past the bottom edge selects rows the user cannot see. Small, but
  needed before M3.
- The status line does not show a selection count.
- Reparse points are not told apart by tag: OneDrive placeholder files show the reparse glyph
  like junctions do. std does not expose the reparse tag; reading it means a
  `FindFirstFileExW` loop of our own (`dwReserved0`), worth doing with M2's icon work.
- Free space for network drives, and drive icons (M2).
- Narrator check that the selected state is announced (needs Jeremy at the console).

### Observations

- The first friendly-name lookup in a process costs about 40 ms on the enumeration worker
  (shell association load, `C:\` with a dozen extensions: 44 ms worker against 0.5 ms with
  the lookup stubbed out). Later lookups are cached per run. No COM initialisation is needed.
- 40k enumeration (`C:\traces\bigdir`) measured 70 to 107 ms worker time in this RDP session,
  with or without the type lookup (checked by stubbing it out), against the prototype's
  27 to 37 ms. The type column is not the cause. The per-entry local-time conversion added in
  #7 (`FileTimeToSystemTime` + `SystemTimeToTzSpecificLocalTime` + `format!`) is: with it
  stubbed out the same runs took 39 to 58 ms. It runs on the worker, so the UI thread does
  not pay it, and criterion 4 (under 100 ms to first screen, under 2 s complete) still holds;
  a cheaper conversion (compute the zone offset once per run) is a candidate for a later pass.
- In-app, RDP session, split-and-scroll scenario on bigdir: second pane rendered 23.9 ms after
  F3, UI thread 12.0% busy over the 10 s after the split. Not comparable with console WPR.

Gate (2026-10-10, Jeremy at the console, `tools/wpr-gate.ps1`, accessibility on, hidden off,
three builds back to back in one session): main thread busy over the 10 s after the split by
WPR sampling, week-one main **16.5%**, M1 **19.9%**, M2 **22.3%** (in-app 18.3 / 19.7 / 23.6).
M1 passes the 20% bar with 0.1 point to spare. Its 350 ms delta over main is text: the Type
column is a fifth `Text` per row (draw_text 499 to 677 ms), plus the row's extra properties
(ensure_updated_listview 333 to 412, Property::get 231 to 302). Accessibility is unchanged
(199 to 224 ms). Traces: `C:	races	winfm-p1-{main,m1,m2}.etl`, symbolized reports under
`C:	races\export	winfm-p1-*f-main-10s-sym2.html` (symbolize with the build's own
`target-*elease` directory on `_NT_SYMBOL_PATH`; the pdb path embedded in the exe is not
searched when that variable is set). M2's failure and the options are in the M2 section.
