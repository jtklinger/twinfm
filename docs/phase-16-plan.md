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

## P16-M2: the small items

Branch `fix/p16-m2-small-items`, the search and files stream, cut from main at 3e4c201 (the kickoff).

### Settled first

- **Whether a text box can pass the mouse's side buttons.** Read in Slint's source at the pinned
  revision (`internal/core/items/text.rs`, `TextInput::input_event`; `internal/core/input.rs`,
  `send_mouse_event_to_item`): a `TextInput` handles the left and middle buttons and answers "ignored"
  to every other press and release, and Slint offers an ignored event to whatever lies behind the item.
  A `TouchArea` **over** the input cannot do it (a `TouchArea` takes every press, the text cursor's
  among them); a `TouchArea` **behind** it gets exactly the two side buttons and no click of the
  input's. The areas were there already: the one behind each field's input that gives it the focus on
  a click beside the text (`Field` in theme.slint, the search box in pane.slint). That is also how
  P6-M3 made the buttons ignored there: an area behind the entry that swallows them. So the answer is
  yes, and the scripted check below shows it.
- **What `own_results` answers today**, before changing how. A result is kept when its folder is the
  folder of one of the items **and** it names one of them; each of the two is tried as text (lowercase,
  no trailing separator) and, when that misses, with the folder above canonicalised and the last name
  compared as text. So a path that differs above the last folder is followed (an 8.3 `%TEMP%`, a `.`
  in the path), and an items' folder named by its own 8.3 name is not: with every item reported that
  way the old loop kept nothing. The new code keeps that answer too (the test pins both).

### What landed

Per item: done, checked and found right, or left with the reason.

- **The big-copy slowdown: done** (0c02233). After a copy, move or delete the shell reports every file
  inside a copied folder as well as the items themselves, and TwinFM keeps only the items' own results.
  It did that by testing every result against every item, with file system calls among them, so a move
  of thousands of files left the status line waiting long after Windows' window had closed. Now the
  items and their folders are put in two sets once, each result is looked up in them, and a path that
  misses is canonicalised once per folder (`CanonDirs`) and looked up in sets canonicalised once, built
  only when a result first misses. The answers are the same; the time is linear. `same_path` (still
  used by the archive jobs and the tests) is the same rule on the same helpers. The extract job's
  comment that named the quadratic loop as a reason no longer says so (it still skips the function,
  because those formats report twice or not at all).
- **The mouse's Back and Forward over a tab: done** (1cd16bf). Over a tab and over its close button
  the two buttons go back and forward in that pane's shown tab, whichever tab the pointer is on, and
  the pane becomes the active one, as over the rows.
- **Over the scrollbars: done** (1cd16bf). `VScrollBar` and `HScrollBar` gained two callbacks and two
  tests in their pointer handler. Wired: each pane's list scrollbar (that pane) and the preview
  column's three (the active pane, as over the toolbar across the window). **Left unwired**: the
  sidebar's scrollbar and the crumb dropdown's, because the rows beside them do nothing with the two
  buttons either; a bar that goes back beside rows that do not would be the odd one.
- **Over a text box: done for the search box and the path's text entry, left ignored over the rename
  and new-name editor** (1cd16bf). Over the search box the pane goes back or forward. Over the path's
  text entry the entry closes first (as Escape closes it) and the pane moves. The fields in the
  settings dialog, the questions and the sidebar's title editor are unwired (they are not a pane's).
- **The click on a row's blank part, against Explorer: no code**; the console check is written below
  (checks 8 to 10).
- Driver: `backclick <pane> tab|scroll|search|preview`, `forwardclick` the same. Lines: `[nav]` as
  before. `CLAUDE.md` § Measuring has both.

### Tests

`cargo test --release`: **366 tests** (364 before), `test result: ok. 366 passed; 0 failed; 2 ignored`.

- `fileops::tests::own_results_is_linear_in_the_items` (pure): 20,000 items of one folder that does not
  exist, their results in another case, and after the first item 20,000 results for a folder's
  contents; the sorting runs on a thread and must answer within 10 s. **Run first against the old
  loop: `20,000 items' results are sorted out within 10 s: Timeout`.** On the sets: `[test]
  own_results: 20000 of 40000 kept in 80 ms`, the 20,000 items' results in the shell's order.
- `fileops::tests::own_results_keeps_what_the_pairwise_loop_kept`: the old loop is kept in the test
  module, and every case is run through both and compared. On a real scratch folder
  (`%TEMP%\twinfm-p16m2-own-<pid>\`, removed after): the items named by their long path, below an 8.3
  path (this machine's `%TEMP%` gave the scratch folder one, `TW7CBB~1`) and with a `.` in the path,
  against results named each of those ways and in capitals, for Copy, Move and Delete: the three items
  kept, a copied folder's contents at two depths, a file that is no item and a result from another
  folder dropped. The items' folder by its own 8.3 name (kept by neither, as before). Items of two
  folders, one inside the other. Rename, MoveAs, NewFolder and NewFile (whose result names the folder),
  a drive's root, items in a folder that is not there, and Restore (all kept).
- The first version of that test expected the fully 8.3 case to keep the three items; the old loop
  keeps none there (§ Settled first), so the expectation was wrong, not the code, and was corrected
  before the new code was written.
- Runs: the full suite was run three times. **The first run: 364 passed, 2 failed**,
  `fileops::archive::tests::extracts_on_the_worker` and `zip_jobs_on_the_worker`; the one message read
  was `Os { code: 32 ... "The process cannot access the file because it is being used by another
  process." }` at the test's last line, removing its scratch folder (Phase 7 recorded the same from
  Windows' archive handler still holding a 7z); the other's message was not kept. Both passed when run
  alone (`2 passed`), and the second and third full runs were `366 passed; 0 failed; 2 ignored` (the
  second before item 1's commit, the third before item 2's). The cause of the first run's two failures
  was not found; nothing in this change touches those tests' cleanup.

### Scripted checks

`tools/check-p16m2.ps1`, one scripted run of the branch's release build in
`%TEMP%\twinfm-p16m2-check\` (a folder of 300 files so the list overflows, a 600-line text file for the
preview), the real renderer, exit 0, no `panicked` line. Each press is a posted `WM_XBUTTONDOWN` and
`WM_XBUTTONUP` through winit's own path. From its output (paths shortened):

```
[auto] backclick pane=0: posted at (1190, 168) logical        the list's scrollbar, in many
[nav] pane=0 Back button (active pane was 0, now 0)
[nav] pane=0 history back: ...\work\many -> ...\work (1 back, 1 forward left)
[auto] forwardclick pane=0: posted at (267, 62) logical       the first tab
[nav] pane=0 Forward button (active pane was 0, now 0)
[nav] pane=0 history forward: ...\work -> ...\work\many (2 back, 0 forward left)
[auto] backclick pane=0: posted at (267, 62) logical          the tab again
[nav] pane=0 history back: ...\work\many -> ...\work (1 back, 1 forward left)
[auto] forwardclick pane=0: posted at (1190, 168) logical     the scrollbar, after a move back from sub
[nav] pane=0 history forward: ...\work\many -> ...\work\many\sub (3 back, 0 forward left)
[auto] backclick pane=0: posted at (1005, 97) logical         the search box
[nav] pane=0 history back: ...\work\many\sub -> ...\work\many (2 back, 1 forward left)
[auto] forwardclick pane=0: posted at (692, 97) logical       the path's text entry
[nav] pane=0 history forward: ...\work\many -> ...\work\many\sub (3 back, 0 forward left)
[auto] backclick pane=0: posted at (692, 97) logical          the entry, opened again
[nav] pane=0 history back: ...\work\many\sub -> ...\work\many (2 back, 1 forward left)
[auto] backclick pane=0: posted at (269, 162) logical         the rename editor: no [nav] line follows
[auto] backclick pane=1: posted at (778, 62) logical          the right pane's tab
[nav] pane=1 Back button (active pane was 0, now 1)
[nav] pane=1 history back: nothing to go back to
[preview] pane=0 gen=2 ...\work\long.txt: text, 600 lines, 16090 bytes, UTF-8, plain
[auto] backclick pane=0: posted at (1193, 422) logical        the preview column's scrollbar
[nav] pane=0 Back button (active pane was 0, now 0)
[nav] pane=0 history back: ...\work -> ...\work\many (2 back, 1 forward left)
```

What the check cannot tell apart: a press on the list's scrollbar from a press on the row under it,
since both log the same `[nav]` line. The bar took the press because it is a `TouchArea` in front of the
rows (it has swallowed these two buttons since P2-M3, which is the roadmap item); the tab, the search
box, the path entry and the preview's bar have nothing behind them that goes back, so their lines can
only be the new code's. The first run of the script aimed its rename step at row 0 while the editor was
on row 1 (the script's mistake), so the row took the press and the pane went back; with the editor on
row 0 the press is ignored, as above.

### Soak

`tools\soak-p5m3.ps1 -Minutes 10` (the branch's own script) on the branch's release build of 1cd16bf's
code, 2026-11-15, harness-launched, with the coordinating session's leave and the other stream holding:
ran 10.1 min, window closed by the script, exit code 0, 0 panicked lines; 222 jobs done (failed 0,
cancelled 0), soak refusals 0, waitops timeouts 0, menupicks refused 0, every "must be 0" count 0.

```
side buttons: Back over the list's scrollbar 2, Forward over a tab 2 (one each per cycle)
held track presses: 4, repeated past one page 4 (two each per cycle)
right-button drags: 6; drags cancelled: 15, dropped: 0 (must be 0); drag loops that found the state busy: 0; drag images set: 15
working set: start 154.8 MB at 5 s, end 228.9 MB at 600 s; private: 165.2 -> 237 MB
```

Three cycles started their file operations; the time ran out in the third before its scroll part, so
the counts taken there are two (the side buttons, the held presses, Tab between the panes). Each Back
over the scrollbar: `[nav] pane=0 history back: C:\traces\bigdir -> ...\twinfm-m5\m2`; each Forward over
the tab: `[nav] pane=0 history forward: ...\twinfm-m5\m2 -> C:\traces\bigdir`. The display scale was not
read for this run.

### Gate

Not run yet. The code head for the gate build is 1cd16bf (the commits after it change the soak
script and the docs only); the reference is `target-p15close` (4d335b7), the defaults triplet, built
and handed to Jeremy by the coordinating session. Its figures go here.

What the gate scenario meets of this milestone: its `drag` steps press the list's scrollbar, whose
pointer handler now makes two more comparisons per pointer event (the event's kind and button against
"released" and "back", then "forward"); a left-button drag fails both at once. The rows, the tiles and
the lists gained nothing; the list host gained two callbacks that are never called in the scenario. It
runs no file operation, so `own_results` is not called, and it presses no tab or text box.

### Console checks (for Jeremy, after the gate, TwinFM started unelevated)

These need a mouse with Back and Forward side buttons (the two thumb buttons).

1. **Over a tab.** Go into a few folders in the left pane so there is somewhere to go back to. Rest
   the pointer on the pane's tab (the strip at the very top of the pane, above the path bar) and press
   the mouse's Back button: the pane goes back a folder. Forward: it goes forward again. The same with
   the pointer on the tab's small x.
2. **A tab in the other pane.** With two panes, the left one active, press Back over the right pane's
   tab: the right pane becomes the active one (its ring, the title bar's text) and goes back.
3. **Over the list's scrollbar.** In a folder long enough to scroll, rest the pointer on the
   scrollbar at the list's right edge (on the thumb, then on the empty track above or below it) and
   press Back, then Forward: the pane moves each time, and the list does not page.
4. **Over the preview's scrollbar.** Alt+P, the cursor on a long text file or a PDF. With the pointer
   on the scrollbar at the preview column's right edge, press Back: the active pane goes back.
5. **Over the search box and the path.** With the pointer on the search box at the right end of the
   path bar, press Back: the pane goes back. Press Ctrl+L (the path as text), rest the pointer on the
   text and press Back: the text entry closes and the pane goes back. Then check the entry still works:
   Ctrl+L, click in the text (the text cursor lands where you clicked), double-click a word (it is
   selected), drag across some text (it is selected).
6. **The rename editor is left alone.** Press F2 on a file and, with the pointer on the name being
   edited, press Back: nothing happens and the name is still being edited.
7. **The big move.** Select a few thousand files in one folder (Ctrl+A in a folder of photos, or of
   `C:\traces\bigdir` copied somewhere first) and move them to a folder on the same drive (Shift+F6 to
   the other pane, or a drag). Watch the status bar at the bottom of the pane as Windows' progress
   window closes: "Moved N items" should show without a long wait after it (the wait before this change
   was not timed on a real move; the test's 20,000 items took over 10 s against 80 ms). Ctrl+Z brings
   them back.
8. **The blank-part click, beside Explorer: where the outline goes.** Open the same folder in
   Explorer (View, Details) and in TwinFM's details list, side by side. In Explorer click a file's
   name, then click another row far to the right of its name (in the empty part of the Name column).
   Look for the thin dotted outline on a row: does it stay on the first file, move to the row clicked,
   or go altogether? (It may only show after one press of an arrow key: press Down once and see which
   row becomes selected; that tells where the outline was.) Do the same in TwinFM: nothing is selected
   and the outline (a solid ring in the accent colour) sits on the row clicked, so Down selects the row
   below the one clicked. Say if Explorer does otherwise and you want it followed.
9. **The same with Ctrl.** In Explorer select a file, hold Ctrl and click another row's blank part:
   is that row added to the selection, or does nothing change? In TwinFM it is added (Ctrl toggles the
   row clicked).
10. **The same with Shift.** In Explorer select a file, hold Shift and click a row's blank part a few
    rows down: is the range selected, or does nothing change? In TwinFM the range from the first file
    to the row clicked is selected.

### Calls made in this milestone, for Jeremy to check

- **The side buttons over the path's text entry and the search box now move the pane.** This
  **reverses P6-M3**, which made them ignored over the path entry on purpose and which Jeremy checked
  then (`phase-6-plan.md`: before it they went back, after it they did nothing). The kickoff's rule
  was to do it if Slint allows it without touching the text cursor's clicks, and it does. Over the path
  entry the entry closes first, so it does not stay open showing the folder left behind.
- **The rename and new-name editor stays ignored**, as P6-M3 left it: a move in the history there
  would throw away the name being typed.
- **Jeremy's decision on those two** (2026-11-15, put to him by the coordinating session with its
  recommendation that the split is right): "Go with your recommendations". So the search box and the
  path entry take Back and Forward, and the rename editor stays ignored.
- **The preview's scrollbars move the active pane**, as the toolbar across the window does; the
  preview has no pane of its own.
- **The sidebar's and the crumb dropdown's scrollbars are not wired**, since their rows are not.
- **Back over any tab acts on the pane's shown tab**, not on the tab under the pointer (a background
  tab is not on screen to be moved; the tab menu is where a background tab is acted on).
- **The driver's `scroll` aim is the old `drag` aim** (10 px in from the pane's edge), so it is the
  scrollbar only while the list overflows; the soak presses it in the 40,000-file folder.
- **In the soak, Back is pressed over the scrollbar and Forward over the tab** (one press each per
  cycle, so the pane ends where it was), where the plan's words were "a `backclick` over a tab and over
  the scrollbar".

### Follow-ups, not done here

- `tabdrag` in `src/auto.rs` presses at y 16 of the window, which is on the toolbar across the window
  since P6-M3 made that the default; `tabmenu` and the new `backclick <pane> tab` reckon the strip's
  place from the layout. Read in the code, not run; no soak or check script uses `tabdrag` today; not
  touched here.
