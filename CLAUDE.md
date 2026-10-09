# rustfm-proto

One-week Rust + Slint dual-pane file manager prototype for Windows 11. `PROTOTYPE-RESULT.md`
is the gate record, `docs/week-one-plan.md` the week-one record (accessibility fix, renderer
decision). Read both before changing the list.

## Gates (run before every commit)
- `cargo test --release` (3 tests)
- `cargo build --release` (default = Slint master a1ce0d8 from `vendor/slint` via `[patch]`,
  with the two accessibility patches in `docs/patches/`)

## Build variants
- `vendor/slint` is git-ignored: it is the Slint master checkout plus the commits in
  `docs/patches/`. Recreate it from the patches if missing (clone slint-ui/slint at a1ce0d8,
  `git am docs/patches/000*.patch`); regenerate the patches with `git format-patch` from
  inside it after any change there.
- `target-*` directories are ignored. Never `git add -A` with an unignored target dir (a
  1.2 GB commit happened once).
- Unpatched Slint for comparison: `tools/build-master.sh` only. Stripping `[patch]` from the
  manifest is not enough while `Cargo.lock` holds the path sources; the script sets the
  lockfile aside, pins the vendored rev, and fails if the binary contains the patch string.
- Accessibility off: `cargo build --release --no-default-features` (isolates the AccessKit cost).
- Both renderers in one binary: `cargo build --release --features skia --target-dir target-skia`;
  select with `SLINT_BACKEND=winit-femtovg` or `winit-skia`. Skia D3D is slower here (22.2% vs
  17.2% main-thread busy) and does not fire the rendering notifier, so the in-app frame timer
  is blank on that path. The renderer stays femtovg.

## Measuring
- In-app: `RUSTFM_SCRIPT` (syntax in `src/auto.rs`) drives scenarios; stderr carries `[perf]`,
  `[busy]`, `[split]`, `[enum]` lines. `SLINT_ACCESSKIT_STATS=1` prints accessibility
  tree-update counters once a second; `SLINT_ACCESSKIT_UPDATE_DELAY_MS` overrides the 50 ms
  coalescing (0 = pre-patch behaviour).
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

## Delivery
Branch, PR, squash-merge on Jeremy's go-ahead, per the projects-root `CLAUDE.md`. The
upstream patches and issue text in `docs/` are posted by Jeremy, never from a session.
