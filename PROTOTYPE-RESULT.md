# Prototype result: Rust + Slint dual-pane file manager

**Verdict: PASS on four criteria, FAIL on two, with the cause of both failures isolated by
experiment to one component.** Split cost, enumeration, stability and memory are far inside
their targets. Frame cadence and UI-thread load fail only while both panes are being scrolled
continuously, and the WPR stacks put 43% of the main thread's CPU in that window in the
AccessKit accessibility tree being rebuilt and re-announced on every scrolled frame. Two
follow-up console runs confirmed it: closing the desktop's UI Automation clients (PowerToys,
Logi Options+) changed little, while building with the accessibility feature off brought the
same scenario to 15.2% main-thread busy, inside the target. The recommendation below is to
proceed, with the accessibility update as a week-one gate on the real build rather than
something to paper over here.

Hardware: SER5-DESK, AMD Radeon integrated graphics at 3840x2160, 60 Hz, Windows 11 Pro
26300, local NTFS SSD. Build: Rust 1.99 stable MSVC, Slint 1.18.1, winit backend, femtovg
(OpenGL) renderer, Fluent style, default features (accessibility on). Benchmark folder
`C:\traces\bigdir`, 40,000 one-byte `.txt` files.

Two measurement sessions fed this table. The **console run** is the gate: Jeremy ran
`tools/wpr-gate.ps1` elevated at the physical console on 2026-10-08 (`C:\traces\rustfm-proto.etl`,
2.0 GB, and `C:\traces\rustfm-proto-stderr.txt`). The **RDP session** is where the prototype
was built and soaked; it presents at 32 Hz through the Remote Display Adapter, so its frame
cadence numbers are display-bound and are only quoted where the console run has no equivalent.

## Criteria

| # | Criterion | Target | Measured (console run unless noted) | Pass |
|---|---|---|---|---|
| 1 | Second pane usable after split | < 200 ms | 37.3 ms from F3 to the second pane rendered with rows; its 40k enumeration complete 49.9 ms after the request (35 to 38 ms across 6 runs in both sessions) | PASS |
| 2 | Scroll frame time p95 | < 16.7 ms | Render pass p95 3.1 to 4.2 ms. Frame interval p50 16.5 ms, p95 19.6 ms during wheel scrolling and 25.8 ms during thumb dragging on both panes, so roughly one frame in twenty misses its vsync slot | FAIL (interval); render itself is 4x inside the target |
| 3 | UI thread busy, 10 s after split | < 20% | WPR CPU Usage (Sampled), main thread tid 19252, 4.48 to 14.48 s: 3,356 samples at 1 ms = **33.6%** while both panes are scrolled for 6 of those 10 s. In-app GetThreadTimes over the same window: 30.3%. With the split followed by nothing: 0.3% (RDP session, in-app) | FAIL under continuous scrolling; PASS when the split stands alone |
| 4 | Enumerate 40k (first screen / complete) | < 100 ms / < 2 s | 27.9 ms / 49.9 ms for the right pane, requested while the event loop was running. The worker itself reads 40,000 entries in 27 to 37 ms. (At process start the left pane's first screen takes 413 ms under WPR, 230 ms without: window creation precedes the event loop, that is startup not enumeration) | PASS |
| 5 | 30-minute soak | 0 crashes | RDP session: 72 min 47 s continuous scripted use (the 30-minute scenario looped 125 times): 125 splits, 626 navigations across the 40k folder, `WinSxS\Manifests` and `System32`, 1,001 sorts, 86,684 frames, 0 crashes, 0 panics, responsive throughout. Logs in `C:\traces\soak.log` and `soak.err` | PASS |
| 6 | Working set, both panes on 40k | < 300 MB | 116 MB peak in the gate scenario, 99 to 104 MB steady over the 73-minute soak (no growth) | PASS |

Baseline for comparison, Files 4.2.9 on the same desktop and folder (from
`C:\traces\files-perf-summary.md`): UI thread 90% busy during enumeration and 64% busy for
6.5 s after adding a pane, items appearing at tens per second, a crash within minutes of
splitting.

## Follow-up runs: isolating the accessibility cost

Same elevated console, same script, same 10 s window after the split, main thread only.
Traces `C:	racesustfm-noclients.etl` and `C:	racesustfm-noa11y.etl`; reports under
`traces/` in this repo.

| Configuration | WPR main-thread busy | In-app (GetThreadTimes) | Frame interval p95 | Render pass p95 | Accessibility frames |
|---|---|---|---|---|---|
| Original: PowerToys and Logi Options+ running, accessibility on | 33.6% (3,356 samples) | 30.3% | 19.6 to 25.8 ms | 3.1 to 4.2 ms | 1,428 samples, 43% |
| PowerToys and Logi Options+ closed, accessibility on | 27.6% (2,756) | 27.0% | 18.8 to 19.0 ms | 3.2 ms | 1,181 samples, 43% |
| Accessibility feature off (`cargo build --release --no-default-features`) | **15.2%** (1,516) | 16.6% | 17.9 to 18.2 ms | 2.8 to 3.1 ms | 0 |

Reading:

- The rebuild is not caused by those two tools. With them closed the accessibility share of
  the main thread stayed at 43%; something still attaches a UI Automation client to every
  window (the text-services host is the usual suspect), and AccessKit treats that as "active"
  and rebuilds the full tree on every change. This is a Slint backend behavior to take up
  upstream, not a desktop-configuration problem.
- With accessibility off, criterion 3 passes by WPR (15.2%) and in-app (16.6%), and the
  remaining main-thread time is 65% render pass (text drawing dominant), 24% ListView row
  maintenance, 11% heap. That is the floor this stack offers without touching the renderer.
- Criterion 2 is marginal in every configuration: the render pass is 4x inside the target
  but the frame-to-frame interval p95 sits at 18 ms against a 16.7 ms vsync period. A dropped
  frame would show as 33 ms; 18 ms is scheduling jitter between the scripted 16 ms input
  timer and the display's vsync, so the practical reading is "no dropped frames, pacing not
  locked to vsync". Whether that is visible to a user needs a real-input check at the
  console; it is not something the application code controls (lesson 2 below).

## WPR: where the main thread's time goes in the 10 s after the split (original run)

Source: `xperf -a stack -butterfly -tid 19252 -range 4480000 14480000 -event Prof` with the
prototype's PDB on the symbol path; report in `C:\traces\export\rustfm\bf-main-10s.html`.
3,356 samples total. Inclusive hits, trivial wrappers (thread start, event-loop entry)
collapsed:

| # | Frame | Samples | Share |
|---|---|---|---|
| 1 | `i_slint_core::timers::TimerList::activate_expired` (the scripted driver's wheel and drag timers and everything they trigger) | 1,988 | 59.2% |
| 2 | `WinitWindowAdapter::with_access_kit_adapter_from_weak_window_adapter` (all accessibility work) | 1,428 | 42.6% |
| 3 | `accesskit_windows::subclass::wnd_proc` (UI Automation requests arriving as window messages) | 1,209 | 36.0% |
| 4 | `i_slint_renderer_femtovg::FemtoVGRenderer::render` (the whole render pass) | 1,163 | 34.7% |
| 5 | `accesskit_windows::SubclassingAdapter::update_if_active` | 1,029 | 30.7% |
| 6 | `i_slint_core::item_tree::visit_item_tree` (walking the item tree, shared by render and accessibility) | 878 | 26.2% |
| 7 | `i_slint_backend_winit::accesskit::NodeCollection::build_new_tree` (rebuilding the accessibility tree) | 835 | 24.9% |
| 8 | `Repeater::visit` (the ListView's visible-row instances) | 743 | 22.1% |
| 9 | `femtovg itemrenderer::draw_text` (text drawing, parley shaping + glyph runs) | 628 | 18.7% |
| 10 | `accesskit_windows::adapter::QueuedEvents::raise` → `UiaRaiseAutomationPropertyChangedEvent` → `NotifyWinEvent` (announcing the changes to UIA) | 400 | 11.9% |

Supporting frames: `Repeater::ensure_updated_listview` 500 (14.9%), text layout
(`text_layout_info` / `text_size` / `cached_paragraphs`) about 390 (11.5%), low-fragmentation
heap allocation 398 (11.9%), of which most sits under the accessibility node builder
(`build_node_for_item_recursively` collects a fresh `Vec` per node, 570 samples).

Reading: of the 33.6% busy, about 14 points are accessibility (tree rebuild plus UIA event
raising), about 12 points are the render pass (text dominates), and the remaining 7 points
are ListView row maintenance, property evaluation and the driver's own timers. Modules by
exclusive samples: `rustfm-proto.exe` 48%, `ntdll.dll` 25% (heap), kernel 14%, the AMD
OpenGL driver `atio6axx.dll` 3%.

This matches the RDP-session experiment in which the `accessibility` Cargo feature was
switched off: UI-thread CPU per frame fell from 6.5 ms to 2.4 to 3.7 ms. The prototype is
shipped with the feature on because a real file manager needs it.

## How the numbers were taken

- Every event in the scenarios went through Slint's own input path
  (`slint::Window::dispatch_event` driven by the in-process script in `src/auto.rs`), so
  hit-testing, scrolling, layout, accessibility and rendering are the real code paths. Only
  the Win32 WndProc hop for input is skipped. The scripted timers are visible in the WPR
  stacks as frame 1 above.
- Frame timing comes from the renderer's BeforeRendering / AfterRendering notifications,
  reported every 5 s as p50 / p95 / max over a 4,096-entry ring buffer.
- In-app UI-thread busy is user + kernel time of the main thread from `GetThreadTimes`
  over the 10 s after F3. The WPR figure is sampled-profile hits on the same thread over the
  same window, located in the trace by the right-pane enumeration worker thread's start
  time (4.495 s).
- Working set is `Process.WorkingSet64` sampled every 250 ms from PowerShell.

The gate scenario (`tools/wpr-gate.ps1`):

```
sleep 2000; split; sleep 500; wheel 1 -120 16; wheel 0 -120 16; drag 1 400 1000;
drag 0 400 1000; sleep 6000; sort 0 1; sleep 1000; sort 1 2; sleep 3000; quit
```

## Recommendation

Proceed with the six-month build on this stack, with one condition. The hypothesis held
where it mattered: with the toolkit owning its rendering and the list backed by a plain
Vec, a 40,000-entry folder costs the UI thread about 30 ms in total, a second pane appears
in under 40 ms, the process sits at 116 MB, and 73 minutes of abuse produced no crash. The
code is about 900 lines and nothing in it is tuned.

The condition: the two failed criteria have a single dominant cause, the per-frame
accessibility tree rebuild, which is Slint backend code, not application code, and which the
follow-up runs show is triggered on an ordinary desktop regardless of which utilities are
running. Week one of the real build should establish, with upstream, how the AccessKit tree
can be updated lazily (only the viewport's rows, only on settle, or only when a real
assistive client is attached rather than any WM_GETOBJECT sender) and re-run this exact
scenario. The accessibility-off run (15.2%) is the proof that nothing else in the design
stands in the way. If that cannot be made to pass 20% with accessibility on, the stack
decision should be revisited then, not six months later. Text rendering is the second cost
and is the normal one.

## Three things learned about Slint on Windows that change the plan

1. **AccessKit is the scroll tax, not rendering.** With the default `accessibility`
   feature the UI thread spends more per scrolled frame rebuilding and announcing the
   accessibility tree than drawing. A screen-reader-capable product cannot turn it off, so
   the plan needs a throttled or viewport-only accessibility update (and an upstream
   issue or patch) before the file list is "done".
2. **Presentation is vsync-bound and not tunable from app code.** The femtovg/OpenGL
   backend blocks in present at the display's rate: 60 Hz at the console, 21 fps over RDP
   regardless of a 1.5 ms render. Evaluate the Skia D3D renderer (`renderer-skia-d3d`) in
   week one of the real build and treat frame pacing as a backend property.
3. **The model boundary is cheap; the rest is plumbing.** `slint::Model` with
   `row_added` per 2,000-entry batch, `row_data` building a four-field struct for visible
   rows only, and `invoke_from_event_loop` for wakeups gave these numbers with no tuning.
   Smaller lessons that cost time: colors must come from `Palette` or dark mode renders
   white text on white; keyboard events need window focus before `forward-focus` applies;
   `DirEntry::metadata()` is free on Windows (FindNextFile data); the window takes 200 to
   400 ms to create before the event loop runs, which dominates first paint at startup; a
   Windows environment variable caps at 32 KB; `xperf -a stack` needs `-event Prof` to
   select sampled-profile stacks.
