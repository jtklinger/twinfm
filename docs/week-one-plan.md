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
