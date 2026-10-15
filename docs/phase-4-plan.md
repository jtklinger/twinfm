# Phase 4: TwinFM shows a folder as large icons

Written 2026-10-14, when Phase 3 closed with P3-M1 to P3-M5 merged and the P3-M5 build at
medians of 18.2% (defaults) and 13.8% (thumbnails on) on the gate under the triplet protocol (see
`phase-3-plan.md` § Phase 3 close). Phase 1 made TwinFM usable for a day instead of Files, Phase 2
made it trustworthy every day, Phase 3 made it find and show what Explorer can. Phase 4's goal,
chosen by Jeremy on 2026-10-14 from the Phase 3 close's candidates: **TwinFM shows a folder as
large icons, with thumbnails**, without giving back the gate.

The starting point is the P3-M5 trace (`phase-3-plan.md` § P3-M5 Trace): variant (b), branch
`trace/p3-m5-b-grid` (075f85a, on origin, never merged; its gate build is `target-p3m5b`), a
large-icon grid under the strip pattern that held 13.2 against the reference's 17.4 (ab14), with
about a third of its busy on its strip. It was built on 085be4d, before thumbnails landed, as the
ONLY list in its build; it left the drop highlight, `dnd::hit`, the rename editor, the shell menu
point and the driver at 22 px rows, and nothing in it is a view switch. Today's main (2a9894b)
chooses one of four list components per pane, once, by `show_type` x `thumbnails`; the grid becomes
further choices made the same way, never per row.

Three milestones, in the order Jeremy agreed on 2026-10-14 (his words: do not fold them; push
forward quickly but efficiently):

1. **P4-M1, the grid as a real view.** The trace carried onto main behind a view switch chosen
   once per pane (a `view` setting per pane, saved; Ctrl+Shift+1 for details and Ctrl+Shift+2 for
   large icons, as Explorer's view keys), with every list component's choice still made outside the
   `for`. Everything the trace left at 22 px rows moves to tile geometry: hit testing (`dnd::hit`),
   the drop highlight, the rename editor, the shell menu point, keyboard movement by tile, and the
   driver's `click`, `rclick` and drop steps. The strip's two known costs are trimmed first, since
   they were about a third of the grid's busy in ab14: the recompose rate (`GRID_MARGIN_ROWS = 1`)
   and the texture upload per compose (a pixel-buffer `Image` has no cache key). Gate: the
   defaults triplet (the details list is the reference's row and must not move) and a grid-on
   triplet with a read-only settings file, as P3-M5's thumbnails-on triplet ran.
2. **P4-M2, thumbnails in the tiles.** 96 px thumbnails through the P3-M5 path
   (`IThumbnailCache::GetThumbnail`, the per-size table, the drop range of rows scrolled past),
   with the `thumbnails` setting governing the grid as it governs the list, scale changes, and the
   two things the trace did not investigate: the thin green fringe on the shell's 96 px folder
   icon and the 16 px placeholders stretched before the first icon lands. Gate: defaults, grid on,
   and grid with thumbnails on.
3. **P4-M3, selection and drops by tile.** Shift and Ctrl selection across tiles and rows of
   tiles, rubber-band selection if the details list has none, drag out of the grid with a
   tile-sized drag image, drops onto tiles and onto empty space, auto-scroll at the grid's edges,
   and a grid cycle in the soak scripts. Gate: defaults and grid on.

**The gate is re-run after every milestone**, under the protocol agreed on 2026-10-12 and
unchanged since: `tools/wpr-triplet.ps1` elevated at the console by Jeremy, accessibility on,
Narrator off, every other TwinFM window closed, the split-and-scroll scenario on
`C:\traces\bigdir`, on clean binaries built with `--locked` into their own `--target-dir
target-<name>` from detached `git worktree add` checkouts with default features. Three runs per
binary, interleaved (reference, candidate, reference, candidate, reference, candidate), compared by
median; a gap counts only when every reference run sits below every candidate run or the medians
differ by more than one point. The reference for the phase's first milestone is `target-p3m5`
(ee0be21, P3-M5's gate build); after each merge the new milestone's gate build becomes the next
reference and the previous one is deleted (`target-p3m4` goes once `target-p3m5` has served).
Tags continue from ab17. Bar: main thread under 20% busy over the 10 s after the split, judged
against the reference's spread in the same session, never against a recorded number. Console
checks run after the triplet, never before. A milestone is not closed until its table is recorded
here. When a milestone fails: stop, symbolize, name the frames, and either fix the mechanism or
report the options; never trade the bar off silently.

**Settings triplets are part of the protocol in this phase** (agreed 2026-10-14): a candidate
may run with a read-only settings file (`-Candidate <name>@<toml>`), and every Phase 4 milestone
runs the triplets its entry above names, each against the same reference with the same file. The
files live beside `C:\traces\thumbs.toml` (`thumbnails = true`): `C:\traces\grid.toml` (the grid
on in both panes) and `C:\traces\grid-thumbs.toml` (the grid with thumbnails on), each made
read-only once written, each candidate's stderr checked for `[settings] file <toml> (read)`
before its triplet is read (the P3-M5 `-Config` slip). The defaults triplet decides whether the
details list has moved; a settings triplet decides whether the grid holds the bar.

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
pane's strip (`icons::compose`, and the grid's `compose_grid`), and every per-key cost is paid on
a worker, once per key per process. Anything that would put a per-row or per-tile image or shell
call back is a trace first, not a feature.

Amendment (P3-M1 and P3-M5), carried over: nothing in the row or the tile may depend on a runtime
property. A per-row `if` cost 1.5 points and two per-row bindings 0.7 in P3-M1; the row's
structure is chosen once per pane, outside the `for`, and proven by diffing the generated code
against the reference's row, as P3-M5 did for its four list components. The grid's tiles carry
their own fields (`selected`, `cut`, `current`) for the same reason: no tile binds to a root
property.

Each milestone runs as in Phase 3: implementer brief, lead review, gates, push, PR, a gate build
from a detached checkout with `--locked`, `tools/wpr-triplet.ps1 -Reference p3m5 -Candidate
<name>[,<name>@<toml>] -Tag <next>`, console checks after the triplet, merge on Jeremy's word,
`target\release` rebuilt from main and said so.

## Open questions

Written 2026-10-14 at the kickoff; the Phase 3 close's open questions are absorbed here.

- **The Slint whole-row snap patch** (P3-M1 Deferred): Slint snaps a `ListView` rebuilt while
  scrolled to a whole row; the one-line fork patch is proven (`target-zcfix`). P4-M1's view switch
  rebuilds the list, so the snap shows on every switch. Jeremy takes the patch when the fork is
  next touched; it is a third fork commit with its own triplet (`CLAUDE.md` § Build variants for
  the fork procedure). Whether P4-M1 is that moment is Jeremy's call once the switch is seen.
- **Upstreaming the two accessibility patches** (`docs/upstream-issue-draft.md`,
  `docs/patches/`): still deferred; Jeremy decides when. Do not post to slint-ui/slint from a
  session.
- **The trace binaries** `target-p3m5a` (9598f79) and `target-p3m5b` (075f85a): `target-p3m5b` is
  the grid's in-session baseline for P4-M1's first comparisons and stays until P4-M1 is merged;
  `target-p3m5a` has no further use and can go on Jeremy's word. Both rebuild from their branches
  on origin.
- **Explorer's view per folder**: Explorer remembers a view per folder; Phase 4's `view` is per
  pane, saved like the Type column, and nothing per folder. Revisit only if asked.
- **Thumbnails for video and Office documents** have no fixture on this machine (P3-M5 Deferred):
  the grid shows whatever the shell's thumbnail cache returns; a person at the console checks it
  when P4-M2 lands.
- **Carried from Phase 3, not in any milestone**: content search (out for now, 2026-10-13); the
  compact Windows 11 context menu; OneDrive status marks through `PHCM_EXPOSE_PLACEHOLDERS`; the
  rest of archives; search polish; the folder sizes' unexercised paths; the Phase 2 lists
  (directory watching, Redo and progress inside TwinFM, spring-loaded folders and drop
  descriptions). The full list with origins is `phase-3-plan.md` § Phase 3 close.

## P4-M1: the grid as a real view

The first milestone of the phase (2026-10-14): the P3-M5 trace's grid (`trace/p3-m5-b-grid`,
075f85a) carried onto main (a7e6121) behind a view switch chosen once per pane, its geometry
moved from 22 px rows to tiles everywhere the trace had left it, and the strip's two known costs
trimmed and measured before the gate. Branch `feat/p4-m1-grid-view`.

### What landed

- **The setting and the keys** (`settings.rs`, `app.slint`, `main.rs`). `view_left` and
  `view_right` in `Config` (`"details"`, the default, or `"icons"`; anything else reads as details;
  a file from before the keys loads as details), saved by every save as the other keys are.
  Ctrl+Shift+1 (details) and Ctrl+Shift+2 (large icons), Explorer's view keys, on the active pane
  through the window's `set-view(pane, grid)` callback; `set_view` in `main.rs` flips the pane's
  `left-grid` / `right-grid` property, re-aims the scroll so the entry that started the first
  visible line still does (`scroll A -> B (entry E first)` in the `[view]` line), hides the strip,
  resolves, and schedules a save. With Shift held a US keyboard reports `!` and `@` for the
  digits, so both are taken. Not in the settings dialog (Deferred).
- **One more list component, chosen once per pane** (`pane.slint`). `FileGrid` (the trace's: the
  ListView's single `for` over grid rows of 120 px, each with a nested `for` over its tiles of 96
  px, a `Tile { name, selected, cut, current }` carrying what it draws, no tile binding to a root
  property, no `Image` anywhere in it) and `FileGridList` (its focus layer, as `FileList` is
  `FileRows`'). `FileListHost` gains `grid`, `grid-rows` (the `GridModel`), `cols` (`max(1,
  floor(width / 96px))`), the `grid-cols` and `grid-cursor` callbacks (sent only while the grid
  shows), and its four `if`s gain `&& !root.grid`, with a fifth `if root.grid: FileGridList`. The
  host's `row-h` is 120 px in the grid; `line-of(i)` and `col-of(i)` (pure functions: an entry's
  grid row and column, or its row and 0) carry the pane's overlays: `bring-into-view`, the drop
  highlight (the tile, 96 x 120, in the grid), `rows-h` (grid rows x 120), the rename editor (over
  the tile's name: 2 px in, 92 px wide, centred on the 19 px text at 99 px). The grid's header is
  Name alone, 24 px, so `dnd::LIST_TOP` holds.
- **The 22 px row is the reference's.** The generated Rust (`target/release/build/twinfm-*/out/app.rs`)
  for `FileList`, `FileListNoType`, `FileList32` and `FileListNoType32` and their rows, 6,090 lines
  from `InnerFileList_root_60` to the next component, compared with `target-p3m5`'s (ee0be21, the
  reference): 240 lines differ, every one of them the Fluent palette global's number (`_545`
  against the reference's) or the numbering of generated temporaries (`returned_expression`,
  `return_check_merge`); with those normalised the diff is empty. `FileGridList` follows as
  `InnerFileGridList_root_210` with its two nested row components.
- **Keyboard by tile** (`FileListHost.key`). Left and Right move one entry (grid only; the details
  list still rejects them, so Alt+Left / Alt+Right stay history), Up and Down move by `step` (the
  pane's `cols` in the grid, 1 in the list: the list's arithmetic is unchanged), Down from the last
  full grid row lands on the last entry, Up on the first grid row stays (Explorer), PageUp and
  PageDown move a viewport of grid rows (`floor(visible-height / 120px) x cols` entries; grid
  only), Home and End as before. Shift with any of them extends the range through the existing
  `cursor-moved`.
- **Tile geometry in Rust.** `dnd::Layout` gains `grid: [Option<usize>; 2]` (each pane's tiles to a
  grid row when it shows the grid, from the window's `left-grid` / `left-grid-cols` properties, so
  `window_layout` still needs no state borrow) and `row_px_of(pane)`; `hit` maps a point in a grid
  pane to (row, column) and `Spot::Row(pane, row * cols + column)`, with the space right of the
  last column and past the last entry as `Spot::Pane`; the drop auto-scroll steps by grid rows
  (`dnd::scrolled` with `TILE_H` and the grid-row count); `shellmenu::tile_menu_point` puts
  Shift+F10's menu 2 px into the cursor's tile under it, clamped to the viewport; `reveal_row`
  after a pass takes the cursor's grid row; the driver's `row_pos` aims every row step at the tile
  of visible entry `row` (`row / cols` down, `row % cols` across), `empty_pos` and the
  `rclickempty` fallback use the grid-row count and height.
- **The strip: a ring of per-row images** (Decision 1). The trace composed one image of the
  visible grid rows plus a margin and re-uploaded it on every recompose (`compose_grid`, 480 x
  1,080 px at five tiles, about 2 MB). Now each grid row is its own image: the pane's `strip-rows`
  model (`[StripRow { img, top, w, h, shown }]`, a `for` of `Image`s under the list's clip, each at
  `y: list.content-y + s.top`) holds a ring of `ceil(visible-height / 120) + 1 + 2 x
  GRID_MARGIN_ROWS` slots; `compose_grid_rows` composes grid row g into slot `g % slots`
  (`compose_grid(.., 1)`, `cols x 96` by 120 px) only when the slot does not already hold g at this
  icon size with no icon changed in it (`State::grid_slots`). A scroll of one grid row composes
  and uploads one row (one tenth of the strip); an icon reply marks the slot of its entry's grid
  row; a pass, a navigation, a width change or a scale change (`strip_dirty`, the slot count,
  `icon_px`) refills the ring. No two covered rows share a slot (the ring is as long as the
  covered range can be), and a stale slot outside the covered range sits off screen under the
  clip until its row comes round again. The details strip is untouched; `hide_strip` empties the
  ring too. `GRID_MARGIN_ROWS` stays 1 (Measurements: 2 was no better).
- **Model** (`model.rs`, the trace's): `GridModel(Rc<PaneModel>)` adapts the shown entries into
  `GridRow { first, tiles }`; `PaneModel` keeps `grid_n`, `grid_cursor` and a `grid_notify` sent
  beside every notification the grid can see (`append` notifies the partial grid row and the new
  ones, `replace` and `clear` reset, `select` and `set_cut` the entries' grid rows,
  `set_grid_cursor` the old and new cursor's). With the details list shown nothing listens to
  `grid_notify`, so those calls cost a peer check. `show_tab` gives the new tab's model the pane's
  `grid_n` and cursor and points `left-grid-rows` / `right-grid-rows` at a `GridModel` over it;
  `on_grid_cols` (Slint's `changed cols`, while the grid shows) re-maps the model and recomposes.
- **Icons at 96 px**: `GridGeom`, `compose_grid`, `grid_strip_range`, `TILE_W`, `TILE_H`,
  `GRID_ICON_PX`, `GRID_ICON_TOP` from the trace; `overlay_pixels` steps SMALL, LARGE, EXTRALARGE,
  JUMBO until the list's icons are at least the requested size. Icons are keyed per size (P3-M5),
  so the grid's 96 px renders live beside the list's 16 px ones and a switch back finds them held.
  The grid asks for its entries' type icons only: `thumbs` is off in `resolve_view` for a grid
  pane (P4-M2 brings thumbnails to the tiles).
- **Logging.** `[view] pane=N starts as large icons`, `pane=N details -> large icons (K tiles to a
  row, R grid rows), scroll A -> B (entry E first)` (and back), `pane=N already <view>`; `[grid]
  pane=N K tiles to a row, R grid rows` on a width change while the grid shows; the `[icons]`
  line's `compose N strips avg` counts grid rows composed in the grid.
- **Driver** (`auto.rs`): `view details|icons` (key names `CtrlShift1`, `CtrlShift2`); key names
  `Up`, `Left`, `Right`, `PageUp`, `PageDown`, `Home`, `End`, `ShiftLeft`, `ShiftRight`; every row
  step aims at tiles in the grid (above). `CLAUDE.md` § Measuring and § Settings carry it.
- **Tests**, 122 to 126: `view_defaults_to_details_and_round_trips_icons`,
  `grid_compose_places_icons_in_their_tiles` (the trace's: icon placement at 1.0, 1.25, 1.5 and
  2.0 scale, a five-entry compose over two grid rows, the strip range, `visible_range` with
  `TILE_H`, the N = 0 guard), `grid_maps_entries_to_rows_and_columns` (the trace's: `grid_rows`,
  `grid_cell`, `grid_span`, the adapter's rows, tiles, cursor, selection and a width change),
  `tile_menu_point_sits_under_the_tile`; `hit_testing_and_drop_folder` extended with a grid pane
  (five tiles to a row: the tile's far corner, the last column, right of the tiles, the second
  grid row, the last entry and past it, the header, scrolled by ten grid rows, and a split with
  the grid on the left and a 22 px list on the right).

### Decisions

- **Decision 1: per-row images in a ring, not a margin.** The brief named the recompose rate
  (`GRID_MARGIN_ROWS = 1`) and the texture upload per compose. Measured in-app (Measurements): the
  margin is not the lever, since a larger strip is composed and uploaded less often but costs
  more each time (margin 2: 212 composes at 1,040 us, 0.22 s, against margin 1's 252 at 818 us,
  0.21 s). The size of what is recomposed is: with one image per grid row, a scroll composes one
  120 px row and the renderer re-uploads one row's texture (a `for` item's `source` changed)
  while the other slots keep theirs (a pixel-buffer `Image` has no cache key, but an unchanged
  property keeps its cached texture). 1,422 row composes at 66 us, 0.094 s: the strip's CPU
  halved, and each upload a tenth of the size. The per-frame cost is one `y` binding per slot
  (about nine) in place of one.
- **The grid shows type icons in this milestone.** Thumbnails in the tiles are P4-M2 as agreed;
  with `thumbnails = true` a grid pane requests plain icons (`thumbs` off for it), so the table
  holds no 96 px thumbnails yet. The list keeps its 32 px thumbnails.
- **The view is per pane, saved as a string** (`"details"` / `"icons"`), so a later view has a
  name without a schema change.
- **Tiles to a row come from Slint.** The host's `cols` is the truth; Rust reads it (the window's
  `left-grid-cols`, a `changed cols` callback while the grid shows) rather than computing the
  list's width again from the layout, so hit testing and the model agree with what is drawn.
- **The switch keeps the first visible entry, not the cursor**, like a zoom; `scrollpos` after a
  switch shows the same entry's line at the top (Scripted checks).
- **The grid's header is Name only**, 24 px: the grid has no columns to sort by from the header,
  and the height keeps `LIST_TOP` and the driver's thumb point valid (the trace's choice).

### Deviations

- **A ring of per-row images** (Decision 1) in place of the margin and upload tuning the brief
  named; `GRID_MARGIN_ROWS` is unchanged at 1 and now sizes the ring.
- **`dnd::Layout` gained `grid` per pane**, not a per-pane row height: the hit test needs the tile
  count too.
- **Up on the first grid row stays put** rather than going to entry 0: the details arithmetic is
  the same as before (`current-row >= step`), the grid follows Explorer.

### Deferred

- **Thumbnails in the tiles** (P4-M2), with the `thumbnails` setting governing the grid.
- **Selection and drops by tile** (P4-M3): rubber band, a tile-sized drag image, the soak cycle.
  What already works there: Shift and Ctrl across tiles, drops onto a tile and onto empty space,
  auto-scroll by grid rows.
- **The view in the settings dialog**: only the keys and the file set it.
- **Explorer's other views** (Ctrl+Shift+3 to 8): no key, no component.
- **PageUp and PageDown in the details list**: still unmapped there (grid only).
- **A header with no columns**: the grid keeps a Name header for the sort arrow and the geometry.
- **The whole-row snap on a rebuilt list** shows on every switch (fork patch, Jeremy's call).
- **Accessibility**: each grid row is one list item, tiles have no node of their own (the trace's
  choice); not checked with Narrator.
- **Shifted digits on other keyboard layouts**: `!` and `@` cover US layouts.
- **Stale slot images** outside the covered range stay in memory until navigation (at most the
  ring: about nine images of `cols x 96` by 120 px, 230 KB each at five tiles).

### Scripted checks

Harness session (RDP, 1216 x 839 window, scale 1.0, accessibility on), this worktree's
`target\release\twinfm.exe` from the commit under review, every run with `TWINFM_SCRIPT`, no
`TWINFM_SOAK`; the grid on through `TWINFM_CONFIG=<scratchpad>\grid.toml` (`view_left = "icons"`,
`view_right = "icons"`, read-only: each such run logs `[settings] could not save: ... Access is
denied`, as the P3-M5 runs did). All exit code 0, no `panicked` line.

- **(i) The switch across the list variants**, `C:\Windows\System32`, defaults: `view icons; key
  Down; view details; key Down`, then the Type column off through the dialog and the pair again,
  then thumbnails on and the pair again, then `view icons` twice. Each switch logs `[view] pane=0
  details -> large icons (10 tiles to a row, 491 grid rows)` (or back) and the `key Down` after it
  takes effect (`[select] pane=0 Only row=0`, then `1`, then `11` (the grid: one grid row down),
  `12`, `22`, `23`), so each new list took the focus: FileList, FileListNoType, FileListNoType32
  and FileGridList were each created. The last switch, from the 32 px list scrolled to -54,
  re-aims to grid row 0 (`entry 1 first`); the second `view icons` logs `already large icons`.
- **(ii) Keyboard by tile**, `C:\traces\bigdir`, grid on, 10 tiles to a row: Down 0, Right 1,
  Right 2, Down 12, Left 11, Up 1, End 39999 (content-y -479,280: grid row 3,999 at the bottom),
  Home 0, PageDown 50 (five grid rows of ten, content-y -6: the sixth row brought fully into
  view), PageDown 100, PageUp 50 (content-y -600), Down 60, Shift+Down `Range row=70 selected=11`,
  Shift+Right `Range row=71 selected=12`, Up 61, 51, 41 (content-y -480).
- **(iii) Mouse, menus, drops and the editor on tiles**, `%TEMP%\twinfm-p4m1\src` (five folders
  and 30 files, made by hand), grid on, 10 tiles to a row: `click 0 3` selects entry 3, `ctrlclick
  0 7` toggles (2 selected), `shiftclick 0 9` ranges (3), `rclick 0 2` selects entry 2 and opens
  its item menu at the pointer (`[menu] pane=0 row=2 1 target(s) at (427, 123)`), `rclickempty`
  clears the selection and opens the background menu at (692, 660), `click 0 1; key ShiftF10`
  opens entry 1's menu at (285, 183): the list's left (187) plus one tile plus 2 px, under the
  first grid row. `dropfiles 0 1 copy ...\dst\dropme.txt` lands on tile 1 (`[dnd] over pane 0 row
  1 (...\src\beta)`, `Copied 1 item` into `beta`), `dropfiles 0 -1 copy ...\droptwo.txt` into the
  folder. `click 0 6; rename renamed-file.txt` renames `file-00.txt` (entry 6: five folders, then
  the files), `newfolder zeta` makes `zeta` (the cursor lands on it, entry 5). `split` logs `[grid]
  pane=0 5 tiles to a row, 8 grid rows` and `pane=1 5 tiles to a row`.
- **(iv) Drag auto-scroll by grid rows**, `C:\traces\bigdir`, grid on: `dragover 0 bottom 700`
  scrolls `content-y 0 -> -120` ... `-1080 -> -1200` (ten rows of 120 px), `dragover 0 top 400`
  back to -600; `view details` then re-aims `scroll -600 -> -1100 (entry 50 first)`: grid row 5 of
  ten tiles is entry 50, row 50 of 22 px.
- **(v) The gate scenario, grid on** (Measurements): 1,422 row composes, 0 panics, and with the
  defaults the reference's shape (`compose 371 strips avg 21 us`, one `ext:` request).

### Measurements

In-app, RELATIVE only: harness session (RDP), 1216 x 839 window, scale 1.0, accessibility on,
the gate scenario on `C:\traces\bigdir`, `[busy]` over the 10 s after the split, each candidate
run interleaved with `target-p3m5` (the reference) by the same runner, reference first. The
session read a quiet 4 to 9 points (the reference's usual in-app 17 to 19 was not seen: the
window was presented at a low rate), so only the pairs and the counters mean anything. Three
builds of this branch, in the order they were made:

| Pair | reference (`target-p3m5`) | grid, one strip, margin 1 (the trace's) | reference | grid, one strip, margin 2 | reference | grid, ring of per-row images |
|---|---|---|---|---|---|---|
| 1 | 6.7 | 7.2 | 7.2 | 7.2 | 6.2 | 4.5 |
| 2 | 6.4 | 4.8 | 9.4 | 3.7 | 8.7 | 3.6 |
| 3 | 7.8 | 6.1 | 8.9 | 5.5 | 5.9 | 7.2 |
| **median** | **6.7** | **6.1** | **8.9** | **5.5** | **6.2** | **4.5** |
| composes | 371 strips at 21 us | 252 at 807 to 831 us (0.21 s) | 371 at 21 us | 212 at 1,031 to 1,047 us (0.22 s) | 371 at 21 us | 1,422 rows at 66.0 to 66.8 us (0.094 s) |

The strip's CPU per run: 0.206 s with the trace's strip (about a third of the grid's 0.6 s, as
ab14 said), 0.220 s with the margin at 2 (fewer, bigger composes: no gain), 0.094 s with the ring
(and each texture upload a tenth of the size; the upload itself is not in these counters, it is
the `Texture::new_from_image` the console trace will show). Resolve calls: 377 to 379 per run in
every build, 2.4 to 3.5 us.

Memory: a slot is `cols x 96 x scale` by `120 x scale` px, 4 bytes a pixel: 230 KB at five tiles
and scale 1.0, 460 KB at ten; a ring of nine slots 2 to 4 MB per pane. The 96 px icon table
entries: 36 KB each at scale 1.0.

### Gate

Protocol as agreed (the plan above): `tools/wpr-triplet.ps1 -Reference p3m5 -Candidate
p4m1,p4m1@C:\traces\grid.toml -Tag ab17`, run from this checkout's `tools\`, Jeremy elevated at
the console, accessibility on, Narrator off, no other TwinFM window, console checks after.
`target-p3m5` (ee0be21) against `target-p4m1` (the PR head, built with `--locked` from a detached
checkout). `C:\traces\grid.toml` holds `view_left = "icons"` and `view_right = "icons"`, read-only;
each grid run's stderr must show `[settings] file C:\traces\grid.toml (read)` and `[view] pane=0
starts as large icons` before its triplet is read. The defaults triplet decides whether the
details list moved (its rows are the reference's byte for byte); the grid triplet whether the
grid holds the bar.

**Both triplets passed (2026-10-15, Jeremy at the console).** Nine runs from 07:21 to 07:30,
reference first in each round, `target-p3m5` (ee0be21) against `target-p4m1` (cb589c8, the PR
head); every grid run's stderr carries `[settings] file C:\traces\grid.toml (read)` and `[view]
pane=0 starts as large icons`. Analysed with `tools/analyze-trace.py --symbols --exe`.

| Run | P3-M5 (`target-p3m5`) | P4-M1 defaults, `target-p4m1` | P4-M1 grid on, `target-p4m1` |
|---|---|---|---|
| 1 | 16.9% (1688 ms) | 17.5% (1746 ms) | 9.9% (987 ms) |
| 2 | 16.9% (1694 ms) | 17.0% (1700 ms) | 9.6% (956 ms) |
| 3 | 17.0% (1703 ms) | 16.9% (1690 ms) | 9.6% (955 ms) |
| **median** | **16.9%** | **17.0%** | **9.6%** |

**Defaults: the details list has not moved.** The medians differ by 0.1, and the candidate's run
3 (16.9) sits below the reference's run 3 (17.0), so neither half of the rule calls a gap; the
six runs lie within 0.6 of a point. Frame shares match within a few samples in every pair
(accessibility 180 / 181, 175 / 182, 182 / 179; render 1003 / 1038, 1008 / 1015, 1007 / 994;
draw_text 601 / 589, 567 / 607, 595 / 592; rows 320 / 329, 329 / 342, 318 / 324; timers 595 /
603, 604 / 606, 603 / 596; heap 147 / 163, 179 / 148, 161 / 168), and no `twinfm` frame of the
grid reaches the table in a defaults window: the row and the default resolve path are the
reference's, as the generated-code proof said. In-app the candidate read 18.4, 17.8, 18.6 against
16.4, 18.0, 18.3, `compose 371 strips avg 24.2 to 24.4 us` (reference 24.3 to 26.8), 0 `panicked`.

**Grid on: seven points under the reference and ten under the bar**, the tightest spread of any
triplet (0.3). The grid draws far less than the list, as in ab14 (rows 38 / 38 / 43 against the
reference's 320 / 329 / 318; draw_text 291 / 296 / 288 against 601 / 567 / 595; accessibility 77 /
72 / 68 against 180 / 175 / 182; timers 284 / 288 / 277 against 595 / 604 / 603), and the strip
now costs it about 15% of its window instead of ab14's 36%: `compose_grid` 82 / 85 / 72 samples
(8.3%, 8.9%, 7.5%; ab14: 254, 19.1%) and `Texture::new_from_image` 66 / 60 / 76 (6.7%, 6.3%, 8.0%,
of it `ImageStore::alloc` 39 / 31 / 36; ab14: 227, 17.1%, of it 169). `resolve_view` inclusive 89 /
86 / 76 (it holds the compose). In-app 11.2, 10.0, 10.3, `compose 1422 strips avg 71.6 to 74.1 us`
(grid rows), 0 `panicked`. Traces `C:/traces/twinfm-ab17-{p3m5,p4m1,p4m1-grid}-{1,2,3}.etl`,
reports under `C:/traces/export/`.

P4-M1 holds both gates: the default row is the reference's, and the grid, with its strip as a ring
of per-row images, sits at 9.6 against the bar's 20. Jeremy's console checks (2026-10-15, after
the triplet): "console checks look good". Merged as #38 (7ca4a5f, 2026-10-15); `targetelease`
rebuilt from main. `target-p4m1` (cb589c8) is the reference for P4-M2; `target-p3m5` goes once it
has served as one, and `target-p3m5b` (the grid's trace baseline) has no further use.

## P4-M2: thumbnails in the tiles

The second milestone of the phase (2026-10-15): the grid takes thumbnails through the P3-M5 path at
the tiles' icon size, the `thumbnails` setting governs the grid as it governs the list, and the two
things the trace left uninvestigated (the thin green fringe on the shell's 96 px folder icon and
the 16 px placeholders stretched before the first icon lands) are explained and fixed. Branch
`feat/p4-m2-thumbnails-tiles`, from d55623b. The change is small: nothing new in Slint, no new
list component, no new worker path, no new driver step.

### What landed

- **Thumbnails reach the tiles through the P3-M5 path, unchanged** (`resolve_view`). The grid
  pane no longer forces `thumbs` off: with the setting on, a grid pane asks for its `thumb:` keys
  at the grid's icon size (`GridGeom::icon_px`: 96 px at 100%, 120 at 125%, 144 at 150%), the
  workers render them from the system thumbnail cache (`IThumbnailCache::GetThumbnail`,
  `WTS_EXTRACT`), letterbox them into the square, and the ring of per-row images composes them as
  it composes icons. The per-size table keeps the grid's 96 px thumbnails beside the list's 32 px
  ones. The publish of the rows a resolve asks for and the workers' drop of thumbnails for rows
  scrolled past (`Views::passed`) work in entry indices, so they carry over as they are: in a
  grid of ten tiles the UI margin (`ICON_MARGIN_ROWS`, 10 entries) is one grid row, and the
  drop margin (`STRIP_MARGIN_ROWS`, 4 entries) under half of one. The `thumbnails` flip through
  the dialog takes the existing relist path for a grid pane too (Scripted checks (ii)), and a scale
  change re-keys the table and the ring by icon size as P4-M1 left it.
- **The green fringe, explained and fixed** (`bgra_to_rgba`). Probed by rendering the shell's
  folder icon (`C:\Windows`) at 48, 96, 128 and 256 px through `render` and dumping the pixels:
  the 96 px bitmap carries an edge row (row 12, the top of the folder's body) of straight, not
  premultiplied, colour, (251, 187, 23) at alpha 64, which no premultiplied pixel can be; 190 of
  its 253 semi-transparent pixels are such, and the 48 px (19 of 57) and 256 px (137 of 1,430)
  renders have them too, so a 256 px fetch fitted down by our box filter shows the same row. The
  renderer blends the buffer as premultiplied (`dst = src + dst x (1 - a)`), so that row comes out
  brighter than any pixel of the icon: over a light background about (255, 255, 214), a pale lime
  line along the top of an orange-yellow folder (the "green fringe"); over the dark theme about
  (255, 211, 47), a brighter yellow line. The fix is in the conversion, once per key: every
  channel is clamped to its alpha (a valid premultiplied pixel is unchanged). The fetch stays as
  it was (`SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK` at the requested size). The clamp applies to the
  16 and 32 px icons too, where the same pixels were blended the same way.
- **Placeholders drawn at the requested size** (`placeholder`, `IconTable::fitted`). The three
  placeholders were 16 x 16 buffers box-filtered up to the icon size, so a 96 px tile showed
  six-pixel blocks before its icon landed. The painter now takes the size: its rectangles are laid
  out on a 16-unit grid scaled to the size, with an `inset` in pixels for the one-pixel outline
  (the file's paper inside its grey edge, the drive's face inside its case), so the outline stays
  one pixel at 96 px and the 16 px pictures are what they were. `fitted` draws a placeholder index
  at the size asked for instead of fitting the 16 px buffer; the raw 16 px buffers stay in the
  table (the drag image's fallback reads them).
- **Tests**, 126 to 127: `placeholders_are_drawn_at_the_requested_size` (the 16 px pictures,
  the 96 px outline's corners, the table's `fitted` for a placeholder);
  `bgra_conversion_keeps_premultiplied_alpha` extended with the clamp (the folder's edge pixel).
- **Settings file for the gate**: `C:\traces\grid-thumbs.toml` (`view_left = "icons"`,
  `view_right = "icons"`, `thumbnails = true`), read-only.

### Decisions

- **The clamp lives in the conversion, not in the fetch.** The probe showed the straight-colour
  edge at every size the shell hands out, so asking for a native size and resampling ourselves
  would not have cured it; clamping each channel to its alpha makes any buffer a valid
  premultiplied one at the cost of three `min`s per pixel, once per key per size.
- **Placeholders are drawn, not resampled**, because a one-pixel outline is a one-pixel outline
  at any size; the 16-unit grid keeps the 16 px pictures byte for byte.
- **No new logging.** The once-a-second `[icons]` line's `thumbs N avg M ms dropped D` counts the
  grid's thumbnails as it counts the list's; `[view]` and `[settings]` lines say which view and
  setting a run is in.

### Deviations

- None from the brief. The entry named "scale changes" as a thing to cover: nothing needed
  changing (P4-M1 keys the ring and the table by icon size), so it is a check, not code.

### Deferred

- **Thumbnails for video and Office documents**: still no fixture on this machine; the grid shows
  what the shell's cache returns (Open questions).
- **A frame or shadow around a thumbnail** (Explorer draws one): the tile shows the letterboxed
  image alone, as the 32 px row does.
- **A thumbnail-sized drag image** out of the grid: P4-M3.
- **The margins in grid rows**: `ICON_MARGIN_ROWS` and `STRIP_MARGIN_ROWS` are in entries, so
  the grid's margins are a grid row and under half of one; enough for the ring (one grid row of
  margin), revisit if a trace shows re-requests on a slow scroll.

### Scripted checks

Harness session (RDP, 1216 x 839 window, scale 1.0, accessibility on), this worktree's
`target\release\twinfm.exe` from the commit under review, every run with `TWINFM_SCRIPT`, no
`TWINFM_SOAK`, the fixture `%TEMP%\twinfm-p3m5-thumbs\` (200 JPEGs). All exit code 0, no
`panicked` line.

- **(i) The grid with thumbnails on** (`TWINFM_CONFIG=C:\traces\grid-thumbs.toml`): `[settings]
  file C:\traces\grid-thumbs.toml (read)`, `[view] pane=0 starts as large icons`, `[icons] list
  geometry: 32 px rows, 32 px icons`; the first resolve asks 80 keys (eight grid rows of ten: the
  viewport plus a grid row's margin) and the workers answer `thumbs 18 avg 60.71 ms`, `45 avg
  45.31 ms`, `17 avg 46.18 ms` (two of the 200 over 100 ms: `[icons] thumbnail 209 ms: ...
  photo-016.jpg`, `228 ms: ... photo-017.jpg`, the system cache cold for this copy), the ten-tick
  wheel asks 100 more (`dropped 6`: rows the scroll passed), `view details` re-aims `scroll -1200
  -> -3200 (entry 100 first)` with no request (the 32 px list's thumbnails are its own keys, held
  from P3-M5's runs), `view icons` back asks 20 and drops 14. The table ends at 183 images.
- **(ii) The flip through the dialog while the grid shows** (`TWINFM_CONFIG=C:\traces\grid.toml`):
  `[settings] dialog: thumbnails false -> true`, `[icons] list geometry: 32 px rows, 32 px icons`,
  the pane lists again (`[enum] pane=0 done: 200 entries`), `+80 requests +80 replies ... thumbs
  80 avg 9.14 ms` (the cache warm from (i)); then `thumbnails true -> false`, `22 px rows, 16 px
  icons`, the pane lists again and asks nothing (its entries' type icon is held).
- **(iii) The defaults** on the fixture: one request (`ext:.jpg`), three strips at 24 us, nothing
  of the grid or of thumbnails.

### Measurements

In-app, RELATIVE only: harness session (RDP), 1216 x 839 window, scale 1.0, accessibility on,
the gate scenario on `C:\traces\bigdir`, `[busy]` over the 10 s after the split, each candidate
run interleaved with `target-p4m1` (cb589c8, the reference) by the same runner, reference first,
three rounds per settings file.

| Pair | reference, defaults | P4-M2, defaults | reference, `grid.toml` | P4-M2, `grid.toml` | reference, `grid-thumbs.toml` | P4-M2, `grid-thumbs.toml` |
|---|---|---|---|---|---|---|
| 1 | 17.2 | 18.3 | 12.8 | 11.6 | 9.5 | 9.5 |
| 2 | 18.6 | 18.9 | 11.4 | 12.2 | 10.6 | 9.7 |
| 3 | 17.2 | 18.0 | 10.8 | 9.5 | 9.5 | 8.4 |
| **median** | **17.2** | **18.3** | **11.4** | **11.6** | **9.5** | **9.5** |
| composes | 371 strips at 22.4 to 24.4 us | 371 at 22.6 to 24.0 us | 1,422 rows at 71.6 to 77.8 us | 1,422 at 73.4 to 79.7 us | 1,422 at 70.5 to 74.4 us | 1,422 at 71.0 to 71.9 us |

The grid pairs and the grid-thumbs pairs sit at or below the reference (the grid-thumbs
reference is `target-p4m1` with the same file: its grid panes show type icons, its lists 32 px
rows). The defaults pairs read the candidate above the reference in all three, by 0.3 to 1.1,
which no code on the defaults path explains (the clamp runs once per key on the worker, the
placeholder branch once per index and size, the `thumbs` flag is a bool), so the same three
rounds were run again with the order swapped, the candidate first:

| Pair | P4-M2, defaults, FIRST | reference, defaults, second |
|---|---|---|
| 1 | 17.5 | 20.5 |
| 2 | 18.6 | 20.5 |
| 3 | 15.9 | 18.6 |
| **median** | **17.5** | **20.5** |

The second slot of a pair read one to three points above the first in this session whichever
binary sat in it: the in-app pairs carry an order bias here, and the defaults difference is that
bias, not the change. The console triplet (interleaved, by median, reference first in every
round) decides, as always.

### Gate

`tools/wpr-triplet.ps1 -Reference p4m1 -Candidate p4m2,p4m2@C:\traces\grid.toml,p4m2@C:\traces\grid-thumbs.toml
-Tag ab18`, run from this checkout's `tools\`, Jeremy elevated at the console, accessibility on,
Narrator off, no other TwinFM window, console checks after. `target-p4m1` (cb589c8) against
`target-p4m2` (the PR head, built with `--locked` from a detached checkout). Each settings run's
stderr must show `[settings] file C:\traces\<toml> (read)` and `[view] pane=0 starts as large
icons` (and, for `grid-thumbs.toml`, `[icons] list geometry: 32 px rows, 32 px icons`) before its
triplet is read. The defaults triplet decides whether the details list moved (its rows are
untouched; the conversion's clamp runs once per key on the worker); the grid triplet whether the
grid holds the bar with the P4-M2 conversion; the grid-thumbs triplet whether the thumbnail
resolve order and the publish hold it (the gate folder's files have no thumbnail extension, so no
thumbnail is rendered there, as in P3-M5's thumbnails triplet).

Console checks after the triplet: the fixture folder in the grid with thumbnails on (the
thumbnails in the tiles, the placeholders before they land, a cold copy of the fixture), a folder
of folders at 100% and 150% (the top edge of the folder icon, no lime line), a folder with video
or Office files if one is at hand.
