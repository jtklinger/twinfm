# Phase 2: from daily driver to trustworthy every day

Written 2026-10-10, when Phase 1 closed with M1 to M6 merged and the full M6 build at 19.8% on
the gate (see `phase-1-plan.md` § Phase 1 close). Phase 1 made TwinFM usable for a day instead
of Files. Phase 2's goal: **TwinFM is trustworthy for every day.** The list stays true to the
disk without F5, file operations are visible and undoable inside the app, and the rough edges
found in daily use are closed, all without giving back the gate.

Five milestones, in the recommended order (confirmed by the lead as the Phase 2 scope; Jeremy
can still reorder them):

1. **P2-M1, directory watching** (soak). `ReadDirectoryChangesW` on one worker, the active tab
   of each pane watched, changes coalesced into one refresh per 250 ms through the existing
   refresh path, `WM_DEVICECHANGE` for the drive lists.
2. **P2-M2, operations inside TwinFM.** Read back the real new name after rename and new folder,
   move the cursor after a delete or move, scroll the new row into view, clear cut marks when
   another program takes the clipboard, Undo (Ctrl+Z); a visible queue with cancel as the stretch
   item.
3. **P2-M3, daily-use polish** from the Phase 1 deferred lists, in the order the gaps annoy after
   a week of use (tab overflow and reorder, restoring tabs, middle-click into a tab, breadcrumb
   dropdown, folder sizes, a settings dialog, and so on).
4. **P2-M4, drag and drop finish** (soak). Drag images, highlight during our own drag, right-button
   drag menu, auto-scroll, the sidebar and tabs as drop targets.
5. **P2-M5, icons and views.** Overlays, reparse tags told apart, a timeout or second worker for
   slow icons, and a large-icon view only if Jeremy wants one (it starts with a trace).

**The gate is re-run after every milestone**: `tools/wpr-gate.ps1` elevated at the console by
Jeremy, on a clean binary built into its own `--target-dir target-<name>` from a detached
`git worktree add` checkout, default features, Narrator off, the split-and-scroll scenario on
`C:\traces\bigdir`. Bar: main thread under 20% busy over the 10 s after the split; the full M6
build read 19.8%, so the headroom is 0.2 point. A milestone is not closed until its run is
recorded here. When a milestone fails: stop, symbolize, name the frames, and either fix the
mechanism or report the options; never trade the bar off silently.

The architecture rules from Phase 1 hold (see `phase-1-plan.md` and the kickoff brief they came
from): only the UI thread touches a Slint model and workers wake it with
`slint::invoke_from_event_loop`; every COM object lives on the thread that made it; no app-state
`RefCell` borrow is held across a modal loop; no `unwrap` on `Weak<App>`; `windows-sys` with
hand-written calls; nothing per frame or per row without a trace first.

Amendment (P2-M4): the modal-loop rule is now load-bearing, not precautionary. Since P2-M4 our
own `DoDragDrop` runs from a posted message outside winit's handler, so `invoke_from_event_loop`
closures (enumeration results, job completions, icon replies, watch and clipboard events) run
INSIDE our own drag loop. Every such closure takes `with_state`; the drag code must therefore
hold no borrow while the loop runs (it takes everything out first and checks `state_is_free`),
and no closure may itself open a modal loop. Context menus and the shell's dialogs still run
inside a handler or on the worker and are unchanged.

## P2-M1: directory watching

### What landed (2026-10-10)

- **`src/fs/watch.rs`: one watcher thread for the app** ("dirwatch"). Every watch is a directory
  handle (`CreateFileW` with `FILE_LIST_DIRECTORY`, `FILE_FLAG_BACKUP_SEMANTICS |
  FILE_FLAG_OVERLAPPED`, shared for read, write and delete) associated with one I/O completion
  port. The thread loops on `GetQueuedCompletionStatus`; the UI thread's commands (`watch`,
  `unwatch`, quit) travel over a `crossbeam_channel`, announced by `PostQueuedCompletionStatus`
  with completion key 1; the wait's timeout is the coalescing timer. Notify filter: file name,
  directory name, attributes, size and last write, no subtree, 64 KiB buffer (a `Vec<u32>`, so
  DWORD-aligned). Records are not parsed: a completion, an empty one (overflow) or
  `ERROR_NOTIFY_ENUM_DIR` all mean "changed".
- **Watch identity.** Each watch is a `Box<Watch>` with its `OVERLAPPED` as the first field; the
  `OVERLAPPED` pointer a completion carries is the watch. Replacing or removing a watch calls
  `CancelIoEx` and `CloseHandle` and keeps the Box on a dying list until its cancelled read
  completes (`ERROR_OPERATION_ABORTED`), so the kernel never writes into freed memory and an
  old completion can never be taken for a new watch at a recycled address. A completion for a
  live watch that is not a change (any other error), or a failure to re-issue the read, closes
  the watch and sends `Lost`; a deleted watched folder shows up as `ERROR_ACCESS_DENIED` on the
  next read (the folder is delete-pending while our handle is open).
- **Coalescing**, per watch: the first change after a quiet spell is delivered 250 ms later (the
  trailing edge, so a burst is one event); changes that keep coming are delivered once per 250 ms.
- **No watch, gracefully**: This PC (or an empty path), network drives (`GetDriveTypeW ==
  DRIVE_REMOTE`), UNC paths, and any failing `CreateFileW`, port association or first
  `ReadDirectoryChangesW` give no watch and one `[watch] pane=N no watch for <path>: <reason>`
  line (once per pane and path). Removable drives and discs (`DRIVE_REMOVABLE`, `DRIVE_CDROM`)
  are not watched either, so TwinFM never holds a directory handle that would make Windows refuse
  to eject them. F5 works as before everywhere. The thread runs with `SEM_FAILCRITICALERRORS`.
- **API**: `Watcher::spawn(tx, wake)`, `watch(pane, tab_id, path)` (at most one watch per pane;
  the same folder again only updates the tab id and keeps the handle), `unwatch(pane)`, `stop()`
  (also on drop), and `Event::{Changed, Lost}` with the pane and tab id, delivered over a channel
  and followed by `wake()`, as `fs::enumerate` does.
- **Wiring in `main.rs`.** `State` owns the `Watcher` and its event receiver; `drain` (the one wake
  path) handles watch events after the enumeration messages, collects the panes to refresh and
  calls `refresh` once the state borrow is released. The watch follows each pane's active tab: it
  is pointed at the folder on `navigate`, on `show_tab` (tab switch, open, close) and again when the
  listing's `Done` arrives without error; a `Done` with an error unwatches the pane. While the
  split is off the second pane watches nothing.
  - `Changed` for the active tab with no listing in flight: `refresh(pane)` (cursor, selection and
    scroll survive by name through the existing pass). With a listing in flight:
    `Tab.refresh_pending`, and the refresh runs when its `Done` lands. For a tab that is no longer
    active: `stale = true`.
  - `Lost`: a `[watch]` line and, for the active tab, one refresh, so the error (folder gone)
    shows in the status bar through the existing error path.
  - A tab that leaves the foreground (`switch_tab`, `open_tab`) is marked stale (see Decisions).
- **`WM_DEVICECHANGE`** (`src/devices.rs`): a permanent comctl32 subclass on the main window (id
  "twdv", beside shellmenu's temporary "twfm"), installed where `st.hwnd` is first obtained
  (`main_hwnd`). On `DBT_DEVICEARRIVAL` or `DBT_DEVICEREMOVECOMPLETE` for `DBT_DEVTYP_VOLUME` it
  logs a `[devices]` line and posts `volumes_changed` to the event loop, which restarts a 500 ms
  `slint::Timer`; when it fires, the sidebar's drive list is re-read and every visible tab showing
  This PC refreshes. The subclass procedure touches no app state (it can run inside a modal loop).
  New `windows-sys` feature: `Win32_System_IO` (completion ports, `CancelIoEx`).
- **Driver**: `touch <pane> <name>`, `rmfile <pane> <name>` and `waitwatch <ms>` (documented in
  `src/auto.rs` and `CLAUDE.md`). `touch` and `rmfile` act on the pane's active folder and refuse
  paths outside the soak folders under `TWINFM_SOAK=1`, as file operations do. `waitwatch` waits
  until the listing of the next watch-driven refresh completes (a counter bumped at its `Done`),
  then 300 ms for the pass.
- **Soak script** `tools/soak-p2m1.ps1` (see Scripted checks).
- **Tests**, 60 to 68, in `fs::watch::tests` on `%TEMP%\twinfm-p2m1-test-<pid>\`: one create gives
  exactly one `Changed` within 2 s; 50 rapid creates and deletes give between 1 and 4 events;
  rename and an attribute change each give `Changed`; after `unwatch` a change gives nothing in
  500 ms; replacing a pane's watch moves the events to the new folder only; a missing folder, This
  PC, an empty path and a UNC path get no watch (and the watcher still works afterwards); deleting
  the watched folder gives `Lost`; dropping the `Watcher` stops the thread within 3 s. A
  test-only `Sync` command makes "the watch is armed" a fact rather than a sleep. The suite takes
  about 2 s and was stable over repeated runs.

### Decisions

- **One thread and one completion port for every watch**, rather than a thread per watch or
  completion routines: the thread count stays fixed, cancellation and replacement are serialised
  on one thread, and the coalescing timer is the wait's timeout, so an idle watcher costs no
  wake-ups at all.
- **Only the active tab of each pane is watched.** Background tabs are not watched; at most two
  directory handles are open.
- **250 ms trailing-edge coalescing.** A burst becomes one refresh after it ends; a continuous
  stream still refreshes four times a second at most, and a refresh that finds a listing in flight
  waits for it (`refresh_pending`), so a pane never has two listings of its own running.
- **A tab that leaves the foreground goes stale.** Its watch moves to the tab that takes its place,
  so TwinFM can no longer know what happened to its folder; it refreshes when shown, through the
  existing stale path. The cost: every tab switch re-enumerates the incoming tab (the old rows stay
  on screen until the new listing is sorted, as for F5, so nothing flashes).
- **No record parsing.** A whole-folder refresh is the delivery mechanism, so the cursor,
  selection and scroll survive through the code that already restores them by name. Targeted row
  updates are deferred.
- **Network drives and shares are not watched.** Change notification over SMB depends on the
  server, and a dead share can stall the calls; F5 still works there.
- **Removable drives and discs are not watched**, as the kickoff brief specified. An open
  directory handle on a USB drive makes "Safely remove" report the device as in use; the lead
  reversed the implementer's choice to watch them before the PR went up. Watching them properly
  means `RegisterDeviceNotification` with `DBT_DEVTYP_HANDLE` and closing the handle on
  `DBT_DEVICEQUERYREMOVE` (Deferred).

### Deviations

- **The watch starts at `navigate`, not only at `Done`.** The brief said to start watching when
  the listing completes without error. Watching from the navigation on means a change made while
  the listing runs is caught (it sets `refresh_pending`) instead of being lost in the gap between
  `read_dir` passing a name and the watch starting. A folder that cannot be listed is still not
  watched: its `Done` carries the error and unwatches the pane (and a folder that cannot be listed
  usually cannot be opened with `FILE_LIST_DIRECTORY` either). `Done` without an error asks again,
  which re-creates a watch that was lost or refused meanwhile and is otherwise a no-op.
- **Same folder, same handle.** A `watch` for the folder a pane already watches only updates the
  tab id, so the refresh after every change does not cancel and reopen the handle (and miss
  changes in between).
- **No `join` when the watcher is dropped.** The first scripted run panicked at exit ("threads
  should not terminate unexpectedly"): the app state lives in a thread-local whose destructor
  runs after Windows has terminated the other threads, and joining a terminated thread panics in
  std. `Watcher::stop` waits for an exit signal with a 1 s timeout instead of joining, and `main`
  calls it after the event loop ends, while the thread is still alive.
- **The hidden second pane is unwatched** while the split is off (turning the split on navigates
  it, which watches again), so changes to a folder nobody sees do not re-enumerate it.
- **The "no watch" line is logged once per pane and path**, not on every request: a This PC tab
  would otherwise log it at every navigation, `Done` and refresh.

### Deferred

- **Parse the records** (`FILE_NOTIFY_INFORMATION`) for targeted row updates: add, remove and
  rename one row instead of re-enumerating the folder. Worth it for large folders that change
  often (a download folder with 40k entries re-lists in 55 to 75 ms on the worker per change).
- **Watching background tabs** instead of marking them stale (a handle per tab, a cap on the
  count, and no refresh until shown).
- **Subtree watches** (sizes or counts of subfolders), not needed until folder sizes exist.
- **Network shares**: `ReadDirectoryChangesW` over SMB, with a timeout and a health check.
- **Watching removable drives.** They refresh with F5 only. Explorer watches them and still
  allows ejection by registering the handle (`RegisterDeviceNotification` with
  `DBT_DEVTYP_HANDLE`) and closing it on `DBT_DEVICEQUERYREMOVE`; TwinFM would need the same
  before it can watch them. Not checked with a real drive (none attached in the session).
- **Renaming or moving the watched folder itself** is not noticed: the handle follows the folder,
  so its changes keep arriving while the tab's path names a folder that no longer exists, until the
  next refresh shows the error.
- **A refresh after our own operations runs twice**: `job_done` refreshes both panes at once, and
  the watch delivers the same change 250 ms later. Harmless; suppressing it needs a "refreshed
  after the change" check.
- **No back-off for folders that change continuously** (a log folder, `%TEMP%`): they refresh up to
  four times a second for as long as they are shown. No measurement of what that costs the UI
  thread on a 40k folder yet.

### Scripted checks

RDP session, `target/release` build of this branch:

- Scratch folders `%TEMP%\twinfm-p2m1-manual\a`, `b`, `c` (not a soak, no fence). Script: split,
  pane 1 on `c`, `touch 0 x.txt`, `waitwatch`, `touch 0 x.txt` (time update), `waitwatch`, `rmfile 0
  x.txt`, `waitwatch`, F5, `touch 0 y.txt`, a new tab and back and forth, close it, This PC, `b`,
  split off, `touch 1 hidden.txt`, quit; meanwhile a separate shell created `a\external.txt` at
  about 9 s and deleted `c` at about 12 s. Each `touch`/`rmfile` gave one `[watch] pane=0 tab=1
  changed: refresh` and a listing of the new count (4, 4, 3, 4 entries); the other program's file
  gave a refresh to 5 entries with no script step involved; each tab switch refreshed the incoming
  (stale) tab; deleting `c` gave `lost ...: Access is denied. (os error 5)` and one refresh showing
  the error; This PC gave one `no watch for This PC` line; after the split went off, `touch 1` gave
  no event. Exit code 0, no panic (after the shutdown fix in Deviations).
- Soak, 2 minutes (`tools/soak-p2m1.ps1 -Minutes 2`, the 30-minute run is pending): exit code 0,
  0 `panicked` lines, 9 cycles, 1,774 writer operations, 298 `[watch]` lines (291 changes
  refreshed, 4 deferred behind a listing in flight, 0 lost, 0 refused), 311 `[refresh]` lines, 0
  `waitwatch` timeouts, 0 soak refusals; working set 136.1 MB at 6 s, 142.0 MB at 120 s (private
  122.0 to 124.1 MB). Two minutes says nothing about a memory trend; the 30-minute soak decides.
- Not checked by hand: a file made in Explorer while TwinFM shows the folder (the harness cannot
  drive Explorer; the separate-shell write above is the same path, another process's change), and
  a USB drive arriving or leaving.

### Gate

Console WPR, Jeremy elevated, `tools/wpr-gate.ps1`, accessibility on, Narrator off, hidden off,
binaries built into their own `target-<name>` from detached worktrees (`target-p2m1` = merge
commit ff594d4, `target-m6` = 3f622dc), analysed with `tools/analyze-trace.py --symbols --exe`.

- **First run (2026-10-10 13:20)**: P2-M1 **20.4%** (2040 sampled ms; in-app 22.7%), against
  the recorded M6 figure of 19.8%. Over the bar by 0.4. Symbolized: no P2-M1 frame anywhere in
  the 10 s window. The watcher thread never woke (`C:\traces\bigdir` does not change during the
  run; no `[watch] changed` line), and the permanent `WM_DEVICECHANGE` subclass costs 2 samples
  (`COMCTL32!MasterSubclassProc` and `DefSubclassProc`, 1 exclusive each; AccessKit already
  subclassed the window before P2-M1, so the chain only grew by one link). The extra 60 ms over
  M6 was in ntdll heap alloc and free under the text path and in the kernel's stack walks for the
  profiler, both of which move run to run. Trace `C:/traces/twinfm-p2m1.etl`.
- **Same-session A/B (2026-10-10 15:17 to 15:19)**, three runs back to back, M6 first:

  | Build | Sampled ms | WPR busy | In-app |
  |---|---|---|---|
  | M6 (`target-m6`) | 1985 | **19.9%** | 20.5% |
  | P2-M1 (`target-p2m1`) | 2003 | **20.0%** | 23.4% |
  | M6 again | 2025 | **20.2%** | 23.6% |

  P2-M1 sits inside M6's own spread (19.9 to 20.2). Frame shares are the same build to build
  within a few samples: accessibility 220 / 226 / 220 ms, render 1154 / 1152 / 1178, draw_text
  668 / 640 / 650, ListView 365 / 362 / 369, timers 694 / 698 / 713. Traces
  `C:/traces/twinfm-ab-{m6,p2m1,m6b}.etl`, symbolized reports under `C:/traces/export/`.

**P2-M1 passes the gate**: no cost attributable to directory watching or the device subclass.
Two facts for every later gate: a single console run moves by about 0.3 point on the same binary
(M6 read 19.8, 19.9 and 20.2 across three sessions and runs), so the bar has no headroom left to
speak of and a difference under half a point needs an A/B in one session, not a single run
against a recorded number. And the binary matters: the main checkout's `target\release` is
rebuilt only when someone builds there, so a feature test from it can run a pre-merge build (the
USB check below did).

### Manual check

- **USB drive (2026-10-10)**: Jeremy inserted and removed a drive twice with TwinFM on This PC
  and nothing refreshed. The binary was `target\release\twinfm.exe` built at 12:07, before P2-M1
  merged at 13:12, so it had no device code. The path was then exercised from the harness with a
  synthetic `WM_DEVICECHANGE` (`DBT_DEVICEARRIVAL` then `DBT_DEVICEREMOVECOMPLETE`,
  `DBT_DEVTYP_VOLUME`, unit mask for Z:) sent to the window of the merged build: `[devices]
  volume arrived`, 500 ms later `drives changed: drive list re-read, This PC refreshed in 1
  pane(s)`, `[refresh]`, `[sidebar] 2 drive(s)`, and the same for the removal. That proves the
  subclass, the debounce and the refresh. Jeremy then repeated the real-drive test on the
  rebuilt binary (`target\release` from ff594d4): insert and remove both refreshed the sidebar
  and This PC without F5. **Passed.**
- **30-minute soak (2026-10-10 15:58 to 16:28, `tools/soak-p2m1.ps1 -Minutes 30`, Jeremy's
  console, `target\release` from ff594d4)**: exit clean, **0 `panicked` lines**, 4,785 `[watch]`
  lines (4,760 changes refreshed, 18 deferred behind a listing in flight, 0 lost, 0 refused),
  5,110 `[refresh]` lines, 0 `waitwatch` timeouts, 0 soak refusals. Private memory 117.6 MB at 6 s,
  119.6 at 5 min, 121.7 from 15 min on, 121.9 at the end: flat after the caches fill, no trend.
  Working set 130.7 MB to 115.2 MB. **Passed.** P2-M1 is closed: gate, soak and the real-drive
  check all recorded.

## P2-M2: operations inside TwinFM

The backlog was the M3 Deferred list (`phase-1-plan.md` § M3): the real new name, the cursor
after a delete or move, the new row scrolled into view, cut marks that outlive the clipboard,
undo, and a queue TwinFM can see and cancel.

### What landed (2026-10-10)

- **Real names through `IFileOperationProgressSink`** (`src/fileops/mod.rs`). A hand-written
  sink (`#[repr(C)]` vtable in declaration order, refcount, `QueryInterface` for `IUnknown` and
  the sink's IID), made per job on the STA worker, passed to `IFileOperation::Advise` before
  `PerformOperations` and `Unadvise`d right after, released on the worker. `PostRenameItem`,
  `PostMoveItem`, `PostCopyItem`, `PostDeleteItem` and `PostNewItem` record an `ItemResult
  { source, dest, hr }`; the paths come from `IShellItem::GetDisplayName(SIGDN_FILESYSPATH)`
  (freed with `CoTaskMemFree`). The shell also reports a folder's contents one by one (a copied
  folder with one file gave two `PostCopyItem` calls), so `own_results` keeps only the job's own
  items: a result whose parent is a source's parent and whose path is a source, compared ignoring
  case and, failing that, through the canonical parent (`%TEMP%` can be an 8.3 path). `Done`
  gained `results`; `real_name` gives the name the shell really used for a rename or a new
  folder. `job_done` puts `focus_name` on it, and the status reads "Created New folder (2)".
  A `[fileops]   item 0x<hr> <source> -> <dest>` line per result. A recycled item's `dest` is its
  place in the Recycle Bin (`C:\$Recycle.Bin\<sid>\$R....txt`); a permanent delete has none.
- **Cursor to the next row after a delete or move** (`model.rs`, `main.rs`). Before a delete,
  Shift+F6 or a paste of a cut, `PaneModel::cursor_after_removal` records the cursor row's index
  less the removed rows above it; for a paste it is computed on the tab that holds the cut marks
  (`State::cut_source`). The job's `JobInfo` carries it; `job_done` hands it to that tab as
  `fallback_cursor`; the next `request_pass` puts it in `Restore::fallback_index`, consumed once.
  `restore_by_name` uses it only when the cursor's name is gone, clamped to `len - 1`; no rows, no
  cursor. Watch-driven refreshes never set it. Undoing a copy or a new folder (a recycle in the
  pane's own folder) gets one too.
- **The operation's row scrolled into view.** When a pass placed the cursor from `focus_name` or
  the fallback, `land_pass` calls `model::reveal_row` once (the pane's `bring-into-view`
  arithmetic in Rust: 22 px rows, content-y from `get_scroll`, visible height from the pane's last
  `viewport-changed`) and sets the scroll only if the row is not fully visible, with the least
  movement. A `[refresh] pane=N cursor row R scrolled into view: <y> -> <y>` line.
- **Cut marks clear when another program takes the clipboard.** `devices::install` also calls
  `AddClipboardFormatListener`; the same permanent subclass posts `crate::clipboard_changed` on
  `WM_CLIPBOARDUPDATE`. `clipboard_put` records `GetClipboardSequenceNumber` right after its
  write (outside the state borrow, as before); `clipboard_changed` clears the cut marks when the
  number differs and a cut is showing (`clipboard::taken_from_cut`), with a `[clipboard]` line.
  The listener is never removed (process exit).
- **Undo, Ctrl+Z** (`src/fileops/undo.rs`). `State.undo` holds up to 20 `Record { undo, text }`,
  pushed by `job_done` for successful jobs from the sink's real paths: a move moves the moved
  items back to their folder, a rename renames back, a new folder is recycled if still empty, a
  copy's copies are recycled. A delete to the Recycle Bin pushes a record that undoes nothing and
  says "Cannot undo a delete yet (use the Recycle Bin)"; a permanent delete pushes nothing.
  Ctrl+Z (list FocusScope, either pane, one shared stack) pops the newest record, refuses with a
  note when an expected path is gone or the new folder is no longer empty (`undo::check`, read
  at undo time), and otherwise submits the inverse through `submit`, so the soak fence and the
  confirmations apply as to any job. `JobInfo.undo_of` marks it: its completion is not recorded
  (no redo) and the status reads "Undid: <original status>". Driver step `undo` (key `CtrlZ`),
  `[undo]` lines.
- **Stretch, done: a visible queue with cancel.** The status bar of both panes appends "N
  operations queued" while jobs wait behind the running one (`statusbar::with_queue`, composed in
  `update_status`, which runs on submit and on every job's refresh, never per frame). Escape in
  the list calls `Worker::cancel_queued(last id)`: the worker reports every job up to that id that
  has not started as `Cancelled` without running it; the running job is untouched (its own
  dialog cancels it).
- **Tests**, 68 to 76: the real-shell test now asserts the sink's results (the new folder's path,
  the copy's, the renamed path, the moved path, "a - Copy.txt", "made (2)" with
  `real_name`/`status_text`, a copied folder reported once without its contents, one result per
  deleted source) and runs an undo round trip (move a file, `undo_for`, `check`, run it, the file
  is back); `cancelled_queued_jobs_never_run` (cancel up to id 2, submit three new folders: two
  `Cancelled` and absent, the third made); `cursor_falls_back_to_the_next_row`;
  `reveal_row_moves_the_least`; `undo::which_job_undoes_which`, `records_and_the_cap`,
  `check_refuses_missing_and_non_empty`; `foreign_clipboard_change_clears_a_cut_only`;
  `queue_text`. Every path the shell hands back is compared canonicalised.

### Decisions

- **Results filtered on the worker**, not in `job_done`: the UI side never sees a folder's
  contents, and the filter's `canonicalize` calls (only for top-level candidates) stay off the UI
  thread.
- **The fallback index subtracts the removed rows above the cursor**, rather than the brief's
  plain `min(old_index, len - 1)`: with rows 3, 4 and 5 deleted and the cursor on 5, the plain
  index would land two rows past the old row 6; this lands on it. The clamp to `len - 1` is the
  brief's.
- **A delete leaves a record that says it cannot be undone** instead of nothing. With nothing
  recorded, Ctrl+Z after a delete would silently undo the operation before it. Ctrl+Z pops it
  and shows the note; the next Ctrl+Z reaches the older records. A permanent delete records
  nothing, as briefed; `undo::check` refuses any older record whose items it removed.
- **A refused undo drops its record**: the items are not where the job left them, so it could
  never succeed later.
- **The undo note goes to the pane Ctrl+Z was pressed in**; both panes refresh as for any job.
- **A move gathered from several folders (a paste) records nothing**: there is no single folder
  to move it back to. Splitting it into one move per folder is possible but was not needed.
- **Escape cancels everything queued at that moment** (ids up to the newest), not the newest one
  only. The id check happens on the worker right before a job would start, so there is no race
  with the job that is running.

### Deviations

- **The fallback rule** (above): removed rows above the cursor are subtracted.
- **A rename onto a taken name did not "ask"** in the scripted check: renaming `f03.txt` to the
  existing `f01.txt` brought up the copy engine's unowned "1 Interrupted Action" window
  (`OperationStatusWindow`), not a confirmation dialog; closing it gave `Cancelled`, with two sink
  results (`0x0027000B` and `0x80270000`) and nothing renamed. The M3 record says a rename onto an
  existing name "still asks, as in Explorer"; it asks through that window.
- **`own_results` was not in the brief**; without it a copied folder's every file would become a
  result (and an undo item).
- **`State::cut_source`** (the pane and tab holding the cut marks) was added for the paste's
  fallback and for "TwinFM has cut marks"; navigating that tab clears it.

### Deferred

- **Redo.**
- **Undo of a delete to the Recycle Bin.** The sink already reports each recycled item's place in
  the Recycle Bin; restoring it means the Recycle Bin folder's "undelete" verb on that item (or a
  move back from `$Recycle.Bin`, which loses the bin's own metadata). Not checked here.
- **Undo of a move that renamed on collision** moves the item back under its new name ("a (2).txt");
  `MoveItem`'s `pszNewName` could restore the old one.
- **Undo of a copy or move that replaced an existing file** recycles the copy (or moves the item
  back) but does not restore the file it replaced; Explorer's undo does not either, but it is worth
  saying. The replaced file is in the Recycle Bin only if the shell put it there.
- **Dimming the cut row's icon** (only its text is dimmed, since M3).
- **Progress inside TwinFM**: the shell's window still shows it; the status bar shows the queue
  count, not a percentage (`UpdateProgress` on the sink could feed one).
- **Explorer's own Undo never sees TwinFM's operations** (checked by Jeremy 2026-10-10: a rename in
  TwinFM, then Explorer's context menu and Ctrl+Z offered nothing). The shell's undo list is per
  process: `FOFX_ADDUNDORECORD` adds the record inside TwinFM's process, where only TwinFM could
  play it back, and there is no public API to do that. This is why the TwinFM-side record exists;
  the flag stays set because it costs nothing and keeps the shell's own bookkeeping consistent.
- **A Narrator check** of the new notes ("Undid: ...", the queue count).

### Scripted checks

RDP session, `target\release` of this branch, scratch `%TEMP%\twinfm-p2m2-manual\a` (f01 to f10)
and `\b`, no `TWINFM_SOAK`. A separate PowerShell closed any dialog of the process once it
appeared (WM_CLOSE) and ran `Set-Clipboard "x"` a second after the cut. Script: split, pane 1 on
`b`, `newfolder New folder` twice, rename the row `f03.txt` to `f01.txt`, delete the cursor row,
`cx`, move a file to `b` then `undo`, `newfolder zz` then `undo`, delete then `undo`, quit. stderr:

- `Done: Created New folder`, then `Done: Created New folder (2)` with the item line
  `-> ...\a\New folder (2)` and the next pass `cursor=Some(1) selected=1` (the real name).
- The rename onto `f01.txt`: the "1 Interrupted Action" window, closed, `Cancelled: Cancelled`.
- Delete with the cursor on row 4 (`f03.txt`): `Deleted 1 item to the Recycle Bin`, the pass
  `11 of 11 entries ... cursor=Some(4)`: the next row (`f04.txt`).
- `Cut 1 item to the clipboard`, then `[clipboard] another program took the clipboard: cut marks
  cleared` (TwinFM's own write before it did not trigger it).
- Move `f05.txt` to `b` (`cursor=Some(5)`, the next row), `[undo] recorded "Moved 1 item": Some(Move
  { sources: [...\b\f05.txt], dest_dir: ...\a })`, Ctrl+Z, `[undo] done id=6 Done: Undid: Moved 1
  item`, `11 of 11 entries`: the file is back.
- `Created zz`, Ctrl+Z, `Undid: Created zz` (the item line shows zz going to `$Recycle.Bin`); in a
  second run the cursor then went to the next row (`cursor=Some(2)`), and a further Ctrl+Z said
  `Nothing to undo`.
- Delete, Ctrl+Z: `[undo] "Deleted 1 item to the Recycle Bin" cannot be undone` and the note
  "Cannot undo a delete yet (use the Recycle Bin)".
- Scroll: after a wheel to the bottom of 62 rows, renaming the row there to `aaa.txt` (which sorts
  to row 3): `cursor row 3 scrolled into view: -650 -> -66`; the watch's refresh after it did not
  scroll.
- Queue: a copy onto an existing name (its "Replace or Skip Files" window left open), two more
  copies, Escape: `1 operation(s) queued`, `2 operation(s) queued`, `Escape: cancelling 2 queued
  operation(s) up to id 3`; after the window was closed all three reported `Cancelled` and
  nothing was copied.

All exit code 0, no panic. These runs put about five entries in the Recycle Bin (scratch files and
folders named f02, f03, f05, zz) and left "x" on the clipboard.

### Gate

Console WPR, Jeremy elevated, `tools/wpr-gate.ps1`, accessibility on, Narrator off, hidden off,
same session, back to back (2026-10-10 20:42 and 20:45): `target-p2m1` (ff594d4) first, then
`target-p2m2` (the PR #23 head, f95c35a). Analysed with `tools/analyze-trace.py --symbols --exe`.

| Build | Sampled ms | WPR busy | In-app |
|---|---|---|---|
| P2-M1 (`target-p2m1`) | 2041 | **20.4%** | 21.9% |
| P2-M2 (`target-p2m2`) | 2028 | **20.3%** | 21.2% |

**P2-M2 passes**: nothing attributable to the milestone. Frame shares P2-M1 / P2-M2: accessibility
232 / 228 ms, render 1193 / 1148, draw_text 664 / 653, ListView 349 / 369, timers 683 / 722, heap
209 / 220; no sink, undo, clipboard-listener or queue frame appears in either window (the sink runs
on the worker, the fallback and the reveal run once per pass swap, the listener wakes the UI only on
a clipboard change). The same P2-M1 binary read 20.0% in the afternoon A/B and 20.4% here, M6 read
19.9% and 20.2% within one session: the whole machine drifts by a few tenths between sessions, which
is exactly why the pass criterion is "inside the same-session reference", not "under 20.0 on one
run". Traces `C:/traces/twinfm-ab2-{p2m1,p2m2}.etl`, reports under `C:/traces/export/`.

The P2-M1 trace also contained a second `twinfm.exe` that Jeremy opened by hand at 29.6 s, after
the gate's process had exited at 21.9 s, which ran to the end of the trace; `analyze-trace.py` had
mixed both processes' threads and crashed on the live one's "MAX" end time. It now takes the
earliest-started TwinFM process (the one the gate script launched), ignores the others, and
tolerates threads still alive at trace end (fixed in this PR).

## P2-M3: daily-use polish

The backlog was the M4, M6a and M6b Deferred lists (`phase-1-plan.md`): tabs that survive a
restart, a tab strip that copes with many tabs, middle-click into a tab, the breadcrumb's sibling
dropdown, the maximised window, bookmark titles and order, and Explorer's right-click on empty
space.

### What landed (2026-10-10)

- **Tabs restored at start** (`settings.rs`, `main.rs`). `config.toml` gains, per pane, the tab
  folders left to right and the active index: `left_tabs = ['C:\', 'C:\Users']`,
  `left_active = 1`, `right_tabs`, `right_active`. At start each pane builds its tabs from the list
  (`settings::start_tabs`: blank entries dropped, the index clamped; an empty or missing list gives
  one tab on `start_left` / `start_right`, so older files work unchanged) with `Tabs::restored`;
  only the active tab navigates, the others are created stale and list their folder when first
  shown, through the existing stale path. A folder that no longer exists still gets its tab and
  shows the error when shown. A command-line folder replaces the left pane's tabs; the right pane's
  tabs are restored only when the split is (otherwise F3 opens the second pane on the left
  pane's folder, as before). `save_now` writes each pane's list, except a pane whose tabs never had
  a folder (the second pane before the split was first opened), which keeps the saved list. Saved
  through the existing debounced `schedule_save` on every navigation, tab open, close, switch and
  move. Scripted runs still ignore the file unless `TWINFM_CONFIG` names one.
- **Tab strip overflow** (`pane.slint`). Every tab has the same width, 160 px down to 120 px as
  tabs are added; past that the strip scrolls instead of shrinking or clipping. The strip is a
  clipped `Rectangle` whose tabs sit at `shown-tab-scroll + i * tab-w`; `tab-scroll` is moved by
  the wheel (a `TouchArea` behind the tabs, `scroll-event`, vertical or horizontal delta) and by
  `reveal-tab(i)`, which scrolls just far enough to show tab `i`. `shown-tab-scroll` and `tab-w`
  are bindings on the tab count and the pane's width, so a closed tab or a wider pane never leaves
  a gap. Rust calls `reveal-tab` from `update_tabs`, which runs only when the tabs change, and once
  more from `register_dnd` when the window first has its size (a restored strip whose active tab
  is far right). `[tab] pane=N strip scrolled to X (tab K of N in view)` on stderr when the offset
  is not 0.
- **Tab reorder.** `Tabs::move_tab(from, to)` (pure, the active tab stays active wherever it
  lands). A left press on a tab activates it, as in browsers (it used to switch on the release);
  moving with the button held reports the pointer's x in the tab, and the pane swaps the dragged tab
  with its neighbour once the pointer passes that neighbour's midpoint (several swaps in one drag).
  Ctrl+Shift+PageUp / Ctrl+Shift+PageDown move the active tab left or right (outer `FocusScope`,
  `tab-key` "moveleft" / "moveright"; no wrap at the ends). `update_tabs` now changes the rows that
  differ when the count is unchanged and resets the model only when a tab opens or closes (see
  Deviations).
- **Middle-click on a row** (`pane.slint`, `main.rs`). The row's `TouchArea` reports a middle
  release as gesture 4; `middle_click_row` opens a folder, a drive or a reparse directory in a new
  tab after the active one in the same pane (`open_tab`, as the sidebar's middle-click does). A file
  logs a line and does nothing.
- **Breadcrumb dropdown** (`pathbar.rs`, `pane.slint`, `app.slint`). Each crumb is now its label
  plus a chevron after it (This PC › C: › Users ›, Explorer's layout). Clicking a chevron lists that
  crumb's subfolders, which are the next crumb's siblings (after the last crumb, the current
  folder's own subfolders): `pathbar::subfolders` on a worker thread, folders only, hidden and
  system ones left out unless the pane's active tab shows hidden files, sorted with the list's
  `natural_cmp`, capped at 200 (`MENU_CAP`) with a "... and N more" line; This PC lists the drives.
  The listing comes back through `invoke_from_event_loop`; a reply for an older request is dropped
  (`crumb_seq`). One `PopupWindow` at the window level holds a `ListView` over a fresh string model,
  so its rows exist only while it is open; a click navigates the pane there and gives the list the
  focus, Escape (a `FocusScope` in the popup) or a click outside closes it. An unreadable folder
  shows its error as the popup's note, an empty one "No folders".
- **Maximised state** (`settings.rs`, `main.rs`). `WindowGeom` gains `maximized`
  (`#[serde(default)]`). `settings::placement` reads `GetWindowPlacement`: when `showCmd` is
  `SW_SHOWMAXIMIZED` the saved x, y, width and height are the normal placement from
  `rcNormalPosition` (workspace coordinates turned into screen ones with the monitor's work-area
  offset; the size less the window frame, which is `GetWindowRect` less `GetClientRect`, over the
  scale factor), so un-maximising after a restart gives the right size. Not maximised: the Slint
  size and position as before; minimised: the saved placement is kept. The normal placement is
  still applied before the window exists; once it exists (`register_dnd`, the timer that waits for
  the window) a saved `maximized = true` calls `ShowWindow(SW_MAXIMIZE)` and logs
  `[settings] window restored maximised`.
- **Bookmark rename and reorder** (`settings.rs`, `sidebar.rs`, `app.slint`). Bookmarks are
  `[[bookmark]]` tables with `path` and an optional `title`. The old bare list
  (`bookmarks = ['C:\x']`) still loads: it is read into a `legacy_bookmarks` field that is never
  written, appended to the entries (duplicates dropped) on load, and so written as `[[bookmark]]`
  tables by the next save. The bookmark menu (right-click a bookmark) has "Rename bookmark", "Move
  up", "Move down" (dimmed at the ends, from `bookmark-count`) and "Remove bookmark", as one
  `MenuItem` component. "Rename bookmark" opens a `LineEdit` in the sidebar row (`side-edit`, an
  `if` so it exists only while editing) with the title selected; Return commits through
  `bookmark-rename`, Escape or focus loss cancels. `sidebar::rename_bookmark` trims the title; an
  empty one or the folder's own name clears it, so the row shows the folder's name again.
  `sidebar::move_bookmark` swaps with the neighbour. The sidebar row label is the title when there
  is one. `[sidebar]` lines for each.
- **Right-click on empty space clears the selection** (M4 Deferred). `SelOp::Clear` (5) empties
  the selection and the anchor; the empty space's right press sends it before the background menu
  opens, and the driver's `rclickempty` fallback (no empty space on screen) sends it too.
- **The mouse's Back button goes up a folder** (added at Jeremy's request, 2026-10-12), as
  Backspace and Alt+Up do, in the pane under the pointer, which becomes the active pane as a click
  there would make it. Slint's winit backend delivers the side button as `PointerEventButton.back`;
  every `TouchArea` grabs whatever press it receives, so each one in the pane reports a Back release
  itself: the rows (gesture 5), the empty space below them, the header cells, the crumbs and their
  chevrons, the path bar's mode glyph, and a `TouchArea` behind everything for the padding, the
  status line and the gaps. All of them end in `back_button(pane)`, which calls the existing
  up-folder path (`go_up`, shared with Backspace and Alt+Up). On This PC it does nothing and logs
  one `[nav] ... This PC has no parent folder, nothing to do` line. Forward does nothing yet.
  Driver step `backclick <pane>` dispatches a real Back press and release at the middle of the
  pane's list.
- **Driver**: `tab moveleft|moveright` (key names `CtrlShiftPageUp`, `CtrlShiftPageDown`),
  `tabdrag <pane> <index> <dx>` (press on the tab's centre, move in 8 px steps, release, through
  Slint's pointer path), `midclick <pane> <row>`, `crumbmenu <pane> <index>`, `crumbpick <name>`,
  `crumbclose`, `bookmark rename <index> <title>`, `bookmark move <index> up|down`, and
  `window max|restore|close` (`ShowWindow`, or `WM_CLOSE`, which goes through the close request
  and saves the settings the way the close button does; `quit` ends the event loop without it).
- **Tests**, 76 to 83: `Tabs::restored` clamps the active index; `Tabs::move_tab` both ways, to
  either edge, another tab passing the active one from each side, out of range and no-op moves;
  the tab lists round trip through TOML and fall back to `start_left` / `start_right` when missing
  or blank; `[[bookmark]]` entries round trip (no `title` line without a title, the old key never
  written), the old bare list loads and converts, both together keep the entries first and drop
  duplicates; bookmark rename (trim, clear by empty or own name) and move (both ways, both edges,
  the title travels); `pathbar::subfolders` on a scratch tree in `%TEMP%\twinfm-p2m3-crumbs-<pid>\`
  (folders only, natural order, hidden and system filtered or not, the cap and the full count, a
  missing folder is an error, This PC lists C:\); `WindowGeom` with `maximized` round trips and an
  older `[window]` table loads as not maximised; `SelOp::Clear` added to the right-click test.

### Decisions

- **Tab lists as string arrays plus an index** (`left_tabs = [...]`, `left_active = n`) rather
  than `[[left_tabs]]` tables: one line per tab, and nothing else per tab is saved yet (sort and
  hidden filter for new tabs stay pane-wide settings).
- **A command-line folder replaces the left pane's tabs**, as it replaced `start_left` before; the
  next save then holds that one tab.
- **The right pane's tabs come back only with the split.** Without it F3 keeps its behaviour (the
  second pane opens on the left pane's folder) and the saved right tabs are kept until the split
  is used again.
- **Equal tab widths** (160 px down to 120 px): the strip's arithmetic (scroll into view, drag
  midpoints, the driver's tab positions) is then a multiplication, and nothing is measured.
- **A tab activates on the press**, so a drag starts from the active tab and a click feels like a
  browser's.
- **A chevron after every crumb** rather than a clickable separator between two: it is Explorer's
  layout, and the chevron after the last crumb lists the current folder's subfolders, which a
  between-crumbs separator could not offer.
- **One dropdown for both panes** at the window level, filled per request.
- **`SelOp::Clear`** instead of clearing in `context_menu`: the keyboard menu also calls
  `context_menu(-1)` when there is no cursor row, and that must not drop a Ctrl+A selection.
- **A bookmark title equal to the folder's name is no title**, so renaming a bookmark back to its
  folder's name leaves no stale title behind if the folder is renamed later.

### Deviations

- **Not a Slint `Flickable`.** The brief named one; its drag-to-flick takes the pointer presses a
  tab drag needs. A clipped `Rectangle` with an offset and a wheel `TouchArea` behaves the same
  for scrolling (no scrollbar, the wheel scrolls it, the offset set only on tab changes).
- **The bookmark menu runs no modal loop.** The brief described it as a `TrackPopupMenuEx` loop;
  since M6b it is a Slint `PopupWindow` (`phase-1-plan.md` § M6b Decisions), so the borrow rule
  for modal loops does not come into it. The new items are in that popup.
- **`update_tabs` changes rows in place** when the tab count is unchanged. Found by the scripted
  `tabdrag` check: pressing an inactive tab switched to it, `set_vec` reset the strip's model, the
  repeater rebuilt its items, and the pressed `TouchArea` lost the pointer grab, so the drag never
  moved anything. `set_row_data` for the rows that differ keeps the items (and is less work).
- **The restored strip is revealed once more when the window exists**: before the first layout the
  panes have no width, `reveal-tab` returned nonsense offsets (now it does nothing at width 0).
- **The frame size comes from `GetWindowRect` less `GetClientRect`**, not `AdjustWindowRectEx`
  (that would need the window's styles and a DPI-aware variant); the difference is the same
  maximised as restored. Checked: a window saved restored and then saved maximised gave the same
  x, y, width and height (260, 260, 1200 x 800).
- **Extra driver steps** `tabdrag` and `window max|restore|close`, so the drag and the maximised
  round trip could be checked without a person.
- **The middle button is handled in the row's `TouchArea`** (gesture 4) rather than in
  `dnd::Gesture`, which tracks the left button's press, drag and release; a middle-click needs no
  state.

### Deferred

- **Folder sizes in the status bar** (M6a): needs a recursive size worker and, for watched
  folders, subtree watches.
- **A settings dialog**: everything is still chosen by using the app or editing `config.toml`.
- **Tabs as a drop target** (drop files on a tab to copy or move into its folder) and **the
  sidebar as a drop target**: P2-M4.
- **Explorer's "protected operating system files" switch** (M1 Decisions, M6b Deferred).
- **The compact Windows 11 menu (`IExplorerCommand`)** and **icons on TwinFM's own menu items**
  (M4 Deferred).
- **A designed icon** and **the toolbar** (M6b Deferred).
- **The tab strip does not re-reveal the active tab when the window is resized**; the offset stays
  in range, but a narrower window can leave the active tab outside the strip until the next tab
  change or a wheel.
- **The breadcrumb dropdown has no keyboard navigation** (arrows, type-ahead) and no drop onto a
  crumb.
- **Rename is reached only from the bookmark menu**; F2 in the sidebar does not start it.
- **Restored tabs keep only the folder**: their sort, hidden filter, cursor and scroll start fresh.
- **A back/forward history per tab**, and with it **Forward = history forward** (the mouse's
  Forward button does nothing yet) and Back as history back rather than up a folder.
- **The Back button over a text entry** (the path entry, the rename editor), **the tab strip or
  the list's scrollbar** does nothing: their own `TouchArea`s take the press and do not report it.

### Scripted checks

RDP session, `target\release` of this branch, scratch folders and configs in
`%TEMP%\twinfm-p2m3-manual\`, never Jeremy's config:

- **Tabs restored.** `TWINFM_CONFIG=cfg1.toml` (new), on `C:\`: `tab new`, `nav 0 C:\Windows`,
  `tab new`, `nav 0 C:\Users`, `tab moveleft` twice, `tabdrag 0 0 200`, `quit`. stderr: `[tab]
  pane=0 moved tab 3 to 2, tab 2 of 3 active`, `moved tab 2 to 1, tab 1 of 3 active`, `moved tab 1
  to 2, tab 2 of 3 active`; the file then held `left_tabs = ['C:\', 'C:\Users', 'C:\Windows']`,
  `left_active = 1`. Restart with the same file: `[tab] pane=0 restored 3 tabs, tab 2 active`,
  `[nav] pane=0 gen=1 path=C:\Users` (the only listing); `tab next` then gave `switched to 3 of 3
  (... C:\Windows)` and `[refresh] ... C:\Windows` (the stale path).
- **Split and a missing folder.** A hand-written file with `split = true`, `left_tabs = ['C:\',
  'C:\does-not-exist-p2m3']`, `right_tabs = ['C:\Windows', 'C:\Users']`, `right_active = 1`: both
  panes restored (`pane=0 restored 2 tabs, tab 1 active`, `pane=1 restored 2 tabs, tab 2 active`),
  only `C:\` and `C:\Users` listed; switching to the missing folder's tab refreshed it and showed
  the error (`no watch for C:\does-not-exist-p2m3: The system cannot find the file specified`).
  A file without tab lists (the bookmark check's, below) started on its `start_left` as before.
- **Overflow.** Defaults (scripted, no file), 11 `tab new`: the strip stayed at 0 up to 8 tabs (126
  px each; the log threshold was tightened after this run printed a rounding "-0" at 7), then
  `strip scrolled to -70 (tab 9 of 9 in view)`, -190, -310 and `-430 (tab 12 of 12 in view)`;
  `tab 1` scrolled it back to 0; `tab prev` (wrapping to 12) gave -430 again. 12 tabs of 120 px are
  1,440 px in a 1,010 px strip: -430 puts the last tab's right edge on the strip's.
- **Tab drag.** Four tabs, `tabdrag 0 0 400` (tab 1 pressed while tab 4 was active): `switched to 1
  of 4`, `moved tab 1 to 2`, `moved tab 2 to 3` (two midpoints crossed in one drag);
  `tabdrag 0 3 -300`: `moved tab 4 to 3`. Before the in-place fix the same steps moved nothing.
- **Middle-click.** `m` with `d1`, `d2` and `f1.txt`: `midclick 0 2` gave `f1.txt is a file,
  nothing to do`; `midclick 0 1` gave `middle-click row 1: new tab on ...\m\d2`, `opened tab 2 of
  2`, and its `[nav]`.
- **Breadcrumb dropdown.** In `m\d1`: `crumbmenu 0 8` (the `m` crumb): `listing the folders in
  ...\m`, `menu: 2 folder(s) ..., listed in 0.2 ms`; the screenshot (software renderer) shows the
  popup with d1 and d2 under the path bar and the chevron after every crumb; `crumbpick d2`:
  `picked ...\m\d2` and its `[nav]`. `crumbmenu 0 0` (This PC): `2 folder(s) in This PC` (C:\ and
  G:\), then `crumbclose`.
- **Maximised.** `TWINFM_CONFIG=cfg2.toml`: a run closed with `window close` saved `x = 260, y =
  260, width = 1200.0, height = 800.0, maximized = false`; the next run, `window max` then `window
  close`, saved the same four numbers with `maximized = true`. The run after that logged
  `[settings] window restored maximised`, and a PowerShell probe read `IsZoomed=True rect=-8,-8,
  3848,1580` while it ran; `window restore` and `window close` then saved 1200 x 800 at 260, 260,
  `maximized = false`.
- **Bookmarks.** A file holding the old `bookmarks = ['C:\Windows', 'C:\Users']`: `bookmark add`
  (on `C:\`), `bookmark rename 0 Win`, `bookmark move 2 up`, `bookmark move 0 up`: `Bookmarked C:\`,
  `renamed bookmark 0 (C:\Windows): Some("Win")`, `moved bookmark 2 to 1: C:\`, `bookmark 0 cannot
  move up`; the file then held three `[[bookmark]]` tables (C:\Windows titled "Win", C:\,
  C:\Users) and no `bookmarks` key. The screenshot shows Win, C:, Users under Bookmarks.
- **Right-click on empty space.** In `m` (empty space below three rows): `click 0 0`, `ctrlclick
  0 2`, `rclickempty 0`: `[select] pane=0 Clear row=-1 selected=0`, then the background menu.
  On `C:\Windows` (no empty space): the fallback logged `calling context-menu(-1) directly` after
  the same `Clear ... selected=0`.
- **Back button** (2026-10-12). On `m\d1` (empty, so the press lands on the empty space):
  `backclick 0` gave `[nav] pane=0 Back button (active pane was 0, now 0): up a folder` and `[nav]
  pane=0 gen=2 path=...\m`. After `split` and `nav 1 C:\Windows\System32` (the list's middle is a
  row there): `backclick 1` gave `pane=1 ... up a folder` and `path=C:\Windows`. With pane 1 active,
  `nav 0 This PC` then `backclick 0`: one line, `[nav] pane=0 Back button (active pane was 1, now
  0): This PC has no parent folder, nothing to do`, and no navigation.
- **In-app gate scenario, not decisive** (RDP, presentation at about 21 fps, frame interval 47 ms):
  six alternating runs on `C:\traces\bigdir`, `target-p2m2` 9.8, 10.3, 9.2, 9.7, 9.7, 7.6% and this
  branch 10.6, 10.3, 4.8, 5.8, 10.3, 8.6%, 379 frames each. Nothing new runs per frame: the strip,
  the breadcrumb, the dropdown and the sidebar change on tab, path or bookmark changes only.

All exit code 0, no panic.

Not checked by a script, needs a person at the console: a real mouse drag of a tab (the scripted
drag goes through Slint's pointer path, not the OS's), the mouse wheel over the tab strip,
clicking a chevron with the mouse (`crumbmenu` calls the callback with a fixed point under the
path bar), Escape inside the dropdown, the bookmark menu's four items and the inline title editor
(`bookmark rename` calls the callback the editor commits through), and a real middle button.

### Console check (2026-10-12) and what it found

Jeremy ran the gate binary with a real mouse and reported: nothing to drag, Escape dead inside the
dropdown, no way to make a bookmark, the middle button doing nothing, a right-click on the other
pane switching panes but also opening the context menu. Sorted out:

- **Nothing to drag, no bookmark**: the strip is hidden with one tab per pane (Ctrl+T opens more)
  and Ctrl+D makes a bookmark; the lead's instructions had not said so. Not bugs.
- **Escape inside the dropdown: a P2-M2 regression, fixed here.** Slint's runtime closes the
  topmost popup on an Escape that the focused element *rejects* (`WindowInner::process_key_input`,
  the `close_on_escape` branch after the focus item declined). P2-M2 made the list's FocusScope
  accept Escape for "cancel queued operations", so from then on no popup closed on Escape: the
  crumb dropdown, the bookmark menu and the About dialog alike. The popup's own FocusScope with
  `init => focus()` never receives the key either (focus stays in the list). The scripted
  `crumbclose` had closed the menu through a direct call, so no script exercised the key. Fix: the
  list issues `cancelqueued` and returns `reject`, so the cancel still happens and the runtime's
  close runs. Proof, scripted through Slint's real key routing (`key Escape`): `crumbmenu 0 1; key
  Escape; click 0 0` then `about open; key Escape; click 0 1` gave `[select] ... row=0` and `row=1`
  (the clicks reached the list), while the same script without the two Escapes gave no `[select]`
  line at all (the popups swallowed the clicks). 83 tests, no warnings.
- **Middle button: not a TwinFM bug.** A real `WM_MBUTTONDOWN`/`WM_MBUTTONUP` posted to the window
  at a folder row (scratchpad `post_input.py`, physical client pixels, DPI 96) gave `[tab] pane=0
  middle-click row 0: new tab on ...\d1`. The window handles the OS message; Jeremy's physical
  button does not produce it. Logi Options+ was running (`logioptionsplus_agent.exe`), and it
  commonly assigns the wheel button to a gesture or SmartShift rather than a middle click. To
  check in Options+: the mouse's wheel button should be "Middle click".
- **Right-click on the other pane opens its menu**: by design since M4 (Explorer does the same: a
  right-click on a row is a context menu for that row, and it also focuses the pane). The
  selection-clearing right-click is for empty space.
- **Maximise and restore**: worked.

Second pass, same day, on the rebuilt binary: Jeremy confirmed the tab drag, the wheel over the
strip, the bookmark menu and its editor, the middle button (after the Logi Options+ fix), the
side button going up a folder, and Escape closing the dropdown. Every P2-M3 console check passed.

### Gate

Console WPR, Jeremy elevated, `tools/wpr-gate.ps1`, accessibility on, Narrator off, same session,
back to back (2026-10-12 11:19 and 11:22): `target-p2m2` (0f57a5d) first, then `target-p2m3`
(f52368a). Analysed with `tools/analyze-trace.py --symbols --exe`.

| Build | Sampled ms | WPR busy | In-app |
|---|---|---|---|
| P2-M2 (`target-p2m2`), reference, 11:19 | 2593 | 25.9% (disturbed) | 27.2% |
| P2-M3 (`target-p2m3`), 11:22 | 2060 | **20.6%** | 18.6% |
| P2-M2 (`target-p2m2`), reference re-run, 11:30 | 2100 | **21.0%** | 20.8% |

The reference run is an outlier, not a reading of P2-M2: the same binary read 20.3% the evening
before, and every frame share in this run is up by about a quarter at once (render 1524 vs 1148
ms, draw_text 887 vs 653, ListView rows 544 vs 369, timers 899 vs 722), which is the machine being
slower or busier for those ten seconds, not a code path. P2-M3's 20.6% sits where every build has
sat across sessions: M6 19.8 / 19.9 / 20.2, P2-M1 20.0 / 20.4, P2-M2 20.3, with about 0.3 point of
drift between sessions. Its frame shares match P2-M2's clean run within a few samples
(accessibility 237 vs 228, render 1193 vs 1148, draw_text 668 vs 653, rows 377 vs 369, timers
729 vs 722, heap 204 vs 220); no tab-strip, crumb, bookmark or Back-button frame appears in the
window. The clean reference re-run eight minutes later read 21.0% (frame shares within a few
samples of P2-M3's: accessibility 221, render 1233, draw_text 695, rows 389, timers 726, heap 225),
so the same-session pair is P2-M2 21.0 / P2-M3 20.6. **P2-M3 passes**: below its reference. The
whole machine sits about half a point above the previous evening (P2-M2 read 20.3 then), one more
data point for "compare within a session, never against a recorded number". Traces
`C:/traces/twinfm-ab3-{p2m2,p2m3,p2m2b}.etl`, reports under `C:/traces/export/`.

## P2-M4: drag and drop finish

The backlog was the M5 Deferred list (`phase-1-plan.md` § M5): drag images, the highlight that did
not paint during TwinFM's own drag, the right-button drag menu, auto-scroll, and (from the M6 and
P2-M3 lists) the sidebar and the tab strip as drop targets. M5's Decisions held: a move dropped on
TwinFM is an optimized move, `Drop` posts its job through the event loop, a scripted drag is only
ever cancelled, and no app-state `RefCell` borrow is held across a modal loop.

### What landed (2026-10-12)

- **Drag images, our drags** (`dnd.rs`). One `IDragSourceHelper` from `CLSID_DragDropHelper`
  (`CoCreateInstance`, hand-written vtable), made on the UI thread when the drop target registers
  and kept for the life of the process (never released: a release from a thread-local destructor
  at exit could run after COM is gone). Before `DoDragDrop`, `attach_drag_image` composes a 96 x 96
  px (at scale 1) top-down 32-bit premultiplied BGRA image: the pressed row's icon (or the first
  dragged item's) at 64 px, 8 px in, and for several items a round accent badge with the count over
  the icon's bottom-right corner ("99+" past 99). The icon comes from `icons::render` called once
  on the UI thread (the same `IShellItemImageFactory` call the icon worker makes, at 64 px), falling
  back to the pane's cached row icon scaled up when the shell has none. The count is GDI text
  (`DrawTextW`, Segoe UI bold) drawn white on black into a scratch DIB, whose green channel is the
  coverage the badge is blended with (GDI text leaves alpha alone). `CreateDIBSection`, then
  `InitializeFromBitmap` with an `SHDRAGIMAGE` (`crColorKey` `CLR_NONE`, cursor offset 16, 16, so
  the cursor sits near the image's top-left); the helper owns the bitmap on success, it is deleted
  on failure. The helper stores the image on the shell's data object with `SetData`
  (`SHCreateDataObject` takes it; no storage of our own was needed). One `[dnd] drag image 96x96 px,
  N item(s), icon from <the shell|the pane's cache|nowhere>, composed in X ms: InitializeFromBitmap
  = 0x...` line per drag.
- **Drag images, drags over us.** One `IDropTargetHelper` (same CLSID), made and kept the same way.
  Our `DragEnter`, `DragOver`, `DragLeave` and `Drop` call it after their own work (after
  `drop_spot`'s borrow is released), with the data object, the screen point and the effect being
  returned, so the shell paints the source's image (Explorer's, or ours) over the window. `Drop`
  calls it before the drop menu, so the image is gone while the menu is up.
- **Right-button drags, source side.** A right press on a row records where it went down (the
  list's `right-down`, `right-x`, `right-y`; Slint's `TouchArea.pressed` is the left button's only);
  a move past the drag threshold with the right button down reports gesture 6, once per press.
  `drag_start(pane, row, right)` checks `VK_RBUTTON`, starts `Gesture::drag_right` (the right press
  already selected the row, as a right-click does), and `DoDragDrop` runs with the same allowed
  effects (copy and move). `continue_drag(escape, lbutton, rbutton, right)`: a right-button drag
  continues while the right button is held, drops on its release, and is cancelled by Escape or the
  left button. The end of a right-button drag hands Slint `PointerExited` instead of a release (a
  right release would open the row's context menu wherever the cursor is), which ends the row's
  grab and cancels the list's right press; `Gesture::drag_ended` goes straight to `Idle` for it.
- **The drop menu, target side.** `Drop` shows "Move here", "Copy here", a separator and "Cancel"
  (`TrackPopupMenuEx`, `TPM_RETURNCMD | TPM_RIGHTBUTTON`, at the drop point, owned by the main window,
  which is brought to the front first so a click elsewhere closes the menu) when the drag is a
  right-button one: `MK_RBUTTON` in `Drop`'s key state, or in the last `DragEnter`/`DragOver`'s (the
  button is already up when OLE calls `Drop`, so the target remembers it). The default item, in
  bold, is what a left-button drop would do (`effect_for`); an effect the source does not allow is
  greyed. The choice maps to an effect through `menu_effect`; "Cancel", Escape or a click elsewhere
  is `DROPEFFECT_NONE`. A move chosen from the menu is an optimized move, as before. The menu is a
  modal loop inside `Drop`: nothing is borrowed across it, and a temporary subclass on the main
  window ("twdm") logs `[dnd] drop menu loop idle: app state free=<bool>` at the menu's first
  `WM_ENTERIDLE`. `shellmenu::set_current_menu` makes the driver's in-menu steps and the second
  right-click guard see it as they see the shell menu. Works for drags from Explorer and for our own.
- **Auto-scroll.** `DragOver` (which OLE calls on a timer even while the mouse rests) scrolls a list
  by one row when the point is within 24 px of the list's top or bottom edge (`dnd::edge_scroll`,
  `dnd::scrolled`, clamped to the list), at most once per 50 ms (a timestamp in the target, no
  Slint timer), through `set-scroll`, then evaluates the spot again so the highlight lands on the
  scrolled list. `[dnd] auto-scroll pane N up|down: content-y A -> B`.
- **The highlight paints during our own drag** (the M5 Deferred item; the bounded spike, accepted,
  see Decisions). `drag_start` no longer runs `DoDragDrop` inside the pointer move that crossed the
  threshold: it takes the request out of the state, keeps it in a thread-local and posts
  `WM_RUN_DRAG` to the main window; the permanent subclass (`devices.rs`) calls
  `run_pending_drag` when winit's message loop dispatches it, outside winit's event handler. OLE's
  loop then hands WM_PAINT to winit, which delivers a redraw to Slint, and `drop_highlight` and the
  auto-scroll call `dnd::repaint_during_own_drag`, which queues a paint (`RedrawWindow` with
  `RDW_INTERNALPAINT`, what winit's `request_redraw` does) whenever the highlight or a scroll
  changes inside our own drag: Slint's own redraw request waits on its frame-throttle timer, and no
  Slint timer runs inside OLE's loop. Proof lines: `[dnd] highlight painted inside our own drag loop
  (frame F after the change at frame G)` (once per drag, checked from `QueryContinueDrag` against
  the frame counter `perf` already keeps) and `[dnd] N frame(s) rendered inside the drag loop` when
  `DoDragDrop` returns.
- **The sidebar and the tab strip as drop targets.** `Spot` gains `Side(row)` and `Tab(pane, index)`;
  `dnd::hit_all` takes `Strips` (the sidebar's width, its list's scroll, each row's height and
  whether it is a target; each pane's tab count, tab width and strip offset) and checks the sidebar
  and the tab strips before `hit`. Sidebar rows are 26 px (headers, never targets), 24 px, or 30
  px while being renamed, from 3 px under the sidebar's top to above its 26 px footer (the About
  row, no target); tabs are `tab-w` wide from the strip's offset, 26 px tall, 3 px in, only while
  the strip shows (two tabs or more). A drive, place or bookmark row drops into its folder, noted in
  the active pane; a tab drops into its tab's folder (a This PC tab is no target). The same
  refusals apply (`refused`: a source's own folder, a dragged item, inside a dragged folder).
  `DropSpot` carries a `Mark` (`Row`, `Tab` or `Side`) instead of a row. Highlights: `drop-tab-pane`
  and `drop-tab` on `App`, drawn by an overlay in the pane's strip (an `if`, so it exists only while
  a drag hovers a tab); `drop-side-row`, `drop-side-y` and `drop-side-h` drawn by an overlay over
  the sidebar list, moved with its scroll (`side-list.content-y`). `drop_highlight` sets them only
  when they change, as before. Nothing per row, nothing per frame.
- **Driver** (`src/auto.rs`): `dragrow <pane> <row> <dx> <dy> right` presses the right button;
  `dropfiles` takes the mode `menu <copy|move|cancel>` (the right button held in `DragEnter` and
  `Drop`, the menu item picked from the in-menu Win32 timer as `menupick` does, which now accepts
  the drop menu's three labels) and the spot forms `side <row>` (headers count; a negative row
  counts from the end, `side -1` is the newest bookmark) and `tab <pane> <index>` in place of
  `<pane> <row>`; `dragover <pane> <top|bottom> <ms> <paths>` enters the window 6 px inside a list's
  edge and calls `DragOver` every 20 ms from a Slint timer for `<ms>`, then `DragLeave` (the
  auto-scroll as another program's hovering drag drives it; it never drops). `dragrow` now waits for
  the drag it asked for to finish (it runs from the posted message after the step's dispatch).
- **Soak script** `tools/soak-p2m4.ps1` (see Soak).
- **Tests**, 83 to 88: `right_button_drag` (`continue_drag` for a right-button drag: held, released,
  Escape, the left button; the gesture's `drag_right` and its end; the drop menu's effect mapping
  and default; `effect_for` unchanged by `MK_RBUTTON`); `hit_sidebar_rows_and_tabs` (sidebar rows,
  headers, the footer, a scrolled sidebar, a collapsed one, tabs, past the last tab, a scrolled
  strip, the lists unchanged); `auto_scroll_near_the_list_edges` (the edge zones of both panes and
  under a tab strip, one row per step, clamped); `drag_image_composition` (geometry at two scales,
  the icon's pixels, the badge and its text); and the real-shell
  `drag_source_helper_takes_a_bitmap`: the shell's data object over a real file in
  `%TEMP%\twinfm-m5-test-img-<pid>\`, `CoCreateInstance(CLSID_DragDropHelper)`,
  `InitializeFromBitmap` with a 16 x 16 image returns `S_OK`, and the data object then offers
  "DragImageBits". `query_continue_drag_states` takes the new argument; `decide_effect`'s test is
  unchanged.

### Decisions

- **The drag image's icon is asked of the shell on the UI thread, once per drag**, at 64 px,
  rather than scaled up from the 16 px row icon in the cache (blocky at 4x) or requested
  synchronously from the icon worker (its queue serves newest first and replies through the shared
  channel). It is the same kind of shell call the drag start already makes (`SHCreateDataObject`,
  the PIDL parsing). Cost: 113 ms for the first drag of a run (the shell's first icon extraction on
  that thread), 5 to 23 ms after that. The cached row icon is the fallback.
- **A count badge for several items**, not the first item's name: the badge needs no text layout
  beyond a number, and it is what Explorer shows.
- **The right-button drag ends without a release for Slint.** M5 gave Slint the left release it
  missed; a right release would open the row's context menu at the cursor (in the first scripted
  run it reached the empty space's `TouchArea` and opened the folder's background menu).
  `PointerExited` ends the grab and nothing else.
- **The target remembers the right button from `DragEnter`/`DragOver`.** OLE calls `Drop` after the
  button is released, so `Drop`'s own key state never has `MK_RBUTTON` for a real right-button
  drag.
- **The drop menu's default is `effect_for`'s choice** (move on the same volume, copy across), and
  the bold item is the one a left-button drop would have done, as in Explorer. No "Create shortcuts
  here": TwinFM makes no links (M5).
- **The item 4 spike is accepted.** Running `DoDragDrop` from a posted message rather than inside
  winit's handler is what makes Slint render during our own drag: winit buffers events only while
  its handler runs, and answers WM_PAINT without rendering then (`should_buffer`, `WM_PAINT` in
  winit 0.30's Windows event loop). The change is about 40 lines of app code (the thread-local and
  the post in `drag_start`, `run_pending_drag`, the subclass's three lines, `repaint_during_own_drag`
  and its two callers) plus the proof logging and the driver's wait. It reverses M5's decision that
  "the drag starts inside the pointer move": the button is still down when the posted message
  arrives (posted messages come before input), and a drag that finds it released drops where it
  started, on our own pane, which `refused` turns into nothing. What runs inside our own drag loop
  now: winit's redraws (Slint renders), and `invoke_from_event_loop` closures (job completions,
  the enumeration drain, icon replies), which before waited until the drag ended. None of them can
  find a borrow held: `run_pending_drag` checks `state_is_free` before it starts, and the first
  `QueryContinueDrag` logs it as before. Slint timers still do not run inside the loop, hence the
  explicit paint request. Nothing is added per frame: the frame counter is read only from
  `QueryContinueDrag` and when the drag ends.
- **A scripted drag's "held" button is fixed when the drag is asked for** (`DragRequest::scripted`,
  copied into the drop source), not read from the driver's global flag inside the loop: the drag now
  starts after the step's dispatch returns, and a driver that cleared the flag early must never be
  able to turn a scripted drag into a drop.
- **A sidebar drop is noted in the active pane**; a tab drop in the tab's pane. Both panes refresh
  when the job ends, as for any drop.
- **Auto-scroll only in `DragOver`**, not in `DragEnter`, and only over the lists (not the sidebar or
  the tab strip).

### Deviations

- **Item 4 is more than "a few lines"**: about 40 lines of mechanism (above), plus the logging that
  proves it and the driver's wait for the deferred drag (`when_drag_over`). It touches no per-frame
  path and patches nothing in Slint. Reverting it is the one commit "dnd: run our own drag outside
  winit's handler so the highlight paints during it". **Lead's decision: accepted.** Dragging between
  the two panes is the core of a dual-pane manager, and a drag whose target never lights up and never
  scrolls is the kind of thing Phase 2 exists to fix. The price is real re-entrancy (the amendment
  at the top of this file); the 2-minute soak ran with it and the 30-minute one will, and every
  closure that can now run inside the loop was checked for modal loops (none).
- **No first-name label** on the drag image (above).
- **The helper test uses its own scratch folder**, `%TEMP%\twinfm-m5-test-img-<pid>\`, not the brief's
  `%TEMP%\twinfm-m5-test-<pid>\`: the existing `shell_data_object_offers_hdrop` test makes and
  removes that folder, and the tests run in parallel.
- **An extra driver step, `dragover`**, for the auto-scroll: `dropfiles` calls `DragEnter` and `Drop`
  only, and auto-scroll lives in `DragOver`.
- **`side -1`** (a negative sidebar row counts from the end), so the soak can aim at the bookmark it
  made without knowing how many drives and places the machine has.
- **The soak's tab is in the left pane**: a `key CtrlShiftRight` right after `split` did not make the
  right pane active in the script, so `tab new` opened the tab in the left pane; the setup now makes
  the tab before the split.

### Deferred

- **Spring-loaded folders** (hovering a folder row or a tab opens it).
- **Virtual files** (`CFSTR_FILEDESCRIPTOR` / `CFSTR_FILECONTENTS`: a zip folder's items, an Outlook
  attachment) and asynchronous data objects: still only `CF_HDROP`.
- **Mounted folders** still count as the drive that holds them when choosing move or copy.
- **The pane-to-pane OLE drop is still unscripted** (no scripted drop, M5): its `Drop` runs in every
  `dropfiles`, and DragEnter/DragOver/the highlight ran inside our own drag in the scripted checks,
  but the drop itself needs the console.
- **Drop descriptions** (the "Move to Folder" text under the drag image, `DROPDESCRIPTION` through
  `IDragSourceHelper2::SetFlags(DSH_ALLOWDROPDESCRIPTIONTEXT)` and the target setting it): not done.
- **Drops on a breadcrumb crumb** and **auto-scroll of the sidebar list or the tab strip**.
- **The first drag of a run waits about 0.1 s** for the drag image's icon (Decisions). Warming the
  shell's icon path on the UI thread at start, or taking the icon from the worker at 64 px ahead of
  time, would remove it.

### Scripted checks

RDP session, `target\release` of this branch (the final build, 3099104 plus docs), scratch folders in
`%TEMP%\twinfm-p2m4-manual\` (`a` with `sub`, `a.txt`, `b.txt`, `c.txt`; `b` and `c` empty), no
`TWINFM_SOAK`. For the in-drag checks a PowerShell helper put the real cursor over the window
(`SetCursorPos`) 5 s into the run; the RDP session moved it between the panes during the drags.
All runs exit code 0, no `panicked` line. stderr:

- **Drag image, our drag**: `click 0 1; ctrlclick 0 2; dragrow 0 1 0 60; sleep 1200; dragcancel`:
  `[dnd] pane=0 row=1: dragging 2 item(s)`, `[dnd] drag image 96x96 px, 2 item(s), icon from the
  shell, composed in 22.7 ms: InitializeFromBitmap = 0x00000000`, `DoDragDrop starts (left button)`,
  `first QueryContinueDrag inside the drag loop: app state free=true`, `DragEnter: 2 file(s)`, `over
  pane 1 row 32 (C:\Windows\System32\de-DE) ... -> effect move`, **`[dnd] highlight painted inside our
  own drag loop (frame 28 after the change at frame 27)`**, two `auto-scroll pane 1 down: content-y 0
  -> -22` / `-22 -> -44` lines while the cursor rested at the list's bottom edge, `QueryContinueDrag
  escape=true keys=0x0 (scripted): cancel`, `DoDragDrop returned 0x00040101 (cancelled)`, `1 frame(s)
  rendered inside the drag loop`, then the release after the drag. The first drag of an earlier run
  composed in 113.0 ms.
- **Right-button drag, cancelled**: `dragrow 0 3 0 60 right; sleep 400; dragcancel`: `[select] pane=0
  Context row=3`, `dragging 1 item(s) with the right button`, `InitializeFromBitmap = 0x00000000`,
  `DoDragDrop starts (right button)`, `highlight painted inside our own drag loop (frame 32 after the
  change at frame 31)`, `cancel`, `(cancelled)`, `2 frame(s) rendered inside the drag loop`, `[dnd]
  right-button drag over: pointer grab ended without a release (no context menu)`, and no `[menu]`
  line. A following `rclick 0 2` opened and closed the row's context menu normally. Under the
  threshold (`dragrow 0 3 1 1 right`): `no right-button drag started (under the threshold?); pointer
  leaves the window`, no menu.
- **Drop menu**: `dropfiles 1 -1 menu copy a\a.txt`: `over pane 1 (...\b) keys=0x2 ... -> effect
  move`, `drop menu starts at (1164, 288)`, `drop menu loop idle: app state free=true`, `drop menu:
  default Move here, chose Copy here -> effect copy`, `Drop ... effect copy (reported to the source:
  copy)`, `Copied 1 item`. `menu move a\b.txt`: `chose Move here -> effect move`, reported `none`
  (the optimized move), `Moved 1 item`. `menu cancel a\c.txt`: `chose Cancel -> effect none`, no
  job. The in-menu ticks logged `app state free=true driver free=true`.
- **Sidebar and tab**: `bookmark add` on `c`, the right pane with tabs on `b` and `c`: `dropfiles side
  12 copy a\a.txt`: `over sidebar row 12 (...\c), noted in pane 1 ... -> effect copy`, `Copied 1 item`
  into `c`. `dropfiles tab 1 0 copy a\b.txt`: `over pane 1 tab 0 (...\b) -> effect copy`, copied into
  `b`. `dropfiles tab 1 1 copy c\a.txt` (into its own folder): `effect none`, no job. `dropfiles tab
  1 0 menu cancel`: the menu, `chose Cancel`. `dropfiles side 3 ...` (the Places header): `over
  nowhere -> effect none`.
- **Auto-scroll, another program's drag** (`dragover`), on `C:\Windows\System32`: `dragover 0 bottom
  1000`: 16 `auto-scroll pane 0 down` lines in 1 s (DragOver every 20 ms, one scroll per 50 ms or
  more), the spot moving down a row each time (`over pane 0 row 32` ... `row 48`), then `DragLeave`;
  `dragover 0 top 400`: 7 `up` lines (`content-y -220 -> -198` the last), the spot moving up.
- **Earlier regression runs** of this branch: two-item and one-item left drags cancelled (both
  images `S_OK`), and the drop menu, side and tab drops above, all as expected.

Not checked by a script, needs a person at the console (below): any real drop, the drag image as
painted (our drag over Explorer and over our own window, Explorer's drag over TwinFM), a real
right-button drag from Explorer and its menu, real auto-scroll with the mouse resting at an edge.

### Soak

2 minutes, `tools/soak-p2m4.ps1 -Minutes 2`, RDP session, `target\release` of this branch: window
closed by the script, exit code 0, **0 `panicked` lines**; 4 cycles started, 63 jobs done, 0 failed,
0 cancelled, 0 soak refusals, 0 `waitops` timeouts; 18 drops, every one as expected (pane copy 3,
pane move 3, bookmark copy 3, tab copy 3, menu copy 3, menu cancel 3), 15 drop jobs; 6 drop menus,
all 6 loops `app state free=true`; 3 right-button drags, 3 cancelled, **0 dropped**, 3 drag images
set; 6 shell menus, 24 in-menu ticks, none busy. Working set 104.0 MB at 5 s, 181.9 at 35 s, 185.2
at 66 s, 175.3 at 121 s; private 104.9, 141.4, 146.9, 135.4 MB (the M5 shape: the context menus'
shell extensions load in the first cycle). The scripted right drags rendered no frame inside their
loops in the soak (the real cursor was elsewhere, so no highlight changed). The 30-minute run is
Jeremy's: `powershell -File tools\soak-p2m4.ps1 -Minutes 30`.

### Manual check (2026-10-12, Jeremy at the console, stderr logged)

Logged run of `target-p2m4` (`2> Documents\twinfm-drag-log.txt`; PowerShell writes UTF-16): a
two-item drag from the left pane dropped on the right pane with the image and the "2" badge
(`drag image 96x96 px, 2 item(s)`, `InitializeFromBitmap = 0x00000000`, `Drop ... 2 file(s),
effect move`); drops on sidebar rows 12 and 13 (`over sidebar row 13 ... effect move`, 5 frames
rendered inside the drag loop); a right-button drag from TwinFM to the other pane
(`DoDragDrop starts (right button)`, `highlight painted inside our own drag loop (frame 283 after
the change at frame 282)`, `drop menu: default Move here, chose Cancel -> effect none`);
Explorer's image over TwinFM on a left drag (`DragEnter ... keys=0x1 allowed=0x7`, DragLeave).
**Items 1, 2, 3 and 5 passed.** An earlier, unlogged attempt had reported all of them failing;
the logged run shows them working, so that attempt was on another binary or without the
conditions (the badge shows only with two or more items; the highlight needs the drag to cross a
row).

**Item 4, a right-button drag out of Explorer, does not start in Explorer**: the log holds no
`DragEnter` with `keys=0x2`; the only Explorer-sourced drag arrived with the left button. Nothing
reached TwinFM, so this is Explorer (or the mouse software) declining to begin a right-button drag,
not the target. When such a drag does arrive, the target remembers `MK_RBUTTON` from DragEnter and
DragOver and shows the menu on Drop, as the scripted `dropfiles menu` checks exercise. Recorded,
not a TwinFM defect.

### Gate

**Passed (2026-10-12, third session), after two pairs that said otherwise; the record below keeps
all three because the lesson is about the measurement.** Console WPR, Jeremy elevated, `tools/wpr-gate.ps1`, same session,
back to back (14:10 and 14:12): `target-p2m3` (2de0e4e) first, then `target-p2m4` (551625a).
A TwinFM window was already open before both runs (its process starts at "MIN" in both traces).

| Build | Sampled ms | WPR busy | In-app |
|---|---|---|---|
| P2-M3 (`target-p2m3`), reference | 1989 | **19.9%** | 19.5% |
| P2-M4 (`target-p2m4`) | 2145 | **21.4%** | 22.7% |

A 1.5-point gap in one session, five times the drift seen between runs of one binary. Symbolized,
every category is up by about seven percent at once (accessibility 223 to 241 ms, render 1156 to
1243, draw_text 709 to 769, ListView rows 374 to 400, timers 706 to 780, heap 196 to 251); the
function-level diff shows no frame that belongs to P2-M4 (no helper, drag, menu, overlay or
subclass frame in the window) and its apparent "new" entries are the generated Slint types
renumbered between builds. The code review found nothing per frame either: the subclass gains
one message comparison, the icon render runs once per drag, the frame counter is only read, the
sidebar and tab overlays are conditional and bound to drop state, not scroll. A uniform rise with
no new frame is what a slower run looks like, as the disturbed P2-M2 reference was (25.9 against
20.3 and 21.0), but one pair cannot tell that from a cost spread thinly, so a second pair with no
other TwinFM window open was requested before this milestone is judged. If the gap holds, the
suspects in order are: the pre-existing window (shared by both runs, so unlikely), the per-row
`pointer-event` handlers extended in P2-M3 and P2-M4 (each row's TouchArea now tests more
buttons; a trace of an older build with and without them would settle it), and the extra elements
in the sidebar and tab strip. Traces `C:/traces/twinfm-ab4-{p2m3,p2m4}.etl`.

**Second pair (2026-10-12 afternoon, no other TwinFM window open)**: P2-M3 **19.6%** (1958 ms;
in-app 20.8), P2-M4 **20.4%** (2037 ms; in-app 21.1). The gap is 0.8 this time against 1.5 before:
two pairs, both the same way, so about one point is P2-M4's and not noise. Category deltas in this
pair: ListView rows 347 to 369 ms, render 1135 to 1170, timers 679 to 704, heap 189 to 219,
accessibility 214 to 218. The mechanism is still unknown; the milestone does not merge until it is
found. Bisect in progress with three binaries from the branch: `target-p2m4a` (6ec0e9b: drag images,
right-button drags, auto-scroll; `dnd.rs` and `main.rs` only, no Slint overlays, drag still inside
the handler), `target-p2m4b` (93c0f2c: plus the drag outside winit's handler), `target-p2m4`
(551625a: plus the sidebar and tab overlays). Traces `C:/traces/twinfm-ab5-{p2m3,p2m4}.etl`.

**Bisect session (2026-10-12 evening, four runs back to back, no other TwinFM window):**

| Build | Sampled ms | WPR busy | In-app |
|---|---|---|---|
| P2-M3 (`target-p2m3`), reference | 2119 | 21.2% | 21.6% |
| p2m4a (6ec0e9b: images, right drags, auto-scroll) | 1993 | 19.9% | 22.3% |
| p2m4b (93c0f2c: plus the drag outside winit's handler) | 2284 | 22.8% | 21.7% |
| P2-M4 (`target-p2m4`, full branch) | 2033 | **20.3%** | 20.8% |

No step: the three branch builds read 19.9, 22.8 and 20.3, and the reference 21.2, so the full
branch sat 0.9 below its reference and the "middle" build 2.9 above the first. That is not a
commit's cost; it is the run-to-run spread of the machine today. Across the three sessions P2-M4
minus its same-session reference reads +1.5, +0.8 and -0.9: mean +0.5, spread 2.4. Category
shares move together in every run (render, text, rows, timers, heap all up or all down), and no
P2-M4 frame has appeared in any of the eight profiles. **Verdict: no cost attributable to P2-M4
has been shown; the milestone passes the gate as every earlier one did, inside its reference's
spread.** The milestone's code review stands: nothing runs per frame.

What the three sessions taught about the gate itself: single runs of one binary ranged 19.6 to
21.2 (P2-M3) and 19.9 to 22.8 (branch builds) in one afternoon, so the usable resolution of a
pair is about two points, not the half point assumed since P2-M1 (which was measured on one
quiet evening). A one-pair A/B can therefore both miss a real one-point cost and invent one.
Proposed for the next milestones, pending Jeremy's agreement: three runs per binary, interleaved
(A B A B A B), compared by median, and a gap counts only when every A sits below every B or the
medians differ by more than one point. Traces `C:/traces/twinfm-ab6-{p2m3,p2m4a,p2m4b,p2m4}.etl`.

Two analyzer findings from these traces, fixed in this PR: a TwinFM process already running when
the trace started reads "MIN" as its start, which the script could not parse, and the gate's
process must be chosen as the earliest that started inside the trace (and its start used for the
split search); the first attempt used the pre-existing window's start and found the wrong
enumeration thread, giving windows two seconds too early (23.1 and 24.4 percent, both wrong).
Close every TwinFM window before a gate run.

## P2-M5: icons

The backlog was the M1 and M2 Deferred lists (`phase-1-plan.md` § M1, § M2): reparse points not
told apart by tag, no overlays (shortcuts, junctions and symbolic links showed no arrow), and one
icon worker whose slow calls held every icon queued behind them. A large-icon view was declined
by Jeremy and is not part of this milestone. The M2 rework held: rows carry no image, icons reach
the screen only through each pane's strip (`icons::compose`), and every per-key cost is paid on a
worker, once per key per process.

### What landed (2026-10-12)

- **Enumeration through `FindFirstFileExW`** (`fs/enumerate.rs`). The `read_dir` loop is now
  `FindFirstFileExW(<folder>\*, FindExInfoBasic, FindExSearchNameMatch, FIND_FIRST_EX_LARGE_FETCH)`,
  `FindNextFileW` and `FindClose` on the worker, skipping `.` and `..`. `WIN32_FIND_DATAW` gives
  the attributes, the size (`nFileSizeHigh/Low`), the modified time (`ftLastWriteTime`, converted
  to unix seconds by `unix_secs`, truncated; a time before 1970 reads 0, as `metadata().modified()`
  through `duration_since(UNIX_EPOCH)` gave) and, for a reparse point, its tag (`dwReserved0`).
  `Entry` holds exactly what it held before; the existing enumeration and model tests pass
  unchanged. A folder that cannot be listed still gives one `Done` with the error text, formatted
  as before (`std::io::Error::from_raw_os_error(GetLastError())`: "... (os error 3)");
  `ERROR_FILE_NOT_FOUND` from the first call (a drive root with nothing in it) is an empty listing,
  as `read_dir` made it. A pattern of 248 characters or more gets the `\\?\` (or `\\?\UNC\`)
  prefix that `read_dir` added, so long folders still list.
- **Reparse kinds** (`model.rs`). `Kind` gains `Junction` (4, `IO_REPARSE_TAG_MOUNT_POINT`),
  `Symlink` (5, `IO_REPARSE_TAG_SYMLINK`), `Cloud` (6, `tag & 0xFFFF0FFF == IO_REPARSE_TAG_CLOUD`,
  the cloud tag and its fifteen numbered variants) and `AppAlias` (7, `IO_REPARSE_TAG_APPEXECLINK`);
  `Reparse` (3) is now "any other tag". `Kind::from_attrs(attrs, tag)` reads the tag only for a
  reparse point; `Kind::from_tag` is the pure mapping. `Entry::is_dir` is unchanged (a junction or
  directory link opens by navigating). The comment over pane.slint's `Row` lists the new values.
- **What the user sees.** The Type column reads "Junction" for a junction, "Symbolic link" for a
  file or directory link, "Application" for an app execution alias, and the type's own text for
  a cloud placeholder. The row's accessible label no longer keys on `kind == 1`: a row with both
  a size and a time (a file of any kind) reads "name, type, size, modified", other rows "name,
  type" plus the size when there is one (drives), as before. Checked with `tools/uia-dump.ps1` on
  `C:\Users`: "All Users, Symbolic link", "Default User, Junction", "desktop.ini, Configuration
  settings, 174 B, 2024-04-01 03:24". The status bar is unchanged.
- **Icon keys.** `icon_key` and `IconKeys::for_entry` take the kind. A cloud placeholder is keyed
  as if it were no reparse point (`key_attrs`): a file by its extension (or full path for the
  own-icon extensions), a folder `dir` unless `custom_folder` finds a `desktop.ini` (read-only or
  system folders only, as before; OneDrive's own folders are read-only and carry one). Junctions,
  symbolic links, app execution aliases and other reparse points stay keyed by full path.
- **`[enum]` reparse split.** The `done:` line ends with `, reparse: J junction, S symlink, C
  cloud, A app alias, O other` when the listing has any (counted on the worker, `ReparseCounts`
  in `Msg::Done`); nothing per entry is logged.
- **Overlays** (`icons.rs`). `Request` gains `overlay: bool`, set in `resolve_view` by
  `icons::wants_overlay(kind, attrs, name)`: junctions, symbolic links, app execution aliases,
  other reparse points, and files named `.lnk` or `.url` (never a folder: a folder named `x.lnk`
  shares `dir`). Every such entry has a full-path key. For those the worker asks the shell for the
  overlay index (`SHGetFileInfoW(path, 0, SHGFI_ICON | SHGFI_SMALLICON | SHGFI_ADDOVERLAYS |
  SHGFI_OVERLAYINDEX)`, `iIcon >> 24`, the icon handle destroyed at once; no
  `SHGFI_USEFILEATTRIBUTES`, the shell looks at the file), and the overlay image from the system
  image list: `SHGetImageList(SHIL_SMALL, IID_IImageList)` (`SHIL_LARGE` when the small list's
  icons are smaller than the strip's), `GetOverlayImage`, `GetIcon(ILD_TRANSPARENT)`, drawn with
  `DrawIconEx(DI_NORMAL)` at the requested size into a transparent 32-bit top-down DIB section
  (premultiplied BGRA, since `DrawIconEx` alpha-blends an alpha icon). The overlay pixels are cached
  per (overlay index, size) in the pool, shared by both threads, and logged once:
  `[icons] overlay index 2 fetched at 16 px`. The base icon is fitted to the requested size and the
  overlay drawn over it by `icons::overlay` (source-over on premultiplied pixels, `dst = src + dst
  * (1 - src.a)`; the shell's overlay images are full icon size with the glyph in the bottom-left
  corner). The reply carries the composed pixels; `IconTable`, `compose`, `fit` and the strip are
  unchanged. `IImageList` is a hand-written vtable struct with every slot in declaration order
  (only `Release`, `GetIcon`, `GetIconSize` and `GetOverlayImage` typed); `ILD_TRANSPARENT` is
  declared locally, so `Cargo.toml` is unchanged.
- **Two icon worker threads** (`ICON_WORKERS = 2`) sharing one queue (`Mutex<(Vec<Request>,
  bool)>` plus a `Condvar`), served newest first as before. A feeder thread ("icons-feed") moves
  requests from the UI's crossbeam channel into the queue and wakes one worker; the channel's
  close sets the flag and wakes both, which then exit. The UI side is unchanged (it still sends on
  a `Sender<Request>`). Each worker initialises COM (MTA) for itself. A key is taken when a worker
  pops it (`taken: Mutex<HashSet>`), so it is rendered at most once per process across the pool;
  a request for a taken key, or with a stale generation, gets `Skipped` as before; a failed render
  gives the key back (the UI's `failed` set is what stops a retry). `Stats` stays atomics and gains
  `overlays` and `slow`. A render (overlay included) of 100 ms or more logs `[icons] slow icon <ms>
  ms: <path>`.
- **`[icons]` line** gains `overlays N, slow N` for the period, after the worker's average.
- **Tests**, 88 to 95: `reparse_tags_map_to_kinds` (the four tags, all sixteen cloud tags, six
  others including WOF, dedup and AF_UNIX, the discriminants); `kind_from_attributes` extended (the
  tag read only for a reparse point); `tells_reparse_points_apart` (a junction made with `cmd /c
  mklink /J` in `%TEMP%\twinfm-enum-reparse-test-<pid>\` lists as `Junction`, "Junction", opens as
  a directory, keyed by full path; a plain file's `modified` equals `metadata().modified()` to the
  second; the `Done` counts; a file symbolic link is asserted when one can be made, else skipped
  with a note, which it was here: no privilege, no Developer Mode); `filetime_and_long_paths`
  (`unix_secs` at and around the epoch, truncation, before 1970, and the `\\?\` and `\\?\UNC\`
  prefixes); `key_rules` extended (every kind, cloud files and folders, a custom cloud folder);
  `overlay_blends_source_over_premultiplied` (2 x 2: transparent, opaque, half over half,
  over a transparent base; sizes that differ); `overlay_only_for_links_and_reparse_points`;
  `pool_serves_others_while_one_icon_is_slow` (a stand-in render sleeps 2 s for one request; three
  requests sent 100 ms later all reply within 500 ms; a stale request is skipped meanwhile; a failed
  key renders again when asked again; one slow render counted) and `pool_renders_each_key_once`
  (one key asked for 20 times at once: one `Pixels`, 19 `Skipped`, one render; a later request is
  skipped; the workers exit when the channel closes), both through the test-only
  `spawn_with(..., render)` seam that `spawn` calls with `render`; and the real-shell
  `worker_renders_exe_drive_and_extension` now also makes a junction in
  `%TEMP%\twinfm-p2m5-test-<pid>\` and asks for it twice, without and with the overlay flag: both
  render, the overlaid one is 16 x 16 and its pixels differ from the plain one fitted to 16 (overlay
  index 2, the link arrow).

### Decisions

- **The pool is fed by a feeder thread, not by the workers draining the channel.** With workers
  draining the channel themselves, a worker that pulled a burst of requests into the shared queue
  and then started a slow render left the rest of the burst sitting there while the other worker
  slept in `recv` (it only wakes for a new message). The feeder moves each request into the queue
  and wakes an idle worker, so whatever is queued is served while one render is stuck. Cost: one
  more thread that sleeps in `recv`.
- **A key is taken when popped, not when delivered.** That is what makes "at most once per process"
  hold with two threads; a failure gives the key back so that `failed` in the UI's table stays the
  only retry policy, as in M2.
- **The overlay is drawn at the requested size by `DrawIconEx`**, not read with `GetIconInfo` and
  fitted: `GetIconInfo`'s colour bitmap of an alpha icon is not premultiplied, `DrawIconEx` into a
  transparent DIB gives premultiplied pixels directly. An overlay that draws nothing visible
  (every alpha byte zero) is treated as no overlay, so `bgra_to_rgba` can never turn it into an
  opaque square.
- **The base icon is fitted to the requested size before the blend**, so the two buffers match
  even when `SIIGBF_BIGGERSIZEOK` returned a bigger image; the table's own `fit` is then a refcount
  bump for these keys.
- **App execution aliases read "Application"** whatever their extension; all fifty on this machine
  are `.exe`, whose shell type is "Application" anyway. The shell gives them no overlay (`overlays
  0` over the 28 rendered in `WindowsApps`), so they show their own icon only.
- **The accessible label keys on the row's text, not its kind**, so a cloud placeholder or an
  alias keeps its size and time in the label without the label naming every file-like kind.
  It is the same per-row binding as before with one more string comparison, evaluated only when
  the accessibility tree asks for the label.

### Deviations

- **Cloud placeholders inside a sync root are not reparse points to TwinFM, so the brief's
  "120 cloud" split and the 568 ms per placeholder cannot occur in a listing of a OneDrive folder.**
  The cloud files filter shows a process placeholders as plain files unless the process opts into
  `PHCM_EXPOSE_PLACEHOLDERS` (`RtlSetProcessPlaceholderCompatibilityMode`); TwinFM does not.
  Measured with a P/Invoke probe of the same `FindFirstFileExW` call: `C:\Users\jtkli\OneDrive`,
  15 entries, 12 reparse points in expose mode (PowerShell's default, 2), 0 in disguise mode (1);
  TwinFM's `[enum]` for that folder and for `OneDrive\Documents` shows no reparse split at all, and
  the old build already keyed those files by extension. What TwinFM does see as cloud is the sync
  root itself, from its parent: `C:\Users\jtkli` lists `OneDrive` as 1 cloud (read-only, with
  `desktop.ini`, so still keyed by full path for its own icon, which is right). M2's 568 ms entry in
  `C:\Users\jtkli` was most likely that folder or another full-path key on a cold shell; it did not
  recur in this session's runs (the slowest there was `netbird-0.80.0-ipv6fix.exe`, 1057 ms, a
  37 MB exe). The cloud mapping and the shared keys still apply to any cloud reparse point TwinFM
  is shown; exposing placeholders is under Deferred.
- **The `IImageList` slot numbers in the brief were off**: counting IUnknown's three, `GetIcon` is
  slot 10 (not 13) and `GetOverlayImage` slot 31 (not 29). Verified against the vtable generated
  from the SDK metadata in the `windows` crate 0.62.2 (`IImageList_Vtbl`, already in the cargo
  registry here; there is no Windows SDK header on this machine). The overlay test's link arrow
  (index 2) is the evidence that the slots are right.
- **No new `windows-sys` features**: `IImageList` is hand-written, `ILD_TRANSPARENT` declared
  locally, and everything else was already enabled.
- **The `[enum]` split has five fields** (junction, symlink, cloud, app alias, other), not four:
  the aliases are their own kind, and `WindowsApps` lists 50 of them.
- **`wants_overlay` takes the attributes too** (kind, attributes, name), so a folder named `x.lnk`
  is never overlaid on the shared `dir` key.
- **The real-shell overlay check uses a junction**, not a `.lnk` (the brief allowed either); the
  `.lnk` and `.url` arrows are checked in the scripted run and screenshot below.
- **The symbolic-link assertions were skipped in this session's test runs** (creating one needs the
  privilege or Developer Mode, neither here); the tag mapping is unit-tested and a real directory
  link (`C:\Users\All Users`) was checked by hand below.
- **A time before 1970 still reads 0**, not a negative number: that is what the old code stored,
  and the brief asks for the same contents.
- **The two icon items are one commit** ("icons: two worker threads ..."): the overlay is fetched
  inside the pool's worker loop and the seam test covers both.

### Deferred

- **Cloud status overlays** (OneDrive's green check or cloud glyph): placeholder keys are shared by
  type, so a per-file status cannot ride on them. It would need a per-file key for placeholders
  (the 568 ms-per-file cost again) or a second, per-row status mark, which is per-row work.
- **Exposing placeholders** (`PHCM_EXPOSE_PLACEHOLDERS`) so that TwinFM sees files inside a sync
  root as cloud reparse points. It changes what every file API in the process reports for them
  (attributes, and possibly file operations), so it needs its own check before it is turned on.
- **The overlay index costs one `SHGetFileInfoW` per overlaid key**, on the worker (40 to 200 ms for
  the first ones of a run, a few ms after; `slow icon 199 ms: ...\OneDrive\Personal Vault.lnk`). A
  folder of thousands of shortcuts would take that many shell calls once each, on the workers.
- **"Other" reparse points are overlaid but none was seen**: no folder listed in the checks had
  one (System32, bigdir, the profile folders), so whether the shell gives them an arrow is
  unchecked.

### Scripted checks

RDP session, `target\release` of this branch (3e1374e then fbf6d6c), against the main checkout's
`target\release\twinfm.exe` built at a35f583 (copied to the session scratchpad and run from there,
so it was not rebuilt or locked). stderr timestamped by a launcher script; all runs exit code 0,
no `panicked` line.

- **Enumeration, `C:\traces\bigdir`** (40,000 files; `sleep 2000;split;sleep 1500;quit`, so pane 0
  lists at start and pane 1 on the split), `[enum]` worker ms, five runs each, old build run before
  and after the new one:

  | Build | Pane 0 (at start) | Pane 1 (split) |
  |---|---|---|
  | old (a35f583), first set | 66.9, 62.7, 69.6, 67.6, 62.9: mean 65.9 | 45.2, 45.4, 50.0, 47.6, 52.5: mean 48.1 |
  | new (FindFirstFileExW) | 55.0, 56.4, 51.5, 52.7, 53.0: mean 53.7 | 37.6, 36.3, 47.4, 37.6, 36.3: mean 39.0 |
  | old, second set | 64.9, 65.7, 66.7, 67.2, 63.4: mean 65.6 | 51.2, 45.4, 51.6, 45.1, 49.4: mean 48.5 |

  The new loop is about 12 ms (18%) faster at start and 9 ms (19%) faster on the split. On
  `C:\Windows\System32` (4,912 entries, where the type lookups and the seven `desktop.ini` probes
  dominate) the five throughput runs below listed in 98.6, 117.2, 112.7, 101.8, 108.5 ms (old) and
  155.2, 90.2, 109.6, 93.1, 93.3 ms (new): the same, with one slow first run.
- **Icon throughput, `C:\Windows\System32`** (`sleep 2500;drag 0 900 1500;sleep 8000;quit`: the
  scrollbar thumb dragged through the whole folder in 1.5 s, 574 requests and 574 replies per run
  for both builds), replies per once-a-second `[icons]` report after the drag, five runs each:

  | Build | 1st report | 2nd report | 3rd report | All 574 in by | avg ms/icon per render |
  |---|---|---|---|---|---|
  | old (one worker) | 179, 159, 162, 178, 176 | 208, 219, 204, 225, 215 | 182, 191, 203, 166, 178 | the 3rd report | 3.8 to 5.6 |
  | new (two workers) | 321, 311, 328, 320, 329 | 248, 258, 241, 249, 240 | none | the 2nd report | 5.1 to 5.8 |

  About 320 icons a second against about 190: each render is slightly slower with two threads in
  the shell (5.5 against 4.8 ms average), but the queue drains in about two thirds of the time,
  one report earlier. `overlays 0, slow 0` in every new run (System32 has no links). The first five
  renders of a run (the shell's warm-up) averaged 25 to 28 ms on the pool against 15 to 19 ms on
  one thread.
- **In-app busy, the gate scenario** (`tools/wpr-gate.ps1`'s script on bigdir, relative only in
  RDP), interleaved old/new: old 16.7, 18.9, 17.3, 16.9 (median 17.1); new 16.2, 16.9, 17.2, 17.7
  (median 17.05). No difference; the strip and the UI thread's icon path are unchanged.
- **Junction, `.lnk` and `.url`**, `%TEMP%\twinfm-p2m5-manual\` (a junction `junction` to `target`,
  `notepad.lnk`, `plain-txt.lnk` made with `WScript.Shell`'s `CreateShortcut`, `example.url`,
  `plain.txt`), software renderer: `[enum] ... reparse: 1 junction, 0 symlink, 0 cloud, 0 app
  alias, 0 other`, `[icons] overlay index 2 fetched at 16 px`, `slow icon 118 ms:
  ...\plain-txt.lnk` (the first overlay of the run), `+6 requests +6 replies ... overlays 4, slow
  1`. The screenshot shows the arrow on the junction, both shortcuts and the `.url`, none on
  `target` or `plain.txt`; Type reads "Junction", "Internet Shortcut", "Shortcut".
- **`C:\Users\jtkli`** (hidden shown with Ctrl+H): `reparse: 10 junction, 0 symlink, 1 cloud, 0 app
  alias, 0 other` (old build: no split; worker 82.9 against 90.4 ms). The junctions (`Application
  Data`, `Cookies`, `NetHood`, `PrintHood`, `Recent`, `SendTo`, `Start Menu`, `Templates`, ...) read
  "Junction" with the arrow; `OneDrive` keeps its own icon and "File folder". One `slow icon 1057
  ms: C:\Users\jtkli\netbird-0.80.0-ipv6fix.exe` while the other rows' icons kept arriving on the
  second thread (`+37 requests +16 replies`, then `+8 +28`, then the slow one alone).
- **`C:\Users`** (Ctrl+H): `reparse: 1 junction, 1 symlink`; `All Users` reads "Symbolic link" and
  `Default User` "Junction", both with the arrow (`overlays 2`); `uia-dump.ps1` labels as above.
- **`C:\Users\jtkli\AppData\Local\Microsoft\WindowsApps`**: `reparse: 0 junction, 0 symlink, 0
  cloud, 50 app alias, 0 other`, worker 1.2 ms; the aliases read "Application" with their own
  icons, `overlays 0`, two `slow icon 161 ms` (`ms-teams_autostarter.exe`, `mspaint.exe`).
- **`C:\Users\jtkli\OneDrive`**: no reparse split (see Deviations); `Personal Vault.lnk` got its
  arrow (`overlays 1`, `slow icon 199 ms`), `Pictures` (custom folder) `slow icon 135 ms`.

Screenshots and logs (session scratchpad, not in the repo): `shots/manual.png` and
`shots/manual-zoom.png` (the arrows, 6x), `shots/home-new.png`, `shots/home-new-mid.png`,
`shots/users.png`, `shots/windowsapps.png`, `shots/onedrive-{old,new}.png`; `logs/enum-*.log`,
`logs/s32-*.log`, `logs/gate-*.log`.

### Gate

**Passed (2026-10-13), the first milestone judged under the interleaved-triplet protocol.** Console
WPR, Jeremy elevated, `tools/wpr-gate.ps1`, accessibility on, Narrator off, no other TwinFM window,
six runs back to back from 09:37 to 09:53, reference and candidate alternating: `target-p2m4`
(551625a, the P2-M4 merge build) and `target-p2m5` (011430e, the PR head, built from a detached
checkout). Analysed with `tools/analyze-trace.py --symbols --exe`.

| Run | P2-M4 (`target-p2m4`) | P2-M5 (`target-p2m5`) |
|---|---|---|
| 1 | 20.9% (2086 ms; in-app 21.1) | 20.4% (2040 ms; in-app 23.1) |
| 2 | 21.6% (2157 ms; in-app 21.6) | 20.9% (2088 ms; in-app 20.9) |
| 3 | 21.0% (2099 ms; in-app 20.6) | 20.1% (2009 ms; in-app 20.5) |
| **median** | **21.0%** | **20.4%** |

The candidate's median is 0.6 below the reference's and no P2-M5 run sits above any P2-M4 run
(its highest, 20.9, equals the reference's lowest). Under the rule (a gap counts only when every
reference run sits below every candidate run or the medians differ by more than one point) there
is no cost attributable to P2-M5. Frame shares match within a few samples in every pair
(accessibility 225 / 209, 226 / 216, 234 / 225 ms; render 1197 / 1186, 1260 / 1234, 1200 / 1142;
draw_text 693 / 686, 689 / 677, 696 / 665; ListView rows 383 / 382, 394 / 379, 377 / 386; timers
723 / 705, 731 / 712, 730 / 718; heap 216 / 232, 235 / 212, 220 / 206), and no icon-pool, overlay
or enumeration frame appears in any window: on bigdir one key (`ext:.txt`) renders once (56 to 63
ms on the first shell call), `overlays 0, slow 0`, and the strips compose at 24 to 26 us as before.
The whole machine sat about half a point above the previous day's bisect session (P2-M4 read 20.3
there), one more data point for comparing within a session only. No `panicked` line in any of the
six logs. Traces `C:/traces/twinfm-ab7-{p2m4,p2m5}-{1,2,3}.etl`, reports under
`C:/traces/export/`.

Jeremy reported the console checks (junction and shortcut arrows, the Type column on `C:\Users`,
a slow icon not holding the rest) done on 2026-10-13 on `target-p2m5`.


## Open questions

- **Phase 2 scope.** Confirmed by the lead as the recommended order above, directory watching
  first; Jeremy can still reorder or cut milestones.
- **The Type column** stays on by default for now. It cost 3.4 gate points in M1; if a Phase 2
  milestone needs per-frame headroom, the options are making it a setting (off by default) or
  buying the headroom elsewhere first.
- **A large-icon view**: declined by Jeremy (2026-10-12). P2-M5 is icons only; a large-icon view
  is a Phase 3 candidate beside thumbnails, which it would share the per-row work with, and it
  starts with a trace if it is ever taken up.
- **Gate protocol, agreed by Jeremy (2026-10-12)**, from P2-M5 on: three console runs per binary,
  interleaved (reference, candidate, reference, candidate, reference, candidate), compared by
  median; a gap counts only when every reference run sits below every candidate run or the
  medians differ by more than one point. Reason: single runs of one unchanged binary ranged 19.6
  to 22.8 on 2026-10-12, so a one-pair A/B resolves about two points and both invented and hid a
  one-point difference during P2-M4.
- **Explorer never starts a right-button drag on this desktop** (P2-M4 manual check): outside
  TwinFM; worth one check without Logi Options+ running if Jeremy cares.
- **Upstreaming the two Slint accessibility patches** (`docs/upstream-issue-draft.md`,
  `docs/patches/`): still deferred; Jeremy decides when.

## Phase 2 close

P2-M1 to P2-M5 are in: directory watching, operations inside TwinFM (real names, cursor, cut
marks, Undo, the queue), the daily-use polish (tabs restored and reorderable, the crumb dropdown,
the maximised state, bookmark editing, the Back button), the drag and drop finish (drag images, the
right-button menu, auto-scroll, the sidebar and tabs as targets, the highlight during our own
drags), and icons (reparse kinds, overlays, two workers, the find loop). Phase 2's goal held:
the list stays true to the disk without F5, operations are visible and undoable inside the app,
and the gate gave nothing back. Every milestone passed inside its same-session reference's spread:
P2-M1 20.0 against M6 19.9 / 20.2; P2-M2 20.3 against P2-M1 20.4; P2-M3 20.6 against P2-M2 21.0;
P2-M4 +1.5, +0.8 and -0.9 over three sessions (mean +0.5, no frame of its own in eight profiles);
P2-M5 median 20.4 against 21.0 under the triplet protocol. Two 30-minute soaks (P2-M1, P2-M4)
ran clean. The protocol itself was the lesson of the phase: a single console run resolves about
two points on a busy day, so from P2-M5 on the gate is three interleaved runs per binary compared
by median.

Deferred to Phase 3, pulled together from every milestone and deduplicated:

- **Directory watching**: parsing the change records for targeted row updates instead of a
  re-listing; watching background tabs; subtree watches (for folder sizes); network shares with a
  timeout; removable drives through `RegisterDeviceNotification` with `DBT_DEVTYP_HANDLE`;
  noticing the watched folder itself being renamed; the double refresh after our own operations;
  a back-off for folders that change continuously (P2-M1).
- **File operations**: Redo; undo of a delete to the Recycle Bin and of a move that renamed on
  collision; restoring a file a copy or move replaced; dimming the cut row's icon; progress inside
  TwinFM (`UpdateProgress`); a Narrator check of the notes (P2-M2).
- **Shell and settings**: folder sizes in the status bar (needs the size worker and subtree
  watches); a settings dialog; Explorer's protected-operating-system-files switch; the compact
  Windows 11 menu (`IExplorerCommand`) and icons on TwinFM's own menu items; a designed icon and
  the toolbar; re-revealing the active tab on resize; keyboard navigation and drops in the crumb
  dropdown; F2 for a bookmark; restored tabs keeping their sort, filter, cursor and scroll; a
  back/forward history per tab (the mouse's Forward button, and Back as history back); the Back
  button over text entries, the tab strip and the scrollbar (P2-M3).
- **Drag and drop**: spring-loaded folders; virtual files (`CFSTR_FILEDESCRIPTOR`) and
  asynchronous data objects; mounted folders counted as their own volume; a scripted pane-to-pane
  OLE drop; drop descriptions; drops on a crumb; auto-scroll of the sidebar and the tab strip;
  warming the shell's icon path so the first drag image does not wait 0.1 s (P2-M4).
- **Icons**: cloud status overlays (OneDrive's check and cloud marks, which need a per-file key or
  a per-row status mark); exposing placeholders (`PHCM_EXPOSE_PLACEHOLDERS`) so files inside a sync
  root are seen as cloud reparse points; the `SHGetFileInfoW` call per overlaid key (a folder of
  thousands of shortcuts); whether the shell overlays "other" reparse points; not rendering icons
  for rows scrolled past; 32 px icons and the large-icon view Jeremy declined for Phase 2; scale
  factor changes flushing the table (M2, P2-M5).
- **Checks that need Jeremy at the console**: a Narrator pass over the Phase 2 notes and the new
  Type texts; the OneDrive folder with placeholders exposed, if that is ever turned on; a
  right-button drag out of Explorer on a machine where Explorer starts one.

Phase 3 candidates, in no agreed order: search (a name filter in the pane first, then content),
archives (zip folders as navigable, which also needs the virtual-file drop path), thumbnails (and
with them the large-icon view, the first per-row rendering since M1, trace first), history and the
Forward button, the settings dialog, folder sizes, the compact context menu, and upstreaming the
two Slint accessibility patches (`docs/upstream-issue-draft.md`, `docs/patches/`), which Jeremy
decides on. Phase 3's kickoff brief should set the gate protocol as the triplet rule from the
start and name the reference binary (`target-p2m5`, 011430e).
