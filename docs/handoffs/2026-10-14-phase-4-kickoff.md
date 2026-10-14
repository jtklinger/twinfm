# Handoff: TwinFM Phase 4 kickoff, 2026-10-14

Written at the Phase 3 close, for the session that starts Phase 4 after Jeremy has decided what
it holds. Nothing is in flight in code: P3-M1 to P3-M5 are squash-merged and every record, the
deduplicated deferrals and the Phase 4 candidates are in `docs/phase-3-plan.md` § Phase 3 close.
The one open code change is PR #35, a test-only CI fix.

## Resume here

- Repo: twinfm (https://github.com/jtklinger/twinfm, private), local clone
  `C:\Users\jtkli\projects\twinfm`, branch `main`. Work on the SER5-DESK desktop (the WPR gate
  needs the console, COM work needs a real shell). Work in a fresh worktree from `origin/main`.
- PRs: #35 (`fix/ci-zip-tests`, test-only, fixes CI red on main since P3-M4; its CI passed) and
  the one carrying this file (`docs/phase-3-close`, docs only). Merge #35 first, then this one,
  each on Jeremy's word.
- Verify state before continuing (command, then what was observed when this was written):
  - `git log --oneline -1 origin/main`: `45b5bfb thumbnails: P3-M5 shell thumbnails on the icon
    workers, 32 px rows behind a setting, the trace of two per-row layouts (#34)` (or #35's and this
    PR's squash commits on top of it)
  - `cargo test --release` (in this PR's worktree, at 45b5bfb plus docs): `test result: ok. 122
    passed; 0 failed; 0 ignored`, build `Finished` in 3m 46s
  - `gh pr checks 35`: `windows pass 7m8s` (run 38629682718), PR open, not merged
  - `ls -la --time-style=long-iso target*/release/twinfm.exe` in the main checkout:
    - `target-p3m4`: 18,519,552 bytes, 2026-10-14 10:19 (07ca56c, P3-M5's reference)
    - `target-p3m5`: 18,806,784 bytes, 2026-10-14 14:42 (ee0be21, the reference for Phase 4)
    - `target-p3m5a`: 18,520,064 bytes, 2026-10-14 11:22 (9598f79, trace variant (a), 32 px details)
    - `target-p3m5b`: 18,274,816 bytes, 2026-10-14 11:34 (075f85a, trace variant (b), the grid)
    - `target\release`: 18,806,784 bytes, 2026-10-14 16:02 (45b5bfb, Jeremy's everyday binary)
  - `ls C:\traces\bigdir | wc -l`: `40000` (the gate folder)
  - `C:\traces\bigdir.zip`: present, 4,257,810 bytes, 2026-10-14 08:12 (fixture, not committed)
  - `C:\traces\thumbs.toml`: read-only (`-r--r--r--`, `IsReadOnly` True), one line `thumbnails =
    true` (the thumbnails-on gate config)
- If a check fails, main moved after this was written: read `git log` and the plan doc's
  Phase 3 close before acting.

Read first, in this order: `CLAUDE.md`; `docs/phase-3-plan.md`, the intro (the gate protocol,
the architecture rules with the P2-M4 and P2-M5 amendments), § Open questions and § Phase 3 close
(the gate table of the phase, the deferrals by area with their origin, the Phase 4 candidates, the
fleet of gate binaries); then the record of whichever milestone the chosen Phase 4 work builds on
(for the grid: § P3-M5 Trace; for archives: § P3-M4; for search: § P3-M2).

## Goal

Phase 4's goal and milestones are not set. Phase 3's goal (accepted 2026-10-13), "TwinFM finds and
shows what Explorer can", held without giving back the gate. The gate bar stays: main thread under
20% busy while two panes scroll a 40k folder, accessibility on, judged by interleaved triplets
against the last merged milestone's build, now `target-p3m5`.

## State

Done and verified, all on main:

- P3-M1 history and settings (#29, e19a32a), P3-M2 search (#30, d2a96c1), P3-M3 folder sizes
  (#31, 6dd7797), P3-M4 archives (#32, b0c1ff3), P3-M5 thumbnails (#34, 45b5bfb; gate build
  ee0be21). Gate medians reference / candidate: 21.2 / 21.8, 26.2 / 26.9 (Defender-disturbed),
  20.7 / 20.6, 20.8 / 20.7, P3-M5 defaults 18.0 / 18.2 and thumbnails on 17.3 / 13.8. The P3-M5
  trace (ab14): reference 17.4, 32 px details 13.7, grid 13.2. 122 tests.
- Jeremy's decisions of 2026-10-14: thumbnails only (setting, default off), 32 px rows only with
  thumbnails on, the large-icon grid deferred with its trace numbers.
- The Phase 3 close and this handoff (this PR, docs only).

In flight: **PR #35** (`fix/ci-zip-tests`, 43b1c91): CI on main has been red since b0c1ff3
(P3-M4) because the two real-shell zip tests built paths from the runner's 8.3 `%TEMP%`
(`RUNNER~1`) and compared them with the shell's long paths; the fix builds them from the long form.
Its `windows` check passed on 2026-10-14 (`gh pr checks 35`: `pass`, 7m8s, run 38629682718), the
first green CI run since P3-M3; the PR was open and unmerged when this was written. The failure
cannot be reproduced locally (this machine's `%TEMP%` has no 8.3 alias), so the runner is the only
proof.

Half-done or unverified: none in code.

## Next steps (ordered, concrete)

1. `gh pr checks 35` in its own command; merge #35 only if `windows` passed (`gh pr merge 35
   --squash --delete-branch`), on Jeremy's word. If it failed, read the job log before anything
   else. Then merge this PR on Jeremy's word, `git checkout main && git pull`, and rebuild
   `target\release` (`cargo build --release` on main), saying so.
2. Ask Jeremy to remove the worktrees listed under Cleanup (the session's permission classifier
   blocks `git worktree remove` and `rm -rf`).
3. Jeremy decides Phase 4's goal and milestones from the candidates in § Phase 3 close (none is
   decided). Present them as listed there, with their origins; do not pick.
4. Write `docs/phase-4-plan.md` in the shape of `phase-3-plan.md`'s intro: the goal as accepted,
   the milestones in Jeremy's order, the gate protocol (reference `target-p3m5`, ee0be21), the
   architecture rules carried with both amendments, and the open questions. In the same PR the
   `CLAUDE.md` intro paragraph gains the Phase 4 record. Delete this handoff in that PR.
5. If the large-icon grid is chosen: the trace is done (§ P3-M5 Trace, branch `trace/p3-m5-b-grid`,
   075f85a); the first work is the strip (its recompose rate, `GRID_MARGIN_ROWS = 1`, and the
   texture upload per compose, 36% of the grid's busy in ab14), then the view switch chosen once
   per pane, keyboard by tile, selection, drops and the driver at tile geometry (the trace build
   left them at 22 px rows). If the Slint snap patch is chosen, it is a fork commit with its own
   triplet (`CLAUDE.md` § Build variants for the fork procedure).
6. Each milestone as in Phase 3: implementer brief, lead review, gates, push, PR, a gate build from
   a detached checkout with `--locked`, `tools/wpr-triplet.ps1 -Reference p3m5 -Candidate <name>
   -Tag ab17` (the next free tag), console checks AFTER the triplet, merge on Jeremy's word,
   rebuild `target\release`, delete `target-p3m4` once `target-p3m5` has served as a reference.

## Open questions / decisions pending

- **Phase 4's goal and milestones** (Jeremy): the candidates are in § Phase 3 close; the grid,
  content search, the snap patch, thumbnails beyond the folder listing, the rest of archives, the
  compact context menu and OneDrive status marks are the larger ones.
- **A thumbnails-on triplet per milestone?** Since P3-M5 the gate can run a candidate with a
  settings file (`p3m5@C:\traces\thumbs.toml`). Whether a milestone that touches the list or the
  icons must also pass with thumbnails on is a protocol change for Jeremy to agree.
- **The Slint whole-row snap patch** (P3-M1): Jeremy takes it when the fork is next touched.
- **Upstreaming the two accessibility patches**: Jeremy decides when.
- **The trace binaries** `target-p3m5a` and `target-p3m5b`: keep (the grid's baseline if it is
  chosen) or delete (both rebuild from their branches on origin); Jeremy's call.

## Gotchas hit this session

- **`wpr-triplet.ps1` must run the `wpr-gate.ps1` beside it** (`$PSScriptRoot`, fixed in 27e79a2):
  a worktree's copy once ran the main checkout's older `wpr-gate.ps1`, which did not know
  `-Config`; a plain PowerShell script puts an unknown parameter in `$args` without a word, so
  three "thumbnails" runs were defaults. Check each candidate's stderr for `[settings] file
  <toml> (read)` before reading a settings triplet.
- **Editing CRLF files from Bash**: Python edits in a Bash heredoc lost backslashes in Windows
  paths and missed CRLF line endings, twice; `sed -i` in Git Bash rewrote `CLAUDE.md` with every
  CR stripped (and so matched nothing on `\r$`). Use the Edit tool in the session's own worktree;
  it refuses files in another worktree, so there write the text to a scratchpad file with the
  Write tool and splice it with a short Python script that reads and writes bytes, then check
  `tr -cd '\r' < file | wc -c` against `wc -l`.
- **The permission classifier blocks `git worktree remove` and `rm -rf`** from a session: ask
  Jeremy to run them.
- **`gh pr merge` runs even when the last CI check failed** (no branch protection): read `gh pr
  checks <n>` BEFORE merging, in a separate command, and merge only on green.
- **This machine's `%TEMP%` has no 8.3 alias**, so a test that passes here can still fail on the
  GitHub runner, whose `%TEMP%` is `C:\Users\RUNNER~1\...`. Build test paths from the long form.
- **The shell renders image files as thumbnails even with `SIIGBF_ICONONLY` at 32 px** (149 ms
  each in System32): a plain icon request on a folder of images can be as slow as a thumbnail.
- Carried from the P3-M5 handoff, still true: console checks after the triplet, never before; gate
  runs as ONE command, after confirming the candidate exe's timestamp; a running TwinFM locks its
  exe ("Access is denied" on a rebuild); Jeremy's stderr capture is UTF-16 (`iconv -f UTF-16LE -t
  UTF-8`); any per-row binding or conditional on a runtime property costs a measurable fraction of
  a point, so choose row structure outside the `for` and prove it with the generated-code diff;
  implementer checks leave scratch folders in the Recycle Bin and overwrite the clipboard.

## Cleanup

For Jeremy, from the main checkout (`git worktree remove <path> && git worktree prune`; the
session cannot), each under `C:\Users\jtkli\projects\twinfm\.claude\worktrees\`:

- `phase-3-kickoff-handoff-991768` (branch `docs/p3-m5-kickoff-handoff`, merged as #33: delete the
  local branch too)
- `p3-m5-trace-a` and `p3-m5-trace-b` (branches `trace/p3-m5-a-32px`, `trace/p3-m5-b-grid`: keep
  them on origin; the local branches can go)
- `p3m5-gate` (detached at ee0be21; `target-p3m5` lives in the main checkout, not here)
- `fix-ci-zip` (a dead-end checkout of `fix/ci-zip-tests-long-paths`: delete that local branch too)
- `p3-m5-kickoff-handoff-16d08f` (the lead's, branch `fix/ci-zip-tests`, after its session ends and
  #35 is merged)
- `phase-3-close` (this PR's, branch `docs/phase-3-close`, after it is merged)

Delete this file in the PR that writes `docs/phase-4-plan.md`, once that plan has absorbed the
open questions above.
