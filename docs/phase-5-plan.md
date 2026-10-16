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

**Deferred by Jeremy (2026-10-15): the ab20 triplet and the console checks run after P5-M2**, not
before the merge. Run on 2026-10-16 with P5-M2 as the candidate (§ P5-M2 Gate): both pass. The decision rests on the in-app pairs above (the candidate below the reference
in every defaults round), the generated rows being the reference's, and nothing in the diff running
per row or per frame; the cost is that a gap found later is bisected across two milestones instead
of one. `target-p4m3` stays the reference until a triplet is recorded here; the USB check (insert,
write from Explorer, "Safely remove", re-insert) is the first real run of the device path and is
still owed.

## P5-M2: file operations finished

The second milestone of the phase (2026-10-15): redo, undo of a delete through the Recycle Bin's
own item, undo of a move that renamed on collision, the cut row's icon dimmed with its text, and
the running job's progress in the status bar. Branch `claude/p5-m2-file-operations-31492e`, from
3131d06 (P5-M1). The brief is the P5-M2 entry of the kickoff above; the work was split three ways
(the file-operation worker and the undo record; the icon strips, the status text, the driver, the
soak and the window's key handler; the wiring in `main.rs`), the fileops half reviewed before the
wiring relied on it, then the branch as a whole, with one fix wave.

### What landed

- **Two jobs for the undo record** (`src/fileops/mod.rs`). `Job::MoveAs { items: (path, name),
  dest_dir }` moves each item into a folder under a given name (`IFileOperation::MoveItem` with
  `pszNewName`); `Job::Restore { items: ($R path, original) }` restores recycled items. A restore
  is not an `IFileOperation` job: the worker lists the Recycle Bin once (`SHGetKnownFolderItem` on
  `FOLDERID_RecycleBinFolder`, `BindToHandler(BHID_EnumItems)`, each item's `SIGDN_FILESYSPATH`
  being its `$R` path), takes the matching item's `IContextMenu` (`BHID_SFUIObject`), fills a
  popup menu with `QueryContextMenu` and invokes the ANSI verb `undelete` (`CMIC_MASK_NOASYNC`, and
  `CMIC_MASK_FLAG_NO_UI` under the soak and the tests, the request's hwnd as the owner), then waits
  (pumping the worker's messages in 20 ms steps, at most 30 s) until the `$R` path is gone and the
  original is there. When the verb cannot be reached (the item is not in the bin, no context menu,
  `QueryContextMenu` fails), fails, or times out, the item is moved back instead: a one-item
  `MoveAs` of the `$R` path into the original's folder under the original's name, through the
  ordinary sink, after which the sibling `$I` metadata file is removed. A verb the user cancelled
  records the cancel and runs no fallback; an original path that is taken again is refused before
  any shell call (item HRESULT 0x800700B7, `ERROR_ALREADY_EXISTS`); a `$R` path outside
  `\$Recycle.Bin\` is refused (`E_INVALIDARG`); a `WM_QUIT` met in the wait stops the job as
  cancelled. Each item gets one `ItemResult { source: $R, dest: original, hr }`; `Restore.paths()`
  names the originals only, so the soak fence sees what the job writes. `[fileops] undelete <$R>
  -> <original>: verb ok (T ms) | verb cancelled 0x... | verb failed 0x...: moved back | verb not
  reachable (<why>): moved back | already back, nothing moved | moved back failed 0x... | refused
  (<name> is in the way) | not a Recycle Bin path | quit while waiting: stopped`. Status texts
  "Moved N items" and "Restored N items from the Recycle Bin", running texts "moving..." and
  "restoring...".
- **The undo record** (`src/fileops/undo.rs`). A move whose result renamed any item on collision
  ("a (2).txt") is undone by a `MoveAs` back under the sources' names (a plain `Move` back
  otherwise); a `MoveAs` by a `MoveAs` back; a delete to the Recycle Bin by a `Restore` from the
  `$R` paths the sink reported (a recycle that reported none still records `undo: None` with the
  note "Cannot undo: the Recycle Bin reported no items"); a `Restore` by a recycle of the originals
  again. `check` refuses a restore whose `$R` path left the bin ("is no longer there") or whose
  original path is taken ("Cannot undo: <name> is in the way").
- **Redo** (`main.rs`). `State.redo` is a second stack, capped by the same `push`; `JobInfo.replay`
  carries the record an undo or redo job replays. The pure `after_job` decides: a plain job that
  changed something (any ok result) records onto undo and clears redo (`[undo] redo cleared
  (N)`); an undo job that ends `Done` records `record_for(&undo_job, ..)` onto redo (`[undo] redo
  recorded "<text>": <job>`), a redo job the same onto undo; a replay that ran nothing (cancelled
  while queued, refused by the fence, a stopped worker) goes back on its stack (`[undo] record
  kept: "<text>"`). Ctrl+Y (`redo`, the mirror of `undo`: `[undo] redo "<text>" with <job>`, `redo
  "<text>" refused: <why>`, `nothing to redo`, "Redid: <text>" / "Redo: <status>") is caught in the
  window's outer `FocusScope` in `app.slint`, beside Ctrl+T, so `pane.slint` and the list and grid
  components are untouched; the list rejects the key and it bubbles there. Ctrl+Z and Ctrl+Y
  while any job is in flight are refused (`[undo] busy: N operation(s) in flight`, the note "Wait
  for the running operation to finish") rather than popping a record that depends on the running
  one. The removal fallback of the cursor covers a `MoveAs` out of the pane's folder
  (`removed_names`).
- **The cut row's icon dims** (`src/icons.rs`, `src/model.rs`). `compose(icons, dim, g)` and
  `compose_grid(icons, dim, g, rows)` take the entries' cut flags (`PaneModel::cut_flags(first,
  last)`, read at compose time); an entry whose flag is set has its copied scanlines passed
  through `dim_line`, every premultiplied channel scaled to 45% (the text's `transparentize(55%)`:
  255 becomes 115). An undimmed entry costs one slice lookup; the icon table is untouched. A cut
  (`clipboard_put`), its clearing (`clear_cuts`: the paste, another program taking the clipboard,
  "Copy path") mark the affected panes' strips dirty and resolve once; a pass swap and a watch's
  row edits already recompose, so the marks carried by name still dim after a refresh. The grid's
  tiles go through the same flags.
- **Progress in the status bar**. The sink's `UpdateProgress` computes `percent(total, sofar)`
  (None for a zero total, clamped to 100) and reports through `ProgressGate` (the first report,
  then at most one per 250 ms and only when the percent changed, 100 at once), to a second
  callback of `Worker::spawn`; `job_progress` (an `invoke_from_event_loop` hop, like `job_done`)
  stores it for the job in flight, logs `[fileops] progress id=N P%` and composes that pane's
  status once. `update_status` appends the percent (`statusbar::with_progress`) to the running
  job's own note only (`progress_for`: this pane, this tab, and the note still the job's running
  text) before the queue count: "copying 3 items... 42%, 1 operation queued". The shell's window
  stays.
- **Driver** (`auto.rs`): `redo` (key name `CtrlY`); the recycle-and-undo cycle is `delete`,
  `waitops`, `undo`, `waitops`, `redo`, `waitops`.
- **Soak** (`tools/soak-m3.ps1`): an `undo` (the restore through the bin's verb) and a `redo` after
  each cycle's recycle; the report counts redos, nothing to undo/redo, refused, busy, record kept,
  the `undelete` outcomes (verb ok, moved back, moved back failed) and the progress lines.
- **Tests**, 159 to 171: `fileops`: `progress_percent_and_gate`, `restore_moves_only_out_of_the_bin`
  (the `$R` guard, `info_file`), `status_texts` extended (MoveAs, Restore, `paths()`),
  `recycle_restore_move_as_and_progress` (the real bin: a file recycled, restored through the
  record in under 25 s, its content and the `$R`/original result checked, the redo material a
  recycle again; a `MoveAs` round trip; progress from a folder copy of 8 x 256 KB); `undo`:
  `moves_under_new_names_recycles_and_restores`, `records_and_the_cap` and
  `check_refuses_missing_and_non_empty` extended (a restore record, "in the way"); `icons`:
  `dim_line_scales_every_channel`, `compose_dims_cut_entries_only`; `model`:
  `cut_flags_follow_the_shown_rows`; `statusbar::progress_text`; `main`:
  `after_job_feeds_the_two_stacks`, `removed_names_are_the_sources_in_that_folder`,
  `progress_shows_beside_the_running_jobs_own_note`, `replay_text_names_the_record_when_done`,
  `explained_names_are_the_jobs_names_in_that_folder` extended (MoveAs, Restore).
- **Docs**: `CLAUDE.md` (test count, the step and key name, the lines, the soak, the bin test).

### Decisions

- **The bin's verb first, the move back second**, as the kickoff's open question asked. At the real
  bin the enumeration exposes each item's `$R` path as its file-system path, so the match is
  direct; the verb returns synchronously for a small file (73 to 107 ms in the checks, 270 to 317
  ms for the soak's 200-file tree) and restores to the location the `$I` file records. The move
  back is what the sink's paths allow directly and is taken only when the verb cannot be reached,
  fails or times out; never after a cancel (the user said no once) and never when the original
  path is taken (the move would replace it under `FOF_NOCONFIRMATION`).
- **A taken original path refuses the undo** instead of letting the verb's conflict dialog decide:
  with "keep both" the item would come back as "x (2).txt" while the result named the original,
  and the redo would then recycle the wrong file.
- **Undo and redo wait for the running job.** A second Ctrl+Z during an undo popped the next
  record and checked it against a disk the first undo had not changed yet (rename then move:
  "b is no longer there"), dropping it for good. Refusing with a note is Explorer's behaviour too.
- **Only a job that changed something clears redo**: a queued job cancelled with Escape, a cancel
  at the shell's first dialog, a job that failed on every item leave the redo stack alone.
- **A replay that ran nothing keeps its record**; a replay refused by `check` still drops it, as
  P2-M2 decided (the items are not where the job left them).
- **Ctrl+Y is window-level, Ctrl+Z list-level.** The alternative, a line in `pane.slint`'s `key()`,
  would change the generated list components' key callback and with it the diff this phase
  relies on. Ctrl+Y therefore works from anywhere but the text boxes (whose own Redo takes it) and
  the settings dialog (excluded by the condition); the asymmetry is recorded, not hidden.
- **Dimming at compose time, in the destination buffer**: the icon table keeps one image per key,
  the strip pays the scaling only for cut rows, and the gate scenario (no cut rows) pays one
  `Vec<bool>` per compose.
- **The percent follows the running job's own note.** With a second job queued from the same tab
  the note already shows the newer job's running text; the percent is shown only while the note
  is the running job's, so "Nothing to undo 42%" cannot appear.
- **The throttle lives on the worker** (`ProgressGate`), so the UI thread sees at most four hops a
  second whatever the copy engine's cadence.

### Deviations

- **`check` for a restore is stricter than the brief**: the original path must be free (above).
- **`Restore.paths()` names the originals only**, so the soak fence can allow a restore into the
  scratch root; the one guard is that every `$R` path must lie under `\$Recycle.Bin\`.
- **The verb's `NOASYNC` and `NO_UI` flags** were not in the brief: without `NOASYNC` a timeout
  could race the fallback move against a verb still running; `NO_UI` only under the soak and the
  tests.
- **An undo whose Delete sources all lie outside the pane's folder gets no cursor fallback now**
  (`removed_names` is empty); before, every undo Delete got one.
- **The soak's "moved back" count** reads the attempt lines only (the failure line also contains
  the words).

### Deferred

- **A restore with one failed item is `Failed` and records nothing**: the items that did come back
  cannot be redone from TwinFM (existing jobs behave the same when `IFileOperation` fails).
- **A mixed recycle** (some items nuked for size or on a volume without a bin) restores the subset
  the bin reported and says "Undid: Deleted N items".
- **Progress through the fallback**: each move back is its own `IFileOperation` with a fresh gate
  (0 to 100 per item); the verb path reports no progress at all, so a restore shows none.
- **Orphan `$I` files**: a restore through the verb leaves the `$I` metadata file on disk in
  `$Recycle.Bin\<SID>\` (the bin does not list it; two such orphans predate this work, so Windows's
  own restores leave them too); the fallback removes its own.
- **Each fallback move adds an entry to Explorer's own undo list** (`FOFX_ADDUNDORECORD`).
- **An undo or redo refused by `check` drops its record** (P2-M2's decision, now on two stacks).
- **The percent is per job, not per queue**: the queue count beside it is the only sign of the jobs
  behind.
- **Ctrl+Y from the sidebar, the tab strip and a crumb button** (above).
- **Undo of a copy or move that replaced an existing file** still does not restore the replaced
  file (P2-M2 Deferred).
- **A delete from the shell context menu leaves no undo record** (console check, 2026-10-16): the
  menu's verb runs the shell's own operation. Routing the menu's Delete (and Cut, Copy, Rename)
  verbs to TwinFM's jobs would record them; a Phase 5 close candidate.
- **Escape does not cancel a cut** as Explorer's does; the marks clear on a paste, a copy or
  another program taking the clipboard.

### Scripted checks

Harness session (RDP, 1216 x 800 window, scale 1.0, accessibility on), this worktree's
`target\release\twinfm.exe`, scratch folders under `%TEMP%\twinfm-p5m2-manual\` (no soak fence),
stderr kept; exit code 0 and no `panicked` line throughout.

- **(i) The recycle-undo-redo cycle**, `click 0 0; delete; waitops; undo; waitops; redo; waitops;
  undo; waitops` on a folder of three files: `[fileops] done id=1 ... Deleted 1 item to the Recycle
  Bin`, `[undo] recorded "Deleted 1 item to the Recycle Bin": Some(Restore { items: [("C:\$Recycle.Bin\S-1-5-21-...\$RP9VRFP.txt", "...\a\f1.txt")] })`,
  `[undo] "Deleted 1 item to the Recycle Bin" with Restore {..}`, `[fileops] undelete ...$RP9VRFP.txt
  -> ...\a\f1.txt: verb ok (73 ms)`, `[undo] done id=2 Done: Undid: Deleted 1 item to the Recycle
  Bin`, `[undo] redo recorded "Deleted 1 item to the Recycle Bin": Some(Delete { sources: [...f1.txt],
  permanent: false })`, `[undo] redo "Deleted 1 item to the Recycle Bin" with Delete {..}`, `[undo]
  redo done id=3 Done: Redid: Deleted 1 item to the Recycle Bin`, then `[undo] recorded ...
  Restore` again and the second undo's `verb ok (73 ms)`. The Recycle Bin's item count
  (`Shell.Application`'s `NameSpace(10)`) was the same before and after (1421).
- **(ii) The redo stack clears and the busy refusal**: a `move` right after it with `undo` pressed
  before `waitops`: `[undo] busy: 1 operation(s) in flight`, the note "Wait for the running
  operation to finish"; then `[undo] recorded "Moved 1 item"`, `[undo] redo cleared (1)`, and after
  `waitops` `[undo] "Moved 1 item" with Move {..}`, `Undid: Moved 1 item`.
- **(iii) A taken name**: a file recycled, a new file written at its path from PowerShell, `undo`:
  `[undo] "Deleted 1 item to the Recycle Bin" refused: Cannot undo: f1.txt is in the way`, no job
  submitted, the new file untouched (the original stays in the bin: one entry).
- **(iv) Progress**: every job through the sink logs `[fileops] progress id=N 0%` and `100%` (a tiny
  job), the soak's 200-file copies `0%, 26%, 54%, 81%, 100%` over about a second; a restore through
  the verb logs none.
- **(v) Cut dimming**: `click 0 0; cx` then `click 0 1; cc`: the once-a-second `[icons]` compose
  count rose by one at each (14 to 15 to 16 in the details list; 32 to 33 to 34 grid rows with
  `C:\traces\grid.toml`), nothing else recomposed.
- **(vi) The grid** (`TWINFM_CONFIG=C:\traces\grid.toml`, read-only): the same script, `[settings]
  file C:\traces\grid.toml (read)`, `[view] pane=0 starts as large icons`, the same Undid and Redid
  lines, the bin at the same count.
- **(vii) The soak**, `tools/soak-m3.ps1 -Minutes 2` on the PR head (78c184b): exit 0, 0 `panicked`, 4
  cycles, 56 jobs (0 failed, 0 cancelled), 0 fence refusals, 0 `waitops` timeouts; 4 redos, 0
  nothing to undo or redo, 0 refused, 0 busy, 0 records kept; 4 restores all `verb ok` (270 to 317
  ms for the 200-file tree), 0 moved back; 128 progress lines; working set 108.7 MB at 5 s, 149.6
  MB at 121 s (peak 159.8), private 111.5 to 147.5 MB. The same two minutes with the 3131d06
  build (the main checkout's `target
elease`) and its own soak script, run right after: exit 0,
  0 `panicked`, 4 cycles, 48 jobs, working set 112.1 to 176.8 MB, private 114.3 to 158.7 MB; the
  milestone adds no memory. A first run before the fix wave (c510c84) read 5 cycles, 59 jobs, 4
  redos, 4 `verb ok`, 140 progress lines, working set 104.6 to 167 MB.
- **(viii) The lists are the reference's.** The generated Rust of `target-p4m3` (53dbc50) and this
  build's (78c184b), numbering normalised, differ in the Ctrl+Y branch of the window's key
  callback (13 lines in `InnerApp`) and the embedded icon's checkout path; the list and grid
  components' region (`InnerFileList_root_` to `InnerFileGridList_root_`, 6,091 lines) in 0.
  `src/ui/pane.slint` did not change.

### Measurements

In-app, RELATIVE only: harness session, 1216 x 800 window, scale 1.0, accessibility on, the gate
scenario on `C:\traces\bigdir`, `[busy]` over the 10 s after the split, each candidate run
interleaved with `target-p4m3` (53dbc50, the reference) by the same runner, reference first, two
sets of three rounds per settings file (six pairs), no cargo or agent running beside. **Jeremy's
session was disconnected throughout** (`query session`: `Disc`), so the window presented at about
21 Hz (frame interval p50 46.6 to 47.3 ms in every run) and the busy figures sit far below P5-M1's
(which ran under RDP at 32 Hz) and swing widely; they are comparable only with each other.

| Pair | reference, defaults | P5-M2, defaults | reference, `grid.toml` | P5-M2, `grid.toml` |
|---|---|---|---|---|
| 1 | 9.7 | 4.8 | 4.5 | 6.2 |
| 2 | 5.1 | 11.4 | 17.0 | 7.6 |
| 3 | 9.7 | 8.7 | 6.7 | 2.2 |
| 4 | 10.8 | 9.2 | 3.1 | 10.3 |
| 5 | 9.0 | 17.2 | 4.7 | 2.8 |
| 6 | 8.9 | 11.2 | 5.0 | 4.5 |
| **median** | **9.4** | **10.2** | **4.9** | **5.4** |
| composes | 371 strips at 20.6 to 21.7 us | 371 at 20.8 to 21.9 us | 1,422 rows at 66.6 to 69.0 us | 1,422 at 66.9 to 68.7 us |

The PR head (78c184b). The pairs go both ways in both settings (defaults: the candidate below in
pairs 1, 3 and 4, above in 2, 5 and 6; grid: below in 2, 3, 5 and 6, above in 1 and 4), the
medians differ by 0.8 and 0.5, and the reference's own spread (5.1 to 10.8, 3.1 to 17.0) is wider
than either gap: the pairs see no cost and cannot see a small one in this session. What the
scenario exercises of the branch is exactly the compose path (the `cut_flags` read and the `dim`
lookup per entry), and its figures are the reference's to the microsecond: 371 strips and 1,422
grid rows per run in every one of the 24 runs, at the same averages. Every grid run's stderr shows
`[settings] file C:\traces\grid.toml (read)` and `[view] pane=0 starts as large icons`; every
run's `[watch]` lines are the two `watching` and two `parent watch on` lines; no `[fileops]`,
`[undo]` or progress line; 0 `panicked`. The console triplet decides.

### Gate

`tools/wpr-triplet.ps1 -Reference p4m3 -Candidate p5m2,p5m2@C:\traces\grid.toml -Tag ab20`, run
from this checkout's `tools\` on 2026-10-16 (08:38 to 08:46), Jeremy elevated at the console,
accessibility on, Narrator off, no other TwinFM window. `target-p4m3` (53dbc50) against
`target-p5m2` (6b1d0f1, the PR head, built with `--locked` from the detached checkout
`.claude\worktrees\gate-p5m2`). The grid runs' stderr shows `[settings] file C:\traces\grid.toml
(read)` and `[view] pane=0 starts as large icons`; no run has a `[fileops]`, `[undo]` or `panicked`
line. Analysed with `tools/analyze-trace.py --symbols --exe`; traces `C:\traces\twinfm-ab20-*.etl`,
reports under `C:\traces\export\`.

| Run | reference `target-p4m3`, WPR | P5-M2 defaults, WPR | P5-M2 `grid.toml`, WPR | in-app (ref / defaults / grid) |
|---|---|---|---|---|
| 1 | 18.5% | 18.4% | 10.2% | 17.3 / 17.5 / 10.0 |
| 2 | 18.5% | 17.3% | 10.0% | 19.2 / 19.7 / 11.2 |
| 3 | 17.6% | 17.4% | 9.9% | 19.1 / 18.3 / 9.5 |
| **median** | **18.5%** | **17.4%** | **10.0%** | 19.1 / 18.3 / 10.0 |

**P5-M2 passes, and with it P5-M1** (this one triplet judges both, § P5-M1 Gate): every run of
both candidates sits under the 20% bar, the defaults candidate's median is 1.1 points below the
reference's with the runs interleaving (18.4 against 18.5 and 17.6; 17.3 and 17.4 below all
three), and the grid median of 10.0 is P4-M3's own 10.2 (`phase-4-plan.md` § Phase 4 close). Frame
shares, reference runs 1 to 3 / candidate runs 1 to 3 (sampled ms in the window): accessibility
201, 185, 188 / 183, 187, 177; render 1081, 1098, 1042 / 1087, 1024, 1021; draw_text 634, 643, 617 /
630, 596, 608; ListView 346, 345, 317 / 342, 306, 357; timers 654, 641, 610 / 634, 602, 626; heap
187, 181, 186 / 186, 159, 174. No fileops, undo, compose, `cut_flags` or `dim_line` frame appears
in any candidate window: the scenario never cuts a row or runs a job, and the cut flags are read
inside the compose that the reference pays too. The grid's shares (accessibility 64 to 77, render
602 to 617, ListView 38 to 47) are the P4 grid's. Peak working set 150 to 161 MB in eight runs and
187 MB in the first candidate run (its busy figure was unremarkable); not explained.

Console checks after the triplet, Jeremy's: P5-M1's (a USB stick inserted, written to from
Explorer, "Safely remove", re-inserted; a dead share if one can be arranged; a folder renamed in
Explorer under a tab; a background tab written into, then selected) and P5-M2's (a file deleted in
TwinFM, Ctrl+Z: the file is back and Explorer's Recycle Bin no longer lists it; Ctrl+Y: it is back
in the bin under its original name; a cut row's icon dimmed beside its text in both views; a copy
large enough to show the percentage climbing beside the shell's window).

**Console checks, 2026-10-16, Jeremy at the console with the `target-p5m2` build:**

- Delete key, Ctrl+Z: the file is back, Explorer's Recycle Bin no longer lists it; Ctrl+Y: back in
  the bin under its original name; Ctrl+Z again brings it back. **A delete through the right-click
  menu cannot be undone**: the shell's own verb runs it, outside the worker and the sink, so no
  record is written (P2-M3's menu; see Deferred).
- Ctrl+Z during a running copy: "Wait for the running operation to finish", nothing undone; the
  copy undone afterwards.
- A taken name: Jeremy deleted `BUILD.txt`, made a new text document in Explorer and renamed it,
  and reports the undo bringing the file back as `BUILD (2).txt`, which is the result of Ctrl+Z
  pressed before the rename (the restore ran onto the free name, and Explorer's later rename
  collided with it and numbered its own file). The same flow scripted against the gate build with
  the rename done first (`New Text Document.txt` created and renamed to `BUILD.txt` by PowerShell
  while TwinFM waited 6 s, then `undo`) gives `[undo] "Deleted 1 item to the Recycle Bin" refused:
  Cannot undo: BUILD.txt is in the way`, the new file untouched and the old one left in the bin:
  the refusal works as designed, and the order of the steps decides which of the two a person sees.
- A cut row's icon dims with its text in the details list and in the grid; Ctrl+C on another row
  restores it (the copied row is not marked). Escape does not clear a cut, and never did (Explorer's
  does; see Deferred).
- Progress: the percentage climbs in the status bar beside the shell's window, "N operations
  queued" after it with a second copy started.
- A move onto a folder holding the same name: the shell's Replace-or-Skip dialog, "keep both" under
  "Compare info"; after Ctrl+Z the moved file came back to its folder under its original name,
  not "(2)".
- P5-M1's: a USB stick written to from Explorer shows the row; "Safely remove" succeeds; re-inserted
  and opened again. A folder renamed in Explorer under a tab: the path bar follows. A background tab
  written into, then selected: the rows are there with no re-listing. A dead share was not available:
  that check stays owed.

## P5-M3: drag and drop finished

The third milestone of the phase (2026-10-16): spring-loaded folders and tabs, drop descriptions
under the drag image, drops on breadcrumb crumbs, auto-scroll of the sidebar list and the tab
strip, mounted folders counted as their own volume, and the first drag's wait for its icon removed.
Branch `claude/p5-m3-milestone-setup-c40a6d`, from 876a0b9 (P5-M2). The brief is the P5-M3 entry of
the kickoff above; the work was split in two halves run one after the other because both edit the
drop target's methods in `src/dnd.rs`: first the self-contained items (descriptions, the icon,
volumes: 92a6a0c), reviewed and committed, then the hit-test and timing items (the spring, crumbs,
the strips' auto-scroll, the driver, the soak: 5db406b), reviewed and committed. Both implementers
were Opus 5.5 agents at Jeremy's word; the lead reviewed each diff and ran the gates again.

### What landed

- **Spring-loaded folders and tabs** (`src/dnd.rs` `Spring`, `SPRING_MS` = 1000). A drag held over
  a folder row, a folder tile or a tab other than the pane's active one opens it in that pane, for
  our own drags and other programs' alike. The timing is a pure state machine in the drop target
  (`arm` when the spot changes to one that opens, `check` on every `DragOver`, which OLE calls on
  its own timer even while the mouse rests, so no timer of ours is needed and none could run inside
  our own drag loop; `disarm` when the spot changes, on DragLeave and on Drop; after firing, the
  same spot does not fire again until the pointer has left it). The open is `crate::spring_open`
  (`navigate`) or `crate::spring_tab` (the tab's `select`), each posted through
  `slint::invoke_from_event_loop`, the closures the drag loop already runs (P2-M4 amendment), so the
  drop target touches no app state and the next DragOver evaluates the spot against whatever the
  pane then shows. `DropSpot.springs` says whether a spot opens (a row spot with a row, i.e. a
  folder row, or an inactive tab); a spot whose effect is none (a refused drop) never springs.
  Not spring-loaded: sidebar rows, crumbs, file rows, empty space, search tabs, This PC's tab, an
  archive level. Lines: `[dnd] spring armed: pane N row R (<folder>)` / `pane N tab I (<folder>)`,
  `spring-loaded: pane N opens <folder> after T ms` / `pane N shows tab I after T ms`, `spring
  disarmed (moved away | DragLeave | Drop)` for an armed spring that had not fired.
- **Drop descriptions** (`Description`, `description`, `description_block`, `DropTarget::describe`).
  Our target sets `DROPDESCRIPTION` ("Move to %1" / "Copy to %1" with the destination folder's name;
  `C:` for a drive root, the share's name for a UNC root) on the drag's data object on every
  DragEnter and DragOver whose (type, insert) differs from the last one set, `DROPIMAGE_NONE` with no
  text for a refused drop (the shell's own "not allowed" image), and `DROPIMAGE_INVALID` on
  DragLeave and Drop so the source's own text comes back. DragOver is not handed the data object, so
  DragEnter keeps a reference (released in DragLeave and Drop); the description is set before the
  target helper's call, which reads it. Our source asks `IDragSourceHelper2::SetFlags
  (DSH_ALLOWDROPDESCRIPTIONTEXT)` before every `InitializeFromBitmap` (the interface queried once
  per process and kept as the helper is). Lines: `drop description: Move to "<folder>" | Copy to
  "<folder>" | none | cleared` on change, `drop description: SetData failed 0x...` once per drag
  whose data object refuses SetData (the driver's own objects do, 0x80004001), and
  `IDragSourceHelper2::SetFlags(DSH_ALLOWDROPDESCRIPTIONTEXT) = 0x...` once per process.
- **Drops on a breadcrumb crumb** (`Spot::Crumb`, `Mark::Crumb`). A crumb's label is a drop target
  for the crumb's folder (This PC's crumb and a search tab's own crumb are none; an archive path is
  refused as drops into archives are), lit as a tab is (`CrumbButton.drop-here`, accent at 65% with
  a 2 px border; `drop-crumb-pane` / `drop-crumb` on `App`, set by `drop_highlight` on change). The
  crumbs' places come from Slint: `CrumbButton` reports its x in the crumb row and its label's width
  on `init` and on `changed x` / `changed width`, and the crumb row reports its offset in the bar
  (the right-alignment) and the bar's width on `changed x` / `changed width`, to `main.rs`
  `crumb_geometry` (a thread-local `CRUMB_GEOM`, outside the app state because Slint reports from its
  instantiation and change-handler passes). `dnd::Strips` carries them; `hit_all` checks crumbs after
  the tab strip and before the lists; constants `CRUMB_PAD` (3) and `CRUMB_H` (26) with `crumb_top`
  from pane.slint's layout. 16 reports in the 15 s gate scenario (4 crumbs plus the row at start, 3
  at the split, 4 for pane 1), none while the lists scroll; one `[crumb] pane=N geometry: K crumbs`
  line per breadcrumb rebuild.
- **Auto-scroll of the sidebar list and the tab strip** (`Edge`, `edge_scroll_all`, `scroll_step`;
  `main.rs` `drop_autoscroll`). Within 24 px of the sidebar list's top or bottom a hovering drag
  scrolls it by one 24 px row per 50 ms, bounded by the ListView's own content height
  (`side-content-height()` / `side-visible-height()` on `App`: its estimate for rows of mixed heights
  sits a px or two off their sum and is what the scroll can reach); within 24 px of either end of a
  tab strip whose tabs overflow it, by one tab per 50 ms (`set-tab-scroll`). Lines: `auto-scroll
  sidebar up|down: content-y A -> B`, `auto-scroll pane N tab strip left|right: x A -> B`.
- **Mounted folders as volumes** (`volume_key`, `path_volume_key`, `VolumeCache`). For a local drive
  path `volume_key` is the mount point `GetVolumePathNameW` returns (lowercase, no trailing
  separator: `c:`, or `c:\mnt\data` for a volume mounted there); UNC paths and mapped network drives
  (`fs::watch::is_remote`) keep the string rule, so DragEnter cannot wait on a dead server, as does
  a failed call. The sources' key is looked up once per DragEnter and the destination's once per
  distinct folder (a one-entry cache in the target); `effect_for` takes `same_volume` from the
  target. Lines: `volumes: sources on <key>` per DragEnter with files, `volumes: dest <path> on
  <key>` per new destination.
- **The first drag's icon** (`icons::drag_key`, `main.rs` `prefetch_drag_icon`, `State.drag_icon`,
  `DragImage.prefetched`). A left or right press on a row sends the icon worker one request for the
  drag image's icon at the image's icon size (a `drag:` key: rendered for every press, never in the
  icon table, never in the `taken` set); the reply lands in a one-slot `drag_icon` (the newest press
  wins, another path's reply is dropped), and `run_pending_drag` drains the channel once and hands
  the buffer to `attach_drag_image`, whose order is now the tile's picture, the worker's icon, the
  shell, the pane's cache (`icon from the worker` in the drag image line). Not for a drive row nor
  for a grid tile already showing a real picture. Measured (`[dnd] drag image ... composed in`, the
  first drag of a fresh process, three runs each; the P2-M4 figure of 113 ms did not reproduce on
  this machine today): `click 0 1; dragrow 0 1 0 60` 19.9 / 10.5 / 11.5 ms from the shell before,
  1.7 / 1.8 / 1.9 ms from the worker after; with `sleep 300` between 11.8 / 10.2 / 11.3 before, 1.7
  / 1.7 / 4.8 after; a bare `dragrow` (its own press, 60 ms before the threshold) 11.2 / 14.4 / 11.8
  before, 2.6 / 1.7 (worker) / 28.9 (the shell fallback: the reply had not landed) after; on
  `C:\traces\bigdir`, interleaved, 8.4 / 8.6 / 9.1 before, 0.7 / 1.1 / 1.6 after.
- **Driver** (`auto.rs`): `dropfiles crumb <pane> <index> <mode> <paths>`; `dragover row <pane>
  <row>`, `tab <pane> <index>`, `crumb <pane> <index>` (the spot's centre, the spring-load's clock),
  `side <top|bottom>`, `strip <pane> <left|right>` beside the old `<pane> <top|bottom>` form.
- **Soak** `tools/soak-p5m3.ps1`: the P4-M3 soak plus, per cycle, a `dragover row 1 0 1300` that
  spring-loads a folder, a `dropfiles crumb 1 -2 copy` onto the parent crumb, a `dragover side
  bottom 300` and a `dragover tab 0 1 300` (under the spring time: armed, never switched).
- **Tests**, 171 to 176: `dnd`: `volume_cache_looks_each_destination_up_once`, `drop_descriptions`,
  `spring_loaded_folders`, `hit_breadcrumb_crumbs`, `auto_scroll_sidebar_and_tab_strip`; `volumes`
  extended with real `GetVolumePathNameW` keys (a folder, its root, a path that does not exist and
  a `\\?\` path all give the drive's key; a UNC path keeps the share) and
  `shell_data_object_offers_hdrop` with the 1044-byte description stored on the shell's data object
  and read back; the existing `effect_for` tests go through the string rule.
- **Docs**: `CLAUDE.md` (test count, the driver forms and lines, the soak, the reference build
  `target-p5m2`); `docs/cleanup.md` (`target-p4m3` deleted 2026-10-16 after serving as the reference
  for ab20; `target-p4m1` still on disk, superseded on 2026-10-15, due for deletion on Jeremy's
  word).

### Decisions

- **The spring is clocked by DragOver, not by a timer.** No Slint timer runs inside our own drag
  loop (P2-M4), and OLE calls DragOver about every 50 ms while the pointer rests, so a time
  comparison there is both the only clock available in every case and free of any per-frame work.
- **The open runs from the event loop** (`invoke_from_event_loop`), as the job of a drop does: the
  drop target holds no app-state borrow and opens no modal loop, and the closure runs inside our own
  drag loop (as every such closure has since P2-M4) or at the next turn for a foreign drag.
- **A spot fires once until the pointer leaves it.** After a row spring the listing changes and the
  spot falls to the pane (or lands on another row), which disarms it; after a tab spring the tab is
  active and no longer springs. Without the rule a drag resting on an unchanged spot would open it
  again every second.
- **Only where a highlight shows.** A spot the drop refuses (effect none: a source's own folder, a
  dragged folder itself) never arms, so the spring and the highlight agree.
- **Drive rows on This PC spring** (they are folders); crumbs and sidebar rows do not (the kickoff
  named rows, tiles and tabs).
- **The description is `DROPIMAGE_NONE` with no text for a refused drop**, not INVALID: INVALID tells
  the shell to fall back to the source's default text, which for a refused spot would read as if the
  drop were allowed. INVALID is used only when the drag leaves or drops.
- **`SetFlags` before every `InitializeFromBitmap`**: the documentation states no order; the samples
  set the flags first; one COM call per drag.
- **Crumb geometry comes from `changed` callbacks**, the brief's own design, kept in a thread-local
  rather than in `State`: Slint reports from its instantiation and change-handler passes, which a
  call that holds the state's borrow could set off. The generated code was checked: layout bindings
  read the instances through `instance_at` and never `ensure_updated`, and instances are made in
  `ensure_tree_instantiated` outside any binding, so a read of `x` in `init` or a change tracker's
  first read cannot recurse. A per-crumb `drop-here` binding is accepted: crumbs are few and outside
  the list (the P3-M1/P3-M5 amendment is about rows and tiles).
- **The sidebar's bound is the ListView's own content height**, not the rows' sum: the ListView's
  estimate for rows of mixed heights sits about 2 px off the sum, and a bound from the sum kept the
  scroll hitting the clamp (`-97 -> -99` on every tick); Slint nudges content-y by about 2 px as it
  corrects its estimate (`-48` then `-46`), which is its own behaviour and harmless.
- **`GetVolumePathNameW` only for local paths**, cached per drag and per destination: the call walks
  the mount-point table, and DragOver runs every 50 ms; a UNC path or mapped drive keeps the string
  rule so no DragEnter waits on a server (mount points lie on local volumes anyway).
- **The drag icon is asked of the worker at the press, not warmed at start**: a warm-up render on
  the UI thread at start could land inside the gate's measured window; the press path costs one
  channel send and the worker one 64 px render per press, which the gate scenario never makes. Taken
  in `run_pending_drag` (after one `drain_icons`), not in `drag_start`, so a reply that lands between
  the threshold and the posted message still counts; a `drag:` key is never cached because the next
  press may be another row's.

### Deviations

- **Right presses prefetch too** (`SelOp::Context` in `on_select`), so a right-button drag gets the
  worker's icon as well; a context-menu right-click costs the worker one render it does not use.
- **`effect_for` gained a parameter** (`same_volume`) instead of looking the volumes up itself, so
  the lookups could be cached in the target; the tests go through a small `eff` helper.
- **One repeated `over` line after a tab spring**: the tab's `springs` flips to false when it becomes
  active, so the (spot, effect) pair changes once more and logs once more. Accepted rather than
  special-casing the comparison.
- **The sidebar auto-scroll's bound** is the ListView's estimate (above), not the geometry the hit
  test uses.
- **The spring-load time is a constant**, 1000 ms, not a setting: the open question says the number
  is set at the console; a change is one constant.

### Deferred

- **Our own `GiveFeedback` still returns `DRAGDROP_S_USEDEFAULTCURSORS`**: with a description showing,
  Explorer's sources handle the cursor themselves ("IsShowingLayered"); ours may show the OLE cursor
  glyph beside the description's icon. A console check decides whether that is worth changing.
- **The description text as painted** (Explorer as the source over TwinFM, TwinFM as the source over
  Explorer and over itself) needs a person at the console: the driver's data objects refuse SetData
  (the unit test shows the shell's data object stores the block).
- **Spring-loading a sidebar row or a crumb** (not in the brief).
- **A spring-load into a folder that then fails to list** leaves the drag over an empty pane (the
  drop goes to the pane's folder, as any drop onto empty space does).
- **The pane-to-pane OLE drop stays unscripted** (P2-M4; a scripted drop once landed on another
  program's window).
- **`target-p4m1`** is still on disk (superseded 2026-10-15); deletion on Jeremy's word.

### Scripted checks

Harness session (console session active, 1216 x 800 window, scale 1.0, accessibility on), this
worktree's `target\release\twinfm.exe` from the commit under review, scratch folders under
`%TEMP%\twinfm-p5m3-manual\` and `%TEMP%\twinfm-p5m3-drag\` (no soak fence), stderr kept; exit code
0 and no `panicked` line throughout (run by the implementers, quoted from their logs).

- **(i) A row spring-load**, `dragover row 0 0 1300` over a folder row: `[dnd] spring armed: pane 0
  row 0 (...\a\sub)`, `[dnd] spring-loaded: pane 0 opens ...\a\sub after 1002 ms`, `[nav] pane=0
  history push ...\a (2 back)`, `[nav] pane=0 gen=5 path=...\a\sub`, `[enum] pane=0 done: 1
  entries`; the hover then armed on `sub\inner` and ended with `spring disarmed (DragLeave)`.
- **(ii) A tab spring-load**, `dragover tab 1 1 1300`: `spring armed: pane 1 tab 1 (...\c)`,
  `spring-loaded: pane 1 shows tab 1 after 1019 ms`, `[tab] pane=1 switched to 2 of 2 (id 3,
  ...\c)`; a 300 ms hover over the other tab: armed, then `disarmed (DragLeave)`, no switch.
- **(iii) The grid** (`TWINFM_CONFIG=C:\traces\grid.toml`, read-only): `[settings] file
  C:\traces\grid.toml (read)`, `[view] pane=0 starts as large icons`, the same `spring armed: pane 0
  row 0 (...\a\sub)` and `spring-loaded: pane 0 opens ...\a\sub after 1006 ms` on the tile.
- **(iv) A crumb drop**, `dropfiles crumb 1 -2 copy <file>`: `[dnd] over pane 1 crumb 8 (...\b) keys=0x8
  ... -> effect copy`, `Drop at pane 1 crumb 8 ...`, `[fileops] done id=1 pane=1 tab=2 Done: Copied 1
  item` into the parent folder. `dragover crumb 0 -2` gives `over pane 0 crumb 8 (...\a)`; a crumb
  inside `C:\traces\bigdir.zip` gives `[archive] refused: drop into C:\traces\bigdir.zip (writing
  into a zip archive is not supported)` and `over nowhere`.
- **(v) The tab strip's auto-scroll**, seven tabs in the right pane, `dragover strip 1 left 400`
  then `right`: `auto-scroll pane 1 tab strip left: x -341 -> -221`, `-221 -> -101`, `-101 -> 0`,
  then `right: 0 -> -120`, `-120 -> -240`, `-240 -> -341`.
- **(vi) The sidebar's auto-scroll**, 24 scripted bookmarks, `dragover side bottom` then `top`:
  `auto-scroll sidebar down: content-y 0 -> -24`, `-24 -> -48`, `-46 -> -70`, `-70 -> -94`, `-94 ->
  -97`, then it stops at the bound; `up: -97 -> -73` ... `-3 -> 0`.
- **(vii) Descriptions and volumes with the driver**, `dragover 0 bottom 400` then `dropfiles 0 -1
  auto` from another `%TEMP%` folder: `[dnd] volumes: sources on c:`, `[dnd] volumes: dest
  C:\...\twinfm-p5m3-drag on c:` (the second drag hit the cache and logged no dest line), `[dnd] drop
  description: SetData failed 0x80004001` once per drag (the driver's own data object), the move
  effect as before. The first real drag of a run logs `[dnd] IDragSourceHelper2::SetFlags
  (DSH_ALLOWDROPDESCRIPTIONTEXT) = 0x00000000`.
- **(viii) The soak**, `tools/soak-p5m3.ps1 -Minutes 2` on 5db406b: exit 0, 0 `panicked`, 3 cycles,
  63 jobs (0 failed, 0 cancelled), 0 fence refusals, 0 `waitops` timeouts; 22 drops, 4 drop menus (0
  busy), 0 drags dropped, grid on/off 4/4, bands 2/2; spring armed 8, spring-loaded rows 2 (two
  navigations into `spring\inner`), tabs 0, disarmed 6 (the existing `dropfiles tab 0 1` steps arm in
  DragEnter and disarm with `(Drop)`); crumb drops 2; sidebar and tab-strip auto-scroll 0 (neither
  overflows in the soak; counted, not required); working set 108.2 MB at 5 s, 180.4 MB at 121 s
  (peak 190.9), private 106.9 to 140.9 MB; the spring folder held only `inner` after the run.
- **(ix) The lists are the reference's.** The generated Rust of `target-p5m2` (6b1d0f1) and this
  build's (5db406b), normalised (a line break after each `;`, `{` and `}`, lines trimmed, `_<digits>`
  suffixes and the change-tracker, repeater and timer indices to `N`, quoted drive paths blanked),
  differ in 294 lines outside the list and grid components, all in `InnerPane_root`,
  `InnerCrumbButton_root` and `InnerApp` (the crumb highlight, the crumb-geometry callbacks and
  change trackers, `crumb_clip`, `drop_crumb`, the new `App` functions); the list and grid
  components' region (`InnerFileList_root_` to `InnerFileGridList_root_`, 6,138 normalised lines on
  each side) in 0.
- **(x) The crumb reports are not per frame**: the 15 s gate scenario on `C:\traces\bigdir` with a
  temporary line per report logged 16 reports (none while the lists scroll); without it, two
  `[crumb] pane=N geometry: 4 crumbs` lines.

### Measurements

In-app, RELATIVE only: harness session with Jeremy's console session active (frame interval p50
16.4 to 16.5 ms in every run: 60 Hz), 1216 x 800 window, scale 1.0, accessibility on, the gate
scenario on `C:\traces\bigdir`, `[busy]` over the 10 s after the split, each candidate run (5db406b,
this worktree's `target\release`) interleaved with `target-p5m2` (6b1d0f1, the reference) by the
same runner, two sets of six pairs: the first with the reference in the first slot of each pair, the
second with the candidate first, as the P4-M2 note asks before an in-app "candidate above" is read
as a cost. No cargo or agent ran beside.

| Pair | set 1: reference first | set 1: P5-M3 second | set 2: P5-M3 first | set 2: reference second |
|---|---|---|---|---|
| 1 | 17.3 | 20.2 | 20.3 | 23.4 |
| 2 | 19.1 | 20.5 | 16.4 | 18.4 |
| 3 | 18.6 | 19.2 | 16.2 | 19.8 |
| 4 | 18.6 | 17.7 | 18.3 | 15.5 |
| 5 | 19.8 | 20.3 | 18.9 | 19.1 |
| 6 | 18.0 | 17.0 | 18.3 | 18.9 |
| **median** | **18.6** | **19.7** | **18.3** | **19.0** |
| composes | 371 strips at 23.3 to 25.7 us | 371 at 23.2 to 25.0 us | 371 at 23.2 to 24.4 us | 371 at 22.1 to 26.8 us |

The second slot of a pair read above the first in nine of the twelve pairs whichever binary sat in
it (the candidate above in four of six when second, below in five of six when first), so the two
sets' medians cancel: over all twelve pairs the reference's median is 18.75 and the candidate's
18.6, inside the reference's own spread (15.5 to 23.4). What the scenario exercises of the branch
is the drop target's absence (no drag), the crumb geometry reports (two `[crumb] pane=N geometry: 4
crumbs` lines per candidate run, none while the lists scroll) and the per-crumb `drop-here`
binding, and the compose figures are the reference's to the microsecond: 371 strips per run in
every one of the 24 runs. Every run's `[watch]` lines are the two `watching` and two `parent watch
on` lines; no `[fileops]`, `[undo]`, `[dnd] spring` or `panicked` line. The console triplet decides.

### Gate

`tools/wpr-triplet.ps1 -Reference p5m2 -Candidate p5m3 -Tag ab21`, Jeremy elevated at the
console, accessibility on, Narrator off, no other TwinFM window: `target-p5m2` (6b1d0f1) against
`target-p5m3` (the PR head, built with `--locked` from the detached checkout
`.claude\worktrees\gate-p5m3`). The defaults triplet only (the kickoff: nothing in this milestone
is per row; the list and grid components are the reference's, (ix) above). Console checks after
the triplet: the drop descriptions with Explorer as the source over TwinFM and TwinFM as the source
over Explorer and over its own other pane (and whether the OLE cursor glyph shows beside the
description's icon, see Deferred); a drag held over a folder row, a tile and a tab for about a
second; a drop on a crumb; a drag resting at the sidebar's bottom edge with the sidebar
overflowing, and at a tab strip's end with many tabs; the first drag of a run starting without a
pause.

Run on 2026-10-16 (15:46 to 15:51), Jeremy elevated at the console, accessibility on, Narrator off,
no other TwinFM window. `target-p5m2` (6b1d0f1) against `target-p5m3` (074ccb2, the PR head, built with
`--locked` from the detached checkout `.claude\worktrees\gate-p5m3`). Every run's stderr shows the two
`watching` and two `parent watch on` lines, the candidate's two `[crumb] pane=N geometry: 4 crumbs`
lines, and no `[fileops]`, `[undo]`, `spring`, `drop description` or `panicked` line. Analysed with
`tools/analyze-trace.py --symbols --exe`; traces `C:\traces\twinfm-ab21-*.etl`, reports under
`C:\traces\export\`.

| Run | reference `target-p5m2`, WPR | P5-M3 `target-p5m3`, WPR | in-app (ref / cand) | peak working set (ref / cand) |
|---|---|---|---|---|
| 1 | 17.9% | 17.9% | 19.7 / 20.5 | 154 / 187 MB |
| 2 | 17.8% | 17.8% | 16.6 / 19.2 | 151 / 150 MB |
| 3 | 17.7% | 17.9% | 20.9 / 15.6 | 151 / 151 MB |
| **median** | **17.8%** | **17.9%** | 19.7 / 19.2 | |

**P5-M3 passes the gate**: every run of both binaries sits under the 20% bar, the runs interleave
(the candidate's 17.8 sits between the reference's 17.7 and 17.9), and the medians differ by 0.1.
Frame shares, reference runs 1 to 3 / candidate runs 1 to 3 (sampled ms in the window):
accessibility 199, 200, 182 / 196, 204, 202; render 1053, 1048, 1037 / 1044, 1043, 1040; draw_text
619, 621, 615 / 637, 588, 614; ListView 331, 323, 344 / 334, 311, 336; timers 634, 633, 628 / 656,
626, 645; heap 153, 181, 177 / 171, 175, 178. The candidate's reports hold no spring, describe,
volume or drag frame in the window (the scenario never drags); the crumb geometry's change trackers
appear only as the Slint runtime's own frames the reference has too. The first candidate run's peak
working set was 187 MB against 150 to 154 MB in the other five, the same pattern as ab20's first
candidate run (§ P5-M2 Gate); its busy figure was unremarkable; not explained. The in-app lines
(15.6 to 20.9) swing wider than the WPR figures and do not decide.

Console checks, 2026-10-16, Jeremy at the console with the `target-p5m3` build: in progress. First
finding: **a drag from File Explorer onto TwinFM shows the denied icon and does not drop**, while a
drag from TwinFM to File Explorer copies. Under investigation (below) before the merge.

## Open questions

Written 2026-10-15 at the kickoff; the Phase 4 close's open questions are absorbed here.

- **The cap on watched background tabs** (P5-M1): settled, 8 for the app, active tabs first
  (§ P5-M1 Decisions).
- **How the watched folder's own rename is noticed** (P5-M1): settled, a parent watch for the
  active tabs only (§ P5-M1 Decisions); whether the handles block a rename of an ancestor folder is
  a console check.
- **Undo of a delete to the Recycle Bin** (P5-M2): settled, the bin's "undelete" verb first, a
  move back from `$Recycle.Bin` only when the verb cannot be reached, fails or times out, never
  after a cancel or onto a taken name (§ P5-M2 Decisions). The verb restores synchronously at the
  real bin; both paths leave the `$I` metadata file on disk.
- **The spring-load delay** (P5-M3): 1000 ms as `dnd::SPRING_MS`, a constant until the console says
  otherwise (§ P5-M3 Decisions).
- **The Slint whole-row snap patch** (P3-M1 Deferred, P4 Open questions): still Jeremy's call;
  every view switch shows the snap. Nothing in Phase 5 touches the fork.
- **Upstreaming the two accessibility patches** (`docs/upstream-issue-draft.md`,
  `docs/patches/`): still deferred; Jeremy decides when. Do not post to slint-ui/slint from a
  session.
- **Carried from Phase 4, not in any milestone**: everything in `phase-4-plan.md` § Phase 4 close
  under Deferred (the views, the grid's accessibility, thumbnails, the band) and under the
  candidates not chosen (content search, archives, the compact menu, OneDrive marks, search
  polish, folder sizes, daily use). Accessibility is deferred for much later by Jeremy's word.
