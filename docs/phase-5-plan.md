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

## Open questions

Written 2026-10-15 at the kickoff; the Phase 4 close's open questions are absorbed here.

- **The cap on watched background tabs** (P5-M1): a handle per tab costs a 64 KiB buffer and a
  port association; the cap (per pane, or for the app) is the implementer's number with its
  reason, to be recorded in the milestone's Decisions.
- **How the watched folder's own rename is noticed** (P5-M1): a parent watch per pane doubles the
  handles; the alternative is the error on the next refresh, which is what happens today. The
  trace and the console decide.
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
