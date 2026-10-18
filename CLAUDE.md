# twinfm

TwinFM: dual-pane file manager for Windows 11 in Rust + Slint. Started as the one-week
prototype `rustfm-proto` (renamed 2026-10-09; stack adopted the same day, see the recommendation
in `PROTOTYPE-RESULT.md`). Crate and exe are `twinfm`; the Slint fork branch keeps its `rustfm` name.
`PROTOTYPE-RESULT.md` is the gate record, `docs/week-one-plan.md` the week-one record
(accessibility fix, renderer decision), `docs/phase-1-plan.md` the Phase 1 record (six
milestones, gate re-run after each), `docs/phase-2-plan.md` the Phase 2 record,
`docs/phase-3-plan.md` the Phase 3 record (closed 2026-10-14; § Phase 3 close holds what is
deferred and the Phase 4 candidates), `docs/phase-4-plan.md` the Phase 4 record (the large-icon
grid with thumbnails, three milestones, closed 2026-10-15; § Phase 4 close holds what is deferred
and the Phase 5 candidates; settings triplets are part of its gate), `docs/phase-5-plan.md` the
Phase 5 record (the Phase 2 lists finished: directory watching, file operations, drag and drop;
three milestones, closed 2026-10-16; § Phase 5 close holds what is deferred and the Phase 6
candidates), `docs/phase-6-plan.md` the Phase 6 record (cloud marks and the icon items, the views
finished, the toolbar and the polish bundle; three milestones, closed 2026-10-18; its first
section quotes Jeremy's kickoff decisions, § Calls made lists the lead's, § Recorded for Phase 7
holds the shell verbs item, and § Phase 6 close holds what is deferred and the Phase 7 candidates),
`docs/phase-7-plan.md` the Phase 7 plan (the context menu finished, archives beyond zip, the
polish bundle; four milestones, opened 2026-10-18; its first section quotes Jeremy's kickoff
decisions and § Calls made lists the lead's). Read them before changing the list.

## Gates (run before every commit)
- `cargo test --release` (194 tests; nine touch the real shell and clipboard, one of them the Recycle
  Bin, three more only read the machine (P6-M1: the sync root registry and the system drive's label,
  imageres.dll's status glyphs, the shell's overlay for a path that does not exist), and fourteen of the
  eighteen `fs::watch` tests, the `stat_entry` test, one undo test, the breadcrumb dropdown's test, the reparse enumeration test,
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
  `cc`, `cx`, `cv`, `undo` (Ctrl+Z, key name `CtrlZ`), since P5-M2 `redo` (Ctrl+Y, key name `CtrlY`:
  the newest undone operation again; a recycle-and-undo cycle is `delete`, `waitops`, `undo`,
  `waitops`, `redo`, `waitops`), and `waitops <ms>` to wait for a job and
  the refresh after it; key `Escape` in the list cancels the queued jobs that have not started.
  `[fileops]` lines on stderr (submit, note, done, one `item` line per item the progress sink
  reported, with its HRESULT and the real destination path, and `N operation(s) queued`; since
  P5-M2 `progress id=N P%` at most four times a second while a job runs, from the sink's
  `UpdateProgress`, the same percentage the status bar shows after the running job's note, and
  `undelete <$R path> -> <original>: verb ok (T ms) | verb cancelled 0x... | verb failed 0x...:
  moved back | verb not reachable (<why>): moved back | already back, nothing moved | moved back
  failed 0x... | refused (<name> is in the way) | not a Recycle Bin path | quit while waiting:
  stopped` for a restore from the Recycle Bin: the bin's own "undelete" verb first, a move back
  from `$Recycle.Bin` under the original name as the fallback, never after a cancel or when the
  original path is taken (item HRESULT 0x800700B7));
  `[undo]` lines (`recorded "<status>"`, the undo job, `done ... Undid: <status>`, `cannot be
  undone`, `refused: <why>`, `nothing to undo`; since P5-M2 a delete to the Recycle Bin is
  undone by a `Restore` job (refused with `<name> is in the way` when the original path is
  taken again, `is no longer there` when the bin was emptied) and a move the shell renamed on
  collision by a `MoveAs` back under the old names; the redo stack: `redo recorded "<status>":
  <job>` when an undo job finishes, `redo "<status>" with <job>` (Ctrl+Y), `redo done ...
  Redid: <status>`, `redo "<status>" refused: <why>`, `nothing to redo`, `redo cleared (N)` when
  a new job that changed something empties it, `busy: N operation(s) in flight` (Ctrl+Z or Ctrl+Y
  while a job runs: refused with the note "Wait for the running operation to finish", nothing
  popped), `record kept: "<status>"` (an undo or redo job that ran nothing, cancelled while
  queued or refused by the soak fence, goes back on its stack); the soak counts `redo "` lines as redo attempts, so no other line
  carries that substring); `[clipboard]` lines (`format listener added`,
  `another program took the clipboard: cut marks cleared`, since P7-M1 `pane=N Escape: cut
  cancelled, marks cleared, clipboard emptied | clipboard left (not TwinFM's cut any more)`: Escape
  in a list with no search to stop and no queued job cancels a cut before it clears the selection).
  Since P7-M1 a shell command picked in the menu runs on the verb worker (`src/verbs.rs`, an STA
  thread with a message loop that builds the menu again for the same items, `shellmenu::run_verb`,
  and invokes by the canonical verb, or by the id when the command has none and the rebuilt item
  there has the same label, `shellmenu::choose`; no owner window), except the canonical verbs
  `delete`, `cut`, `copy`, `paste` and `rename`, which run TwinFM's own commands (`shellmenu::route`;
  the menu's Delete with Shift held is Shift+Delete; inside an archive they are refused as the keys
  are). `menuverb <pane> <verb>` feeds a canonical verb as if picked in the menu for the cursor row's
  selection (the background with no cursor row); `sleep:<ms>` is a test verb that sleeps on the
  worker, and `waitops` waits for the worker too. `[menu]` adds `shell verb "<verb>" routed to
  TwinFM's <cmd>`, `verb "<verb>" ("<label>") queued on the verb worker, id N`, `InvokeCommand
  verb="..." offset=N label="..." by verb = 0x... | by offset = 0x... | refused: <why> | slept |
  failed: <why> in T ms (verb worker, id N)`, `InvokeCommand by verb "<verb>" = 0x...: by offset
  instead`, `verb worker CoInitializeEx(APARTMENTTHREADED) = 0x...`, and at exit `quit while N verb
  job(s) queued or running: not waited for`. Context menu steps: `rclick <pane> <row>`, `rclickempty <pane>`, key names
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
  and `dragrow` waits for it. `[dnd]` lines on stderr, P2-M4's among them: `drag image 96x96 px,
  64 px icon, N item(s), icon from ...: InitializeFromBitmap = 0x...` (since P4-M3 with the icon
  size; `128x128 px, 96 px icon` out of the grid), `DoDragDrop starts (left|right button)`,
  `highlight painted inside our own drag loop`, `N frame(s) rendered inside the drag loop`,
  `right-button drag over: pointer grab ended without a release`, `drop menu starts`, `drop menu
  loop idle: app state free=`, `drop menu: default X, chose Y -> effect Z`, `auto-scroll pane N
  up|down: content-y A -> B`, and spots `sidebar row N (...)` and `pane N tab I (...)`. Since
  P5-M3 the drag image line's `icon from` reads `the worker` when the icon asked of the icon worker
  at the press (a `drag:` key, never in the icon table) had landed before the drag started, else
  `the shell` as before. `IDragSourceHelper2::SetFlags(DSH_ALLOWDROPDESCRIPTIONTEXT) = 0x...` once
  per process, at the first drag. `drop description: Move to "<folder>" | Copy to "<folder>" |
  none | cleared` when the description our target sets on the drag's data object changes (none:
  the drop is refused; cleared: DragLeave or Drop gives the text back to the source), and `drop
  description: SetData failed 0x...` once per drag whose data object refuses it (the driver's
  `dropfiles` and `dragover` objects do, 0x80004001). `volumes: sources on <key>` once per
  DragEnter with files, and `volumes: dest <path> on <key>` when a new destination's volume is
  looked up (`GetVolumePathNameW`; the key is the lowercase mount point without its trailing
  separator, `c:` or `c:\mnt\data`, or `\\server\share`). Also since P5-M3: `dropfiles crumb
  <pane> <index> <mode> <paths>` aims at a breadcrumb crumb's label (0 is This PC; a negative index
  counts from the end, -1 the folder itself, -2 its parent; the place is the one Slint reported,
  below), and `dragover` hovers other spots: `dragover row <pane> <row> <ms> <paths>`, `dragover
  tab <pane> <index> ...` and `dragover crumb <pane> <index> ...` at the centre of that spot (as
  `dropfiles` aims; the hover's DragOver every 20 ms is the spring-load's clock), `dragover side
  <top|bottom> ...` 6 px inside the sidebar list's top or its bottom above the footer, `dragover
  strip <pane> <left|right> ...` 6 px inside a tab strip's end (shown with two tabs or more).
  Spring-loaded folders: a drag held `dnd::SPRING_MS` (1000 ms) over a folder row or tile, or a
  tab other than the active one, opens it in that pane (a navigation, or the tab switch, run from
  the event loop), for drags from anywhere; once per spot until the pointer leaves it; never a
  sidebar row, a crumb, a file row, empty space, a search tab, This PC's tab or an archive level,
  nor a spot that refuses the drop. Lines: `spring armed: pane N row R (<folder>)` or `pane N tab
  I (<folder>)`, `spring-loaded: pane N opens <folder> after T ms` or `pane N shows tab I after T
  ms`, `spring disarmed (moved away | DragLeave | Drop)` (an armed spring only, one that had not
  fired). The spot `pane N crumb I (<folder>)` (the crumb's folder; This PC's crumb and a search
  tab's own crumb are no folder, a crumb inside a zip archive is refused as `[archive] refused:
  drop into ...`). `auto-scroll sidebar up|down: content-y A -> B` (one 24 px row per 50 ms while
  the drag is within 24 px of the sidebar list's top or bottom, bounded by the ListView's own
  content height) and `auto-scroll pane N tab strip left|right: x A -> B` (one tab per 50 ms
  within 24 px of the strip's ends, only when its tabs overflow it). `[crumb] pane=N geometry: K
  crumbs` once per breadcrumb rebuild, when every crumb has reported its place (`changed` callbacks
  in pane.slint's `CrumbButton` and the crumb row, kept in `main.rs` `CRUMB_GEOM`; about 16
  reports in the 15 s gate scenario, none while the lists scroll). Tabs and
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
  `thumbnails`, `sidebar` take `true|false`, `path_bar` takes `entry|crumb`, since P6-M2 `view_left`,
  `view_right` take `details|large|medium`), `settings ok` and `settings close` (the OK
  and Cancel buttons; `key Escape` cancels through the key path); `window max|restore|close` maximises or restores
  the window, or closes it through `WM_CLOSE` (the close button's path, which saves the settings;
  `quit` does not); `[sidebar]` and `[settings]` lines on stderr, the dialog's among them:
  `dialog opened`, `dialog: <key> <old> -> <new>` per applied change, `dialog closed (ok), N
  change(s)` or `dialog closed (cancel), N change(s) discarded`. Directory watching: `touch <pane> <name>` creates an
  empty file in the pane's folder (or sets an existing one's modified time to now), `rmfile <pane>
  <name>` deletes it, and `waitwatch <ms>` waits until the next refresh started by the watch has
  listed its folder (at most `<ms>`), then 300 ms more for the pass, like `waitops`. `touch` and
  `rmfile` obey the `TWINFM_SOAK` fence as file operations do; outside a soak, point them at
  `%TEMP%` scratch folders only. Since P5-M1 `mvfile <pane> <old> <new>` renames a file there (the
  same fence), `waitwatch` also returns when a watch window has been applied as row edits, and
  `waitedit <ms>` waits for an applied window only (so a script can assert the edit path, not the
  fallback, was taken). `[watch]` lines on stderr, every one `pane=N tab=T` since watches are per
  tab: `watching <path>`, `parent watch on <parent>` (the active tabs: the folder's own rename or
  delete), `no parent watch for <path>: <why>`, `no watch for <path>: <reason>` (This PC, discs, a
  search tab, an archive level, a folder that cannot be opened, `watch cap reached (8)`, `stalled:
  no watch` for a share whose open took over 3 s, then refused at once for 30 s; logged once per
  tab and path), `records R, changes C, window W ms[, overflow]` per delivered window, `back-off:
  window W ms` (250 ms doubling to 2 s while changes arrive within 1 s of the last delivery, back
  to 250 after a quiet second), `N records in the window: overflow` (past 1,000), `applied: +A -R
  ~C (stat S ms, apply T ms)` (the window became row edits), `R records netted to nothing`, `R
  records explained by the operation: no refresh` (after our own job), `fallback: <reason> (R
  records): refresh | refresh after it | stale` (the whole-folder refresh, the three outcomes of
  P2-M1; reasons `overflow`, `not a folder listing`, `listing in flight`, `pass in flight`,
  `network path`, `unknown name <name>` (from `land_edits`, after the stats), `folder gone`, `C
  changes over the cap` (500), `subtree watch`), `edits dropped: <why>: <outcome>` (the listing
  changed, a pass was requested, the path changed, tab closed), `watched in the background`, `past the cap: stale when shown`,
  `folder gone on show: refresh`, `folder renamed: <from> -> <to>` with `[nav] pane=N path follows
  the rename: <to>`, `lost <path>: <error>` (`the folder was deleted`, `the folder was moved away`,
  `the drive was removed`, or an OS error) with `watch lost (<error>): refresh | stale`, `suspended
  for removal: <path>` and `resumed after a failed removal: <path>` (a removable drive's handle
  closed for "Safely remove" and reopened when it failed); `[devices]` lines for volume arrivals
  and removals and `query remove for a watched handle: suspended | no reply within 1 s | the
  watcher has stopped`.
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
  Since P4-M1, the large-icon grid: `view details|icons` presses Ctrl+Shift+1 / Ctrl+Shift+2 on the
  active pane (key names `CtrlShift1`, `CtrlShift2`; the setting is per pane, `view_left` /
  `view_right`, so a gate run with the grid on takes `TWINFM_CONFIG=C:\traces\grid.toml`, read-only,
  both panes `"icons"`). In the grid every step that takes a row (`click`, `ctrlclick`, `shiftclick`,
  `dblclick`, `midclick`, `rclick`, `dropfiles <pane> <row>`) aims at the tile of visible entry
  `<row>` (`row / cols` grid rows down, `row % cols` tiles across, 96 x 120 px each); `rclickempty`
  and `dropfiles <pane> -1` work as before. Key names `Up`, `Down`, `Left`, `Right`, `PageUp`,
  `PageDown`, `Home`, `End`, `ShiftLeft`, `ShiftRight` move the cursor in the focused list (in the
  grid: Left/Right one entry, Up/Down one grid row, PageUp/PageDown a viewport of grid rows; the
  details list takes Up, Down, Home, End as before and ignores the rest). `[view]` lines on stderr:
  `pane=N starts as large icons`, `pane=N details -> large icons (K tiles to a row, R grid rows),
  scroll A -> B (entry E first)` (and back), `pane=N already <view>`; `[grid] pane=N K tiles to a
  row, R grid rows` when the pane's width changes the count (split, resize, the first layout) while
  it shows the grid. With the grid on, the once-a-second `[icons]` line's `compose N strips` counts
  grid ROWS composed (one 96 x cols by 120 px image per grid row, in a ring of slots; a scroll of one
  grid row composes one), not whole strips. Hit testing (`dnd::hit`), the drop highlight, the rename
  editor, the keyboard menu point (`shellmenu::tile_menu_point`) and the driver read the pane's
  view and tile count from the window (`window_layout`: `Layout::grid`).
  Since P4-M2, thumbnails in the tiles: a grid pane with `thumbnails` on asks for its `thumb:` keys
  at the grid's icon size (96 px at 100%) through the P3-M5 path, so the `[icons]` line's `thumbs N
  avg M ms dropped D` counts them; no new driver step or line. A gate run with the grid and
  thumbnails on takes `TWINFM_CONFIG=C:\traces\grid-thumbs.toml` (read-only: both panes `"icons"`,
  `thumbnails = true`); its stderr shows `[settings] file ... (read)`, `[view] pane=0 starts as
  large icons` and `[icons] list geometry: 32 px rows, 32 px icons`. The shell's icon bitmaps can
  carry straight-colour edge pixels (the 96 px folder icon's top row: (251, 187, 23) at alpha 64),
  which `bgra_to_rgba` clamps to their alpha; the placeholders are drawn at the requested size.
  Since P4-M3, the grid's rubber band and drag image: `band <pane> <x0> <y0> <x1> <y1> [ctrl|shift]
  [hold <ms>]` presses the left button at (x0, y0) of the pane's list (px from its top-left corner;
  aim at empty grid space: the gap right of the last column, 12 px short of the list's edge, the
  gap after the last tile of a partial last grid row, or below the rows), moves to (x1, y1) in 8 px
  steps and releases, through Slint's pointer path; `hold` keeps the button down for `<ms>` and runs
  the next steps meanwhile (`key Escape` cancels the band; the edge auto-scroll runs; no `waitops`
  or `waitwatch` inside a hold). The band (`src/band.rs`) starts past the drag threshold, selects
  every tile it touches (a plain drag replaces the selection, Ctrl toggles the touched tiles against
  the selection at the press, Shift adds them), scrolls the list a grid row every 50 ms while the
  pointer is within 24 px of the list's top or bottom edge or beyond it, and ends on the release; a
  press released under the threshold clears the selection (Explorer) unless Ctrl or Shift is held.
  `[band]` lines: `pane=N start at (x, y), Replace|Toggle|Add` (content coordinates), `pane=N K
  selected, rect (x, y) W x H` (at most four a second), `auto-scroll pane N up|down: content-y A ->
  B`, `pane=N done: K selected` (since P6-M2 `, cursor R`), `pane=N cancelled (Escape|the pointer grab ended): K selected`. A
  drag out of a grid pane carries a 128 px drag image (at 100%) with the pressed tile's 96 px
  picture, the thumbnail or the type icon the tile shows (`icon from the tile`; a placeholder tile
  asks the shell); the `[dnd] drag image` line reads `WxW px, I px icon, N item(s), icon from
  ...` (the details list's stays 96 px with a 64 px icon from the shell). Shift and Ctrl across
  tiles, drops onto a tile and onto empty space and the drag auto-scroll by grid rows are P4-M1's.
  Since P6-M1, cloud marks (`src/cloud.rs`, `icons::Cloud`, `icons::Mark`): no new driver step (a
  `nav` to a OneDrive folder or to `G:\My Drive` is the check). A listing is a cloud location when its
  folder is under a registered sync root (`HKLM\...\Explorer\SyncRootManager\*\UserSyncRoots`, read
  at start and on every device change; OneDrive and Phone Link's CrossDevice here) or on a volume
  named "Google Drive"; This PC, archive levels and search tabs never are. Keys there:
  `ext:<mark>:.txt`, `folder:<mark>`, `thumb:<mark>:<path>|<modified>|<size>`, `cloud:<mark>:<path>`
  (a full-path key), the mark `s<N>` (a sync root's `System.StorageProviderState`, drawn with the
  imageres.dll glyph Explorer's Status column uses) or `o<N>` (the shell's overlay index, Google
  Drive's handlers); a sync root file's state comes from its attributes on the enumeration worker, a
  sync root folder and every Google Drive entry first carry `look:<s|o>:<plain key>|<path>` and the
  icon workers look the mark up when the row is on screen (one property read, about 23 ms, or one
  `SHGFI_OVERLAYINDEX` call, about 5 ms), after which the row takes the marked key. Links and reparse
  points keep their own keys and arrow. A thumbnail for a cloud-only file (state 1) and for any
  Google Drive file is asked with `WTS_INCACHEONLY`. `[cloud]` lines: `sync roots: N (<roots>)` at
  start and after a device change, `volume <key> named "<name>" (google drive|not cloud)` or `volume
  <key> has no name (not cloud)` once per volume, `pane=N <folder>: sync root|google drive` per
  listing of a cloud location. `[icons]` adds `status glyph state S (imageres -R) fetched at G px over
  P px` once per state and size, `slow cloud lookup <ms> ms: <path>` (100 ms or more, counted in
  `slow`), and the once-a-second line's `cloud N` (lookups made in the period, after `dropped D`);
  `overlays N` counts every icon drawn with a mark or an overlay, so a refresh of a cloud folder
  whose keys are known shows `cloud N` lookups and `overlays 0`.
  Since P6-M2, the views finished: `view medium` presses Ctrl+Shift+3 on the active pane (key name
  `CtrlShift3`): the grid at medium tiles, 80 x 72 px with the shell's 48 px icon (`icons::TileGeom`,
  `MEDIUM`; `LARGE` is P4-M1's 96 x 120 with 96 px), chosen per pane and saved as `tile_left` /
  `tile_right`; a gate run with it takes `TWINFM_CONFIG=C:\traces\grid-medium.toml` (read-only: both
  panes `"icons"` at `"medium"`), whose stderr shows `[view] pane=0 starts as medium icons`. The
  medium tile is its own component (`FileGridMedium`, a copy of `FileGrid` with its constants), picked
  by `FileListHost` once per pane; the pane's overlays, `window_layout` (`Layout::tile`), `dnd::hit`,
  the band, the menu point and the driver read the tile from the pane (every step that takes a row
  aims at a medium tile as it does at a large one). `[view]` lines name the view as `details`, `large
  icons` or `medium icons` (`pane=N large icons -> medium icons (K tiles to a row, R grid rows) ...`).
  A view switch puts every entry of the pane's tabs back to its placeholder so the icons are asked
  again at the new size (before P6-M2 the first screenful of a details list showed its 16 px icons
  blown up in the grid). PageUp and PageDown move the cursor a viewport of rows in the details list
  too (key names `PageUp`, `PageDown`, `ShiftPageUp`, `ShiftPageDown`; `scrollpos` reads the result);
  Ctrl+Shift+PageUp / PageDown pass to the tab strip from either list. The band moves the cursor to
  the touched tile under its moving corner and anchors Shift there (`band::cursor`; `[band] ... done:
  K selected, cursor R`). A thumbnail in a tile (large or medium) gets a one-pixel grey frame
  (`icons::FRAME`) around its picture, drawn when the grid row is composed from the picture's
  rectangle the worker reports (`Outcome::Picture`, `IconTable::picture`); never around a type icon,
  never in the lists. A drag out of medium tiles carries an 80 px image with the tile's 48 px picture
  (`[dnd] drag image 80x80 px, 48 px icon`). The settings dialog has a View row (a choice per pane:
  details, large icons, medium icons; `[settings] dialog: view_left details -> medium`).
  Since P6-M3, the toolbar (`src/toolbar.rs`, pane.slint `Toolbar`): seven buttons (New file, New
  folder; Cut, Copy, Paste; Rename, Delete), each calling its key's command (`toolbar::command`:
  `newfile`, `newfolder`, `cx`, `cc`, `cv`, `rename`, `delete`), placed by the setting `toolbar`:
  `"top"` (the default, so **every scripted run, the gate's included, has the toolbar across the
  window: the sidebar and the panes start 36 px down**, `dnd::TOP_BAR`, `Layout::top`), `"pane"` (icons
  only, at the top of each pane above its tab strip and path bar, 30 px with the spacing,
  `dnd::PANE_BAR`, `Layout::bar`) or `"none"`; the glyphs are Segoe Fluent Icons characters (the
  font of Explorer's command bar, part of Windows 11). Which buttons apply (`toolbar::state`, the
  app's `left-tools` / `right-tools`) is set in `update_status` on change only: New and Paste in a
  folder (Paste with `CF_HDROP` on the clipboard, read at start and on each `WM_CLIPBOARDUPDATE`),
  Cut, Copy and Delete with a selection, Rename with exactly one, nothing on This PC or in an archive
  level, no New or Paste in a search tab. A gate run with the in-pane toolbar takes
  `TWINFM_CONFIG=C:\traces\toolbar-pane.toml` (read-only, `toolbar = "pane"` only; no `split`).
  Driver: `toolbar <pane> <button>` (`newfile`, `newfolder`, `cut`, `copy`, `paste`, `rename`,
  `delete`) calls the callback a press calls, which makes the pane active (and focuses its list,
  except for the three buttons that open the name editor); a disabled button is not pressed
  (`[auto] toolbar pane=N <button>: disabled, not pressed`). `[toolbar]` lines: `pane=N <button> ->
  <command>`, `clipboard holds files | holds no files`. New file: `Job::NewFile`, Ctrl+Alt+N (key
  name `CtrlAltN`; driver `newfile <name>`, typed into the editor; `newfile` alone then `key Return`
  keeps `New Text Document.txt`, numbered by the shell on a collision), undone to the Recycle Bin.
  Escape in a list stops a search and cancels queued jobs, or else clears the selection (`[select]
  pane=N Escape: selection cleared`). Ctrl+H shows files with the hidden attribute; files both hidden
  and system need the pane's protected-files setting too (`show_protected_left` /
  `show_protected_right`, the dialog's "Protected files" row, `settings set protected_left
  true|false`; `[hidden] pane=N show_protected=true`), and a file with the system attribute alone is
  always shown (`model::Show`, `shows`). `backclick <pane> path|edit` posts the Back button over the
  path bar's middle or over row 0's name cell: ignored over the text entry and the rename editor.
  `sidebar click <row>` is a left click (a negative row counts from the end): the sidebar takes the
  keys, `key F2` opens a bookmark row's title editor, `key Escape` (or Tab) gives the keys back to
  the list. In the breadcrumb dropdown (`crumbmenu`) `key Down` / `key Up` move a highlight and
  `key Return` picks it. `[tab] pane=N strip resized: scrolled to X (tab I of N in view)` when the
  pane's width changes moved its strip. Search results' rows read `file.txt  (sub\dir)`
  (`model::search_label`; the entries, the sort and the commands keep `sub\dir\file.txt`). A tab sent
  to the background keeps its finished folder sizes (`[size] pane=N tab=T keeps K size(s) in the
  background`; walks in flight and stale sizes go: `[size] cancelled ... (tab in the background)`,
  `[size] stale ...: tab in the background, size dropped`), its background watch is a subtree watch
  while it has any (a stale background tab with sizes stays watched for them), a change below a sized
  folder drops that size there (`[size] invalidated <path> (<change>): tab in the background, size
  dropped`), and an unwatched one drops them all (`[size] cache cleared pane=N (not watched in the
  background | watch lost)`). Restored tabs: `[tab] pane=N tab=T restored: cursor Some(R), scroll Y
  (now Y)` when a restored tab's first pass lands.
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
  chosen per pane by `show_type` x `thumbnails`, never per row); since P4-M1 `view_left` and
  `view_right` (`"details"`, the default, or `"icons"`, the large-icon grid: a fifth list component,
  `FileGridList`, chosen under the same per-pane `if`s; Ctrl+Shift+1 / Ctrl+Shift+2 set and save it;
  since P4-M2 a grid pane with `thumbnails` on shows thumbnails in its tiles, at the grid's icon size,
  through the same workers and table); since P6-M2 `tile_left` and `tile_right` (`"large"`, the
  default, or `"medium"`: a sixth list component, `FileGridMedium`; Ctrl+Shift+3 sets medium, and a
  pane on details keeps its tile size), and the view (details, large icons, medium icons) is in the
  settings dialog; since P6-M3 `toolbar` (`"top"`, the default, `"pane"` or `"none"`, in the
  dialog), `show_protected_left` / `show_protected_right` (default false, in the dialog), and
  `left_tabs` / `right_tabs` as `[[left_tabs]]` tables (`settings::SavedTab`: `path`, and `sort`,
  `ascending`, `hidden`, `cursor` (the cursor row's name), `scroll`, each optional), so a restored tab
  gets its sort, hidden files, cursor and scroll back; a bare list of paths still loads (the
  defaults new tabs have), and the next save writes tables. `SLINT_ACCESSKIT_STATS=1` prints accessibility
  tree-update counters once a second; `SLINT_ACCESSKIT_UPDATE_DELAY_MS` overrides the 50 ms
  coalescing (0 = pre-patch behaviour).
- File operation soak: `tools/soak-m3.ps1 -Minutes N` builds a scratch tree in
  `%TEMP%\twinfm-m3\`, loops copy/rename/move/recycle/permanent delete/new folder/clipboard
  cycles between the panes with the bigdir/System32/Manifests scroll scenario in between, and
  reports exit code, `panicked` count and working set (CSV every 30 s). It runs the app with
  `TWINFM_SOAK=1`, which adds `FOF_NOCONFIRMATION` to every job and refuses any file operation
  outside `%TEMP%\twinfm-m3` and `%TEMP%\twinfm-m5`. Never set `TWINFM_SOAK` otherwise. Since P5-M2
  each cycle also undoes the recycle (a restore through the bin's verb) and redoes it, and the report
  carries the undo/redo counts (redos, nothing to undo/redo, refused, busy, record kept), the `undelete` outcomes (verb
  ok, moved back, moved back failed) and the number of progress lines. Each cycle
  adds one Recycle Bin entry (the undo's restore takes the first one out, the redo adds one) and
  overwrites the clipboard. `tools/soak-m5.ps1 -Minutes N` does the
  same in `%TEMP%\twinfm-m5\` and adds, per cycle, a `dropfiles` copy and move and a scripted
  context menu on a row and on empty space; it also reports drops, menus and in-menu ticks.
  `tools/soak-p2m4.ps1 -Minutes N` is the M5 soak plus, per cycle, a `dropfiles` onto a sidebar
  bookmark (`side -1`) and onto a tab (`tab 0 1`), a drop menu's "Copy here" and "Cancel"
  (`menu copy`, `menu cancel`), and a right-button `dragrow` cancelled with `dragcancel`; it
  reports drops by kind, drop menus, right-button drags (cancelled, and dropped, which must be 0)
  and drag images besides the M5 counts.
  `tools/soak-p4m3.ps1 -Minutes N` is the P2-M4 soak plus, per cycle, a grid cycle: both panes
  switched to the large-icon grid, a `dropfiles` onto a tile and onto the grid's empty space, a
  context menu on a tile, a left and a right-button `dragrow` out of a tile (both cancelled), a
  `band` from the gap after the last tile that selects the right pane's two files, and Shift+Delete
  on that selection after the switch back to details; it reports grid switches, tile drops, 128 px
  drag images and bands (started, done with 2 selected, cancelled) besides the P2-M4 counts.
  `tools/soak-p5m3.ps1 -Minutes N` is the P4-M3 soak plus, per cycle: the right pane on
  `spring` (made by the setup, holding `inner`), a `dragover row 1 0 1300` that spring-loads
  `inner`, a `dropfiles crumb 1 -2 copy` onto the parent crumb (a copy into `spring`), a
  `dragover side bottom 300` and a `dragover tab 0 1 300` (under the spring time: armed, never
  switched), then back to `spring` and Shift+Delete on the two copies; it reports spring armed,
  spring-loaded rows (and the navigations into `inner`) and tabs (must be 0), spring disarmed,
  crumb drops, and the sidebar and tab-strip auto-scroll lines (counted, not required: the sidebar
  scrolls only when it overflows) besides the P4-M3 counts. Since P6-M2 each cycle also runs a
  medium-grid cycle after the large one (two files into `dst`, the right pane on medium tiles, a drop
  onto tile 0, a cancelled drag out of it, a band from the gap after the last tile that selects both
  and leaves the cursor on tile 0, Shift+Right from there, back to details, Shift+Delete); it reports
  medium switches, 80 px drag images, bands done with the cursor on tile 0 and the Shift+Right's
  range. Since P6-M3 each cycle also runs a toolbar cycle in `dst` (New file and its undo, New
  folder, Rename to `t`, Copy and Paste, Cut and Paste into `src`, Delete there, then Shift+Delete
  of `t`; three Recycle Bin entries per cycle in all); it reports toolbar presses (eight per cycle),
  presses refused as disabled (must be 0), new files made and undone, and renames. Since P7-M1 each
  cycle also runs a verb cycle in `dst` (the test verb sleeping 1500 ms while the left list
  scrolls, the menu's Delete and its undo, the menu's Cut cancelled by Escape, Create shortcut on the
  worker, Shift+Delete of the three files); it reports verb jobs, sleeps, shortcuts made by verb,
  routed deletes and restores, and cuts cancelled.
  `tools/soak-p2m1.ps1 -Minutes N` keeps both panes on watched folders in `%TEMP%\twinfm-m5\watch\`
  (2,000 files each) and scrolls them, with `touch`/`rmfile`/`waitwatch`, tab switches and Ctrl+R,
  while a PowerShell writer job creates, renames, deletes and appends to files there (a few per
  second, a 200-operation burst every 15 s); since P5-M1 a third folder `w3` sits on a second left
  tab that the writer churns in the background, each cycle adds a `mvfile`, a `waitedit` and a
  switch to `w3` and back, and the report carries the record counts (records delivered, windows
  applied with their `+A -R ~C` sums, edits dropped, fallbacks by reason, netted, explained,
  watched in the background, past the cap, back-off lines and the largest window, switches to
  `w3` that refreshed) besides `[watch]` and `[refresh]` counts, exit code, panics and working
  set. It uses neither the clipboard nor the Recycle Bin.
- Tests that touch the desktop: `fileops::tests::worker_runs_real_shell_operations` runs real
  `IFileOperation` jobs in `%TEMP%\twinfm-m3-test-<pid>\` (and checks the progress sink's
  results and an undo round trip), `fileops::tests::cancelled_queued_jobs_never_run` makes one
  folder through the shell in `%TEMP%\twinfm-m3-test-queue-<pid>\`,
  `undo::tests::check_refuses_missing_and_non_empty` only reads folders it makes in
  `%TEMP%\twinfm-m3-test-undo-<pid>\` (none of the three uses the Recycle Bin);
  `verbs::tests::worker_runs_a_real_verb_and_the_test_verb` (P7-M1) runs "Create shortcut" on the
  verb worker for a file in `%TEMP%\twinfm-p7m1-test-<pid>\` and removes the folder;
  `fileops::tests::recycle_restore_move_as_and_progress` (P5-M2) recycles one file in
  `%TEMP%\twinfm-p5m2-test-<pid>\` and restores it through the bin's "undelete" verb (the bin's
  listing is as it found it; an orphan `$I` metadata file may stay on disk in `$Recycle.Bin\<SID>\`,
  which Windows leaves after its own restores too), runs a `MoveAs` round trip and a folder copy
  for the progress reports, and deletes the folder permanently; and
  `clipboard::tests::real_clipboard_round_trip_overwrites_clipboard` replaces the clipboard's
  contents and leaves it empty, and `dnd::tests::shell_data_object_offers_hdrop` asks the shell
  for a data object over two files in `%TEMP%\twinfm-m5-test-<pid>\`, and
  `dnd::tests::drag_source_helper_takes_a_bitmap` makes the shell's drag-drop helper store a drag
  image on a data object over a file in `%TEMP%\twinfm-m5-test-img-<pid>\`. The `fs::watch` tests create,
  rename, delete and change attributes of files in `%TEMP%\twinfm-p2m1-test-<pid>\` (removed
  afterwards; since P5-M1 one of them renames and one moves the watched folder itself there, one
  makes nine folders for the cap, and one re-watches a folder with the subtree flag flipped); they touch neither the clipboard nor the Recycle Bin.
  `fs::enumerate::tests::stat_entry_reads_one_entry` writes files (one named with a trailing dot
  through a verbatim path) in `%TEMP%\twinfm-p5m1-stat-<pid>\` and removes them.
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
  so in-app busy numbers taken minutes apart are not comparable. In-app pairs run back to back
  carry a run-order bias too (2026-10-15, P4-M2): the second slot of a pair read one to three
  points above the first whichever binary sat in it, so swap the order before reading an in-app
  "candidate above in every pair" as a cost; the console triplet decides. Record Narrator on/off
  with every run (it adds about 6 points of busy).
- Console checks start TwinFM unelevated: Windows blocks drag and drop from a normal-integrity
  program (File Explorer) into an elevated window, so a TwinFM started from the elevated gate
  console shows the denied cursor and never gets `DragEnter` (2026-10-16, `docs/phase-5-plan.md`
  § P5-M3 Gate). The gate itself may stay elevated; its scenario never drags.
- Scripted checks of a layout change set the two panes up unequal (tabs on one side only, the
  filter box open in one, the split on): the in-pane toolbar's misalignment showed only that way
  (2026-10-18, `docs/phase-6-plan.md` § Phase 6 close).
- Close every TwinFM window before a gate run: a pre-existing one appears in the trace with
  start "MIN" (the analyzer picks the process that started inside the trace, but the extra
  process still shares the machine). Single runs of one unchanged binary ranged 19.6 to 22.8
  in one afternoon (2026-10-12), so compare only within a session and never against a
  recorded number. The gate protocol since P2-M5 (agreed 2026-10-12): three runs per binary,
  interleaved reference, candidate, reference, candidate, reference, candidate, compared by
  median (`tools/wpr-triplet.ps1 -Reference <name> -Candidate <name> -Tag <tag>` runs all six
  from one elevated console, builds named by their `target-<name>` dir, and refuses to start
  while a TwinFM window is open; `-Candidate` takes a comma-separated list, and a candidate may be
  `name@C:\path.toml`, a settings file for that candidate's runs, e.g. `C:\traces\thumbs.toml`,
  labelled `name-<basename>` in the trace names; run the script from the checkout whose `tools/`
  it sits in, since it calls the `wpr-gate.ps1` beside it); a gap counts only when every reference run sits below every candidate run or the
  medians differ by more than one point. A session can change state part-way through a triplet
  (2026-10-17, ab23): at the same 60 Hz cadence, some runs of every binary cost about a third more
  per frame. Compare the `[perf] ... render[... p50=...]` figure in each run's `.stderr.txt` (3.0 to
  3.2 ms against 2.2 to 2.3 ms in ab23) before reading the medians, and read such a session like for
  like; the gap rule is about a cost, so a candidate's median below the reference's is not a gap.
  The reference binary is the last merged milestone's
  gate build (`target-p6m3`, c632e93, after P6-M3); the previous one is deleted once the new one
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
