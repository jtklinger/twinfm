# Draft upstream issue for slint-ui/slint (to be posted by Jeremy)

Suggested title: **winit/accesskit: full tree rebuild on every scrolled frame makes
accessibility the dominant UI-thread cost on a large ListView (Windows, 1.18.1)**

Labels to request: `bug`, `a:accessibility`, `a:winit`. Related: #3867 (same symptom,
Slint 1.3.0, open since 2023-11), #13676 (leak fix in the same code path, merged 2026-09-27).

---

## Summary

With the default `accessibility` feature on Windows 11, scrolling a `ListView` over a
40,000-row model spends more main-thread CPU rebuilding the AccessKit tree than rendering.
WPR sampled-CPU on the main thread over 10 s of continuous wheel and scrollbar-thumb
scrolling in two panes: 33.6% busy with accessibility on, 15.2% with the feature off; 43%
of the samples with it on are under `WinitWindowAdapter::with_access_kit_adapter_from_weak_window_adapter`.
This is #3867 reproduced on 1.18.1 with numbers and stacks.

## Environment

- Slint 1.18.1, `backend-winit`, `renderer-femtovg`, style `fluent`, default features
- Rust 1.99 stable, `x86_64-pc-windows-msvc`
- Windows 11 Pro 26300, AMD Radeon integrated graphics, 3840x2160 at 60 Hz
- The adapter is "active" on an ordinary desktop: closing the obvious UI Automation
  clients (PowerToys, Logi Options+) did not change the cost (27.6% busy, accessibility
  share unchanged at 43%). No screen reader was running.

## Reproduction

Repository: <private, Jeremy to decide whether to make it public or attach the files>.
A `ListView` with `for row[i] in rows:` over a custom `slint::Model` of 40,000 rows, each
row a `Rectangle` with four `Text` items. A scripted driver dispatches
`WindowEvent::PointerScrolled` every 16 ms and a thumb drag via `PointerPressed` /
`PointerMoved` / `PointerReleased`; the behavior is the same with real mouse input.

Build and run with `wpr -start CPU -filemode`, then compare the main thread with
`--no-default-features` (which drops only `accessibility` in this crate).

## Main-thread stacks, 10 s window, 3,356 samples (1 ms)

| Frame | Samples | Share |
|---|---|---|
| `WinitWindowAdapter::with_access_kit_adapter_from_weak_window_adapter` | 1,428 | 42.6% |
| `accesskit_windows::subclass::wnd_proc` | 1,209 | 36.0% |
| `accesskit_windows::SubclassingAdapter::update_if_active` | 1,029 | 30.7% |
| `NodeCollection::build_new_tree` | 835 | 24.9% |
| `NodeCollection::build_node_for_item_recursively` | 571 | 17.0% |
| `alloc::vec::spec_from_iter_nested` under `build_node_for_item_recursively` | 570 | 17.0% |
| `PropertyTracker::evaluate` (accesskit::Node) | 517 | 15.4% |
| `accesskit_windows::adapter::QueuedEvents::raise` -> `UiaRaiseAutomationPropertyChangedEvent` -> `NotifyWinEvent` | 400 | 11.9% |
| `FemtoVGRenderer::render` (for comparison: the whole render pass) | 1,163 | 34.7% |

With the feature off the same window is 1,516 samples: 65% render pass, 24%
`Repeater::ensure_updated_listview`, 11% heap.

## Expected

Scrolling a list whose visible-row count does not change should not rebuild the whole
accessibility tree per frame. Options that would fix it from the application's point of
view, in order of preference:

1. Issue node updates for rows whose content changed and structural changes only for rows
   entering and leaving the viewport, instead of `build_new_tree` on every update.
2. Coalesce rebuilds until input settles (one frame of quiet, or at most every 100 ms).
3. At minimum, document that `accessibility` has this cost on large lists (the ask in #3867).

## A measured mitigation

Scheduling both update paths (`invoke_later` for `reload_tree`, and
`AccessibilityPropertyDirtyHandler::notify` for the dirty-node rebuild) on a 50 ms
single-shot timer instead of a zero-delay one, with no other change, took the same
scenario on master (a1ce0d8), with the rows carrying `accessible-role: list-item`,
labels and `accessible-item-selected`, and focus delegated to the current row as
StandardListView does:

| Build | Main thread busy, 10 s after split | Accessibility | of which full rebuild / UIA events |
|---|---|---|---|
| master a1ce0d8, unpatched | 31.9% | 1,706 ms | 741 / 687 ms |
| patched, delay forced to 0 ms (control) | 30.5% | 1,645 ms | 711 / 701 ms |
| patched, 50 ms | 19.1% | 507 ms | 186 / 259 ms |

Narrator still reads each row as the user arrows and announces rows after a wheel scroll
with no perceptible lag at 50 ms or 100 ms. With Narrator attached the patched build
measures 25.6% (unpatched 40.6%).

The trigger on this desktop is `TextInputHost.exe` (the Windows text-input host), which
attaches a UIA client to every focused window; closing third-party tools (PowerToys,
Logi Options+) did not change the cost. The patch is attached as a format-patch against
a1ce0d8. I would expect a proper fix to be structural (node updates for moved rows rather
than a full rebuild, which would also cut the UIA event volume), but coalescing alone
clears the practical problem and is small enough to review quickly.

Happy to test a patch on this machine with the same WPR scenario.
