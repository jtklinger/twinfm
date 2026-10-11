# twinfm

TwinFM: dual-pane file manager for Windows 11 in Rust + Slint. Started as the one-week
prototype `rustfm-proto` (renamed 2026-10-09; stack adopted the same day, see the recommendation
in `PROTOTYPE-RESULT.md`). Crate and exe are `twinfm`; the Slint fork branch keeps its `rustfm` name.
`PROTOTYPE-RESULT.md` is the gate record, `docs/week-one-plan.md` the week-one record
(accessibility fix, renderer decision), `docs/phase-1-plan.md` the Phase 1 record (six
milestones, gate re-run after each), `docs/phase-2-plan.md` the Phase 2 record. Read them before
changing the list.

## Gates (run before every commit)
- `cargo test --release` (83 tests; four touch the real shell and clipboard, and the eight
  `fs::watch` tests, one undo test and the breadcrumb dropdown's test write to scratch folders,
  see below)
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
  `[busy]`, `[split]`, `[enum]`, `[sort]`, `[select]`, `[hidden]`, `[refresh]`, `[icons]`, `[watch]` lines. Driver
  keys include `CtrlH` (hidden files), `F5`/`CtrlR` (refresh), `CtrlA`, `Space`,
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
  pane's folder; `|` separates paths), and `dragrow <pane> <row> <dx> <dy>` starts a real OLE
  drag followed by `sleep` and `dragcancel` steps, which run on a helper thread because OLE's drag
  loop runs no Win32 timer. A scripted drag can only be cancelled, never dropped (a scripted drop
  in an RDP session once landed on another program's window; see the M5 record). `[dnd]` lines on
  stderr. Tabs and path bar: `tab <new|close|next|prev|N>` presses Ctrl+T, Ctrl+W, Ctrl+Tab,
  Ctrl+Shift+Tab or Ctrl+N on the active pane (key names `CtrlT`, `CtrlW`, `CtrlTab`,
  `CtrlShiftTab`, `Ctrl1` to `Ctrl9`, and `CtrlL` for the path entry), `tab moveleft|moveright`
  presses Ctrl+Shift+PageUp / PageDown (key names `CtrlShiftPageUp`, `CtrlShiftPageDown`) to move
  the active tab, `tabdrag <pane> <index> <dx>` drags tab `<index>` (0-based) `<dx>` px through
  Slint's pointer path, `midclick <pane> <row>` middle-clicks a row (a folder opens in a new tab),
  `crumb <pane> <index>` clicks a breadcrumb button through its callback, `crumbmenu <pane>
  <index>` opens the dropdown of crumb `<index>`'s subfolders (listed on a worker: `sleep` before
  picking), `crumbpick <name>` picks one and `crumbclose` closes it, `pathmode <pane> entry|crumb`
  switches the path bar; `[tab]` (including `strip scrolled to <x>` when the tab strip
  overflows), `[crumb]` and `[open]` lines on stderr. `TWINFM_PERF_STATUS=1` adds the
  enumeration time to the status bar (the gate tools read `[enum]` on stderr, not the status
  bar). Sidebar and dialogs: `sidebar toggle` (F9), `sidebar click <row>` (headers count),
  `bookmark add` (Ctrl+D), `bookmark remove <index>`, `bookmark rename <index> <title>` (what the
  row's title editor commits; empty goes back to the folder's name), `bookmark move <index>
  up|down`, `about open` (F1) and `about close`; `window max|restore|close` maximises or restores
  the window, or closes it through `WM_CLOSE` (the close button's path, which saves the settings;
  `quit` does not); `[sidebar]` and `[settings]` lines on stderr. Directory watching: `touch <pane> <name>` creates an
  empty file in the pane's folder (or sets an existing one's modified time to now), `rmfile <pane>
  <name>` deletes it, and `waitwatch <ms>` waits until the next refresh started by the watch has
  listed its folder (at most `<ms>`), then 300 ms more for the pass, like `waitops`. `touch` and
  `rmfile` obey the `TWINFM_SOAK` fence as file operations do; outside a soak, point them at
  `%TEMP%` scratch folders only. `[watch]` lines on stderr: `watching <path>`, `no watch for <path>:
  <reason>` (This PC, network drives and shares, removable drives and discs, a folder that cannot
  be opened; logged once per pane and path), `changed: refresh`, `changed while listing: refresh after it`, `changed in the
  background: stale`, `lost <path>: <error>`; `[devices]` lines for volume arrivals and removals.
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
  then holding the normal placement. `SLINT_ACCESSKIT_STATS=1` prints accessibility
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
  for a data object over two files in `%TEMP%\twinfm-m5-test-<pid>\`. The `fs::watch` tests create,
  rename, delete and change attributes of files in `%TEMP%\twinfm-p2m1-test-<pid>\` (removed
  afterwards); they touch neither the clipboard nor the Recycle Bin.
  `pathbar::tests::subfolders_for_the_crumb_dropdown` makes folders (two of them hidden and
  system) in `%TEMP%\twinfm-p2m3-crumbs-<pid>\` and removes them.
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
