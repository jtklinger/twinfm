# Phase 16: a drag that says what it will do, more views, and search that keeps up

Written 2026-11-15, the day after Phase 15 closed and 0.6.0 went out (`phase-15-plan.md` § Phase 15 close).
Phase 16's goal, from Jeremy's kickoff on 2026-11-15: **make it plain, before the button is let go, whether a
drag will move or copy; add Explorer's other views; make plain-text content search as fast as the
regular-expression one and let an open search tab pick up new matches; and fix the small items from the
roadmap (the big-copy slowdown, the mouse's Back and Forward over the tab strip and scrollbars).** The items
this phase takes leave `docs/roadmap.md` when this plan merges (Jeremy's rule, 2026-10-26).

What Jeremy decided at the kickoff (2026-11-15):

- **The drag**, his words: "One item that bugs me is I never know if a file drag from one pane to the other is
  going to perform a move or a copy. How do I know the difference?" Asked whether he sees the "Move to" or
  "Copy to" label by the pointer while he drags: "Yes, but easy to miss". So the label works and is not
  enough. This item was never on the roadmap; it goes first.
- **Scope**: "The other 4 recommendations can be folded into phase 16." The four were: faster plain-text
  content search; the big-copy slowdown; Explorer's other views (extra large icons, small icons, list); the
  small mouse and search leftovers (Back and Forward over the tab strip, a text box or a scrollbar; a new
  match in an open search tab; the cursor outline after a click on a row's blank part, checked against
  Explorer).
- **The view keys**: "Match Explorer" (asked with the kickoff PR, 2026-11-15): Ctrl+Shift+1 extra large, 2
  large, 3 medium, 4 small, 6 details, which moves the three keys in use until now (1 details, 2 large, 3
  medium).
- **Not taken** (in the recommendation, not objected to): the search index (a phase of its own, with a design
  discussion first), the preview's gaps, the Slint fork's whole-row snap, accessibility.

The starting point is main at 2e5df94 (the 0.6.0 release; code as `target-p15close`, 4d335b7). What each item
rests on today, from reading the code (three read-only subagents, 2026-11-15):

- **A drag's effect** (`src/dnd.rs` `decide_effect`, line 242): Ctrl or Alt held is a copy, Shift a move, else
  a move on the same volume and a copy onto another (Explorer's rule); into an archive always a copy. The
  right button opens Move here / Copy here / Cancel on the release (`dt_drop`, `drop_menu`). `DragOver`
  reckons the effect again every time (about every 50 ms) from the keys held, and pushes out only what
  changed: the drop description on the drag's data object ("Move to <folder>" / "Copy to <folder>", the
  small label Windows draws by the pointer, P5-M3), and the drop highlight (`main.rs` `drop_highlight`).
  **The highlight knows the spot and not the effect**: it takes `(pane, Mark)`, and every spot (a row, a
  tile, the whole list, a tab, a crumb, a sidebar row) is drawn the same, `Theme.selection` with a
  `Theme.cursor` border. So a key pressed mid-drag changes the small label and nothing else. A note can
  reach the pane under the pointer from inside a drag (`drop_note`, through `invoke_from_event_loop`, as
  the archive refusal's does).
- **A setting with named choices**: `toolbar` is the pattern (`settings.rs` the field, an enum with
  `from_name` / `name` / `index`, the `Dialog` field and `fields()`; app.slint's `dlg-` property, the
  dialog's `Choice` row and `settings-stage`; `main.rs` at open, read back, OK and start; the driver's
  `settings set`).
- **The mouse's Back and Forward**: Slint does not bubble pointer events, so every `TouchArea` that takes a
  press checks for the two buttons itself. Rows, the pane's background, the path bar, the crumbs, the header
  cells, the toolbar (P7-M4) and the "+" do. `TabButton` (pane.slint) handles left, middle and right only;
  `VScrollBar` and `HScrollBar` (theme.slint) the left button only; a text box is Slint's `TextInput`, which
  takes the pointer itself and has no hook.
- **`own_results`** (`src/fileops/mod.rs` 823): after a job, each result the shell reported is tested against
  every top-level item's parent and then against every item, with `same_path`, which lowercases into new
  strings and, on a mismatch, canonicalises both paths on disk. N items and R results cost R x N comparisons
  with file system calls among them. The extract job avoids the function for this reason
  (`fileops/archive.rs` 642). Only the end-to-end worker test covers it.
- **Plain content search** (`src/fs/content.rs`): the needle is folded once (`fold`: each character's full
  lowercase form, a final sigma read as sigma); **each file's whole text is folded into a new string**
  (`first_match`), with a line-by-line fallback when a character's lowercase form has another length. The
  `re:` path (`first_match_regex`, P15-M4) searches the text as it is. Phase 15 measured 968 ms against 86
  ms on a 100 MB tree. The regex crate's case-insensitive match puts Σ, σ and ς in one class, so the sigma
  rule holds there; where the two differ is characters whose lowercase form is longer than one character
  (today a plain `i` matches `İ`; `(?i)i` does not). One test pins each (content.rs 259, 261).
- **A search tab** is never watched (`watch_tabs`: a results tab is watched only if it `follows()`, true for
  sizes views alone). Every 5 s a finished search tab in front has its rows on screen statted again
  (`restat_results`, P11-M2): the gone removed, the changed updated; nothing looks for new files. Since
  P14-M2 a subtree watch hands every record of a window with its path under the root (`Event::Subtree`'s
  `deep`; none on an overflow or past 10,000 records), and the sizes view applies them (`sizeview::plan`).
  The watch cap is 8 for the whole window. `NameMatch` tests a file's own name, so a created or renamed path
  can be tested without a walk.
- **The views** (`src/icons.rs`, pane.slint): two tile sizes, `LARGE` (96 x 136, a 96 px icon) and `MEDIUM`
  (80 x 107, 48 px), told apart by a bool (`TileGeom::of(medium)`, `set_left_medium`, the host's ternaries);
  `FileGrid` and `FileGridMedium` are copies that differ in constants only, because nothing in a row or tile
  may depend on a runtime property (P3-M1, P4). `FileListHost` chooses among nine list components. The
  shell's icon lists go up to jumbo, so a 256 px icon is there to ask for. Hit testing, the band, hover,
  paging, the menu point and the arrow keys need only a tile's width and height; the rename editor, the
  new-item placeholder and the cursor tile's whole-name overlay assume **the name under the icon**, and
  `GridGeom::icon_pos` centres the icon across the tile. Everything assumes **rows across, scrolled up and
  down**: the grid rows in a `ListView`, `content-y` in the strip, the overlays, the hit test, the band,
  hover and paging. Explorer's List view runs down each column and scrolls sideways, which none of that
  does.

## The milestones

Six milestones: two streams and a coordinating session, as Phases 11 to 15 ran. Within a stream the order is
the order of work.

**The drag and views stream** (one session; touches `src/dnd.rs`, `src/settings.rs`, `src/icons.rs`,
`src/band.rs`, `src/toolbar.rs`, the drop, view and settings paths in `src/main.rs`, pane.slint's grids,
host and overlays, app.slint's drop properties and settings dialog, `src/auto.rs`, `tools/soak-p5m3.ps1`):

1. **P16-M1, a drag that says what it will do.**
   - **The cue**: while a drag is over a spot that takes it, TwinFM shows what the drop will do in its own
     look, where the eye already is: a badge on the highlighted target reading **"Move here"** or **"Copy
     here"**, with how to get the other ("Ctrl copies" / "Shift moves") in the muted colour beside it, and
     the same words in that pane's status line with the count and the folder ("Drop moves 3 items to
     Documents"). One badge for the window, placed by the spot (a list, a row or tile, a tab, a crumb, a
     sidebar row), never an element in the rows. It changes the moment Ctrl or Shift is pressed or let go,
     since `DragOver` already reckons the effect each time: the effect joins the spot in `drop_highlight`,
     pushed on change only. Move and copy also differ in colour (the badge's, and the highlight's border),
     so the difference shows without reading.
   - **Settled first, with Jeremy, before it is built**: how the badge looks and where it sits. The stream
     shows him two or three mock-ups (screenshots of a scripted hover) and builds the one he picks. The look
     is his to decide.
   - **The default is a setting**: `drag_default`, in the settings dialog as "Dragging files": **As Explorer**
     (the default: move on the same drive, copy onto another), **Always copy**, **Always move**, **Always
     ask** (the Move here / Copy here / Cancel menu on every drop, the right button's menu reused). Ctrl and
     Shift still force a copy or a move under every choice, and the right button still asks. It applies to
     every drop onto TwinFM, from its own panes and from other programs; a drag out of TwinFM onto another
     program follows that program's rule, which TwinFM cannot change. Into an archive stays a copy.
   - The Windows label by the pointer stays as it is.
   - Driver: `dragover` and `dropfiles` as today (their mode `auto` takes the setting), `settings set
     drag_default explorer|copy|move|ask`, and a `keys` word on `dragover` to hover with Ctrl or Shift held.
     Lines: `[dnd] cue: Move here | Copy here | none (pane N | sidebar)` on change, `[settings] dialog:
     drag_default <old> -> <new>`, `[dnd] drop menu: asked by the setting`. Tests: `decide_effect` under each
     setting with each key (pure); the badge's text and the status line's for one item, several, a move and a
     copy (pure); the settings round trip with an old file. Soak: per cycle a hover with the cue read back as
     Move, the same with Ctrl as Copy, a drop under "Always copy" onto the same drive that copies, the
     setting put back. Gate: the defaults triplet (the scenario never drags: it proves the rest untouched,
     and the record says so). Console checks, unelevated: a drag between panes on one drive and across two,
     Ctrl and Shift pressed mid-drag, each of the four settings, a drag from Explorer into TwinFM, a drop on
     a tab, a crumb and a sidebar row.
2. **P16-M3, extra large and small icons.**
   - **Extra large icons**: a third tile, a 256 px icon (the shell's jumbo list) with the name under it, on
     the grid machinery as large and medium are: a copy of `FileGrid` with its constants, thumbnails at that
     size through the same workers.
   - **Small icons**: a 16 px icon with the name beside it, in cells across the pane, rows scrolled up and
     down (Explorer's Small icons). The same machinery with two additions: the icon at the cell's left
     (`GridGeom::icon_pos`), and the rename editor, the new-item placeholder and the whole-name overlay
     beside the icon, not under it.
   - **The bool becomes a size**: `medium` is replaced by a tile size of four (`TileGeom`, the host's
     arithmetic, `set_left_medium` and the right's, `settings::View`, the saved `tile_left` / `tile_right`
     strings `"xlarge"`, `"large"`, `"medium"`, `"small"`), chosen once per pane as today. The toolbar's View
     menu, the settings dialog's View row and the keys list them in Explorer's order: Ctrl+Shift+1 extra
     large, 2 large, 3 medium, 4 small, and details moves to Ctrl+Shift+6 as in Explorer (Jeremy:
     "Match Explorer"; it renumbers the three keys of today, 1 details, 2 large, 3 medium). List takes
     Ctrl+Shift+5, Explorer's, when P16-M5 lands.
   - **Settled first**: what a 256 px strip costs in memory and per composed grid row (a trace of the
     scripted scroll before anything else is built), and the drag image's size out of each new tile.
   - Driver: `view xlarge|large|medium|small|details` (`icons` stays as large). Lines: the `[view]` lines
     name the two new views; `[icons] list geometry` as today. Tests: the tile arithmetic, hit test, band,
     paging and hover at the two new sizes (pure); the settings round trip (an old file's `"icons"` and
     `"medium"` load as before). Soak: per cycle each new view switched to, a drop onto a tile, a band, a
     cancelled drag out, back to details. Gate: the defaults triplet, and a triplet each with
     `C:\traces\grid-xlarge.toml` and `C:\traces\grid-small.toml` (read-only; both panes on that view), each
     against the reference with `grid.toml`. Console checks: each view in Downloads and Pictures with
     thumbnails on and off; rename, new folder and a long name in each; the keys.
3. **P16-M5, the List view.**
   - Explorer's List: a small icon and the name, filling each column from top to bottom and then the next,
     scrolled sideways. **This is the phase's large item and new machinery**: a container scrolled along x,
     an index that runs down columns, and x-axis forms of the hit test, the band, hover, paging, the menu
     point and the auto-scroll. Column width from the longest name on screen, as Explorer.
   - **Settled first, by a spike**: whether a sideways list of 40,000 entries holds the gate (a throwaway
     build, an in-app pair and then a triplet against `grid-small.toml`'s run). If it does not, the
     milestone stops there, the record says why, and the view goes back to the roadmap; small icons covers
     most of the need.
   - It comes last in its stream and builds on P16-M3's size enum and beside-the-icon overlays. Driver,
     lines, tests, soak cycle and console checks are written into its section once the spike has answered.

**The search and files stream** (one session; touches `src/fileops/mod.rs`, `src/fs/content.rs`,
`src/fs/watch.rs`, `src/fs/enumerate.rs`, `src/model.rs`, the search, watch and restat paths in
`src/main.rs`, pane.slint's `TabButton`, theme.slint's scrollbars, `src/auto.rs`, `tools/soak-p5m3.ps1`):

4. **P16-M2, the small items.**
   - **The big-copy slowdown**: `own_results` looks its results up in sets built once (the items' parents,
     which are mostly one folder, and the items, each by the rule `same_dir` uses), and canonicalises only on
     a miss, against sets canonicalised once. The results are the same; the time is linear. A test of its
     own (pure, 20,000 items of one folder, with a time bound a quadratic loop cannot meet, and the cases
     the end-to-end test covers today: a copied folder's contents not reported).
   - **The mouse's Back and Forward** over a tab (`TabButton`) and over the scrollbars (`VScrollBar`,
     `HScrollBar`) go back and forward in that pane, as over the rows. **A text box**: checked first whether
     Slint's `TextInput` lets a `TouchArea` over it pass the two buttons without taking the text cursor's
     clicks; if not, it stays ignored there (as P6-M3 left it on purpose over the path entry and the rename
     editor) and the record says so.
   - **The click on a row's blank part, against Explorer**: a console check for Jeremy (where the dotted
     focus outline goes after the click, and what Ctrl and Shift do there, in Explorer's details view beside
     TwinFM's). The code changes only if he finds a difference he wants followed.
   - Driver: `backclick <pane> tab|scroll` and `forwardclick` the same. Lines: `[nav]` as today. Soak: per
     cycle a `backclick` over a tab and over the scrollbar, counted. Gate: the defaults triplet (the
     scenario's `drag` steps press the scrollbar, whose pointer handler gains two tests). Console checks: the
     side buttons over a tab, the list's scrollbar and the preview's; a move of a few thousand files out of
     one folder with the status line watched after Windows' window closes.
5. **P16-M4, plain content search on the fast engine.**
   - A plain text becomes an escaped, case-insensitive expression on the engine `re:` already uses
     (`regex::escape`, `model::compile`), so no file's text is copied to be searched; the row's line comes
     from the match's own place.
   - **Settled first**: what changes in what matches. The sigma rule holds (the engine puts Σ, σ and ς in
     one class; the test stays). Characters whose lowercase form is longer than one character differ (a
     plain `i` finds `İ` today and would not). The stream lists every such case it can find, with the
     engine's answer beside today's, and **Jeremy sees the list before the old path is removed**; the lead's
     lean is to take the engine's answers (they are what Explorer's and most editors' searches give).
   - The live filter and the name search are untouched (names are short; nothing to gain).
   - Measured with `tools/time-p15m4.ps1` (the 100 MB tree): plain text before and after, against `re:`.
     Tests: today's content tests kept, the differing cases pinned to the answer chosen. Gate: the defaults
     triplet (the scenario searches nothing; the record says so). Console checks: a content search of the
     repo and of Documents for a word, timed by the `done` line against 0.6.0's.
6. **P16-M6, a search tab picks up new matches.**
   - A finished **name search** in a folder tree takes a subtree watch on its root, as the sizes view does,
     and each window's created and renamed-to paths are tested against the tab's matcher; a match is statted
     and added in its sorted place, a removed or renamed-away path's row goes at once (the 5 s re-stat stays
     for what the watch does not say). Hidden folders and links are left out as the walker leaves them out.
   - **A content search** follows too: a created or modified file is read again on a worker
     (`content::search_file`, the same caps and cloud rules) and its row added, updated or removed. Settled
     first whether that is cheap enough under a build or a sync writing many files; if not, content searches
     keep F5 and the record says so.
   - **Limits**: the window's eight watches are shared, so a search tab takes one only while it is in front
     or while a slot is free; a window past the record cap, or the second overflow in a minute, stops the
     following and the status line says "Not following changes: F5 searches again" (the sizes view's rule).
     Never for a search rooted on This PC, in a cloud location or inside an archive. A search of a whole
     drive is watched like any other and will often hit the cap; that is the honest limit until there is an
     index.
   - Driver: no new step (`touch`, `rmfile`, `mvfile` make the changes; `waitwatch` covers the window).
     Lines: `[search] pane=N gen=G watching <root>`, `[search] pane=N window: R records, A added, D removed,
     in T ms`, `[search] pane=N not following changes (<why>)`. Tests: a window's records against a matcher
     into edits (pure); a real watched tree with a file made, renamed into and out of the pattern, and
     deleted. Soak: per cycle a search tab left open while a matching file is made and removed, the two
     edits counted. Gate: the defaults triplet (no search tab in the scenario; the record says so). Console
     checks: a search for `*.pdf` in Downloads with a PDF saved from the browser while it is open; the same
     tab in the background; a content search with a file edited in Notepad.

**The coordinating session** (this one) serialises the machine, hands each gate to Jeremy, merges on his
word, keeps `docs/roadmap.md`, and runs the long soak of the merged code at the phase's end (60 minutes of
`soak-p5m3.ps1`, compared with Phase 15's 244 to 251 MB private at 100%; the display scale recorded with
it). No release comes first: 0.6.0 is out and main is its code.

Where the streams meet: `src/main.rs` (different functions), `src/auto.rs` (new steps), pane.slint (the
grids and overlays against `TabButton`), `tools/soak-p5m3.ps1` (a cycle each) and `CLAUDE.md` § Measuring.
The second to merge takes main in first; the coordinating session orders the merges (the lead's
expectation: M1, M2, M4, M3, M6, M5).

Not in Phase 16, on the roadmap: the search index, the preview's gaps, the Slint fork's whole-row snap and
upstreaming, Explorer's Tiles and Content views, the scrollbars' other items, the owed checks, accessibility.

Each milestone brings its driver steps and stderr lines (in `CLAUDE.md` § Measuring), tests for its pure
parts, and its soak cycle in `tools/soak-p5m3.ps1`. Every item in a milestone is a commit of its own, and the
record says per item: done, checked and found right, or left with the reason. Console checks after each
triplet, unelevated, and each check's line says where on screen the thing is.

**The gate is re-run after every milestone** under the protocol of 2026-10-12, as Phase 15 ran it
(`phase-15-plan.md` § The milestones has it in full): `tools/wpr-triplet.ps1` elevated at the console by
Jeremy, three runs per binary, interleaved, compared by median, each run's frame interval and render p50
read first. The reference for the first Phase 16 gate is **`target-p15close`** (4d335b7); after each merge
the new milestone's gate build becomes the next reference. **Tags continue from ab84.** One wrapper per
milestone, and two milestones' triplets in one sitting where they are ready together (Phase 15's lesson).
The rules for two streams hold (every stream reports to the coordinating session before any cargo run, gate
build, soak, TwinFM window or hand-over and waits for its reply; one soak at a time; tests run and read,
then the commit, as two steps; CI waited for before a merge; merges without `--delete-branch` from a
worktree session; `target\release` rebuilt only with Jeremy's window closed). The architecture rules from
Phase 1 and P12-M3 hold, and nothing per frame or per row without a trace first.

## Calls made in this kickoff, for Jeremy to check

Decisions Jeremy did not make and the lead settled, each reversible before its milestone starts:

- The drag item is both a louder cue and a setting (the kickoff's options A and B); "always ask" is one of
  the setting's choices, not the default. The default stays Explorer's rule.
- The cue is a badge on the drop target and a line in that pane's status bar, move and copy in different
  colours, with the key that gives the other named beside it. Its look is shown to Jeremy as mock-ups before
  it is built.
- The setting applies to every drop onto TwinFM, from other programs too; Ctrl, Shift and the right button
  work as today under every choice.
- Small icons is Explorer's Small icons (cells across, scrolled up and down); **List is a milestone of its
  own, last, behind a spike**, and goes back to the roadmap if it cannot hold the gate. Explorer's Tiles and
  Content views are not taken.
- Plain content search takes the regex engine's answers where the two differ, after Jeremy has seen the list
  of differences.
- A search tab follows changes through a subtree watch, name searches for certain and content searches if
  the measurement allows; a search of a whole drive will often stop following and say so.
- The mouse's Back and Forward over a text box is done only if Slint allows it without touching the text
  cursor's clicks.
- The click on a row's blank part is checked against Explorer by Jeremy at the console; no code changes
  unless he asks.
- Two streams: the drag and views (M1, M3, M5) and the search and files (M2, M4, M6), split by files; the
  long soak at the phase's end with the coordinating session.

## Open questions

Each brief settles first: the badge's look and place, with Jeremy (P16-M1); what a 256 px strip costs in
memory and per composed row (P16-M3); whether a sideways list of 40,000 entries holds the gate (P16-M5);
whether a `TouchArea` over a `TextInput` can pass the side buttons (P16-M2); the list of matches that change
on the regex engine (P16-M4); what following a content search costs while many files are written (P16-M6).

## P16-M3: extra large and small icons

Branch `feat/p16-m3-icon-sizes`, the drag and views stream, cut from main at 3e4c201 (P16-M1 not yet
merged then).

### Settled first: what a 256 px strip costs, and the drag image out of each new tile

Both views were built first on the existing grid machinery (they are needed to measure anything), then
measured in-app with `tools/measure-p16m3.ps1` (2026-11-15, unelevated, harness-launched, the machine
quiet by the coordinating session's hold, every run at 60 Hz: frame interval p50 16.4 ms; Narrator off,
display scale 100%, no other TwinFM window open). No WPR trace was taken: the in-app figures were far
enough from a cost to settle the question, and the console triplets below decide the gate.

**Scrolling** (the gate's scenario on `C:\traces\bigdir`, 40,000 files, both panes on the view, two
rounds with the order swapped in the second):

| View | In-app busy, % | Render p50, ms | Per composed grid row, µs | Peak private, MB |
|---|---|---|---|---|
| Large (`grid.toml`) | 10.3, 10.0 | 1.34, 1.31 | 67 | 167 |
| Extra large (`grid-xlarge.toml`) | 9.1, 6.2 | 0.95, 0.94 | 93 | 172 |
| Small (`grid-small.toml`) | 9.7, 9.2 | 1.93, 1.93 | 15 | 165 |

A 256 px grid row takes 93 µs to compose against a large row's 67 µs and is composed less often (a
row is 296 px of scroll, not 136), so per pixel scrolled it costs about half; the strips add 5 MB.
Small icons compose a row in 15 µs but draw more per frame (about 37 rows of cells with a text each
against a large grid's five rows of tiles): render p50 1.93 ms against 1.31, with the busy still under
large's. Neither view costs more than large icons while scrolling.

**Memory with thumbnails** is where 256 px costs: a thumbnail is 256 x 256 x 4 = 262 KB at 100% (590
KB at 150%), measured as reckoned (1,372 held: 497 MB private; 1,822 held: 608 MB), and the table's
only limit was a count of 20,000 (set when a thumbnail was 4 KB), which would be 5 GB. So the table
got a budget in bytes (below). Measured at the budget: 3,600 generated pictures paged through in
extra large icons with thumbnails on, then Home and forty pages again: at most 3,043 thumbnails in
760 MB, 309 handed back each time the budget was reached (927 in all), the table at 3,603 images
throughout (a returning thumbnail takes its old index), peak private 934 MB (about 165 MB of base and
the 768 MB), exit 0.

**The drag image**: out of an extra large tile, the large tile's image (128 px with the tile's picture
fitted down to 96 px; a 288 px image would hide what it is dragged over); out of a small tile, the
details list's image (96 px with the shell's 64 px icon; a 16 px icon is too small to drag).

### What landed

- **Extra large icons**: a 260 x 296 px tile with the shell's 256 px icon (its jumbo list) and the
  name under it on up to two lines, as the large tile; thumbnails at 256 px through the same workers
  and table when thumbnails are on.
- **Small icons** (Explorer's Small icons): 220 x 22 px cells across the pane, rows scrolled up and
  down, a 16 px icon at the cell's left with the name beside it on one line, elided. The rename box
  and the new item's placeholder sit beside the icon there; the cursor tile's whole-name overlay is
  not shown (Explorer elides too).
- **The bool became a size of four** (`icons::TileSize`: extra large, large, medium, small), chosen
  once per pane as before: two more copies of the grid component with their constants
  (`FileGridXL`, `FileGridSmall`), picked by the host outside any `for`; no tile reads a runtime
  property. Hit testing, the band, hover, paging, the menu point and the arrow keys needed nothing
  but the tile's width and height.
- **Explorer's keys and order**: Ctrl+Shift+1 extra large icons, 2 large, 3 medium, 4 small, 6
  details (5 is kept for List). The toolbar's View menu and the settings dialog's View row list them
  in that order. **This moves the three keys of 0.6.0** (1 was details, 2 large, 3 medium): 2 and 3
  are unchanged, details went from 1 to 6. Saved as `tile_left` / `tile_right` `"xlarge"`, `"large"`,
  `"medium"`, `"small"`; an older file reads as it did.
- **A memory budget for thumbnails** (not in the kickoff; the measurement above forced it): 768 MB,
  beside the count of 20,000. Past it the thumbnails drawn longest ago give their pixels back, down
  to nine tenths of the budget, never the 64 drawn most recently; each keeps its place in the table
  and is fetched again (from Windows' thumbnail cache) when its row is next on screen. 768 MB is just
  above what the 20,000 cap allows large tiles at 100% (737 MB), so at 100% nothing changes for the
  views of 0.6.0; memory can only fall. At display scales above 100% it also bounds large tiles,
  which the count alone allowed up to 1.7 GB at 150%.
- **End and Home in a grid of tall tiles**: after a jump of more than a viewport Slint's list starts
  a whole grid row at the top, so End left the cursor's extra large tile nearly off the list (the
  same shortfall was always there and unseen with 22 px rows). The list is aimed once more a frame
  later.
- Driver: `view xlarge|large|medium|small|details`. Lines and the rest in `CLAUDE.md` § Measuring.

### Found on the way

- **A request loop the measurement found** (fixed before any PR): the icon workers remember every key
  they have rendered and refuse it a second time. A thumbnail asked again after giving its pixels back
  was therefore skipped, and the window asked without end (three threads busy, the window frozen
  after Home in the 3,600-picture run). A delivered thumbnail's key is now given back to the workers;
  the window's own set of keys in flight still stops a key being asked twice at once. Reproduced in 30
  s with a 20 MB budget, gone in the same run after the fix, and covered by a worker test.
- **One shared blank** stands in for a thumbnail whose pixels are away, in place of a resampled blank
  cached per thumbnail (which would have grown as the budget shrank what it held).

### Tests

`cargo test --release`: `test result: ok. 366 passed; 0 failed; 2 ignored` (364 on main at 3e4c201; two
new tests), on the branch's last code (57b5a9e). The suite ran five times on this branch: the first had
one failure, in a new assert of this milestone whose expectation was wrong (a point below the window
reads as outside it, not as the pane), fixed and the whole suite run again; the other four passed. No
test was run twice for flakiness.

New: `icons::the_four_tile_sizes` (the size's index and name each way, an older file's value, each
tile's icon place and name box, the strips' icon positions for small cells and for extra large tiles
at 150%, a screenful of each), `icons::thumbnails_over_the_budget_give_their_pixels_back_and_return_at_their_index`
(the least recently drawn go, the arrival stays, a returning thumbnail takes its index and marks its
strips, an icon never counts, and the most recently drawn stay under any budget); added to existing
tests: the hit test, the band, the hover and paging by grid rows at both new sizes, the drag image
out of an extra large tile, the settings round trip for the two new sizes and an older file, the View
menu in Explorer's order with its keys and commands (and its five items in More when View does not
fit), and a delivered thumbnail rendered a second time by the worker pool where a delivered icon is
skipped.

### Scripted checks

`tools/check-p16m3.ps1` (2026-11-15, the branch's release build, no settings file, the software
renderer, `TWINFM_THUMB_MB=1`), one scripted run in `%TEMP%\twinfm-p16m3-check\`, exit 0, no panic.
From its stderr (the scratch folder's path cut to `...`):

```
[view] pane=0 details -> extra large icons (3 tiles to a row, 19 grid rows), scroll 0 -> 0 (entry 0 first)
[dnd] Drop at pane 0 row 1 (...\work\Beta) keys=0x8 allowed=0x3: 1 file(s), effect copy (reported to the source: copy)
[fileops] done id=1 pane=0 tab=1 Done: Copied 1 item
[dnd] drag image 128x128 px, 96 px icon, 1 item(s), icon from the tile, composed in 4.1 ms: InitializeFromBitmap = 0x00000000
[band] pane=0 done: 1 selected, cursor 55
[view] pane=0 extra large icons -> small icons (4 tiles to a row, 14 grid rows), scroll 0 -> 0 (entry 0 first)
[dnd] Drop at pane 0 row 1 (...\work\Beta) keys=0x8 allowed=0x3: 1 file(s), effect copy (reported to the source: copy)
[fileops] done id=2 pane=0 tab=1 Done: Copied 1 item
[dnd] drag image 96x96 px, 64 px icon, 1 item(s), icon from the worker, composed in 1.0 ms: InitializeFromBitmap = 0x00000000
[band] pane=0 done: 2 selected, cursor 52
[view] pane=0 small icons -> large icons (10 tiles to a row, 6 grid rows), scroll 0 -> 0 (entry 0 first)
[view] pane=0 large icons -> medium icons (12 tiles to a row, 5 grid rows), scroll 0 -> 0 (entry 0 first)
[view] pane=0 medium icons -> details (12 tiles to a row, 5 grid rows), scroll 0 -> 0 (entry 0 first)
[settings] dialog: thumbnails false -> true
[view] pane=0 details -> extra large icons (3 tiles to a row, 19 grid rows), scroll 0 -> 0 (entry 0 first)
[view] pane=0 extra large icons -> small icons (4 tiles to a row, 14 grid rows), scroll -5008 -> -264 (entry 48 first)
[view] pane=0 details -> extra large icons (3 tiles to a row, 50 grid rows), scroll -0 -> 0 (entry 0 first)
[icons] thumbnail memory at its cap (1 MB): the 1 drawn longest ago gave their pixels back, asked again when shown
[settings] dialog: view_left details -> xlarge
[view] pane=0 details -> extra large icons (3 tiles to a row, 19 grid rows), scroll 0 -> 0 (entry 0 first)
[icons] +0 requests +0 replies (total 276 / 276), table 257 images (65 thumbnails in 8 MB, 192 given back)
[auto] scrollpos pane=0 content-y -14184.0 current-row 149
```

Read from it: every view is reached by its key and by the dialog; in each new view a drop onto a folder
tile copied, a drag out carried the image settled above and was cancelled, and a band from empty space
selected what it touched (the last tile of the short extra large row; two small cells); End in extra
large left the list at -5008 (616 px of viewport less nineteen rows of 296: the last row whole) and at
-14184 in the 150-picture folder (fifty rows). The budget phase: with a 1 MB budget and the 64 most
recently drawn kept, 150 pictures were paged through, Home, End: 192 thumbnails gave their pixels back,
the table stayed at 257 images while 276 requests were answered (the 19 beyond the table's images are
thumbnails that returned at their own index), and the run ended (the request loop found by the
measurement would have hung it at Home). Nine pictures were taken and looked at: `xlarge.png`,
`small.png`, `xlarge-rename.png`, `small-rename.png` (the box beside the icon), `xlarge-newfolder.png`,
`small-newfolder.png`, `xlarge-thumbs.png`, `xlarge-thumbs-end.png` (the pictures framed, the last row
whole after End), `small-thumbs.png`.

### Soak

`tools\soak-p5m3.ps1 -Minutes 10` on the branch's release build (57b5a9e), 2026-11-15, display scale
100%, harness-launched, the other stream holding: ran 10.1 min, window closed by the script, exit code
0, 0 panicked lines; 228 jobs done (failed 0, cancelled 0), soak refusals 0, waitops timeouts 0,
menupicks refused 0, every "must be 0" count 0.

```
extra large and small icons: medium to extra large 3, extra large to small 3, small to medium 3 (one each per cycle); bands done with 1 selected on tile 0: 6 (two per cycle); 96 px drag images (the list's, and out of small tiles): 9
grid: switched on 8, off 8; tile drops: 12 (four per cycle: large, medium, extra large and small); 128 px drag images: 9 (left-button drags started: 15); bands: 12 started, 6 done with 2 selected (two per cycle), 0 cancelled
medium grid: switched on 6, off 6; 80 px drag images: 3; bands done with the cursor on tile 0: 6 (two per cycle); Shift+Right from the band's cursor: 3
right-button drags: 6; drags cancelled: 21, dropped: 0 (must be 0); drag loops that found the state busy: 0; drag images set: 21
working set: start 155.8 MB at 5 s, end 215.3 MB at 601 s; private: 165.4 -> 222.4 MB
```

Three full cycles. In each the right pane went from medium tiles to extra large, to small and back,
with a drop onto tile 0, a cancelled drag and a band on tile 0 in both new views (the six bands of one
tile), and the medium cycle's own band and Shift+Right after it were as before. The 128 px drag images
are nine where they were six (three more out of extra large tiles), the 96 px ones include the three
out of small tiles, and the 21 scripted drags all returned with none dropped.

### Gate

Three triplets, to be run by Jeremy from the coordinating session's hand-over (tags continue from
P16-M1's and P16-M2's): the defaults against the reference; `C:\traces\grid-xlarge.toml` against the
reference with `grid.toml`; `C:\traces\grid-small.toml` against the reference with `grid.toml`. The two
settings files were made for this milestone (read-only, both panes on that view).

What the defaults scenario meets of this milestone: the host's `tile` is an int where it was a bool
(the same per-pane ternaries, evaluated when the view changes), the icon table's `fitted` sets one
counter per icon it hands a strip (when a strip is composed, never per frame), a delivered thumbnail
takes a lock once more on the worker (none in the defaults: thumbnails are off), and the host carries
one more Timer that never runs in a details list. The rows are untouched.

### Console checks (for Jeremy, after the gates, TwinFM started unelevated)

"View" is the button on the toolbar at the top; the keys work in either pane's list.

1. **The keys.** In a folder with a few dozen files press Ctrl+Shift+1 (extra large icons), 2
   (large), 3 (medium), 4 (small), 6 (details). Each view shows at once; the toolbar's View menu lists
   them in that order with the current one ticked. Note that Ctrl+Shift+1 used to be details.
2. **Extra large in Pictures, thumbnails on** (Settings, Thumbnails). Ctrl+Shift+1: pictures show at
   256 px with a thin frame, names under them. Scroll with the wheel to the end and back: smooth, and
   the pictures are there when you come back.
3. **Extra large in Downloads, thumbnails off.** Big type icons, names on up to two lines; a long
   name shows whole under the tile the cursor is on.
4. **Small in Downloads.** Ctrl+Shift+4: rows of small cells, each a 16 px icon with its name beside
   it; long names end in "…". The arrow keys move across and down; PageDown and PageUp page; End and
   Home go to the ends.
5. **End in extra large.** Press End: the last tile is whole at the bottom of the list, with its
   name. Home: the first row at the top.
6. **Rename and new folder in each.** In extra large and in small: F2 on a file (the box sits on the
   name: under the icon in extra large, beside it in small), Escape; then New folder on the toolbar:
   a folder placeholder with the name box, Return makes it and the cursor lands on it.
7. **Mouse.** In each new view: click, Ctrl+click and Shift+click tiles; drag a band from empty space
   over a few; right-click a tile (the menu opens at it); drag a tile to the other pane (the drag
   picture is a normal size, not 256 px).
8. **Drop onto a tile.** Drag a file from the other pane onto a folder's tile in extra large and in
   small: the tile lights up, and held a second the folder opens.
9. **The dialog.** Settings, View: both panes list the five views in Explorer's order; pick one, OK.
10. **A big picture folder, if one is to hand** (a few thousand photos, thumbnails on, extra large):
    scroll through all of it and back. Task Manager's memory for TwinFM should level off near 900 MB
    and not keep climbing; pictures scrolled back to reappear after a moment.
11. **Restart.** Close TwinFM in small icons and start it again: the pane comes back in small icons.

### Calls made in this milestone, for Jeremy to check

- **The thumbnail memory budget**, 768 MB with the least recently drawn handed back (above): put to
  Jeremy by the coordinating session as a call of this stream.
- **With thumbnails on, small icons show 16 px thumbnails**, where Explorer's Small icons shows type
  icons. It follows the one setting ("thumbnails on" means pictures everywhere); say if small icons
  should always show type icons.
- **Sizes**: the extra large tile is 260 x 296 px and the small cell 220 px wide. Explorer's small
  cells follow the longest name on screen; a fixed width keeps the cells in columns and needed no
  measuring pass. The width is one constant if it wants changing.
- **Details moved from Ctrl+Shift+1 to Ctrl+Shift+6** with "Match Explorer"; there is no setting to
  keep the old keys.
- **The drag image** out of an extra large tile is the large tile's size, and out of a small tile the
  list's (above).
- **The far-jump fix is a second aim a frame later**, in TwinFM, not a patch to the Slint fork: the
  fork's whole-row behaviour stays on the roadmap as it was.
- **`TWINFM_THUMB_MB`** is a test knob for the check script, not a setting.
- **No WPR trace for the settle-first question**: the plan said "a trace"; the in-app pair at 60 Hz
  showed both views under large icons, so the elevated trace was left to the gate's triplets.
