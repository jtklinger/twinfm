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
- **`WM_DEVICECHANGE` is unverified with real hardware**: no USB drive was inserted or removed in
  the session; the code path (subclass, debounce, drive list re-read, This PC refresh) is untested.

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
  subclass, the debounce and the refresh; it does not prove that the shell's broadcast reaches
  the window. Re-test with a real drive on the rebuilt binary is pending.
- **30-minute soak**: pending (not yet run; the 2-minute run is under Scripted checks).

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
