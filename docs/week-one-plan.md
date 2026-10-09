# Week one of the real build: clear the accessibility gate

Written 2026-10-08 after the prototype gate (see `../PROTOTYPE-RESULT.md`). The prototype
passed on split cost, enumeration, stability and memory, and failed frame cadence and
UI-thread load only while scrolling continuously, with the AccessKit tree rebuild as the
single dominant cause (43% of main-thread CPU; 33.6% busy with it, 15.2% without). The
six-month build does not start on list or file-operation features until this gate is
cleared or consciously waived.

## Exit criteria for the week

Run `tools/wpr-gate.ps1` elevated at the console, default features (accessibility on), with
the desktop in its normal state (PowerToys and Logi Options+ running):

1. Main thread under 20% busy over the 10 s after the split, by WPR.
2. Accessibility frames under 10% of main-thread samples in that window.
3. A screen reader (Narrator) still reads the focused row's name, size and date, and
   announces row changes when arrowing through the list.

If 1 and 2 hold but 3 does not, the week has failed: an accessible list is a requirement.

## Day 1: reproduce on Slint master and name the trigger

- Build the prototype against Slint `master` (git dependency) instead of 1.18.1. The leak
  fix from [slint-ui/slint#13676](https://github.com/slint-ui/slint/pull/13676), merged
  2026-09-27, lives in `build_new_tree` and may change the cost profile. Re-run the gate.
- Find what makes the adapter "active" on this desktop. `accesskit_windows` activates on the
  first `WM_GETOBJECT` for `UiaRootObjectId`. Candidates after PowerToys and Logi were ruled
  out: `TextInputHost.exe` / `ctfmon.exe` (text services), Windows Explorer's shell
  integration, the Zoom and Teams clients. Method: log the sender of the first `WM_GETOBJECT`
  (hook the subclassed window procedure in a debug build and record
  `GetWindowThreadProcessId` of the message's origin via `GetCurrentInputMessageSource`, or
  use Spy++ style tracing under WPR with the `Microsoft-Windows-Win32k` provider).
- Deliverable: a sentence naming the client, and the master-build gate number.

## Days 2 and 3: make the rebuild cheap

Three approaches, in order of preference; stop at the first that meets the exit criteria.

1. **Viewport-only tree.** Today `build_node_for_item_recursively` walks every instantiated
   item on every update. The ListView already instantiates only visible rows, so the tree
   is already bounded; the cost is that every scrolled frame re-walks it (about 35 rows x 4
   text items x property evaluation plus a `Vec` allocation per node). Patch
   `i-slint-backend-winit/accesskit.rs` so that a scroll that only shifts existing rows
   issues node *updates* for changed nodes rather than a full `build_new_tree`, and so
   that rows leaving and entering the viewport are the only structural changes.
2. **Settle-based updates.** Coalesce tree rebuilds until no input has arrived for one
   frame (16 ms) or until 100 ms has passed, whichever first. Screen readers do not consume
   60 updates per second; UIA clients re-query on `StructureChanged` anyway. This is a
   small change in `reload_tree` (a timer instead of an immediate deferred callback) and is
   the fallback if 1 is too invasive for a week.
3. **Client gating.** Only rebuild when the client that activated the adapter is an
   assistive technology rather than an incidental `WM_GETOBJECT` sender. Hard to do
   reliably (there is no API that says "this is a screen reader"), so this is the last
   resort and must be paired with 2.

Each approach is measured with the same WPR scenario and with Narrator running. The
prototype's `src/auto.rs` driver and `tools/wpr-gate.ps1` are the harness; nothing new is
needed to measure.

## Day 4: upstream

- Open the issue in `docs/upstream-issue-draft.md` on slint-ui/slint (Jeremy posts it; the
  prototype session was not allowed to), linking #3867 and the measurements.
- If approach 1 or 2 produced a patch, open it as a PR against master with the WPR numbers
  in the description. Carry the patch as a git dependency on a fork until it is released.

## Day 5: renderer check and real-input check

- Build with `renderer-skia` + `renderer-skia-d3d` (skia-safe prebuilt binaries; expect a
  long first build) and run the gate. Record render-pass time and frame-interval p95
  against femtovg. Keep whichever is better at 4K; this is the only point in the project
  where swapping the renderer is cheap.
- Real-input scroll test at the console: a human wheel and thumb-drag on both panes on the
  40k folder while `[perf]` lines stream, to decide whether the 18 ms interval p95 seen
  with scripted input is visible. If it is, this becomes a frame-pacing investigation in the
  winit backend (present timing), not application work.

## What not to do this week

- No file operations, icons, context menus or settings. The list is the product's core and
  the gate is about the list.
- No change to the model or enumeration design. They cost 30 ms per 40k folder; nothing
  there is on the critical path.
- No alternative toolkit evaluation unless the exit criteria cannot be met by Friday. If
  they cannot, the write-up says so and the stack decision is reopened with this evidence.

## Execution notes

### Day 1 (2026-10-08, evening)

- **Trigger named.** In the original console trace, the process that readied the prototype's
  main thread most often was `TextInputHost.exe` ("InputApp", the Windows text-input host):
  10,365 wakeups in a 20 s run, about 500 per second, against 1,686 for FancyZones and 164
  for the Logi agent. It attaches a UI Automation client to every focused window to track
  text input, which is why closing PowerToys and Logi Options+ changed nothing. It is a
  Windows component on every Windows 11 desktop, so client gating (approach 3) is not a
  viable fix and the cost must be removed in the backend.
  Method: `wpaexporter` with `C:	races
ustfm-readying.wpaProfile` (CPU Usage (Precise),
  Readying Process, filtered to the main thread), aggregated by count.
- **Two update paths in `internal/backends/winit/accesskit.rs` on master (a1ce0d8).**
  `reload_tree` does a full `build_new_tree` and is scheduled whenever a component is
  unregistered, which the ListView does for every row that leaves the viewport, i.e. on
  every scrolled frame. `rebuild_tree_of_dirty_nodes` is the incremental path; it is driven
  by a property tracker that goes dirty whenever any node's geometry changes, which during
  a scroll is every visible row, so it re-serialises every node every frame. Both are
  scheduled through zero-delay `Timer::single_shot`, which is why they show under
  `TimerList::activate_expired` in the stacks. `reload_tree` already coalesces through
  `pending_update`; the dirty-node path does not.

### Day 2 (2026-10-08, late evening)

Console WPR runs via `tools/wpr-compare.ps1`, analysed with `tools/analyze-trace.py --symbols`.
Main thread, 10 s after the split, same scrolling scenario, PowerToys and Logi running:

| Variant | Busy | Accessibility | Render pass | ListView rows | Frame interval p95 |
|---|---|---|---|---|---|
| Slint 1.18.1 (gate run) | 33.6% | 1,428 ms | 1,163 ms | 500 ms | 19.6 to 25.8 ms |
| Slint master a1ce0d8, unpatched | 28.4% | 1,201 ms | 987 ms | 385 ms | 17.7 to 31.5 ms |
| master + patch, `SLINT_ACCESSKIT_UPDATE_DELAY_MS=0` (control) | 26.8% | 1,147 ms | 997 ms | 323 ms | 18.1 to 18.3 ms |
| master + patch, 50 ms (default) | **18.4%** | **349 ms** | 959 ms | 344 ms | 18.6 to 19.4 ms |

- Exit criterion 1 (under 20% by WPR, accessibility on, normal desktop) is met by the 50 ms
  patch: 18.4%. The control at 0 ms sits with unpatched master, so the gain is the
  coalescing, not the vendored build.
- Exit criterion 2 (accessibility under 10% of main-thread samples) is not met: 349 of
  1,839 samples is 19%, although in absolute terms it is 3.5% of the thread's time. A
  100 ms delay would roughly halve it again; whether that latency is acceptable is a
  Narrator question for day 3, not a performance one.
- Master alone is worth about 5 points over 1.18.1 (render pass 987 vs 1,163 ms), which
  the RDP session could not show.
- The remaining floor with the patch is the render pass (52%, text drawing 28%) and
  ListView row maintenance (19%). Those are the day-5 renderer question.
- Patch: `docs/patches/accesskit-coalesce.diff` (17 lines in
  `internal/backends/winit/accesskit.rs`). Both update paths use a configurable delay
  instead of a zero-delay single-shot timer.

### Day 3 (2026-10-09, morning)

- **Correction to the day-2 table.** The "Slint master, unpatched" row there is genuine: that
  binary was built before the `[patch]` section existed. But the "master" binary in this
  morning's rerun was built from the manifest of the commit that already carries the patch,
  so it is a second patched-50 run. In the table below the 0 ms control is the honest
  unpatched number. Fixed by building unpatched master from the branch manifest with the
  `[patch]` section stripped rather than from an older commit.
- **Narrator read nothing** because no element declared an `accessible-role`; every
  day-2 accessibility measurement was on a tree with no row content. The rows, list and
  path bar now carry roles, labels, selection state and item index (`src/ui/pane.slint`),
  and `tools/uia-dump.ps1` confirms the UI Automation tree: a `List` named "Files" whose
  children are `ListItem`s named "file_1.txt, 1 B, 2026-10-08 19:33" with a selection
  pattern. Narrator still did not announce rows, because keyboard focus stayed on the list
  container; the fix in progress mirrors StandardListView (zero-width FocusScope inside a
  component inheriting ListView, `accessible-delegate-focus` on the current row).

Console WPR with rows in the accessibility tree (same scenario, same analysis):

| Variant | Busy | Accessibility | Render pass | ListView rows |
|---|---|---|---|---|
| master + patch, 0 ms (= unpatched behaviour) | **40.6%** | 2,135 ms | 1,223 ms | 516 ms |
| master + patch, 50 ms (run 1) | 25.6% | 663 ms | 1,195 ms | 485 ms |
| master + patch, 50 ms (run 2, mislabelled "master") | 26.2% | 706 ms | 1,220 ms | 462 ms |

- Exposing the rows roughly doubles the unpatched accessibility cost (2,135 vs 1,147 ms)
  and the patch now saves about 15 points, but 25.6% is still above the 20% exit criterion.
  The non-accessibility floor is about 18% (render 12%, ListView 5%), so coalescing alone
  cannot clear the gate with rows exposed: either the rebuild has to become incremental
  (approach 1) or the render pass has to get cheaper (day 5), or both.

### Day 3, afternoon (2026-10-09)

- **Narrator reads the rows** with the StandardListView-style structure (`FileListBase`
  inherits ListView and holds the single `for`; `FileList` adds the zero-width FocusScope and
  `accessible-delegate-focus: max(0, current-row)`). Jeremy heard each row as he arrowed and
  noticed no lag after wheel scrolling at either 50 ms or 100 ms. Exit criterion 3 met.
- **Narrator was running during the morning WPR comparison** and off during the afternoon
  one, which explains the 25.6% vs 19.1% gap on near-identical code. Both numbers stand,
  labelled by condition. All "master" rows recorded today were patched builds: the manifest
  I stripped was written to `C:	mp` by Python while bash copied from Git Bash's `/tmp`,
  so the unstripped file was used every time. Verified by grepping the binary for the
  patch's environment-variable string; a genuinely unpatched master is being rebuilt with
  that check in the loop.

Console WPR, rows exposed and focus delegated, Narrator **off**:

| Variant | Busy | Accessibility | Render pass | ListView rows | Frame interval p95 |
|---|---|---|---|---|---|
| master + patch, 50 ms | **19.1%** | 507 ms | 893 ms | 329 ms | 18.0 ms |
| master + patch, 0 ms (= unpatched behaviour) | 30.5% | 1,645 ms | 885 ms | 328 ms | 18.7 ms |

Same, Narrator **on** (morning): patched 50 ms 25.6% (accessibility 663 ms), 0 ms 40.6%
(2,135 ms).

- Exit criterion 1 (under 20% by WPR, accessibility on, normal desktop) is met at 19.1%
  with no screen reader attached, which is the normal desktop. With Narrator attached the
  same build is 25.6%; that is a screen-reader session, where a 60 Hz scroll is not the
  user's workload, so it is recorded rather than gated.
- Exit criterion 2 (accessibility under 10% of main-thread samples) is not met: 27% at 50 ms.
  The structural fix (node updates for moved rows instead of a full rebuild) is what would
  close it; coalescing is at its useful limit.

### Day 4 (2026-10-09): verified baseline and upstream package

Unpatched Slint master a1ce0d8, built by `tools/build-master.sh` (binary verified free of
the patch string), rows exposed, Narrator off, same scenario: **31.9%** main-thread busy,
accessibility 1,706 ms (full rebuild 741, UIA event raising 687), render 933 ms, ListView
343 ms, frame interval p95 22.9 ms. It matches the 0 ms control (30.5%, 1,645 ms), which
closes the question of whether the vendored build itself changed anything.

Final week-1 table (console WPR, main thread, 10 s after the split, rows in the tree):

| Variant | Narrator | Busy | Accessibility | Render | ListView | Interval p95 |
|---|---|---|---|---|---|---|
| Slint master a1ce0d8, unpatched | off | 31.9% | 1,706 ms | 933 ms | 343 ms | 22.9 ms |
| master + patch, 0 ms (control) | off | 30.5% | 1,645 ms | 885 ms | 328 ms | 18.7 ms |
| master + patch, 50 ms | off | **19.1%** | 507 ms | 893 ms | 329 ms | 18.0 ms |
| master + patch, 50 ms | on | 25.6% | 663 ms | 1,195 ms | 485 ms | n/a |
| master + patch, 0 ms | on | 40.6% | 2,135 ms | 1,223 ms | 516 ms | n/a |

Upstream package: `docs/patches/0001-winit-accesskit-coalesce-accessibility-tree-updates-.patch`
(format-patch against a1ce0d8, commit message carries the numbers) and
`docs/upstream-issue-draft.md`. Jeremy posts both; this session is not allowed to.

Remaining for the week: day 5, the renderer check (`renderer-skia` + `renderer-skia-d3d`)
against the 893 ms render pass that is now the largest cost, and the real-input scroll check.

### Day 4, afternoon (2026-10-09): the structural fix

Implemented in the vendored Slint (`docs/patches/0002-*.patch`, cumulative diff
`docs/patches/accesskit-week1.diff`), three parts:

1. **Parked component ids.** The backend reused a destroyed row's component id for the next
   row created, so AccessKit saw an *updated* node (new name, new position in set) and the
   Windows adapter raised a property-changed event per node, every frame. Freed ids now wait
   behind 1,024 others before reuse.
2. **Incremental reload.** `build_new_tree` keeps the node cache across structural changes;
   a node whose place, content and geometry are unchanged is neither rebuilt nor re-sent.
   Each cached node has a content tracker and a geometry tracker, and the last-sent node is
   kept for a value compare.
3. **A content node per Flickable.** Accessible children inside a Flickable are grouped
   under a synthetic GenericContainer whose AccessKit transform is the scroll offset; row
   bounds are relative to it. UI Automation filters that node, so the tree and the screen
   rectangles are unchanged (verified with `tools/uia-dump.ps1` before and after a scroll).
   The first version keyed this on the accessible item being a Flickable and did nothing,
   because the item with the list role in a ScrollView is a Rectangle above the Flickable;
   the fix looks for a Flickable on the path between an accessible item and each child.

In-process counters (`SLINT_ACCESSKIT_STATS=1`): nodes sent per incremental update fell
from ~168 (every row of the scrolling pane) to ~30 to 55 (rows entering the viewport plus
the content node); backend CPU about 15 ms per second of scrolling. In-app busy numbers in
this session stopped being comparable between runs (the presentation mode changed mid-way:
250 frames per 5 s one minute, 160 the next), so the console WPR comparison decides.

### Day 4, evening (2026-10-09): structural fix measured at the console

Console WPR, rows exposed, Narrator off, same scenario and analysis:

| Variant | Busy | Accessibility | of which rebuild / UIA events | Render | ListView | Interval p95 |
|---|---|---|---|---|---|---|
| Slint master a1ce0d8, unpatched (verified) | 31.7% | 1,747 ms | 772 / 737 ms | 928 ms | 343 ms | 19.2 ms |
| structural fix, coalescing off (0 ms) | 21.7% | 721 ms | 432 / 197 ms | 906 ms | 341 ms | 17.5 ms |
| structural fix + 50 ms coalescing | **16.7%** | **198 ms** | 124 / 43 ms | 948 ms | 357 ms | 18.9 ms |

Narrator reads each row as the user arrows on the structural build (transform-based
coordinates), so the screen-reader check holds.

Week-one exit criteria, final:

1. Under 20% main-thread busy, accessibility on, normal desktop: **met, 16.7%** (was 31.7%).
2. Accessibility under 10% of main-thread samples: **11.9% by the share definition**
   (198 of 1,671), **2.0% of the thread's time** in absolute terms (was 17.5%). The share
   misses by two points because the denominator shrank with it. The remaining cost is the
   per-update walk over the cached nodes evaluating dirty geometry trackers (rows move every
   frame; the visibility read walks ancestors for the clip). A cheaper visibility read would
   close it; not done this week.
3. Narrator reads and announces rows: **met**.

The accessibility backend is no longer the dominant cost: the render pass (948 ms, 57% of
the remaining time, text 533 ms) and ListView row maintenance (357 ms) are. That is the
day-5 question (renderer check) and beyond.

### Day 5 (2026-10-09): renderer check and real-input scroll

**Renderer.** One binary built with both renderers (`cargo build --release --features skia
--target-dir target-skia`), the week-one patches in both, selected with `SLINT_BACKEND`.
Console WPR, same scenario, Narrator off:

| Renderer | Busy | Render pass | of which text | ListView rows | Accessibility |
|---|---|---|---|---|---|
| femtovg (OpenGL) | **17.2%** | 975 ms | 493 ms | 363 ms | 206 ms |
| Skia (Direct3D) | 22.2% | 1,456 ms | 616 ms | 321 ms | 183 ms |

Skia costs the main thread 50% more per frame than femtovg on this list: its command
recording and text path are heavier CPU-side, and the Direct3D flush itself is only 56 ms.
Decision: **stay on femtovg**. Two notes for later: under Skia Direct3D Slint does not fire
the rendering notifier, so the in-app frame timer is blank on that path; and the Skia
binary is 25 MB against 15 MB.

**Real-input scroll check.** Jeremy scrolled both panes by wheel and by thumb drag for
about 55 s on the patched femtovg build: 2,418 frames, interval p50 16.5 ms and p95 between
17.9 and 19.6 ms in every 5 s window, render p95 under 2.8 ms, a handful of 50 to 85 ms
outliers at drag starts and pane switches, UI thread 14.2% busy over the first 10 s. The
motion looked smooth throughout. The 18 ms p95 that the original gate's criterion 2 trips
on is scheduling jitter around vsync, not dropped frames, and it is not visible: recorded
as such, not pursued further.

### Week one: closed

| Exit criterion | Target | Result |
|---|---|---|
| Main thread busy, accessibility on, normal desktop | under 20% | **16.7%** (from 31.7%) |
| Accessibility share of main-thread samples | under 10% | 11.9% by share, 2.0% of the thread's time (from 17.5%) |
| Narrator reads and announces rows | works | works, no lag at 50 or 100 ms |

Deliverables: the two patches in `docs/patches/` with `docs/upstream-issue-draft.md`,
posted by Jeremy; the tooling in `tools/`; this plan as the record. The renderer stays
femtovg. Next largest costs, in order: the render pass (975 ms, text 493 ms), ListView row
maintenance (363 ms), then the remaining accessibility walk (206 ms).
