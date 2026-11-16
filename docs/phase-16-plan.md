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

## P16-M4: plain content search on the fast engine

Branch `feat/p16-m4-content-search`, the search and files stream, cut from main at 3e4c201 (the kickoff;
it does not depend on P16-M2).

### Settled first: what changes in what matches

Measured, not recalled: the ignored test `fs::content::tests::probe_plain_against_the_engine` searched
for each of the 3,078 characters that have an upper or lower case form in every one of the 1,112,064
characters there are, by the comparison used until now (each character's lowercase, a final sigma as a
sigma, the text a substring) and by the engine (the text escaped, case-insensitive), and then tried 33
words. **194 letter pairs differ, in three groups; nothing differs for English text, punctuation or
accents.**

1. **The Turkish dotted capital İ (1 letter, 2 pairs).** Until now a plain `i` (or `I`) found a line
   whose only i is `İ`, so `ali` found `ALİ`; on the engine it does not. The old answer was half a
   rule: `istanbul` did not find `İSTANBUL` before either (İ lowercases to an i and a separate dot, so
   only a text ending in i matched). On the engine `İ` is found only by `İ`.
2. **Other shapes of the same letter (96 pairs): the engine finds more.** `s` and the long `ſ`; the
   Greek `μ` and the micro sign `µ`; the Greek symbol forms `ϐ ϑ ϕ ϖ ϰ ϱ ϵ` with `β θ φ π κ ρ ε`; the
   iota forms (`ι`, the combining iota below, U+1FBE); two accented Greek letters with their second
   code points; the old Cyrillic shapes U+1C80 to U+1C88 with `в д о с т ъ ѣ ꙋ`; `ṡ` and `ẛ`; the two
   st ligatures with each other. Before, each was found only by typing that exact shape.
3. **Letters whose case pairs are newer than the engine's tables (48 capital and small pairs, 96 pairs
   counted both ways):** the Beria Erfe script (U+16EA0 to U+16ED3, 25 pairs), Latin phonetic letters
   at U+1DF40 to U+1DF7F (16 pairs) and seven more Latin pairs (U+A7CE/A7CF, A7D2/A7D3, A7D4/A7D5,
   U+0277/A7DD, U+027C/A7E2, U+AB4B/AB6C, U+AB4C/AB6D). Rust's own tables know them and the `regex`
   crate's (1.13.1) do not yet, so a small one no longer finds its capital until the crate is bumped.

The same before and after (the probe's words): ordinary letters in either case, accents (`é` finds
`É`), `ß` and `ẞ`, the final sigma rule (`ς`, `σ` and `Σ` find each other), the Kelvin and Ångström
signs, the digraphs `ǆ ǅ`, Georgian, Cherokee, and every punctuation character typed as itself (`a.b`
finds `A.B` and not `AxB`). Neither treats `ss` as `ß`, `fi` as the ligature `ﬁ`, or `é` as e and a
separate accent.

One more visible change: a matching line with a character that lowercases to another length before the
match (`İ`) was shown from the line's start; it is now shown around the match, as every other line.

**Jeremy's decision** (2026-11-15; the three groups went to him through the coordinating session,
with the lean to take the engine's answers, before the change left its branch; asked "Take the new
engine's answers for the content-search letters?"): **"Yes, take them"**. So the change stands as
built: no extra letter for `İ`, the other shapes found, the newest letters left to the crate.

### What landed

- **Plain text is an expression** (41ca503). The box's plain text is compiled once as an escaped,
  case-insensitive expression (`fs::content::plain`: `regex::escape`, then `model::compile`, the
  builder `re:` uses) and each file's text is searched as it is; until now the text of every file was
  lowercased into a new string first. The row's line comes from the match's own place. `content::fold`,
  the old `first_match` and the `Needle` enum are gone: one function finds the line
  (`content::first_match`, P15-M4's `first_match_regex` renamed) and a content search's needle is a
  compiled expression (`enumerate::Content::needle`, an `Option<regex::Regex>`; none, which nothing
  builds today, finds nothing).
- **Not touched**: the live filter and the name search (names are short), the 16 MB cap, the binary
  test, the cloud rules, the `[search]` lines (a plain content search still logs `(text)`), saved
  searches and history entries (they keep the typed text and compile it when run).
- **The probe stays** as an ignored test (about two seconds; `cargo test --release -- --ignored
  probe_plain_against_the_engine --nocapture`), with the old comparison written out inside it, so the
  list can be measured again after a bump of the `regex` crate or of Rust.

### Timing

`tools/time-p15m4.ps1` (the 100 MB tree in `%TEMP%\twinfm-p15m4-time`, 400 files of about 256 KB, the
text in every fiftieth; one scripted run, a warm-up, then plain text and `re:` three times each,
alternating), 2026-11-15, the machine quiet both times (the coordinating session held the other
stream), 400 files read and 8 found in every search, 0 panicked lines.

| The search's `done` time, ms | plain text | `re:` |
|---|---|---|
| Before (main, 3e4c201) | 982, 985, 983 (median 983) | 99, 93, 107 (median 99) |
| After (41ca503) | 104, 120, 88 (median 104) | 100, 107, 89 (median 100) |

Plain text went from 983 ms to 104 ms and is level with `re:`. The before run's warm-up was 4,379 ms
(the files not yet in the file cache), the after run's 123 ms (cached from the before run), so the two
warm-ups are not compared. Phase 15 measured 968 against 86 on the same tree.

### Tests

`cargo test --release`: **365 tests** (364 before), `test result: ok. 365 passed; 0 failed; 3 ignored`
(the third ignored test is the probe); two full runs, one before the code commit and one before the
docs commit, no failure in either; `cargo build --release` finished.

- `fs::content::tests::plain_text_is_an_escaped_expression` (new): every typed character is itself
  (`a.b`, brackets, braces, `|`, `*`, `+`, `?`, `^`, `$`, a backslash, `$5.00 (usd)`); the cases that
  stay the same (accents, `ẞ`; `ss`, the `ﬁ` ligature and a separate accent not unfolded); **the
  differing cases pinned to the engine's answers**: the long s, the micro sign and mu, two Greek symbol
  forms found; `İ` not found by `i`, `ALİ` not by `ali`, found by `İ`. Group 3 is not pinned (it
  changes back by itself when the crate's tables catch up; the probe lists it).
- `the_first_matching_line_is_kept_and_cut` (kept, on the new path): the first line, trimmed; either
  letter case of the needle; Greek both ways (the sigma test the kickoff asked to keep); a line with
  `İ` before the match; the cuts of long lines.
- `files_are_read_up_to_the_cap` and the walker's
  `content_search_reads_text_files_and_stays_out_of_links_and_clouds` (kept): their plain needles are
  built by `plain`.
- The differing answers were not first seen failing as a test: they were measured on the old code by
  the probe before the change (its 194 pairs and the words marked DIFFERENT), and the new test pins
  what the probe showed for the engine.

### Scripted checks

`tools/check-p15m4.ps1` (P15-M4's, two runs in `%TEMP%\twinfm-p15m4-check\`, with two plain searches
added), 41ca503's release build, no `panicked` line. The plain content searches, from its output:

```
[search] pane=0 gen=6 content "todo" (text) in ...\files
[search] pane=0 gen=6 done: 2 found in 15 files read, 0 skipped (...), 2 folders walked, ... in 15 ms
[auto] rows pane=0: IMG_0500.jpg  (sub)  not a picture: todo is mid-line here [], todo.txt  (sub)  TODO: write the docs []
[search] pane=0 gen=7 content "\re:port" (text) in ...\files
[search] pane=0 gen=7 done: 1 found in 15 files read, ...
[auto] rows pane=0: fixme.c  (sub)  // see re:port 7 []
[search] pane=0 gen=8 content "TODO|FIXME" (text) in ...\files
[search] pane=0 gen=8 done: 0 found in 15 files read, ...
[search] pane=0 gen=9 content "X. //" (text) in ...\files
[search] pane=0 gen=9 done: 0 found in 15 files read, ...
[search] pane=0 gen=11 content "re:TODO|FIXME" (regex) in ...\files
[search] pane=0 gen=11 done: 3 found in 15 files read, ...
```

`todo` and `\re:port` find the rows P15-M4's record shows. The two new ones show that typed characters
are themselves: `TODO|FIXME` typed plainly finds nothing where the expression finds three files, and
`X. //` nothing where a dot read as "any character" would find `int x; // fixme later`. The rest of
the script (the filter, the name searches, the expressions, the saved search and its reopening) printed
what it did in P15-M4. The script prints blank exit codes, as it did then.

### Soak

`tools\soak-p5m3.ps1 -Minutes 10` (the branch's own script, unchanged: each cycle already runs three
plain content searches, which now take the new path) on 41ca503's release build, 2026-11-15,
harness-launched, with the coordinating session's leave and the other stream holding: ran 10.1 min,
window closed by the script, exit code 0, 0 panicked lines; 223 jobs done (failed 0, cancelled 0), soak
refusals 0, waitops timeouts 0, menupicks refused 0, every "must be 0" count 0.

```
content search: done 9 (three per cycle), files read 1812 (600 or more per cycle), found 12 (four per cycle)
search patterns: name searches with a class started 3, done with 30 or more found 3; content searches with an expression started 3, done with 2 found 3 (one each per cycle); not-a-pattern lines 0 (must be 0)
saved searches: saved 3, opened 3, run again and found 3, removed 3 (one each per cycle)
right-button drags: 6; drags cancelled: 15, dropped: 0 (must be 0); drag loops that found the state busy: 0; drag images set: 15
working set: start 154.6 MB at 5 s, end 233.9 MB at 601 s; private: 165 -> 225.1 MB
```

The content search counts are the ones P15-M4's soak gave (9 done, 12 found). The display scale was not
read for this run.

### Gate

Tag ab86, the defaults, 2026-11-16, run by Jeremy from the elevated console, built and handed over by
the coordinating session: `target-p16m4` (41ca503; the commit after it changes the record, `CLAUDE.md`
and the check script only) against `target-p15close` (4d335b7), six runs interleaved, every run's
checks ok, no panic.

```
Run         WPR InApp RenderP50 Checks
p15close-1 20.5 21.7  2.68      ok
p15close-2 20.4 19.7  2.66      ok
p15close-3 20.4 20.9  2.60      ok
p16m4-1    20.3 19.4  2.66      ok
p16m4-2    20.3 21.4  2.69      ok
p16m4-3    20.2 22.8  2.68      ok
```

p16m4's median is 20.3% against p15close's 20.4% (WPR main-thread busy), 0.1 apart with the candidate
the lower: **no gap**.
The whole sitting (ab84 to ab87, one elevated console) ran in the costlier state: render p50 was 2.60
to 2.79 ms in every run of both binaries, where Phase 15's gates sat at 2.0 to 2.3 ms, so no run of
either binary is under 20% and the figures are read like for like within the triplet only (the ab23
rule), never against a recorded number. The summary is `C:\traces\gate-ab86-summary.txt`.

What the gate scenario meets of this milestone: nothing. It runs no search; no row, list or timer
changed.

### Console checks (for Jeremy, after the gate, TwinFM started unelevated)

1. **A word in the repo.** In `C:\Users\jtkli\projects\twinfm`, click the small page icon at the left
   of the search box (right end of the path bar) so it lights up, type `scroll`, Enter. A tab "Text:
   scroll" fills with files, each row showing the line with the word. With logging on (Settings, Log),
   the log's `[search] ... done: ... in T ms` line gives the time; do the same in 0.6.0 for the figure
   to compare.
2. **A word in Documents.** The same in `Documents` with a word you know is in some files: the same
   files as 0.6.0 finds, sooner.
3. **Capitals do not matter.** Search `SCROLL` in the repo: the same rows as `scroll`.
4. **Punctuation is itself.** Search `fn main(` in the repo: only lines with exactly that text. Search
   `a.b`: lines with a real dot between a and b, not `axb`.
5. **The long line.** A match far along a very long line shows `…`, 40 characters of lead, then the
   match, as before.

### Calls made in this milestone, for Jeremy to check

- **The engine's answers are taken in all three groups**, the kickoff's lean and Jeremy's decision
  above. Keeping `i` finding `İ` would be one more letter in the expression built for a plain search; it was
  not done because the old rule was already half a rule.
- **Group 3 is left to the `regex` crate**: no table of TwinFM's own for 48 pairs of letters nobody
  here will meet; the probe says when they are back.
- **The probe is kept in the test module** (ignored, never part of the gates), as P15-M1's extract
  probe is.
- **No new soak cycle**: each cycle already runs three plain content searches, which now take the new
  path; the counts are the old ones.
- **`first_match_regex` is now `first_match`** (one function is left; the old name's reason is gone).
