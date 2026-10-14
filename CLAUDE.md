# twinfm

TwinFM: dual-pane file manager for Windows 11 in Rust + Slint. Started as the one-week
prototype `rustfm-proto` (renamed 2026-10-09; stack adopted the same day, see the recommendation
in `PROTOTYPE-RESULT.md`). Crate and exe are `twinfm`; the Slint fork branch keeps its `rustfm` name.
`PROTOTYPE-RESULT.md` is the gate record, `docs/week-one-plan.md` the week-one record
(accessibility fix, renderer decision), `docs/phase-1-plan.md` the Phase 1 record (six
milestones, gate re-run after each), `docs/phase-2-plan.md` the Phase 2 record,
`docs/phase-3-plan.md` the Phase 3 plan and record. Read them before changing the list.

## Gates (run before every commit)
- `cargo test --release` (122 tests; seven touch the real shell and clipboard, and nine of the ten
  `fs::watch` tests, one undo test, the breadcrumb dropdown's test, the reparse enumeration test,
  the search walker's test, the size walker's test, the two archive tests and two virtual-drop
  tests write to scratch folders, see below)
- `cargo build --release` (Slint comes from the `rustfm` branch of the jtklinger/slint fork,
  pinned by rev in `Cargo.toml`: master a1ce0d8 plus the two accessibility patches in
  `docs/patches/`; a fresh clone builds with no manual steps)

## Build variants
- The patched Slint lives at https://github.com/jtklinger/slint, branch `rustfm` (a1ce0d8 +
  `git am docs/patches/000*.patch`). To change it: commit on that branch, push, regenerate
  `docs/patches/` with `git format-patch a1ce0d8`, and bump the `rev` in both `Cargo.toml`
  entries. Every git dependency is pinned by `rev`; never `branch = "master"`.
- `target-*` directories are ignored. Never `git add -A` with an unignored target dir (a
  1.2 GB commit would be the result).
- Unpatched Slint for comparison: `tools/build-master.sh` only. It rewrites the fork
  references to slint-ui/slint at a1ce0d8, builds with the lockfile set aside, restores both
  files, and fails if the binary contains the patch string.
- Accessibility off: `cargo build --release --no-default-features` (isolates the AccessKit cost).
- Both renderers in one binary: `cargo build --release --features skia --target-dir target-skia`;
  select with `SLINT_BACKEND=winit-femtovg` or `winit-skia`. Skia D3D is slower here (22.2% vs
  17.2% main-thread busy) and does not fire the rendering notifier, so the in-app frame timer
  is blank on that path. The renderer stays femtovg.

## Measuring
- In-app: `TWINFM_SCRIPT` (syntax in `src/auto.rs`) drives scenarios; stderr carries `[perf]`,
  `[busy]`, `[split]`, `[enum]`, `[sort]`, `[select]`, `[hidden]`, `[refresh]`, `[icons]`, `[watch]`,
  `[filter]`, `[search]`, `[size]`, `[archive]` lines.
  Since P2-M5 the `[enum] ... done:` line ends with `, reparse: J junction, S symlink, C cloud, A app
  alias, O other` when the listing has reparse points, and the once-a-second `[icons]` line
  carries `overlays N` (icons given the shell's overlay) and `slow N` (renders of 100 ms or
  more) for the period; each slow render also logs `[icons] slow icon <ms> ms: <path>`, and the
  first fetch of each overlay image `[icons] overlay index K fetched at S px`. Driver
  keys include `CtrlH` (hidden files), `F5`/`CtrlR` (refresh), `CtrlA`, `Space`, `Down`,
  `ShiftUp`/`ShiftDown`; `click`/`ctrlclick`/`shiftclick <pane> <row>` select rows; file
  operation steps `copy`, `move`, `delete`, `sdelete`, `newfolder <name>`, `rename <name>`,
  `cc`, `cx`, `cv`, `undo` (Ctrl+Z, key name `CtrlZ`), and `waitops <ms>` to wait for a job and
  the refresh after it; key `Escape` in the list cancels the queued jobs that have not started.
  `[fileops]` lines on stderr (submit, note, done, one `item` line per item the progress sink
  reported, with its HRESULT and the real destination path, and `N operation(s) queued`);
  `[undo]` lines (`recorded "<status>"`, the undo job, `done ... Undid: <status>`, `cannot be
  undone`, `refused: <why>`, `nothing to undo`); `[clipboard]` lines (`format listener added`,
  `another program took the clipboard: cut marks cleared`). Context menu steps: `rclick <pane> <row>`, `rclickempty <pane>`, key names
  `ShiftF10` and `Menu`, each followed by in-menu steps (`sleep`, `menuopen <text>`,
  `menukey <key>`, `menupick <own item>`, `menuclose`) that run from a Win32 timer inside the
  menu's modal loop, because no Slint timer fires while it runs (`src/auto.rs` header);
  `[menu]` lines on stderr. A script must end every menu with `menuclose` or `menupick`, or the
  run waits for a person. Drag and drop steps: `dropfiles <pane> <row> <copy|move|auto>
  <path|path>` calls the drop target's DragEnter and Drop directly (no OLE loop; row -1 is the
  pane's folder; `|` separates paths); in place of `<pane> <row>`, `side <row>` aims at a sidebar
  row (headers count; -1 is the last row, the newest bookmark) and `tab <pane> <index>` at a tab
  (shown with two tabs or more); the mode `menu <copy|move|cancel>` holds the right button, so
  Drop opens the Move here / Copy here / Cancel menu and the in-menu timer picks the item.
  `dragover <pane> <top|bottom> <ms> <path|path>` hovers a drag 6 px inside a list's edge for
  `<ms>` (DragEnter, DragOver every 20 ms, DragLeave; never a drop): the auto-scroll. `dragrow
  <pane> <row> <dx> <dy> [right]` starts a real OLE drag (with the right button given `right`)
  followed by `sleep` and `dragcancel` steps, which run on a helper thread because OLE's drag
  loop runs no Win32 timer. A scripted drag can only be cancelled, never dropped (a scripted drop
  in an RDP session once landed on another program's window; see the M5 record). Since P2-M4 our
  own drag runs from a posted `WM_RUN_DRAG` (outside winit's handler, so the highlight paints),
  and `dragrow` waits for it. `[dnd]` lines on stderr, P2-M4's among them: `drag image 96x96 px, N
  item(s), icon from ...: InitializeFromBitmap = 0x...`, `DoDragDrop starts (left|right button)`,
  `highlight painted inside our own drag loop`, `N frame(s) rendered inside the drag loop`,
  `right-button drag over: pointer grab ended without a release`, `drop menu starts`, `drop menu
  loop idle: app state free=`, `drop menu: default X, chose Y -> effect Z`, `auto-scroll pane N
  up|down: content-y A -> B`, and spots `sidebar row N (...)` and `pane N tab I (...)`. Tabs and
  path bar: `tab <new|close|next|prev|N>` presses Ctrl+T, Ctrl+W, Ctrl+Tab,
  Ctrl+Shift+Tab or Ctrl+N on the active pane (key names `CtrlT`, `CtrlW`, `CtrlTab`,
  `CtrlShiftTab`, `Ctrl1` to `Ctrl9`, and `CtrlL` for the path entry), `tab moveleft|moveright`
  presses Ctrl+Shift+PageUp / PageDown (key names `CtrlShiftPageUp`, `CtrlShiftPageDown`) to move
  the active tab, `tabdrag <pane> <index> <dx>` drags tab `<index>` (0-based) `<dx>` px through
  Slint's pointer path, `midclick <pane> <row>` middle-clicks a row (a folder opens in a new tab),
  `backclick <pane>` and `forwardclick <pane>` post `WM_MOUSEMOVE`, `WM_XBUTTONDOWN` and
  `WM_XBUTTONUP` (XBUTTON1, XBUTTON2) at the middle of the pane's list, so the side buttons take
  winit's real path (that pane becomes active and goes back or forward in its active tab's
  history), `history back|forward` presses Alt+Left / Alt+Right on the active pane (key names
  `AltLeft`, `AltRight`; Backspace and `AltUp` stay up a folder), `scrollpos <pane>` logs the
  pane's list scroll (content-y) and cursor row,
  `crumb <pane> <index>` clicks a breadcrumb button through its callback, `crumbmenu <pane>
  <index>` opens the dropdown of crumb `<index>`'s subfolders (listed on a worker: `sleep` before
  picking), `crumbpick <name>` picks one and `crumbclose` closes it, `pathmode <pane> entry|crumb`
  switches the path bar; `[tab]` (including `strip scrolled to <x>` when the tab strip
  overflows), `[crumb]` and `[open]` lines on stderr, and since P3-M1 `[nav]` history lines:
  `history push <folder> (K back)`, `history back|forward: <from> -> <to> (K back, M forward
  left)`, `history back: nothing to go back to` (and forward), `Back|Forward button (active pane
  was A, now B)`. `TWINFM_PERF_STATUS=1` adds the
  enumeration time to the status bar (the gate tools read `[enum]` on stderr, not the status
  bar). Sidebar and dialogs: `sidebar toggle` (F9), `sidebar click <row>` (headers count),
  `bookmark add` (Ctrl+D), `bookmark remove <index>`, `bookmark rename <index> <title>` (what the
  row's title editor commits; empty goes back to the folder's name), `bookmark move <index>
  up|down`, `about open` (F1) and `about close`, `settings open` (Ctrl+, , key name `CtrlComma`),
  `settings set <field> <value>` (stages a value through the function the dialog's control calls:
  `start_left`, `start_right` take a path, `hidden_left`, `hidden_right`, `type_column`,
  `thumbnails`, `sidebar` take `true|false`, `path_bar` takes `entry|crumb`), `settings ok` and `settings close` (the OK
  and Cancel buttons; `key Escape` cancels through the key path); `window max|restore|close` maximises or restores
  the window, or closes it through `WM_CLOSE` (the close button's path, which saves the settings;
  `quit` does not); `[sidebar]` and `[settings]` lines on stderr, the dialog's among them:
  `dialog opened`, `dialog: <key> <old> -> <new>` per applied change, `dialog closed (ok), N
  change(s)` or `dialog closed (cancel), N change(s) discarded`. Directory watching: `touch <pane> <name>` creates an
  empty file in the pane's folder (or sets an existing one's modified time to now), `rmfile <pane>
  <name>` deletes it, and `waitwatch <ms>` waits until the next refresh started by the watch has
  listed its folder (at most `<ms>`), then 300 ms more for the pass, like `waitops`. `touch` and
  `rmfile` obey the `TWINFM_SOAK` fence as file operations do; outside a soak, point them at
  `%TEMP%` scratch folders only. `[watch]` lines on stderr: `watching <path>`, `no watch for <path>:
  <reason>` (This PC, network drives and shares, removable drives and discs, a folder that cannot
  be opened; logged once per pane and path), `changed: refresh`, `changed while listing: refresh after it`, `changed in the
  background: stale`, `lost <path>: <error>`; `[devices]` lines for volume arrivals and removals.
  Since P3-M2, filter and search: `filter <pane> <text>` puts `<text>` (the rest of the step,
  trimmed, may be empty) in the pane's filter box, opening it without the focus, and calls the
  box's `edited` callback as typing does (applied 150 ms later); `search <pane>` calls Ctrl+Shift+F's
  callback (a recursive name search for the box's text in a new tab; an empty box opens, focused,
  and Return there searches); `waitsearch <ms>` waits until no search tab is still walking (at
  most `<ms>`), then 300 ms for the pass. Key names `CtrlF` (the box, focused), `CtrlShiftF`, and
  any single character typed as itself (`key a`, into whatever has the focus). `[filter]` lines:
  `pane=N "<pattern>" (glob|substring): K of M shown, pass P ms` once per pass while a filter is
  on, `pane=N cleared`. `[search]` lines: `pane=N gen=G start "<pattern>" in <root>`, `batch K (N so
  far)` (at most ten a second), `done: N found, D folders walked, S skipped, in T ms`, `cancelled
  after D folders (<why>)` (tab closed, navigated away, new search, Escape, F5), `skipped N
  folders`; `[watch] no watch for <root>: search results` and `[dnd] pane N tab I shows search
  results: no drop there` for search tabs.
  Since P3-M3, folder sizes: `sizeall <pane>` focuses the pane's list and presses Alt+Return (key
  name `AltReturn`: size every folder of the listing, at most 500, nearest the cursor first);
  `waitsize <ms>` waits until no folder size is queued or walking and the last reply has landed (at
  most `<ms>`), then 300 ms; `click`, `ctrlclick`, `CtrlA`, `Space` select folders, which queues
  them (at most 100 per selection). `[size]` lines: `queued <path> (K in queue)`, `partial <path>:
  X bytes, F files, D folders` (at most four a second per path, from the worker), `done <path>: X
  bytes, F files, D folders in T ms (S skipped)` (`, truncated at depth 64` when it was), `cancelled
  <path> (<why>)` (deselected, navigation, F5, tab in the background, tab closed), `cache cleared
  pane=N (<why>)`, `invalidated <path> (<change>)`, `stale <path>: re-walk in N s` (a changed sized
  folder keeps its old size and is walked again after max(2 s, twice its last walk), once per
  window; `waitsize` does not wait for that), `stale <path>: not selected any more, size dropped`,
  `capped: K of M folders sized`, `pane=N Alt+Return: K of M folder(s) to size`. `[watch]` adds
  `subtree on <path>` / `subtree off <path>` (a tab with sizes cached or in flight watches every
  folder below its folder), `changed below the folder: no refresh`, `subtree overflow <path>: sizes
  dropped` and `subtree disabled for this tab after two overflows`.
  Since P3-M4, zip archives: `nav <pane> C:\dir\x.zip\sub` lists a level of an archive (a zip is
  entered by `key Return` on its row too; `key Return` on a file inside one opens it through its
  PIDL); `dropvirtual <pane> <row> [async] <name=size|...>` drops generated virtual files
  (`FileGroupDescriptorW` + `FileContents`, `name\` a folder, `async` a source asking for
  asynchronous extraction) and `dropvirtual <pane> <row> zip <path|...>` the shell zip folder's own
  data object for paths inside one folder of an archive (`ownzip` in place of `zip`: as our own drag
  out of the archive, which the target turns into the extraction job, `[dnd] DragEnter: N
  item(s) inside an archive (extraction job)`; `idlist`: the shell's `SHCreateDataObject` over those items,
  a Shell IDList Array only); `waitops` also waits for an async drop. A drag our target refuses
  with no files in it logs `[dnd] DragEnter refused: allowed=... keys=...; formats [...];
  QueryGetData ...`.
  The `[enum] ... done:` line of an archive level reads `done: N entries (archive level, central
  directory C entries, read in R ms, cached|fresh), worker ...`. `[archive]` lines: `open <zip>: C
  entries, central directory R ms`, `cache evicted <zip>` (8 archives are cached), `refused: <what>
  (<why>)` (paste, copy or move into, drops into: "writing into"; rename, delete, permanent delete,
  move out, cut, new folder: "editing a zip archive is not supported"; folder sizes), `parse "<path>" -> item ok|failed 0x...` (a shell item or PIDL for a path inside
  an archive). `[dnd]` adds `DragEnter: N virtual item(s) (FileGroupDescriptorW, no CF_HDROP)`,
  `virtual drop: N file(s), M bytes, sync|async`, `virtual drop wrote <path> (K of N)`, `virtual
  drop done in T ms: Ok((files, bytes))`, `virtual drop refused: <why>` (a sync source over 256 MB),
  `virtual drop: GetAsyncMode = ..., StartOperation = ...`, `EndOperation(...)`, `the zip folder's
  data object: SetAsyncMode(TRUE) = ...`; `[watch] no watch for <path>: inside an archive`; `[menu]
  inside an archive: no IContextMenu2/3 routing`. Fixtures Jeremy keeps for this:
  `C:\traces\bigdir.zip` (the 40,000 files of `C:\traces\bigdir`, made with
  `[System.IO.Compression.ZipFile]::CreateFromDirectory`; not in the repo).
  Since P3-M5, thumbnails: `settings set thumbnails true|false` then `settings ok` (or
  `TWINFM_CONFIG=<a read-only file with thumbnails = true>`) switches both panes to the 32 px lists
  and lists them again; no new driver step. `[settings] dialog: thumbnails <old> -> <new>`; `[icons]
  list geometry: 32 px rows, 32 px icons` (or `22 px rows, 16 px icons`) once per change and at a
  start with thumbnails on; `[icons] thumbnail <ms> ms: <path>` for a thumbnail of 100 ms or more
  (counted in `slow`, as `slow icon` lines are); `[icons] thumbnail cap reached: N thumbnails held,
  new ones get their type's icon` once (20,000). The once-a-second `[icons]` line carries `thumbs N
  avg M ms dropped D` after `slow N`: thumbnail requests rendered in the period (from the shell's
  thumbnail cache, or the icon when there is none; not counted in `rendered`) and their average,
  and thumbnail requests the workers dropped because the list had scrolled away from their row
  (`thumbs 0 avg 0.00 ms dropped 0` with thumbnails off). Fixture: `%TEMP%\twinfm-p3m5-thumbs\`
  (200 copies of `C:\Windows\Web\Wallpaper`, `Screen` and `4K` JPEGs, `photo-000.jpg` on; made
  by hand, not in the repo; a new copy is cold in the system thumbnail cache).
- Settings: `%APPDATA%\twinfm\config.toml` (`src/settings.rs`). **A scripted run
  (`TWINFM_SCRIPT` set) neither reads nor writes it**: it starts from the defaults, so the gate,
  soak and check scripts are deterministic and never touch Jeremy's file (a saved `split = true`
  would make the gate's `split` step close the second pane). `TWINFM_CONFIG=<path>` points any run,
  scripted or not, at another file; a test config used across runs is rewritten at exit (make it
  read-only to keep it fixed). Keys since P2-M3: `left_tabs` / `right_tabs` (each pane's tab
  folders, left to right) with `left_active` / `right_active` (the active index), restored at
  start (the right pane's only with `split`; empty or missing lists fall back to `start_left` /
  `start_right`, a command-line folder replaces the left pane's tabs); bookmarks as `[[bookmark]]`
  tables with `path` and an optional `title` (an old `bookmarks = [...]` list still loads and is
  rewritten as tables by the next save); `maximized` in `[window]`, with x, y, width and height
  then holding the normal placement. Since P3-M1 the settings dialog (Ctrl+, or "Settings" at the
  foot of the sidebar) sets `start_left` / `start_right` (no longer overwritten by every save),
  hidden files per pane, `show_type` (the Type column, default true; each pane chooses its header and
  its list (`FileListHost`), with or without the Type cell, once, never per row; off, a saved
  `type` sort becomes `name`), `sidebar_visible` and
  `path_bar`; since P3-M5 `thumbnails` (default false: 22 px rows and 16 px icons; true: 32 px rows,
  32 px icons, and thumbnails for files in `icons::THUMB_EXTS`; both panes, four list components
  chosen per pane by `show_type` x `thumbnails`, never per row). `SLINT_ACCESSKIT_STATS=1` prints accessibility
  tree-update counters once a second; `SLINT_ACCESSKIT_UPDATE_DELAY_MS` overrides the 50 ms
  coalescing (0 = pre-patch behaviour).
- File operation soak: `tools/soak-m3.ps1 -Minutes N` builds a scratch tree in
  `%TEMP%\twinfm-m3\`, loops copy/rename/move/recycle/permanent delete/new folder/clipboard
  cycles between the panes with the bigdir/System32/Manifests scroll scenario in between, and
  reports exit code, `panicked` count and working set (CSV every 30 s). It runs the app with
  `TWINFM_SOAK=1`, which adds `FOF_NOCONFIRMATION` to every job and refuses any file operation
  outside `%TEMP%\twinfm-m3` and `%TEMP%\twinfm-m5`. Never set `TWINFM_SOAK` otherwise. Each cycle
  adds one Recycle Bin entry and overwrites the clipboard. `tools/soak-m5.ps1 -Minutes N` does the
  same in `%TEMP%\twinfm-m5\` and adds, per cycle, a `dropfiles` copy and move and a scripted
  context menu on a row and on empty space; it also reports drops, menus and in-menu ticks.
  `tools/soak-p2m4.ps1 -Minutes N` is the M5 soak plus, per cycle, a `dropfiles` onto a sidebar
  bookmark (`side -1`) and onto a tab (`tab 0 1`), a drop menu's "Copy here" and "Cancel"
  (`menu copy`, `menu cancel`), and a right-button `dragrow` cancelled with `dragcancel`; it
  reports drops by kind, drop menus, right-button drags (cancelled, and dropped, which must be 0)
  and drag images besides the M5 counts.
  `tools/soak-p2m1.ps1 -Minutes N` keeps both panes on watched folders in `%TEMP%\twinfm-m5\watch\`
  (2,000 files each) and scrolls them, with `touch`/`rmfile`/`waitwatch`, tab switches and Ctrl+R,
  while a PowerShell writer job creates, renames, deletes and appends to files there (a few per
  second, a 200-operation burst every 15 s); it reports `[watch]` and `[refresh]` counts besides
  exit code, panics and working set. It uses neither the clipboard nor the Recycle Bin.
- Tests that touch the desktop: `fileops::tests::worker_runs_real_shell_operations` runs real
  `IFileOperation` jobs in `%TEMP%\twinfm-m3-test-<pid>\` (and checks the progress sink's
  results and an undo round trip), `fileops::tests::cancelled_queued_jobs_never_run` makes one
  folder through the shell in `%TEMP%\twinfm-m3-test-queue-<pid>\`,
  `undo::tests::check_refuses_missing_and_non_empty` only reads folders it makes in
  `%TEMP%\twinfm-m3-test-undo-<pid>\` (none of the three uses the Recycle Bin), and
  `clipboard::tests::real_clipboard_round_trip_overwrites_clipboard` replaces the clipboard's
  contents and leaves it empty, and `dnd::tests::shell_data_object_offers_hdrop` asks the shell
  for a data object over two files in `%TEMP%\twinfm-m5-test-<pid>\`, and
  `dnd::tests::drag_source_helper_takes_a_bitmap` makes the shell's drag-drop helper store a drag
  image on a data object over a file in `%TEMP%\twinfm-m5-test-img-<pid>\`. The `fs::watch` tests create,
  rename, delete and change attributes of files in `%TEMP%\twinfm-p2m1-test-<pid>\` (removed
  afterwards); they touch neither the clipboard nor the Recycle Bin.
  `pathbar::tests::subfolders_for_the_crumb_dropdown` makes folders (two of them hidden and
  system) in `%TEMP%\twinfm-p2m3-crumbs-<pid>\` and removes them.
  `fs::enumerate::tests::tells_reparse_points_apart` makes a junction (`cmd /c mklink /J`) and
  tries a file symbolic link in `%TEMP%\twinfm-enum-reparse-test-<pid>\`, and
  `icons::tests::worker_renders_exe_drive_and_extension` makes a junction in
  `%TEMP%\twinfm-p2m5-test-<pid>\` and asks the shell for its icon and overlay, and since P3-M5
  writes a 16 x 16 PNG there and asks for its thumbnail (which adds an entry to the system
  thumbnail cache); both remove their folders. `fs::enumerate::tests::search_walks_the_tree_without_links_or_hidden_folders`
  makes a tree with a hidden folder, a junction to an ancestor (`cmd /c mklink /J`) and 620 small
  files in `%TEMP%\twinfm-p3m2-test-<pid>\`, and removes it.
  `fs::sizes::tests::walker_counts_hidden_files_and_skips_links` makes a tree with hidden and
  system files, a junction to an ancestor and a folder whose listing is denied to Everyone
  (`icacls /deny *S-1-1-0:(RD)`, removed again before the tree is deleted) in
  `%TEMP%\twinfm-p3m3-test-<pid>\`, and removes it. The subtree watch's test
  (`subtree_watch_tells_deep_changes_from_top_ones`) uses the `fs::watch` scratch folder.
  `fs::archive::tests::central_directory_levels` writes zips (nested, folder only, empty,
  non-ASCII, zip64, corrupt, and nine more for the cache's eviction) in
  `%TEMP%\twinfm-p3m4-test-<pid>\`, `archive_paths_are_decided_by_the_file_system` a zip-named file
  and folder in `%TEMP%\twinfm-p3m4-split-<pid>\`; `fileops::tests::shell_items_inside_a_zip_extract`
  asks the shell's zip folder for shell items inside a zip and extracts two of them with a real
  `IFileOperation` copy in `%TEMP%\twinfm-p3m4-shell-<pid>\` (no Recycle Bin);
  `dnd::tests::zip_folder_data_object_offers_virtual_files` asks the zip folder for its data object
  and extracts from it in `%TEMP%\twinfm-p3m4-dnd-<pid>\`; `virtual_files_round_trip` and
  `async_virtual_drop_on_a_worker` write virtual drops into `%TEMP%\twinfm-p3m4-virtual-<pid>\` and
  `%TEMP%\twinfm-p3m4-async-<pid>\`. All remove their folders.
- Numbers that decide anything come from console WPR runs Jeremy performs elevated
  (`tools/wpr-gate.ps1` for one variant, `tools/wpr-compare.ps1 -Set accessibility|renderer`
  for several), analysed without elevation by `python tools/analyze-trace.py <etl> --symbols
  --exe <the traced twinfm.exe>`. The script finds `xperf` on PATH or at the Windows Performance
  Toolkit's default install location.
- Gate binaries are built into their own `--target-dir target-<name>` from a detached
  `git worktree add` checkout, so an agent's builds in `target` do not confound them, and a
  running soak locks `target\release\twinfm.exe`. `analyze-trace.py --symbols` resolves twinfm
  frames only if the build's `target-<name>\release` is on `_NT_SYMBOL_PATH` (the exe's embedded
  pdb path is not searched once that variable is set); `--exe` adds that directory itself.
- Harness-launched runs flip between RDP 32 Hz and console 60 Hz presentation without notice,
  so in-app busy numbers taken minutes apart are not comparable. Record Narrator on/off with
  every run (it adds about 6 points of busy).
- Close every TwinFM window before a gate run: a pre-existing one appears in the trace with
  start "MIN" (the analyzer picks the process that started inside the trace, but the extra
  process still shares the machine). Single runs of one unchanged binary ranged 19.6 to 22.8
  in one afternoon (2026-10-12), so compare only within a session and never against a
  recorded number. The gate protocol since P2-M5 (agreed 2026-10-12): three runs per binary,
  interleaved reference, candidate, reference, candidate, reference, candidate, compared by
  median (`tools/wpr-triplet.ps1 -Reference <name> -Candidate <name> -Tag <tag>` runs all six
  from one elevated console, builds named by their `target-<name>` dir, and refuses to start
  while a TwinFM window is open); a gap counts only when every reference run sits below every candidate run or the
  medians differ by more than one point. The reference binary is the last merged milestone's
  gate build (`target-p3m4`, 07ca56c, after P3-M4); the previous one is deleted once the new one
  has served as a reference. Console checks run AFTER the triplet, never before: a recursive
  search of `C:\` before the P3-M2 gate left Defender busy and lifted the whole machine five
  points for the run, while sessions with the checks after were the quietest yet. A running
  TwinFM locks its exe, so a rebuild into `target-<name>` fails with "Access is denied" while
  Jeremy's console window from that build is open: ask him to close it first.
- Paths handed to `twinfm.exe` from Git Bash must be Windows-style (`cygpath -w`): a
  forward-slash path fails in `SHParseDisplayName` (0x80070057) and a scripted drag never starts.
- PowerShell's `2>` redirect writes UTF-16 with wrapped lines; convert with
  `iconv -f UTF-16LE -t UTF-8` before grepping a console log.
- `target\release` is Jeremy's everyday binary and is rebuilt only by hand: rebuild it after
  every merge (`cargo build --release` on main) and say so.
- `tools/uia-dump.ps1` shows what a screen reader sees (names, selection, screen rectangles).
  Elements with no `accessible-role` are invisible to it; rows use the StandardListView
  layering (base component holds the single `for`, derived adds the zero-width FocusScope and
  `accessible-delegate-focus`).

## License
The app is MIT. Slint is used under its Royalty-free 2.0 license, which allows a modified
Slint inside an application with attribution (the badge in `README.md`, and since M6b the
`AboutSlint` widget in the About dialog, F1 or "About TwinFM" at the foot of the sidebar). The
patches and fork commits are offered under MIT; see README § License before changing any of this.

## Delivery
Branch, PR, squash-merge on Jeremy's go-ahead, per the projects-root `CLAUDE.md`.
**Upstreaming is deferred** (2026-10-09): `docs/upstream-issue-draft.md` and `docs/patches/`
are the record. Do not post to slint-ui/slint from a session; Jeremy decides when.
