# Phase 10: TwinFM's UI and UX polish, starting with the rust-ui look

Written 2026-10-22, the day after Phase 9 closed with P9-M1 to P9-M4 merged (`phase-9-plan.md`
§ Phase 9 close; main at 0a41cda). Phase 10's goal, chosen by Jeremy on 2026-10-22, in his words
(relayed by the Phase 9 coordinating session): **"Phase 10 is going to be a UI/UX polish, connect
with session 'Display windows modernization' and advise it to begin working on the polish as phase
10."** That session is this record's: on 2026-10-21 Jeremy had asked it for a review of "the current
state of the display windows" and whether they could look "more modern"; a canvas of the window as
built beside a Windows 11 style refresh was put to him, he answered "I'm more interested to know if
we can make the UI look more like the components found at rust-ui.com", the canvas was redone in
that idiom, and he chose it: "Yes, this rust-ui direction is right. Write the plan for it as a Phase
9 milestone and start the Slint changes here, gate-safe pieces first." That work, written into the
Phase 9 record as "P9-M5" while the phase ran, is Phase 10's first milestone, **P10-M1, the rust-ui
look**, on PR #91; its three gate-safe tasks landed before the phase was named.

The starting point is main at 0a41cda (the Phase 9 close). Not in Phase 10 unless Jeremy adds them:
content search, syntax colours, zoom and selection in the preview, PDFium, the Slint fork's snap
patch and upstreaming, OneDrive's syncing states (all Phase 9 close candidates, Jeremy's to pick);
accessibility stays deferred for much later (Jeremy, 2026-10-15).

Milestones:

1. **P10-M1, the rust-ui look**: its section below (the design, what was settled first, the tasks,
   what landed, the open questions). Tasks 1 to 3 (the theme tokens, the Lucide icons, the dialog
   controls) are on the PR; tasks 4 (Inter) and 5 (the bar heights) wait on Jeremy's answers; task 6
   is the gate against `target-p9m4` (3374f2b, the last merged milestone's build), tag ab41.
2. **Later milestones**: Jeremy decides from § Phase 10 candidates below, after P10-M1's console
   checks and some daily use with the new look.

The architecture rules from Phase 1 hold, and the rule this phase adds, from P10-M1's first slice:
**the look changes inside the same boxes**. A styling change that moves no height, width or position
(every `dnd.rs` constant, every driver aim, every soak's row and tile aim stands) is gate-safe in the
sense that only the per-row drawing can cost, and the triplet still measures that; a change that
moves a bar or a row changes the geometry constants, the driver and the soaks in the same commit, and
takes a soak as well as the triplet. Nothing drawn with `drop-shadow` moves with the lists.

## Calls made in this kickoff, for Jeremy to check

- The look is copied as tokens (`src/ui/theme.slint`), not as components: rust-ui's parts are Dioxus
  and Tailwind and cannot run in Slint. The grey scale has no accent colour in either scheme; the
  cursor ring is grey, the Windows accent is not read. (The canvas Jeremy chose.)
- The rows keep Windows' own file type icons and thumbnails; only the chrome takes the Lucide-style
  stroke icons. (The canvas's monochrome row icons were a stand-in.)
- The first three tasks changed no geometry, Jeremy's "gate-safe pieces first" read strictly; the
  roomier bars are a task of their own with his values.
- The inactive pane shows its selection at half strength and the active pane a 3 px soft ring drawn
  in the panes' 3 px margin, so no box moved for it.
- `ComboBox` and `LineEdit` stay Slint's fluent widgets in P10-M1; a popup choice and a text field of
  our own are polish candidates, not part of the first milestone.
- The Phase 9 pointer: the P9-M5 section left the Phase 9 record whole (one line in its close points
  here), and the branch's artefacts are named p10m1 (the gate build, its wrapper, the record's lines).

## Phase 10 candidates, for Jeremy's decision

Decided so far (2026-10-22): **P10-M2, the search box and button** (§ P10-M2 below, from his P10-M1
console check) and **P10-M3, the P10-M1 leftovers** (§ P10-M3 below, his pick, his words: "the
P10-M1 leftovers only"); the rubber band in the details list went to § Deferred to Phase 12 by his
routing. The in-app theme switch became P10-M1's task 7 the same day.

Later, not started, Jeremy picks (kept short so the list stays honest): a comfortable row height as a
setting (the canvas's 30 px rows beside today's 22; it doubles the row variants or rests on the
per-row binding P9-M1 measured as free, with the driver and soaks to follow); the sidebar
(icons-only collapsed state, pointer drag-reorder of bookmarks, a Recent or Pinned group); the
preview column's chrome in the look and the Markdown details deferred since Phase 8; TwinFM's own
password, replace and open-many questions in the look (Windows message boxes today); tooltips in
the look and the status bar's badges; a 100 ms ease on hover fills (never on anything that scrolls);
and, carried from the Phase 9 close, Google Drive's volume by its name, a sync-root scratch folder
for the soak, the dead share console check owed since P5-M1, the stale worktrees and gate builds.

## P10-M1: the rust-ui look

Branch `claude/display-windows-modernize-1c0866`, from 192d388 (P9-M2 merged) with main at f2fa4eb
(P9-M1 and P9-M3) merged in on 2026-10-21 before any code, and main at 0a41cda (P9-M4 and the Phase 9
close) merged on 2026-10-22, when the milestone became Phase 10's first.

### The design (2026-10-21)

The canvas: https://claude.ai/artifact/CeGZSFc6YSVEwqSydHqHc4 (private; four boards: the window as
built, the proposed look light and dark, and a parts sheet of each element before and after).
rust-ui.com is shadcn/ui ported to Dioxus: Tailwind-styled HTML in a webview. TwinFM draws with Slint
on femtovg, so **none of its components can be used; what is copied is the design language**, which
is small:

- **Type**: Inter (rust-ui's font) at 13 px for rows and controls, 14 px for the window title, 12 px
  for the muted labels (sidebar headers, column headers, the status bar); weight 500 for labels, the
  active tab, the cursor row's sidebar entry, folder names and the current crumb; nothing bold.
- **A neutral grey scale, no accent colour**: light ground `#FFFFFF`, card `#FFFFFF`, sidebar
  `#FAFAFA`, muted fill `#F5F5F5`, text `#0A0A0A`, muted text `#737373`, hairline `#E5E5E5`, ring
  `#A3A3A3`, primary button `#171717` on `#FAFAFA`; dark ground `#0A0A0A`, card and sidebar
  `#171717`, muted `#262626`, text `#FAFAFA`, muted text `#A3A3A3`, hairline `#FFFFFF1A`, ring
  `#737373`, active tab segment `#3F3F3F`, primary `#E5E5E5` on `#171717`. Light or dark follows the
  system, as the fluent palette does today.
- **Radii**: 6 px on controls, tabs' segments and rows' hover and selection, 8 px on the tab strip and
  the toolbar's container, 10 px on the pane cards and dialogs.
- **Buttons**: ghost (no fill; the muted fill on hover, 50% opacity when disabled), outline (a
  hairline, a 1 px shadow under it), one primary (near-black) button per dialog.
- **Tabs**: a muted strip, the active tab a lifted segment (the card colour with a 1 px shadow),
  the others muted text with no box. **Rows**: a hairline under each, the muted fill for selected
  rows, a 2 px ring at 50% inset on the cursor row, no zebra, cut rows at 50%. **The active pane**:
  the card's hairline turns ring-grey and a 3 px ring at 25% sits around it; the inactive pane's
  selected rows show at half strength. **Breadcrumb**: muted links with chevron icons, the current
  folder in full text at weight 500, no box. **Filter**: an outline field with a leading search
  icon. **Status bar**: muted text with the selection as badges (a muted-fill badge for the count, an
  outline badge for the size). **Dialogs**: 10 px corners, a soft shadow, the title at 16 px weight
  600, a switch for booleans, outline Cancel and primary Save.
- **Icons**: Lucide-style 16 px stroke icons (2 px stroke, round caps) for the toolbar, the sidebar's
  kinds, the chevrons, the close and new-tab buttons, the sort arrow and the filter. **The rows keep
  the shell's own file type icons and thumbnails** (a file manager without them is unusable; the
  canvas's monochrome row icons were a stand-in).

### Settled first (2026-10-21, before any code)

- **Where the colours live**: one global `Theme` in a new `src/ui/theme.slint` (imported by
  `app.slint`, `pane.slint` and `preview.slint`), its `dark` read from `Palette.color-scheme` so the
  system setting still decides, every token above as an `out property <brush>` named `ground`,
  `card`, `sidebar`, `muted`, `muted-half` (the muted fill at 50% alpha), `fg`, `fg-muted`,
  `fg-faint` (`fg` at 50%), `border`, `ring`, `ring-soft` (`ring` at 25%), `ring-half` (`ring` at
  50%), `tab-active`, `primary`, `primary-fg`, `shadow`, and `radius` 6px, `radius-md` 8px,
  `radius-lg` 10px as lengths. Every `Palette.` read in the three files becomes a `Theme.` read
  (60 in `pane.slint`, 38 in `app.slint`, 18 in `preview.slint` at the start). The std widgets still
  in use (`ListView`, `LineEdit`, `ComboBox`, `AboutSlint`, `ScrollView`) keep their fluent drawing
  until a task replaces them; `Button` and `CheckBox` are replaced in task 3.
- **Gate-safe pieces first, Jeremy's rule, read as: no height, width or position changes in tasks 1
  to 3.** Every height the drop targets, the band, the menu point and the driver read (`dnd.rs`
  `LIST_TOP`, `TAB_H`, `CRUMB_H`, `TOP_BAR`, `PANE_BAR`, `SIDE_FOOTER`, the 24 px sidebar rows, the
  22 and 32 px list rows, the 18 px status bar, the 24 px header) stays as it is in those tasks: the
  look changes inside the same boxes. The canvas draws roomier bars (a 36 px tab strip, a 40 px
  header, 30 px rows); those are task 5, with the geometry constants, the driver and the soaks
  changed together, after Jeremy picks the values (§ Open questions below).
- **The font**: tasks 1 to 3 set the window's `default-font-family` to `"Segoe UI Variable Text"`
  (on every Windows 11; the fluent default today is Segoe UI through the system), which has Inter's
  proportions. Inter itself is task 4: three static TTFs (Regular, Medium, SemiBold; SIL Open Font
  License 1.1, about 410 KB each) from the rsms/inter release, embedded with `import
  "fonts/Inter-Regular.ttf";` in `app.slint` and `default-font-family: "Inter"`, the licence file
  beside them and a line in README § License. **A download needs Jeremy's OK first**, so task 4 is
  asked, not assumed; if he declines, Segoe UI Variable stays and the record says so.
- **Icons**: Lucide (ISC licence) SVG files under `src/ui/icons/`, each a 24-unit stroke path with
  `stroke="currentColor"`, drawn by `Image { source: @image-url("icons/<name>.svg"); colorize:
  Theme.fg; }` at 16 px; `ToolButton`'s `glyph` string becomes an `icon` image. Segoe Fluent Icons
  stay where they draw Windows' own glyphs (the imageres status glyphs are bitmaps already; the task
  box in the preview's lists and the crumb chevron move to Lucide). The Lucide notice goes in README
  § License (it is not a crate, so not in `Cargo.toml`).
- **The active pane's ring**: no `drop-shadow` on anything that moves with the lists (a blurred
  layer per frame is the one cost this look could add). The ring is a second `Rectangle` behind the
  pane card, 3 px larger on each side, `border-width: 3px`, `border-color: Theme.ring-soft`,
  `border-radius: Theme.radius-lg + 3px`, shown only for the active pane; since the pane's own box
  must not move (the geometry rule), the ring is drawn inside the pane's `PANE_PAD` (3 px) margin,
  which is exactly 3 px. The tab segment's and the outline button's 1 px shadow is a 1 px
  `Rectangle` under them at `Theme.shadow`, not `drop-shadow`. Dialogs keep their `drop-shadow`
  (they do not scroll).
- **Rows**: the zebra goes (`mod(i, 2)` in every row variant); the hairline is a 1 px `Rectangle`
  at each row's bottom at `Theme.border` (one element per visible row, about 40; the gate measures
  it); selected rows `Theme.muted` (the inactive pane `Theme.muted-half`, read from the host's
  `active`); the cursor row `border-width: 2px; border-color: Theme.ring-half; border-radius:
  Theme.radius` in place of today's 1 px accent border; `row.cut` text `Theme.fg-faint`. The Type,
  Size and Modified cells read `Theme.fg-muted`, Name `Theme.fg`. The rename editor, the drop
  highlight, the band and the grid's cursor overlay take the same tokens (`Theme.muted` fills,
  `Theme.ring-half` borders).
- **What is not changed**: the preview column's block layout (its colours only), the Windows 11
  context menu popup (`wm-popup` takes the tokens, its rows stay 32 px), the breadcrumb dropdown's
  rows, the sidebar's drag spots, the grid tiles' geometry (their selection and cursor take the
  tokens). Nothing in Rust changes in tasks 1 to 3 except `toolbar.rs` if the icon names are passed
  from Rust (they are not: the buttons are declared in `pane.slint`).
- **Measuring**: each task ends with `cargo build --release`, `cargo test --release`, a screenshot
  of the gate scenario's window (`SLINT_BACKEND=winit-software`, `tools/shot.ps1`, the P7-M2 method)
  against the canvas, and in-app `[busy] 10 s after split` pairs against `target-p9m3` (111ef17, the
  last merged milestone's gate build when the pairs ran; the triplet's reference is `target-p9m4`,
  3374f2b, since the Phase 9 close), swapped in order as the
  CLAUDE.md run-order rule says. The console triplet (defaults and `C:\traces\grid-thumbs.toml`, the
  variant with the most drawing per row) runs once at the end, before the merge.

### Tasks

Each task is one commit on the branch, pushed; the PR is opened after task 1 and grows.

1. **Theme and colours** (`src/ui/theme.slint` new; `app.slint`, `pane.slint`, `preview.slint`).
   - Write `Theme` with the tokens above; `import { Theme } from "theme.slint";` in the three files.
   - Replace every `Palette.` read with the token the design names: window and pane ground
     `Theme.ground`, the toolbar band and the sidebar `Theme.sidebar`, pane cards `Theme.card` with
     `border-color: Theme.border` (active: `Theme.ring`) and `border-radius: Theme.radius-lg`, the
     active-pane ring rectangle, tabs (strip `Theme.muted` at `Theme.radius-md`, the active tab
     `Theme.tab-active` at `Theme.radius` with its 1 px shadow line, inactive titles `Theme.fg-muted`),
     the crumb bar (no border, no fill; crumbs `Theme.fg-muted`, the last `Theme.fg` at 500), header
     cells (`Theme.fg-muted` at 500, a `Theme.border` hairline under the row, no cell borders), rows
     as settled, the status bar (`Theme.fg-muted`), sidebar rows (`Theme.muted` fill at `Theme.radius`
     for the current row, headers `Theme.fg-muted` at 500), the splitter and preview handle
     (`Theme.border`, `Theme.ring` on hover), the preview column (ground `Theme.card`, code blocks
     `Theme.muted`, rules and table lines `Theme.border`), the menu popup and the dialogs
     (`Theme.card`, `Theme.border`, `Theme.radius-lg`).
   - `default-font-family: "Segoe UI Variable Text"` on the `Window`; row and control text 13px,
     muted labels 12px, the dialog titles 16px at 600.
   - Build, test, screenshot, busy pairs. Commit `ui: P10-M1 theme global and the rust-ui colours`.
2. **Lucide icons** (`src/ui/icons/*.svg` new: file-plus, folder-plus, scissors, copy,
   clipboard-paste, pencil, trash, panel-right, monitor, file, download, image, hard-drive, star,
   folder, chevron-right, chevron-down, x, plus, arrow-up, search, sliders-horizontal, info,
   arrow-left, arrow-right; `pane.slint` `ToolButton` and `Toolbar`, `app.slint` `SideRow` and the
   sidebar footer, the crumb chevron, the tab close button, `HeaderCell`'s sort arrow, the filter
   box's glyphs; README § License).
   - `ToolButton`: `in property <image> icon;` drawn colorized, in place of `glyph`; the eight
     buttons' `glyph:` lines become `icon: @image-url(...)`. The sidebar's `SideRow` gets an icon by
     `item.kind`. The chevron, close, plus, sort and filter glyph `Text`s become colorized `Image`s of
     the same size in the same slots.
   - Build, screenshot (the toolbar at both placements, `C:\traces\toolbar-pane.toml`), busy pairs.
     Commit `ui: P10-M1 Lucide icons for the chrome`.
3. **Dialog controls** (`theme.slint` gains `GhostButton`, `OutlineButton`, `PrimaryButton`,
   `Switch`; `app.slint` settings and About dialogs, the bookmark menu).
   - The std `Button`s become `OutlineButton` (Cancel, About's close) and `PrimaryButton` (OK);
     `CheckBox` rows become `Switch` (36 x 20, `Theme.primary` knob track when on, `Theme.border`
     off); labels at 500; 12 px between rows. `ComboBox` stays (writing a popup choice is a task of
     its own, deferred). Driver steps `settings set`, `settings ok`, `settings close`, `about close`
     call the same callbacks.
   - Build, test (the settings dialog's scripted test in the P6 soak: `settings open`, `set`, `ok`),
     screenshot. Commit `ui: P10-M1 dialog buttons and switches`.
4. **Inter** (after Jeremy's OK): `src/ui/fonts/Inter-{Regular,Medium,SemiBold}.ttf` and `OFL.txt`,
   the three `import` lines, `default-font-family: "Inter"`, README § License. Build, screenshot,
   busy pairs (text shaping is per row: this is the one task in the gate-safe set that touches what
   every row does). Commit `ui: P10-M1 Inter bundled`.
5. **The roomier bars** (after Jeremy picks the values): the tab strip, path bar, header, status bar,
   sidebar rows and footer, and the toolbar band take the canvas's heights, with `dnd.rs`'s constants,
   `main.rs` `window_layout`, `auto.rs`'s aims and `tools/soak-*.ps1`'s row and tile aims changed in
   the same commit; the list rows stay 22 and 32 px unless Jeremy chooses otherwise (§ Open
   questions). Build, test, the P5-M3 soak for 10 minutes, busy pairs. Commit `ui: P10-M1 bar
   heights`.
6. **Gate and close**: `target-p10m1` built `--locked` from a detached checkout of the PR head;
   `tools/gate-p10m1.ps1` on the P9-M4 wrapper's pattern with `target-p9m4` (3374f2b) as the
   reference, tag ab41, the defaults and `C:\traces\grid-thumbs.toml` (the tiles' restyle measured);
   the coordinating session is told before the gate build and hands the run to Jeremy; the record's § What landed, § Decisions, § Scripted
   checks, § Gate and § Console checks; CLAUDE.md's reference-build line.

### What landed so far (2026-10-21, tasks 1 to 3; PR #91, named P10-M1 on 2026-10-22)

- **Task 1, fcc285b**: `src/ui/theme.slint` with the `Theme` global (every token of § Settled first,
  plus `input`, an unchecked switch's track), imported by the three UI files; no `Palette` read is
  left outside it. The window sets `default-font-family: "Segoe UI Variable Text"` and
  `default-font-size: 13px`. The rows, the pane card and its ring, the tabs, the header cells, the
  crumbs, the status bar, the sidebar rows, the toolbar band, the splitters, the preview column, the
  menus and the dialogs take the tokens as § Settled first says. No constant in `dnd.rs`, `main.rs`
  or `auto.rs` changed; the soaks were not touched.
- **Task 2, d113789**: 32 Lucide-style SVGs under `src/ui/icons/` (one `d` path each, colorized by
  `Image.colorize`), drawn in the toolbar's eight buttons (`ToolButton.icon` in place of `glyph`),
  the sidebar's rows (a drive, a bookmark, and the places by their label: Home, Desktop, Downloads,
  Pictures, Music, Videos, OneDrive; any other place a folder), the sidebar's hide button, the crumb
  chevrons, the tab close, the sort arrow, the filter's clear and the path bar's mode glyph. README
  § License carries the Lucide notice. The Windows 11 menu's icon row and the preview's task boxes
  keep their Segoe Fluent glyphs.
- **Task 3, 862bca8**: `Btn` (outline, and `primary: true`) and `Switch` in `theme.slint`; the
  Settings dialog's nine check boxes are switches, its OK (primary) and Cancel and About's Close
  are `Btn`s; the rows' spacing is 8 px and the dialog 510 px tall (480 before) so its note still
  fits. `ComboBox` and `LineEdit` stay fluent (a popup choice of our own is deferred).
- **Task 4, 6e76950** (2026-10-22, after Jeremy's "Yes, download Inter and bundle it as task 4"): Inter 4.1's
  static Regular, Medium and SemiBold (`extras/ttf/` of the rsms/inter release zip, 412, 417 and 420 KB)
  under `src/ui/fonts/` with the release's licence as `OFL.txt`, three `import "fonts/..."` lines at the
  top of `app.slint`, `default-font-family: "Inter"` on the window, and the notice in README § License.
  The `P9-M5` comments in the three Slint files read `P10-M1`.
- **Task 5, 4fb6a0d** (2026-10-22, Jeremy's answer: "keep 22 and 32 px rows; raise the bars to the lead's
  values"): the toolbar band across the window 44 px (36), the tab strip 30 (26), the path bar 36 (32),
  the header 32 (24), the status line 28 (18), the sidebar's rows, headers and footer 32 (24, 26, 26);
  the in-pane toolbar and the list rows unchanged. In one commit with them: `dnd.rs` `LIST_TOP` 75 (63),
  `LIST_BOTTOM_GAP` 33 (23), `TAB_STRIP` 32 (28), `TOP_BAR` 44, `SIDE_FOOTER` 32, `TAB_H` 30, `CRUMB_H` 30;
  `main.rs`'s sidebar row heights and keyboard scroll step (32); `auto.rs`'s crumb-menu point
  (36 px above the list), the grip's centre (18 px above the list, was 14) and its comments;
  `tools/soak-p5m3.ps1`'s hovers at y 22 (was 18); CLAUDE.md's three figures. Checked by a scripted
  run before the commit: `coldrag 0 size 30` read back as `size 90 -> 60 px (drag)`, `colfit`, the three
  tooltips at y 22, `sidebar click 3` then `key Down` (row 4, Home), `crumbmenu` on both panes, a
  `dragover tab 1 0` (`spring armed: pane 1 tab 0`), `dragover strip` and `dragover side bottom`,
  `rclickempty` (the background menu shown and closed); then `tools/soak-p5m3.ps1 -Minutes 10` on it (2026-10-22, the machine otherwise idle): exit 0, 0 panics, 5 cycles started, 296 jobs done (0 failed), drops 73, drags dropped 0, menupicks refused 0, spring-loaded tabs 0, toolbar presses refused 0, zip jobs not done 0, picture failures 0, Office unreadable 0, alt text 0, waitpreview timeouts 0, worker panics 0; tooltips 15 (three per cycle at y 22), column drags read back as Size 90 -> 60 four times, fits 4, widths back at 120, 90, 140 five times; working set 135 to 240 MB over the ten minutes.

**Checks**: `cargo build --release` after each task; `cargo test --release` 270 of 271 after each,
the one failure `fileops::clipboard::tests::real_clipboard_round_trip_overwrites_clipboard` ("the
clipboard is in use by another program", twenty tries over a second), which fails the same way from
the unchanged main checkout at f2fa4eb and passed once earlier in the session, so it is the machine
(PowerToys Advanced Paste runs; no `twinfm` process was left, no window held the clipboard open at
the moment asked), not this branch. Screenshots of the scripted window (`SLINT_BACKEND=winit-software`,
`tools/shot.ps1`; Jeremy's system in dark mode, so every one is the dark board): the details list
with a selection and the cursor, two tabs with the filter and the preview column, the large-icon
grid with the settings dialog, both toolbar placements with the path entry and the sidebar hidden,
and the dialog with the switches. In-app busy pairs after task 1 against `target-p9m3` (eight runs,
`[busy] 10 s after split`, one session state, render p50 2.5 to 3.4 ms): reference 21.9 / 18.0 /
18.4 / 20.9 (median 19.7), candidate 20.3 / 23.0 / 19.2 / 19.2 (median 19.8): flat. After task 3 one
pair read 18.0 / 22.2 and the next four runs presented no frames (render n=6, interval 50 ms: the
session had changed state), so they decide nothing; tasks 2 and 3 change nothing the gate scenario
draws per row. The console triplet is still to run.

**Waiting on Jeremy**: the gate (task 6, his elevated console, handed over by the coordinating session).
Inter and the bar heights were answered 2026-10-22 (tasks 4 and 5 above).

### Gate

Run by Jeremy on 2026-10-22, finished 09:47 (tag ab41 for both settings, `C:\traces\gate-p10m1.ps1`,
the same file as `tools/gate-p10m1.ps1`, 18 min): `target-p10m1` (b8b8853, built `--locked` from a
detached checkout, Inter embedded) against `target-p9m4` (3374f2b, P9-M4's gate build, the last merged
milestone's), three rounds of reference, candidate, reference with `C:\traces\grid-thumbs.toml`,
candidate with it. Every run passed the wrapper's checks (no panic, the settings file read, the grid
line present).

| Run | WPR | In-app | Render p50 (ms) |
|---|---|---|---|
| p9m4-1 / 2 / 3 | 22.3 / 19.6 / 19.9 | 20.6 / 19.8 / 19.8 | 3.00 / 2.38 / 2.42 |
| p10m1-1 / 2 / 3 | 20.6 / 20.3 / 19.9 | 20.3 / 22.7 / 19.4 | 2.43 / 2.53 / 2.33 |
| p9m4-grid-thumbs-1 / 2 / 3 | 11.5 / 11.2 / 11.3 | 11.6 / 11.2 / 11.6 | 1.19 / 1.20 / 1.19 |
| p10m1-grid-thumbs-1 / 2 / 3 | 11.7 / 11.8 / 11.4 | 12.8 / 12.5 / 12.8 | 1.26 / 1.30 / 1.29 |

Medians: defaults 19.9 (reference) / 20.3 (candidate), grid-thumbs 11.3 / 11.7. No gap that counts in
either setting (no setting has every reference run below every candidate run, and neither median
differs by more than a point). Read like for like: the first reference run sat in the slower state
(render p50 3.00 ms against 2.33 to 2.53 for the other five defaults runs), so the defaults compare as
19.6 / 19.9 against 20.6 / 20.3 / 19.9, the candidate about half a point above; in the grid the
candidate's render p50 is 1.26 to 1.30 ms against 1.19 to 1.20, about 7% more per frame, and the busy
figure 0.4 of a point above. Two candidate defaults runs read above 20% (20.6, 20.3); the reference's
own runs read 19.6 to 22.3 in the same session. **Passed by the gate's rule**; the half point is the
plausible price of the hairline `Rectangle` in every visible row and of Inter's shaping, and it is
recorded here for the next reference to carry. Jeremy's word on the merge is the next step.

**The hairline experiment** (2026-10-22, after the gate, asked by the coordinating session since two
like-for-like candidate runs read above 20%): a build in which each row's hairline is the last pixel
of the row's own background (a hard-edged `@linear-gradient` on the existing `Rectangle`, stops at
95.45% for 22 px rows and 96.875% for 32 px rows, two `color` tokens for the stops) in place of the
child `Rectangle` per row, measured in-app against the gate's code, four pairs with the order swapped
(ref, cand, cand, ref, ref, cand, cand, ref), the gate scenario, a quiet machine, one session state
(render p50 2.34 to 2.55 ms throughout):

| Build | `[busy] 10 s after split` | Median | Render p50 (ms) |
|---|---|---|---|
| the gate's code (a child Rectangle per row) | 18.9 / 19.1 / 18.9 / 18.4 | 18.9 | 2.44 / 2.38 / 2.35 / 2.34 |
| the gradient variant | 18.9 / 20.5 / 20.0 / 18.6 | 19.5 | 2.47 / 2.54 / 2.55 / 2.45 |

The variant reads no lower (half a point higher by the medians, a tenth of a millisecond more per
frame: a gradient paint per row costs femtovg more than a solid 1 px rectangle), so the per-row
hairline is not the gate's half point and the code stays as gated. What remains of the half point is
Inter's shaping of every visible row's text against Segoe UI's, accepted. Nothing of the variant was
kept; `target-hairline` was removed.

### Console checks (for Jeremy, unelevated, after the gate)

The look was only ever seen in dark mode here (this machine's app mode); the light scheme is the
first thing to look at.

- **Light mode**: Settings > Personalization > Colors > Choose your mode > Light, then TwinFM: the
  white ground and cards, the `#F5F5F5` muted selection, the `#E5E5E5` hairlines, the near-black
  primary button in the Settings dialog, the grey ring on the cursor row; then back to dark, with the
  window open, to see the switch follow (`Palette.color-scheme`).
- **150% scaling**: the 44 px band, the 30 px tabs with their 2 px inset segment, the 36 px path bar,
  the 32 px header with the grips, the 28 px status line, the 32 px sidebar rows; the tooltips over
  the band; nothing clipped in the Settings dialog (510 px).
- **The layouts unequal** (Phase 6's rule): tabs in one pane only, the filter open in the other, the
  split on; then the in-pane toolbar (`toolbar = "pane"`), its 28 px row above the strip, the preview
  column on.
- **The rows**: a selection of several with the cursor inside and outside it, a cut (Ctrl+X) row at
  50%, the drop highlight on a row, a tab, a sidebar row and a crumb during a real drag from Explorer,
  the band in the grid at large and medium tiles, the rename editor (F2) in the list and in a tile.
- **The chrome's icons**: every toolbar button at both placements with its disabled state (nothing
  selected, an empty clipboard, This PC), the sidebar's icons for the drives, the places and a
  bookmark, the crumb chevrons and the dropdown, the sort arrow both ways, the filter's clear, the
  path bar's ellipsis and chevron.
- **The dialogs**: Settings (every switch flips and saves, Cancel discards, Escape), About (Close),
  the Windows 11 menu's icon row (still Segoe glyphs, by the kickoff's call), the password and
  replace questions (Windows' own, unchanged).
- **Daily use** from `target\release` for a few days: what the look gets wrong in use goes on the
  Phase 10 list.

### After the console check (2026-10-22)

Jeremy's results, his words (relayed by the coordinating session): "2-7 all seem to work as expected."
Check 1, light mode, he read as a TwinFM setting ("I don't see a personalization option"); it is
Windows' Settings app, and TwinFM follows the system's app mode. His three remarks, verbatim:

- "The highlight of a single line (in details) or large icon is difficult to see the first few times
  using it. Is there a way to make a highlighted item more visible?"
- "On folders that are configured with details view, I cannot click-and-drag the mouse cursor to
  highlight multiple items."
- "What's the search function? Can we consider adding a dedicated search box/button?"

**The first is fixed in P10-M1, f2a5d63**, colours and alpha only (no geometry, nothing new per row, so
no re-gate): the cursor ring on a row, a tile and the grid's name overlay is the Windows accent
(`Theme.cursor`, from `Palette.accent-background`, the system's accent lightened for dark mode by the
fluent style; 2 px as before, full strength instead of grey at 50%), and the selection is an accent
tint (`Theme.selection`: the accent at 22% in light mode, 28% in dark) on rows, tiles, the sidebar's
current row, the crumb dropdown's highlight, every drop highlight (a row, a tile, a tab, a sidebar
row, a crumb, the pane) and the band; hover stays the muted grey. The one-token change the kickoff
had left open (§ Open questions, dark mode's accent) is thus taken, for both schemes. One in-app
defaults run on the gate scenario: `[busy] 10 s after split` (the accent fix and task 7 together, this build against the gated `target-p10m1` in four swapped pairs, one session state): candidate 18.6 / 20.3 / 21.1 / 18.6 (median 19.5) against the gated build's 18.4 / 16.7 / 18.4 / 21.7 (median 18.4); no reference run sits below every candidate run, the medians differ by one point, in-app noise is two to five points per build, and three earlier single runs of the candidate on a busier machine read 21.1 to 24.4; nothing per row changed (two global brushes replace two), so the reading is noise, put to the coordinating session for the re-gate call, render p50 2.35 to 2.45 on both builds ms, in the
gate's range. Scripted screenshots in both schemes through a new override, `TWINFM_SCHEME=light|dark`
(`main.rs`, read at start, sets the fluent `Palette.color-scheme`; unset, the system decides as
always; `Palette` is re-exported from `app.slint` for it): the light look was seen on this machine for
the first time (white ground and cards, `#F5F5F5` hover, `#E5E5E5` hairlines, the blue ring and tint),
and the dark one as before with the blue in place of the grey.

**The other two are P10-M2 candidates** (§ Phase 10 candidates, from daily use), not started.

**Task 7, the theme switch, 6fa9b09** (2026-10-22, Jeremy's decision, his words: "Let's make light/dark a
selectable switch inside twinfm", folded into P10-M1 before the merge): the setting `theme` =
`"system"` (the default; Windows' app mode, followed live) | `"light"` | `"dark"` in `config.toml`
(`settings::ThemeMode`; a file without the key reads system, anything else too), the dialog's Theme
row (System, Light, Dark; a `ComboBox` under the View row; the dialog 550 px), applied on OK
(`[settings] dialog: theme system -> light`) and at start, live without a restart: light and dark
set the fluent `Palette.color-scheme`, system sets it unknown so the style resolves it from the OS
and follows it again (`main.rs` `apply_theme`). `TWINFM_SCHEME` keeps precedence for scripts (a
run under it ignores the setting); the driver's `settings set theme system|light|dark` goes
through the generic stage path, no new step. A scripted run switched system, light, dark, system
through the dialog, each logged, with the dialog captured open in the light scheme after the
switch: the fluent `ComboBox` and `LineEdit` follow the scheme; **the window's title bar does not**
(Windows draws it from the app mode; a `DwmSetWindowAttribute(DWMWA_USE_IMMERSIVE_DARK_MODE)` call
per setting is a small later item, § Phase 10 candidates). The same in-app run as the accent fix
covers it (above): colours and a dialog row, nothing per row.

**The reading, accepted by the coordinating session (2026-10-22): noise, no re-gate.** Nothing per
row changed, the gated build's own runs span 16.7 to 21.7 inside the same pairs, so a one-point median
gap over four runs sits inside the day's spread; the gap rule is for console triplets. **For the next
gates**: `target-p10m1` (b8b8853) stays the reference as the gated build of P10-M1; main's P10-M1 head
is b8b8853 plus the accent fix and the theme switch, both measured in-app only, which the next
reference carries as its caveat.

### Open questions

- **Row height and the bar heights**: answered 2026-10-22, the lead's recommendation taken: rows stay
  22 and 32 px; the bars take tab strip 30, path bar 36, header 32, status 28, sidebar rows 32, toolbar
  44 (task 5). The canvas's roomier values (36, 36, 40, 40, 32, 52, with 30 px rows) stay a density
  candidate.
- **Inter**: answered 2026-10-22, bundled (task 4; 1.2 MB in the exe).
- **Dark mode's accent**: taken on 2026-10-22 after the console check, for both schemes (the cursor
  ring and the selection tint, § After the console check).

## P10-M2: the search box and button

Jeremy's decision, 2026-10-22, from his P10-M1 console check ("What's the search function? Can we
consider adding a dedicated search box/button?") and his routing the same day: "add in the search as
another p10 task." It is the milestone after P10-M1, not a P10-M1 task: P10-M1 is gated and the
Phase 11 preview stream waits on its merge. **No code before the coordinating session says P10-M1 is
merged.**

The brief, to be settled first then: one search box in the chrome (where, settled against the
toolbar's two placements and the path bar; whether it moves the list's top decides the gate), a
button and a key; name search as today, nothing removed (the filter box's Ctrl+F behaviour and
Ctrl+Shift+F's recursive name search both reachable from it); and a content mode left as a hook for
Phase 11's P11-M1 content search (`docs/phase-11-plan.md` § P11-M1 on PR #93 names `csearch <pane>
<text>` and the box: the two designs are kept one, so P11-M1 adds a mode to this box rather than a
box of its own). Gate: the defaults triplet (new chrome, no per-row change) unless the box changes
the list's top, in which case `dnd.rs`'s constants and the driver's aims follow in the same commit
and a soak runs before the gate.

Go: Jeremy's word on 2026-10-22 after P10-M1 merged (main 8dc692a, then 0ab9a83 with the Phase 11
kickoff), relayed by the coordinating session: "let's proceed with the other items remaining in P10".
Branch `feat/p10-m2-search-box` from 0ab9a83.

### Settled first (2026-10-22, before any code)

**What is there today** (`pane.slint`, `main.rs`): Ctrl+F opens a 220 px `LineEdit` at the right end
of the pane's path bar, created only while `filter-open`; typing filters the listing live (150 ms
after the last edit); Return or Down applies the filter at once and gives the list the focus; Escape
or the clear glyph closes it; Ctrl+Shift+F runs a recursive name search for the box's text in a new
tab, or, with the box empty, opens it "armed" so that Return there starts the search. Nothing on
screen says any of this exists, which is Jeremy's remark.

**The box.** One box per pane, **always visible**, at the right end of the path bar where the filter
box opens today, inside the 36 px bar (a 32 px outline field in the look: the hairline, 6 px corners,
the card colour; a leading Lucide `search` icon; the placeholder "Filter, Enter to search"), 200 px
wide, 150 px in a pane narrower than 520 px (a pane property, never per row). The crumbs and the path
entry give up that width as they do today while the box is open. **It does not move the list's top**
(the bar stays 36 px; `dnd.rs` LIST_TOP and the crumb clip's width arithmetic are as they are), so the
gate is the defaults triplet and no soak is owed for geometry; one soak cycle is added anyway for the
new paths. The `filter-open` property stays as the "has text" state Rust and the driver set; the
`LineEdit` is no longer under an `if`.

**What typing, the keys and the button do.** Typing filters the listing live, as today. **Return runs
the recursive name search** for the box's text in a new tab (what Ctrl+Shift+F does; with an empty
box Return gives the list the focus). **Down** applies the filter at once and gives the list the
focus (today's Return). Escape clears the box and the filter and gives the list the focus. A
**search button** at the box's right end (Lucide `search` again, 24 px, ghost, the tooltip "Search
this folder and below (Ctrl+Shift+F)") runs the same search as Return and Ctrl+Shift+F; the clear
glyph shows left of it while the box has text. **Ctrl+F** focuses the box with its text selected, as
today; **Ctrl+Shift+F** searches as today, and with an empty box focuses it (the "armed" state goes:
Return always searches, so nothing needs arming). Nothing is removed: the live filter, the applied
filter, the recursive search and both keys are all reachable from the box, and the driver's `filter`
and `search` steps keep their meaning. Return's change (search, not apply) is the one behaviour
change, named for Jeremy's console check; the live filter has already applied by the time Return is
pressed, so nothing is lost by it.

**The hook for P11-M1.** The search button is where the mode goes: P11-M1 makes it a name-or-content
choice (a chevron beside it, or a toggle), `csearch <pane> <text>` sets the box's text and runs the
content mode, and the content rules (never a cloud location, the matching line appended to the
label) are P11-M1's; P10-M2 leaves the button a plain search so that P11-M1 adds a mode to this box
and never a second box (`docs/phase-11-plan.md` § P11-M1 reads "a second box, or a mode of the
filter box, settled first against the toolkit and the Phase 10 restyle": this is that settlement).

**Code, tests first.** A pure module `src/searchbox.rs`: `width(pane_w: f32) -> f32` (200, or 150
under 520), `on_return(text: &str) -> ReturnAction` (`Search(pattern)` for a trimmed non-empty text,
`FocusList` otherwise), `placeholder()`; tests for the three. `pane.slint`: the always-present box,
its icon, its clear glyph and its search button, `accepted` → `filter-return(text)` (new callback),
Down → `filter-now(text)` (new; the old apply) and the list's focus, the button → `search-pressed()`;
`app.slint` routes the three to Rust; `main.rs` `filter_return` (search or focus), `filter_now`
(today's `filter_apply`), `search_key` without the armed state. Driver: `searchreturn <pane>` (Return
in the box: `filter-return` with the box's text) and `searchbutton <pane>` (the button); `filter` and
`search` as before. `[search]` lines: `pane=N box Return: searches "<text>"`, `pane=N box Return:
empty, the list takes the focus`, `pane=N search button`; `[auto] searchreturn ...`, `[auto]
searchbutton ...`. Soak (`tools/soak-p5m3.ps1`): per cycle `filter 0 tree`, `searchreturn 0`,
`waitsearch`, the search tab closed, and `searchbutton 0` once with the box empty (the list takes the
focus, no tab); the report counts box searches and empty Returns. CLAUDE.md § Measuring's filter and
search paragraph gains the steps and lines. Screenshots: both toolbar placements, the box empty and
with text, a narrow pane.

**Gate**: `target-p10m2` from a detached checkout of the PR head against `target-p10m1` (b8b8853),
tag ab42, the defaults; the coordinating session is told before the build, the soak and any question
for Jeremy. Console checks: the box at 100% and 150%, typing then Return, Down, Escape, the button,
Ctrl+F and Ctrl+Shift+F with and without text, a search in a search tab, the path entry mode with
the box, a narrow pane.

## P10-M3: the P10-M1 leftovers

Jeremy's pick, 2026-10-22, his words (relayed by the coordinating session): "the P10-M1 leftovers
only". Three items and nothing more; it starts after P10-M2 merges, in a new branch from main, with
its own Settled first, a soak cycle where a step changes, and the gate (the defaults triplet; nothing
here touches a row).

1. **The native title bar's colour follows the theme setting**: the DWM dark-title-bar attribute
   (`DwmSetWindowAttribute`, `DWMWA_USE_IMMERSIVE_DARK_MODE`) set at start and on each theme change;
   system mode follows Windows as today (P10-M1 task 7's note).
2. **A popup choice and a text field drawn in the look**, replacing the fluent `ComboBox` and
   `LineEdit` where they remain: the Settings dialog's choices (view per pane, toolbar, theme) and
   its start-folder fields, the path bar's text entry, the rename editor and the bookmark title
   editor (the search box is already a field of the look since P10-M2). The driver's `settings set`
   path and the fields' `edited` and `accepted` callbacks are unchanged.
3. **The Windows 11 menu's icon row with the Lucide set**: `src/winmenu.rs`'s items unchanged, only
   the glyphs (Cut, Copy, Rename, Share, Delete, Paste) drawn from `src/ui/icons/`.

## Deferred to Phase 12 (roadmap)

Jeremy's routing of 2026-10-22: "add the click-and-drag to phase 12, or a roadmap list."

- **The rubber band in the details list** (his P10-M1 console check: "On folders that are configured
  with details view, I cannot click-and-drag the mouse cursor to highlight multiple items"): deferred
  since Phase 4; `src/band.rs` applies to the grid only; a press on a row starts a drag today, so the
  band would start from a press on empty space, or past a threshold on a row, settled first. Phase
  11's kickoff (PR #93) carries its own Phase 12 items; the Phase 11 close gathers them with this one.
