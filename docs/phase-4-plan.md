# Phase 4: TwinFM shows a folder as large icons

Written 2026-10-14, when Phase 3 closed with P3-M1 to P3-M5 merged and the P3-M5 build at
medians of 18.2% (defaults) and 13.8% (thumbnails on) on the gate under the triplet protocol (see
`phase-3-plan.md` § Phase 3 close). Phase 1 made TwinFM usable for a day instead of Files, Phase 2
made it trustworthy every day, Phase 3 made it find and show what Explorer can. Phase 4's goal,
chosen by Jeremy on 2026-10-14 from the Phase 3 close's candidates: **TwinFM shows a folder as
large icons, with thumbnails**, without giving back the gate.

The starting point is the P3-M5 trace (`phase-3-plan.md` § P3-M5 Trace): variant (b), branch
`trace/p3-m5-b-grid` (075f85a, on origin, never merged; its gate build is `target-p3m5b`), a
large-icon grid under the strip pattern that held 13.2 against the reference's 17.4 (ab14), with
about a third of its busy on its strip. It was built on 085be4d, before thumbnails landed, as the
ONLY list in its build; it left the drop highlight, `dnd::hit`, the rename editor, the shell menu
point and the driver at 22 px rows, and nothing in it is a view switch. Today's main (2a9894b)
chooses one of four list components per pane, once, by `show_type` x `thumbnails`; the grid becomes
further choices made the same way, never per row.

Three milestones, in the order Jeremy agreed on 2026-10-14 (his words: do not fold them; push
forward quickly but efficiently):

1. **P4-M1, the grid as a real view.** The trace carried onto main behind a view switch chosen
   once per pane (a `view` setting per pane, saved; Ctrl+Shift+1 for details and Ctrl+Shift+2 for
   large icons, as Explorer's view keys), with every list component's choice still made outside the
   `for`. Everything the trace left at 22 px rows moves to tile geometry: hit testing (`dnd::hit`),
   the drop highlight, the rename editor, the shell menu point, keyboard movement by tile, and the
   driver's `click`, `rclick` and drop steps. The strip's two known costs are trimmed first, since
   they were about a third of the grid's busy in ab14: the recompose rate (`GRID_MARGIN_ROWS = 1`)
   and the texture upload per compose (a pixel-buffer `Image` has no cache key). Gate: the
   defaults triplet (the details list is the reference's row and must not move) and a grid-on
   triplet with a read-only settings file, as P3-M5's thumbnails-on triplet ran.
2. **P4-M2, thumbnails in the tiles.** 96 px thumbnails through the P3-M5 path
   (`IThumbnailCache::GetThumbnail`, the per-size table, the drop range of rows scrolled past),
   with the `thumbnails` setting governing the grid as it governs the list, scale changes, and the
   two things the trace did not investigate: the thin green fringe on the shell's 96 px folder
   icon and the 16 px placeholders stretched before the first icon lands. Gate: defaults, grid on,
   and grid with thumbnails on.
3. **P4-M3, selection and drops by tile.** Shift and Ctrl selection across tiles and rows of
   tiles, rubber-band selection if the details list has none, drag out of the grid with a
   tile-sized drag image, drops onto tiles and onto empty space, auto-scroll at the grid's edges,
   and a grid cycle in the soak scripts. Gate: defaults and grid on.

**The gate is re-run after every milestone**, under the protocol agreed on 2026-10-12 and
unchanged since: `tools/wpr-triplet.ps1` elevated at the console by Jeremy, accessibility on,
Narrator off, every other TwinFM window closed, the split-and-scroll scenario on
`C:\traces\bigdir`, on clean binaries built with `--locked` into their own `--target-dir
target-<name>` from detached `git worktree add` checkouts with default features. Three runs per
binary, interleaved (reference, candidate, reference, candidate, reference, candidate), compared by
median; a gap counts only when every reference run sits below every candidate run or the medians
differ by more than one point. The reference for the phase's first milestone is `target-p3m5`
(ee0be21, P3-M5's gate build); after each merge the new milestone's gate build becomes the next
reference and the previous one is deleted (`target-p3m4` goes once `target-p3m5` has served).
Tags continue from ab17. Bar: main thread under 20% busy over the 10 s after the split, judged
against the reference's spread in the same session, never against a recorded number. Console
checks run after the triplet, never before. A milestone is not closed until its table is recorded
here. When a milestone fails: stop, symbolize, name the frames, and either fix the mechanism or
report the options; never trade the bar off silently.

**Settings triplets are part of the protocol in this phase** (agreed 2026-10-14): a candidate
may run with a read-only settings file (`-Candidate <name>@<toml>`), and every Phase 4 milestone
runs the triplets its entry above names, each against the same reference with the same file. The
files live beside `C:\traces\thumbs.toml` (`thumbnails = true`): `C:\traces\grid.toml` (the grid
on in both panes) and `C:\traces\grid-thumbs.toml` (the grid with thumbnails on), each made
read-only once written, each candidate's stderr checked for `[settings] file <toml> (read)`
before its triplet is read (the P3-M5 `-Config` slip). The defaults triplet decides whether the
details list has moved; a settings triplet decides whether the grid holds the bar.

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

Each milestone runs as in Phase 3: implementer brief, lead review, gates, push, PR, a gate build
from a detached checkout with `--locked`, `tools/wpr-triplet.ps1 -Reference p3m5 -Candidate
<name>[,<name>@<toml>] -Tag <next>`, console checks after the triplet, merge on Jeremy's word,
`target\release` rebuilt from main and said so.

## Open questions

Written 2026-10-14 at the kickoff; the Phase 3 close's open questions are absorbed here.

- **The Slint whole-row snap patch** (P3-M1 Deferred): Slint snaps a `ListView` rebuilt while
  scrolled to a whole row; the one-line fork patch is proven (`target-zcfix`). P4-M1's view switch
  rebuilds the list, so the snap shows on every switch. Jeremy takes the patch when the fork is
  next touched; it is a third fork commit with its own triplet (`CLAUDE.md` § Build variants for
  the fork procedure). Whether P4-M1 is that moment is Jeremy's call once the switch is seen.
- **Upstreaming the two accessibility patches** (`docs/upstream-issue-draft.md`,
  `docs/patches/`): still deferred; Jeremy decides when. Do not post to slint-ui/slint from a
  session.
- **The trace binaries** `target-p3m5a` (9598f79) and `target-p3m5b` (075f85a): `target-p3m5b` is
  the grid's in-session baseline for P4-M1's first comparisons and stays until P4-M1 is merged;
  `target-p3m5a` has no further use and can go on Jeremy's word. Both rebuild from their branches
  on origin.
- **Explorer's view per folder**: Explorer remembers a view per folder; Phase 4's `view` is per
  pane, saved like the Type column, and nothing per folder. Revisit only if asked.
- **Thumbnails for video and Office documents** have no fixture on this machine (P3-M5 Deferred):
  the grid shows whatever the shell's thumbnail cache returns; a person at the console checks it
  when P4-M2 lands.
- **Carried from Phase 3, not in any milestone**: content search (out for now, 2026-10-13); the
  compact Windows 11 context menu; OneDrive status marks through `PHCM_EXPOSE_PLACEHOLDERS`; the
  rest of archives; search polish; the folder sizes' unexercised paths; the Phase 2 lists
  (directory watching, Redo and progress inside TwinFM, spring-loaded folders and drop
  descriptions). The full list with origins is `phase-3-plan.md` § Phase 3 close.
