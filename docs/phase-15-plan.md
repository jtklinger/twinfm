# Phase 15: Extract in the menu, search patterns, and the small items from daily use

Written 2026-11-13, the day Phase 14 closed (`phase-14-plan.md` § Phase 14 close). Phase 15's goal, from
Jeremy's kickoff on 2026-11-13: **unpack an archive from its right-click menu, search by glob classes and
regular expressions, and finish the small items the Phase 14 console checks and daily use turned up (a tab
menu, a progress line for folder sizes, a visible note for a saved search, the Size on disk column's grip and
sort).** The items this phase takes leave `docs/roadmap.md` when this plan merges (Jeremy's rule, 2026-10-26).

What Jeremy decided at the kickoff (2026-11-13):

- **Extract**: he sent a screenshot of the Windows 11 menu on `IBMPlexMono.zip` in Downloads, with no words.
  The lead read it as "the menu has no Extract" (the code has none, and the roadmap did not list it) and put
  it at the head of the recommended scope.
- **Scope**: "proceed with recommended scope". The recommendation was: Extract in the menu; a "Sizing N
  folders…" line and a more visible note for a saved search; a right-click menu on tabs; a grip and sort for
  the Size on disk column; glob and regular expression patterns in search. Not taken: the Slint fork's
  whole-row snap (as at the last two kickoffs), Explorer's other views, the preview's gaps; accessibility
  stays deferred.
- **How it runs**: two parallel streams with a coordinating session, as Phases 9 to 14 (in the recommendation).
- **The release**: Phase 14's work goes out as 0.5.0 before Phase 15 starts (in the recommendation); the
  release PR is its own, and it is published on his word.

The starting point is main at 55aef8f (the Phase 14 close; code as `target-p14close`, 61a905a, but #143's
focus call). What each item rests on today, from reading the code (four read-only subagents, 2026-11-13):

- **The Windows 11 menu** (`src/winmenu.rs`, P7-M2): `items()` builds the entries from a `Ctx` (the place, how
  many files, folders and drives, the clipboard, one type or not). It does not know whether a selected file
  is an archive: "inside an archive" there means the place. "Open in other pane" shows for exactly one folder
  or drive, so a zip, which Return opens as a folder, has none. "Compress to" shows in a folder or a search
  tab. There is no Extract.
- **Copying out of an archive** (P3-M4, P7-M3): a plain `Job::Copy` whose sources are paths inside the
  archive; the shell extracts (its zip folder for a zip, `ArchiveFolder` for the other formats), the progress
  sink reports each item, and undo recycles what landed. There is no job that takes an archive file and
  unpacks its whole root, the UI thread never reads an archive (so it cannot list the root items itself), and
  nothing makes the folder to extract into. `archivewrite::archive_path` numbers a new archive's name on a
  collision (`a (2).7z`). A password archive's question is the shell's own dialog, which a scripted run
  cannot answer.
- **Tabs** (`pane.slint` `TabButton`, P2-M3, P12-M5): a left press selects and drags, a middle click closes,
  the "+" opens one; **a right click is taken by the tab and does nothing**. `close_tab`, `open_tab` and
  `save_search` exist; closing several at once does not. TwinFM's own menus are the Windows 11 menu's popup
  fed a list of items (`winmenu::Cmd::Tool`, `open_tool_menu`, `run_tool`: the toolbar's View and More, and
  the history menu the same way).
- **Folder sizes** (P3-M3): Alt+Shift+Enter queues up to 500 folders (`TabSizes.pending`, each marked `all`),
  and `update_status` already runs when they are queued and as each lands; the status line says nothing about
  them, and a queued folder's Size cell stays empty.
- **Notes** (`note()` in `main.rs`): a line in the pane's status bar, 12 px in the muted colour, with no
  timer; "Saved search …" and "… is saved already" are two of them. The sidebar can scroll a row into view
  (`set-side-scroll`, `side-row-top`) but draws its current row only while it has the keys.
- **The Size on disk column** (P13-M3's follow-up): a fixed 100 px (`cols::ONDISK_W`) in lists of its own
  (`FileListOnDisk`, `FileListOnDisk32`), which already read its width as a property; `cols.rs` models three
  adjustable columns (`Col`, `Widths`, `ALL`) and treats this one apart (`ondisk_shown`: first to make way).
  Each entry keeps only its formatted text (`Entry::ondisk_text`), so there is nothing to sort by.
- **Search patterns** (`model::NameMatch`, P3-M2): the live filter and the recursive name search share one
  matcher: a case-insensitive substring, or, when the text has `*` or `?`, a glob anchored at both ends
  (`*` any run, `?` one character; no classes, no escaping). Content search (`fs/content.rs`) is a
  case-folded substring. The `regex` crate is in `Cargo.lock` only through build tools and the optional
  `skia` feature: it is not in the exe. Its license (MIT or Apache-2.0) is on `about.toml`'s accepted list.
  A search's pattern is stored as typed in saved searches, history places and tab titles.

## The milestones

Four milestones: two streams and a coordinating session, as Phases 11 to 14 ran. Within a stream the order is
the order of work.

**The menus stream** (one session; touches `src/winmenu.rs`, `src/fileops/`, `src/fs/archive.rs`,
`src/fs/archivewrite.rs`, the menu, tab and status paths in `src/main.rs`, `src/statusbar.rs`,
`src/tabs.rs`, pane.slint's `TabButton` and tab strip, app.slint's sidebar, `src/auto.rs`,
`tools/soak-p5m3.ps1`):

1. **P15-M1, Extract in the menu.**
   - **The items**: with archive files selected (one or several, every one an archive TwinFM opens as a
     folder: `fs::archive::is_archive_name`, so a zip and whichever of the other formats Windows'
     `ArchiveFolder` owns on the machine), in a folder or a search tab, the Windows 11 menu gains an
     **Extract to** submenu where Compress to stands for other files (the lead's call: the same shape as
     Compress to, one row in the menu):
     - **"<name>\"** (for several: "A folder each"): a new folder beside each archive, named after it
       without its extension (`a.tar.gz` gives `a`), numbered when the name is taken (`a (2)`).
     - **Here**: the archive's root items into the folder it is in.
     - **Other pane** (with two panes showing, the other on a folder): a folder named after each archive in
       the other pane's folder.
     Not shown inside an archive (an archive stored in an archive is copied out first; the lead's call) or on
     This PC. A selection mixing archives and other files gets no Extract.
   - **The job**: `Job::Extract { archive, dest_dir, named }` on the file operation worker, one per archive,
     queued in order. The worker lists the archive's root (the UI thread reads no archive), makes the named
     folder when asked, and copies the root items in. **Settled first**: which engine unpacks a zip. The
     shell's zip folder is the path a copy out takes today and is slow on many small files; the `zip` crate
     TwinFM already reads zips with is the alternative. The brief times both on `C:\traces\bigdir.zip`
     (40,000 files) into a scratch folder; the lead's lean is TwinFM's own reader for a zip when the shell
     takes more than twice as long, with the job's progress and Escape's cancel kept, and the shell for the
     other formats either way (they are read-only through Windows' handler, and it is the only reader).
   - **Collisions**: into a named folder there are none (the folder is new). Here: the shell's own replace
     question when the shell copies; with TwinFM's reader, TwinFM's replace question (`crate::ask`, the zip
     writer's Replace, Skip, Cancel), replace under `TWINFM_SOAK`.
   - **Passwords**: with the shell, its own dialog (brought to the front as today); with TwinFM's reader for a
     zip, TwinFM's password question, asked again while wrong, and `zippassword` in a script. A scripted run
     never extracts a password archive through the shell.
   - **Undo**: an extract into a named folder is undone by recycling that folder; Here by recycling the items
     that landed; an extract that replaced a file is not undone (P11-M3's rule).
   - **Open in other pane** for one archive file, as for one folder (`menu_request_for` takes the archive as
     the place to open; with one pane the split turns on, as for a folder).
   - **After it**: the cursor of the pane the folder landed in goes to the new folder when that pane still
     shows the destination (as a new folder's does).
   - Driver: `menuopen Extract to`, then `menupick <name>\` | `menupick Here` | `menupick Other pane` (none
     runs on the verb worker, so a script may pick them); `extract <pane> <here|named|other>` for the cursor
     row's selection without the menu. Lines: `[archive] extract <archive> -> <dest> (named | here): N root
     item(s), <engine>`, `[archive] extract <archive>: Done | Cancelled | Error("<why>") in T ms, F files, B
     bytes`, `[archive] extract: folder <name> taken: <name (2)>`. Tests: the menu's items for one archive,
     several, a mix, a zip inside an archive, a search tab (pure); the folder's name and numbering; a real
     extract of a zip, a 7z and a tar.gz on the worker in `%TEMP%\twinfm-p15m1-test-<pid>\`, with undo. Soak:
     per cycle, in `%TEMP%\twinfm-m5\arch`, a.zip extracted to its folder and Here, a.7z to its folder, each
     undone. Gate: the defaults triplet (the scenario opens no menu: it proves the rest untouched, and the
     record says so). Console checks: Extract on a zip in Downloads to its folder, Here and to the other pane;
     a 7z and a tar.gz; several zips at once; a password zip; Ctrl+Z after each; Escape during a large one;
     Open in other pane on a zip.
2. **P15-M3, the small items.**
   - **A menu on tabs**: a right click on a tab opens TwinFM's menu there (the popup the toolbar's View and
     More use): **Close** (Ctrl+W; off for a pane's only tab), **Close other tabs**, **Close tabs to the
     right** (each off when there is none to close), **Duplicate** (a tab after it on the same place; a search
     tab's duplicate runs the search again, a sizes view's walks again), **Save this search** (a search tab
     only, with Ctrl+D beside it). The right click does not switch to that tab; the commands act on the tab
     clicked (the lead's calls: Explorer's and the browsers' rule). Closing several is one call in
     `tabs.rs`, saved and drawn once. A right click on the strip's empty part does nothing, as today.
   - **Folder sizes say they have started**: while folders queued by Alt+Shift+Enter (or More's "Size all
     folders") are waiting or walking, the pane's status line ends "Sizing N folders…", N counting down as
     each lands, gone at none (`statusbar.rs`, beside `with_queue`; only the folders marked `all`, so sizing a
     selection keeps its own "so far" badge). And each of those folders' Size cells reads "…" until its size
     lands (the lead's addition: on `C:\` the line alone is far from the rows; a placeholder text, the sort
     reading it as no size, as today's empty cell).
   - **A saved search is seen to be saved**: on Ctrl+D, More's item, the headers' and the tab menu's, the
     sidebar scrolls the Searches row into view and shows it in the hover colour for 1.5 s (a Slint `Timer`
     and one property on the sidebar, nothing in the file rows), for a new row and for "saved already" alike;
     on the rail the same row, by its icon. The status line's note stays. No toast is built (the lead's call:
     one flash on the row itself says where the thing went).
   - Driver: `tabmenu <pane> <index>` (the right click), then `toolmenu <text>`; `rclicktab` is not added
     (`clickat <x> <y> right` reaches a tab through the real pointer path for the check that the click is
     wired). Lines: `[tab] pane=N menu on tab I: <items>`, `[tab] pane=N tab I: close others (K closed) |
     close to the right (K closed) | duplicate -> tab J`, `[size] pane=N sizing K folder(s)` (when the count
     appears and when it reaches none), `[sidebar] saved search <i> flashed`. Tests: the tab menu's items
     for a first, a middle, a last and an only tab and a search tab (pure); closing others and to the right
     in `tabs.rs` (the active tab's index after); the status line's text with folders queued. Soak: per
     cycle, three tabs opened, the menu's Close other tabs on the middle one, Duplicate, Close; a `sizeall`
     in `src\tree` with the `sizing` lines counted (one each way). Gate: the defaults triplet (the status
     line is built in `update_status`'s rhythm, not per frame; the record says what the scenario meets).
     Console checks: the menu on the active and on a background tab, each item; Size all folders on `C:\`
     (the line and the cells at once); a search saved twice with the sidebar scrolled away and on the rail.

**The lists and search stream** (one session; touches `src/cols.rs`, `src/model.rs`, `src/settings.rs`,
pane.slint's header, grips and `FileListOnDisk` pair, app.slint's column properties, `src/fs/enumerate.rs`,
`src/fs/content.rs`, `src/searchbox.rs`, the column, sort and search paths in `src/main.rs`, `Cargo.toml`,
`about.toml`, `src/auto.rs`, `tools/soak-p5m3.ps1`):

3. **P15-M2, the Size on disk column's grip and sort.**
   - **The width**: per pane, saved (`col_ondisk_left`, `col_ondisk_right`, default 100, 40 to 2,000 as the
     others; its floor the Size column's, the same text). A grip on the **Modified | Size on disk** boundary,
     named `ondisk`: a drag moves that boundary, Size on disk narrowing by what Modified grows (the rule of
     every grip: the column on the right gives to its left neighbour); a double-click fits it to its cells on
     screen and its header. It stays the first column to make way for the name (`cols::ondisk_shown`, now at
     its own width) and outside `ALL`'s hide order. "Reset column widths" resets it too.
   - **The sort**: a click on its header sorts by size on disk (`SortCol::OnDisk`), folders first and by name
     among themselves as the Size sort has them, name breaking a tie. Each entry keeps the number
     (`Entry::ondisk`, 8 bytes an entry, filled where the text is: the listing's directory read and the watch
     edit's stat). Saved with the tab (`sort = "ondisk"`). With the setting turned off, a tab sorted by it
     goes to `name`, as a `type` sort does when the Type column is turned off; in a pane too narrow to show
     the column the sort stays.
   - **Zero cost when off** (Jeremy's rule from P13-M3) holds: the rows of the four lists without the column
     gain nothing; the on-disk pair already read the width as a property.
   - Driver: `colwidth`, `coldrag` and `colfit` take `ondisk`; `sort <pane> 4`. Lines: the `[cols]` lines
     with a fourth width (`widths type T size S modified M ondisk O`), `[sort]` as the others. Tests: the
     drag, fit and floor rules with the fourth column (pure); the hide order unchanged; the sort's order with
     folders and empty values; the settings round trip with an old file; the sort falling back when the
     setting goes off. Soak: per cycle, the column on, its grip dragged and fitted, a sort by it and back,
     the column off. Gate: the defaults triplet (the entry grew by 8 bytes) **and** the triplet with
     `C:\traces\ondisk.toml` (the column on). Console checks: drag and fit the grip in both panes; sort by it
     in Downloads against Explorer's own Size on disk column; the setting off while sorted by it; a narrow
     pane.
4. **P15-M4, search patterns.**
   - **Glob classes**: `[abc]`, `[a-z]`, `[!abc]` in the glob the filter and the name search share
     (`model::NameMatch`), still anchored at both ends and case-insensitive; a `[` with no `]` is a plain
     character (so a name such as `notes [draft].txt` typed whole still matches itself: checked first by a
     test of today's patterns that contain a bracket).
   - **Regular expressions**, by a prefix in the box (the lead's call, against a third toggle button: the
     pattern is stored as typed in saved searches, history and tab titles already, so the prefix needs no new
     field in five structures and shows its mode wherever the search is named): **`re:`** followed by the
     expression, in the live filter, the name search and the content search. Matched anywhere in the file's
     own name (not its path), or anywhere in the text of a file, a line at a time for `^` and `$`;
     case-insensitive, with `(?-i)` in the expression for a case-sensitive one. The `regex` crate (linear
     time: no pattern can hang a search), its compiled size capped. A literal search for text starting `re:`
     is typed `\re:`.
   - **A half-typed expression**: while the text after `re:` does not compile, the live filter keeps the
     listing as it was before the edit and the status line says why ("Not a pattern yet: <the crate's
     reason>"); Return on one searches nothing and shows the same note (the lead's call: a list that empties
     at every keystroke of an expression would be worse than one that waits).
   - **Content search with a pattern**: the "first matching line" a result's row shows comes from the match's
     own place in the text (no second pass); the 16 MB cap, the binary test and the cloud rules unchanged.
     **Settled first**: what the crate adds to the exe's size and to a content search's time on a fixed tree
     (a plain text against the same text as `re:`), recorded; the plain substring path stays as it is for a
     pattern without the prefix.
   - The placeholder texts gain nothing (they are full); the search button's and the content toggle's
     tooltips name `*`, `?`, `[a-z]` and `re:`. `README.md` says so too.
   - Driver: no new step (`filter`, `search`, `csearch` and `searchreturn` take the text as typed). Lines:
     the `[filter]` line's kind reads `glob | substring | regex`, `[search] ... start` names it too, `[filter]
     pane=N "<text>": not a pattern yet (<why>)`. Tests: the classes (pure, with the bracket cases above);
     the prefix's parse and escape; a regex against names and against text with the line cut right; an
     invalid one; the walkers' tests with a pattern of each kind. `about.toml` and the packaging: the crate's
     notices appear in `THIRD-PARTY-LICENSES.html` (run `tools/package-release.ps1` once on the branch and
     read it). Soak: per cycle a name search with a class and a content search with `re:` in `src`, their
     `done` lines counted. Gate: the defaults triplet (the scenario filters nothing: it proves the rest
     untouched), plus the timed pair above recorded. Console checks: `*.[ch]`, `report-202[0-9]*`,
     `re:^IMG_\d{4}\.jpe?g$` in Downloads and Pictures; a content search for `re:TODO|FIXME` in the repo; a
     half-typed expression; a saved search with a pattern, opened after a restart.

**The coordinating session** (this one) serialises the machine, hands each gate to Jeremy, merges on his
word, keeps `docs/roadmap.md`, releases 0.5.0 (Phase 14's work) before the streams start, and runs the long
soak of the merged code at the phase's end (60 minutes of `soak-p5m3.ps1`, compared with Phase 14's 260 to
275 MB private at 125%; the display scale recorded with it).

Where the streams meet: `src/main.rs` (different functions), `src/auto.rs` (new steps), pane.slint (the tab
strip against the header and grips), `tools/soak-p5m3.ps1` (a cycle each) and `CLAUDE.md` § Measuring. The
second to merge takes main in first; the coordinating session orders the merges (the lead's expectation: M1,
M2, M3, M4).

Not in Phase 15, on the roadmap: the Slint fork's whole-row snap and upstreaming, Explorer's other views,
the preview's gaps, a search index, the scrollbars' other items, the owed checks (the dead share's last case,
OneDrive's error mark), accessibility.

Each milestone brings its driver steps and stderr lines (in `CLAUDE.md` § Measuring), tests for its pure
parts, and its soak cycle in `tools/soak-p5m3.ps1`. Every item in a milestone is a commit of its own, and the
record says per item: done, checked and found right, or left with the reason. Console checks after each
triplet, unelevated, and each check's line says where on screen the thing is (Phase 14's lesson).

**The gate is re-run after every milestone** under the protocol of 2026-10-12, as Phase 14 ran it:
`tools/wpr-triplet.ps1` elevated at the console by Jeremy, accessibility on, Narrator off, every other
TwinFM window closed, the split-and-scroll scenario on `C:\traces\bigdir`, clean binaries built with
`--locked` into their own `--target-dir target-<name>` from detached `git worktree add` checkouts. Three runs
per binary, interleaved, compared by median; a gap counts only when every reference run sits below every
candidate run or the medians differ by more than one point; each run's frame interval (16.4 ms at 60 Hz; a
session at about 22 Hz is rerun, not read) and `[perf] ... render[... p50=...]` read first; a fixed-order gap
the code cannot explain gets the order-swapped rerun first. The reference for the first Phase 15 gate is
**`target-p14close`** (61a905a); after each merge the new milestone's gate build becomes the next reference.
**Tags continue from ab80.** One wrapper per milestone (`gate-<milestone>.ps1`). The rules for two streams
hold (every stream reports to the coordinating session before any cargo run, gate build, soak, TwinFM window
or hand-over and waits for its reply; one soak at a time; tests run and read, then the commit, as two steps;
CI waited for before a merge unless Jeremy says otherwise; merges without `--delete-branch` from a worktree
session; `target\release` rebuilt only with Jeremy's window closed). The architecture rules from Phase 1 and
P12-M3 hold, and nothing per frame or per row without a trace first.

## Calls made in this kickoff, for Jeremy to check

Decisions Jeremy did not make and the lead settled, each reversible before its milestone starts:

- The screenshot meant "there is no Extract in the menu" (it came without words).
- Extract is one **Extract to** submenu: a folder named after the archive (the first item), Here, Other pane;
  for several archives, a folder each. Not offered inside an archive or for a mixed selection.
- Which engine unpacks a zip is decided by timing both on `bigdir.zip`; the other formats go through Windows'
  handler.
- Undo of an extract recycles the new folder (or, for Here, the items); an extract that replaced a file is
  not undone.
- A zip (or other archive) gets "Open in other pane", as a folder does.
- The tab menu: Close, Close other tabs, Close tabs to the right, Duplicate, Save this search; a right click
  does not switch tabs; Duplicate on a search tab searches again.
- Folder sizes: "Sizing N folders…" in the status line and "…" in each waiting folder's Size cell.
- A saved search's row is scrolled into view and shown in the hover colour for 1.5 s; no toast.
- The Size on disk grip resizes Size on disk against Modified; its width is saved per pane; turning the
  setting off moves a tab sorted by it to Name.
- Regular expressions are typed as `re:<expression>` in the same box (no new button); glob gains `[a-z]`
  classes; a half-typed expression leaves the list as it was and says why.
- The `regex` crate is added (MIT or Apache-2.0, on the accepted list; it grows the exe, by how much the
  milestone records).
- Two streams: the menus (M1, M3) and the lists and search (M2, M4), split by files; the long soak at the
  phase's end with the coordinating session.

## Open questions

Each brief settles first: the shell's zip folder against the `zip` crate for a whole-archive extract, by time
on `bigdir.zip`, and whether the progress sink's results hold for a root's full item list (P15-M1); what the
`regex` crate adds to the exe and to a content search's time, and whether today's saved patterns containing
`[` still match what they did (P15-M4); whether the eight bytes an entry gains show in the defaults triplet
(P15-M2's gate answers it).

## P15-M1: Extract in the menu

Branch `feat/p15-m1-extract`, the menus stream.

### Settled first

**Which engine unpacks a zip.** Both were timed on `C:\traces\bigdir.zip` (40,000 files at its root) by the
ignored test `fileops::archive::tests::probe_extract_engines` (never part of the gates), each into an empty
folder under `%TEMP%`, one after the other, 2026-11-13, with the coordinating session's clearance:

| Engine | Time | What it reported |
|---|---|---|
| TwinFM's own reader (`fs::unzip`, the `zip` crate) | 23.1 s | Done |
| The shell (a copy of the 40,000 root items out of the zip folder, `IFileOperation`) | 802.6 s | done, 40,000 root items, the sink reported 40,000 items |

The shell is about 35 times slower, far past the kickoff's "more than twice": **a zip is unpacked by TwinFM's
own reader**, and the other formats go through the shell, which is the only thing that reads them.

**Whether the progress sink's results hold for a root's full item list.** What is known: for 40,000 root
items the sink reported 40,000 items, one per root item, so the count holds. What is not known: whether
`own_results` (the pass that keeps, of the sink's items, those that are the job's own top-level items) keeps
all 40,000. That was the probe's third step, and it did not finish: it compares every result against every
item, and after 54 minutes at full CPU (about 68 minutes of processor time) it was still running, so it was
stopped and no figure exists for it. The Extract job does not depend on the answer: for the shell's formats
it never runs that pass and reads the items that landed from the disk (the root's names, looked for in the
destination), and for a zip the reader knows what it wrote. The same pass still runs for ordinary copies and
moves, where a job of tens of thousands of top-level items would spend a long time in it after the shell had
finished; that is older than this milestone and is listed below as a follow-up, not fixed here.

### What landed

- **Extract to** in the Windows 11 menu, a submenu above Compress to, when every selected item is a file
  TwinFM opens as a folder (in a folder or a search tab; not inside an archive, not on This PC, not for a
  selection with anything else in it): **`<name>\`** (several archives: "A folder each"), **Here**, and
  **Other pane** when two panes show and the other is on a folder.
- **The folder's name**: the archive's without its extension (`a.tar.gz` gives `a`), numbered when taken
  (`a (2)`): `fs::archive::folder_name`, `extract_folder`.
- **The job**: `Job::Extract { archive, dest_dir, named }` on the file operation worker, one per archive
  (`fileops::archive::extract`). The worker lists the root and makes the folder; the UI thread reads no
  archive.
- **A zip by TwinFM's own reader** (`src/fs/unzip.rs`): the job's percentage in the status bar, TwinFM's
  password question, TwinFM's replace question for Here, each file given its entry's time. Names are split
  at `/` and `\` with empty, `.` and `..` parts dropped, so nothing lands outside the destination; a zip
  holding a name with `:` is refused whole.
- **The other formats through the shell**: a copy of the root items, with Windows' own progress window and
  its own replace and password questions.
- **Escape stops a running extract** before its next file, also when it is the only job; what was written
  stays.
- **Undo** recycles the folder made, or for Here the root items that landed; an extract that landed on an
  item already there is not undone (P11-M3's rule).
- **The cursor** of each pane showing the folder a named extract was made in goes to the new folder.
- **Open in other pane** for one archive file.
- **Driver**: `menuopen Extract to` then `menupick <name>\ | Here | Other pane`; `extract <pane>
  <here|named|other>`. Lines in `CLAUDE.md` § Measuring.
- **The soak** (`tools/soak-p5m3.ps1`) gains an extract cycle; `tools/check-p15m1.ps1` is the scripted check.

### Tests

`cargo test --release`: `test result: ok. 353 passed; 0 failed; 2 ignored` (348 before; the two ignored are
the probe above and the older `measure_walks`). New: `fs::unzip`'s two (names kept below the destination; a
zip unpacked with its folders, times and password), `fs::archive`'s folder names, `winmenu`'s Extract to
items, `fileops::archive::extracts_on_the_worker` (a zip, a 7z and a tar.gz through the real worker), and a
password zip's extract inside `zip_jobs_on_the_worker`. No test was run twice.

### Scripted checks

`tools/check-p15m1.ps1`, one scripted run in `%TEMP%\twinfm-p15m1-check\`; it ran to its `quit` step with no
panic and no timeout (the exit code printed blank in that run; the script has waited for the exit since).
From its stderr:

```
[winmenu] pane=0 seq=1 shown after 250 ms (...): [Cut Copy Rename Share Delete] Open | Open with > | Open in other pane | Open in terminal | --- | Extract to > | Compress to > | Copy path | Properties | --- | Show more options
[winmenu] submenu "Extract to" at (495, 494): [] a\ | Here
[archive] extract ...\work\a.zip -> ...\work\a (named): 2 root item(s), TwinFM's reader
[archive] extract ...\work\a.zip: Done in 12 ms, 3 files, 19 bytes, 2 of 2 root item(s) landed
[undo] recorded "Extracted a.zip to a": Some(Delete { sources: ["...\\work\\a"], permanent: false })
[auto] scrollpos pane=0 content-y 0.0 current-row 0
[undo] done id=2 Done: Undid: Extracted a.zip to a
[archive] extract ...\work\a.zip -> ...\work (here): 2 root item(s), TwinFM's reader
[undo] recorded "Extracted 2 items from a.zip": Some(Delete { sources: ["...\\work\\top.txt", "...\\work\\tree"], permanent: false })
[winmenu] submenu "Extract to" at (495, 494): [] a\ | Here | Other pane
[archive] extract ...\work\a.zip -> ...\other\a (named): 2 root item(s), TwinFM's reader
[auto] scrollpos pane=1 content-y 0.0 current-row 0
[archive] extract: folder a taken: a (2)
[archive] extract ...\work\b.7z -> ...\other\b (named): 2 root item(s), the shell
[archive] extract ...\work\b.7z: Done in 145 ms, the sink reported 8 item(s), 2 of 2 root item(s) landed
[archive] extract ...\work\c.tar.gz: Done in 116 ms, the sink reported 8 item(s), 2 of 2 root item(s) landed
[winmenu] submenu "Extract to" at (495, 530): [] A folder each | Here | Other pane
[fileops] done id=11 pane=0 tab=1 Done: Extracted a.zip to a
[fileops] done id=12 pane=0 tab=1 Done: Extracted d.zip to d
[archive] extract ...\work\pw.zip: Error("this zip is password-protected and the script gave no password") in 7 ms
[archive] password for ...\work\pw.zip (ZipCrypto): from the script
[archive] password for ...\work\pw.zip: right
[archive] extract ...\work\pw.zip: Done in 6 ms, 1 files, 5 bytes, 1 of 1 root item(s) landed
[ask] shown replace "Replace files in work?" (from fileops, 0 waiting)
[ask] replace answered Skip after 154 ms
[archive] 3 file(s) already in ...\work: skip
[ask] replace answered Replace after 155 ms
[archive] 3 file(s) already in ...\work: replace
[fileops] pane=0 note: Cannot undo: it replaced a file, which cannot be brought back
[menu] open in other pane: ...\work\a.zip
[archive] extract C:\traces\bigdir.zip -> ...\other\bigdir (named): 40000 root item(s), TwinFM's reader
[fileops] Escape: cancelling 0 queued operation(s) up to id 20, an extract among the jobs
[archive] extract C:\traces\bigdir.zip: Cancelled in 3226 ms, 4820 files, 4820 bytes, 4820 of 40000 root item(s) landed
```

The menu for a zip and a text file together showed no Extract to. The picture of the menu with the submenu
open (`%TEMP%\twinfm-p15m1-check\menu.png`, the software renderer) was looked at: Extract to sits above
Compress to, its submenu beside it reads `a\`, `Here`.

### Soak

`tools\soak-p5m3.ps1 -Minutes 10` on the merged branch's release build (505ec9e's code, v0.5.0), 2026-11-13,
display scale 125%, harness-launched: ran 10.1 min, window closed by the script, exit code 0, 0 panicked
lines; 234 jobs done (failed 0, cancelled 0), soak refusals 0, waitops timeouts 0, menupicks refused 0.

```
extract: by TwinFM's reader 6 (two per cycle), through the shell 3 (one per cycle), done 9 (three per cycle), not done 0 (must be 0), undone 9 (three per cycle)
Windows 11 menu: shown 60 (eight per cycle with P15-M1's Extract to, and each rebuild again), rebuilt with the worker's answer 0, picks run 51 (five per cycle), ZIPs made 3, new folders 6 (two per cycle: the menu's and the toolbar's), submenus 15 (three per cycle), classic menus from Show more options 3, menupicks refused 0 (must be 0)
right-button drags: 6; drags cancelled: 15, dropped: 0 (must be 0); drag loops that found the state busy: 0; drag images set: 15
working set: start 179.7 MB at 5 s, end 256.6 MB at 600 s; private: 199.7 -> 262.9 MB
```

Every other count read as its "per cycle" figure says (three full cycles). **The right-button drag the lists
stream's soak saw hang did not show here**: 15 drags started (`DoDragDrop starts`, 6 of them with the right
button), 15 returned, 15 cancelled, and no `the scripted drag is still running after 20 s` line. The working
set's rise over ten minutes is the soak's usual one (the phase's 60-minute soak is the memory check).

### Gate

**Passed.** Tag ab80, 2026-11-14 08:09, 505ec9e (`target-p15m1`) against `target-p14close`, the defaults,
three runs each, interleaved (the figures are the coordinating session's, from the gate's summary):

| Binary | WPR main-thread busy, three runs | Median |
|---|---|---|
| `p14close` (reference) | 21.1, 17.9, 18.0 | 18.0 |
| `p15m1` (candidate) | 17.9, 18.3, 18.5 | 18.3 |

RenderP50 2.04 to 2.22 ms across the six runs (one session state), every run's checks ok. No gap that
counts: the medians differ by 0.3 points, and the first reference run's 21.1 is the outlier. Jeremy:
"Merge both"; merged as 81a6101. The console checks below had not been run by Jeremy at the merge.

Wrapper `tools/gate-p15m1.ps1`: tag ab80, `target-p15m1` against `target-p14close`, the defaults
only (the scenario opens no menu and runs no file job, and the wrapper checks that the candidate's runs log
no `[archive] extract` line). The gate build and the run are the coordinating session's and Jeremy's.

### Console checks (for Jeremy, after the gate, TwinFM started unelevated)

1. **A zip to its own folder.** In a folder with a zip (Downloads will do), right-click the zip. The menu
   shows "Extract to" just above "Compress to"; point at it and pick the first entry, the zip's name with a
   backslash. A folder of that name appears in the same list with the cursor on it, and the status bar reads
   "Extracted <zip> to <name>".
2. **Ctrl+Z after it.** The folder goes (to the Recycle Bin); the status bar reads "Undid: Extracted ...".
3. **Here.** Right-click the zip, Extract to, Here: the zip's top-level items appear beside it in the same
   list. Ctrl+Z takes them away again.
4. **Other pane.** With two panes showing (the toolbar's Two panes, or F3) and the other pane on any folder,
   Extract to has a third entry, "Other pane": the new folder appears in the other pane, cursor on it.
5. **The same folder twice.** Extract the same zip to its folder twice without undoing: the second folder is
   named `<name> (2)`.
6. **A 7z and a tar.gz** (`C:\traces\bigdir.7z` is large; any small one is better): Extract to, the name.
   Windows' own progress window may flash; the folder appears as for a zip.
7. **Several zips.** Select two or more zips, right-click one: the first entry reads "A folder each", and a
   folder per zip appears. Ctrl+Z, once per zip, removes them.
8. **A zip and another file together.** Select a zip and a text file, right-click: no "Extract to" in the menu.
9. **A password zip** (`C:\traces\password-twinfm.zip`, password `twinfm`): Extract to its folder. TwinFM's
   own password question appears in the middle of the window; a wrong password asks again, Cancel leaves
   nothing behind but an empty status, the right one unpacks.
10. **Here into a folder that already has the files.** Do Here twice: the second time TwinFM asks "Replace
    files in <folder>?" with Cancel, Skip, Replace. After Replace, Ctrl+Z says "Cannot undo: it replaced a
    file, which cannot be brought back".
11. **Escape during a large one.** Extract `C:\traces\bigdir.zip` to the other pane (on a scratch folder);
    the status bar counts up a percentage. Press Escape with the keys in a file list: the status bar reads
    "Cancelling the extract", then "Cancelled", and the folder holds the files written so far.
12. **Open in other pane on a zip.** Right-click a zip, "Open in other pane": the other pane shows the zip's
    contents as a folder.

### Calls made in this milestone

- **Extract to sits above Compress to**; both show for an archive file.
- **Escape stops a running extract** (the kickoff did not say). The files written so far stay; a stopped or
  failed extract leaves its partly filled folder and records no undo.
- **One bad entry stops a zip's extract** with an error rather than being skipped, and a zip with a `:` in
  any name is refused before anything is written.
- **For the shell's formats, what landed is read from the disk**, not from the sink's list (see Settled
  first).
- **Other pane is hidden**, not greyed, unless two panes show and the other is on a real folder.
- **Open in other pane** is offered for one archive anywhere a file menu shows; Extract only in a folder or
  a search tab.
- **After "Skip" for every file** the status still reads "Extracted N items from <zip>" though nothing was
  written (nothing to undo is recorded). Left as it is; say if it should read otherwise.
- The password question names "AES-256" for any AES zip (128 and 192 too), as the zip writer's does.

### Follow-ups, not done here

- `own_results` is quadratic in a job's top-level items (Settled first): an ordinary copy or move of tens of
  thousands of items out of one folder would spend many minutes in it after the shell had finished. For the
  roadmap unless Jeremy wants it sooner.
