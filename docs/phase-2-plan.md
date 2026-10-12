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

Still needing the real mouse, with Ctrl+T first: a tab drag, the wheel over the strip, the
bookmark menu (Ctrl+D, then right-click the bookmark) and its inline editor.

### Gate

Pending Jeremy: console WPR, `tools/wpr-gate.ps1` elevated, accessibility on, Narrator off, hidden
off, an A/B in one session against `target-p2m2`, both built into their own `target-<name>` from
detached worktrees. Nothing in P2-M3 runs per frame or per row: the tab strip, breadcrumb,
dropdown and bookmark rows are rebuilt on change only, and the gate scenario opens no second tab.

## Open questions

- **Phase 2 scope.** Confirmed by the lead as the recommended order above, directory watching
  first; Jeremy can still reorder or cut milestones.
- **The Type column** stays on by default for now. It cost 3.4 gate points in M1; if a Phase 2
  milestone needs per-frame headroom, the options are making it a setting (off by default) or
  buying the headroom elsewhere first.
- **A large-icon view**: wanted at all? It would be the first feature since M1 to add per-row
  rendering work, so P2-M5 starts it with a trace only if Jeremy says yes.
- **Upstreaming the two Slint accessibility patches** (`docs/upstream-issue-draft.md`,
  `docs/patches/`): still deferred; Jeremy decides when.
