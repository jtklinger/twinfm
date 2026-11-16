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

## P16-M1: a drag that says what it will do

Branch `feat/p16-m1-drag-cue`, the drag and views stream, cut from main at 3e4c201 (the Phase 16 kickoff).

### Settled first: the badge's look and place, with Jeremy

The cheapest working cue was built first (the effect handed to `drop_highlight` beside the spot, on
change only), with three looks switchable by an environment variable, and each was photographed in a
scripted hover of three files from the left pane over the right one (`tools/check-p16m1.ps1`, the
software renderer, the window found by the run's own process id; 36 pictures, the dark scheme the machine
is set to and option 1 in the light scheme as well):

1. **A tag on the target**: a small card on the highlighted thing itself, a pill in the effect's colour
   with an icon and "Move here" or "Copy here", then in the muted colour how to get the other.
2. **A banner**: no tag; a bar in the effect's colour across the foot of the target pane's list.
3. **The tag and a tinted target**: option 1, with the highlight filled in the effect's colour too.

In all three the highlight's border and the pane's status line take the effect's colour. The coordinating
session put them to Jeremy with a recommendation of option 1 and three points of mine; his answer
(2026-11-15): **"Go with your recommendations"**. So:

- **Option 1, the tag on the target.** The other two looks and their switch were removed.
- My three calls that he let stand: **the colours** are fixed, amber for a move, green for a copy and
  violet for a drop that will ask (fixed so that they stay apart from the Windows accent, whatever it is
  set to); **"Choose on drop"** is the wording for a drop that will open the menu; **under "Always ask",
  items that can only be copied** (virtual files, items out of an archive) are copied without a menu.
- `drag_default` stays `explorer` by default.
- **Accepted with the pick** (the coordinating session's note on `m1-move-row.png`): on a details row the
  tag sits at the row's right end and covers that row's Modified date while the drag is over it.

### What landed

- **The cue.** While a drag is over a spot that takes it, the target shows a tag: an arrow and **"Move
  here"** in amber, a plus and **"Copy here"** in green, or an ellipsis and **"Choose on drop"** in violet
  (the drop will open Move here / Copy here / Cancel: a right-button drag, or the "Always ask" setting),
  and beside it in grey the key that gives the other: "Ctrl copies", "Shift moves", "Let go of Ctrl to
  move", or under "Always ask" "Ctrl copies, Shift moves". A key is named only where pressing it (or
  letting it go) really changes the drop: nothing is named for a source that allows one effect, for
  items that are only ever copied, or with Ctrl held where the drop would have copied anyway. The
  highlight's border takes the same colour on every kind of target, and the status line of the pane the
  drop is noted in reads, in that colour, "Drop moves 3 items to Documents", "Drop copies 1 item to
  Documents" or "Drop asks whether to move or copy 3 items to Documents" in place of its usual text
  while the drag hovers. It changes the moment Ctrl, Alt or Shift is pressed or let go.
- **Where the tag sits**: at a details row's right end, clear of the scrollbar; across a tile's lower
  edge; at the top middle of the list for a drop into the pane's folder; under a tab; under a crumb;
  right of a sidebar row (over the pane's edge, since the sidebar is too narrow for it). It is kept
  inside the window.
- **One element for the window, nothing in the rows.** The tag is one element of the window, laid over
  it and placed from Rust by the target's rectangle; the rows, tiles, tabs and sidebar rows gained
  nothing. The crumb buttons (a handful per pane, not rows) read the colour as a property. The cue is
  built in the drop target only when the spot, the effect or the deciding keys change, and `drop_cue`
  sets the window's properties only when the target or the cue differs from what is shown; a DragOver
  that changes nothing sets nothing.
- **The setting `drag_default`**, in the settings dialog as "Dragging files": As Explorer (the default:
  move on the same drive, copy onto another), Always copy, Always move, Always ask. It decides only
  with no key held: Ctrl (and Alt) copy and Shift moves under every choice, and the right button asks
  as before. It applies to every drop onto TwinFM, from its own panes and from other programs. Into an
  archive "Always move" does not reach: a copy unless Shift is held, as before. "Always ask" reuses the
  right button's menu for a left-button drop, the bold item being what Explorer's rule would have done.
- The Windows label by the pointer is unchanged.
- Driver: `dragover ... <ms> keys <none|ctrl|shift|alt|right>[,...]` (several key sets share the hover's
  time in order, so one hover can press and let go of a key), `dropfiles ... ask <copy|move|cancel>`,
  `settings set drag_default explorer|copy|move|ask`. Lines in `CLAUDE.md` § Measuring.

### Tests

`cargo test --release`: `test result: ok. 368 passed; 0 failed; 2 ignored` (364 before), on the first
cut (fa821a7's tree) and again on the picked look. The run before that last one, on the same tree, had
one failure, `fileops::archive::tests::zip_jobs_on_the_worker`, at its last line: removing its scratch
folder gave os error 32 (a file there in use by another process), after every assert of the test had
passed. It is a real-shell test in `%TEMP%`, the other stream had leave for a full suite at the same
time, and this milestone touches nothing it runs; the whole suite was run again at once, as the
coordinating session had said to, and passed. No other test was run twice. The suite's runs on this
branch: three, of which that one failed in that one test (1 of 3); the search and files stream saw the
same test fail the same way with the suites overlapping, and the coordinating session carries it to the
roadmap at the close.

New (all pure): `dnd::the_drag_default_setting_decides_only_with_no_key_held` (the effect under each of
the four settings on the same and on another volume, with no key, Ctrl, Alt, Ctrl+Shift and Shift; a
source that allows one effect; a refused drop; into an archive; when the menu is asked for),
`dnd::the_cue_says_what_the_drop_does_and_how_to_get_the_other` (the tag's words, the hint and the
status line for one item and several, a move and a copy, each key held, items only ever copied, a drop
that asks, and none for no effect or a bookmark), `dnd::the_cue_badge_sits_on_its_target` (the target's
rectangle for the list, a scrolled row and a tile, checked against the hit test, and the tag's anchor
for each kind of target), `settings::drag_default_defaults_to_explorer_and_round_trips` (an older file
with no key reads as Explorer's rule; each value written and read back; anything else the default), and
the dialog's changes test gained the field.

### Scripted checks

`tools/check-p16m1.ps1` (2026-11-15, the branch's release build, no settings file, the software renderer),
one scripted run in `%TEMP%\twinfm-p16m1-check\`, exit 0, no panic: the left pane on `left` (three
files), the right on `right` (Documents, Photos, a file); every hover is a `dragover` (DragEnter,
DragOver, DragLeave, never a drop) and lasts under the spring time. From its stderr (the scratch
folder's path cut to `...`):

```
[dnd] cue: Move here (pane 1)
[dnd] over pane 1 row 0 (...\right\Documents) keys=0x0 allowed=0x3 -> effect move
[dnd] cue: none
[dnd] cue: Copy here (pane 1)
[dnd] over pane 1 row 0 (...\right\Documents) keys=0x8 allowed=0x3 -> effect copy
[dnd] cue: none
[dnd] cue: Move here (pane 1)
[dnd] over pane 1 row 0 (...\right\Documents) keys=0x0 allowed=0x3 -> effect move
[dnd] cue: Copy here (pane 1)
[dnd] over pane 1 row 0 (...\right\Documents) keys=0x8 allowed=0x3 -> effect copy
[dnd] cue: Move here (pane 1)
[dnd] over pane 1 row 0 (...\right\Documents) keys=0x0 allowed=0x3 -> effect move
[dnd] over pane 1 row 0 (...\right\Documents) keys=0x4 allowed=0x3 -> effect move
[dnd] cue: none
[dnd] cue: Copy here (sidebar)
[dnd] over sidebar row 2 (G:\), noted in pane 0 keys=0x0 allowed=0x3 -> effect copy
[dnd] cue: Move here (pane 1)
[dnd] over pane 1 tab 0 (...\right) keys=0x0 allowed=0x3 -> effect move
[dnd] cue: Move here (pane 1)
[dnd] over pane 1 crumb 8 (...) keys=0x0 allowed=0x3 -> effect move
[dnd] cue: Choose on drop (pane 1)
[dnd] over pane 1 row 0 (...\right\Documents) keys=0x2 allowed=0x3 -> effect move
[settings] dialog: drag_default explorer -> copy
[dnd] cue: Copy here (pane 1)
[dnd] over pane 1 row 0 (...\right\Documents) keys=0x0 allowed=0x3 -> effect copy
[dnd] cue: Move here (pane 1)
[dnd] over pane 1 row 0 (...\right\Documents) keys=0x4 allowed=0x3 -> effect move
[dnd] Drop at pane 1 row 0 (...\right\Documents) keys=0x0 allowed=0x3: 1 file(s), effect copy (reported to the source: copy)
[fileops] done id=1 pane=1 tab=2 Done: Copied 1 item
[settings] dialog: drag_default copy -> move
[dnd] cue: Move here (pane 1)
[dnd] cue: Copy here (pane 1)
[dnd] over pane 1 row 0 (...\right\Documents) keys=0x8 allowed=0x3 -> effect copy
[settings] dialog: drag_default move -> ask
[dnd] cue: Choose on drop (pane 1)
[dnd] over pane 1 row 0 (...\right\Documents) keys=0x0 allowed=0x3 -> effect move
[dnd] cue: Copy here (pane 1)
[dnd] over pane 1 row 0 (...\right\Documents) keys=0x8 allowed=0x3 -> effect copy
[dnd] drop menu: asked by the setting
[dnd] drop menu: default Move here, chose Cancel -> effect none
[dnd] Drop at pane 1 row 1 (...\right\Photos) keys=0x0 allowed=0x3: 1 file(s), effect none (reported to the source: none)
[dnd] drop menu: asked by the setting
[dnd] drop menu: default Move here, chose Copy here -> effect copy
[fileops] done id=2 pane=1 tab=2 Done: Copied 1 item
[dnd] Drop at pane 1 row 1 (...\right\Photos) keys=0x8 allowed=0x3: 1 file(s), effect copy (reported to the source: copy)
[fileops] done id=3 pane=1 tab=2 Done: Copied 1 item
[settings] dialog: drag_default ask -> explorer
[dnd] cue: Move here (pane 1)
```

Read from it: on one drive the cue is a move and with Ctrl a copy; inside one hover Ctrl pressed and
let go and Shift pressed change the cue with no DragLeave between (Shift over a move changes no word,
so only the `over` line records it); a sidebar row on another drive (`G:`) is a copy; a tab and a crumb
take the cue; the right button reads "Choose on drop". Under "Always copy" the same hover is a copy,
Shift still moves, and a drop with no key on the same drive copied. Under "Always move" Ctrl still
copies. Under "Always ask" the hover reads "Choose on drop", Ctrl skips the menu, and two left-button
drops opened the menu, Cancel doing nothing and Copy here copying. Nine pictures were taken and looked
at (`cue-move-row.png`, `cue-copy-row.png`, `cue-move-list.png`, `cue-copy-list.png`, `cue-side.png`,
`cue-tab.png`, `cue-crumb.png`, `cue-ask-right.png`, `cue-ask-setting.png`): the tag on each target in
its colour, the border the same colour, the status line's words; and the mock-up run's light-scheme
set for option 1, the same code path.

### Soak

`tools\soak-p5m3.ps1 -Minutes 10` on the branch's release build (2132c70's tree), 2026-11-15, display
scale 100%, harness-launched, with the coordinating session's "soak go" and the other stream holding:
ran 10.2 min, window closed by the script, exit code 0, 0 panicked lines; 227 jobs done (failed 0,
cancelled 0), soak refusals 0, waitops timeouts 0, menupicks refused 0, every "must be 0" count 0.

```
drag cue: Move here 23, Copy here 39, Choose on drop 6, none 68 lines; Ctrl hovers over inner read as a copy 2; Always copy on 2, off 2; drops onto inner with no key that copied 2 (the Ctrl hover, the setting each way and that drop: one each per cycle)
drops: 50 (pane copy 18, pane move 3, bookmark copy 3, tab copy 3, menu copy 3, menu cancel 3, as expected: pane copy is three per cycle here), drop jobs submitted: 44
right-button drags: 6; drags cancelled: 15, dropped: 0 (must be 0); drag loops that found the state busy: 0; drag images set: 15
spring: armed 18, spring-loaded rows 3 (navigations into spring\inner: 5), tabs 0 (must be 0), disarmed 15; crumb drops: 3; auto-scroll lines: sidebar 0, tab strip 0
working set: start 155.9 MB at 5 s, end 214.7 MB at 601 s; private: 165.7 -> 217.6 MB
```

Three cycles started; the run ended inside the third, before its drag cue steps, so the cue's own counts
are two (the steps sit at the end of a cycle's fixed part, as "Tab between the panes: 2" and "details
bands: 2" beside them show). In both: the hover over `inner` with Ctrl read as a copy, "Always copy"
went on and off through the dialog, and the drop onto `inner` with no key, on the same drive, copied.
The cue lines in all (136) come from every scripted drag and drop of the soak, each of which now logs
its cue and its `none`; the six "Choose on drop" are the right-button drop menus' drops. The
navigations into `spring\inner` are five where there were three: the two more are this cycle's own
`nav` there to remove the copy; the spring-loaded rows are three as before, so the two 500 ms hovers
per cycle opened nothing. The 15 scripted drags all returned and none dropped.

### Gate

The defaults triplet, `target-p16m1` against `target-p15close` (4d335b7), to be run by Jeremy from the
coordinating session's hand-over; tags continue from ab84.

What the gate scenario meets of this milestone: **it never drags, so the triplet proves the rest
untouched.** No cue is built and `drop_cue` never runs. What every run does carry: one more element in
the window behind an `if` that is false (never instantiated), the highlight overlays' border colour
read from a property in place of the theme's token (the overlays are invisible unless a drag hovers),
the status line's text, weight and colour each behind a test of an empty string (three bindings per
pane, evaluated when the status changes, never per frame), and each crumb button's border colour as a
property. No row, tile, tab or sidebar row has a new binding.

**Result (ab85, 2026-11-16, Jeremy at the console, `target-p16m1` 2132c70 against `target-p15close`):**

| Run | p15close | p16m1 |
|---|---|---|
| 1 | 20.3 | 20.3 |
| 2 | 20.3 | 20.4 |
| 3 | 20.5 | 20.5 |
| median | 20.3 | 20.4 |

The runs interleave and the medians differ by 0.1: no gap. The sitting was in the costlier state
throughout (render p50 2.62 to 2.74 ms in every run of both binaries), so it reads like for like.

### The first console check (2026-11-16, `target-p16m1` 2132c70): four changes

Checks 1, 2, 5, 6, 8, 10, 11, 12 and 13 passed ("Works as expected"). Four things came back, relayed by
the coordinating session with its reading; Jeremy's words are quoted.

**A. The settings dialog's OK and Cancel took no mouse click.** "first issue is the settings screen. If
I make a change, the cancel/ok buttons no longer accept mouse input. Keyboard esc/enter are still
working." Reproduced in the gate build with a real click (`clickat` on OK: the dialog stayed open), and
without any change staged: the buttons were dead from the moment the dialog opened. It is this
milestone's fault, not 0.6.0's: the same click closes the dialog in `target-p15close`. The dialog is a
popup with a fixed height (710 px), a popup takes no click outside its own rectangle, and the new
"Dragging files" row made the content 40 px taller, which pushed the two buttons below the popup's lower
edge: drawn, but out of the mouse's reach. Escape and Enter go through the keyboard and never noticed.
The scripted check had not caught it because its `settings ok` step calls the button's function rather
than clicking it. Fixed by the dialog's height (750 px); `tools/check-p16m1.ps1` now closes the dialog
with real clicks on Cancel and on OK, which fail on the old build. The small grey note under the buttons
still sits on the dialog's lower edge, as it does in 0.6.0 (left as it is).

**B. One key for the other effect.** "This works, but wouldn't it make sense to have the same key either
control or shift, used for #2 and #3?" and "just have it use the control button to toggle". Ctrl is now
the one key the tag names, and Ctrl alone gives the other effect of whatever a plain drop would do: on
one drive "Ctrl copies", across drives "Ctrl moves", under Always copy "Ctrl moves", under Always move
"Ctrl copies"; held, the tag reads "Let go of Ctrl to move" (or "to copy"). Shift still forces a move and
Alt (and Ctrl+Shift) a copy, for Explorer habits: the tag's title says what will happen, but no hint
names them. Into a zip a drop stays a copy with no hint, and Ctrl changes nothing there. **This departs
from Explorer**, where Ctrl is always a copy (across drives Ctrl now moves): Jeremy's choice over the
kickoff's "Match Explorer", the coordinating session's reading of his words.

**C. Always ask asks with the mouse only.** "Change the 'Always Ask' process to not prompt for a key-use
only the mouse copy/move/cancel. This will help resolve #3, and just have a single button." and, of the
right button's menu, "this is how #9 should work." Under Always ask the tag reads "Choose on drop" with
no key named, and every drop opens the Move here / Copy here / Cancel menu, Ctrl or Shift held or not,
as a right-button drag does (a held key had skipped the menu). The menu's bold item is still the effect
the keys would have given. The optional Ctrl toggle under Always ask is not built.

**D. A way back out of a spring-loaded folder.** "Works, but needs a way to go back up the folder tree
if I accidently go into the wrong subfolder--without having to cancel the whole copy/move." The folder
names in the path bar already open when a drag is held over them (P11-M3). Added: **the path bar's Back
and Up buttons** do the same. Hold the drag over Up for a second and the pane goes to the parent; over
Back, back to where it was; the button is lit while the drag is over it. A greyed button does nothing,
and nothing can be dropped on either (the pointer shows the no-drop sign there). Held on, Up climbs one
level a second. It was sound in the hit test: the two buttons became spots of their own (`Spot::Nav`),
where they had counted as the pane's own folder, so a drop let go over Back or Up, which used to land
in the folder shown, now lands nowhere. Forward and Refresh are unchanged.

The driver's `dropfiles ... copy` holds Alt now (Ctrl would move where the default copies), and
`dragover nav <pane> <back|up>` hovers a button. Tests 368 passed, 0 failed, 2 ignored (the two rule
tests rewritten for B and C, the crumb hit test extended for D). The scripted check: exit 0; its lines
show Cancel and OK by the mouse, "Move here" for Ctrl under Always copy, "Choose on drop" and the menu
with a key held under Always ask, and the right pane going up out of Documents and back into it.

### Console checks (for Jeremy, after the gate, TwinFM started unelevated)

Revised after the first console check: 2, 3, 7, 8 and 9 changed, 14 and 15 are new.

Two panes showing (the toolbar's Two panes). "The tag" is the small coloured label that appears on the
thing under the pointer while you drag; "the status line" is the text at the bottom left of the pane you
are dragging into.

1. **A drag between panes on one drive.** Both panes on folders of `C:`. Drag a file from the left
   pane over an empty part of the right pane's list: the list gets an amber border, an amber tag "Move
   here" with "Ctrl copies" beside it shows at the top middle of the list, and the right pane's status
   line reads "Drop moves 1 item to <folder>" in amber. Let go: the file moves.
2. **Ctrl mid-drag.** Start the same drag and, still holding the mouse button, press Ctrl: the tag
   turns green, "Copy here", "Let go of Ctrl to move", and the status line "Drop copies…". Let go of
   Ctrl: amber "Move here" again. Press Escape to cancel the drag.
3. **Across two drives: the same key.** The right pane on another drive (a USB stick, or `G:`). Drag a
   file from `C:` over it: green "Copy here", "Ctrl moves". Hold Ctrl: amber "Move here", "Let go of
   Ctrl to copy". (Shift also moves, as in Explorer, but the tag no longer mentions it.) Escape.
4. **Onto a folder row.** Drag a file over a folder's row in the other pane: the row gets the coloured
   border and the tag sits at the row's right end (over that row's date, which is expected). Hold it
   there a second: the folder opens, as before, and the tag moves to the top of the list.
5. **A tab, a crumb, a sidebar row.** Drag a file over another tab at the top of a pane (the tag hangs
   under the tab), over a folder name in the path bar (under the name), and over a drive or bookmark in
   the sidebar at the left (the tag shows just right of the sidebar, and the active pane's status line
   names that folder). Escape each time.
6. **Several files.** Select three files and drag them: the status line says "3 items".
7. **Always copy.** Settings (Ctrl+comma, or Settings at the sidebar's foot), "Dragging files", Always
   copy, OK (click OK with the mouse). Drag a file between two folders of `C:`: green "Copy here",
   "Ctrl moves", and letting go copies. With Ctrl held it reads "Move here" and moves.
8. **Always move.** The same with Always move, onto another drive: amber "Move here", "Ctrl copies",
   and Ctrl copies.
9. **Always ask.** With Always ask, drag a file to the other pane: a violet tag "Choose on drop" and
   nothing beside it. Let go: a small menu Move here / Copy here / Cancel opens at the pointer; pick
   one. Drag again with Ctrl held: still "Choose on drop", and the menu opens all the same.
10. **The right button.** Under any setting, drag a file with the right mouse button: violet "Choose on
    drop", and the menu on release, as before.
11. **From Explorer into TwinFM.** Drag a file from a File Explorer window into a TwinFM pane: the same
    tag, colours and status line, and the setting applies. (Dragging out of TwinFM into Explorer follows
    Explorer's rule; TwinFM cannot change that.)
12. **Light and dark.** Settings, Theme, Light, then Dark: the tag's colours are readable in both.
13. Put "Dragging files" back to As Explorer when done, if that is what you want to keep.
14. **The settings buttons.** Open Settings, change anything, click Cancel with the mouse: the dialog
    closes and nothing changed. Open it again, change something, click OK: it closes and the change is
    applied.
15. **Back out of a folder mid-drag.** Drag a file over a folder's row in the other pane and hold until
    it opens. Still holding, move the pointer onto that pane's Up arrow (left of the path) and hold: the
    arrow lights and after a second the pane goes back up to the parent. Hold on and it climbs another
    level. Do the same on the Back arrow. Then drop the file in a list as usual. Letting go over the
    arrows themselves drops nothing.

### Calls made in this milestone, for Jeremy to check

- **The tag covers the row's date while the drag is over a details row** (accepted with the pick,
  above). It sits there because the row's right end is where a wide pane has room and the name stays
  readable.
- **"Choose on drop" also shows during a right-button drag**, since that drop opens the menu too; there
  no key is named (the keys do not skip the menu for the right button).
- **After the first console check** (above, B to D): Shift and Alt keep working without being named;
  Ctrl+Shift stays a copy; a drop into a zip is left out of the Ctrl rule; Up and Back take no drop,
  where a file let go over them used to land in the folder shown; a drag held on over Up keeps climbing,
  one level a second; a pane reached by Up or Back during a drag does not become the active pane.
- **A scripted left-button drop under "Always ask" with no pick given picks Cancel**, so that no script
  ever waits at a menu for a person.
- **No cue over the Bookmarks gap** in the sidebar: that spot already says "Bookmark" by the pointer and
  shows its own line, and nothing is moved or copied there.
- **The status line names the folder as the Windows label does**: its last name, `C:` for a drive.
- **The hint under Ctrl**: with Ctrl held over a spot where the drop would have copied anyway, no key is
  named, because Shift does not move while Ctrl is down (Ctrl+Shift copies, as before).
- **The `[dnd] over` line is logged when a deciding key changes** over the same spot with the same
  effect too (it was logged only when the spot or the effect changed): the cue's state follows the same
  record.
- **The plan's line `[dnd] cue: ... | none`** gained the third value `Choose on drop`.
