# Phase 5: TwinFM finishes the Phase 2 lists

Written 2026-10-15, when Phase 4 closed with P4-M1 to P4-M3 merged and the P4-M3 build at medians
of 17.9% (defaults) and 10.2% (grid on) on the gate under the triplet protocol (see
`phase-4-plan.md` § Phase 4 close). Phase 1 made TwinFM usable for a day instead of Files, Phase 2
made it trustworthy every day, Phase 3 made it find and show what Explorer can, Phase 4 showed a
folder as large icons with thumbnails. Phase 5's goal, chosen by Jeremy on 2026-10-15 from the
Phase 4 close's candidates: **TwinFM finishes the Phase 2 lists still open**, the directory
watching, file operation and drag-and-drop items deferred from P2-M1, P2-M2 and P2-M4 and carried
through two phases (`phase-2-plan.md` § P2-M1 / P2-M2 / P2-M4 Deferred, `phase-3-plan.md` § Phase
3 close), without giving back the gate. Not in Phase 5, by the same decision: the P2-M3 polish
leftovers (the compact Windows 11 menu, menu icons, the toolbar, the tab strip on resize, the
crumb dropdown's keyboard, F2 for a bookmark, restored tabs' state, the Back button over a text
entry, the protected-files switch) and the P2-M5 icon items (cloud marks, the shell call per
overlaid key, "other" reparse overlays). Accessibility (the grid read by a screen reader, the
Narrator passes) is deferred for much later.

The starting point is main at 3929c06 (P4-M3). What the three lists rest on today: one watcher
thread with one completion port (`src/fs/watch.rs`), a handle per active tab (at most two), 250 ms
trailing-edge coalescing, no record parsing on the plain watch (every change is a whole-folder
refresh; the P3-M3 subtree watch does parse records, into `Event::Subtree`), no watch on network
drives, shares, removable drives or discs; file operations through `IFileOperation` on an STA
worker with a hand-written progress sink that records every item's real path (`src/fileops/`),
an undo stack of 20 records played back as inverse jobs (`src/fileops/undo.rs`), a delete to the
Recycle Bin recorded as "cannot undo", the status bar showing the queue count, the shell's own
progress window; drag and drop through our own `IDropSource` / `IDropTarget` (`src/dnd.rs`) with
the shell's drag-image helpers, `effect_for` choosing move or copy by drive, auto-scroll of the
lists by row and grid row, drops onto rows, tiles, empty space, sidebar rows and tabs, and a
right-button drop menu.

Three milestones, in the order Jeremy agreed on 2026-10-15 (one per list; do not fold them):

1. **P5-M1, directory watching finished.** The plain watch parses its records
   (`FILE_NOTIFY_INFORMATION`) and the UI applies them to the pane's model as targeted row
   updates (one row added, removed, renamed or modified) instead of re-listing the folder, with
   the whole-folder refresh kept as the fallback for anything the records cannot settle
   (overflow, `ERROR_NOTIFY_ENUM_DIR`, a record for a name the listing does not know, a change
   while a listing is in flight). Background tabs are watched (a handle per tab under a cap; past
   the cap a tab goes stale as today), so a tab switch no longer re-enumerates a watched tab. The
   watched folder's own rename, move or delete is noticed (the records of its parent name it; a
   second handle per pane on the parent, or the next refresh's error, whichever the trace
   favours), the tab's path follows a rename and a delete shows the error at once. The refresh
   after our own operations runs once: a change the job's own results explain is not refreshed
   again. Folders that change continuously get a back-off (the coalescing window grows while
   changes keep coming and resets after a quiet spell; measured on a log folder). Network shares
   are watched, with the blocking calls off the watcher thread and a stall treated as "no watch"
   (F5 still works). Removable drives are watched through `RegisterDeviceNotification` with
   `DBT_DEVTYP_HANDLE`: the handle closes on `DBT_DEVICEQUERYREMOVE` so "Safely remove" succeeds,
   and reopens on `DBT_DEVICEQUERYREMOVEFAILED`. Gate: the defaults triplet and the grid triplet
   (`C:\traces\grid.toml`), since the model's row edits reach both views; the generated rows are
   diffed against the reference's before the gate, as every milestone since P3-M5 has.
2. **P5-M2, file operations finished.** Redo (Ctrl+Y, a second stack fed by undo, cleared by the
   next new job, as Explorer's). Undo of a delete to the Recycle Bin, through the bin's own item
   (the sink already records each recycled item's `$R` path; the bin's "undelete" verb restores it
   with its metadata, a move back from `$Recycle.Bin` is the fallback if the verb cannot be
   reached). Undo of a move that renamed on collision restores the old name (`MoveItem` with
   `pszNewName`). The cut row's icon dims with its text (in the strip's compose, per entry at
   compose time, the icon table untouched; the grid's tiles the same way). Progress inside
   TwinFM: the sink's `UpdateProgress` feeds a percentage to the status bar (at most four times a
   second, composed in `update_status`, never per frame) beside the queue count; the shell's
   window stays. Gate: the defaults triplet and the grid triplet, since the compose path changes
   (the scenario has no cut rows, so what the triplets measure is the branch).
3. **P5-M3, drag and drop finished.** Spring-loaded folders (a drag held over a folder row, a
   tile or a tab for about a second opens it in that pane, ours and foreign drags alike, inside
   `DragOver`'s timer and the modal-loop rule: the navigation runs through the closures the drag
   loop already allows). Drop descriptions ("Move to <folder>", "Copy to <folder>" under the drag
   image: `DSH_ALLOWDROPDESCRIPTIONTEXT` on our source, `DROPDESCRIPTION` set by our target on
   every `DragOver` and cleared on `DragLeave`, so Explorer as a source shows them too). Drops on
   a breadcrumb crumb (`Spot::Crumb`, the crumb's folder, a highlight on the button). Auto-scroll
   of the sidebar list and the tab strip while a drag hovers their edges. Mounted folders counted
   as their own volume in `effect_for` (`GetVolumePathNameW` on both sides, not the drive letter).
   The first drag's wait for its icon removed (warm the shell's icon path at start, or take the 64
   px icon ahead of the threshold; measured, then chosen). The pane-to-pane OLE drop stays
   unscripted (a scripted drop once landed on another program's window, P2-M4). Gate: the
   defaults triplet; nothing in it is per row.

Each milestone brings its driver steps and stderr lines (named in its brief and recorded in
`CLAUDE.md` § Measuring), tests for its maths and its model edits, and a soak: P5-M1 extends
`tools/soak-p2m1.ps1` with the record counts and a writer on a background tab, P5-M2 adds a
recycle-and-undo and a redo to the operation cycle, P5-M3 adds a spring-load and a crumb drop
to the drag cycle. Console checks after each triplet: a real USB drive watched and ejected
(P5-M1), a dead share (P5-M1, if one can be arranged), the Recycle Bin restore and Explorer's
view of it (P5-M2), the drop descriptions with Explorer as the source and as the target (P5-M3).

**The gate is re-run after every milestone**, under the protocol agreed on 2026-10-12 and
unchanged since: `tools/wpr-triplet.ps1` elevated at the console by Jeremy, accessibility on,
Narrator off, every other TwinFM window closed, the split-and-scroll scenario on
`C:\traces\bigdir`, on clean binaries built with `--locked` into their own `--target-dir
target-<name>` from detached `git worktree add` checkouts with default features. Three runs per
binary, interleaved (reference, candidate, reference, candidate, reference, candidate), compared by
median; a gap counts only when every reference run sits below every candidate run or the medians
differ by more than one point. The reference for the phase's first milestone is `target-p4m3`
(53dbc50, P4-M3's gate build); after each merge the new milestone's gate build becomes the next
reference and the previous one is deleted (`target-p4m2` goes once `target-p4m3` has served).
Tags continue from ab20. Bar: main thread under 20% busy over the 10 s after the split, judged
against the reference's spread in the same session, never against a recorded number. Console
checks run after the triplet, never before. A milestone is not closed until its table is recorded
here. When a milestone fails: stop, symbolize, name the frames, and either fix the mechanism or
report the options; never trade the bar off silently.

**Settings triplets stay part of the protocol** (Phase 4's rule): a milestone that touches the
list, the grid or the icons runs the defaults triplet and the settings triplets its entry above
names (`C:\traces\grid.toml`, read-only, each candidate's stderr checked for `[settings] file
<toml> (read)` and `[view] pane=0 starts as large icons` before its triplet is read). The
defaults triplet decides whether the details list has moved; a settings triplet whether the grid
holds the bar. The gate scenario changes no file, so what the triplets measure in this phase is
the wiring: the record path's presence in `drain`, the compose branch, the drop target's spot
tests; a milestone whose scenario cost is zero by construction still runs its triplet, because
P3-M1 found 1.5 points in a per-row `if` nobody expected to cost anything.

The architecture rules from Phase 1 hold (see `phase-1-plan.md` and the kickoff brief they came
from): only the UI thread touches a Slint model and workers wake it with
`slint::invoke_from_event_loop`; every COM object lives on the thread that made it; no app-state
`RefCell` borrow is held across a modal loop; no `unwrap` on `Weak<App>`; `windows-sys` with
hand-written calls; nothing per frame or per row without a trace first.

Amendment (P2-M4), carried over: the modal-loop rule is load-bearing, not precautionary. Our own
`DoDragDrop` runs from a posted message outside winit's handler, so `invoke_from_event_loop`
closures (enumeration results, job completions, icon replies, watch and clipboard events) run
INSIDE our own drag loop. Every such closure takes `with_state`; the drag code holds no borrow
while the loop runs (it takes everything out first and checks `state_is_free`), and no closure may
itself open a modal loop. Context menus and the shell's dialogs still run inside a handler or on
the worker.

Amendment (P2-M5), carried over: rows carry no image; icons reach the screen only through each
pane's strip (`icons::compose`, and the grid's `compose_grid`), and every per-key cost is paid on
a worker, once per key per process. Anything that would put a per-row or per-tile image or shell
call back is a trace first, not a feature.

Amendment (P3-M1 and P3-M5), carried over: nothing in the row or the tile may depend on a runtime
property. A per-row `if` cost 1.5 points and two per-row bindings 0.7 in P3-M1; the row's
structure is chosen once per pane, outside the `for`, and proven by diffing the generated code
against the reference's row, as P3-M5 did for its four list components. The grid's tiles carry
their own fields (`selected`, `cut`, `current`) for the same reason: no tile binds to a root
property.

Amendment (P5, from P2-M1 and P3-M3): a record is applied to the model only when the listing is
settled (no pass in flight, no filter or sort change pending) and the name is accounted for;
anything else is the whole-folder refresh, logged with its reason. The model's edits go through
the notifications the views already listen to (`append`, a removal, a one-row replace, with the
`grid_notify` beside them); no view learns a new property for this.

Each milestone runs as in Phase 4: implementer brief, lead review, gates, push, PR, a gate build
from a detached checkout with `--locked`, `tools/wpr-triplet.ps1 -Reference p4m3 -Candidate
<name>[,<name>@C:\traces\grid.toml] -Tag <next>`, console checks after the triplet, merge on
Jeremy's word, `target\release` rebuilt from main and said so.

## P5-M1: directory watching finished

The first milestone of the phase (2026-10-15): the plain watch's records reach the listing as row
edits, background tabs are watched, the watched folder's own rename is followed, our own
operations are not refreshed twice, folders that never stop changing back off, and network shares
and removable drives are watched. Branch `claude/p5-m1-directory-watching-44524f`, from bdd6c34.
The brief is the P5-M1 entry of the kickoff above; the work was split three ways (the watcher
thread, the model's edits, the wiring in `main.rs`), each half reviewed before the wiring relied on
it, then the branch as a whole.

### What landed

- **The watcher parses its records** (`src/fs/watch.rs`). A plain watch's `FILE_NOTIFY_INFORMATION`
  records are parsed with the P3-M3 parser, the rename pairs (`RENAMED_OLD_NAME`, `RENAMED_NEW_NAME`)
  joined, and the window's records folded per name into their net effect (`net_changes`, pure and
  unit-tested, keyed by the ASCII-lowercased name): added then removed is nothing, modified then
  removed is removed, removed then added is modified, renamed `a` to `b` then `b` to `c` is renamed
  `a` to `c`, a rename cycle (`a` to `t`, `c` to `a`, `t` to `c`) is a modified on each name. The
  output is ordered departures first (removals, then renames, each held back until the rename that
  vacates its target is out), then additions, then modifications, so a safe-save (`doc.tmp` written,
  `doc` renamed to `doc.bak`, `doc.tmp` renamed to `doc`) applies as `Renamed doc -> doc.bak, Added
  doc` and loses nothing. `Event::Changed` now carries `changes: Vec<Change>` (`Added`, `Removed`,
  `Modified`, `Renamed { from, to }`), `records` (the raw count) and `overflow` (an empty completion,
  `ERROR_NOTIFY_ENUM_DIR`, or more than `MAX_RECORDS` = 1,000 raw records in one window, logged as
  `N records in the window: overflow`); the subtree event is as it was.
- **Watches are keyed by tab** (`Watcher::watch(pane, tab_id, path, subtree, parent)`,
  `unwatch(tab_id)`, `unwatch_pane(pane)`; re-watching a folder another way delivers the window
  gathered so far before the old handle is retired, so a change just before a tab switch is not
  lost), with a hard limit of `MAX_WATCHES` = 8 main handles for
  the app; a refusal of any kind is now an event too (`Event::Refused { pane, tab_id, path, why }`),
  besides the once-per-tab-and-path line.
- **The parent watch** (`parent: true`, asked for the active tabs): a second handle on the folder's
  parent with `FILE_NOTIFY_CHANGE_DIR_NAME` only and a 4 KiB buffer, the same Box-with-OVERLAPPED
  identity and the one `completed` path (`Role::Main` / `Role::Parent`). A rename of the watched
  folder's own name gives `Event::Moved { from, to }` at once (not coalesced) and the watch
  re-points its stored path (the kernel handle already follows the folder); an unpaired old name
  (`the folder was moved away`) or its removal (`the folder was deleted`) gives `Lost` and closes
  both handles. No parent watch for a drive root. In practice Windows reports the delete on the main
  handle first (`Access is denied`), and a move to another folder as a removal.
- **Back-off** (`Backoff`, pure): a watch's window starts at 250 ms; a change within `QUIET` (1 s)
  of the last delivery doubles the next window, up to `COALESCE_MAX` (2 s); a change later than that
  starts again at 250 ms. `[watch] ... back-off: window W ms` on every change of the window, and the
  delivery line `records R, changes C, window W ms` carries it. Subtree watches back off the same way.
- **Network shares are watched.** `unwatchable` no longer refuses UNC paths or `DRIVE_REMOTE`. A
  remote open (main and parent) runs on a helper thread ("dirwatch-open") that posts its result to
  the completion port under its own key; the watcher's loop never waits on it. An open still
  pending after `OPEN_TIMEOUT` (3 s) is refused with `stalled: no watch` and its root (the drive or
  share, lowercased) is on a 30 s cool-down (`STALL_COOLDOWN`) during which requests are refused at
  once; a late handle is closed on arrival. Retiring a remote watch (`CancelIoEx`, `CloseHandle`)
  runs on a "dirwatch-close" helper that posts a packet when done; the Box is freed only after
  both the cancelled read's completion and that packet, and `stop` leaves the port open while any
  helper runs. Local paths keep the direct path.
- **Removable drives are watched and still eject.** `unwatchable` no longer refuses
  `DRIVE_REMOVABLE` (a disc stays refused: `a disc cannot change`). After the handle opens the
  watcher registers it (`RegisterDeviceNotificationW` with `DEV_BROADCAST_HANDLE`,
  `DEVICE_NOTIFY_WINDOW_HANDLE` on the main window, whose HWND reaches the thread through
  `Watcher::set_hwnd`, called from `main_hwnd` after the subclass is installed). The subclass in
  `devices.rs` sees `DBT_DEVTYP_HANDLE` events and calls `fs::watch::device_event`, which talks to
  the thread through a process-wide static (no app state: the subclass may run inside a modal loop):
  `DBT_DEVICEQUERYREMOVE` suspends the watch (main and parent handles closed, the Box kept with its
  path, the reply awaited at most 1 s; the subclass returns `TRUE` either way, so "Safely remove"
  proceeds), `DBT_DEVICEQUERYREMOVEFAILED` resumes it (reopened, re-armed, re-registered, and one
  overflow window sent so the listing refreshes), `DBT_DEVICEREMOVECOMPLETE` loses it (`the drive
  was removed`). The registration stays through the suspension (the later events arrive on it) and
  is replaced on resume. A resume while the cancelled read's completion is still queued waits for
  it, so the `OVERLAPPED` is never re-armed in use. The volume arrival and removal handling is as
  it was.
- **Row edits in the model** (`PaneModel::apply_edits(Vec<Edit>, cursor) -> Result<Applied, String>`,
  `src/model.rs`; `Edit::{Upsert(Entry), Remove(name), Rename { from, entry }}`). All-or-nothing:
  every name is resolved first against an overlay of the listing's name map (an `Upsert` exactly; a
  `Remove` or `Rename.from` exactly, then ASCII-case-insensitively, the map for that built only on
  such a miss), and an unknown one is `Err` with nothing changed; a refresh in flight, no pass landed
  since the listing was cleared, or a pass in flight is `Err` too. `all` is edited in place
  (`swap_remove`: every pass re-sorts); `shown` keeps its order through a binary search with
  `cmp_entries` (the comparator factored out of `sort_entries`, byte for byte the same order): an
  insert goes after its equals, a replaced entry stays in place while it still sorts between its
  neighbours and moves otherwise (a removal and an insert), an entry that turns hidden or filtered
  leaves `shown` and is counted, and back. The replaced entry keeps its row's `selected` and `cut`,
  its resolved icon when the icon key is unchanged, and a folder's computed size. Selection indices
  and the anchor shift with inserts and removals (`sel_version` bumps when the set or the anchor
  ends up different); a removed selected row leaves the selection; names that left leave `cut`. The
  cursor's row is followed and returned (the row after it when it is removed; -1 for an empty
  list). Notifications: `row_added(i, 1)`, `row_removed(i, 1)` or `row_changed(i)` per step on the
  details list, never a reset; the grid gets `grid_changed` per in-place change or one `reset` when
  any row was added or removed. `settled()` (no refresh in flight, the newest pass landed:
  `landed_pass`, set by `land_pass`, reset by `clear`), `has_name`.
- **One entry's stat** (`fs::enumerate::stat_entry(dir, name, ..)`): `FindFirstFileExW` on the exact
  verbatim path (`\\?\`, so a name ending in a dot or a space is found as itself; `[` is no wildcard
  there), the same `entry_of` as the listing, the on-disk casing returned; a name with `: \ / * ? <
  > "` is None.
- **The wiring** (`main.rs`). `watch_event` on a window decides, in this order: `overflow` is a
  fallback; an empty `changes` is `R records netted to nothing` and nothing else; a window on an
  active tab whose every name is in the tab's `explained` set (below) within 3 s is `R records
  explained by the operation: no refresh`; then the fallbacks `not a folder listing`, `listing in
  flight` (no `done_ms`, or an error), `pass in flight` (`!settled()`), `network path` (a stat over
  SMB never runs on our threads), `C changes over the cap` (`EDIT_CAP` = 500); else the window goes to the edit worker. An
  unknown name (a removed, modified or renamed-from name the listing lacks) is `apply_edits`'s
  error, not a scan per name on the UI thread: `land_edits` reads the name out of the error and,
  when it is lenient (it vanished before it was listed, or the listing is younger than
  `COALESCE_MAX + COALESCE`, and it holds no `~`: see Deviations), drops that removal or turns
  that rename into an upsert and applies again, at most eight times; otherwise `fallback: unknown
  name <name>`. Every fallback logs `fallback: <reason> (R records): refresh |
  refresh after it | stale`, the three outcomes P2-M1 had. The edit worker ("watch-edits", one
  thread fed by a channel, so windows land in order) normalises the folder once per window
  (`stat_dir`: `GetFullPathNameW` through `std::path::absolute`, since a typed `C:/x` or `C:\a\..\b`
  is not normalised under the verbatim prefix) and stats the added, modified and renamed names
  (`stat_entry`; a name whose stat fails is gone: an add becomes nothing, a modify a removal, a
  rename a removal of the old name), times it, (when any stat fails it checks that the folder itself still exists: `fallback: folder gone`
  otherwise, since the handle follows a renamed folder while the stats use the stored path), and
  posts `land_edits`, which re-checks the tab, its path, its generation, the pass and `settled()`
  (else `edits dropped: <why>` and a fallback), drops the
  sizes of removed or renamed folders, applies the edits with the cursor (`current_row` for the
  active tab, the tab's saved cursor otherwise), puts the cursor back (`set_grid_cursor` too),
  and for the active tab runs `size_selection`, `update_status`, `strip_dirty` and `resolve_view`
  (the new rows' icons come through the strip as a scroll's do). `[watch] ... applied: +A -R ~C
  (stat S ms, apply T ms)`. `land_pass` sets `landed_pass`. The subtree watch's `top` keeps its
  whole-folder refresh (`fallback: subtree watch (N records)`).
- **Background tabs watched.** `watch_tabs` (the pure `watch_set` decides) asks for every active
  folder tab with a parent watch (the subtree flag as before; a search tab or a failed listing
  gets none; pane 1 nothing without the split), then background folder tabs whose listing is done,
  error-free and not stale, most recently shown first (`Tab.shown_at`), shallow and without a parent
  watch, until `MAX_WATCHES`; every other tab is unwatched (always sent, a no-op for a tab without a slot, so a `Lost` drained
  after a re-watch cannot leak a slot) and, in the background, stale (`past
  the cap: stale when shown` when it was eligible). A tab leaving the foreground (`switch_tab`,
  `open_tab`, `start_search`) is no longer marked stale; it still forgets its sizes (a shallow
  watch cannot invalidate them). A watched tab coming to the front checks its folder is still
  there with one `metadata` call (skipped on remote paths; `folder gone on show: refresh`) and
  refreshes only when stale. `Refused` for the tab's path clears `watched` and makes a background
  tab stale; `Lost` clears it, refreshes the active tab once (the error shows) and makes a
  background tab stale. After a file operation or a shell verb only the unwatched background tabs
  go stale (`mark_background_stale(st, false)`); the thumbnails switch still marks them all.
  `close_tab` and the split going off release their watches. `[watch] ... watched in the
  background`.
- **The folder's own rename** (`Event::Moved`): the tab's path follows (`[nav] pane=N path follows
  the rename: <to>`), the path bar, the tab strip and the saved tab list with it, the watch is
  asked for again under the new path, and a listing still in flight (of the old path, which will
  fail) is run again; otherwise no refresh; the
  history's current place is the tab's path, so nothing is pushed.
- **No second refresh after our own operations.** `job_done` stores on each refreshed active tab
  the lower-cased names its results name in that folder (`explained_names`: every source and
  destination whose parent is the tab's folder, plus a rename's or new folder's asked-for name)
  with the time; a window arriving within `EXPLAINED_FOR` (3 s) whose every name is in the set is
  dropped with the line above. `navigate_to`, `Moved` and the expiry clear it.
- **Driver** (`auto.rs`): `mvfile <pane> <old> <new>` renames a file in the pane's folder (the soak
  fence as `touch`); `waitwatch <ms>` now returns on a watch-driven refresh or an applied window;
  `waitedit <ms>` waits for an applied window only (so a script can assert the edit path was taken).
- **Soak** (`tools/soak-p2m1.ps1`): a third scratch folder `w3` on a second left tab that the
  writer churns while it sits in the background, a `mvfile` and a `waitedit` per cycle, a switch
  to `w3` and back per cycle (counted against `[refresh]` lines), and the record counts in the
  report (records delivered, windows applied with their `+A -R ~C` sums, edits dropped, fallbacks by
  reason, netted, explained, watched in the background, past the cap, back-off lines and the
  largest window).
- **Tests**, 134 to 159: `fs::watch` (18, 5.5 s): the nine of P2-M1 and P3-M3 adapted to the
  changes they now carry, `net_changes_fold_a_window` (the folding table, the safe-save, the swap,
  a shift, a case-only rename), `backoff_grows_under_churn_and_resets_after_quiet` (synthetic
  instants), `renaming_the_watched_folder_is_noticed` (`Moved`, then a change inside the renamed
  folder), `moving_the_watched_folder_away_loses_it`, `watches_are_keyed_by_tab_and_capped` (nine
  tabs, the ninth refused, a tenth after an unwatch), `continuous_changes_back_off` (windows 250,
  500, 1000, 2000 ms; four events in 3 s of churn), `remote_paths_are_told_apart`,
  `rewatching_the_same_folder_delivers_the_gathered_window`; `model`:
  `apply_edits_inserts_at_the_sorted_position`, `..._replaces_in_place_and_moves_when_the_key_changes`,
  `..._keeps_selection_cut_and_cursor`, `..._counts_hidden_and_filtered`,
  `..._refuses_unknown_names_without_changing_anything`, `..._renames`,
  `..._notifies_rows_not_resets` (real notifications recorded through a model change listener),
  `..._upserts_exactly_and_keeps_resolved_icons`, `landed_pass_and_settled`,
  `cmp_entries_matches_sort_entries`; `fs::enumerate::stat_entry_reads_one_entry`
  (`%TEMP%\twinfm-p5m1-stat-<pid>\`); `main`: `watch_set_takes_active_tabs_then_the_most_recently_shown_up_to_the_cap`,
  `explained_by_needs_every_name_of_the_window`, `explained_names_are_the_jobs_names_in_that_folder`,
  `edits_from_folds_failed_stats`, `stat_dir_normalises_a_typed_path`.
- **Docs**: `CLAUDE.md` (test count, the steps and lines, the soak, the scratch folder).

### Decisions

- **The cap is 8 watched tabs for the app**, active tabs first, then background tabs most recently
  shown first. A handle costs a 64 KiB buffer and a port association; eight is 512 KiB and more
  tabs than a session keeps open, and the ninth goes stale as every background tab did before. The
  watcher enforces it as a hard limit; the UI chooses the set, so the choice is testable without a
  thread.
- **The parent watch, for the active tabs only.** The alternative was the error on the next refresh,
  which is what happened before and leaves a renamed folder's tab showing a path that no longer
  exists while its changes keep arriving. The parent handle is idle unless a folder name changes
  beside the watched one (the gate's parent is `C:\traces`), a 4 KiB buffer, no records parsed
  into anything but the folder's own name; two of them in the app. A background tab gets none: the
  `metadata` check when it comes to the front catches a deleted folder, and a renamed one shows
  the error on that refresh, as before.
- **Net effect per window, departures first.** The stage-one review found that applying records in
  order of first appearance loses the file of a safe-save and one side of a rename swap; ordering
  the departures first and folding cycles into modifications makes every window order-free.
- **The stats run on one worker, never on the UI or watcher threads.** A stat is microseconds on
  NTFS and unbounded on a USB drive that spins up or a share; one long-lived thread keeps windows
  in arrival order (two threads could land a removal before its add).
- **A network path never takes the edit path.** The watch arrives, the whole-folder refresh runs
  on the enumeration worker as before; a stall is the watcher's (an open past 3 s is "no watch",
  with a 30 s cool-down so a dead share costs one attempt per half minute).
- **Back-off doubles to 2 s and resets after a quiet second**, the brief's "grows while changes
  keep coming and resets after a quiet spell" with numbers: four refreshes a second on a log folder
  become one every two seconds while it writes, and the first change after a pause is still shown
  within 250 ms.
- **The window's edits are capped at 500**, and a window is an overflow past 1,000 raw records: a
  re-list on the worker is cheaper than hundreds of single edits (Measurements), and 1,000 records
  is more than any burst short of an unzip.
- **An `Upsert` matches exactly; a `Remove` or `Rename.from` falls back to a case-insensitive
  match.** A record carries the on-disk casing, so an add or modify of a name that differs only in
  case is a new entry; the case-insensitive map is built only when a removal misses, which took
  the fixed cost of a window with an insert on a 40k listing from 33 ms to 6 (the review's
  finding, Measurements).
- **A refresh after a job runs once, and the watch's window for it is dropped by name**, not by
  time alone: a window whose names the job did not touch (another program writing meanwhile) goes
  the normal way.
- **A tab leaving the foreground still forgets its sizes** (P3-M3): its background watch is
  shallow, so a change inside a sized subfolder would go unseen. The sizes come back when the tab
  is selected again, as before; keeping subtree watches on background tabs is Deferred.
- **A network share re-lists on every window.** Before this milestone shares were not watched at
  all; now a busy share lists itself again up to once per window (2 s at the back-off's top) on
  the enumeration worker. The alternative, stats over SMB on our threads, is what the brief ruled
  out; F5 is no slower than before.
- **Two blocking calls stay on the UI thread, bounded**: the one `metadata` of a watched tab's
  folder when it comes to the front (skipped on remote paths; a USB disk spinning up is the worst
  case), and `device_event`'s wait of at most 1 s for the watcher's suspend reply inside
  `WM_DEVICECHANGE` (Windows is waiting on that message anyway).
- **Removable drives: only the main handle registers** for device notifications; a suspension
  closes the parent too (it would block the eject as well). USB hard disks report `DRIVE_FIXED`
  and get no registration (Deferred).

### Deviations

- **Names that vanished before they were listed, and a young listing, are not "unknown".** The
  brief's rule (a record for a name the listing lacks is a fallback) gave seven false fallbacks per
  soak run, every one right after a navigation or Ctrl+R: a back-off window of up to 2 s carries
  records from before the new listing began, and an `Added` whose stat finds the file gone is
  dropped, so its later `Removed` names a stranger. Now the worker reports the names that vanished
  (`Tab.vanished`, keyed by the listing's generation), a listing requested less than 2.25 s ago
  (`COALESCE_MAX + COALESCE`) is young, and a removal of such a name is dropped while a rename from
  one becomes an upsert of the fresh stat. The end state is still ground truth, since every edit is
  a fresh stat. The soak went from seven fallbacks to none. The final review moved the check out
  of `watch_event` (a scan of the 40k names per change on the UI thread) into the retry on
  `apply_edits`'s error, and excluded names with `~` (an 8.3 short name in a record would
  otherwise let a real removal pass).
- **The watcher's remote open is asynchronous**, not a blocking open with a timeout on the watcher
  thread (the brief's wording): the review found a dead share would hold every other watch's
  delivery, and the device reply, for 3 s per attempt.
- **`start_search` drops the sizes of the tab it leaves**, as `switch_tab` and `open_tab` did
  (it did not before; a background watch cannot keep them honest).
- **The "no watch" line carries the tab id** now (`pane=N tab=T no watch for ...`), since watches
  are per tab.
- **Subtree records are not turned into edits**: a change in the folder itself on a tab with sizes
  still re-lists the folder (`fallback: subtree watch`). The subtree event names children, not the
  folder's own entries; carrying both is P5-M1's one piece left for later.

### Deferred

- **Subtree records as edits** (above), and **subtree watches on background tabs** (so their
  sizes survive a tab switch).
- **USB hard disks** (`DRIVE_FIXED`) get no device registration, so a tab on one still blocks
  "Safely remove" while it is shown; registering every non-system volume's handle is the fix.
- **The device and network paths were exercised by no test and no script**: the console checks
  after the triplet (a USB stick inserted, watched, ejected and re-inserted; a dead share if one
  can be arranged) are the first real run of `device_event`'s suspend and resume and of the stalled
  open.
- **`the folder was deleted` rarely appears**: the main handle reports `Access is denied` first.
  Both reach the same refresh and the same error in the status bar.
- **A folder moved to another parent is `Lost`, not `Moved`**: Windows reports the move as a
  removal of the name.
- **The 33 ms per window that a case-insensitive removal still costs** on a 40k listing (the map
  built on that miss); a persistent lower-cased index would remove it.
- **An `Opened` packet posted after the watcher thread exited leaks its handle** at process end.
- **A rename onto a name that exists under another casing** replaces that row only when the
  window already built the case-insensitive map (a removal missed exactly); otherwise the twin row
  stays until the next listing. Building the map on every rename would cost 33 ms per
  single-rename window on a 40k listing.
- **`explained` is by name and time only**: a program touching a just-copied file within 3 s of
  our job is dropped with it (the next change shows it), and a job writing into a subfolder
  (`Modified(subdir)`) is not explained, so that pane still refreshes twice; a pane with folder
  sizes (a subtree watch) refreshes twice after a job too, since subtree windows skip the check.
- **`land_edits` after a navigation or a requested pass falls back to a refresh** of the (new)
  folder rather than dropping or re-queuing the edits; harmless and rare.
- **The directory handles may block a rename of an ancestor folder** ("folder in use"), up to
  eight background handles and the active tabs' parent handles: to verify at the console.
- **Rows inserted or removed above the viewport shift the visible rows** (the scroll offset is
  not compensated, nor a background tab's saved scroll); `row_insert`/`row_remove` cost grows
  with the selection (Ctrl+A on 40k rows); the parent watch's `pane` is not updated when a tab
  changes pane (log lines only); `Moved` does not follow tabs on descendants of the renamed
  folder, nor the icon keys built with the old path.
- **Unit tests cover the pure pieces** (`watch_set`, `explained_by`, `edits_from`, `stat_dir`,
  `net_changes`, `Backoff`, `apply_edits`) and the watcher's thread; `window_refusal`,
  `land_edits`'s leniency and `watch_tabs`'s stale marking are covered by the scripted checks and
  the soak only. `apply_edits_notifies_rows_not_resets` records notifications through
  `slint::private_unstable_api` (test code; fragile across a fork bump).
- **Records queued for a tab's old folder can arrive after a navigation**; they are statted against
  the new path and at worst cost a fallback refresh, never a wrong row (the young-listing rule
  covers the common case).

### Scripted checks

Harness session (RDP, 1216 x 800 window, scale 1.0, accessibility on), this worktree's
`target\release\twinfm.exe` from the commit under review, scratch folders under
`%TEMP%\twinfm-p5m1-manual\` (no soak fence), every run `TWINFM_SCRIPT`-driven with its stderr
kept; exit code 0 and no `panicked` line throughout.

- **(i) The four edits**, after `click 0 2`, each with `waitedit 3000`: `touch` gives `applied: +1
  -0 ~0 (stat 4.5 ms, apply 0.0 ms)`, a second `touch` `+0 -0 ~1`, `mvfile` `+0 -0 ~1` (the rename
  keeps its row), `rmfile` `+0 -1 ~0`; the same script with the folder typed as
  `C:/Users/.../twinfm-p5m1-manual/./c1s` gives the same four lines (the review found that an
  unnormalised folder made every stat fail under the verbatim prefix, turning a modify into a
  removal); no `[refresh]` between them and no `[select]` after the
  click. With `click 0 2; ctrlclick 0 4` and `a-new.txt` (which sorts before the cursor) inserted
  then removed, `scrollpos` reads `current-row 4`, then 5, then 4: the cursor follows its row.
- **(ii) Netted to nothing**: a separate PowerShell creating and deleting a file within 50 ms gives
  `records 3, changes 0, window 250 ms` and `3 records netted to nothing`; no refresh.
- **(iii) Bursts and the back-off**: 600 files written in a loop arrive as `records 333, changes
  333, window 250 ms` / `applied: +333 -0 ~0 (stat 26.5 ms, apply 0.4 ms)` and `records 267 ...
  window 500 ms` / `+267 (14.0 ms, 0.5 ms)`, both under the cap; a writer every 20 ms for 4 s logs
  `back-off: window 250 ms`, `500 ms`, `1000 ms`, `2000 ms` with 16, 34, 66 and 126 records per
  window, each applied as `~1`.
- **(iv) A background tab**: a second tab on another folder, a switch back, an external write
  there, a switch to it: `tab=3 watched in the background`, `records 3, changes 2`, `applied: +1 -1
  ~0` while in the background, then `[tab] pane=0 switched to 2 of 2 (id 3, ...)` followed by
  `parent watch on` and no `[refresh]`.
- **(v) The cap**: nine tabs log `watched in the background` for seven of them and `tab=1 past the
  cap: stale when shown`; after `closed tab 9 ... 8 left` a `tab new` gets `watching` with no `cap
  reached`; selecting tab 1 again makes another tab the one past the cap and refreshes tab 1 (it
  was stale).
- **(vi) The folder's own rename and delete** (PowerShell beside the app): `folder renamed:
  ...\r6 -> ...\r6-renamed`, `[nav] pane=0 path follows the rename: ...\r6-renamed`, a `touch`
  inside it `applied: +1`; the delete gives `lost ...\r6-renamed: Access is denied. (os error 5)`,
  `watch lost (...): refresh`, one `[refresh]` whose listing fails and the error in the status bar.
- **(vii) Our own copy** (`cc`, `cv` of three files from the left pane into the right): `[fileops]
  done ... Copied 3 items`, one `[refresh]` per pane, then `tab=2 records 9, changes 3, window 250
  ms` and `9 records explained by the operation: no refresh`.
- **(viii) The grid** (`view icons`, ten tiles to a row): an insert `applied: +1 -0 ~0 (stat 4.4
  ms, apply 0.1 ms)` with `[icons] ... +1 requests +1 replies` for the new tile, a removal `-1`,
  the cursor 3, 4, 3.
- **(ix) The gate scenario** on `C:\traces\bigdir`: the only `[watch]` lines are `watching
  C:\traces\bigdir` and `parent watch on C:\traces` per pane; no window, no fallback.
- **(x) The soak**, `tools/soak-p2m1.ps1 -Minutes 2` on the PR head: exit 0, 0 `panicked`, 8
  cycles, 1,775 writer operations, 543 `[watch]` lines, 2,616 records delivered, 177 windows
  applied (+156 -88 ~106), 0 edits dropped, 0 fallbacks, 30 `watched in the background`, 106
  back-off lines (largest window 2,000 ms), 14 `[refresh]`, 7 switches to the background tab with
  0 refreshes; working set 127.9 to 130.1 MB. Before the final review's fixes,
  `tools/soak-p2m1.ps1 -Minutes 3`: exit 0, 0 `panicked`, 12 cycles, 2,764 writer
  operations, 904 `[watch]` lines, 4,046 records delivered, 286 windows applied (+294 -219 ~173), 0
  edits dropped, 0 fallbacks of any reason, 46 `watched in the background`, 0 past the cap, 209
  back-off lines with a largest window of 2,000 ms, 22 `[refresh]` lines, 11 switches to the
  background tab with 0 refreshes on the switch, 0 `waitwatch`/`waitedit` timeouts; working set
  126.5 to 130.3 MB, private 117.0 to 117.4 MB. The two runs before the young-listing rule had
  seven `unknown name` fallbacks each.
- **(xi) The lists are the reference's.** The generated Rust of `target-p4m3` (53dbc50) and this
  build's, numbering normalised, differ in one line (the embedded icon's checkout path); the list
  and grid components' region (`InnerFileList_root_` to `InnerFileGridList_root_`, 6,091 lines) in
  0. No `.slint` file changed.

### Measurements

**The edit path's cost**, on a 40k copy of `bigdir` with 100 external creates per window, this
worktree's build before the review's last fix: `applied: +100 (stat 8.6 ms, apply 43.5 ms)` at the
front of the sort, `+100 (8.0, 34.2)` at the end, `-100 (0.0, 30.4)` front removals, `-100 (0.0,
4.5)` end removals; a benchmark without views (40k entries) read inserts at 33 to 36 ms for 1, 100
or 500 at the end and 33, 40, 62 at the front, removals 5 to 8 ms at the end and 5, 31, 130 at the
front: about 33 ms fixed per window with an insert (the case-insensitive name map, built on the
insert's exact miss), 5 ms for the index map, 0.25 ms per front removal (the memmove). After the
fix (the map only for a removal's miss) one insert at the end is 5.5 to 6.2 ms. On a ten-row
folder 333 inserts take 0.4 ms. The 500-edit cap and the 1,000-record overflow follow from these
numbers: a window past them re-lists on the worker instead.

In-app, RELATIVE only: harness session (RDP), 1216 x 800 window, scale 1.0, accessibility on, the
gate scenario on `C:\traces\bigdir`, `[busy]` over the 10 s after the split, each candidate run
interleaved with `target-p4m3` (53dbc50, the reference) by the same runner, reference first, three
rounds per settings file.

| Pair | reference, defaults | P5-M1, defaults | reference, `grid.toml` | P5-M1, `grid.toml` |
|---|---|---|---|---|
| 1 | 18.0 | 16.4 | 9.8 | 10.0 |
| 2 | 19.1 | 17.3 | 11.1 | 10.6 |
| 3 | 20.9 | 17.0 | 10.3 | 10.5 |
| **median** | **19.1** | **17.0** | **10.3** | **10.5** |
| composes | 371 strips at 22.4 to 24.1 us | 371 at 21.7 to 22.6 us | 1,422 rows at 69.0 to 72.5 us | 1,422 at 69.5 to 72.3 us |

The PR head (ef7f057) on a quiet machine. Defaults: the candidate sits below the reference in
all three pairs despite the second-slot bias of P4-M2 and P4-M3 (which lifted the second run of a
pair by one to three points there). Grid: the medians differ by 0.2 and the pairs go both ways
(10.0 against 9.8, 10.6 against 11.1, 10.5 against 10.3), inside the reference's own spread.
Nothing on the row or compose path moved: the generated rows are the reference's (Scripted checks
(xi)), and what differs in the scenario is two idle parent handles, a `Refused` that never fires
on `bigdir`, `landed_pass` set once per pass, and `cmp_entries` out of line on the sort worker.
An earlier set on 3d87f73 (before the review's fixes) read 19.4 / 20.8 / 19.5 against 18.6 /
19.1 / 18.4 (defaults) and 12.0 / 11.2 / 13.0 against 11.7 / 11.1 / 10.5 (grid), the candidate
below in all six pairs; a set run while `cargo test` compiled beside it read 23.0 for the
candidate's third defaults run and is discarded (the machine, not the binary). Each grid run's
stderr carries `[settings] file C:\traces\grid.toml (read)` and `[view] pane=0 starts as large
icons`; every run's `[watch]` lines are the two `watching` and two `parent watch on` lines and
nothing else; 0 `panicked`. The console triplet decides.

### Gate

`tools/wpr-triplet.ps1 -Reference p4m3 -Candidate p5m1,p5m1@C:\traces\grid.toml -Tag ab20`, run
from this checkout's `tools\`, Jeremy elevated at the console, accessibility on, Narrator off, no
other TwinFM window, console checks after. `target-p4m3` (53dbc50) against `target-p5m1` (the PR
head, built with `--locked` from a detached checkout). The grid run's stderr must show `[settings]
file C:\traces\grid.toml (read)` and `[view] pane=0 starts as large icons` before its triplet is
read. Nothing in the scenario changes on disk, so what the triplets measure is the wiring: the
parent handle on `C:\traces`, the `Refused` event per listing of an unwatchable path, `landed_pass`
set once per pass, and `cmp_entries` out of line on the sort worker; the rows are the reference's
(Scripted checks (xi)).

Console checks after the triplet: a USB stick inserted, opened in a tab, a file written to it from
Explorer (the row appears), "Safely remove" (`suspended for removal`, the eject succeeds), the
stick re-inserted; a dead share if one can be arranged (`stalled: no watch`, F5 still lists it or
shows the error, the UI never waits); a folder renamed in Explorer while a tab shows it (the path
bar follows); a tab left in the background while Explorer writes into its folder, then selected
(no re-listing, the new rows there).

## Open questions

Written 2026-10-15 at the kickoff; the Phase 4 close's open questions are absorbed here.

- **The cap on watched background tabs** (P5-M1): settled, 8 for the app, active tabs first
  (§ P5-M1 Decisions).
- **How the watched folder's own rename is noticed** (P5-M1): settled, a parent watch for the
  active tabs only (§ P5-M1 Decisions); whether the handles block a rename of an ancestor folder is
  a console check.
- **Undo of a delete to the Recycle Bin** (P5-M2): the bin's "undelete" verb against a move back
  from `$Recycle.Bin`; the verb keeps the bin's metadata and is what Explorer plays, the move is
  what the sink's paths allow directly. Tried in that order.
- **The spring-load delay** (P5-M3): Explorer opens after about a second; the number is set at
  the console.
- **The Slint whole-row snap patch** (P3-M1 Deferred, P4 Open questions): still Jeremy's call;
  every view switch shows the snap. Nothing in Phase 5 touches the fork.
- **Upstreaming the two accessibility patches** (`docs/upstream-issue-draft.md`,
  `docs/patches/`): still deferred; Jeremy decides when. Do not post to slint-ui/slint from a
  session.
- **Carried from Phase 4, not in any milestone**: everything in `phase-4-plan.md` § Phase 4 close
  under Deferred (the views, the grid's accessibility, thumbnails, the band) and under the
  candidates not chosen (content search, archives, the compact menu, OneDrive marks, search
  polish, folder sizes, daily use). Accessibility is deferred for much later by Jeremy's word.
