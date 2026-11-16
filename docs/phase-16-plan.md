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

## P16-M6: a search tab picks up new matches

Branch `feat/p16-m6-search-follows`, the search and files stream, cut from main at 3e4c201 (the kickoff;
before P16-M2 and P16-M4 merged, so its content following reads files by the old lowercase path until
main is taken in).

### Settled first: what following a content search costs while many files are written

`tools/time-p16m6.ps1`, 2026-11-15, 5413138's release build, the machine quiet (the coordinating session
held the other stream): a search tab open on a tree of 200 text files while the script wrote under its
root in four phases, the search run again (F5) before each. Both runs: 0 panicked lines, exit 0, and the
tab still following after every phase.

| Phase | Content search ("needle") | Name search (`*.txt`) |
|---|---|---|
| Sync: 2,000 text files of 4 KB written in 1.6 s | 4 windows, 6,000 records, 2,001 files read; follow thread 417 ms in all (longest window 178 ms); process CPU 453 ms over 9.7 s (5% of one core) | 4 windows, 2,000 rows added; thread 149 ms; CPU 344 ms (4%) |
| Build: 20 s of 20 binary files of 100 KB and 5 text files of 20 KB a second | 13 windows, 1,395 records, 93 files read; thread 120 ms; CPU 125 ms over 28 s (under 1%) | 13 windows, 93 rows added; thread 73 ms; CPU 391 ms (1%) |
| Log: a 12 MB text file appended to five times a second for 20 s | 12 windows, 98 records, 12 reads of the whole file; thread 1,449 ms (longest 123 ms); CPU 1,391 ms over 28 s (5%) | 12 windows, nothing to do; CPU 0 ms |
| Idle, 20 s | CPU 31 ms over 28 s | CPU 16 ms |

**Cheap enough: content searches follow.** The worst case the tree allows (a text file just under the 16
MB cap rewritten all the time, read whole again in each window) costs a twentieth of one core, on a thread
of its own; a binary file is closed after its first 8 KB as in the walk, which is why a build costs next
to nothing. Three things bound it: the watch's own window (250 ms, backing off to 2 s while changes keep
coming, so a file written a hundred times a second is read at most a few times a second), one batch at a
time (the records of later windows wait and are netted together, a file modified ten times read once),
and the caps (a window past 10,000 records, or 10,000 waiting, ends the following). With P16-M4's engine
the reads cost about a tenth (983 ms against 104 ms for 100 MB).

### What landed

- **A finished search tab is watched** (5413138). Once its walk has run to the end, a search tab takes
  a subtree watch on its root, as a sizes view does: in front always, in the background only after
  every folder tab has its watch (the window's eight are shared). Never for a search rooted in a cloud
  location or inside an archive; This PC is refused before a search starts. A search stopped with
  Escape is not followed (it never saw the whole tree).
- **New matches appear.** Each window's created and renamed-to paths are statted and tested the way the
  walk tests them: the file's own name against the pattern, and no folder on the way that the tab's
  hidden-file switches keep out, no link, and for a content search no sync root. A match goes into its
  sorted place. A folder that arrives whole (moved in, or renamed) comes as one record, so it is walked
  for what it holds; its rows under the old name go and come back under the new one.
- **Removed and renamed-away files go at once**, with every row below a folder that went. The 5 s
  re-stat of the rows on screen (P11-M2) stays: it is what updates a name search's row when the file's
  size or date changes, which the watch does not say for a name search.
- **A content search follows too.** A created or modified file is read again on the follow thread (the
  same 16 MB cap, binary test and cloud rule as the walk): its row is added, takes its new matching
  line, or goes when the text is no longer in it.
- **Where it stops.** The status line says "Not following changes: F5 searches again" when a window's
  records are not known (the watch's buffer overflowed, or more than 10,000 records in one window or
  waiting), when the folder cannot be watched or its watch is lost, and when a followed tab loses its
  watch (a ninth watched tab pushes a background search tab out; the second pane hidden). F5 runs the
  search again and follows again. A search of a whole drive is watched like any other and will often
  stop this way; that is the honest limit until there is an index.
- Driver: no new step. Lines and the soak's counts in `CLAUDE.md` § Measuring.

### Tests

`cargo test --release`: **367 tests** (364 before), `test result: ok. 367 passed; 0 failed; 2 ignored`;
four full runs (three while the code changed, one before the soak and the docs commit), no failure in any;
`cargo build --release` finished. New, in `searchfollow::tests`:

- `a_windows_records_are_netted` (pure): created, removed and renamed paths; a modified file ignored by
  a name search and read once by a content search; made and removed within the window; saved by removing
  and writing again; a file made in a folder that then went; a folder that went and came back; a folder
  renamed; a file modified and then renamed to (its newest letter case kept); a folder only modified
  because a file in it was made.
- `rows_below_a_gone_path_go` (pure): a row's name against the paths that went, any letter case, a
  longer name being another folder.
- `looked_at_paths_match_as_the_walk_does` (a real tree in `%TEMP%\twinfm-p16m6-look-<pid>\`, a hidden
  folder and a junction): a name search and a content search over the same looked-at paths: matches by
  their own names, a folder that arrived walked, one only modified left alone, nothing behind the hidden
  folder (found with Ctrl+H's switch on) or through the junction, a gone file reported, the matching
  line set, six files read for the content search.
- The watch-set order (background search tabs last) has no test of its own: the existing `watch_set`
  test's candidates all carry `spare: false`.

### Scripted checks

`tools/check-p16m6.ps1`, 5413138's release build, one run in `%TEMP%\twinfm-p16m6-check\`, exit 0, no
`panicked` line. From its output (`<tree>` the scratch tree):

```
[search] pane=0 gen=3 done: 6 found, 4 folders walked, 0 skipped, in 8 ms
[search] pane=0 gen=3 watching <tree>
[auto] touch <tree>\new.txt: Ok(())
[search] pane=0 window: 2 records, 1 added, 0 removed, 0 changed, in 3.4 ms (applied in 0.1 ms)
[auto] rows pane=0: a.txt [], also.txt  (sub) [], b.txt [], d.txt  (sub\deep) [], hold.txt [], new.txt [], s.txt  (sub) []
[auto] touch <tree>\other.log: Ok(())
[search] pane=0 window: 2 records, 0 added, 0 removed, 0 changed, in 3.6 ms (applied in 0.0 ms)
(sub\deep\made.txt written by the script)
[search] pane=0 window: 2 records, 1 added, 0 removed, 0 changed, in 4.5 ms (applied in 0.0 ms)
(hid\made.txt written, in the hidden folder)
[search] pane=0 window: 2 records, 0 added, 0 removed, 0 changed, in 0.2 ms (applied in 0.0 ms)
[auto] mvfile <tree>\c.log -> <tree>\c.txt: Ok(())
[search] pane=0 window: 2 records, 1 added, 0 removed, 0 changed, in 3.4 ms (applied in 0.0 ms)
[auto] mvfile <tree>\new.txt -> <tree>\new.log: Ok(())
[search] pane=0 window: 2 records, 0 added, 1 removed, 0 changed, in 3.7 ms (applied in 0.1 ms)
(sub\deep\made.txt deleted)
[search] pane=0 window: 1 records, 0 added, 1 removed, 0 changed, in 0.0 ms (applied in 0.3 ms)
[auto] mvfile <tree>\sub -> <tree>\moved: Ok(())
[search] pane=0 window: 2 records, 3 added, 3 removed, 0 changed, in 4.1 ms (applied in 0.1 ms)
[auto] rows pane=0: a.txt [], also.txt  (moved) [], b.txt [], c.txt [], d.txt  (moved\deep) [], hold.txt [], s.txt  (moved) []
(the tab in the background) [auto] touch <tree>\bg.txt: Ok(())
[search] pane=0 window: 2 records, 1 added, 0 removed, 0 changed, in 3.2 ms (applied in 0.0 ms)
[search] pane=0 gen=4 content "needle" (text) in <tree>
[search] pane=0 gen=4 watching <tree>
[auto] rows pane=0: also.txt  (moved)  another Needle here [], hold.txt  this file holds the needle []
(moved\fresh.txt written with the word)
[search] pane=0 window: 2 records, 1 added, 0 removed, 0 changed, 1 files read, in 3.7 ms (applied in 0.2 ms)
(the word taken out of it)
[search] pane=0 window: 2 records, 0 added, 1 removed, 0 changed, 1 files read, in 3.6 ms (applied in 0.1 ms)
(the word written into a.txt)
[search] pane=0 window: 2 records, 1 added, 0 removed, 0 changed, 1 files read, in 3.5 ms (applied in 0.0 ms)
[auto] rows pane=0: a.txt  now with a NEEDLE [], also.txt  (moved)  another Needle here [], hold.txt  this file holds the needle []
(hold.txt deleted)
[auto] rows pane=0: a.txt  now with a NEEDLE [], also.txt  (moved)  another Needle here []
(3,000 files written into burst\ in 4.0 s, the name search open)
[search] pane=0 window: 524 records, 175 added, 0 removed, 0 changed, in 30.6 ms (applied in 0.5 ms)
[search] pane=0 window: 1086 records, 362 added, 0 removed, 0 changed, in 65.6 ms (applied in 0.9 ms)
[search] pane=0 window: 2377 records, 792 added, 0 removed, 0 changed, in 146.3 ms (applied in 1.8 ms)
[search] pane=0 window: 4562 records, 1521 added, 0 removed, 0 changed, in 219.0 ms (applied in 1.7 ms)
[search] pane=0 window: 451 records, 150 added, 0 removed, 0 changed, in 16.7 ms (applied in 0.8 ms)
```

Read with it:

- **hold.txt's row went by the re-stat, not by its window**: the window that came after logged "0
  removed" because the 5 s re-stat of the rows on screen had already taken the row off. Either takes it
  off; the soak counts both.
- **The first run of the check found a cost bug**, fixed before the commit: a folder whose own entry was
  only modified (because a file in it was made) was walked again, so one new file in `moved` read every
  file below it (that run's line: `3 records, 1 added, 0 removed, 2 changed, 5 files read`). Now only a
  folder that arrived is walked (`1 files read` above), and the netting test pins it.
- In that first run two `touch` steps named files below the root, which the driver refuses (`touch`
  takes a plain name); the script now writes those files itself.
- During the burst the watcher also logged `[watch] pane=0 tab=5 1001 records in the folder itself:
  refresh` for the search tab: that line is the subtree watch's own, written for the listing's row edits
  it could not keep; a search tab takes nothing from them and nothing is refreshed. The folder tab on
  the same tree in the background went stale by its own overflow, as before.
- Not checked by a script: the ninth watch (a background search tab pushed out says so), the second
  pane hidden, a watch lost (the root deleted), an overflow. Their paths are the code's, read and not
  run.

### Soak

`tools\soak-p5m3.ps1 -Minutes 10` (the branch's own script, with this milestone's steps) on the
release build of 5413138's code, 2026-11-15, harness-launched, with the coordinating session's leave and
the other stream holding: ran 10.3 min, window closed by the script, exit code 0, 0 panicked lines; 222
jobs done (failed 0, cancelled 0), soak refusals 0, waitops timeouts 0, menupicks refused 0, every "must
be 0" count 0.

```
search tabs following: took a watch 24, windows 4; the made file's row added 2, taken off by its window 2 or by the re-stat 0 (one per cycle between the two); stopped following 0 (0 expected)
content search: done 9 (three per cycle), files read 1812 (600 or more per cycle), found 12 (four per cycle)
sizes view following: watching 4 (two per cycle), windows 8 (three or more per cycle), the last window against F5's walk: matches 2 (one per cycle), mismatches 0 (must be 0); re-walks 0, stopped 0 (0 expected)
right-button drags: 6; drags cancelled: 15, dropped: 0 (must be 0); drag loops that found the state busy: 0; drag images set: 15
working set: start 155.5 MB at 5 s, end 218 MB at 600 s; private: 165.3 -> 221.1 MB
```

Two cycles reached this milestone's step (the third was cut by the time before it). In each, the open
`p16m6-*.txt` search took its watch (`[search] pane=0 gen=196 watching ...\twinfm-m5\src`), gained the row
one window after `touch` (`window: 2 records, 1 added, 0 removed`) and lost it one window after `rmfile`
(`window: 1 records, 0 added, 1 removed`). The 24 watches are every finished search tab of the run (the
content, class, expression and saved searches follow too); none stopped following. The display scale
was not read for this run.

### Gate

Not run yet. The code head for the gate build is 5413138 (the commit after it changes the soak
script, `CLAUDE.md` and the record only); the defaults triplet, built and handed to Jeremy by the
coordinating session. Its figures go here.

What the gate scenario meets of this milestone: no search tab, so nothing of the following runs.
`watch_tabs` sorts its candidates by one more key (the same order for folder tabs), each tab's
`follows()` makes one more test that fails at once for a folder tab (no search), and the re-stat's 5 s
tick looks through the tabs for waiting records (two tabs, none waiting). Nothing per frame or per row.

### Console checks (for Jeremy, after the gate, TwinFM started unelevated)

1. **A PDF saved while the search is open.** In `Downloads`, type `*.pdf` in the search box (right end
   of the path bar) and press Enter: a tab of results. Leave it open, save a PDF from the browser into
   Downloads: within a second or two a row for it appears in its sorted place, with no F5.
2. **Deleted and renamed.** In Explorer, delete one of the PDFs the tab lists: its row goes within a
   second or two. Rename another to `.txt`: its row goes. Rename a `.txt` to `.pdf`: a row appears.
3. **In a subfolder.** Copy a PDF into a folder below Downloads: its row appears with the folder beside
   the name.
4. **The tab in the background.** With the search tab open, switch to another tab of the pane, save
   another PDF, switch back: the row is there.
5. **A content search with Notepad.** In a folder of text files, click the page icon left of the search
   button so it lights up, type a word, Enter. Open a file that is NOT listed in Notepad, type the word
   in it, save: its row appears, showing the line. Take the word out and save: the row goes.
6. **A whole folder moved in.** Drag a folder holding PDFs into Downloads: rows for the PDFs in it
   appear.
7. **Too much at once.** Search `C:\` for `*.log` and leave the tab open for a few minutes while
   Windows works: sooner or later the status bar at the bottom of the pane reads "Not following changes:
   F5 searches again" (a whole drive changes faster than can be followed). F5 searches again.

### Calls made in this milestone, for Jeremy to check

- **The following ends at the first window whose records are not known**, where the kickoff said "the
  second overflow in a minute": after an overflow some changes are unknown, a search cannot read again
  only the part that changed (a sizes view walks again; a search run again unasked could take minutes),
  and carrying on would show a list that silently misses files. Saying so at once is the honest answer.
  Told to Jeremy by the coordinating session.
- **Content searches follow**, by the measurement above.
- **A background search tab comes after every folder tab** for the eight watches, and one that loses
  its watch stops following and says so when shown (it cannot make up what it missed).
- **A search stopped with Escape is not followed.**
- **A folder that arrives is walked on the follow thread**, however large; the windows that come
  meanwhile wait (up to 10,000 records, then the following ends).
- **A row found again keeps its place only under the same name letter for letter**; a rename that
  changes only the letter case takes the old row off and adds the new one.
- **The line carries two times** (`in T ms (applied in U ms)`): the thread's and the UI thread's, so a
  slow apply would show.
- **A lost or refused watch never runs the search again** (a folder tab refreshes there; a search
  refreshed is a search run again).

### Follow-ups, not done here

- The `[watch] ... records in the folder itself: refresh` line reads wrongly for a search tab (nothing
  is refreshed); the watcher writes it and does not know the tab's kind.
- After P16-M4 merges and main is taken in, the follow thread's content reads use the engine
  (`content::plain`); the test's needle is then built by it.
