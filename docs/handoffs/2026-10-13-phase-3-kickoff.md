# Handoff: TwinFM Phase 3 kickoff, 2026-10-13

Written after the P2-M5 merge closed Phase 2 (2026-10-13). Nothing is in flight: every Phase 2
milestone is squash-merged, `main` is clean, the plan doc holds the Phase 2 close. This is a
kickoff brief for Phase 3, in the shape of the P2-M5 kickoff that produced that milestone. The
scope and order of Phase 3 are Jeremy's decisions before any milestone starts (Open questions).

## Resume here

- Repo: twinfm (https://github.com/jtklinger/twinfm, private), local clone
  `C:\Users\jtkli\projects\twinfm`, branch `main`. Work on the SER5-DESK desktop (the WPR gate
  needs the console, COM work needs a real shell).
- PR: none open once the PR that carries this file is merged.
- Verify state before continuing (command, then what was observed when this was written):
  - `git log --oneline -1` on main: `83cc93b icons: P2-M5 reparse kinds, shell overlays, two
    icon workers, FindFirstFileExW listing (#27)` (or the docs commit that merged this file on
    top of it)
  - `cargo test --release`: `test result: ok. 95 passed; 0 failed`
  - `cargo build --release`: `Finished`, no warnings
  - `target\release\twinfm.exe` built 2026-10-13 10:09 from 83cc93b (Jeremy's everyday binary)
  - `target-p2m5\release\twinfm.exe` built 2026-10-12 17:41 from 011430e, the Phase 3 reference
    binary for the gate; every older `target-*` directory was deleted on 2026-10-13
- If a check fails, main moved after this was written: read `git log` and
  `docs/phase-2-plan.md` § Phase 2 close before acting.

Read first, in this order: `CLAUDE.md`; `docs/phase-2-plan.md`, the intro (architecture rules,
the P2-M4 amendment on re-entrancy inside our own drag loop) and § Phase 2 close (every deferral
deduplicated, the Phase 3 candidates, the gate protocol); then the Phase 1 and Phase 2 records
for whichever area the first milestone touches (`phase-1-plan.md` § M2 and `phase-2-plan.md`
§ P2-M5 for anything near icons or the list; § M6a and § P2-M3 for tabs, path bar and history;
§ M5 and § P2-M4 for anything that drops or drags).

## Goal

Phase 1 made TwinFM usable for a day instead of Files; Phase 2 made it trustworthy every day.
Phase 3's goal, proposed for Jeremy to confirm or change: **TwinFM finds and shows what Explorer
can**, without giving back the gate (main thread under 20% busy while two panes scroll a 40k
folder, accessibility on, judged by interleaved triplets against `target-p2m5`).

The candidates from the Phase 2 close, in the lead's recommended order (Jeremy can reorder or
cut; none is started until he has):

1. **P3-M1, history and the settings dialog.** A back/forward history per tab (the mouse's
   Forward button, Back as history back, Alt+Left/Right, Backspace stays "up"), and a settings
   dialog for what is today only in `config.toml` (start folders, hidden files, the Type column,
   the sidebar). Small, finishes two P2-M3 deferrals, and gives the Type column its switch in
   case a later milestone needs the 3.4 gate points.
2. **P3-M2, search.** A name filter in the pane first (type-to-filter the current listing on the
   existing pass path, nothing per keystroke per row without a trace), then a recursive name
   search on a worker with results streamed into a tab, then content search only if wanted.
3. **P3-M3, folder sizes** in the status bar and the Size column for selected folders: a
   recursive size worker with cancellation, and the subtree watches P2-M1 deferred.
4. **P3-M4, archives.** Zip folders navigable as folders (the shell's zip namespace or a Rust
   reader; decide with a trace of enumeration cost), extraction through the existing file
   operation path, and the virtual-file drop path (`CFSTR_FILEDESCRIPTOR`) P2-M4 deferred.
5. **P3-M5, thumbnails and the large-icon view** (trace first). The first per-row rendering since
   M1: the strip pattern must carry it or the milestone stops at the trace. Jeremy declined the
   large-icon view for Phase 2 on 2026-10-12; it is here only because thumbnails share its work.
6. **Across the phase, Jeremy's call**: the compact Windows 11 context menu (`IExplorerCommand`),
   and upstreaming the two Slint accessibility patches (`docs/upstream-issue-draft.md`,
   `docs/patches/`), which is not a session's decision.

## State

Done and verified, all on main:

- Phase 2 closed: P2-M1 (#21), P2-M2 (#23), P2-M3 (#24), P2-M4 (#25), P2-M5 (#27), each with its
  gate record; the Phase 2 close in `docs/phase-2-plan.md`.
- P2-M5 (2026-10-12 to 13): the listing uses `FindFirstFileExW` and tells reparse tags apart
  (junction, symbolic link, cloud placeholder, app execution alias); shortcuts, junctions and
  links carry the shell's overlay, composed on the worker; two icon worker threads share one
  newest-first queue; gate median 20.4 against the P2-M4 reference's 21.0, the first milestone
  judged by interleaved triplets.
- Tooling: `tools/analyze-trace.py` finds xperf, takes `--exe`, tolerates MIN/MAX start and end
  times and picks the process that started inside the trace.
- Tests: 95. The symbolic-link assertions in `fs::enumerate::tests::tells_reparse_points_apart`
  skip on this machine (no symlink privilege, no Developer Mode) and say so.

Half-done or unverified: none in code. On disk: the worktree
`.claude\worktrees\p2-m5-kickoff-resume-9be1b5`, re-created for the docs branch that carries
this file; remove it after that PR merges (`git worktree remove <path> && git worktree prune`
from the main checkout, once the session that wrote this is closed).

## Next steps (ordered, concrete)

1. Merge the PR carrying this file (docs only), `git checkout main && git pull`, remove the
   worktree above.
2. Get Jeremy's decisions (Open questions): the Phase 3 goal and order, and whether the
   first milestone is P3-M1 as proposed. Do not start a milestone before that.
3. Write `docs/phase-3-plan.md`: the intro in the shape of `phase-2-plan.md`'s (the goal, the
   agreed milestones, the gate paragraph with the triplet protocol and `target-p2m5` as the
   reference, the architecture rules with both amendments carried over), then one section per
   milestone as they land. Commit it with the first milestone's PR.
4. Branch `feat/p3-m1-<slug>` in a worktree from `origin/main`; brief an Opus implementer the
   way P2-M1 to P2-M5 were briefed (the briefs are reconstructible from each record's "What
   landed" section; the P2-M5 brief's shape: read-first list, the architecture rules verbatim,
   numbered scope items each with mechanism, logging, tests and a measurement, the driver and
   scripted-check expectations, the gates, the record to write, and what to report back).
5. Lead reviews the diff against the architecture rules, runs the gates, pushes, opens the PR,
   builds a gate binary into `target-p3m1` from a detached checkout
   (`git worktree add --detach .claude/worktrees/gate-p3m1 <sha>`, then `cargo build --release
   --target-dir C:\Users\jtkli\projects\twinfm\target-p3m1` from it, about 3.5 minutes), and
   hands Jeremy the six gate commands (reference, candidate, three times, `-Etl
   C:\traces\twinfm-ab8-<build>-<n>.etl`) and the console checks. Analyse with `python
   tools/analyze-trace.py <etl> --symbols --exe <that build's exe>`; judge by the median rule;
   record the table in the plan doc.
6. Merge on Jeremy's go-ahead (`gh pr merge --squash --delete-branch` worked on 2026-10-13; it
   cannot remove the worktree the session stands in, which is harmless), pull main, rebuild
   `target\release` and say so, remove the gate worktree, delete the previous reference
   `target-*` once the new one has served as a reference.

## Open questions / decisions pending

- **Phase 3 goal and order**: the proposal above is the lead's; Jeremy decides. In particular
  whether search comes before history and settings, and whether archives are wanted at all.
- **Thumbnails and the large-icon view**: in or out of Phase 3. If in, they start with a trace of
  a 32 px per-row layout against `target-p2m5` before any UI is built.
- **The Type column** stays on by default (3.4 gate points) unless a milestone needs headroom;
  P3-M1's settings dialog would give it a switch either way.
- **Exposing OneDrive placeholders** (`PHCM_EXPOSE_PLACEHOLDERS`, P2-M5 Deferred): changes what
  every file API in the process reports; needs its own check before it is turned on, and a
  decision on whether cloud status marks are wanted at all.
- **Upstreaming the two Slint accessibility patches**: still deferred; Jeremy decides when.
- **Explorer's right-button drag never starts on this desktop**: outside TwinFM; worth one check
  without Logi Options+ running if Jeremy cares.

## Gotchas hit this session

- **The gate needs six runs now, not two.** Single runs of one binary ranged 19.6 to 22.8 on
  2026-10-12; the triplet medians on 2026-10-13 were 21.0 and 20.4 with every pair's frame shares
  within a few samples. Never judge on one pair; never compare against a number from another
  session; the whole machine drifts by about half a point between sessions.
- **`gh pr merge --squash --delete-branch` works from the worktree** (the P2-M5 handoff said it
  was blocked); it fails only at removing the worktree it runs in, and the merge still goes
  through. Check `gh pr view --json state` afterwards rather than trusting the exit text.
- **CI may still be pending on a docs-only commit when the merge is asked for**; the code head's
  run is what matters, but say so in the report.
- **A session cannot delete its own worktree folder**, and the harness refuses writes to the base
  checkout from a worktree session: for a docs-only follow-up, re-create the worktree at the
  session's own path on the new branch (`git worktree add <same path> <branch>`) and write there.
- **A Python heredoc that holds Windows paths needs raw strings**: `"C:\Users"` inside a normal
  triple-quoted string is a `\U` escape and the script dies before touching the file. Write the
  text to a scratch file and read it in, or use `r"""`.
- **`du -sh` over the `target-*` directories takes longer than the two-minute tool timeout**;
  delete them without measuring, or measure one.
- **Files inside a OneDrive sync root are not reparse points to TwinFM**: Windows disguises
  placeholders unless the process opts in. Only the sync root shows as cloud. A test for cloud
  placeholders can only use the raw tags.
- **No symlink privilege on this machine**: `std::os::windows::fs::symlink_file` fails; tests
  must skip the symlink assertions with a note, as the P2-M5 test does. Junctions (`mklink /J`)
  need no privilege.
- **`IImageList` vtable slots**: `GetIcon` is 10 and `GetOverlayImage` 31 counting IUnknown's
  three; the `windows` crate's generated `IImageList_Vtbl` in the cargo registry is the
  reference on this machine (no SDK headers installed).
- **Scripted checks that call a callback directly prove nothing about real input** (the P2-M2
  Escape regression). Demand one step through Slint's real key or pointer routing and a negative
  control; real OS input can be posted to the window with `PostMessageW`.
- **The everyday binary is not rebuilt by a merge.** Rebuild `target\release` after every merge
  and say so.
- **Paths handed to `twinfm.exe` from Git Bash must be Windows-style**; **PowerShell's `2>`
  writes UTF-16 with wrapped lines** (the gate script's `.stderr.txt` files are plain UTF-8,
  `grep -a` reads them).
- **Close every TwinFM window before a gate run**, and **tell Jeremy the keystrokes a check
  needs** (Ctrl+T, Ctrl+D, F3, Ctrl+H).

## Cleanup

Delete this file in the PR that opens P3-M1 (or that writes `docs/phase-3-plan.md`), once that
plan doc has absorbed the open questions above.
