# Phase 3: TwinFM finds and shows what Explorer can

Written 2026-10-13, when Phase 2 closed with P2-M1 to P2-M5 merged and the P2-M5 build at a
median of 20.4% on the gate under the triplet protocol (see `phase-2-plan.md` § Phase 2 close).
Phase 1 made TwinFM usable for a day instead of Files; Phase 2 made it trustworthy every day.
Phase 3's goal, accepted by Jeremy on 2026-10-13: **TwinFM finds and shows what Explorer can**,
without giving back the gate.

Five milestones, in the order Jeremy agreed on 2026-10-13 (the compact Windows 11 context menu
was cut from the phase; upstreaming the Slint patches stays Jeremy's decision, outside any
milestone):

1. **P3-M1, history and the settings dialog.** A back/forward history per tab (Alt+Left,
   Alt+Right, the mouse's Back and Forward buttons; Backspace and Alt+Up stay "up a folder"), and
   a settings dialog for what is today chosen only by editing `config.toml` (start folders, hidden
   files, the Type column, the sidebar, the path bar). Finishes two P2-M3 deferrals and gives the
   Type column its switch in case a later milestone needs its 3.4 gate points.
2. **P3-M2, search.** A name filter in the pane first (type-to-filter the current listing on the
   existing pass path, nothing per keystroke per row without a trace), then a recursive name
   search on a worker with results streamed into a tab, then content search only if wanted.
3. **P3-M3, folder sizes** in the status bar and the Size column for selected folders: a
   recursive size worker with cancellation, and the subtree watches P2-M1 deferred.
4. **P3-M4, archives.** Zip folders navigable as folders (the shell's zip namespace or a Rust
   reader; decide with a trace of enumeration cost), extraction through the existing file
   operation path, and the virtual-file drop path (`CFSTR_FILEDESCRIPTOR`) P2-M4 deferred.
5. **P3-M5, thumbnails and the large-icon view** (trace first). The first per-row rendering since
   M1: it starts with a trace of a 32 px per-row layout against the reference, and the strip
   pattern must carry it or the milestone stops at the trace.

**The gate is re-run after every milestone**, under the protocol agreed on 2026-10-12:
`tools/wpr-gate.ps1` elevated at the console by Jeremy, accessibility on, Narrator off, every
other TwinFM window closed, the split-and-scroll scenario on `C:\traces\bigdir`, on a clean binary
built into its own `--target-dir target-<name>` from a detached `git worktree add` checkout with
default features. Three runs per binary, interleaved (reference, candidate, reference, candidate,
reference, candidate), compared by median; a gap counts only when every reference run sits below
every candidate run or the medians differ by more than one point. The reference for the phase's
first milestone is `target-p2m5` (011430e, the P2-M5 PR head); after each merge the new
milestone's gate build becomes the next reference and the previous one is deleted. Bar: main
thread under 20% busy over the 10 s after the split, judged against the reference's spread in the
same session (single runs of one binary range about two points on a busy day, so no recorded
number from another session decides anything). A milestone is not closed until its table is
recorded here. When a milestone fails: stop, symbolize, name the frames, and either fix the
mechanism or report the options; never trade the bar off silently.

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
pane's strip (`icons::compose`), and every per-key cost is paid on a worker, once per key per
process. Anything that would put a per-row image or a per-row shell call back is a trace first,
not a feature.

## Open questions

- **Milestone order** was agreed on 2026-10-13 with P3-M1 first; Jeremy can still reorder or cut
  P3-M2 to P3-M5.
- **The Type column** stays on by default (3.4 gate points in M1). P3-M1 gives it a switch
  (`show_type`, default true); turning it off by default is a later decision, taken only if a
  milestone needs the headroom.
- **Thumbnails and the large-icon view** (P3-M5) are in the phase; they start with a trace of a
  32 px per-row layout against the reference before any UI is built.
- **Exposing OneDrive placeholders** (`PHCM_EXPOSE_PLACEHOLDERS`, P2-M5 Deferred) changes what
  every file API in the process reports; it needs its own check before it is turned on, and a
  decision on whether cloud status marks are wanted at all. Not part of any milestone yet.
- **Upstreaming the two Slint accessibility patches** (`docs/upstream-issue-draft.md`,
  `docs/patches/`): still deferred; Jeremy decides when.
- **Explorer's right-button drag never starts on this desktop**: outside TwinFM; worth one check
  without Logi Options+ running if Jeremy cares.
- **The compact Windows 11 context menu (`IExplorerCommand`)**: cut from Phase 3 on 2026-10-13;
  stays on the deferred list from M4.
