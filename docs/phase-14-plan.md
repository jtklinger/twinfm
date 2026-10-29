# Phase 14: saved searches, a sizes view that keeps up, and the look's leftovers

Written 2026-10-29, the day Phase 13 closed (`phase-13-plan.md` § Phase 13 close). Phase 14's goal, from
Jeremy's kickoff on 2026-10-29: **keep a search and run it again from the sidebar, let the sizes view follow
the disk as it changes, and finish the look's two leftovers (hover on file rows and tiles, a held press on a
scrollbar's track).** The items this phase takes leave `docs/roadmap.md` when this plan merges (Jeremy's
rule, 2026-10-26).

What Jeremy decided at the kickoff (2026-10-29), his picks from the Phase 13 close's list:

- **Scope**: "Save a search tab", "Sizes view follows changes", "The look's leftovers". Not taken: the Slint
  fork's whole-row snap (it stays on the roadmap, with upstreaming, his call when); accessibility stays
  deferred.
- **What a saved search shows when opened**: "Run it again" (the folder and the text are kept; opening it
  searches afresh, as Windows' saved searches do).
- **Where saved searches live**: "A sidebar group" (a Searches group under Bookmarks, renamed and reordered
  as bookmarks are; a click opens it in a tab).
- **Content searches**: "Yes, the same way" (a saved search remembers whether it searched names or text in
  files).

The starting point is main at 4d92d3a (the Phase 13 close; code as `target-p13close`, 13cfe82). What each
item rests on today, from reading the code:

- **Search tabs** (P3-M2, P10-M2, P11-M1): `start_search(pane, pattern, content, history_root)` (main.rs)
  opens a new tab titled by the pattern (`Text: <text>` for a content search) and walks on a worker; a search
  is already a place in a tab's history (`Place`, and `SavedPlace { path, search, content }` in
  `settings.rs` since P11-M3), so Back re-runs it. A search tab is never saved with the open tabs: the tab
  lists (`left_tabs`, `right_tabs`, `SavedTab`) hold folders and, since P13-M3, sizes views. Ctrl+D in a
  search tab bookmarks the search's root folder (`bookmark_add` takes the tab's `path`).
- **The sidebar** (`src/sidebar.rs`, P12-M5): groups Drives, Places, Recent (when it has any), Bookmarks,
  in that order; bookmarks are `[[bookmark]]` tables (`path`, optional `title`), renamed with F2 or the
  row's editor, reordered by a drag (Slint's pointer, `drag_target`, `move_bookmark_to`) or `bookmark move`,
  removed from the row's menu; the headers have right-click menus (Recent's "Clear recent", Bookmarks'
  "Bookmark this folder").
- **The sizes view** (`src/sizeview.rs`, `src/fs/dirinfo.rs`, P13-M3): one walk per view on a `sizeview`
  thread into a `Tree` of every folder (totals: on disk, logical, files, folders, skipped; no file entries);
  never watched (`[watch] pane=N no watch for <root>: sizes view`); a delete or a move out made by TwinFM
  takes the items off every total above them at once (`sizes_removed`); anything else needs F5, which walks
  the whole root again. The Name column takes the room left, so at a wide pane Share, Size on disk and Files
  sit at the far right (P13-M3 console check).
- **Folder sizes' subtree watch** (P3-M3, P7-M4): a tab with folder sizes watches every folder below its own
  (`bWatchSubtree`), reads the records, drops or re-walks the size of the sized folder a change falls under
  (`stale <path>: re-walk in N s`, after max(2 s, twice its last walk)), and gives up after two overflows.
- **Hover**: TwinFM's chrome hovers (buttons, tabs, the sidebar's and the crumb dropdown's rows, without a
  fade since they scroll); **file rows and tiles have no hover at all** (`phase-12-plan.md` § P12-M1: nothing
  that scrolls animates, and a per-row `has-hover` was never traced).
- **The scrollbars** (`theme.slint` `VScrollBar`, `HScrollBar`, P12-M2): a press on the track pages once
  towards the pointer and does not repeat while held.

## The milestones

Three milestones: two streams and a coordinating session, as Phases 11 to 13 ran (the lead's proposal).
Within a stream the order is the order of work.

**The sidebar and look stream** (one session; touches `src/sidebar.rs`, `src/settings.rs`, the sidebar's and
pane's Slint files, `src/ui/theme.slint`, the search and sidebar paths in `src/main.rs`, `src/toolbar.rs`'s
More menu, `src/auto.rs`, `tools/soak-p5m3.ps1`):

1. **P14-M1, saved searches.**
   - **Saving**: in a search tab (a name or a content search, finished or still walking), **Ctrl+D saves the
     search** instead of bookmarking its root (the lead's call: Ctrl+D is "keep this place", and the root is
     one click away in the crumbs); also ⋯ More's "Save this search" (shown in a search tab only) and the
     Searches header's menu (below). Tabs have no right-click menu today, and none is added. What is kept: the root, the pattern, name or content. A search
     saved already (same root, pattern and kind, the root compared as bookmarks compare folders) stays where
     it is, with the note "<title> is saved already". The note on saving: "Saved search <title>".
   - **The group**: **Searches**, after Bookmarks, shown only while it has a row (as Recent is). Each row a
     Lucide `search` icon (`file-search` for a content search) and its title; the default title is
     `"<pattern>" in <folder name>` (`Text "<text>" in <folder name>` for a content search), the folder's
     name as the sidebar shows a bookmark's; the tooltip the root's full path. Rename (F2, the row's editor;
     empty goes back to the default title), reorder by drag and `move`, remove from the row's menu ("Remove
     saved search"), the header's menu "Save this search" (enabled in a search tab) — each as bookmarks do,
     with the same code where it can be shared (the lead's call: generalise `move_bookmark_to` and the drag
     target over a group, not copy them).
   - **Opening**: a click (or Return with the sidebar's keys) runs the search again in **a new tab of the
     active pane**, as Return in the search box does (`start_search`, the same refusals: a content search
     rooted in a cloud location or an archive, a root that is gone — "<root> is not there any more"); a
     middle click the same in the other pane (the lead's call). A drop onto a saved search's row is refused
     (it is no folder); the bookmark gap rules (P12-M5) do not extend to this group.
   - **Saved** in the settings as `[[search]]` tables (`path`, `pattern`, `content` when true, `title` when
     renamed); an older file has none. Search tabs themselves are still not restored at start (Jeremy chose
     the group, not restored tabs).
   - Driver: `searchsave <pane>` (Ctrl+D's command in the pane's active tab), `searchopen <index>` (a click on
     saved search `<index>`), `searchremove <index>`, `searchrename <index> <title>`, `searchmove <index>
     up|down`; `sidebar click <row>` and the sidebar keys reach the rows as any. Lines: `[sidebar] saved search
     added "<title>": <root> "<pattern>" name|content`, `[sidebar] "<title>" is saved already`, `[sidebar]
     saved search <i> opened: <root> "<pattern>" name|content`, `... removed`, `... renamed "<a>" -> "<b>"`,
     `... moved F -> T`. Tests: the default title, the duplicate rule, the settings round trip with an old
     file, the group's rows with and without saved searches, the shared reorder. Soak: per cycle a content
     search in `src` saved, opened from the sidebar into a tab (its `done` line counted), the tab closed, the
     row removed. Gate: the defaults triplet (the scenario has no saved search: it proves the rest untouched,
     and the record says so). Console checks: save from Ctrl+D, More and the Searches header's menu; open, rename, drag,
     remove; a saved content search; a saved search whose root was deleted; the settings file read back after
     a restart.
2. **P14-M3, the look's leftovers.**
   - **Hover on file rows and tiles**: the row (or tile) under the pointer gets `Theme.muted`, as the
     sidebar's rows do, in every details list variant and both grids; never under the selection's colour or
     during a band or a drag (the lead's calls). Settled first, by a trace of the gate scenario with the
     pointer over a list while it scrolls: **one hover rectangle per list host**, placed from the pointer's y
     and the list's `content-y` (no per-row binding, no per-row `TouchArea` state), against a per-row
     `has-hover` in the row components; the lead's recommendation is the one rectangle (P3-M1's lesson: a
     per-row binding costs on 40,000 rows). No fade (it scrolls; P12-M1's rule), unless the trace shows the
     single rectangle's fade free. Off while the pointer is outside the list or over the scrollbar's strip.
   - **The track's repeat**: a press held on a scrollbar's track pages once, then after 500 ms again every 50
     ms until the thumb reaches the pointer or the button is released (Windows' own scrollbar timing; the
     lead's call), in both bars, every list that uses them.
   - Driver: `hover <x> <y>` exists (P7-M4); `trackhold <pane> <top|bottom> <ms>` presses the pane's list
     track 8 px from its top or bottom end and holds it `<ms>` through Slint's pointer path. Lines: `[auto]
     trackhold pane=N: content-y A -> B in K pages`. Tests: the hover row from a pointer y (pure: details rows,
     grid rows and columns, the empty space below), the repeat's stop rule. Soak: per cycle a hover across
     both lists and a held track press each way. Gate: the defaults triplet and the grid-thumbs triplet
     (`C:\traces\grid-thumbs.toml`), plus an in-app pair with the pointer parked over a scrolling list
     (`hover` before the scroll) recorded. Console checks: light and dark beside Explorer; the hover under the
     selection, during a drag, a band and a rename; a held press on each list's track.

**The sizes stream** (one session; touches `src/sizeview.rs`, `src/fs/dirinfo.rs`, the sizes view's paths in
`src/main.rs`, `src/fs/watch.rs` where the subtree watch is shared, pane.slint's `SizeList`, `src/auto.rs`,
`tools/soak-p5m3.ps1`):

3. **P14-M2, the sizes view follows changes.**
   - **Watched**: a sizes view in front takes a subtree watch on its root once its walk is done (P3-M3's,
     with its records read), and a view in the background keeps it, as a background tab with folder sizes
     does (P5-M1); the watch cap (8) counts it. Changes made by TwinFM's own jobs are explained by the job as
     today (`sizes_removed` keeps taking deletes and moves out at once, and their records net to nothing).
   - **What a change does** (the lead's design, settled first by measuring the `Tree`'s memory on a walk of
     `C:\`): each folder in the `Tree` keeps the totals of its **own files** beside its subtree's, so a change
     to a file is applied by **re-reading that one folder's directory information** (one `dirinfo::read`, no
     walk) and adding the difference to every total above it; a folder created is walked (its subtree only)
     and added; a folder removed is taken off; a rename within the root moves the folder's subtree in the tree
     (an old name with a new one in the same window, as the watch pairs them); a move out of the root is a
     removal, a move in a creation. Re-reads are gathered per window (250 ms, the watch's back-off to 2 s) on
     the view's `sizeview` thread, never on the UI thread; the rows are built again as the walk's are, the
     cursor and selection kept by path; an open level's file rows are listed again when its folder changed.
   - **When it gives up**: an overflow (past the records' buffer) or a change under a folder the walk skipped
     walks the whole root again, the old figures shown meanwhile (`[sizeview] ... re-walk: <why>`); after two
     overflows in a minute the view stops watching and its status says "Not following changes: F5 walks
     again" (the lead's call, P3-M3's rule of two).
   - **The columns' place** (the Phase 13 console check's finding, the lead's addition): the Name column
     stops at the width its longest name on screen needs (measured as `colfit` measures), at least 240 px, so
     Share, Size on disk and Files sit beside the names at a wide pane; the rest of the row stays empty.
     Reversible at the console check.
   - Driver: `waitsizeview` also waits for the re-reads in flight; no new step (`touch`, `rmfile`, `mvfile`
     and the shell's jobs make the changes). Lines: `[sizeview] pane=N gen=G watching <root>`, `[sizeview]
     pane=N window: R records, F folders re-read, C created, D removed, M moved, in T ms`, `[sizeview] pane=N
     <folder>: S -> S2 bytes on disk (K totals above)`, `[sizeview] pane=N re-walk: overflow | a skipped folder
     changed`, `[sizeview] pane=N not following changes (two overflows)`. Tests: applying a change to a tree
     (pure: a file grown, a folder created, removed, renamed within, moved out), the own-files totals summing
     to the subtree's, a real watch on a scratch tree in `%TEMP%\twinfm-p14m2-test-<pid>\`. Soak: per cycle a
     sizes view of `src` left open while the cycle's `touch`, `rmfile` and `mvfile` run under it, its totals
     checked by the lines against a fresh walk at the cycle's end (must match). Gate: the defaults triplet
     (the scenario never opens the view; the record says so), plus an in-app run of the view on `C:\` left
     open for 10 minutes, its memory and the re-reads per minute recorded (Windows writes under `C:\` all the
     time). Console checks: a sizes view of Downloads while a large download lands; a folder deleted and a
     folder renamed in Explorer under an open view; `C:\` left open while working.

**The coordinating session** (this one) serialises the machine, hands each gate to Jeremy, merges on his
word, keeps `docs/roadmap.md`, and runs the long soak of the merged code at the phase's end (60 minutes of
`soak-p5m3.ps1`, compared with Phase 13's 247 MB private).

Where the streams meet: `src/main.rs` (different functions), `src/auto.rs` (new steps) and
`tools/soak-p5m3.ps1` (a cycle each). The second to merge takes main in first; the coordinating session
orders the merges (the lead's expectation: M1, M2, M3).

Not in Phase 14, on the roadmap: the Slint fork's whole-row snap and upstreaming, the scrollbars' other
items (arrow buttons, the hovered thumb over the Modified column's last pixel, a grid's tile count at under
10 px), the tooltip's delay and place (Slint's), the Size on disk column's grip and sort, the owed checks (the
dead share's last case, OneDrive's error mark), accessibility.

Each milestone brings its driver steps and stderr lines (in `CLAUDE.md` § Measuring), tests for its pure
parts, and its soak cycle in `tools/soak-p5m3.ps1`. Every item in a milestone is a commit of its own, and the
record says per item: done, checked and found right, or left with the reason. Console checks after each
triplet, unelevated.

**The gate is re-run after every milestone** under the protocol of 2026-10-12, as Phase 13 ran it:
`tools/wpr-triplet.ps1` elevated at the console by Jeremy, accessibility on, Narrator off, every other
TwinFM window closed, the split-and-scroll scenario on `C:\traces\bigdir`, clean binaries built with
`--locked` into their own `--target-dir target-<name>` from detached `git worktree add` checkouts. Three runs
per binary, interleaved, compared by median; a gap counts only when every reference run sits below every
candidate run or the medians differ by more than one point; each run's frame interval (16.4 ms at 60 Hz; a
session at about 22 Hz is rerun, not read) and `[perf] ... render[... p50=...]` read first; a fixed-order gap
the code cannot explain gets the order-swapped rerun first. The reference for the first Phase 14 gate is
**`target-p13close`** (13cfe82); after each merge the new milestone's gate build becomes the next reference.
**Tags continue from ab76.** One wrapper per milestone (`gate-<milestone>.ps1`). The rules for two streams
hold (every stream reports to the coordinating session before any cargo run, gate build, soak, TwinFM window
or hand-over and waits for its reply; one soak at a time; tests run and read, then the commit, as two steps;
CI waited for before a merge unless Jeremy says otherwise; merges without `--delete-branch` from a worktree
session). The architecture rules from Phase 1 and P12-M3 hold, and nothing per frame or per row without a
trace first.

## Calls made in this kickoff, for Jeremy to check

Decisions Jeremy did not make and the lead settled, each reversible before its milestone starts:

- Ctrl+D in a search tab saves the search (it bookmarked the root before); ⋯ More and the Searches header's
  menu offer it too.
- The Searches group sits after Bookmarks and shows only while it has a row; the default title is `"<pattern>"
  in <folder>` (`Text "<text>" in <folder>` for text in files); a search saved twice stays one row.
- A click opens the saved search in a new tab of the active pane; a middle click in the other pane.
- Search tabs left open are still not restored at start.
- Hover on rows and tiles is one rectangle per list (decided by a trace), with no fade, never over a selected
  row's colour or during a band or drag.
- A held press on a scrollbar's track repeats after 500 ms, every 50 ms, until the thumb reaches the pointer.
- The sizes view watches its root once its walk is done, re-reads only the folder a change is in, walks again
  on an overflow, and stops following after two overflows in a minute.
- The sizes view's Name column stops at its longest name on screen (at least 240 px), so the numbers sit
  beside the names at a wide pane.
- Two streams: the sidebar and look (M1, M3) and the sizes view (M2), split by files; the long soak at the
  phase's end with the coordinating session.

## Open questions

Each brief settles first: how much memory each folder's own-files totals add to a walk of `C:\` (P14-M2);
the hover's shape by the trace (P14-M3); where the bookmark code and the saved searches' share their reorder
(P14-M1).

## P14-M1: saved searches

Built by the sidebar and look stream on `feat/p14-m1-saved-searches`, 2026-10-29.

### Settled first: where bookmarks and saved searches share their reorder (2026-10-29)

The pure rules in `sidebar.rs` (remove, move up or down, move to a place, the drag's landing gap) never
touched anything a bookmark has, so they became generic over the list (`remove_at`, `move_by`, `move_to`;
`drag_target` already was) and both groups call them; only renaming stays per group, since the default
titles differ. The real coupling was in `app.slint`: the drag's gap and its insertion line assumed the
bookmarks were the sidebar's **last** rows. With Searches after them that no longer holds, so each group's
first row and count come from `sidebar::group` (`bookmark-first`/`-count`, `search-first`/`-count`), the
drag carries its group (`side-drag(kind, from, gap)`), and the gap and the line are measured inside it. The
drop target's Bookmarks gap uses the bookmarks' own first row and count. The coordinating session approved
it before the build; its addition: the empty space below the whole list still appends to the bookmarks
(P12-M5's rule) with Searches between, which the hit test's unit test and the scripted check both show.

### What landed

- **Saving**: Ctrl+D in a search tab (name or content, finished or still walking) saves the search;
  elsewhere it bookmarks as before. Also ⋯ More's "Save this search" (a search tab only; not a sizes view)
  and the Searches header's menu (enabled in a search tab). No tab menu: the kickoff's console-check line
  named "the tab's menu" by mistake and now reads "More and the Searches header's menu" (the coordinating
  session's correction). A search saved already stays one row, with the note "<title> is saved already".
- **The group**: Searches after Bookmarks, shown only with a row; Lucide `search` or `file-search` (added
  under `src/ui/icons/`); the default title `"<pattern>" in <folder>` or `Text "<text>" in <folder>`; the
  root as the tooltip. Rename (F2 or the row's menu; empty goes back to the default), Move up and down, drag,
  "Remove saved search".
- **Opening**: a click or Return runs it again in a new tab of the active pane (`start_search` with
  `SearchAt::Saved`, the same refusals plus "<root> is not there any more"); a middle click in the other
  pane, turning the split on first when it is off (the lead's call left the one-pane case open; this is the
  one that runs anywhere).
- **Drops**: a saved search's row and the Searches header refuse a drop; the space below the list appends
  to the bookmarks.
- **Settings**: `[[search]]` tables after `[[bookmark]]`; an older file has none (tested). Search tabs are
  still not restored at start.
- **A P13-M2 bug found here**: the toolbar's More and View menus never opened in a search tab or a sizes
  view. The background menu's refusal on results ("search results have no folder menu") caught them too,
  since P13-M2's toolbar menus are built through the same request. Toolbar menus now skip that refusal
  (they show no shell items), and More's separator index counts the new item.

### Scripted checks (2026-10-29, the branch's release build, `tools/check-p14m1.ps1`, a scratch settings file in `%TEMP%`)

Two runs, the window closed through `WM_CLOSE` so the settings are written, then read by the second run.
The first run showed the More bug (`[auto] waitmenu: no settled Windows 11 menu`, the note "search results
have no folder menu"); after the fix, every case held:

- Ctrl+D: `[sidebar] saved search added ""*.rs" in proj": ...\proj "*.rs" name`, then again `""*.rs" in
  proj" is saved already`.
- More: `[toolbar] menu more: Preview pane | Save this search | Select all | ...`, `picks "Save this
  search" -> savesearch`, `saved search added "Text "needle" in proj": ... "needle" content`.
- Rename, move, open: `renamed ""*.rs" in proj" -> "Rust files"`, `moved 1 -> 0`, `saved search 0 opened:
  ... "needle" content` and its `done: 1 found in 3 files read`; an empty rename back to the default title.
- The drop below the list with the Searches group showing and one bookmark: `[dnd] over sidebar bookmarks
  gap 1 ... -> effect copy`, `bookmark added ...\dropped at 1 (drop)`.
- After a restart: the three `[[search]]` tables read back (`content = true` on the text one only); the
  `gone` root deleted between the runs: `[search] pane=0 saved search "x": ...\gone is not there any more`;
  `saved search 2 removed`; the content search opened again and found its file.

Tests: 342 passed (four new: the default title and duplicate rule with the rename, the group's rows with and
without saved searches, the shared reorder, the settings round trip with an older file; the drop target's
test gained the Searches group's rows; More's test its new item).

### Soak

2026-10-29, `tools/soak-p5m3.ps1 -Minutes 10` on the branch's release build (code as 28c4e7b), the machine
handed over by the coordinating session: 10.1 minutes, closed by its script, **exit code 0, 0 panicked
lines**, three full cycles, 238 jobs done (0 failed, 0 cancelled), and every must-be-0 count 0 (dropped
drags, toolbar presses refused, menupicks refused, spring tabs, zip jobs not done, picture failures,
Office unreadable, alt text, waitpreview timeouts, worker panics, copy failures, chunks refused, wrong
passwords). The new cycle: **saved 3, opened 3, run again and found its file 3, removed 3** (one each per
cycle); content searches done 6 (two per cycle since: the saved one's run again), 1,208 files read, 6 found.
Memory: working set 148.9 MB at 5 s, 221.7 MB at 600 s; private 183.9 -> 248.1 MB (a ten-minute run, not
the phase-end hour; Phase 13's 60-minute soak ended at 247 MB private, flat).

### Gate

ab76, 2026-10-29, Jeremy at the console: 28c4e7b (`target-p14m1`, built `--locked` from the detached
`gate-p14m1` worktree by the coordinating session) against `target-p13close` (13cfe82), by
`tools/gate-p14m1.ps1`; summary `C:\traces\gate-ab76-summary.txt`. The scenario never saves or opens a search
and has no Searches group, so the triplet proves the rest untouched (the wrapper also fails a candidate run
that logs a saved-search line; none did). WPR main-thread busy, the defaults:

| Run | p13close | p14m1 |
| --- | --- | --- |
| 1, 2, 3 | 20.5, 20.7, 22.1 | 20.1, 21.2, 20.1 |
| Median | 20.7 | **20.1** |

**No gap**: the runs overlap and the candidate's median sits below the reference's. Every run at 60 Hz
(interval p50 16.29 to 16.41 ms), render p50 2.65 to 2.85 ms with no state change part-way, every check ok.

A lesson from it: the wrapper first failed at its `git` line because Jeremy's elevated console sat in the
deleted `gate-p13cf` folder; it ran clean once started from `C:\traces`. **Start the gate console in
`C:\traces`.**

### Console checks (for Jeremy, unelevated, after the gate)

Save from Ctrl+D, More and the Searches header's menu; open, rename, drag, remove; a saved content search; a
saved search whose root was deleted; the settings file read back after a restart.

### Calls made in this milestone, for Jeremy to check

- A middle click on a saved search with one pane turns the second pane on and runs it there.
- The "saved already" note names the row that holds it, its own title if renamed.

## P14-M2: the sizes view follows changes

Branch `feat/p14-m2-sizes-watch` from main at 29c80dd (the Phase 14 kickoff), the sizes stream.

### Settled first: what the own-files totals cost

Estimated from the P13-M3 `C:\` run (373,474 folders, 1,214,052 file IDs, +72 MB over the start) before
building, and accepted by the coordinating session (the bar is 150 MB):

- Each `Tree` node keeps its own files' totals (`Own`: size on disk and logical as u64, file names and
  links as u32) and a `skipped` flag: the node grows from 72 to 104 bytes (Rust's padding), +32 B x 373k =
  **+12 MB**.
- To read one folder again correctly a hard-linked file must be counted by the folder that met it, so the
  walk's file-ID set (`HashSet<u64>`) became an owner map (`HashMap<u64, u32>`, `dirinfo::HardLinks`): at
  2^21 buckets 19 -> 36 MB, **+17 MB**.
- A window's records (at most 10,000, `fs::watch::MAX_DEEP`) are held only until they are applied.

So about **+30 MB, near 100 MB over the start** for a walk of `C:\`. Measured in the in-app `C:\` run below: the `done:` line read **+109 MB since
the start** (P13-M3's walk of the same disk +72 MB: the own totals and the owner map cost about +37 MB, a
little over the estimate), under the 150 MB bar.

### Items

- **Watched** (done): a sizes view whose walk is done takes a subtree watch on its root (`Tab::follows`,
  `watch_tabs`: in front with its parent watch, in the background without, the cap of 8 counting it);
  This PC's view is not watched (no one folder). The subtree watch hands every record of a window
  (`Event::Subtree`'s `deep`, the path under the root), and the view's own `sizeview` thread, which stays
  after the walk, takes the windows in order (`sizeview::Cmd`); it ends when the tab drops its sender.
  TwinFM's own deletes and moves out still come off at once (`sizes_removed`, now also off the folder's own
  totals), so their window finds nothing changed; a move within the root, not followed before, is followed
  by the watch now.
- **What a change does** (done): `sizeview::plan` turns a window's records into the folders to read again
  (each record's folder, or the deepest folder of its path the tree has; a folder's own entry modified is
  no change; below a folder not entered, nothing), shallowest first; a rename pair in one folder renames the
  node, subtree and all. `follow` reads each folder once (`Listing::read`, one `dirinfo::read`, outside the
  tree's lock) and `Tree::apply` puts the difference of its own files' totals on every total above it, takes
  off the subfolders that went and adds the ones that came, which are walked (their subtree only). The rows
  are built again as the walk's are; an open level of a folder read again is listed again. Re-reads run on
  the `sizeview` thread only, never the UI thread.
- **Hard links** (done, the coordinating session's point): a hard-linked file counts in the folder that met
  it first; when that folder is gone (deleted, moved out), the next read that meets another name of the file
  counts it there (`HardLinks::counts`'s `gone`, tested). **Known undercount until F5**: the name that
  counted the file deleted while its folder stays (the file's other names are not counted until a new walk).
- **When it gives up** (done): an overflow, or a window past 10,000 records, walks the whole root again on
  the same thread, the old figures shown until it is done (the open levels kept by path); the second within
  a minute stops the following, and the status says "Not following changes: F5 walks again".
- **The columns' place** (done): the Name column stops at its longest name on screen (`fit_sizes_name`,
  measured as `colfit` measures, one call per depth, the indent and chevron included; at least 240 px;
  within the list's reported width less 320 px), at each rebuild of the rows and 150 ms after a scroll
  rests; Share, Size on disk and Files sit beside the names and the rest of the row is empty. The clamp is
  done in Rust from the list's reported width, so no Slint binding reads a width (a layout loop otherwise).
  For the console check.

### Calls made in this milestone, for Jeremy to check

- **A change under a folder the walk could not read reads that one folder again** (readable now, its
  subtree is walked; still unreadable, nothing), instead of walking the whole root as the plan had it:
  Windows writes under unreadable folders of `C:\` (System Volume Information and the like) all the time,
  so the plan's rule would walk all of `C:\` (74 s of disk) again and again. Only overflows and windows
  past 10,000 records walk the root again. Accepted by the coordinating session.
- Past 10,000 records in one window (anywhere below the root) counts as an overflow.
- The `window:` line ends with the root's figures (`; root S bytes on disk, F files, D folders`), which the
  soak compares with F5's walk.

### Tests

`cargo test --release`: 342 passed, 1 ignored (the measuring test) on the branch; 346 passed, 1 ignored with main (P14-M1) merged in (1af867b). New: `a_folder_read_again_changes_only_what_changed`
(a file grown, a folder created, removed, the same listing again, a hard link in two folders and re-owned,
a folder unreadable and readable again, a removal by `sizes_removed` then read again; every folder's totals
its own plus its subfolders'), `a_window_names_the_folders_to_read_again` (records to folders, a rename, a
move out, below a folder not entered, a skipped folder), `the_name_column_fits_its_widest_row`, and
`a_watched_tree_follows_its_changes` (a real subtree watch on `%TEMP%\twinfm-p14m2-test-<pid>\`: a file
grown, a folder made with files below it, one deleted, one renamed, one moved out, a root file appended;
the followed tree equals a fresh walk folder by folder); `a_file_counts_once_by_its_id` covers the owner
map.

### Soak

`tools/soak-p5m3.ps1 -Minutes 10` from the branch, two runs (the coordinating session's slots, after the
P14-M1 gate):

- **First run** (8612f82's code): exit 0, no panic, 16 cycles, 251 jobs (3 failed), every must-be-0 count 0;
  the P14-M2 cycle's watching 6 (two per cycle), windows 9, re-walks 0, stopped 0, but **the last window
  against F5's walk: 0 matches, 3 mismatches**, each 2 files over (202 against 200). The stderr showed the
  following right (the two `touch`es' window 200 -> 202 files, the rename's 202): the two `rmfile`s' window
  had not been delivered when F5 walked, as the watch had backed off to a 1 s window, the script slept 700
  ms, and **`waitsizeview` does not wait for a window the watch has not delivered yet** (now in its
  CLAUDE.md line). The soak's waits were made 2.3 s, past the watch's longest window (128989c); no code
  changed. Private 180 -> 238 MB.
- **Rerun** (128989c): exit 0, no panic, 16 cycles, 245 jobs (0 failed, 0 cancelled), every must-be-0 count
  0; watching 6, windows 12, **the last window against F5's walk: 3 matches (one per cycle), 0
  mismatches**, re-walks 0, stopped 0. Working set 149.5 -> 212.8 MB (at most 240.2), private 164.5 -> 217.6
  MB.

### The in-app `C:\` run

2026-10-29, the coordinating session's slot after the soak: the branch's own `target\release` (8612f82's
code), a script of `sizeview 0 C:\`, `waitsizeview`, then 10 minutes left open, no settings file, the
process's memory sampled every 30 s from outside. **97 s** for 349 GB on disk (353 GB logical) in 1,293,565
files and 334,566 folders, 200 folders unreadable; `done:` working set 214 MB, **+109 MB since the start**,
private 213 MB, 334,514 folders and 1,232,279 file IDs kept. Over the 10 minutes it followed `C:\`: **319
windows** (about 32 a minute), 6,899 records, **907 folders read again** (about 91 a minute), 31 created, 7
removed, the slowest window 29.8 ms on the `sizeview` thread; **no re-walk, no window past the 10,000-record
cap, no stop**, no panic. Private bytes 153 MB at 30 s, at most 213, **209 MB at the end** (working set
195).

### Gate

`tools/gate-p14m2.ps1`, tag ab77: the defaults triplet against `target-p14m1` (P14-M1 merged; the scenario never opens the sizes
view and never asks for a subtree watch; every run's stderr must hold no `[sizeview]` line). Handed to Jeremy by the coordinating session; the result is recorded here when it is in.
