# Handoff: TwinFM Phase 2 kickoff, 2026-10-10

Written at the close of Phase 1 for the session that starts Phase 2. Nothing is in flight:
every Phase 1 branch is merged and `main` is clean. This document is the brief, in the shape
of the Phase 1 handoff that produced M1 to M6. Jeremy decides the Phase 2 scope; the
milestones below are the recommended order, not a mandate.

## Resume here

- Repo: twinfm (https://github.com/jtklinger/twinfm, private), local clone
  `C:\Users\jtkli\projects\twinfm`, branch `main`. Work on the SER5-DESK desktop, not in the
  cloud: the WPR gate needs the console and COM work needs a real shell. Ignore any Homelab
  checkout the session happens to start in.
- PR: none open. Phase 1 PRs #11 to #18 are all squash-merged.
- Verify state before continuing (command, then what was observed when this was written):
  - `git log --oneline -1` on main: `3f622dc ui: M6b sidebar with drives, places and bookmarks,
    settings in config.toml, window icon, About dialog (#18)`
  - `cargo test --release`: `test result: ok. 60 passed; 0 failed`
  - `cargo build --release`: `Finished`, no warnings
- If a check fails, main moved after this was written: read `git log` and
  `docs/phase-1-plan.md` before acting.

Read first, in this order: `CLAUDE.md`, `PROTOTYPE-RESULT.md`, `docs/week-one-plan.md`,
`docs/phase-1-plan.md` (1,367 lines; the architecture rules, every milestone's decisions and
deferrals, and the gate record are there). Then all of `src/` (about 9,600 lines of Rust and
Slint across `main.rs`, `model.rs`, `icons.rs`, `shellmenu.rs`, `dnd.rs`, `fileops/`, `tabs.rs`,
`pathbar.rs`, `statusbar.rs`, `sidebar.rs`, `settings.rs`, `fs/enumerate.rs`, `auto.rs`,
`perf.rs`, `ui/*.slint`).

## Goal

Phase 1 made TwinFM usable for a day instead of Files. Phase 2 makes it trustworthy for every
day: the list stays true to the disk without F5, file operations are visible and undoable
inside the app, and the rough edges found in daily use are closed. Performance must not
regress: the gate (main thread under 20% busy while two panes scroll a 40k folder,
accessibility on, by WPR at the console) is re-run after every milestone, and headroom is
0.2 point.

## State

Done and verified (all on main):

- M1 entry model: Win32 attributes, `Kind`, Type column, Ctrl+H hidden and system filter,
  multi-selection, F5 refresh restoring cursor, selection and scroll by name, This PC drive
  list, syscall-free local-time formatting (40k enumeration 55 to 75 ms on the worker).
- M2 icons: one MTA COM worker renders shell icons by key; the UI thread resolves the visible
  range only and draws ONE composed strip image per pane (a per-row `Image` failed the gate).
- M3 file operations: `IFileOperation` on an STA worker with a real message pump, Explorer's
  dialogs owned by the main window, Recycle Bin by default, rename on collision, Shift+F5 and
  Shift+F6 to the other pane, F2 and F7 inline editors, Ctrl+C/X/V through `CF_HDROP`.
- M4 shell context menu with `IContextMenu3` routing through a temporary window subclass,
  TwinFM's own items above a separator, Shift+F10 and the Menu key.
- M5 OLE drag and drop both ways, optimized-move protocol as a target, press-and-move drag
  source, Escape cancels, release after a drag suppressed.
- M6 tabs per pane, breadcrumb path bar with Ctrl+L entry, status bar with free space, open
  files via `ShellExecuteExW`, collapsible sidebar (drives, places, bookmarks), TOML settings,
  window title and placeholder icon, About dialog with `AboutSlint`.
- Soaks: M3 and M5 ran 30 minutes each with zero panics and no memory trend.
- Gate record (console WPR, same conditions): week-one main 16.5%, M1 19.9%, M2 per-row
  22.3% (failed, reworked), M2 strip 19.3%, M3 19.6%, full M6 19.8%.

Half-done or unverified (recorded in `docs/phase-1-plan.md` under each milestone):

- Manual checks only Jeremy can do at the console: real Explorer drags both ways and a
  pane-to-pane drag (M5 § Manual check), opening files, the bookmark menu and sidebar
  middle-click (M6), Narrator on selection, the icon strip, the editors and the sidebar.
- The GitHub repo description still says "One-week prototype" (repo settings, Jeremy).
- `tools/analyze-trace.py --symbols` misses twinfm frames unless the build's own
  `target-*\release` directory is on `_NT_SYMBOL_PATH` (the exe's embedded pdb path is not
  searched when that variable is set). Phase 1 worked around it by hand; the script should add
  the exe's directory itself.

## Numbers to protect

| Measure | Value | Where it comes from |
|---|---|---|
| Main thread busy, 10 s after split, two panes scrolling, accessibility on | 19.8% (bar 20%) | `tools/wpr-gate.ps1` at the console, `tools/analyze-trace.py` |
| 40k enumeration on the worker | 55 to 75 ms | `[enum]` stderr line |
| Second pane rendered after F3 | 27 to 34 ms | `[split]` stderr line |
| Icon strip compose | about 24 us per strip | `[icons]` stderr line |
| 30-minute soak | 0 panics, no memory trend | `tools/soak-m5.ps1 -Minutes 30` |

The gate headroom is 0.2 point. Anything that runs per frame (a `changed` handler on a
property that moves during scrolling, a per-row item or property, a per-frame Rust callback)
needs a trace before it is proposed. The Type column alone cost 3.4 points in M1; a fifth
`Text` per row is the kind of thing that cannot be added again without buying headroom first.

## Architecture rules (carry forward, enforce in review)

- The UI thread touches the Slint models; nothing else does. Workers wake it with
  `slint::invoke_from_event_loop`. `row_data` stays a plain clone.
- Plain-struct entries, batched enumeration, one `row_added` per batch, filter and sort on a
  worker over a snapshot, whole-Vec swap, display text formatted on the worker.
- Every COM object lives on the thread that created it: icons on the MTA worker,
  `IFileOperation` on the STA worker, `IContextMenu`, `DoDragDrop` and `IDropTarget` on the UI
  thread.
- Modal-loop rule: no app-state `RefCell` borrow may be held across `TrackPopupMenuEx`,
  `InvokeCommand` or `DoDragDrop`. Take what is needed out first. Phase 1 found that winit 0.30
  buffers every Slint event (timers, paint, `invoke_from_event_loop`) while such a loop runs
  inside its handler, so today nothing re-enters; the rule stays because a shell extension
  that pumps messages, or a winit change, would make re-entry possible again.
- No `unwrap` or `expect` on `Weak<App>`; `upgrade()` with an early return.
- `windows-sys` with hand-written vtables, as every module does; the `windows` crate only with
  a reason in the commit. New crates only where the milestone names them.
- Pixel-buffer `slint::Image`s carry no cache key, so femtovg uploads a texture per item per
  change; never put one on a per-row item. Compose into one image instead (the strip pattern).
- OLE's drag loop runs neither Slint nor Win32 timers; scripted drags are kept turning by a
  helper thread posting `WM_MOUSEMOVE`, and a scripted drop is never attempted (one landed on
  another program's window in the RDP session).
- Scripted runs (`TWINFM_SCRIPT`) ignore `config.toml`; `TWINFM_SOAK=1` enables no-confirmation
  and fences every operation to `%TEMP%\twinfm-m3` and `%TEMP%\twinfm-m5`.

## Proposed milestones, in order

Each is one or more PRs, squash-merged on Jeremy's go-ahead, and ends with the gate.
SOAK marks a 30-minute scripted soak as well.

### P2-M1. Directory watching (SOAK)

The gap most other deferrals share: late-finishing shell verbs, Explorer's asynchronous moves
after a drag out, other programs, and background tabs. `ReadDirectoryChangesW` on a dedicated
worker, one watch per visible tab's folder (background tabs stay stale and refresh when
shown, as now), coalesced into one refresh per 250 ms, delivered as the existing refresh path
so cursor, selection and scroll survive. Network shares and removable drives fail
gracefully (no watch, F5 still works). `WM_DEVICECHANGE` refreshes the drive lists. Tests on
a scratch tree; soak with a writer process churning the watched folder.

### P2-M2. Operations inside TwinFM

Read back the real new name after rename and new folder (`IFileOperationProgressSink` or
`PostNewItem`), move the cursor to the next row after a delete or move, scroll the new row
into view, clear cut marks when another program takes the clipboard
(`AddClipboardFormatListener`), Undo with Ctrl+Z through the shell's undo record where it
works and a TwinFM-side record where it does not. A visible operation queue with cancel is
the stretch item; the shell's own progress window remains the default.

### P2-M3. Daily-use polish from the deferred lists

Pick from `docs/phase-1-plan.md` § Phase 1 close after a week of use, in the order the gaps
annoy: tab overflow and reorder, restoring tabs at start, middle-click a folder into a tab,
breadcrumb sibling dropdown, folder sizes in the status bar, maximised state, bookmark
reorder and rename, a settings dialog, Explorer's protected-system-files switch, a designed
icon, Windows 11's compact context menu (`IExplorerCommand`).

### P2-M4. Drag and drop finish (SOAK)

Drag images through `IDragSourceHelper` and `IDropTargetHelper`, highlight during our own drag
(needs a repaint path that works inside `DoDragDrop`; measure before designing), right-button
drag menu, auto-scroll near the list edges, the sidebar and tabs as drop targets.

### P2-M5. Icons and views

Overlays (shortcut arrows), telling reparse tags apart (OneDrive placeholders versus
junctions), a timeout or second worker for slow icons, and a large-icon view with 32 px icons
only if Jeremy wants one; it is the first feature since M1 that would add per-row rendering
work, so it starts with a trace.

Explicitly not in Phase 2 unless Jeremy says otherwise: search, archive browsing, image
thumbnails, network share browsing beyond `read_dir`, MSIX packaging, winget, auto-update,
themes, upstream posting of the Slint patches.

## Gates and measurement

- `cargo test --release` (60 now) and `cargo build --release` with no warnings before every
  commit. CI runs both on windows-latest. The runner's `%TEMP%` is an 8.3 path (`RUNNER~1`):
  compare canonical paths in any test that reads paths back from the shell.
- After every milestone: `tools/wpr-gate.ps1` run by Jeremy from his own elevated console
  window, never the harness's Run button (presentation mode differs), on a clean binary built
  into its own `--target-dir target-<name>` from a detached `git worktree add` checkout, so an
  agent's builds in `target` do not confound it. Analyse with
  `python tools/analyze-trace.py <etl> --symbols` with the build's `target-<name>\release` on
  `_NT_SYMBOL_PATH` (or fix the script first, see State). Record WPR busy, in-app busy, Narrator
  state and the top frames in the PR and the plan doc. In-app `[busy]` in an RDP session reads
  about 3 points lower than console WPR and is relative only.
- When a milestone fails the gate: stop, symbolize, name the frames, and either fix the
  mechanism or report the options; never trade the bar off silently. The M2 rework in
  `docs/phase-1-plan.md` § M2 § Rework is the worked example.
- Soaks: `tools/soak-m5.ps1 -Minutes 30` is the template; extend it per milestone. A soak
  overwrites the clipboard and fills the Recycle Bin with scratch; say so before starting one.
  A running soak locks `target\release\twinfm.exe`, so parallel work builds elsewhere.
- Keep `docs/phase-2-plan.md` as the running record in the style of `docs/phase-1-plan.md`.

## Rules

- Branch, PR, squash-merge on Jeremy's go-ahead (the permission classifier blocks the session
  from merging on its own; `gh api -X PUT .../merge` with `merge_method=squash` is what works
  once he says so, followed by deleting the remote branch).
- Stacked branches are fine while a PR waits, but after each squash merge the next branch
  needs `git merge origin/main`; the conflicts are the squash meeting identical content and
  resolve with `git checkout --ours`, EXCEPT that any commit added to the previous branch after
  the next one was cut must be re-applied by hand. Phase 1 lost a CI test fix, a soak
  paragraph and two gate notes this way and had to restore them. Check `git log
  <prev-branch> ^<this-branch>` before resolving.
- Never force-push or rewrite history without asking. Never `git add -A` with a target
  directory present. No em dashes in code comments, docs or commit messages. Single-line
  commit subjects with scope prefixes.
- Opus agents for milestone implementation; the lead reviews the diff, runs the gates and
  pushes. Agents must build into a separate `--target-dir` while a soak or a gate binary is in
  use, and must never run file operations outside the soak fence.
- Do not post to slint-ui/slint or files-community/Files. Do not touch the Homelab repo or any
  lab host. No system changes on the desktop beyond what is installed; ask before installing.
- Python scripts that edit repo files go through a file written with the Write tool, not a
  shell heredoc: the shell mangled backslashes once and the default encoding is cp1252.

## Open questions / decisions pending

- Phase 2 scope: confirm or reorder the milestones above. Directory watching first is the
  recommendation.
- Whether the Type column stays on by default (3.4 gate points) or becomes a setting, if
  Phase 2 needs per-frame headroom.
- Whether a large-icon view is wanted at all.
- When to post the two Slint accessibility patches upstream (`docs/upstream-issue-draft.md`,
  `docs/patches/`); still deferred.

## Gotchas hit this session

See Architecture rules and Rules above; every one of them was learned the hard way in Phase 1
and is also recorded with its measurements in `docs/phase-1-plan.md`.

## Cleanup

This is a kickoff brief, not an in-flight resume note. Delete it when `docs/phase-2-plan.md`
exists and has absorbed the open questions, in the PR that creates that file.
