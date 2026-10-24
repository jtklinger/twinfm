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
`docs/phase-7-plan.md` the Phase 7 record (the context menu finished, archives beyond zip, the
polish bundle with the drag cursor; four milestones, closed 2026-10-20; its first section quotes
Jeremy's kickoff decisions, § Calls made lists the lead's, and § Phase 7 close holds what is
deferred, the phase's lessons and the Phase 8 candidates, daily use recommended),
`docs/phase-8-plan.md` the Phase 8 record (the preview pane: P8-M1 the column with PDF through
hayro, P8-M2 Markdown; opened 2026-10-20, closed 2026-10-21; its first section quotes Jeremy's kickoff
decisions, § Phase 8 close holds the gates, lessons, what is deferred and the Phase 9 candidates;
§ Phase 7 leftovers is the parallel stream's record, P8-L1 to P8-L3, settled with Jeremy 2026-10-20),
`docs/phase-9-plan.md` the Phase 9 record (adjustable column widths; text, pictures and HTML in the
preview; Word, Excel and PowerPoint read into the preview's own blocks; the thumbnails and cloud polish
bundle; four milestones run as two parallel streams, opened and closed 2026-10-21; its first section
quotes Jeremy's kickoff decisions, § Calls made lists the lead's with the six superseded by what was
built, and § Phase 9 close holds the gates, lessons, what is deferred and the Phase 10 candidates, daily
use recommended), `docs/phase-10-plan.md` the Phase 10 record (UI and UX polish; opened and closed
2026-10-22: P10-M1 the rust-ui look (`src/ui/theme.slint`'s tokens and controls, the Lucide icons under
`src/ui/icons/`, Inter, the bar heights, the accent cursor and selection, the theme switch), P10-M2 the
search box, P10-M3 the title bar, the look's field and choice, the menu's icon row; its first section
quotes Jeremy's decision, § Calls made lists the lead's, § Phase 10 close holds the gates, lessons, what is
deferred, the open questions and the Phase 11 note; § Deferred to Phase 12 the roadmap), `docs/phase-11-plan.md` the Phase 11 record (the deferred
list swept and the cut features picked up: content search, the cloud and search sweep, the edge-case
bundle with the owed checks, the preview gaps bundle, syntax colours, password PDFs through hayro; two
streams, the preview stream after P10-M1 merges; opened 2026-10-22; its first section quotes Jeremy's
kickoff decisions, § Calls made lists the lead's, § Open questions what each brief settles first). Read
them before changing the list.

## Gates (run before every commit)
- `cargo test --release` (302 tests; nine touch the real shell and clipboard, one of them the Recycle
  Bin, three more only read the machine (P6-M1: the sync root registry and the system drive's label,
  imageres.dll's status glyphs, the shell's overlay for a path that does not exist), and fourteen of the
  eighteen `fs::watch` tests, the `stat_entry` test, one undo test, the breadcrumb dropdown's test, the reparse enumeration test,
  the search walker's test, the size walker's test, the two archive tests and two virtual-drop
  tests write to scratch folders, and since P7-M3 five more (the zip writer's, the 7z and TAR
  writers', the ArchiveFolder listing's, and two file operation worker tests; since 2026-10-19 the
  ArchiveFolder case test too) write archives into
  scratch folders, and since P8-M1 the preview worker's test writes PDFs into one, and since P8-M2
  two more write Markdown and pictures into scratch folders, and since P9-M2 two more write text,
  pictures and a page into scratch folders, and since P9-M3 one more writes Office files into one,
  and since P11-M1 two more write text files (and a junction) into scratch folders, since P11-M4
  one more writes a HEIC with Windows' HEIF encoder into one, since P11-M2 one more writes two
  folders of files for a shell menu over both, since P11-M5 one more writes source files into
  one, since P11-M6 one more writes encrypted PDFs into one, and since P11-M7 one more writes log
  files into one (and points the process's stderr at them while it runs), see below)
- `cargo build --release` (Slint comes from the `rustfm` branch of the jtklinger/slint fork,
  pinned by rev in `Cargo.toml`: master a1ce0d8 plus the two accessibility patches in
  `docs/patches/`; a fresh clone builds with no manual steps)
- Run the tests, read the result, then commit, as two steps: never chain a test run and a commit
  (two Phase 9 commits went in past a failing test that way, `docs/phase-9-plan.md` § Phase 9 close).

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
- Since 0.1.0 the exe is a windowed program (`#![windows_subsystem = "windows"]`): no console window,
  and stderr's lines reach a file only through a redirect (`Start-Process -RedirectStandardError`, as
  every tool here does, or PowerShell's `2>`); run by hand in a terminal it prints nothing there.
  Since P11-M7 the setting `log` (off by default; the dialog's Log row, applied on OK without a
  restart) writes them to `twinfm.log` beside the settings file (`src/logfile.rs`): TwinFM points its
  own standard error at the file (`SetStdHandle`; Rust's stderr looks the handle up on every write, so
  no `eprintln!` changed), the file's first line is `TwinFM <version> log, started <time>`, the last
  run's file is kept as `twinfm.previous.log`, and a run past 20 MB is cut the same way (checked once a
  minute). Off, nothing is touched, so a launcher's redirect gets the lines as before; on, they go to
  the file only (a scripted run reads no settings, so the gate, soak and check scripts never turn it
  on unless `TWINFM_CONFIG` names a file with `log = true`). A file the log left is not closed while
  TwinFM runs (a write racing the switch must not reach a reused handle). Lines: `[log] writing to
  <path> (the last run's kept as twinfm.previous.log)`, `[log] cannot write to <dir>: <why>`, `[log]
  stopped writing to <path>` (to wherever stderr went at start), `[log] not written: no settings file in
  a scripted run`, `[log] cut at 20 MB: a fresh twinfm.log`, `[log] open log folder: <dir>` (the
  dialog's "Open log folder": the dialog closes as OK does and the active pane shows the folder with
  the log selected), `[settings] dialog: log false -> true`. A gate whose reference was built before
  0.1.0 compares a console build against a windowed one: say so in its record.
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
  job(s) queued or running: not waited for`. Since P7-M2 a right click, Shift+F10 and the Menu key
  open the **Windows 11 style menu** (`src/winmenu.rs` the items, placement and keyboard, pure and
  tested; `src/explorercmd.rs` on the verb worker; app.slint `wm-popup`, a Slint `PopupWindow`
  covering the window, `close-policy: no-auto-close`): an icon row (Cut, Copy, Rename, Share,
  Delete where they apply, `toolbar::state`'s rule; Share for files only; Paste on a background
  with files on the clipboard), Open (one item), Open with > (one file: the shell's recommended
  apps, `SHAssocEnumHandlers`, then "Choose another app", the `openas` verb), Open in other pane,
  Open in terminal, the packaged apps' `IExplorerCommand`s (from the manifests, below), Compress to
  > ZIP file (the classic menu's "Send to > Compressed (zipped) folder", found by its label on the
  worker), New folder and New file on a background, Copy path, Properties (Alt+Enter), Show more
  options (Shift+F10). Every item calls the command its key, toolbar button or P7-M1 route calls
  (`winmenu::Cmd::key_command`: `cx`, `cc`, `cv`, `rename`, `delete` (Shift held: `sdelete`),
  `newfolder`, `newfile`, `properties`), run on the next turn of the event loop after the popup
  closed. **Alt+Enter is Properties** (`command "properties"`: the shell's `properties` verb for the
  cursor row's selection, or the folder's, on the verb worker) and **folder sizes moved to
  Alt+Shift+Enter** (key name `AltShiftReturn`; `sizeall` presses it; key name `AltReturn` is now
  Properties). Shift+right-click opens the classic menu at once with the extended verbs, and "Show
  more options" opens it for the same row at the same point. The packaged commands: each installed
  package with packaged COM (`HKCR\PackagedCom\Package`, its folder from
  `GetPackagePathByFullName`) has its `AppxManifest.xml` read once on the verb worker for
  `windows.fileExplorerContextMenus` (`ItemType` -> `Verb Clsid`; `*` any file, `.ext`, `Directory`
  a folder or a drive, `Directory\Background`, `Drive`), and again when that key's subkey count or
  last-write time changed (checked at each menu); for a menu the worker creates each class whose
  types cover every target, asks `GetState` (hidden: left out), `GetTitle` (`&` removed), `GetIcon`
  (`icons::location_pixels`: `path,-id`, a plain `.ico`/`.exe`, or a packaged app's `@{...}`
  indirect string drawn through the shell's image factory) and its subcommands, and keeps the
  objects until the next menu, so a pick invokes the object it asked (`explorercmd::run`, with an
  `IShellItemArray`; an Open with app with the items' data object). Nothing is asked inside an
  archive or on This PC's background. The menu shows when the worker answers or after 250 ms
  (`WINMENU_WAIT_MS`), and an answer after that rebuilds it. `[winmenu]` lines: `catalog: P of N
  packages declare context menu commands, C classes, read in T ms`, `packages changed: the catalog
  is read again`, per class asked `{clsid} "<title>" state S flags 0x.. in T ms (<package>)`,
  `open-with app "<name>": no icon from "<location>"`, `query id=Q: K of A commands shown (H
  hidden), N open-with app(s), in T ms (verb worker)`, `pane=N seq=Q item menu|background menu for
  K item(s) in <folder>: query queued on the verb worker | no query`, `pane=N seq=Q shown after T ms
  (<why>): [<icon row>] <items, --- a separator, > a submenu, (off) disabled>` (why: `the worker
  answered`, `the worker had not answered after 250 ms`, `rebuilt with the worker's answer`, `no
  query`), `placed at (x, y) 280 x H for (ax, ay) in a W x H window: rightward|leftward,
  downward|upward[, its rows scroll]`, `submenu "<item>" at (x, y): <items>`, `pane=N seq=Q closed
  (<why>)`, `pane=N runs <Cmd>`, `<Pick> "<title>" queued on the verb worker, id N`, `Invoke <Pick>
  "<title>" = 0x... | failed: <why> in T ms (verb worker, id N)`, `seq=Q answered after its menu
  closed: dropped`; `[menu] verb ... queued on the verb worker (found there by its label)` for Share
  and the ZIP (`InvokeCommand ... by offset` or `refused: no "<label>" in the menu`). Context menu
  steps: `rclick <pane> <row>` and `rclickempty <pane>` (and key names `ShiftF10` and `Menu`) open
  the Windows 11 menu; `waitmenu <ms>` waits until it is shown and the worker has answered, then
  100 ms; in it `menupick <text>` picks an item or icon-row button by its text (refused, closing the
  menu, for anything that runs on the verb worker: Share, Properties, Open with's apps, the packaged
  commands, except "ZIP file" inside the `TWINFM_SOAK` fence), `menuopen <text>` opens a submenu
  ("Open with", "Compress to"), `menukey <Down|Up|Home|End|Right|Left|Return|Escape|ShiftF10>`,
  `menuclose`; these run as ordinary steps (no modal loop). `menuopen Show more options` (or
  `menupick` of it, or `menukey ShiftF10`) and `rclick <pane> <row> shift` / `rclickempty <pane>
  shift` open the classic menu, which takes the in-menu steps after it (`sleep`, `menuopen <text>`,
  `menukey <key>`, `menupick <own item>`, `menuclose`) from a Win32 timer inside its modal loop,
  because no Slint timer fires while it runs (`src/auto.rs` header); `[menu]` lines on stderr. A
  script must end every classic menu with `menuclose` or `menupick`, or the run waits for a person.
  Drag and drop steps: `dropfiles <pane> <row> <copy|move|auto>
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
  the event loop), for drags from anywhere; once per spot until the pointer leaves it; since P11-M3
  a sidebar row (opened in the active pane) and a crumb other than the folder shown spring-load too;
  never a file row, empty space, a search tab, This PC's tab, an archive level or anything inside an
  archive, nor a spot that refuses the drop. Lines: `spring armed: pane N row R (<folder>)` or `pane N tab
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
  `view_right` take `details|large|medium`, since P10-M1 `theme` takes `system|light|dark`), `settings ok` and `settings close` (the OK
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
  Since P3-M2, filter and search (**since P10-M2 the search box**: always at the right end of each
  pane's path bar, 200 px, 150 in a pane under 520 px, `src/searchbox.rs` the pure rules; typing
  filters the listing live, **Return runs the recursive name search** for the text in a new tab
  (blank: the list takes the focus), Down applies the filter at once and gives the list the focus,
  Escape clears, the box's search button is Ctrl+Shift+F's search, and the button is where P11-M1
  puts the name-or-content mode): `filter <pane> <text>` puts `<text>` (the rest of the step,
  trimmed, may be empty) in the pane's search box, without the focus, and calls the box's `edited`
  callback as typing does (applied 150 ms later); `search <pane>` calls Ctrl+Shift+F's callback (a
  recursive name search for the box's text in a new tab; a blank box takes the focus, and Return
  there searches); since P10-M2 `searchreturn <pane>` is Return in the box with its text
  (`[search] pane=N box Return: searches "<text>"` or `box Return: empty, the list takes the
  focus`) and `searchbutton <pane>` the box's button (`[search] pane=N search button`, then as
  `search`); `waitsearch <ms>` waits until no search tab is still walking (at most `<ms>`), then
  300 ms for the pass. Key names `CtrlF` (the box, focused), `CtrlShiftF`, and
  any single character typed as itself (`key a`, into whatever has the focus). `[filter]` lines:
  `pane=N "<pattern>" (glob|substring): K of M shown, pass P ms` once per pass while a filter is
  on, `pane=N cleared`. `[search]` lines: `pane=N gen=G start "<pattern>" in <root>`, `batch K (N so
  far)` (at most ten a second), `done: N found, D folders walked, S skipped, in T ms`, `cancelled
  after D folders (<why>)` (tab closed, navigated away, new search, Escape, F5), `skipped N
  folders`; `[watch] no watch for <root>: search results` and `[dnd] pane N tab I shows search
  results: no drop there` for search tabs.
  Since P11-M1, **content search**: the box's content toggle (the Lucide `file-text` button left of
  the search button, lit with the accent while on; per pane, for the session, never saved; the
  placeholder "Text in files, Enter") makes Return, the button and Ctrl+Shift+F search inside the
  files for the box's text, in a tab titled `Text: <text>`, and typing filters nothing (turning it
  on clears the tab's name filter, turning it off applies the box's text as one). The walker
  (`fs::enumerate::walk` with a `Content`) reads every plain file below the root (`src/fs/content.rs`):
  over 16 MB (`CONTENT_MAX_BYTES`, the listing's size) never opened; binary by its first 8 KB (a
  NUL, or over 10% invalid UTF-8 or control characters; a UTF-16 byte order mark is text); decoded
  by the preview's rule; a case-insensitive substring (`content::fold`, a final sigma read as
  sigma); a result's row reads `file.txt  (sub\dir)  <the first matching line>` (`model::search_label`
  with `Entry::found`; control characters as spaces, trimmed, cut at 200 characters, a match deeper
  in a long line shown from 40 characters before it behind `…`). Never read: a cloud-only file (its
  attributes, `content::cloud_only`), links and reparse points, anything below a sync root met on the
  way (`Locations::roots`); a content search rooted in a cloud location (a sync root or the Google
  Drive volume) or inside an archive is refused with a note. Its history entry re-runs it (`Place`'s
  `content`). Driver: `contentmode <pane> on|off` (the toggle's click: the property, then its
  callback when it changed; `[auto] contentmode pane=N on|off`), `csearch <pane> <text>` (the toggle
  on, the text in the box, Ctrl+Shift+F's callback); `waitsearch` covers it. `[search]` lines: `pane=N
  content mode on|off`, `pane=N gen=G content "<text>" in <root>`, `done: N found in F files read, S
  skipped (B over the cap, C binary, O cloud-only, U unreadable), D folders walked, K cloud, L not
  listed, in T ms`, `pane=N content "<text>" in <root>: refused (<why>)`, `box Return: searches
  "<text>" inside files`.
  Since P11-M2, the cloud and search sweep: **a refresh of a cloud listing** keeps each row's icon
  while its mark is looked up again (`model::finish_refresh`, `carry_marks`: a new entry with a
  `look:` key takes the icon its name had on screen when its key differs only by the mark,
  `icons::same_but_mark`; the lookup still runs, `resolve_icons` asks for a kept row's `look:` key
  while any is kept, and `relabel` keeps the icon when the mark is the same or puts the placeholder
  back when it changed): `[icons] pane=N marks carried: K of L across the refresh` (L the entries
  waiting for a lookup, K those that kept an icon; rows never on screen have none to keep). **Search
  results from several folders** get one shell menu (classic and Windows 11) and one drag:
  `shellmenu::parse_items` hands the shell the desktop's `IShellFolder` and each item's absolute PIDL
  (`parse_absolute`) when `common_parent` finds more than one folder (not inside an archive, which
  keeps the refusal). **A search tab's rows on screen follow changes**: every 5 s
  (`RESTAT_EVERY`) and when the tab comes to the front, a finished search tab in front has its
  visible rows statted again on a worker thread (`restat_results`, `stat_entry` per row under its
  folder), and `land_restat` removes the gone and updates the changed through `apply_edits`
  (`model::restat_edits`; new matches still need F5): `[search] pane=N results re-stated: R rows, G
  gone, C changed, in T ms`, `results re-stated: dropped (the listing changed)`, `results re-stated:
  not applied (<why>)`. **A rename in a search tab** puts the cursor on the renamed result
  (`search_row_name`: its path under the root). **Reset column widths** (the settings dialog, under
  the Type column switch; staged, applied on OK): `settings set cols_reset`, `[settings] dialog:
  cols_reset`, `[cols] pane=N widths reset to 120, 90, 140` per pane, then the `(reset)` change lines;
  it counts as one change in the dialog's closed line.
  Since P11-M3, the edge-case bundle: **Ctrl+Z is window-level** beside Ctrl+Y (`app.slint`'s outer
  scope, not the list's), so it works from the sidebar, the tab strip and the crumbs; text boxes keep
  their own undo. **A partial Recycle Bin restore** (some items failed) records the items it brought
  back (`undo::record_for`). **A copy or move that replaced a file** is not undone: the file worker
  notes the destination paths there before the job (`fileops::Done::existed`) and an item landing on
  one makes the record `CANNOT_UNDO_REPLACED` (`undo::forbid_if_replaced`; Ctrl+Z: "Cannot undo: it
  replaced a file, which cannot be brought back"). **A folder moved to another parent is followed**:
  the parent watch sees it removed (or an old name with no new one), and the folder's own open handle
  names its new place (`GetFinalPathNameByHandleW`, `watch.rs` `moved_away`): `[watch] pane=N tab=T
  folder moved: <from> -> <to>`, then the `Moved` event's `[nav] ... path follows the rename`; a
  deleted folder still goes `Lost`. **A fixed drive whose disk is hot-pluggable** (a USB hard disk;
  here also Google Drive's `G:`) registers its watches for device events as a removable drive does
  (`IOCTL_STORAGE_GET_HOTPLUG_INFO`): `[devices] <letter>: fixed, removable media: registered | not
  removable` once per drive. `waitsize` also waits while a stale size waits for its re-walk. **The
  history menu**: Alt+Down in a list (`command "histmenu"`) or a right click on the path bar's
  background opens the pane's places in the Windows 11 menu (`open_history_menu`; forward places
  first, the current one checked, back places after it; each labelled by its name, then where it
  is, `Place::label`); a pick goes that many steps in one move (`History::jump`): `[nav] pane=N
  history menu: K entries (B back, F forward)`, `[nav] pane=N history jump S: <from> -> <to> (B back,
  F forward left)`. **History is saved with restored tabs** (`SavedTab::back` / `forward`, at most
  50 places each, `SavedPlace` with a search's pattern and `content`; an entry equal to the restored
  tab's own place is dropped, `History::restored`). **The settings dialog's Browse buttons** beside
  the start folders run the shell's folder picker on the verb worker (`shellmenu::pick_folder`,
  `verbs::Work::PickFolder`): `[settings] browse <field>: the folder picker queued on the verb
  worker`, `[settings] browse <field>: <path> | cancelled | failed: <why>`; a start folder staged
  that is not a folder shows "Left|Right start folder not found: <path>" in the dialog's footer
  (`start_folder_check`; never asked for a network path): `[settings] dialog: <field> "<path>" is not
  a folder`. Driver: `histmenu <pane>` (Alt+Down's command), `histpick <index>` (the open history
  menu's entry at that position, 0 the top), `dragover side <row>` (a sidebar row's centre, as
  `dropfiles side <row>` aims).
  Since P3-M3, folder sizes: `sizeall <pane>` focuses the pane's list and presses Alt+Shift+Return
  (key name `AltShiftReturn`, Alt+Return until P7-M2: size every folder of the listing, at most 500, nearest the cursor first);
  `waitsize <ms>` waits until no folder size is queued or walking and the last reply has landed (at
  most `<ms>`), then 300 ms; `click`, `ctrlclick`, `CtrlA`, `Space` select folders, which queues
  them (at most 100 per selection). `[size]` lines: `queued <path> (K in queue)`, `partial <path>:
  X bytes, F files, D folders` (at most four a second per path, from the worker), `done <path>: X
  bytes, F files, D folders in T ms (S skipped)` (`, truncated at depth 64` when it was), `cancelled
  <path> (<why>)` (deselected, navigation, F5, tab in the background, tab closed), `cache cleared
  pane=N (<why>)`, `invalidated <path> (<change>)`, `stale <path>: re-walk in N s` (a changed sized
  folder keeps its old size and is walked again after max(2 s, twice its last walk), once per
  window; `waitsize` does not wait for that), `stale <path>: not selected any more, size dropped`,
  `capped: K of M folders sized`, `pane=N Alt+Shift+Return: K of M folder(s) to size`. `[watch]` adds
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
  (<why>)` (folder sizes; since P7-M3 every write into a read-only format, below), `parse "<path>" -> item ok|failed 0x...` (a shell item or PIDL for a path inside
  an archive). `[dnd]` adds `DragEnter: N virtual item(s) (FileGroupDescriptorW, no CF_HDROP)`,
  `virtual drop: N file(s), M bytes, sync|async`, `virtual drop wrote <path> (K of N)`, `virtual
  drop done in T ms: Ok((files, bytes))`, `virtual drop refused: <why>` (a sync source over 256 MB),
  `virtual drop: GetAsyncMode = ..., StartOperation = ...`, `EndOperation(...)`, `the zip folder's
  data object: SetAsyncMode(TRUE) = ...`; `[watch] no watch for <path>: inside an archive`; `[menu]
  inside an archive: no IContextMenu2/3 routing`. Fixtures Jeremy keeps for this:
  `C:\traces\bigdir.zip` (the 40,000 files of `C:\traces\bigdir`, made with
  `[System.IO.Compression.ZipFile]::CreateFromDirectory`; not in the repo).
  Since P7-M3, archives beyond zip, and zips written: a file whose extension Windows' `ArchiveFolder`
  owns on this machine (`.7z`, `.bz2`, `.cpio`, `.gz` (so `.tar.gz`), `.mtree`, `.nupkg`, `.rar`,
  `.tar`, `.tbz2`, `.tgz`, `.txz`, `.tzst`, `.uu`, `.xar`, `.xz`, `.zst`, each kept only while its
  `HKCR\.ext` names `ArchiveFolder`: an archive another program owns opens in it) opens as a folder
  as a zip does (`nav`, `key Return`, middle-click); `fs::archive::split` asks the file system only
  about components with one of those extensions or `.zip`. Their levels are listed by the shell
  (`BHID_EnumItems`) on the `archive` STA thread (`src/fs/archivefolder.rs`), one level at a time,
  cached with the zips (8 archives). They are **read-only** (Windows' handler refuses every write,
  Jeremy 2026-10-19): copy out works, any write is refused before a job (`fileops::archive::classify`,
  also run on the worker). **Zips are written by TwinFM itself** (`src/fs/archivewrite.rs`, the `zip`
  crate): paste, drops, copy and move into a zip, new folder and new file, rename, delete (permanent:
  a zip has no Recycle Bin; asks first unless `TWINFM_SOAK`), cut and move out of it (the shell
  extracts, then TwinFM removes the items); every write rewrites the zip into `<name>.~twinfm<pid>.tmp`
  beside it and swaps it in with `ReplaceFileW`, existing entries copied as bytes (`fs::zipraw` since
  P8-L2: the local record, data and descriptor and the central record kept whole, only the name and
  offset patched). Files that would
  replace files in the zip are asked about once per job (a message box: Yes replace, No skip, Cancel;
  replace under `TWINFM_SOAK`). A move into a zip recycles the originals afterwards. No write into a
  zip is undone (Ctrl+Z: "Cannot undo: changes to a zip archive cannot be undone"). Since P8-L2 items
  go from one zip into another or within one (`Plan::FromZip`, `archivewrite::copy_from_zip`: entries
  copied as bytes, an encrypted one keeping its own password, a plain file into a password zip
  encrypted on the way; a copy into its own folder named "a - Copy.txt"; a move within one zip a
  rename of the paths, refused onto a file there or into itself; a move from another zip removes the
  items from it after the copy). Since P8-L3 a copy from the other formats into a zip extracts the
  items through the shell into `%TEMP%\twinfm-copy-<pid>-<job>` (its progress window, and the shell's
  own password question for a protected 7z), adds what is in that folder (`Plan::FromShell`), and
  removes the folder; a move from them is refused (`move_from_read_only_why`: nothing can leave a
  read-only archive). **An archive inside an archive opens as a folder** (Return, a double-click, a
  middle-click, `nav`): `fs::archive::descend`, on the workers that list levels, finds each
  component that is an archive stored as a file in the level above, extracts it once to
  `%TEMP%\twinfm-nested-<pid>\<n>-<time>\<name>` (a zip's entry by the `zip` crate; another format's
  item by `fileops::copy_quietly`, a silent `IFileOperation` copy on the `archive` STA thread,
  `archivefolder::extract`), keyed by the outer archive's path and the inner path, and lists the
  level from that copy (any depth; the copy is kept while the outer archive's size and time are
  unchanged; all removed at exit, `clear_nested`). `fs::archive::real` maps a path through the copies
  made so far (cheap: the UI thread may ask), and the three places that hand an in-archive path to
  the shell use it (`shellmenu::open_in_archive`, `shellmenu::parse_items` for menus and drags,
  `fileops::shell_item` for jobs), so opening, the menus, drags out and copies out work there.
  **Read-only** (Jeremy's choice): `nested` (a folder, from the inner archive's root) makes
  `writable` false and `in_readonly_archive` true (the toolbar and menu as in a 7z, the note "an
  archive inside an archive is read-only: copy it out to change it", `NESTED_READ_ONLY`), and
  `classify` refuses a copy, move, new item, rename or delete there; `nested_item` (below the inner
  archive's root: the inner archive file itself is an item of the archive holding it, so it still
  moves and goes) refuses a move, rename or delete of the items, and a copy of them reads the copy.
  A password-protected inner zip entry is not extracted (a note: copy it out to open it).
  `[archive] nested <outer>\<inner>: extracted to <copy> (B bytes) in T ms`, `copy from an archive: N
  item(s) extracted to <folder>: <outcome>`. A drop of another program's drag out of a zip or 7z
  view goes to the extraction job (the data object's Shell IDList Array names the items' paths). The
  Windows 11 menu's Compress to has "7z file" and "TAR file" besides ZIP: a `Job::Compress` on the
  file operation worker (TwinFM's own writers, the `sevenz-rust2` and `tar` crates; any name, a pax
  `path` record for a non-ASCII TAR name), the archive beside the first item and named after it
  (`a.7z`, `a (2).7z`), undone by recycling it; `menupick 7z file` and `menupick TAR file` run
  in a script. The `[enum] ... done:` line of a level of another format reads `done: N entries
  (archive level, ArchiveFolder, listed in R ms, cached|fresh)`. `[archive]` adds `ArchiveFolder takes
  K of 16 extensions (<exts>), read in T ms` (once), `worker CoInitializeEx(APARTMENTTHREADED) =
  0x...`, `open <archive> level "<inner>": N entries through ArchiveFolder in R ms`, `cache dropped
  <archive>` (after TwinFM wrote it), `refused: <what> (Windows' archive handler cannot write into
  .<ext> archives)` and `(an archive inside an archive is read-only: ...)`, `refused
  on the worker: <why>`, `job id=N "copy in|move in|new folder|new file|rename|delete|move out|copy
  from a zip|move from a zip|copy from an archive" on
  <zip>: Done | Cancelled | Error("<why>") in T ms`, `move into <zip>: originals recycled: <outcome>
  (N item(s))`, `N file(s) already in <zip>: replace|skip|cancelled`, `compress N item(s) to <archive>:
  Ok(files) | Err("<why>") in T ms`. `[fileops] done` reads `Failed: <why>` for an archive job's
  error; a refused drop notes `drop: <why>` in the pane under the pointer. A password-protected zip
  is written since P8-L2 (refused in P7-M3): a job that adds files, a new file, or plain files from
  another zip asks for the password (a dialog of TwinFM's own on the file operation worker,
  `ask_password`, masked; or the driver's `zippassword <text>`, the answer to the next question; a
  scripted job without one, or with a wrong one, is refused, never shown the dialog), checks it by
  reading the zip's smallest encrypted file whole (`archivewrite::check_password`), asks again while
  it is wrong, and encrypts the new files with the zip's own method (`Protection`: ZipCrypto, or
  AES-256 for a zip of WinZip AES entries; the crate's `aes-crypto` feature); rename, delete and move
  out ask nothing. PKWARE's strong encryption is refused. `[archive] password for <zip> (ZipCrypto|
  AES-256): asked | asked again | from the script`, `password for <zip>: right | wrong | cancelled`,
  `job id=N "<kind>" on <zip>: Cancelled at the password | refused (<why>)`, `move from <zip> into
  <zip>: removed from the source: <result>`; `[auto] zippassword: ...`. Fixture
  `C:\traces\password-twinfm.zip` (password `twinfm`). While file jobs run,
  the shell's dialogs (`#32770`) hidden behind TwinFM's window come to the front when TwinFM is the
  foreground program: `[fileops] dialog "<title>" was behind the window: brought to the front
  (ok|refused)`, or `... is open while another program is in front: left there` (once each). `fs::archive::within`
  (an item below an archive's root) is the question for items, `inside` (the archive file itself
  too) for folders to open. Windows fills the Send to submenu in the background (about 450 ms
  cold), so the verb worker asks again for up to 3 s: `[menu] "<label>" found after T ms (a submenu
  filled late)` or `not in the menu after T ms`. Fixtures Jeremy keeps beside `bigdir.zip`: `C:\traces\bigdir.7z` and
  `C:\traces\bigdir.tar.gz` (made with `C:\Windows\System32\tar.exe`, bsdtar 3.8.9; Git Bash's `tar`
  is GNU tar and writes no 7z; not in the repo). Windows' `tar.exe` crashes on names outside the
  system code page (P7-M3), which is why TwinFM writes 7z and TAR itself.
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
  `<row>` (`row / cols` grid rows down, `row % cols` tiles across, 96 x 136 px each since P7-M4); `rclickempty`
  and `dropfiles <pane> -1` work as before. Key names `Up`, `Down`, `Left`, `Right`, `PageUp`,
  `PageDown`, `Home`, `End`, `ShiftLeft`, `ShiftRight` move the cursor in the focused list (in the
  grid: Left/Right one entry, Up/Down one grid row, PageUp/PageDown a viewport of grid rows; the
  details list takes Up, Down, Home, End as before and ignores the rest). `[view]` lines on stderr:
  `pane=N starts as large icons`, `pane=N details -> large icons (K tiles to a row, R grid rows),
  scroll A -> B (entry E first)` (and back), `pane=N already <view>`; `[grid] pane=N K tiles to a
  row, R grid rows` when the pane's width changes the count (split, resize, the first layout) while
  it shows the grid. With the grid on, the once-a-second `[icons]` line's `compose N strips` counts
  grid ROWS composed (one 96 x cols by 136 px image per grid row, in a ring of slots; a scroll of one
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
  `CtrlShift3`): the grid at medium tiles, 80 x 107 px (72 until P7-M4, 88 until P8-L1) with the shell's 48 px icon
  (`icons::TileGeom`, `MEDIUM`; `LARGE` is P4-M1's 96 x 136 with 96 px, 120 until P7-M4), chosen per
  pane and saved as `tile_left` /
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
  window: the sidebar and the panes start 44 px down**, `dnd::TOP_BAR`, `Layout::top`), `"pane"` (icons
  only, at the top of each pane above its tab strip and path bar, 30 px with the spacing,
  `dnd::PANE_BAR`, `Layout::bar`) or `"none"`; the glyphs are Segoe Fluent Icons characters (the
  font of Explorer's command bar, part of Windows 11). Which buttons apply (`toolbar::state`, the
  app's `left-tools` / `right-tools`) is set in `update_status` on change only: New and Paste in a
  folder (Paste with `CF_HDROP` on the clipboard, read at start and on each `WM_CLIPBOARDUPDATE`),
  Cut, Copy and Delete with a selection, Rename with exactly one, nothing on This PC, only Copy in a
  read-only archive level (since P8-L3; nothing before), no New or Paste in a search tab. A gate run with the in-pane toolbar takes
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
  Since P7-M4, the polish bundle: the toolbar's buttons carry Slint's built-in `Tooltip` (500 ms
  under the pointer: the name and the key, "Cut (Ctrl+X)", disabled buttons too, both placements);
  driver `hover <x> <y>` moves the pointer to (x, y), logical px of the window, through Slint's
  pointer path (a posted `WM_MOUSEMOVE` is dropped by winit while the real cursor is outside the
  window), and `[toolbar] top|pane=N tooltip "<name> (<key>)"` logs each tooltip shown (on the bar
  across the window at 100%, New file is near x 55, Copy 300, Delete 545, y 22). The mouse's Back and
  Forward over the toolbar (buttons, disabled ones and the gaps) go back and forward in the active
  pane (the bar across the window) or that pane (its own bar): `backclick <pane> tool` and
  `forwardclick <pane> tool` post them over the bar's first button. The sidebar, once it has the
  keys (`sidebar click <row>`), takes `key Up`, `Down`, `Home`, `End` (past the headers,
  `sidebar::step`, the row scrolled into view) and `Return` (opens the row in the active pane):
  `[sidebar] key <Up|Down|Home|End>: row R (<label>)`, `[sidebar] Return on row R`. PageDown and
  PageUp in the details list are Explorer's (`model::page_target`, through the `Paging` global): to
  the last (first) row wholly on screen, then a screenful less one row; the grid's are as before.
  Search results sort by the file's own name, `sub\dir` breaking a tie (`model::cmp_names`). A
  subtree watch (a tab with folder sizes) also gathers the records in the folder itself, and a
  window with any becomes row edits as a plain watch's do (`take_window`, under the 500-change cap),
  for an active or a background tab: `[watch] pane=N tab=T subtree records R in the folder, changes
  C | over the cap, window W ms`, then `applied: ...` or `fallback: <reason> ...`; past 1,000 records
  in the folder itself `[watch] ... N records in the folder itself: refresh`, and the fallback's
  `subtree watch` reason is left for an overflow and that case. Tile names wrap to two lines (word-wrap, a
  long word anywhere, elided on the second line; `max-lines: 2`), and the cursor tile's whole name,
  when two lines do not hold it, shows over the tiles below (one overlay per pane in the grid's
  clip, never in the tiles). Our drag source's `GiveFeedback` sets the plain arrow and returns S_OK
  while a drop description shows (`IsShowingLayered` and a `DropDescription` other than
  `DROPIMAGE_INVALID`, `dnd::own_cursor`; it then posts `DDWM_UPDATEWINDOW`, WM_USER + 3, to the
  helper's `DragWindow`, or the description is never drawn), else OLE's default cursors: `[dnd] cursor: the arrow
  (drop description type K)` and `[dnd] cursor: OLE's default cursors` on each change.
  Since P8-M1, the preview pane (`src/preview.rs`, `src/ui/preview.slint`): a column at the window's
  right edge, right of the panes and a 6 px handle, `preview_width` (0.2 to 0.6, default 0.35) of the
  space right of the sidebar (`dnd::Layout::preview`, `dnd::preview_w`: the panes narrow, the split's
  ratio applies to what is left), shown by Alt+P (key name `AltP`, the `outer` scope), the toolbar's
  Preview button (an eighth button, at the bar's end in both placements, always enabled, drawn as on
  while the column shows; `toolbar <pane> preview`, `[toolbar] pane=N preview -> toggle`) or the
  setting `preview`. It previews the active pane's cursor file: app.slint's `changed` handlers on the
  cursor rows and the active pane (and `show_tab`) call `preview::kick`, one timer restart, only while
  the column shows; 150 ms after the last (`SETTLE_MS`) the cursor entry (path, attributes, folder or
  not, from the listing) goes to the `preview` worker thread with a generation. The worker classifies
  (a folder, a type other than `.pdf`, a PDF inside an archive, a cloud-only PDF by its attributes:
  a message, nothing read), opens a PDF with hayro (`=0.8.0`), answers its page sizes, and renders
  the pages the UI asks for at the column's width in physical px (never over 8,192 px on a side,
  `MAX_SIDE`), each under `catch_unwind`; the UI lays the pages out from their sizes, asks for those in
  view and one either side as the column scrolls (the worker skips one no longer kept), keeps only
  those images, and after a width change re-renders them once it has rested 200 ms
  (`WIDTH_SETTLE_MS`). Driver: `preview toggle` (Alt+P through the key path), `preview width <ratio>`
  (as the handle's drag; clamped), `previewscroll <px>` (the column's scroll from its top),
  `wheelat <x> <y> <dy> [shift]` (the wheel turned by `<dy>` logical px, -60 a notch down, at (x, y) of
  the window through Slint's pointer path, with a Shift key event either side given `shift`, as Slint
  keeps the modifiers from key events; logs `[auto] wheelat (x, y) dy D[ shift]: preview scroll A -> B`
  300 ms later; the column at the default 1200 px window and ratio 0.35 is near x 1100; Shift+wheel
  scrolls the column up and down since the console check, preview.slint's TouchArea behind the pages),
  `waitpreview <ms>` (until no settle or width timer runs and the file has its answer: a message, or
  every page kept holding its image at the current width; then 100 ms), `settings set preview
  true|false`. `[preview]` lines (pages counted from 1): `shown at W px (ratio R)` (when the column
  reports its width as it appears), `hidden`, `pane=N gen=G <path>: pdf, P pages, opened in T ms |
  folder | no preview for this type | inside an archive | cloud-only | password | damaged: <why>` (why: an I/O
  error, hayro's `LoadPdfError`, `no pages`, `panicked: <msg>`), `gen=G pages in view A..B, kept K
  image(s)` (when the kept pages change; K after the drop), `gen=G page K rendered WxH px in T ms`,
  `gen=G page K capped at 8192 px (was WxH)`, `gen=G page K took T ms: too long` (over 5 s: "Page K
  took too long" in its place), `gen=G superseded: N queued page(s) dropped` (when a newer file or
  a close ends the document), `width W px: re-render after settle` (W the page width, logical px),
  `worker panicked on <path>: <msg>` (the column then reads "This PDF could not be read"). Messages:
  "No preview", "Password-protected PDF: not previewed", "This PDF could not be read", "Not
  previewed: the file is in the cloud and not downloaded", "Not previewed inside an archive". The
  gate scenario's cursor sits on no row, so a run with `preview = true` shows the empty column.
  Since P8-M2, Markdown (`.md`, `.markdown`, any case; `src/markdown.rs` the pure part): the archive
  and cloud checks apply as for a PDF, and the label for any other type reads `no preview for this
  type` (was `not a pdf`). The worker reads at most 2 MB (`markdown::MAX_BYTES`; over it the text ends
  at the last line break before 2 MB, else at a character boundary, and a note block comes first; a
  UTF-16 file by its BOM is decoded, other invalid UTF-8 replaced), parses it with pulldown-cmark
  (`=0.13.4`, Slint's copy: tables, task lists, strikethrough, footnotes, GFM, YAML front matter
  skipped) into a flat block list (heading, paragraph, code, rule, table, picture, note; list depth,
  quote depth, a list item's bullet or number or task box on its first block; a code block or table
  over 40 lines or rows split into chunks drawn as one, a table's column shares from its longest cells
  so the chunks line up), writes each paragraph's, heading's (wrapped in `**`) and table cell's inline
  content back as inline Markdown (ASCII punctuation escaped, emphasis as `*`, a zero-width space
  between a closing and an opening run, code spans fenced one backtick longer than any run inside,
  links as `[text](<dest>)`, U+E541 removed) and makes it a `StyledText` there (`from_plain_text` on
  Slint's refusal, logged), answers the blocks, then decodes the local pictures one at a time
  (`image` with png, jpeg, gif, webp; `resvg` for SVG, its text not drawn; at most 8,192 px a side,
  scaled down; 64 MB of decoded pixels per file, `PIXEL_BUDGET`; a cloud-only or missing file by its
  attributes, never opened), checking the generation between them. Remote pictures show their alt
  text where they stand; an HTML block shows its text without tags, its local `<img>` as a picture.
  preview.slint shows the blocks in a ListView (`mode` 1; only the rows on screen are built), the
  Shift+wheel TouchArea behind it too. Links (`link-clicked`, `preview::link`): `http`, `https`,
  `mailto` open in the default browser (`shellmenu::open_url`, no IDList mask); `#anchor` scrolls to
  the heading with that GitHub slug (`markdown::Slugger`; the list goes to its far end, then 60 ms
  later to the block, so the ListView seeks to it exactly); a local path (against the file's folder,
  percent-decoded, `?` and `#` dropped): a folder opens in the active pane, a program or script
  (`.exe .bat .cmd .ps1 .lnk .vbs .msi .com .scr .pif .js .jse .wsf .wsh .hta .cpl .msc`) is shown
  there (its folder, the cursor on it; `main.rs` `show_in_pane`), another file opens as a double-click
  does. Under `TWINFM_SCRIPT` the browser and documents are never launched, only logged; anchors,
  folders and shown programs run. Driver: `previewlink <dest>` (the rest of the step) as a click on
  that link; `waitpreview` also waits for the blocks, every picture and the anchor timer;
  `previewscroll` and `wheelat` scroll the Markdown list too. `[preview]` lines: `pane=N gen=G <path>:
  markdown, B blocks (H headings, C code, T tables, I images), parsed in T ms[, cut at 2 MB]` (code
  blocks and tables counted once however many chunks), `unreadable: <why>` (the column reads "This
  file could not be read"), `gen=G N blocks in the list in T ms (UI thread)`, `gen=G image <src>: alt
  text (remote)`, `gen=G image K <src>: WxH px in T ms | alt text (missing | cloud-only | over the
  budget | failed: <why>)` (K the block; the first 50 of a file, then only `gen=G pictures done: S shown,
  A alt text, F file(s) decoded, M MB of the budget, in T ms`; a file repeated in a document is
  decoded and counted once), `gen=G superseded: N picture(s) not decoded`, `gen=G block
  K: styled text fell back to plain: <err>` (the first 20, then `N more block(s) fell back to
  plain`), `link "<dest>": browser | anchor -> block K | open <path> | show <path> | refused: <why>`
  (` (scripted: not launched)` after browser and document opens in a scripted run), `gen=G scrolled
  to block K: top Y of H px`.
  Since P9-M2, text, pictures and HTML (`preview.rs` `TEXT_EXTS`, `PICTURE_EXTS`, `HTML_EXTS`, any
  case; anything else, a file with no extension, HEIC, HEIF and AVIF are "No preview"; the archive and
  cloud checks apply as for a PDF). **Text** (`.txt`, `.log`, `.json`, `.rs`, `.ps1` and the rest of
  the list) is read as Markdown is (2 MB, a UTF-8 or UTF-16 BOM, else UTF-8 with invalid bytes
  replaced, the note first when cut) and shown as code blocks (`markdown::text_blocks`: tabs as four
  spaces, chunks of at most 40 lines and 8 KB, `TEXT_CHUNK_BYTES`, a longer line split at a character
  boundary; Consolas, wrapped as Markdown's code is). **A picture** (`.jpg`, `.png`, `.gif` (first
  frame), `.webp`, `.bmp`, `.tif`, `.ico`, `.svg` and the rest) is decoded on the worker
  (`decode_picture`: the EXIF orientation applied, as it is now for Markdown's pictures too; up to 512
  MB of pixels while decoding, `DECODE_MAX`), scaled down by `fit_px` (no side over 8,192 px, the
  pixels within the 64 MB budget, never up), and shown as a picture block at its own size in screen
  pixels (the `Picture` reply's `physical`), fitted to the column, with a note `W x H, <size>,
  <FORMAT>` under it; "This picture could not be read" when the decoder refuses it. **HTML** (`.html`,
  `.htm`) is read by TwinFM's own scanner (`src/html.rs`, pure; no parser crate) into the Markdown
  blocks: headings, paragraphs (`p`, `div` and the other block containers), lists, quotes, `pre` as
  code, rules, tables (`th` rows as heads, `colspan` ignored), local pictures, `a`, `b`/`strong`,
  `i`/`em`, `code` inline; `script`, `style`, `head`, `nav`, `noscript`, `title`, `template`, `svg`,
  `iframe` and comments dropped; entities decoded (`markdown::entities`: Latin-1 and the common
  typographic names, numeric ones); an element's `id` (and `<a name>`) anchors the block it starts,
  so `previewlink #id` scrolls to it. No new driver step (`waitpreview` covers the three). `[preview]`
  lines: `pane=N gen=G <path>: text, L lines, B bytes, <UTF-8 | UTF-8 BOM | UTF-16LE | UTF-16BE>[ (invalid
  bytes replaced)][, cut at 2 MB]`, `pane=N gen=G <path>: picture WxH px, <format>, decoded in T ms[,
  scaled to WxH]` (W x H upright, before scaling), `pane=N gen=G <path>: picture failed: <why>`
  (`missing`, `failed: <decoder error>`, `over 512 MB of pixels`, `panicked: <msg>`), `pane=N gen=G
  <path>: html, B blocks (H headings, C code, T tables, I images), parsed in T ms[, cut at 2 MB]`;
  a text or HTML file that cannot be read gives `unreadable: <why>` as Markdown does.
  Since P9-M3, Word, Excel and PowerPoint (`.docx .docm`, `.xlsx .xlsm`, `.pptx .pptm`, any case,
  `office::OfficeKind`; `.doc .xls .ppt` are "No preview"; the archive and cloud checks apply): the
  worker opens the file as a zip (`zip`) and `src/office.rs` reads its XML parts as a stream
  (`quick-xml =0.41.0`), each part at most 64 MB (`OFFICE_MAX_XML`; past it the blocks so far and the
  note "Only the first part of this document is shown." first), into the Markdown preview's blocks.
  **Word**: `word/document.xml` paragraphs, headings by the style's name in `styles.xml` (`heading 1`
  to `6`, `Title`, or `outlineLvl`), list items from `numbering.xml` (bullets by depth, numbers
  counted by TwinFM), bold, italic, monospace runs as code, hyperlinks and bookmark anchors, tables
  (`tblHeader` rows as heads), pictures from `word/media` through the budget, page breaks as rules;
  deleted text, field codes, footnote marks and `mc:Fallback` dropped; headers, footers, footnotes and
  comments never read. **Excel**: one sheet at a time (the first), a heading naming it and, with two
  sheets or more, a row of links `#sheet=N`; the sheet read up to 500 rows from its first with a value
  (`XLSX_MAX_ROWS`) and 50 columns (`XLSX_MAX_COLS`), shared strings read only up to the highest index
  used, dates by their number format as `yyyy-mm-dd` (1900 or 1904 system), numbers in their shortest
  form, all-number columns right-aligned; a chart sheet "Chart sheets are not previewed." **PowerPoint**:
  per slide a heading `Slide N`, its title as a level-2 heading, text bodies as paragraphs and list
  items (`a:buChar`, `a:buAutoNum`, and every paragraph of a body placeholder unless `a:buNone`),
  tables, pictures, a rule between slides; notes never read. `previewlink #sheet=N` (the sheet
  heading's anchors name every sheet) previews the same file again with that sheet, a new generation
  (`Req::Open`'s `sheet`). Messages: "This document could not be read", "Password-protected document:
  not previewed" (an OLE file whose first 64 KB name `EncryptionInfo`). `[preview]` lines: `pane=N
  gen=G <path>: docx, B blocks (H headings, T tables, I images), parsed in T ms[, cut at 64 MB]`,
  `... xlsx, S sheets, sheet "<name>" R x C[ (cut at 500 rows|50 columns|500 rows and 50 columns)],
  parsed in T ms` (`chart` in place of `R x C` for a chart sheet), `... pptx, N slides, B blocks,
  parsed in T ms`, `... unreadable: <why>` (`not a zip: ...`, `no word/document.xml`, ...), `...
  password`, `link "#sheet=N": sheet N | refused: no sheet N`, `gen=G superseded: the document's
  reading stopped` (a newer file asked for between parts); the pictures' lines as Markdown's, the part
  name in place of the source (`gen=G image K word/media/image1.png: ...`).
  Since P8-L1, the first of the Phase 7 leftovers: Return, a double-click and the Windows 11 menu's
  Open open **every selected item** when the row is in a selection of several (`model::open_set`;
  the menu's Open takes the selection as every menu item does): folders, drives and archives each
  in a new tab of the pane (placed after the active tab, in order, the last one shown), files with
  the shell's default command for all of them at once on the verb worker
  (`VerbRequest::default_item`, `GetMenuDefaultItem`, one job per folder for a search tab's files),
  files inside an archive each through its PIDL; above 15 items (`model::OPEN_ASK_ABOVE`) a message
  box asks first (none under `TWINFM_SOAK`). `[open] pane=N K items: D folder(s) to tabs, F
  file(s)`, `[open] pane=N K items: not opened (the question was declined)`, and `[menu] verb ""
  ("Open") queued on the verb worker (the default command), id N`. A script never opens several
  files (they would start their apps on the desktop); several folders are safe. The menu shows Open
  for any selection and Open with for several files of one extension too (`winmenu::same_type`;
  Explorer offers none for several, Jeremy chose to), its submenu then the type's apps alone (no
  "Choose another app": Windows' chooser takes one file) or a disabled "No apps for this type"; the
  picked app gets every file at once. PageDown and PageUp in the grid are the details list's rule by
  grid rows, the cursor keeping its column (`model::grid_page_target`, the `Paging` global's
  `grid-target`). Medium tiles are 80 x 107 with names on up to three lines (`max-lines: 3`, the
  name box 57 px, `name-h` on the list host), and the cursor tile's whole name shows over the tiles
  below when three lines do not hold it.
  Since P9-M1, adjustable column widths (`src/cols.rs` the pure part): the details list's Type, Size
  and Modified widths are per pane, logical px (`col_type_left`, `col_size_left`,
  `col_modified_left` and the `_right` three; defaults 120, 90, 140; 40 to 2,000 on load), the
  window's `left-col-type` and the rest, read by the header, every details row variant (rows read
  the list's `type-w`, `size-w`, `modified-w`: the spike measured no cost against per-pane copies)
  and the rename editor. A 6 px grip (pane.slint `ColGrip`, an overlay over the header, never in the
  rows; the east-west cursor) sits on each boundary, named by the column on its right: `type` is
  Name | Type, `size` Type | Size (Name | Size with the Type column off), `modified` Size |
  Modified; none at the list's right edge, none in the grid. Dragging grip `c` moves that boundary
  from the widths at the press: `c` narrows by what its left neighbour (Name for `type`) grows; a
  column stops at 40 px, the name cell at 120 px (a pane already narrower keeps what it has:
  `dnd::name_cell`, the name cell's x range from the widths, clamped at zero). A double-click fits
  column `c` to the widest of its cells on screen and its bold header label, plus 12 px (one hidden
  `Text` in app.slint measures them, `measure`), moving the same boundary. Driver: `colwidth <pane>
  <type|size|modified> <px>` (as a drag's end), `coldrag <pane> <col> <dx>` (press on the grip's
  centre 18 px above the list's top, moves in 8 px steps, release, through Slint's pointer path),
  `colfit <pane> <col>` (two clicks at the grip; `[auto] col pane=N <col>: no such grip in this
  pane's header` for `type` with the Type column off or a pane in the grid). `[cols]` lines: `pane=N
  <col> W -> W2 px (drag | fit | script | config)` per column changed (a drag logged at the
  release; `config` at start for a value other than the default), `pane=N fit <col>: widest of K
  rows on screen = W px`, and `pane=N widths type T size S modified M (name N px)` after each change
  and for both panes 400 ms after start (N the name cell's width then; the right pane's is its
  would-be width before the split). A gate run with other widths takes
  `TWINFM_CONFIG=C:\traces\cols.toml` (read-only: both panes 200, 110, 180).
  Since P9-M4, thumbnails beyond the listing and cloud polish: a search tab's results are keyed for
  their thumbnails as a folder's entries are when `thumbnails` is on (`fs::enumerate::walk`'s
  `thumbnails`; off for a search rooted in a cloud location, `main.rs` `search_thumbnails`, where a
  thumbnail could start a download), so the `[icons]` line's `thumbs N` counts them. Archive levels
  keep their type keys: the shell has no thumbnail provider inside a zip or a 7z
  (`WTS_E_FAILEDEXTRACTION`, the P9-M4 pin check). The press's drag request (`prefetch_drag_icon`)
  asks for the thumbnail when the pressed row's key is a `thumb:` key (its mark kept, so a cloud
  file's stays in the cache; the worker never drops a drag thumbnail as scrolled past), and the
  drag image line reads `icon from the thumbnail` (`dnd::IconFrom`: the tile, the thumbnail, the
  worker, the shell). Under a sync root a cloud placeholder file that is not in sync
  (`icons::placeholder_state`, `CfGetPlaceholderStateFromAttributeTag` from the listing's attributes
  and reparse tag, no file opened; `needs_state_read`) is keyed `look:s:` as a folder is, so its glyph
  is the provider's own state; `[cloud] state read <path>: S in T ms` for each such file (folders'
  reads are not logged). The shell's overlay images (Google Drive's mark, the link arrow) over an icon
  larger than 48 physical px are drawn at two thirds of it at its lower left (`icons::overlay_side`,
  `lower_left`); `[icons] overlay index K fetched at S px over P px` then. No new driver step.
  Since P11-M4, the preview gaps bundle. **PDF zoom**: Ctrl+wheel over a PDF's pages (preview.slint's
  TouchArea behind them; never a scroll), or Ctrl+plus (`+` or `=`), Ctrl+minus and Ctrl+0 (100%) with
  the pointer over the column (app.slint's outer scope; `preview::zoom_key` checks the cursor's client
  point against `window_layout`, else the key goes on), steps `preview::ZOOMS` 50, 67, 75, 90, 100,
  110, 125, 150, 175, 200, 250, 300, 400 (`zoom_step`), saved as `preview_zoom`; the pages are laid out
  and rendered at the zoom times the column's width (the 8,192 px cap kept), centred when narrower,
  scrolled sideways when wider (the ScrollView's content width), the scroll kept at the same place in the
  document; Markdown and the rest have no zoom. **HTML's charset**: a page with no BOM is read in the code
  page its first 1,024 bytes name (`html::meta_charset`: `<meta charset>` or the Content-Type meta):
  Windows-1252 for its labels and ISO-8859-1's and ASCII's (the HTML standard's mapping), ISO-8859-15,
  UTF-8; UTF-16 named in a meta reads as UTF-8 (the standard's rule; a real UTF-16 page has its BOM); any
  other label reads as UTF-8, invalid bytes replaced. **Entities**: all 2,125 HTML5 names (the 2,231 less
  the legacy forms without `;`), asked of pulldown-cmark's own table (`markdown::entity`). **colspan**: a
  cell is followed by `colspan - 1` empty cells (at most 64). **HEIC, HEIF, AVIF** (`.heic .heif .hif
  .avif`, `wic::EXTS`): pictures decoded by Windows' own decoders (`src/wic.rs`, WIC on the preview
  worker in the multithreaded apartment; the HEIF Image Extensions from the Store, with HEVC for HEIC and
  AV1 for AVIF), oriented by `System.Photo.Orientation`; without the extension the column reads "No
  decoder for this format is installed". **Markdown details**: a picture inside `<p align=center>`, `<div
  align=center>` or `<center>` is centred (`Block::center`; text stays left); table lines are 1 px where
  cells meet; an SVG's text is drawn with seven Windows fonts read at the first SVG (`SVG_FONTS`: Segoe UI
  and bold, Arial and bold, Times New Roman and bold, Consolas; an unknown family in Segoe UI). **Copy
  text**: a right-click on a block of the Markdown, text, HTML, Office or picture preview opens app.slint's
  `preview-menu` ("Copy this block": a code block or table whole; "Copy all"), plain text with CRLF line
  ends (`markdown::block_text`, `copy_text`; a table's cells tab-separated, a list item with its marker,
  a picture its alt text; text as shown: tabs as four spaces); the column takes no keyboard focus, so
  Ctrl+A and Ctrl+C stay the lists'. Driver: `previewzoom <in|out|reset>` (as Ctrl+wheel; `[auto]
  previewzoom: no PDF shown` when there is none), `previewcopy <block|all>` (the menu's pick; overwrites
  the clipboard). `[preview]` lines: `zoom Z%: pages W px wide[ (unchanged)]`, `menu on block K`, `copy
  all|block K: N characters, L lines`, `copy all|block K: failed: <why>`, `copy: no text shown`, `svg
  fonts: F faces from N files in T ms` (once), `pane=N gen=G <path>: html, ... parsed in T ms[, cut at 2
  MB], <encoding>` (`UTF-8`, `UTF-8 BOM`, `UTF-16LE`, `windows-1252`, `ISO-8859-15`, ... `, charset
  "<label>" not known`), `pane=N gen=G <path>: picture WxH px, HEIC|HEIF|AVIF, decoded in T ms`, and
  `picture failed: no decoder | failed: not a picture Windows can read | failed: <call> 0x...`.
  Since P11-M5, syntax colours: a text file whose extension `syntax::lang_of` knows (Rust, PowerShell,
  Python, JSON, TOML, INI and `.reg`/`.inf`/`.cfg`/`.conf`, YAML, XML, CSS, JavaScript and TypeScript,
  C and C++, C#, Java, Go, shell, batch, Slint, SQL, `.log`, diff; `.txt`, `.csv` and the rest stay
  plain) is coloured by TwinFM's own tokenizer (`src/syntax.rs`, pure: one table per language, five
  token kinds, keywords, strings, comments, numbers, types; a block comment or a multi-line string open
  at a chunk's end carried into the next; a log's levels, time stamps and `[tags]`; Jeremy chose it over
  syntect, 2026-10-23). The worker answers the plain blocks first (one more answer counted per code
  chunk in `Blocks`' `pictures`), then colours the chunks in order, the generation checked between
  them, each a `StyledText` made from inline Markdown with `<font color>` runs (`syntax::markup`: ASCII
  punctuation escaped, a line's leading spaces as no-break spaces, every line a hard break), answered as
  `Reply::Colour` and drawn by preview.slint's code block as a `StyledText` in Consolas (`MdBlock`'s
  `coloured`). The palette (`syntax::LIGHT`, `DARK`: Tailwind's 600 and 400 steps) is chosen by
  `Palette.color-scheme` when the file is asked for; a change of scheme (app.slint's `theme-dark`) or of
  the setting previews the file again (`preview::recolour`). Setting `syntax_colours` (default true, the
  dialog's "Code" row; `settings set syntax_colours true|false`). `waitpreview` waits for the colours.
  `[preview]` lines: the text line ends `, <language> | plain | plain (colours off)`, `gen=G coloured N
  chunk(s) in T ms (<language>)`, `gen=G superseded: N chunk(s) not coloured`, `gen=G block K: colours
  refused, plain kept: <err>`, `recolour: the file is previewed again`.
  Since P11-M6, password PDFs through hayro (no new crate): a PDF hayro cannot open with the empty
  password shows "Password-protected PDF" and an "Enter password…" button (Jeremy's choice: never asked
  as the cursor passes); the button opens the file again with `Req::Open`'s `ask` (the window's handle),
  and the worker asks in TwinFM's masked dialog (`fileops::archive::ask_password`, the zip password's,
  now taking its prompt), asks again while the password is wrong, and on Cancel leaves the message.
  hayro takes the user password of revisions 2 to 4 (RC4-40, RC4-128, AES-128) and the user or owner
  password of 5 and 6 (AES-256); an owner-password-only file opens with no question. The password lives
  in the open document only, never written anywhere. Driver: `pdfpassword <text>` (the answer to the next
  question, then the button pressed; a scripted run without it is refused, never shown the dialog).
  `[preview]` lines: `password button`, `pane=N gen=G <path>: pdf password asked | asked again | from the
  script | right | wrong | cancelled | refused (the script gave none)`; the open's `password` line as
  before.
  Since the P11 video-thumbnail follow-up: `IThumbnailCache::GetThumbnail`'s failures are logged,
  `[icons] thumbnail <path>: GetThumbnail = 0x...` (never `WTS_E_FAILEDEXTRACTION`, no picture to make,
  nor `STG_E_FILENOTFOUND` under `WTS_INCACHEONLY`, a cloud-only file not in the cache). An answer that
  says "not yet" (`LATER_HRESULTS`: `WTS_E_EXTRACTIONTIMEDOUT`, `WTS_E_SURROGATEUNAVAILABLE`,
  `WTS_E_EXTRACTIONPENDING`, `E_PENDING`; extraction only) ends the line ` (asked again later)`: the row
  shows the type icon (`Outcome::Later`), the worker gives the key back, and the table asks again after
  2, 5 and 10 s (`LATER_WAITS`, `SizeKeys::later`, `IconTable::retry_due` from the once-a-second icon
  timer; the rows showing that icon keep it while they ask, `SizeKeys::stale`); after the fourth such
  answer the icon stays for the session. `[icons] N thumbnail(s) asked again`, `[icons] thumbnail not
  ready after 4 answers: <key> keeps its type icon`. No new driver step. A test video made by Windows
  itself (no ffmpeg here): WinRT `MediaComposition` with `MediaClip.CreateFromImageFileAsync` and
  `RenderToFileAsync` from Windows PowerShell (the plan's record has the script's shape).
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
  defaults new tabs have), and the next save writes tables; since P8-M1 `preview` (the preview
  column, default false, Alt+P, in the dialog) and `preview_width` (its share, 0.2 to 0.6, default
  0.35, its handle's drag), since P11-M4 `preview_zoom` (a PDF's zoom, percent, 50 to 400, default
  100), since P11-M5 `syntax_colours` (default true, in the dialog), since P11-M7 `log` (default
  false, in the dialog, `logfile`); since P10-M1 `theme` (`"system"`, the default, Windows' app mode followed live,
  `"light"` or `"dark"`, in the dialog's Theme row, applied at start and on OK without a restart;
  `TWINFM_SCHEME=light|dark` overrides it for a scripted screenshot of the other scheme; since P10-M3 the
  native title bar follows it through `DWMWA_USE_IMMERSIVE_DARK_MODE`, set when the window's handle is
  first fetched and on each change, and **a system setting sets no colour scheme at start**: the style's
  own binding follows Windows live, while setting the scheme to unknown would blank Slint's default
  text colour in dark mode; a switch back to system at runtime sets Windows' mode of that moment until
  the next start); since P9-M1 `col_type_left`, `col_size_left`, `col_modified_left` and
  the `_right` three (each pane's column widths, logical px, defaults 120, 90, 140, 40 to 2,000;
  the header grips set them). `SLINT_ACCESSKIT_STATS=1` prints accessibility
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
  routed deletes and restores, and cuts cancelled. Since P7-M2 its menus open the Windows 11 menu
  (row 0 goes on to "Show more options", the grid's tile menu is `rclick ... shift`), and each
  cycle runs a menu cycle in `dst` (Delete and its undo, Cut and Escape, Compress to > ZIP file,
  New folder from the background, Open with's submenu, then Shift+Delete of all); it reports menus
  shown and rebuilt, picks run, ZIPs made, new folders, submenus, classic menus from Show more
  options, and menupicks refused (must be 0). Since P7-M3 each cycle also runs an archive cycle in
  `%TEMP%\twinfm-m5\arch` (a.7z, a.tar.gz and a.zip made by the setup with Windows' `tar.exe`, and
  c.txt): browse the tar.gz and the 7z, copy out of the 7z into `dst`, a refused copy back into it,
  then in the zip copy in, new folder, rename, delete, move out and a dropped move back in, and
  Compress to > 7z file and TAR file on c.txt, each undone; it reports levels listed by
  ArchiveFolder, zip jobs done by kind, zip jobs not done (must be 0), refusals, archives compressed
  and undone (three more Recycle Bin entries per cycle). Since P8-M1 each cycle also runs a preview
  cycle in `%TEMP%\twinfm-m5\preview` (made by the setup: a-pages.pdf, four pages, and b-one.pdf,
  written by the script with a hand-made cross-reference table, c-damaged.pdf, d-notes.txt): Alt+P,
  the cursor through the four files with `waitpreview`, a `previewscroll`, the width changed and set
  back, Alt+P; it reports PDFs opened, pages rendered, damaged and not-a-PDF lines, superseded lines,
  waitpreview timeouts and worker panics (both must be 0). Since P8-M2 the setup adds e-dot.png (made
  with System.Drawing), f-readme.md (headings, lists, a table, code, the picture, an anchor link) and
  g-big.md (over 2 MB), and the cursor goes on through them, f-readme.md's `#usage` link followed and
  g-big.md scrolled; it reports no-preview lines (two per cycle), Markdown previews (two), pictures
  shown and alt texts (summed from the `pictures done` lines; alt text must be 0), styled-text
  fall-backs and cuts at 2 MB (one). Since P9-M2 d-notes.txt and e-dot.png are previewed (text, a
  picture; no "no preview" line is left), and the setup adds h-notes.txt (tabs and a 20,000-character
  line), i-photo.jpg (System.Drawing) and j-page.html (a head, script and style dropped, ids, a list,
  a table, a pre, e-dot.png), the cursor going on through them and j-page.html's `#end` followed; it
  reports text previews (two per cycle), pictures (two), picture failures (must be 0) and HTML
  previews (one). Since P9-M3 the setup adds k-report.docx, l-table.xlsx and m-deck.pptx (zips of
  hand-written parts, `Write-Parts`), the cursor going on through them and l-table.xlsx's
  `#sheet=2` followed; it reports docx, xlsx (two per cycle) and pptx previews, sheet links (one) and
  Office files unreadable (must be 0). Since P9-M1 each cycle also drags the left pane's Type | Size
  grip 30 px right in `src\tree`, fits Modified and sets the widths back with `colwidth`; it reports
  drags read back as Size 90 -> 60, fits, and widths lines back at 120, 90, 140. Since P9-M4 each
  cycle also turns thumbnails on, drags i-photo.jpg's row in `preview` (cancelled) and searches for
  photo there, then turns them off; it reports drag images from the thumbnail and the searches.
  Since P11-M1 each cycle also runs a content search in `src` (`csearch 0 scratch file 3/7`, its tab
  closed, the toggle off again); it reports content searches done, files read and files found (one).
  Since P11-M2 each cycle also narrows the left pane's Type column and resets both panes through the
  settings dialog (`settings set cols_reset`, OK); it reports the resets (two per cycle).
  Since P11-M3 each cycle also takes the left pane from `src` into `src\tree` and back through the
  history menu (`histmenu 0`, `histpick 1`); it reports the jumps (one per cycle).
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
  `%TEMP%\twinfm-p3m4-async-<pid>\`. Since P7-M3: `fs::archivewrite::tests::zip_edits_read_back`
  edits zips (one made by `C:\Windows\System32\tar.exe` when it exists) in
  `%TEMP%\twinfm-p7m3-zipw-<pid>\`, `compress_seven_z_and_tar` makes a 7z and a TAR of a tree with
  non-ASCII names in `%TEMP%\twinfm-p7m3-pack-<pid>\` and lists them through `ArchiveFolder`,
  `fs::archive::tests::shell_levels_are_listed_and_cached` lists a 7z it makes in
  `%TEMP%\twinfm-p7m3-shell-<pid>\`, `shell_levels_take_any_case` another in
  `%TEMP%\twinfm-archive-case-<pid>\`, `fileops::archive::tests::jobs_are_classified` writes zip- and
  7z-named files in `%TEMP%\twinfm-p7m3-classify-<pid>\`, and `zip_jobs_on_the_worker` runs zip jobs
  and a Compress through the real worker in `%TEMP%\twinfm-p7m3-jobs-<pid>\` (no Recycle Bin). Since
  P8-M1 `preview::tests::the_worker_renders_a_pdf_and_refuses_a_damaged_one` writes a two-page PDF, a
  damaged one and a text file in `%TEMP%\twinfm-p8m1-test-<pid>\` and runs the preview worker on
  them; since P8-M2 `preview::tests::pictures_are_capped` decodes PNGs, an SVG and a broken file in
  `%TEMP%\twinfm-p8m2-pic-<pid>\`, and `the_worker_previews_markdown_with_a_picture` runs the worker on
  a README with a local PNG and a file over 2 MB in `%TEMP%\twinfm-p8m2-test-<pid>\`; since P9-M2
  `preview::tests::a_phone_photo_shows_upright` decodes JPEGs with an EXIF orientation, a BMP and a
  broken file in `%TEMP%\twinfm-p9m2-pic-<pid>\`, and `the_worker_previews_text_pictures_and_html`
  runs the worker on text (UTF-16 and over 2 MB too), a JPEG, a broken PNG and a page in
  `%TEMP%\twinfm-p9m2-test-<pid>\`; since P9-M3 `the_worker_previews_office_files` writes the office
  tests' hand-made docx, xlsx and pptx, a text file named .xlsx and an OLE header named .docx into
  `%TEMP%\twinfm-p9m3-test-<pid>\` (the `office::tests` build their zips in memory); since P11-M4
  `wic::tests::heic_through_windows_decoders` writes a HEIC with Windows' HEIF encoder (skipped, with a
  check of the "no decoder" answer, where none is installed) and a damaged one into
  `%TEMP%\twinfm-p11m4-heic-<pid>\`, and `preview::tests::svg_text_is_drawn` reads seven fonts from
  `%WINDIR%\Fonts`; since P11-M5 `the_worker_colours_source_after_its_blocks` writes a Rust file
  and a text file into `%TEMP%\twinfm-p11m5-test-<pid>\`; since P11-M6 `password_pdfs_open_with_their_password` writes
  two encrypted PDFs (pypdf's, kept in the test as hex) into `%TEMP%\twinfm-p11m6-test-<pid>\`. Since P8-L2 `fs::zipraw::tests::copies_keep_every_byte_but_the_name_and_offset` writes
  `%TEMP%\twinfm-p8l2-raw-<pid>.zip`, `fs::archivewrite::tests::password_zips_and_copies_between_zips`
  makes ZipCrypto, AES and plain zips in `%TEMP%\twinfm-p8l2-pw-<pid>\`, and `zip_jobs_on_the_worker`
  writes into its tar.exe password zip with the script's password (`zippassword`'s path) and copies
  between two zips. Since P8-L3 `fs::archive::tests::archives_inside_archives_list_from_their_copies`
  makes a zip holding a zip and a 7z holding it in `%TEMP%\twinfm-p8l3-nested-<pid>\` (copies in
  `%TEMP%\twinfm-nested-<pid>\`, removed by the test), and `zip_jobs_on_the_worker` copies from a 7z
  into a zip (a temporary `%TEMP%\twinfm-copy-<pid>-<job>\`). Since P11-M1
  `fs::content::tests::files_are_read_up_to_the_cap` writes text, UTF-16 and binary files in
  `%TEMP%\twinfm-p11m1-content-<pid>\`, and `fs::enumerate::tests::content_search_reads_text_files_and_stays_out_of_links_and_clouds`
  a tree with a junction to its root and a file marked offline in `%TEMP%\twinfm-p11m1-test-<pid>\`.
  Since P11-M2 `shellmenu::tests::a_menu_for_results_in_two_folders` builds a shell menu over files
  in two folders of `%TEMP%\twinfm-p11m2-menu-<pid>\`, and `dnd::tests::shell_data_object_offers_hdrop`
  also makes a data object over two subfolders of its scratch folder. Since P11-M3
  `fs::watch::tests::moving_the_watched_folder_to_another_parent_follows_it` and
  `a_subtree_watch_past_the_record_cap_gives_no_edits` (2,000 files) use the `fs::watch` scratch
  folder, and `worker_runs_real_shell_operations` also copies a file over itself (a replace). Since
  P11-M7 `logfile::tests::the_log_takes_stderr_rotates_and_cuts` writes `twinfm.log` and
  `twinfm.previous.log` in `%TEMP%\twinfm-p11m7-log-<pid>\` and points the test process's stderr at
  them while it runs. All remove their folders.
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
  gate build (`target-p10m3`, 4f07cba, after P10-M3); the previous one is deleted once the new one
  has served as a reference. When milestones run as parallel sessions, one coordinating session
  serializes the machine: every session reports before a gate build, a soak or a hand-over and waits
  for its reply; one soak at a time (the soaks share `%TEMP%\twinfm-m5` and the fence); nothing
  builds, soaks or opens a TwinFM window from "gate running" to "machine free" (`docs/phase-9-plan.md`
  § Phase 9 close). Console checks run AFTER the triplet, never before: a recursive
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
- Screenshots of a scripted run: the femtovg window captures blank (CopyFromScreen and
  PrintWindow alike). Run with `SLINT_BACKEND=winit-software` and capture with
  `PrintWindow(hwnd, hdc, 2)` from a script started in the background, finding the window by its
  title prefix `TwinFM - ` (2026-10-18, P7-M2's menu check). **The title bar is not in that capture**:
  `PrintWindow` draws a stale frame (it showed the bar dark after the theme had set it light, P10-M3), so
  the bar is checked by a `CopyFromScreen` of the window's top 60 px from `GetWindowRect`, with the real
  renderer; the client area stays the `PrintWindow` capture.

## License
The app is MIT. Slint is used under its Royalty-free 2.0 license, which allows a modified
Slint inside an application with attribution (the badge in `README.md`, and since M6b the
`AboutSlint` widget in the About dialog, F1 or "About TwinFM" at the foot of the sidebar). The
patches and fork commits are offered under MIT; see README § License before changing any of this.
The repository is public since 0.1.0 (2026-10-22). Releases are zips built by
`tools/package-release.ps1` (into the ignored `dist\`): the exe, `THIRD-PARTY-LICENSES.html` from
`cargo about` (`about.toml`, `about.hbs`), and `licenses/` (Slint's license, and the fonts, CMaps and
profile hayro embeds, which cargo-about does not see). A new dependency with a license outside
`about.toml`'s `accepted` list fails the packaging: decide it, then add it there.

## Delivery
Branch, PR, squash-merge on Jeremy's go-ahead, per the projects-root `CLAUDE.md`.
**Upstreaming is deferred** (2026-10-09): `docs/upstream-issue-draft.md` and `docs/patches/`
are the record. Do not post to slint-ui/slint from a session; Jeremy decides when.
