# twinfm

TwinFM: dual-pane file manager for Windows 11 in Rust + Slint. Started as the one-week
prototype `rustfm-proto` (renamed 2026-10-09; stack adopted the same day, see the recommendation
in `PROTOTYPE-RESULT.md`). Crate and exe are `twinfm`; the Slint fork branch keeps its `rustfm` name.
`PROTOTYPE-RESULT.md` is the gate record, `docs/week-one-plan.md` the week-one record
(accessibility fix, renderer decision), `docs/phase-1-plan.md` the Phase 1 record (six
milestones, gate re-run after each). Read them before changing the list.

## Gates (run before every commit)
- `cargo test --release` (36 tests; two touch the real shell and clipboard, see below)
- `cargo build --release` (Slint comes from the `rustfm` branch of the jtklinger/slint fork,
  pinned by rev in `Cargo.toml`: master a1ce0d8 plus the two accessibility patches in
  `docs/patches/`; a fresh clone builds with no manual steps)

## Build variants
- The patched Slint lives at https://github.com/jtklinger/slint, branch `rustfm` (a1ce0d8 +
  `git am docs/patches/000*.patch`). To change it: commit on that branch, push, regenerate
  `docs/patches/` with `git format-patch a1ce0d8`, and bump the `rev` in both `Cargo.toml`
  entries. Every git dependency is pinned by `rev`; never `branch = "master"`.
- `target-*` directories are ignored. Never `git add -A` with an unignored target dir (a
  1.2 GB commit would be the result).
- Unpatched Slint for comparison: `tools/build-master.sh` only. It rewrites the fork
  references to slint-ui/slint at a1ce0d8, builds with the lockfile set aside, restores both
  files, and fails if the binary contains the patch string.
- Accessibility off: `cargo build --release --no-default-features` (isolates the AccessKit cost).
- Both renderers in one binary: `cargo build --release --features skia --target-dir target-skia`;
  select with `SLINT_BACKEND=winit-femtovg` or `winit-skia`. Skia D3D is slower here (22.2% vs
  17.2% main-thread busy) and does not fire the rendering notifier, so the in-app frame timer
  is blank on that path. The renderer stays femtovg.

## Measuring
- In-app: `TWINFM_SCRIPT` (syntax in `src/auto.rs`) drives scenarios; stderr carries `[perf]`,
  `[busy]`, `[split]`, `[enum]`, `[sort]`, `[select]`, `[hidden]`, `[refresh]`, `[icons]` lines. Driver
  keys include `CtrlH` (hidden files), `F5`/`CtrlR` (refresh), `CtrlA`, `Space`,
  `ShiftUp`/`ShiftDown`; `click`/`ctrlclick`/`shiftclick <pane> <row>` select rows; file
  operation steps `copy`, `move`, `delete`, `sdelete`, `newfolder <name>`, `rename <name>`,
  `cc`, `cx`, `cv`, and `waitops <ms>` to wait for a job and the refresh after it; `[fileops]`
  lines on stderr. `SLINT_ACCESSKIT_STATS=1` prints accessibility
  tree-update counters once a second; `SLINT_ACCESSKIT_UPDATE_DELAY_MS` overrides the 50 ms
  coalescing (0 = pre-patch behaviour).
- File operation soak: `tools/soak-m3.ps1 -Minutes N` builds a scratch tree in
  `%TEMP%\twinfm-m3\`, loops copy/rename/move/recycle/permanent delete/new folder/clipboard
  cycles between the panes with the bigdir/System32/Manifests scroll scenario in between, and
  reports exit code, `panicked` count and working set (CSV every 30 s). It runs the app with
  `TWINFM_SOAK=1`, which adds `FOF_NOCONFIRMATION` to every job and refuses any file operation
  outside `%TEMP%\twinfm-m3`. Never set `TWINFM_SOAK` otherwise. Each cycle adds one Recycle Bin
  entry and overwrites the clipboard.
- Tests that touch the desktop: `fileops::tests::worker_runs_real_shell_operations` runs real
  `IFileOperation` jobs in `%TEMP%\twinfm-m3-test-<pid>\`, and
  `clipboard::tests::real_clipboard_round_trip_overwrites_clipboard` replaces the clipboard's
  contents and leaves it empty.
- Numbers that decide anything come from console WPR runs Jeremy performs elevated
  (`tools/wpr-gate.ps1` for one variant, `tools/wpr-compare.ps1 -Set accessibility|renderer`
  for several), analysed without elevation by `python tools/analyze-trace.py <etl> --symbols`.
- Harness-launched runs flip between RDP 32 Hz and console 60 Hz presentation without notice,
  so in-app busy numbers taken minutes apart are not comparable. Record Narrator on/off with
  every run (it adds about 6 points of busy).
- `tools/uia-dump.ps1` shows what a screen reader sees (names, selection, screen rectangles).
  Elements with no `accessible-role` are invisible to it; rows use the StandardListView
  layering (base component holds the single `for`, derived adds the zero-width FocusScope and
  `accessible-delegate-focus`).

## License
The app is MIT. Slint is used under its Royalty-free 2.0 license, which allows a modified
Slint inside an application with attribution (the badge in `README.md`; the `AboutSlint`
widget once there is an About dialog). The patches and fork commits are offered under MIT;
see README § License before changing any of this.

## Delivery
Branch, PR, squash-merge on Jeremy's go-ahead, per the projects-root `CLAUDE.md`.
**Upstreaming is deferred** (2026-10-09): `docs/upstream-issue-draft.md` and `docs/patches/`
are the record. Do not post to slint-ui/slint from a session; Jeremy decides when.
