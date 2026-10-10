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

Gate: pending Jeremy's console WPR run.

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

Gate: pending Jeremy's console WPR run.
