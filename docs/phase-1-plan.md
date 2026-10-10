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
(199 to 224 ms). Traces: `C:/traces/twinfm-p1-{main,m1,m2}.etl`, symbolized reports under
`C:/traces/export/twinfm-p1-*/bf-main-10s-sym2.html` (symbolize with the build's own
`target-*/release` directory on `_NT_SYMBOL_PATH`; the pdb path embedded in the exe is not
searched when that variable is set). M2's failure and the options are in the M2 section.

## M2: icons

### What landed (2026-10-09)

- **Icon worker** (`src/icons.rs`). One thread, started with the app, `CoInitializeEx`
  multithreaded, owns every shell call. Per request it calls `SHCreateItemFromParsingName`
  for `IShellItemImageFactory` and `GetImage(16 x scale, SIIGBF_ICONONLY |
  SIIGBF_BIGGERSIZEOK)`, reads the HBITMAP top-down with `GetDIBits` (negative height),
  converts premultiplied BGRA to premultiplied RGBA in a `SharedPixelBuffer`, deletes the
  bitmap and sends the buffer back over a crossbeam channel, then wakes the UI thread with
  `invoke_from_event_loop`. COM is plain `windows-sys` with a four-slot vtable struct; the
  `windows` crate was not needed. The worker keeps the set of keys it has delivered and never
  renders one twice per process. New `windows-sys` features: `Win32_System_Com`,
  `Win32_Graphics_Gdi`.
- **Cache keys**, computed on the enumeration worker with no extra syscall for most entries:
  ordinary files `ext:.<lowercase ext>`; plain directories one shared `dir`; `.exe`, `.lnk`,
  `.ico`, `.url`, `.cur`, files with no extension, reparse points and drives by full path;
  customised folders (a `desktop.ini` the shell honours) by full path.
- **Icon table** (`IconTable`, UI thread, one `Rc<RefCell<..>>` shared by both panes):
  `Vec<slint::Image>` whose first three slots are generated 16 x 16 placeholders (file, folder,
  drive), a key-to-index map, and the pending and failed key sets. A reply's pixels become a
  `slint::Image` once per key.
- **Model.** `Entry` gained `icon: u16` (table index, a placeholder until resolved) and
  `icon_key: SharedString`; `Row` gained `icon: image`. `row_data` adds one indexed `Vec`
  access and one refcount bump (`icons.borrow().image(e.icon)`).
- **Visible rows only.** `FileList` reports `(content-y, visible-height)` through a new
  `viewport-changed` callback from `changed content-y`, `changed visible-height` and
  `changed row-count`; Rust also re-resolves after every pass swap and after every batch of
  icon replies. `icons::visible_range` turns that into the row range, and
  `PaneModel::resolve_icons` walks the visible rows plus 10 above and below: a row whose
  key is in the table gets its index and a `row_changed`, a key never seen is requested once,
  pending and failed keys are left alone. Rows already resolved cost one integer compare.
- **UI.** The glyph column (folder triangle, drive and reparse marks) is now a 16 px logical
  `Image` for every kind, in the same 22 px slot; the buffer is 16 x the window scale factor,
  so it stays sharp at 150% and 200%.
- **Cancel by generation.** The UI stores each pane's generation in a shared
  `[AtomicU64; 2]` on navigate and refresh; the worker answers a stale request with
  `Skipped` instead of rendering it, and the UI asks again if the row is still on screen.
- **Driver.** No new steps: the existing `wheel` and `drag` steps move `content-y`, which
  fires `viewport-changed`. stderr has a new `[icons]` line once a second when icon work
  happened: requests and replies in that second and in total, table size, the worker's
  average ms per icon, and the number and average cost of UI-side resolve calls.
- **Tests**, 18 to 25: key rules; the `desktop.ini` rule (needs the file and a read-only or
  system folder); BGRA to RGBA on a 2 x 2 buffer (opaque, premultiplied half alpha,
  transparent, and an all-zero-alpha bitmap treated as opaque); visible-range arithmetic
  including overscroll and past-the-end; the table's intern, dedupe, pending, failed and
  skipped states; `resolve_icons` requesting each key once and applying a reply only to the
  visible rows; and an integration test that starts the worker, asks for
  `C:\Windows\notepad.exe`, `C:\` and `ext:.txt` plus one stale request, and gets three icons
  with visible pixels and one `Skipped` within 5 s (it skips with a message if the shell
  returns nothing at all).

### Decisions

- **`desktop.ini` check on the enumeration worker, filtered to read-only or system folders.**
  The shell only reads `desktop.ini` in folders with one of those attributes, so other
  folders cannot have a custom icon and need no probe. Measured with a standalone timing of
  `Path::join("desktop.ini").exists()`: unfiltered it costs 25 to 155 us per directory,
  1.1 s warm and 4.2 s cold for the 26,867 folders of `C:\Windows\WinSxS` and 4.4 ms for the
  179 folders of System32; filtered, WinSxS has no candidates (0 ms), System32 has 7
  (0.26 ms), `C:\Users\jtkli` 8 (0.3 ms, all 8 custom). `C:\traces\bigdir` and
  `WinSxS\Manifests` have no folders at all. That is under the 5 ms per 40k budget in every
  folder tried, so the lazy variant (all folders `dir`, checked in the icon worker) was not
  needed. A folder of tens of thousands of read-only subfolders would cost seconds on the
  enumeration worker, not on the UI thread.
- **Resolve on the visible range, not by walking the listing.** A reply does not walk
  `shown` or `all`; it re-runs the visible-range resolve for both panes (a screenful plus the
  margin). Rows that scroll into view later resolve from the table then, and a pass swap
  resolves its visible rows before the next frame. Nothing on the UI thread is O(entries):
  no per-entry table lookup at append or pass time. Measured resolve cost: 4 to 10 us
  average per call, 30 to 60 us for the first calls of a listing.
- **The worker serves newest first.** The first version served its queue in order; a fast
  wheel through System32 queued 725 requests (every `.exe` scrolled past) and the rows on
  screen at the end waited about 5 s behind them. With a LIFO queue the rows on screen get
  their icons first and the rest still render, later, into the cache.
- **No `waiting` map.** The UI keeps `pending` (and `failed`) per key; a `Skipped` reply
  clears the pending mark and the next resolve re-requests the key if a row on screen still
  needs it. Requests carry no row index and replies no row list, since the visible-range
  resolve finds the rows itself.
- **Failed keys are not retried** in this process; their rows keep the placeholder.
- **Visible-range arithmetic in Rust** (`icons::visible_range`), fed with `content-y` and
  `visible-height` from Slint, rather than computed in `.slint`, so it is unit-tested and the
  same function serves the pass-swap and reply paths, which have no Slint event.
- **The shared `dir` key renders from the first plain folder requested.** It cannot be a
  customised folder (those have their own key), so it gets the generic folder icon.
- **Scale factor** is read at request time (`window().scale_factor()`), which also covers a
  window that has not been shown yet at startup.

### Deviations

- `Request` is `{ generation, pane, key, path, size_px }` and `Reply` is `{ key, outcome }`
  with `Outcome::{Pixels, Failed, Skipped}`: no `row`/`rows` or `kind` fields (unused, see
  above).
- The UI keeps no `waiting` map (above).
- The `desktop.ini` check runs eagerly, filtered by attribute, rather than lazily.

### Deferred

- **32 px icons.** Only the 16 px logical size is requested; a 32 px fetch comes with a
  large-icon view.
- **Scale-factor changes.** Icons already in the table keep the size they were rendered at
  when the window moves to a monitor with a different scale; the table would need a flush and
  re-request.
- **Overlays.** `SIIGBF_ICONONLY` returns no overlay, so junctions, symlinks and shortcuts
  show no arrow, and the M1 reparse glyph is gone with the glyph column. Reparse points still
  get their own shell icon (keyed by full path), which tells OneDrive and other cloud
  placeholders apart from junctions; telling reparse tags apart remains deferred from M1.
- **One worker, no timeout.** A slow icon (a network path, a cloud placeholder: one entry in
  `C:\Users\jtkli` took 568 ms) delays the icons queued behind it. Only rows on screen are
  requested and newest are served first, so the cost is a late icon, not a stalled UI.
- **Rows scrolled past are still rendered.** A fast scroll queues every key it passes over;
  the worker renders them after the visible ones. Cancelling by row range could save that
  worker time.
- **Narrator check** that the icon images add nothing to the accessibility tree (they have no
  `accessible-role`, so they should not), with Jeremy at the console.

### Measurement

In-app, RDP session, default features, the split-and-scroll script
(`sleep 2500;split;sleep 1500;wheel 0 -40 30;wheel 1 -40 30;sleep 3000;drag 0 300 1000;sleep 2000;quit`),
the M1 build (`feat/m1-entry-model`, built into `target-m1`) and the M2 build run
alternately so both see the same session conditions. UI-thread busy over the 10 s after the
split:

| Folder | M1 runs | M1 mean | M2 runs | M2 mean |
|---|---|---|---|---|
| `C:\traces\bigdir` (40,000 `.txt`) | 8.1, 7.0, 7.0, 8.2, 6.7, 5.3 | 7.1% | 6.1, 6.8, 8.9, 8.2, 8.3, 7.2 | 7.6% |
| `C:\Windows\System32` (4,912) | 9.2, 10.6, 9.8, 7.6 | 9.3% | 10.0, 8.1, 9.8, 10.0 | 9.5% |
| `C:\Windows\WinSxS\Manifests` (35,969 `.manifest`) | 9.7, 9.5, 8.6, 9.2 | 9.3% | 10.9, 7.8, 10.9, 8.4 | 9.5% |

The difference is inside the run-to-run spread (5.3 to 8.9 for the same build and folder).
No panics in any run.

- bigdir: one key (`ext:.txt`), table 4 images (3 placeholders + 1), one render of 60 to
  120 ms (the first shell icon call in a process loads the shell; later calls take 4 to 7 ms).
  The second pane needs no request at all.
- WinSxS\Manifests: one key (`ext:.manifest`), one render of 52 to 59 ms. Every file there
  shares the extension, so it says nothing about throughput.
- System32 is the throughput case: each `.exe` is its own key. The script's scrolling sent
  338 requests and got 338 replies, table 341 images, 4.4 to 7.9 ms per icon on the worker,
  about 140 to 220 icons a second. A wheel through the whole folder sent 725 requests and
  the worker cleared them in about 3.5 s at 4.5 to 4.9 ms per icon (about 210 a second).
- Resolve on the UI thread: 4 to 10 us average per call over 260 to 1,600 calls.
- Screenshots (software renderer, `tools/shot.ps1`) of System32, This PC and
  `C:\Users\jtkli` show per-type file icons, `.exe` icons, drive icons (including the
  Google Drive volume's own icon) and custom folder icons (OneDrive, Videos).

### Rework (2026-10-10)

**The failure.** Jeremy's console WPR gate (accessibility on, two panes scrolling the 40k
folder for 10 s after the split), three clean builds back to back: week-one main 16.5%,
M1 19.9%, M2 22.3% main-thread busy. The bar is under 20% with icons on, so M2 failed by
2.3 points; the in-app RDP comparison above had not shown it. M2's 247 ms of extra samples
over M1 (symbolized trace `C:/traces/export/twinfm-p1-m2/bf-main-10s-sym2.html` against the
M1 one) split into: `ImageItemVTable::render` / `draw_image` 106 ms, of which
`Texture::new_from_image` 29, femtovg `ImageStore::alloc` 29, `delete_image` 24 and
`CachedGraphicsData::new` 34; plus `ensure_instantiated` 413 to 573 ms and `Property::get`
302 to 435 ms. The Rust side (visible-range resolve, the `changed content-y` callback)
stayed under 15 samples.

**The mechanism.** `Image::from_rgba8_premultiplied` gives an image with
`ImageCacheKey::Invalid`, and the femtovg renderer shares GPU textures only for keyed images
(`TextureCacheKey::new` in `internal/renderers/femtovg/images.rs`, `itemrenderer.rs` around
line 828). Every time a recycled row's `icon` property changed during a scroll, the renderer
created a texture for that row and deleted the old one, although the 40k rows show one
image. On top of that each of the about 70 visible rows in two panes had one more item
(`Image`) and one more property (`icon`) to instantiate and evaluate as rows recycled. The
per-row `Image` also carried Slint's default `accessible-role: image`, so it was not, as the
Deferred list above assumed, invisible to the accessibility tree.

**The strip design.** Rows carry no image and no icon column item: `Row` lost `icon`, the
row's 22 px icon slot became 28 px of left padding (6 px plus 22 px, the same text position),
and `row_data` no longer touches the icon table. Each pane has one `Image` over its list's
icon column, inside a clipping `Rectangle` the size of the list's viewport, at x 6 px and
`y: list.content-y + strip-first * 22px`, with `accessible-role: none` and no `TouchArea`,
so clicks, wheel and the list's accessibility tree are the rows' as before. Its source is a
`SharedPixelBuffer` composed on the UI thread (`icons::compose`) from the icons of the
visible rows plus `STRIP_MARGIN_ROWS` (4) above and below: `icon_px` (16 x scale, rounded
once) wide and `round(rows x 22 x scale)` tall, each icon copied scanline by scanline to
`round((k x 22 + 3) x scale)`, shown at exactly its physical size (`image-fit: fill`,
`image-rendering: pixelated`, width and height set from Rust as physical / scale). The
table now holds pixel buffers instead of `slint::Image`s, and hands out each one resampled
to the strip's icon size (box filter, cached per index for the current size), so buffers
from `SIIGBF_BIGGERSIZEOK` and the 16 x 16 placeholders compose at any scale, and a scale
change now recomposes from the fitted buffers rather than drawing stale sizes. The
`changed content-y` callback still drives `resolve_view`, which now also decides
(`icons::needs_recompose`) whether to recompose: when nothing is composed, the visible rows
leave the composed ones, the list shrank under them, the scale changed, or the strip is
dirty (an icon landed for a row in it, or a pass swap replaced the rows). A scroll that
keeps the visible rows inside the strip moves the image through its `y` binding and
recomposes nothing. Navigate and an empty list hide the strip; the second pane's strip is
hidden with the pane when `split` is off. The `[icons]` line now ends with the strips
composed and their average compose time. Tests 25 to 29: strip layout arithmetic (row to y
offset, range to height, every icon inside its row band at 1, 1.25, 1.5, 1.75 and 2), a
compose of three rows from 2 x 2 icons (pixels at the row offsets, the placeholder for a
row with no icon, transparent elsewhere), the recompose decision, and the box-filter fit.

**Measured in-app** (RDP session, so relative only; default features; script
`sleep 2500;split;sleep 500;wheel 1 -120 16;wheel 0 -120 16;drag 1 400 1000;drag 0 400 1000;sleep 6000;quit`;
builds run alternately; M1 is `target-m1`, unchanged code at 41304c6; "M2 before" is last
night's per-row build in `target-m2`). UI-thread busy over the 10 s after the split:

| Folder | M1 | M2 before | M2 strip |
|---|---|---|---|
| `C:\traces\bigdir` | 16.4, 16.9, 18.1, 16.6, 17.0, 17.0, 16.4, 16.7: **16.9%** | 18.3, 19.2, 18.7, 19.2: **18.9%** | 16.2, 14.8, 15.3, 18.4, 15.9, 16.4, 16.6, 16.7, 16.1, 16.6, 18.0, 16.1: **16.4%** |
| `C:\Windows\System32` | 18.3, 18.6, 18.0, 17.2, 17.5, 18.9, 19.1, 18.0: **18.2%** | 19.4, 21.7, 21.4, 20.9: **20.8%** | 18.7, 18.9, 20.9, 19.2, 18.6, 16.4, 18.3, 18.6, 18.1, 17.7, 19.5, 18.7: **18.6%** |

The strip build is at M1 on bigdir and 0.4 points above it on System32, inside the
run-to-run spread (16.4 to 20.9 for the same build there); the per-row build sat 2 to 2.6
points above M1 in the same session, which matches the size of the WPR failure. System32 is
the case where icons actually load (555 requests and replies per run, table 558 images), so
some of its remaining difference is icon replies landing and recomposing. No panics.

- Compose cost: 21 to 25 us average per strip (two panes, about 40 visible rows plus the
  margin each), 369 composes per bigdir run and 419 to 461 per System32 run, so 8 to 10 ms
  of UI-thread time in a 10 s run.
- Screenshots (software renderer, `tools/shot.ps1`) of System32 at the top and scrolled to
  the end show the icons aligned with their rows in both panes and the partly scrolled top
  row's icon clipped at the header.

Gate (2026-10-10, Jeremy at the console, strip build `target-m2`, same conditions as the
morning run): **19.3%** by WPR (1932 sampled ms in the 10 s after the split; in-app 20.5%).
Passes the 20% bar; the per-row version read 22.3% and M1 19.9%. Compose cost in the run: 125
strips at 24 us average. Trace `C:/traces/twinfm-p1-m2strip.etl`.

## M3: file operations

### What landed (2026-10-09)

- **File operation worker** (`src/fileops/mod.rs`). One thread, started with the app,
  `CoInitializeEx(COINIT_APARTMENTTHREADED)`, running a real `GetMessage`/`DispatchMessage`
  loop, because the shell's progress, conflict and confirmation windows are created on the
  calling thread and need a pump there. Jobs arrive over a crossbeam channel, each followed by
  a `PostThreadMessage(WM_APP + 1)` nudge; the worker runs every queued job synchronously
  (`PerformOperations` pumps its own dialogs), reads `GetAnyOperationsAborted`, releases every
  interface on the same thread, and reports through a callback the app turns into
  `invoke_from_event_loop`. COM is plain `windows-sys` with a 23-slot vtable struct for
  `IFileOperation` (only the used slots typed) and an RAII `Com` wrapper that calls `Release`
  on drop; the `windows` crate was not needed.
- **Jobs**: `Copy { sources, dest_dir }`, `Move { sources, dest_dir }`,
  `Delete { sources, permanent }`, `Rename { source, new_name }`, `NewFolder { parent, name }`,
  wrapped in `Request { id, job, hwnd, no_confirm }`. Items come from
  `SHCreateItemFromParsingName`, one `CopyItem`/`MoveItem`/`DeleteItem` call per source.
  Flags: `FOF_ALLOWUNDO | FOFX_ADDUNDORECORD | FOF_NOCONFIRMMKDIR | FOFX_SHOWELEVATIONPROMPT`;
  a permanent delete drops `FOF_ALLOWUNDO` and adds `FOF_WANTNUKEWARNING`; a copy into the
  sources' own folder and a new folder add `FOF_RENAMEONCOLLISION`; `no_confirm` adds
  `FOF_NOCONFIRMATION` (and drops `FOF_WANTNUKEWARNING`). Outcome: `Done`, `Cancelled`
  (aborted, `COPYENGINE_E_USER_CANCELLED` or `HRESULT_FROM_WIN32(ERROR_CANCELLED)`), or
  `Failed(HRESULT)`.
- **Owner window.** The slint feature `raw-window-handle-06` and the `raw-window-handle` 0.6
  crate give `RawWindowHandle::Win32`; the HWND is fetched on first use and cached as an
  `isize` in the app state, then passed with every job to `SetOwnerWindow`.
- **Keys**, in the list's `FocusScope` (pane.slint), forwarded as `command(pane, name)`:
  **Shift+F5** copy and **Shift+F6** move the selection to the other pane's folder (with the
  split off the status reads "open the second pane (F3) first"); **Delete** to the Recycle
  Bin, **Shift+Delete** permanent (the shell's "Delete File" confirmation shows); **F2**
  rename; **F7** and **Ctrl+Shift+N** new folder; **Ctrl+C / Ctrl+X / Ctrl+V** through the
  clipboard. Operations act on the selection, or the cursor row when nothing is selected.
- **Inline name editor.** One per pane: a `LineEdit` in an opaque box over the cursor row's
  name cell (rename, brought into view first) or over the top row (new folder, prefilled
  "New folder"), text all selected. Return commits through `edit-commit(pane, mode, original,
  typed)`, Escape cancels, focus loss cancels. Rust trims the name and refuses an empty name,
  `< > : " / \ | ? *` and control characters, and a trailing dot or space with a status
  message; an unchanged rename is a no-op.
- **Clipboard** (`src/fileops/clipboard.rs`): `CF_HDROP` (a `DROPFILES` header, `pFiles = 20`,
  `fWide = TRUE`, then the double-NUL-terminated wide path list, in a `GlobalAlloc(GMEM_MOVEABLE)`
  block) plus the registered "Preferred DropEffect" DWORD. `OpenClipboard(hwnd)` is held by
  an RAII guard, so every early return closes it; a block `SetClipboardData` refuses is freed.
  Paste reads both formats (no Preferred DropEffect means copy), runs Copy or Move into the
  active pane's folder, and empties the clipboard after a successful move.
- **Cut rows render dimmed.** `cut: bool` on `Entry` and `Row`, set by name on the source pane
  (`PaneModel::set_cut`) and carried across passes by name on the worker, like the selection.
  The texts draw at 45% foreground; the icon is not dimmed.
- **Results.** The source pane's status line shows a note before the item count ("copying
  3 items...", then "Copied 3 items", "Moved 1 item", "Deleted 3 items to the Recycle Bin",
  "Deleted 1 item permanently", "Renamed to X", "Created X", "Cancelled", "Failed: 0x%08X"),
  cleared on navigate. Both panes refresh through the existing `refresh(pane)` path when a job
  ends. Drives in the This PC list are refused with a status message, as are new folders and
  pastes in This PC and copies or moves to a pane that shows it.
- **Driver.** `src/auto.rs` gained `copy`, `move`, `delete`, `sdelete`, `cc`, `cx`, `cv`,
  `newfolder <name>` and `rename <name>` (the key, then the name typed character by character
  into the editor, then Return), `waitops <ms>` (until no job runs and no pane refreshes, plus
  300 ms for the pass), and key names `ShiftF5`, `ShiftF6`, `Delete`, `ShiftDelete`, `F2`,
  `F7`, `CtrlC`, `CtrlX`, `CtrlV`, `CtrlShiftN`, `Escape`. stderr has `[fileops]` lines
  (submit, note, done).
- **Soak tool** `tools/soak-m3.ps1 -Minutes N`: builds `%TEMP%\twinfm-m3\src\tree` (10 folders
  x 20 files) and an empty `dst`, writes a script of up to 500 cycles, runs the app with
  `TWINFM_SOAK=1`, samples working set and private bytes every 30 s into a CSV, closes the
  window (`WM_CLOSE` to the window titled "twinfm") at the deadline, and reports exit code,
  `panicked` lines, job outcomes and memory.
- **Tests**, 25 to 32 (36 after the icon-strip merge): flags per job (including rename-on-collision and the soak flags);
  status texts for every outcome; name validation; the `DROPFILES` block layout (offsets,
  `fWide`, NUL terminators, length) and its round trip, including an ANSI list; cut marks and
  operation targets in the model; the real clipboard (a message-only window owns it: write a
  move of two paths, read back paths and effect, write a copy, read, clear, read nothing);
  and the real shell: the STA worker with no owner window and `no_confirm` runs NewFolder,
  Copy, Rename, Move, a copy into its own folder ("a - Copy.txt"), a new folder over an
  existing name ("made (2)") and a permanent Delete in `%TEMP%\twinfm-m3-test-<pid>\`,
  checking the disk after each.

### Decisions

- **File operation keys live in the list's FocusScope, not the window's outer scope.** Keys
  reach the focused element first and bubble up; in the outer scope a Delete or Ctrl+X that the
  path bar or the name editor does not consume would act on files while the user is typing.
  In the list they only fire while the list has focus. Shift+F5 is caught there before F5
  bubbles to the outer refresh.
- **Channel plus nudge, and drain until empty.** A nudge posted while a job's dialog is up is
  eaten by the dialog's modal loop (it dispatches thread messages to no window). No job is
  lost: the request is sent before the nudge is posted, and the worker drains the channel
  until it is empty after every job, so a request whose nudge was eaten is found then.
- **One call per source** (`CopyItem` in a loop) rather than an `IShellItemArray`; the shell
  still treats them as one operation with one progress window.
- **HWND on first use**, not in the rendering notifier or a one-shot timer: winit creates
  the window lazily, and the first job or clipboard action is always after it exists.
- **Selection after an operation comes from the existing by-name restore.** Moved and deleted
  names no longer exist when the refresh lands, so they drop out; a copy's sources are still
  there and stay selected. After a rename or a new folder the next pass puts the cursor and
  the selection on the new name.
- **Clipboard calls run on the UI thread with no app-state borrow held.** `EmptyClipboard`
  sends `WM_DESTROYCLIPBOARD` to the previous owner, which can be our own window, so the
  state is read, released, the clipboard touched, and the state borrowed again.
- **A paste is a move only when Preferred DropEffect has MOVE without COPY**; a source that
  offers both gets a copy, as Explorer does.
- **Move to the same folder is refused** with a status message. **A copy into the sources'
  own folder, and a new folder over an existing name, rename on collision** ("a - Copy.txt",
  "New folder (2)"), as Explorer does. Without `FOF_RENAMEONCOLLISION` the copy engine stops
  at a "1 Interrupted Action" window, even with `FOF_NOCONFIRMATION`; the first scripted
  edge-case run found this (the worker sat in that window until the app quit). A rename onto
  an existing name still asks, as in Explorer.
- **`TWINFM_SOAK=1` also fences the paths**: every source and destination must lie inside
  `%TEMP%\twinfm-m3`, or the job is refused with a status message. The soak script also
  scrolls `C:\traces\bigdir` and System32, and a mis-clicked row there must never become a
  permanent delete without confirmation. With `no_confirm`, `FOF_WANTNUKEWARNING` is dropped
  too, because that flag partially overrides `FOF_NOCONFIRMATION`.
- **No ".." row exists** in TwinFM (Backspace and Alt+Up go up), so the ".." case does not
  arise; drive rows are the only refused rows.

### Dialog parenting, checked from outside the process

Run without `TWINFM_SOAK`, with a scratch file, a PowerShell `EnumWindows` probe listing the
process's visible top-level windows, their class, owner (`GetWindow(GW_OWNER)`) and thread:

- **Shift+Delete**: a "Delete File" dialog (`#32770`), **owned by the main window**, created on
  the worker thread (not the UI thread), and the main window **disabled while it is up**
  (modal, as in Explorer). `WM_CLOSE` to it: the job reported `Cancelled`, the file was still
  there, the main window was enabled again. The UI thread kept rendering throughout.
- **Delete** (Recycle Bin): no dialog; the file went to the Recycle Bin. Windows 11's default.
- **Copy onto an existing name in another folder**: a "Replace or Skip Files" window of class
  `OperationStatusWindow`, **unowned (owner 0) and modeless** (the main window stays enabled)
  despite `SetOwnerWindow`. That is the copy engine's own progress window, which Explorer
  shows the same way (its own taskbar button). `WM_CLOSE`: `Cancelled`, the destination
  untouched.
- So `SetOwnerWindow` parents the confirmation dialogs; the copy engine's progress and
  conflict window stays top-level. While such a window waits for an answer, later jobs queue
  behind it on the single worker.

### Deviations

- Status texts carry more than the brief's bare words: "Deleted N items to the Recycle Bin" or
  "permanently", "Renamed to X", "Created X".
- `Request` carries the test-only `no_confirm` beside the `Job`, rather than inside it.
- Results come back through a `done` callback on the worker (the app wraps it in
  `invoke_from_event_loop`; the test sends to a channel), not a second channel.
- The HWND is fetched on first use (above).
- A `waitops` step was added; no `select` alias (`click` already does it).
- Rename-on-collision for a copy into its own folder and for a new folder (above).

### Deferred

- **The actual new name is not read back.** A new folder made as "New folder (2)" reports
  "Created New folder" and the cursor looks for "New folder"; an
  `IFileOperationProgressSink::PostNewItem` would give the real name.
- **The cursor after a delete or move** goes nowhere (its name is gone); Explorer moves it to
  the next row.
- **Cut marks do not notice another program taking the clipboard**; they clear on the next
  copy or cut, a cut-paste, or navigating away. `AddClipboardFormatListener` would fix it.
- **No progress or queue in TwinFM itself**: the shell's window shows progress; the status line
  shows only "copying N items..." and the result. Queued jobs cannot be cancelled from TwinFM.
- **Undo**: `FOFX_ADDUNDORECORD` is set, but Explorer's Undo picking it up has not been
  checked, and TwinFM has no Ctrl+Z of its own.
- **The renamed or new row is not scrolled into view** after the pass.
- The cut row's icon is not dimmed (only its text).
- A Narrator check of the name editor and of the status notes.

### Soak

10 minutes, `tools/soak-m3.ps1 -Minutes 10`, RDP session, default features, on the build
before the rename-on-collision change (the cycle never copies into its own folder or makes a
folder over an existing name, so that change does not touch it):

- exit code 0 (window closed by the script at 10.0 min), **0 `panicked` lines**;
- 24 cycles started, **278 jobs, 0 failed, 0 cancelled**, 0 soak refusals, 0 `waitops`
  timeouts; each cycle added one Recycle Bin entry (the 200-file tree);
- working set 102.7 MB at 5 s (before the first cycle had loaded the shell and bigdir), then
  140.9 MB at 35 s and 148.8 MB at 601 s, oscillating between 140.9 and 162.8 MB with the
  scroll scenario and no upward trend; private bytes 113.1 MB at 35 s, 119.2 MB at the end,
  range 113.1 to 136.8 MB.

A 3-minute confirmation run on the final build: exit code 0, 0 `panicked` lines, 7 cycles,
84 jobs, 0 failed or cancelled; working set 142.0 MB at 35 s, 160.3 MB at 181 s (peak 172.9 MB
during the scroll scenario), private bytes 112.9 to 146.2 MB.

Soak, 30 minutes (2026-10-10, `tools/soak-m3.ps1 -Minutes 30`, M3 build eb2c2df, RDP session):
exit 0 with the window closed by the script, 0 panics, 70 cycles, 832 file operations with 0
failed, 0 cancelled, 0 soak refusals, 0 `waitops` timeouts. Working set 102.7 MB at 5 s, 135.9 MB
at 1801 s, range 102.7 to 173.4 MB; private bytes 102.9 to 125.9 MB. The end point sits below the
10-minute soak's 148.8 MB, so the swings follow the scroll scenario and there is no growth trend.
Samples: `%TEMP%/twinfm-m3/soak-m3-memory.csv`.

Gate (2026-10-10, Jeremy at the console, `target-m3`, run right after the M2 strip run):
**19.6%** by WPR (1957 sampled ms; in-app 19.4%). Within noise of M2's 19.3%, as expected for
a milestone that adds no per-frame work. Trace `C:/traces/twinfm-p1-m3.etl`.

## M4: shell context menu

### What landed (2026-10-10)

- **Shell menu** (`src/shellmenu.rs`), on the UI thread. `SHParseDisplayName` turns the pane's
  folder into a PIDL (the drive list uses This PC's parsing name, so drives get their own
  menus), `SHBindToObject` gives the folder's `IShellFolder`, `IShellFolder::ParseDisplayName`
  gives a child PIDL per selected name, and `GetUIObjectOf(hwnd, n, apidl, IID_IContextMenu)`
  gives the menu. Empty space gets `CreateViewObject(hwnd, IID_IContextMenu)`, the folder
  background menu ("New", "Paste", "Properties" and the installed handlers). Then
  `QueryContextMenu(hmenu, <after our items>, 100, 0x7FFF, CMF_NORMAL | CMF_EXPLORE | (Shift ?
  CMF_EXTENDEDVERBS : 0))` and `TrackPopupMenuEx(TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN |
  TPM_TOPALIGN)` at the click's screen point (the logical point times the scale factor, through
  `ClientToScreen` on the main HWND). A shell id goes to `InvokeCommand` with a
  `CMINVOKECOMMANDINFOEX`: `lpVerb` and `lpVerbW` are `MAKEINTRESOURCE(id - 100)`, `fMask` is
  `CMIC_MASK_UNICODE | CMIC_MASK_PTINVOKE` with the point, `nShow` is `SW_SHOWNORMAL`, and item
  menus get the folder as `lpDirectoryW`. RAII wrappers release the interfaces, free the PIDLs
  with `CoTaskMemFree` and destroy the menu, in reverse creation order. COM is plain
  `windows-sys` vtables, as in `icons.rs` and `fileops`. New `windows-sys` features:
  `Win32_UI_Shell_Common` (`ITEMIDLIST`), `Win32_UI_Input_KeyboardAndMouse` (virtual-key codes
  for the driver) and `Win32_Security` (only because `windows-sys` gates `CreateProcessW` behind
  it).
- **COM on the UI thread.** `CoInitializeEx(COINIT_APARTMENTTHREADED)` once, at the first menu,
  logged as `[menu] UI thread CoInitializeEx`. It returned `S_FALSE` on every run: winit's
  `OleInitialize` (for drag and drop) had already made the thread an STA. `RPC_E_CHANGED_MODE`
  would refuse the menu with a status note. It is never uninitialised (see Decisions).
- **IContextMenu2/3 routing.** The menu is queried for `IContextMenu3`, then `IContextMenu2`.
  While `TrackPopupMenuEx` runs, a comctl32 subclass (`SetWindowSubclass`) on the main window
  forwards `WM_INITMENUPOPUP`, `WM_DRAWITEM`, `WM_MEASUREITEM` and `WM_MENUCHAR` to
  `HandleMenuMsg2` (or `HandleMenuMsg`; `WM_MENUCHAR` only with 3) and returns its result.
  Everything else goes to `DefSubclassProc`, which is winit's window procedure. The routing data
  is a `Box` that lives until `RemoveWindowSubclass`, which runs before the menu interfaces are
  released. A `[menu] popup` line logs each submenu's name and item count after the shell
  filled it.
- **TwinFM's own items** above a separator, with ids 1 to 3 (1..=20 reserved, the shell starts at
  100), inserted with `InsertMenuW` before `QueryContextMenu`:
  - "Open in other pane": for a single directory or drive, or the pane's own folder on the
    background menu. Turns the split on if needed.
  - "Open in terminal": `wt.exe -d "<dir>"` through `CreateProcessW`, or, if that does not
    start, `powershell.exe -NoExit -Command "Set-Location -LiteralPath '<dir>'"`. Each gets a
    new console and the folder as its working directory, and inherits no handles. The directory
    is the selected one when the selection is exactly one directory, else the pane's folder.
  - "Copy path": `CF_UNICODETEXT`, one full path per line, CRLF between lines. It clears the cut
    marks, since the cut is no longer on the clipboard.

  The This PC background menu has no own items.
- **Right-click** (pane.slint). On a row, the right press selects (new `SelOp::Context = 4`:
  an unselected row becomes the only selection, a selected row keeps the selection) and moves
  the cursor and focus there. The right release fires `context-menu(row, x, y, shift)` with
  window coordinates from `absolute-position` plus the mouse position. A `TouchArea` over the
  list's empty space below the last row (zero height when the rows fill the viewport) fires
  `context-menu(-1, ...)`.
- **Keyboard.** Shift+F10 and the Menu key (Slint has `Key.Menu`; winit maps the Apps key to it)
  in the list's `FocusScope` scroll the cursor row into view, then call `context-key(row, list
  position, content-y, visible-height)`. Rust places the menu at the name column under that row
  (`shellmenu::row_menu_point`, kept inside the viewport), or at the list's top as the
  background menu when there is no cursor row. As with the file operations, the menu acts on
  the selection, or on the cursor row when nothing is selected.
- **After `InvokeCommand`**, both panes refresh through the existing path. A failed command
  (anything but a cancel) shows its HRESULT in the status line. The shell's "rename" verb (read
  with `GetCommandString(GCS_VERBW)`) is not invoked: it opens TwinFM's own name editor,
  because the shell's rename needs an `IShellView` site and does nothing without one.
- **Driver.** `rclick <pane> <row>` (press, 60 ms for a frame so the row is drawn selected,
  then release) and `rclickempty <pane>` (the real empty space when there is some on screen,
  else the `context-menu` callback called directly, which the log says). After them come the
  in-menu steps: `menuopen <text>` (Down presses, skipping separators, then Right),
  `menukey <Down|Up|Right|Left|Escape>`, `menupick <own item>` (refuses shell items, so a
  script can never run Delete) and `menuclose` (`EndMenu`). The `key` names `ShiftF10` and
  `Menu` take in-menu steps too. stderr has `[menu]` lines: the target, the full item list, each
  submenu as it fills, the close with its duration and UI-thread CPU, and the invoked verb.
- **Tests**, 36 to 42: menu id partitioning (own range, gap, shell range, out of range); the
  terminal command lines and the PowerShell fallback (a trailing backslash, `;`, `'`, `$`);
  the Copy path text; the `CF_UNICODETEXT` block; the right-click selection rule; and the
  keyboard menu point (row to screen position, scrolled, clamped above and below, a collapsed
  list).

### Decisions

- **Slint does not run inside the menu's modal loop.** The brief expected the loop to dispatch
  Slint timers, paint and the driver, so that a held borrow would panic. With winit 0.30 it does
  not. The menu runs inside winit's event handler (from the release event), and
  `EventLoopRunner::send_event` buffers every event that arrives while the handler runs: Slint's
  timers (driven by winit's own wait), `invoke_from_event_loop` wake-ups, input. winit's
  `WM_PAINT` handler sees `should_buffer` and only validates the window. Measured: a 6-second
  menu held the 5-second `[perf]` report until just after it closed. During a 1.5-second menu, a
  refresh's filter and sort result arrived and was applied right after the menu closed, cursor
  and selection restored. So the `[perf]`-during-a-menu proof cannot exist. What runs inside the
  loop is Win32: the subclass, Win32 timers, and the shell's code. The rule stays: `show` is
  called with no `RefCell` borrow held. It is commented at the call site and in the module
  docs, because a winit change or a shell extension pumping messages could make re-entry
  possible, and the driver checks it on every in-menu tick.
- **No paint spin while a menu is up.** A 6-second idle menu cost 0 ms of UI-thread CPU.
- **In-menu driver steps run from a Win32 thread timer** (`SetTimer` with a `TIMERPROC`, which
  `DispatchMessage` calls inside the menu's loop), since a `slint::Timer` would only fire after
  the menu closed. It is one repeating 10 ms timer per menu with a due time. The first version
  re-set a one-shot timer from its own callback: Windows gave the new timer the old id, and the
  menu loop dispatched the `WM_TIMER` it had already generated for that id again at once, so a
  6000 ms `sleep` ended after 3 ms. Menu keys are posted as `WM_KEYDOWN` to the main window; the
  menu's loop reads keyboard input from the thread's queue.
- **Own items above the shell's**: `QueryContextMenu` gets `indexMenu` = the number of our items
  plus the separator. The brief's `indexMenu` 0 would have put the shell items above ours.
- **Select on the right press, open on the release**, as Explorer does. Nothing renders while
  the menu is up, so selecting on the release would leave the row undrawn behind the menu.
- **"Open in terminal" through `CreateProcessW`, not `std::process::Command`.** Command always
  passes `STARTF_USESTDHANDLES` with our standard handles, so PowerShell in its new console would
  read and write the console TwinFM was started from. `ShellExecuteExW` would need
  `Win32_System_Registry`. "wt.exe not on PATH" is detected by `CreateProcessW` failing rather
  than by a PATH search: same outcome, and wt.exe is an app execution alias (a reparse point)
  that a hand-written PATH search can get wrong.
- **`CoInitializeEx` once, never `CoUninitialize`.** `S_FALSE` also takes a reference that should
  strictly be balanced, but a `CoUninitialize` at the end of each menu could tear down an
  apartment a shell extension's object still uses. One reference held for the life of a
  thread that lives as long as the process costs nothing.
- **The drive list's menus come from the This PC shell folder**, so a drive gets its real menu
  (Properties, Eject, Format). The This PC background menu gets no own items.
- **The background menu has "Open in other pane"** too: it opens the pane's own folder there.

### Deviations

- `QueryContextMenu`'s `indexMenu` is after our items, not 0 (above).
- `CoUninitialize` is never called, including after `S_OK` (above).
- `menuclose` calls `EndMenu` instead of posting `WM_CANCELMODE` or Escape (Escape closes only
  the innermost open submenu). `menuopen`, `menukey` and `menupick` were added to open and
  check submenus and to drive the own items.
- `rclickempty` calls the callback directly when no empty space is on screen. `C:\Windows` and
  bigdir fill the list, so the proof runs use that path; the real pointer path was checked in a
  four-entry folder.
- The `[perf]`-during-a-menu check was replaced by the in-menu tick check (above).
- The shell's "rename" verb goes to TwinFM's editor.
- New feature `Win32_Security`, for `CreateProcessW`.

### Deferred

- **Refresh after `InvokeCommand` only catches what the command did before it returned.** Many
  verbs finish later (Properties, the shell's own Delete, archive tools); their changes appear
  at the next F5. Directory watching is the fix.
- **The rename interception and the invocation of a shell verb have not been driven by a
  script**: `menupick` refuses shell items by design. They need a check by hand.
- Explorer clears the selection on a right-click in empty space; TwinFM keeps it (the
  background menu acts on the folder either way).
- This is the classic menu ("Show more options" in Explorer), which is what `IContextMenu`
  hosts get. Windows 11's compact menu (`IExplorerCommand`) is not shown. Some entries appear
  twice, for example Windows Terminal's own "Open in Terminal" next to TwinFM's "Open in
  terminal".
- TwinFM's own items have no icons.
- wt.exe's `;` separator is escaped as `\;`, but this was not tried on a real folder with a `;`
  in its name. The PowerShell fallback was not run live (wt.exe is installed here); the unit
  test covers its command line.
- **No screenshot of a menu.** `tools/shot.ps1` uses `PrintWindow` on the main window, and the
  menu is a separate top-level window; screen capture does not work from the harness. The
  submenu contents below come from the `[menu] popup` lines, read from the real menus after
  the shell filled them. A visual check and a Narrator check need Jeremy at the console.

### Modal-loop proof

`TWINFM_SCRIPT="sleep 2000;rclick 0 3;sleep 400;menuclose;sleep 300;rclickempty 0;sleep 400;menuclose;sleep 300;quit"`,
20 runs on `C:\Windows` and 20 on `C:\traces\bigdir`, back to back, RDP session, default
features, perf timers running:

| Folder | Runs | Exit 0 | `panicked` | Borrow errors | Menus opened and closed | In-menu ticks with state and driver free |
|---|---|---|---|---|---|---|
| `C:\Windows` | 20 | 20 | 0 | 0 | 40 | 80 of 80 |
| `C:\traces\bigdir` | 20 | 20 | 0 | 0 | 40 | 80 of 80 |

Other scripted runs:

- **Submenus fill through the routing**: "Send to" 5 items (Bluetooth device, Compressed
  (zipped) folder, Desktop (create shortcut), Documents, Mail recipient) on a folder in
  `C:\Windows`; "Open with" 8 items on a `.txt`; "New" on the background menu 16 items (Folder,
  Shortcut, then document templates) in a folder under `%TEMP%`, and 1 item (Folder) in
  `C:\Windows`. Each root `WM_INITMENUPOPUP` reports "not handled" by `HandleMenuMsg2`, which is
  expected; every submenu reports handled.
- **Own items end to end**: Copy path put `...\twinfm-m4\sub dir` on the clipboard
  (`Get-Clipboard` read it back); Open in other pane turned the split on and opened the folder;
  Open in terminal started `wt.exe -d "...\sub dir"` (one Windows Terminal window, closed by the
  check). Shift+F10 and the Menu key opened the item menu under the cursor row at (31, 129)
  logical: the list's left plus 28 px, and row 2's bottom edge.
- **Re-entry**: a 6-second menu kept the `[perf]` report back until it closed. A refresh
  started just before a 1.5-second menu: its worker pass result arrived while the menu was open
  and was applied after it closed.
- UI-thread CPU while a menu is open: 0 ms for an idle 6-second menu, 16 to 156 ms for menus
  whose submenus the shell filled.

Soak: with M5 (the M5 soak opens and closes 40 menus in 10 minutes; see there).

Gate: not run separately; nothing here runs per frame. The full M6 build, which contains this
milestone, read 19.8% by WPR on 2026-10-10 (see M6b).

## M5: drag and drop

### What landed (2026-10-10)

- **Drop target** (`src/dnd.rs`), on the UI thread. winit registers its own `IDropTarget` on the
  window, so `register` calls `RevokeDragDrop(hwnd)` once and then `RegisterDragDrop(hwnd, ours)`;
  both return `S_OK` (`[dnd] registered: RevokeDragDrop (winit's target) = 0x00000000,
  RegisterDragDrop (ours) = 0x00000000` on every run). The window handle is not there before
  `run` (winit creates the window lazily), so a 50 ms Slint timer started in `main` retries until
  `main_hwnd` has it, registers, and stops; the same step sets the drag threshold from
  `SM_CXDRAG`/`SM_CYDRAG` over the scale factor (4 x 4 px here). winit revokes whatever is
  registered when it destroys the window, which releases ours. The object is a `#[repr(C)]`
  struct with a hand-written vtable and an atomic reference count, as in `fileops` and `shellmenu`.
- **DragEnter / DragOver / DragLeave / Drop.** DragEnter reads the data object's `CF_HDROP`
  (`QueryGetData`, `GetData`, `ReleaseStgMedium`, parsed by `clipboard::parse_dropfiles`) and
  keeps the paths in the target; a data object without `CF_HDROP` is refused everywhere. Each
  call turns the screen point into client px (`ScreenToClient`), then `main::drop_spot` divides by
  the scale factor and hit-tests (`dnd::hit`) with the layout arithmetic of `app.slint` and
  `pane.slint`: pane x ranges from the split ratio (the 6 px splitter between them is no target),
  the list from y 63 to 23 px above the bottom and 3 px in from the pane's sides, 22 px rows from
  `content-y`. A directory row (including a drive row in This PC and a reparse directory) is the
  drop folder; any other point in a pane (a file row, empty space, the header, the path bar, the
  status line) is the pane's folder; This PC itself is no folder. The effect (`dnd::effect_for`):
  `DROPEFFECT_MOVE` when the first source and the folder share a volume (`c:` or
  `\\server\share`, `\\?\` prefixes removed), else `DROPEFFECT_COPY`; Ctrl forces copy, Shift
  move; Ctrl+Shift and Alt (a shortcut in Explorer) copy; an effect the source does not allow
  falls back to the other one it allows. `DROPEFFECT_NONE` for a drop into a source's own folder,
  onto a dragged item, or into a folder inside a dragged folder.
- **Drop** reads `CF_HDROP` again from the data object it is handed, decides as above, clears the
  highlight, and hands the job to `slint::invoke_from_event_loop` (`main::drop_files`, which
  submits a `Copy` or `Move` to the M3 worker; the shell shows its own progress, both panes
  refresh when it ends, the drop pane shows the note). A move is reported to the source as an
  optimized move (Decisions).
- **Highlight.** `drop-pane` and `drop-row` on `App` (-1 none; row -1 the pane's folder), set by
  the target through `main::drop_highlight` only when they change, cleared on DragLeave and Drop.
  `Pane` draws them as two overlays over the list viewport: the row as a 22 px band
  (accent at 35%, 2 px accent border) at `content-y + row * 22px`, the folder as a tint over the
  whole list with a 3 px accent border. No per-row binding was added.
- **Drag source.** The row's `TouchArea` reports the left button to Rust as `row-gesture(row,
  what, ctrl, shift)`: 0 press, 1 release, 2 the first move past the threshold with the button
  down (`self.pressed`, `abs(mouse - pressed) > drag-dx / drag-dy`, once per press), 3
  double-click. `dnd::Gesture`, one per pane in the app state, decides: pressing an unselected row
  selects it at once (Only, or Toggle with Ctrl, Range with Shift); pressing a row that is
  already selected changes nothing until the release, so a drag carries the whole selection and
  a click without a drag still selects only that row (Explorer). A drag (`main::drag_start`)
  checks that the left button is really down (`GetKeyState`), takes the selection's names and the
  folder out of the state, drops the borrow, and calls `dnd::drag`: `shellmenu::parse_items` (the
  context menu's PIDL code, now shared) for the folder and child PIDLs,
  `SHCreateDataObject(pidlFolder, n, apidl, NULL, IID_IDataObject)`, our `IDropSource`
  (`QueryContinueDrag`: Escape or the right button cancels, releasing the left button drops;
  `GiveFeedback`: `DRAGDROP_S_USEDEFAULTCURSORS`) and `DoDragDrop(data, source, COPY | MOVE)`. Drives
  are not dragged (a note says so). After `DRAGDROP_S_DROP` the source pane refreshes.
- **The release after a drag.** OLE's loop consumes the button-up, so Slint never sees it and
  keeps the row's `TouchArea` grabbed. `drag_start` ends with a `Timer::single_shot(0)` that
  dispatches a `PointerReleased` at the cursor, after the pointer event that started the drag has
  finished dispatching. The gesture is `Dragged` by then, so that release selects nothing, and the
  double-click Slint may derive from it (it fires before the release's pointer event) opens
  nothing (`[dnd] pane=N double-click at the end of a drag: not opened`).
- **Driver** (`src/auto.rs`): `dropfiles <pane> <row> <copy|move|auto> <path|path...>` builds a
  data object that serves only `CF_HDROP` (`dnd::hdrop_data`, a `GlobalAlloc` block per `GetData`)
  and calls our registered target's DragEnter then Drop through its vtable, at the screen point of
  a row's centre (row -1: the pane's header, so its folder), with Ctrl for copy, Shift for move,
  nothing for auto. `dragrow <pane> <row> <dx> <dy>` presses a row, waits 60 ms, and moves the
  pointer, which starts a real OLE drag; the `sleep` and `dragcancel` steps after it run on a
  helper thread (Decisions). stderr has `[dnd]` lines: registration, DragEnter, each change of
  spot or effect, Drop with the effect and what was reported, the job, `DoDragDrop`'s start and
  result, the first `QueryContinueDrag` of each drag with whether the app state is free, and the
  release after the drag.
- **Soak tool** `tools/soak-m5.ps1` (`soak-m3.ps1` untouched): the M3 cycle in
  `%TEMP%\twinfm-m5\`, plus per cycle a `dropfiles` copy of two files into the right pane, a
  `dropfiles` move of those copies back into the left pane, their permanent delete, and
  `rclick 0 0; sleep 300; menuclose` and `rclickempty 1; sleep 300; menuclose`. `TWINFM_SOAK=1` now
  fences paths to `%TEMP%\twinfm-m3` or `%TEMP%\twinfm-m5`; drops go through the same `submit`, so
  the fence and `FOF_NOCONFIRMATION` apply to them.
- **Tests**, 42 to 49: the `QueryContinueDrag` decision over (Escape, left, right); volume keys
  (drive, `\\?\`, UNC, `\\?\UNC\`); the effect decision (same and other volume, every modifier,
  what the source allows, every refused drop, the optimized-move report); hit-testing (rows,
  below the last row, header, status line, padding, splitter, a scrolled list, each pane's own
  scroll, another split ratio) and the drop folder (directory row, file row, empty space, a drive
  row, This PC); the gesture rules (click on unselected and selected rows, Ctrl and Shift, a drag
  that keeps the selection, the release and double-click after a drag, a real double-click that
  drops its deferred selection); the synthetic data object round-tripping three paths through
  `GetData` twice, refusing other formats and media, and its `QueryInterface`; and the shell's own
  data object over two real files offering `CF_HDROP` with both paths and storing a "Logical
  Performed DropEffect" written with `SetData`.

### Decisions

- **A move dropped on TwinFM is an optimized move.** The worker moves the files later, so `Drop`
  returns `DROPEFFECT_NONE` and sets "Performed DropEffect" `DROPEFFECT_NONE` and "Logical
  Performed DropEffect" `DROPEFFECT_MOVE` on the data object, the protocol Explorer uses as a
  target. Returning `DROPEFFECT_MOVE`, as the brief has it, invites a source to delete the
  originals (delete-on-paste) before the worker has moved them. A copy returns `DROPEFFECT_COPY`.
- **`Drop` posts the job through `invoke_from_event_loop`**, not straight to the worker. A job
  needs an id, the source-pane entry and the note, all in the app state; the closure runs after
  our own `DoDragDrop` has returned (a pane-to-pane drag) or at the next turn of the event loop
  (another program's drag). The target methods themselves read the state only under a short
  `try_borrow` in `drop_spot` (pane folders, row counts, the hovered entry), set the two
  highlight properties, and never submit.
- **The source pane refreshes after any `DRAGDROP_S_DROP`**, not only after `DROPEFFECT_MOVE`:
  Explorer reports its own optimized moves as `DROPEFFECT_NONE`.
- **Refused drops** include a folder inside a dragged folder, beyond the brief's own-folder and
  onto-a-dragged-item cases.
- **The drag starts inside the pointer move** that crossed the threshold, as the context menu
  opens inside the right release: no deferral, so the button is still down when OLE starts. The
  `RefCell` rule holds: `drag_start` takes everything out of the state and drops the borrow before
  `DoDragDrop`. The first `QueryContinueDrag` inside every drag logged `app state free=true`.
- **Scripted drags run their in-drag steps on a helper thread.** OLE's drag loop does not
  dispatch Win32 timer callbacks: a thread timer (M4's mechanism) and then a timer on the main
  window both sat silent, and with the mouse untouched the loop never even called
  `QueryContinueDrag`, so the first scripted drags hung in `DoDragDrop` until killed. The pump
  thread posts a `WM_MOUSEMOVE` to the main window every 20 ms (at the real cursor position) so
  the loop turns, and posts `WM_KEYDOWN VK_ESCAPE` for `dragcancel`; OLE then calls
  `QueryContinueDrag` with `fEscapePressed` set (the real Escape path). It posts Escape on its own
  after 20 s. While a scripted drag runs, the drop source counts the left button as held.
- **No scripted drop.** A `dragdrop` step (release the scripted button) and a `dragto` step
  (`SetCursorPos` onto a row) were built and tried twice, then removed. In this RDP session the
  cursor kept snapping back to the client's mouse position between ticks, so OLE alternated
  DragEnter and DragLeave on our target, and at the release it dropped on whatever window it had
  last hit-tested. The first try reported `DoDragDrop` dropped with effect move from an unknown
  window of another program (the scratch file `d.txt` stayed in `src`; no `d.txt` appeared on the
  desktop or elsewhere in the profile; what that window did with it is not known); the second
  dropped with effect none. A scripted drag can now only be cancelled.
- **Highlight as overlays**, two `Rectangle`s in `Pane` with no input and no accessibility role,
  rather than a binding on every row: the rows carry nothing new for the gate scenario.
- **Pressing a selected row defers its selection to the release** (Only, or Toggle with Ctrl),
  so dragging a multi-selection by any of its rows carries all of it. Ctrl still wins over Shift,
  as in M1's click handler.
- **Alt drags copy.** Explorer makes a shortcut; TwinFM makes none (recorded deviation, below).
- **The soak fence has two roots**, `twinfm-m3` and `twinfm-m5`, so each soak keeps its own
  scratch folder.

### Deviations

- `Drop` reports `DROPEFFECT_NONE` for a move (optimized move, above), not the effect performed.
- The job is posted from `Drop` through the event loop, not to the worker directly (above).
- `dragrow`'s in-drag steps run on a helper thread, not a Win32 timer; there is no scripted
  release (above).
- `dropfiles` takes a mode (`copy`, `move`, `auto`) before the paths and separates paths with `|`,
  because `;` separates script steps.
- Ctrl+Shift and Alt copy (no links).
- `TWINFM_SOAK=1` fences two roots.
- `shellmenu`'s PIDL parsing moved into `parse_items`, shared with the drag source; `Com`, `Pidl`,
  `wide` and `ensure_com` became `pub(crate)`.

### Deferred

- **The highlight of a drag we started does not repaint while the drag runs.** Our own
  `DoDragDrop` runs inside a winit handler, so (as with the M4 menu) nothing of Slint renders until
  it returns; the properties are set, and drawn once the drag is over (then cleared). A drag from
  Explorer or another program is not affected: its target calls arrive through winit's ordinary
  loop. No fix was attempted; rendering from inside the loop would mean driving the renderer
  outside winit.
- **No drag image.** The cursor shows the effect only; `IDragSourceHelper` (our drags) and
  `IDropTargetHelper` (Explorer's image over our window) would add one.
- **Right-button drag** (Explorer's "Copy here / Move here / Create shortcuts here" menu) is not
  supported: a right-drag from TwinFM never starts, and a right-drag from Explorer drops with the
  default effect and no menu.
- **No auto-scroll** near the list's top and bottom edges, and no spring-loaded folders.
- **After a drag into Explorer moves the files,** the source pane refreshes when `DoDragDrop`
  returns; Explorer may still be moving them, and the pane shows them until the next F5
  (directory watching would fix it).
- **Virtual files** (`CFSTR_FILEDESCRIPTOR`: items from a zip folder, an Outlook attachment) and
  asynchronous data objects are refused: only `CF_HDROP` is read.
- **Mounted folders** count as the drive that holds them when choosing move or copy.
- **The OLE path of a pane-to-pane drop is not scripted** (no scripted drop, above): `Drop`'s
  code runs in every `dropfiles` and the soak, and DragEnter and DragOver ran inside our own
  `DoDragDrop` in the scripted drags, but a real `Drop` delivered from inside our own drag loop
  needs the manual check.

### Scripted checks

All in `%TEMP%\twinfm-m5\` (src: `sub\`, `a.txt`, `b.txt`, `c.txt`; dst empty), RDP session,
default features:

- `dropfiles 1 -1 copy src\a.txt|src\b.txt`: DragEnter effect copy (keys Ctrl), Drop reported copy,
  job "Copied 2 items"; both in `dst`, both still in `src`.
- `dropfiles 1 -1 move src\c.txt`: effect move (Shift), Drop reported none, "Moved 1 item";
  `c.txt` left `src`.
- `dropfiles 1 -1 auto src\sub\x.txt`: no modifier, same volume, so move; `x.txt` moved.
- `dropfiles 0 0 copy dst\a.txt` onto row 0 of the left pane (`sub`): the target named
  `pane 0 row 0 (...\src\sub)`, copied into `src\sub`.
- `dropfiles 0 -1 auto src\b.txt` (its own folder) and `dropfiles 0 1 copy src\a.txt` (a file row,
  so its pane's folder, the source's own): effect none, no job.
- `click 0 1; ctrlclick 0 2; dragrow 0 1 0 60; sleep 400; dragcancel`: dragged 2 items (the
  pressed row was selected, so the selection stayed), `QueryContinueDrag escape=true (scripted):
  cancel`, `DoDragDrop returned 0x00040101 (cancelled)` after 406 ms, the release after the drag
  selected nothing (no `[select]` line), files unchanged.
- `dragrow 0 1 1 1` (under the threshold): no drag; the release selected only row 1, the click a
  selected row gets.
- Regression: `click`, `shiftclick`, `ctrlclick`, a `click` on a row of a three-row selection
  (Only on the release), `dblclick` on a folder (opened), `rclick` and `rclickempty` with
  `menuclose` (in-menu ticks with state and driver free), then a `dragrow` and `dragcancel`, then a
  `click`: all as before. In that run the real cursor happened to be over the window, so OLE
  called our DragEnter from inside our own `DoDragDrop` (`over pane 0 (...\src) -> effect none`,
  its own folder) and DragLeave at the cancel.

### Soak

10 minutes, `tools/soak-m5.ps1 -Minutes 10`, RDP session, default features, build f819d8a:

- exit code 0 (window closed by the script at 10.0 min), **0 `panicked` lines**;
- 20 cycles started, **300 jobs, 0 failed, 0 cancelled**, 0 soak refusals, 0 `waitops` timeouts;
- **40 drops** through the drop target (20 copies reported as copy, 20 moves reported as none, the
  optimized move), 40 drop jobs submitted, all among the 300 done;
- **40 menus** opened and closed (20 on a row, 20 on the right pane's empty space), 80 in-menu
  ticks, none with the app state or the driver borrowed;
- working set 102.9 MB at 5 s, 172.4 MB at 35 s, then 175.2 to 184.3 MB from 66 s to 576 s and
  201.2 MB in the last sample at 601 s; private bytes 134.7 MB at 35 s, 136.1 to 145.7 MB to
  576 s, 161.2 MB at 601 s. The plateau sits about 20 MB above M3's (140.9 to 162.8 MB), which
  the shell extensions the context menus load (7-Zip, PowerRename, File Locksmith and others,
  which stay loaded) can account for. Over the plateau private bytes rose about 1 MB a minute
  with the scroll scenario's swings on top, and the last sample is the highest; ten minutes do
  not settle whether that is growth or swing. A 30-minute run (as M3 had) is the next check.

Samples: `%TEMP%/twinfm-m5/soak-m5-memory.csv`. This also closes M4's "Soak: with M5".

### Manual check

Jeremy, at the console, with two scratch folders (for example `C:\Temp\dnd-a` with a few files
and a subfolder, and an empty `C:\Temp\dnd-b`):

From Explorer into TwinFM:

1. Open `dnd-a` in Explorer and `dnd-b` in TwinFM's left pane. Drag one file from Explorer onto
   the empty part of TwinFM's list: the list gets a tinted border while the cursor is over it, the
   cursor says Move, and on release the file leaves `dnd-a` and appears in TwinFM.
2. Drag another file with Ctrl held: the cursor says Copy, and it stays in `dnd-a` too.
3. Drag a file onto a folder row in TwinFM: only that row is highlighted, and the file lands
   inside that folder.

From TwinFM into Explorer:

1. Open `dnd-a` in TwinFM and `dnd-b` in Explorer. Ctrl+click two files in TwinFM, then drag one
   of them into the Explorer window: both go (Move) and leave TwinFM's list (if they still show,
   Explorer was still moving them when TwinFM refreshed: F5, see Deferred).
2. Drag a file with Ctrl into Explorer: it is copied and stays in TwinFM.
3. Start a drag from TwinFM and press Escape before releasing: nothing happens, the selection is
   unchanged, and clicking a row afterwards behaves normally (no folder opens on the release).

Between the panes (the one path no script can drive): with `dnd-a` left and `dnd-b` right, drag a
file from the left pane to the right one: it moves and both panes refresh. No highlight shows
while this drag runs (Deferred).

Soak, 30 minutes (2026-10-10, `tools/soak-m5.ps1 -Minutes 30`, merged M5 build 290f68f, RDP
session): exit 0 with the window closed by the script, 0 panics, 58 cycles, 870 file operations
with 0 failed or cancelled, 116 drops (58 copy, 58 move, all as expected), 116 menus opened and
closed with 0 in-menu ticks finding state or driver borrowed. Working set 103.7 MB at 5 s, 193.7
MB at 1800 s, range 103.7 to 203.5 MB. Private bytes 105 MB at start, 136 at 3 min, 162 at 12
min, back to 138 to 140 from 18 to 27 min, 154 at the end: the 10-minute run's apparent 1 MB/min
climb was the upswing of an oscillation, not growth. Samples: `%TEMP%/twinfm-m5/soak-m5-memory.csv`.

Gate: not run separately; nothing here runs per frame. The full M6 build, which contains this
milestone, read 19.8% by WPR on 2026-10-10 (see M6b).

## M6a: tabs, path bar, opening files, status bar

M6 is split into two branches and two PRs: M6a (this section) and M6b (sidebar, settings, window
icon and title, About dialog).

### What landed (2026-10-10)

- **Tabs per pane** (`src/tabs.rs`, `main.rs`). `Tabs<T>` is a pure list with an active index:
  `open` inserts after the active tab and activates it, `close` refuses the last tab and moves to
  the right neighbour (the left one when the closed tab was the rightmost), `next`/`prev` wrap,
  `select` takes an index. Each pane keeps a `Tabs<Tab>`; a `Tab` owns its `PaneModel` (so its sort
  column, direction and hidden filter), its folder, timing, note, pending focus name, free space,
  and, while in the background, its cursor row and scroll offset. Switching saves the outgoing
  tab's cursor and `content-y`, points the pane's `ListView` at the new tab's model
  (`set_left_rows(ModelRc::from(tab.model.clone()))`), restores path, breadcrumb, sort indicators,
  cursor and scroll, resets the drag gesture, hides the icon strip and marks it dirty, and resolves
  the visible rows, so the strip is recomposed for the new rows before the next frame. Ctrl+T opens
  a tab on the same folder with the same sort and hidden setting; Ctrl+W closes the active tab;
  Ctrl+Tab and Ctrl+Shift+Tab (or Backtab) cycle; Ctrl+1 to Ctrl+9 pick by position. The keys sit
  in the window's outer `FocusScope`; Slint's own Tab focus traversal runs only when no element
  accepted the key, and it leaves Ctrl+Tab alone anyway.
- **Tab strip**: `if tabs.length > 1` above the path bar, one `for` over a `[TabInfo]` (`title`,
  `active`) `VecModel` per pane, rebuilt with `set_vec` on navigation, open, close and switch only.
  Click switches; middle-click or the close glyph closes. Tabs are 60 to 180 px wide.
- **Tab identity in every reply.** Enumeration generations now come from one counter for the whole
  app (`State::generation`), so a generation names one tab: a batch or `Done` finds its tab by
  generation among the pane's tabs, active or not, and a closed tab's model is set to generation 0
  so its late replies find nothing. Filter and sort passes carry the tab id and land in that tab's
  model; a background tab's restored cursor goes into the tab, not the list. The icon worker's
  per-pane generation is the active tab's (set on navigate, refresh and switch); background tabs
  never request icons. Every site that indexed `st.panes[pane]` goes through the active tab:
  file operation sources, notes and the job table (job id to pane and tab id), clipboard cut marks
  (cleared across all tabs, set on the source tab), the context menu, the drag source, the drop
  target's `drop_spot`, refresh and its scroll restore, the icon strip, `ops_busy` and the
  first-frame timing.
- **Background tabs after an operation.** When a job ends or a shell verb is invoked, both panes'
  active tabs refresh as before and every background tab is marked stale; a stale tab refreshes
  when it comes to the front.
- **Path bar** (`src/pathbar.rs`, `pane.slint`). Breadcrumb by default: one button per component
  (This PC > C: > Users > x; a UNC path gives This PC > \\server\share > dir), built from the path
  on navigation and tab switch; clicking button `i` navigates to the prefix it stands for. The row
  is right-aligned inside a clip when it is wider than the bar, so the current folder stays in
  view. Ctrl+L switches to the existing `LineEdit` with the folder selected; Return navigates and
  switches back; Escape or focus loss switches back and restores the text. The "..." glyph at the
  bar's right end makes the text entry permanent (`path-entry-sticky` on the window, both panes);
  the chevron that replaces it brings the breadcrumb back. Both modes occupy the same 32 px, so the
  list's top does not move.
- **Opening files.** Return or a double-click on a non-directory calls `shellmenu::open_item`:
  `ShellExecuteExW` with `SEE_MASK_NOASYNC | SEE_MASK_INVOKEIDLIST`, no verb, the folder as
  `lpDirectory`, on the UI thread with no state borrow held (the shell can show "Open with" from
  inside the call). A failure shows "could not open <name>: <system message>" in the status bar;
  `ERROR_CANCELLED` counts as success. Directories, reparse directories and drives still navigate.
  New `windows-sys` feature `Win32_System_Registry` (`SHELLEXECUTEINFOW` has an `HKEY` field).
- **Status bar** (`src/statusbar.rs`): three texts per pane in place of the status line. Left "N
  items (K hidden)", after the tab's note and with " (loading...)" while enumerating; middle "M
  selected, <total size>" ("M selected" when only folders are selected, empty with no selection);
  right "X free of Y" for the folder's volume. Composed from the active tab on navigation,
  enumeration batches, `Done`, pass swaps, selection changes and notes, never from a scroll or
  render path. The free space comes from `GetDiskFreeSpaceExW` on the enumeration worker, after
  the listing (the `[enum]` worker time is taken before it), in a new `free` field of `Msg::Done`;
  an operation's refresh brings a new figure. `TWINFM_PERF_STATUS=1` appends ", enumerated in X ms"
  to the left text; no tool reads the status bar (`tools/*.ps1` and `tools/analyze-trace.py` read
  `[enum]` and `[busy]` from stderr).
- **Layout arithmetic.** `dnd::Layout` gained `tabs: [bool; 2]` and `dnd::list_top` (63 px, or
  91 px under a tab strip: 26 px strip plus 2 px spacing). `main::window_layout` builds the layout
  from the window and its properties with no state borrow, and the driver's row, thumb, empty-space
  and drop points now use it, so scripts and drops stay right with a tab strip showing.
- **Driver.** `tab <new|close|next|prev|N>` presses the keys through Slint's key path on the
  active pane (`CtrlT`, `CtrlW`, `CtrlTab`, `CtrlShiftTab`, `Ctrl1` to `Ctrl9`; `CtrlL` too),
  `crumb <pane> <index>` calls the breadcrumb callback, `pathmode <pane> entry|crumb` calls
  `set-path-mode`. stderr has `[tab]` (open, close, switch with the restored cursor and scroll),
  `[crumb]` and `[open]` lines.
- **Tests**, 49 to 55: tab bookkeeping (open after the active tab, the last tab never closes,
  next and previous wrap, closing the active tab moves right or, at the end, left, closing a tab
  left of the active one keeps it active); breadcrumb splitting for This PC, `C:\`, `C:\Users\x`
  (with a trailing separator too) and `\\server\share\dir`, with targets and tab titles; status
  text composition (items, hidden, loading, timing, note, error, selection with and without
  files); free-space formatting and a real `GetDiskFreeSpaceExW` on `C:\` and `C:\Windows`; the
  selection summary counting file bytes only.

### Decisions

- **One generation counter for all tabs** rather than a (tab, generation) pair in every message:
  the enumeration `Msg`, the icon `Request` and the worker threads stay as they were, and a
  generation can only ever match one tab.
- **Background tabs refresh when shown, not when the operation ends**: an operation can touch any
  folder (a drop, a shell verb, the other pane), and re-enumerating every tab would cost a 40k
  listing per hidden tab per job.
- **A new tab opens after the active one** (browser order) and copies its sort and hidden setting.
- **The free space is read on the enumeration worker**, so a navigation to a slow network share
  costs no UI time, and it is refreshed after every operation through the refresh.
- **`ShellExecuteExW` after all.** M4 avoided it because `windows-sys` gates
  `SHELLEXECUTEINFOW` behind `Win32_System_Registry`; that is a feature flag of a crate already in
  the build, not a new dependency, and the shell's own association handling (Open with, packaged
  app handlers, `SEE_MASK_INVOKEIDLIST`) is what opening a file needs.
- **The path-mode glyph is "..."**: U+270E (pencil) did not render with the software renderer
  used for screenshots; the chevron U+203A does and is used for the way back.
- **Tab titles** are the folder's own name, the drive (C:) at a root, the share at a UNC root,
  and "This PC" for the drive list.

### Deviations

- The breadcrumb puts This PC at the head of UNC paths too, since Backspace from a share root
  goes there.
- `crumb` calls the callback instead of clicking the button: the button's x depends on text width.
- `tab close` on a pane's last tab logs "the last tab stays" and does nothing.

### Deferred

- **Tab strip overflow**: no scrolling; tabs shrink to 60 px, then the strip clips.
- **Tabs are not restored at start** and cannot be dragged, reordered or receive drops.
- **Breadcrumb**: no sibling-folder dropdown on the separators, no drop onto a crumb.
- **Middle-click on a folder row** does not open it in a new tab (M6b adds middle-click in the
  sidebar).
- **The status bar sums file sizes only**; folder sizes are not computed.
- **Opening files is not scripted**: a scripted open would start a program in Jeremy's session
  (and closing it by force could take an unrelated Notepad window with it). Needs a manual check:
  Return on a `.txt` opens it, Return on a file of an unknown type shows "Open with", cancelling
  it leaves no note.
- **Focus loss in scripted runs.** During the scripted checks a 30-minute soak (another twinfm)
  was running; the foreground window alternated between the two processes (`GetForegroundWindow`
  polled every 30 ms changed eight times in five seconds), and each deactivation took the focus
  from the path entry, which then switched back to the breadcrumb, as the focus-loss rule says.
  The M3 rename editor behaves the same way (checked). With one TwinFM window this should not
  happen; worth a check by hand.

### Scripted checks

RDP session, default features, M6a build in `target-m6`:

- `sleep 1500; tab new; sleep 800; nav 0 C:\Windows; sleep 1200; click 0 2; wheel 0 -5 16; ...;
  tab prev; tab next; tab 1; crumb 0 1; tab 2; pathmode 0 entry; pathmode 0 crumb; tab close;
  tab close; quit` on `C:\Users`: tab 2 opened on `C:\Users` and navigated to `C:\Windows`,
  switches back and forth by Ctrl+Shift+Tab, Ctrl+Tab and Ctrl+1, the breadcrumb's "C:" took tab 1
  to `C:\`, Ctrl+2 went back, the path mode toggled, the first Ctrl+W closed tab 2, the second
  logged "the last tab stays"; exit 0, no panic.
- `C:\traces\bigdir`: scrolled 20 wheel ticks, clicked row 139, opened a tab (scroll 0, no
  cursor), went back: `cursor 139 scroll -2400`; forward and back again: `cursor 139 scroll
  -2398`. The screenshot shows rows 110 to 140 with row 139 selected and each row's icon beside it.
- Screenshots (software renderer) of the split view with two tabs per pane: both tab strips, the
  breadcrumbs This PC > C: > Users > jtkli and This PC > C: > Windows > System32, two rows
  selected on the right, and status bars reading "85 items (17 hidden)" / "492.39 GB free of
  930.47 GB" and "4904 items (8 hidden)" / "2 selected" / the same free space.

Gate (2026-10-10, Jeremy at the console, full M6b build `target-m6`, accessibility on, hidden
off): **19.8%** by WPR (1980 sampled ms in the 10 s after the split; in-app 20.3%). Passes the
20% bar with 0.2 point to spare; M3 on the same conditions read 19.6%, so M4 to M6 together
cost about 0.2 point. Trace `C:/traces/twinfm-p1-m6.etl`.

## M6b: sidebar, settings, window icon and title, About dialog

### What landed (2026-10-10)

- **Sidebar** (`src/sidebar.rs`, `app.slint`), left of the panes, 180 px by default, resizable
  from 120 to 400 px by dragging its 4 px right edge, hidden and shown by F9 or the arrows at its
  foot and on the 12 px strip it collapses to. Three sections in one Slint `ListView` over a
  `[SideItem]` (`label`, `kind`, `index`) `VecModel`, rebuilt with `set_vec` only when something
  in it changes: **Drives** from the existing `drive_entries()` on a worker thread (labels and free
  space can stall), refreshed when the sidebar is shown and on F5; **Places** from
  `SHGetKnownFolderPath` (`FOLDERID_Profile` as "Home", Desktop, Documents, Downloads, Pictures,
  Music, Videos), read once at start, a folder the shell does not resolve left out; **Bookmarks**
  in the order added, titled like tabs. A click opens the row in the active pane, a middle-click in
  a new tab there; headers do nothing. Ctrl+D bookmarks the active tab's folder (not This PC;
  duplicates are refused case-insensitively and with or without a trailing separator), with a
  status note either way. Right-click on a bookmark opens a one-item "Remove bookmark" menu.
- **The sidebar is cached as a rendering layer** (`cache-rendering-hint: true`). femtovg draws the
  whole window every frame, so without it the sidebar's dozen texts were redrawn on every scroll
  frame; with it the sidebar is one texture blit until one of its own properties changes (hover,
  a new row). See Measurement.
- **Settings** (`src/settings.rs`): `%APPDATA%\twinfm\config.toml`, serde + toml (the two new
  crates; both were already in the lock file as Slint dependencies, so `Cargo.lock` gained two
  lines). Fields: the start folders of both panes (the active tabs' folders when last saved),
  hidden files per pane for new tabs (the last Ctrl+H there), split on/off and ratio, sort column
  and direction for new tabs (the last header click), sidebar visibility and width, bookmarks,
  path bar mode (`breadcrumb` or `entry`), and the window's position (physical px) and size
  (logical px). `#[serde(default)]` on the struct, so missing keys take defaults and unknown keys
  are ignored; out-of-range ratio and width are clamped. Loaded before the first navigation;
  a command-line folder still overrides the left start folder; a saved split reopens the second
  pane on its folder. Saved through one single-shot 500 ms `slint::Timer`, restarted by every
  change (splitter drag, sidebar resize, path bar mode, sidebar toggle, split toggle, Ctrl+H, a
  header click, a bookmark, a navigation), so a drag writes once after it stops; also at once on
  the window's close request, and after `run` returns if a save is still pending. Writes go to
  `config.toml.tmp`, then `MoveFileExW(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)`.
  A missing file gives the defaults quietly; an unreadable or malformed one gives the defaults
  and a `[settings]` line, and a malformed one is copied to `config.toml.bad` first because the
  next save replaces it. The window's placement is restored only when at least 100 px across and
  16 px down of its title bar would land on a monitor's work area (`EnumDisplayMonitors` +
  `GetMonitorInfoW`), and is not recorded while the window is minimised or maximised.
- **Scripted runs leave the settings alone.** With `TWINFM_SCRIPT` set no file is read or
  written and the defaults apply, so the gate, soak and check scripts stay deterministic (a saved
  `split = true` would make the gate's `split` step close the second pane) and never touch
  Jeremy's file. `TWINFM_CONFIG=<path>` points any run, scripted or not, at another file.
- **Window title and icon.** The title is "TwinFM - " plus the active pane's folder name (a Slint
  binding on `left-title`/`right-title`, which Rust sets on navigation and tab switch, and on
  `active-pane`). The icon is a small embedded SVG (`src/ui/icon.svg`: a blue frame with two
  panes) on `Window.icon`; a designed icon is deferred. `tools/soak-m3.ps1` and `soak-m5.ps1` find
  the window by a title starting with "TwinFM - " (or the old "twinfm").
- **About dialog**: F1 or "About TwinFM" at the foot of the sidebar opens a `PopupWindow` with the
  name, the version (`env!("CARGO_PKG_VERSION")`), the MIT line, the Slint license line and
  Slint's `AboutSlint` widget, which is the attribution README section License and CLAUDE.md
  section License call for; CLAUDE.md now says the dialog exists.
- **Driver**: `sidebar toggle` (F9), `sidebar click <row>` (the row's callback; headers count),
  `bookmark add` (Ctrl+D), `bookmark remove <index>` (the menu's callback), `about open` (F1),
  `about close`. stderr has `[sidebar]` and `[settings]` lines.
- **Tests**, 55 to 60: the config round trip (defaults and a full config through TOML and back,
  unknown keys and tables ignored, missing keys defaulted, clamping, malformed text and a wrong
  type rejected, `load` of a malformed file giving the defaults and leaving the `.bad` copy, a
  missing file giving the defaults, the sort-column names); the atomic write (creates the folder,
  replaces an existing file, leaves no `.tmp`); the on-screen check (one monitor, a monitor at
  negative coordinates, partly off every edge, no monitors, and the real monitor list); bookmark
  add, remove and dedupe; the sidebar composition with real `SHGetKnownFolderPath` results (every
  resolved place is an existing folder, Home first) and drive labels; plus the hit-test with a
  sidebar and a collapsed sidebar in `dnd`.

### Decisions

- **The bookmark menu is a Slint `PopupWindow`**, not the shell menu machinery's own-items path:
  `shellmenu::show` needs a folder and items to build an `IContextMenu` for, and a bookmark is
  neither; the popup also runs no modal loop, so the modal-loop rule does not come into it.
- **Start folders are the last ones open**, not fixed settings: with no settings dialog, a field
  that only changes by hand-editing the file would rarely be right. Same reasoning for the sort
  and hidden-file defaults (the last choice made).
- **Scripted runs ignore the settings file** (above).
- **`layout.left`** joined `dnd::Layout`: the panes start right of the sidebar and its edge (or
  of the collapsed strip); the split ratio applies to the panes' own width; the drop target and
  the driver's geometry both use it.
- **The drive list in the sidebar refreshes when the sidebar is shown and on F5**, not on
  `WM_DEVICECHANGE` (Phase 2).
- **"Home"** labels the profile folder.

### Deviations

- The icon is an SVG through `@image-url` (winit and femtovg already enable Slint's `svg`
  feature), not a PNG or ICO.
- `cache-rendering-hint` on the sidebar, which the brief did not ask for (Measurement).
- A malformed settings file is also copied to `.bad`.
- Settings are saved after every navigation too (the start folders changed), debounced like the
  rest.
- The soak scripts' window lookup changed with the title.

### Deferred

- **No settings dialog**: everything is chosen by using the app or by editing `config.toml`.
- **The window's maximised state** is not saved; a window closed maximised reopens at its last
  normal placement.
- **The sidebar is no drop target**, and bookmarks cannot be reordered, renamed or dragged.
- **No device-change refresh** of the drive list.
- **Right-click on a bookmark and middle-click in the sidebar are not scripted**: the driver
  calls `bookmark-remove` and `side-clicked` directly. Manual check: right-click a bookmark,
  choose "Remove bookmark"; middle-click a place, a tab opens on it.
- **A designed icon**; the SVG is a placeholder.
- **The toolbar** from the original M6 line and Explorer's separate "protected operating system
  files" switch (M1 Decisions) were not built.

### Scripted checks

RDP session, default features, M6b build in `target-m6`, soak finished:

- With a scratch `TWINFM_CONFIG`: `sidebar click 4` opened Home (`C:\Users\jtkli`), `bookmark add`
  bookmarked it, `split`, two more `bookmark add` on the same folder reported "is bookmarked
  already", `about open` and `about close`, `bookmark remove 0`, `sidebar toggle` twice (hidden,
  shown, drives re-read); exit 0, no panic. The file then held both start folders, `split = true`
  and the window at 104, 104, 1200 x 800. Screenshots show the sidebar (Local Disk (C:), Google
  Drive (G:), the seven places, the bookmark), the title "TwinFM - jtkli", the window icon, the
  About dialog with the `AboutSlint` logo, and the collapsed strip.
- Restore: the same file edited to `split_ratio = 0.35`, `show_hidden_right = true`,
  `path_bar = "entry"` and a bookmark: the next start opened both panes at 0.35, the right one
  with hidden files (102 entries against 85), both path bars as text, the bookmark in the
  sidebar.
- Malformed (`split = [`): `[settings] ... is malformed, using the defaults (copied to
  ...config.toml.bad): TOML parse error at line 2, column 9`, defaults, exit 0, then a fresh
  file written.
- A read-only test config is left alone (`could not save: ... Access is denied.`), which is how
  a fixed config can be used across scripted runs.
- Ctrl+L with no other TwinFM running: the path entry still had the focus about 2 s later, at
  the screenshot, which confirms the M6a note that the earlier focus loss came from the soak's
  window taking the foreground.

### Measurement

In-app, RDP session, default features, standard gate script
(`sleep 2500;split;sleep 500;wheel 1 -120 16;wheel 0 -120 16;drag 1 400 1000;drag 0 400 1000;sleep 6000;quit`)
on `C:\traces\bigdir`, builds run alternately, defaults (sidebar shown). M4 is `target-m4`
(main before M5), M6a is the M6a commit 539ee1a built into `target-m6a`. UI-thread busy over the
10 s after the split:

| Build | Runs | Mean |
|---|---|---|
| M6b, sidebar not cached (two rounds) | 16.6, 18.0, 19.1, 18.4, 18.1, 16.1, 17.2, 18.1 | 17.7% |
| M4, same rounds | 17.2, 15.5, 17.3, 15.0, 16.6, 16.1, 17.0, 14.8 | 16.2% |
| M6a, second round | 15.5, 18.0, 16.2, 16.7 | 16.6% |
| M6b, sidebar cached | 16.9, 17.0, 16.1, 18.4, 15.9 | **16.9%** |
| M4, same round | 17.2, 16.6, 17.3, 16.6, 15.5 | **16.6%** |
| M6a, same round | 16.1, 16.6, 16.4, 16.4, 17.0 | 16.5% |

The uncached sidebar put M6b 1.5 points above M4 (higher in six of eight pairs); M6a sat 0.4 above.
With the cached layer M6b is 0.3 points above M4 and 0.4 above M6a, inside the spread of either
build (15.5 to 17.3 for M4). No panics in any run. Nothing new runs per frame in Rust: the
sidebar, tab strip, breadcrumb, status bar and title change on navigation, selection, a tab
switch or a sidebar change only.

Gate (2026-10-10, Jeremy at the console, full M6b build `target-m6`, accessibility on, hidden
off): **19.8%** by WPR (1980 sampled ms in the 10 s after the split; in-app 20.3%). Passes the
20% bar with 0.2 point to spare; M3 on the same conditions read 19.6%, so M4 to M6 together
cost about 0.2 point. Trace `C:/traces/twinfm-p1-m6.etl`.

## Phase 1 close

M1 to M6 are in: real entries, icons, file operations, the shell context menu, drag and drop,
and the shell around them (tabs, breadcrumb, status bar, sidebar, bookmarks, settings, About).
The console WPR gate (main thread busy over the 10 s after the split, bar under 20%) is recorded for week-one main (16.5%), M1 (19.9%), M2 per-row (22.3%, failed and reworked), M2 strip (19.3%) and M3 (19.6%); M4 and M5 add nothing per frame and were not run separately; the full M6 build read 19.8%. Deferred to Phase 2, pulled together from
every milestone and deduplicated:

- **Directory watching**, which several gaps share: changes made by shell verbs that finish
  late (M4), by Explorer after a drag out (M5), by other programs, and background tabs (M6a,
  which refresh when shown instead).
- **File operations**: read back the real new name (`PostNewItem`), move the cursor after a
  delete or move, scroll a renamed or new row into view, notice another program taking the
  clipboard (cut marks), progress and a queue inside TwinFM with cancel, Undo (Ctrl+Z), dim the
  cut row's icon (M3).
- **Context menu**: Windows 11's compact menu (`IExplorerCommand`), icons on TwinFM's own items,
  clearing the selection on a right-click in empty space, a scripted check of shell verbs and of
  the rename interception, `;` in wt.exe paths and the PowerShell fallback run live (M4).
- **Drag and drop**: highlight during our own drag, drag images (`IDragSourceHelper`,
  `IDropTargetHelper`), right-button drags, auto-scroll and spring-loaded folders, virtual files
  (`CFSTR_FILEDESCRIPTOR`), mounted folders, a scripted pane-to-pane OLE drop, the sidebar and
  tabs as drop targets (M5, M6a, M6b).
- **Icons and entries**: 32 px icons for a large-icon view, overlays (shortcut and link arrows),
  telling reparse tags apart (OneDrive placeholders against junctions), a timeout or second
  worker for slow icons, not rendering icons for rows scrolled past, free space for network
  drives (M1, M2).
- **Shell and settings**: a settings dialog, tab overflow scrolling, tab drag and reorder and
  restoring tabs at start, a breadcrumb sibling dropdown, middle-click a folder row into a new
  tab, folder sizes in the status bar, the maximised state, bookmark reorder and rename,
  `WM_DEVICECHANGE` drive refresh, a toolbar, Explorer's protected-system-files switch, a designed
  icon (M1, M6a, M6b).
- **Checks that need Jeremy at the console**: the WPR gate runs above; Narrator on selection, the
  icon strip, the name editor and the status notes, menus and the sidebar (M1 to M6); a
  screenshot of a menu (M4); the drag and drop manual check (M5); opening files, the bookmark menu
  and sidebar middle-click (M6a, M6b).

Gate: the full M6 build read **19.8%** by WPR on 2026-10-10. Phase 1 closes under the bar.
