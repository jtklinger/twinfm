# Phase 13: the mouse, the name's room, and the sizes view

Written 2026-10-27, the day Phase 12 closed and 0.3.0 was released (`phase-12-plan.md` § Phase 12 close).
Phase 13's goal, from Jeremy's kickoff on 2026-10-27: **make everything TwinFM does reachable with the
mouse, give the file name the room it needs in the details view, and add a TreeSize-style view for
freeing space; then the checks still owed.** The items this phase takes leave `docs/roadmap.md` when
this plan merges (Jeremy's rule, 2026-10-26).

What Jeremy decided at the kickoff (2026-10-27), his words:

- **His two complaints**, with a screenshot of a narrow left pane on `G:\My Drive\A_ScanDocs` whose long
  names were cut by the Type column: "1. In details view, the file name gets cut off by the next column,
  I'd rather have the columns not displayed ... 2. I'd actually like a way to show/hide the 2nd pane, but
  that might be because of the file name cutoff."
- **When columns go**: "Only when the pane is too narrow, with other columns coming back as space allows."
- **The mouse**: "my main complaint is that many of the feature/functions were written with keyboard
  control in mind when in fact my workflow typically uses the mouse for most of my work. Making sure that
  we have mouse capabilities for all keyboard actions would be appropriate."
- **Where the new buttons go**, from three mockups: **B**. Each pane gets Back, Forward, Up and Refresh
  beside its path bar; the toolbar adds Undo, Redo, View ▾ and Two panes beside Preview, and a "⋯ More"
  menu holds the rest.
- **The rest of the phase**: "let's tackle treesize and checks still owed."
- **What TreeSize is for**: "My main reason for using treesize is to find where the most capacity is
  being consumed on disk; typically when I'm trying to free up space."
- **The sizes view's shape**, from two mockups (a sorted list opened level by level with share bars, or a
  map of boxes): **A**, the list.
- **Saving a search tab**: "roadmap" (it stays there).
- **Where a selecting drag starts in the details view**, the builder check's question: "Like Explorer:
  dragging from a row's blank part (past the name, or on Size/Modified/Type) selects; dragging the name or
  icon moves files."
- **The dead share check's share**: Jeremy asked for an SMB share on the lab's Ceph cluster, set up from the
  Homelab repo by a session of its own (2026-10-27), with a switch for each way of going dead.

The starting point is main at 8b00bb3 (the Phase 12 close; code as `target-p12seg`, 3698cd2). What each
item rests on today, from reading the code:

- **Columns** (`src/cols.rs`, P9-M1): the details list's Type, Size and Modified widths are per pane
  (defaults 120, 90, 140, `FLOOR` 40), the name cell takes what is left and is clamped at `NAME_FLOOR`
  (120 px) only when a grip is dragged; a pane narrower than its columns plus 120 px gives the name what
  remains, down to zero (`dnd::name_cell`). The only way to hide a column is the setting `show_type`, for
  Type alone, both panes, chosen once per pane among the list components (`FileListHost`), never per row.
- **The second pane**: F3 (`toggle-split`, app.slint:829) shows and hides it; the setting `split` is saved
  but is not in the settings dialog; nothing on screen offers it.
- **Buttons**: the toolbar (`Toolbar`, pane.slint:148) has New file, New folder, Cut, Copy, Paste, Rename,
  Delete and Preview pane, each `ToolButton` with its key in its tooltip, placed across the window
  (`toolbar = "top"`, the default), in each pane (`"pane"`, icons only) or nowhere. The path bar (36 px,
  crumbs or the text entry, then the search box at its right end) has no buttons. Back and Forward are the
  mouse's side buttons and Alt+Left/Right; Up is Backspace or Alt+Up; Refresh is F5 or Ctrl+R; Undo and
  Redo are Ctrl+Z and Ctrl+Y; the views are Ctrl+Shift+1/2/3; hidden files Ctrl+H; select all Ctrl+A;
  copy and move to the other pane Shift+F5 and Shift+F6; folder sizes Alt+Shift+Enter; Escape cancels
  waiting jobs. None of these has a button or a menu item (the Windows 11 menu has Cut, Copy, Paste,
  Rename, Delete, New, Open, Properties and the shell's own).
- **Selection by dragging**: the rubber band (`src/band.rs`, P4-M3) works in the two grids only; in the
  details list a press on a row starts an OLE drag past the threshold, and a press below the last row does
  nothing but clear the selection.
- **Folder sizes** (`src/fs/sizes.rs`, P3-M3): one `sizes` worker walks a folder with the listing loop
  (`enumerate::list_dir`), adding every file's **logical size** (`nFileSizeHigh/Low` from
  `FindFirstFileExW`), hidden and system files too, never entering a reparse point (counted at 0 bytes),
  at most 64 levels deep, partial totals every 250 ms, newest request first, a cancel flag per request;
  `TabSizes` keeps the per-tab cache and the re-walk after a change; a subtree watch keeps sizes current.
  Nothing reads a file's size on disk (its allocated clusters): a compressed, sparse or cloud-only file
  counts at its full logical size today.

## The milestones

Four milestones: two streams and a coordinating session, as Phases 11 and 12 ran (the lead's proposal).
Within a stream the order is the order of work.

**The mouse stream** (one session; touches `src/ui/pane.slint`, `src/ui/app.slint`, `src/ui/theme.slint`,
`src/toolbar.rs`, `src/cols.rs`, `src/band.rs`, `src/dnd.rs`, `src/auto.rs`, the command paths in
`src/main.rs`, `tools/soak-p5m3.ps1`):

1. **P13-M1, the name gets its room, and the second pane by mouse.** **Columns that make way**: in the
   details list, when the pane is too narrow to give the name cell `NAME_FLOOR` (120 px) beside the
   columns at their set widths, columns are hidden, one at a time, until it can; as the pane widens they
   come back in the reverse order, each when it fits at its set width with the name keeping 120 px. The
   order they go (the lead's call): **Type first, then Size, then Modified**; Name never goes. The set
   widths are not changed by hiding (a column comes back at the width the user gave it), and `show_type`
   off still hides Type at any width. Settled first: how a hidden column reaches the rows without a
   per-row cost. Today each pane picks its row component once; the candidates are (a) more components
   (Name+Size+Modified, Name+Modified, Name only, each with and without thumbnails and in two row heights:
   too many to copy by hand), (b) one component per thumbnail and row height whose cells are bound to a
   width the host sets to 0 when hidden (P9-M1's spike measured a width read when a row is made as free,
   and this is the same kind of binding), (c) the hidden cells drawn but clipped. The lead's recommendation
   is (b), decided by a trace of the gate scenario, which splits the window so its panes are about 590 px
   wide and **may hide columns itself**: the trace records which columns the scenario's panes show before
   and after. The header, the grips (a hidden column has none; `colfit` and `coldrag` on it log `no such
   grip`), the rename editor, `dnd::name_cell`, `window_layout` and the driver follow the shown columns.
   Pure: `cols::shown(widths, show_type, pane_width) -> [bool; 3]`, tested at each boundary both ways. Line
   `[cols] pane=N shows <name, type, size, modified> at W px` when the set changes. **The second pane by
   mouse**: a **Two panes** button on the toolbar, drawn as on while both panes show, calling F3's
   `toggle-split` (tooltip "Two panes (F3)"); in the in-pane toolbar it sits on each pane's bar. Driver
   `toolbar <pane> split`; `[toolbar] pane=N split -> toggle`. Tests: `shown`, the toolbar state. Soak: per
   cycle the window's split toggled by the button and the left pane narrowed by the splitter until Type and
   Size go, then widened back (`[cols] ... shows` lines counted both ways). Gate: the defaults triplet (with
   what the scenario's panes show, before and after, in the record) and the cols triplet
   (`C:\traces\cols.toml`). Console checks: Jeremy's own `G:\My Drive\A_ScanDocs` at the width of his
   screenshot; the splitter dragged slowly both ways; Two panes from the toolbar; both toolbar placements.
2. **P13-M2, everything by mouse.** Layout B (Jeremy's pick), as the mockup drew it:
   - **Each pane's path bar** starts with four icon buttons, Back, Forward, Up and Refresh (Lucide
     `arrow-left`, `arrow-right`, `arrow-up`, `rotate-cw`), the crumbs and search box after them; the bar's
     height (36 px) and so the list's top do not move. Each acts on its own pane and makes it active.
     Back and Forward are disabled with nothing to go to, Up at This PC, Refresh on nothing it cannot
     refresh (never disabled in a folder, an archive level or a search tab). A right click on Back or
     Forward opens the pane's history menu (Alt+Down's, P11-M3) (the lead's addition). Tooltips name the
     keys ("Back (Alt+Left)", "Up (Alt+Up)", "Refresh (F5)"). The crumb dropdown and crumb geometry
     (`CRUMB_GEOM`) shift right by the buttons' width; their reported places are what the driver aims at,
     so `crumb`, `crumbmenu` and `dropfiles crumb` follow on their own.
   - **The toolbar** gains, after Delete: **Undo** and **Redo** (Lucide `undo-2`, `redo-2`; disabled when
     their stack is empty or a job runs, tooltips "Undo (Ctrl+Z)" with the record's status text after it),
     a separator, **View ▾** (a menu: Details, Large icons, Medium icons, the current one ticked; Ctrl+Shift+1/2/3's
     command, for the active pane, or that pane's in the in-pane toolbar), **Two panes** (P13-M1's), Preview
     as today, and **⋯ More**, a menu of: Select all (Ctrl+A), Copy to other pane (Shift+F5), Move to other
     pane (Shift+F6), Size all folders (Alt+Shift+Enter), Show hidden files (Ctrl+H, ticked while on), Show
     sizes (P13-M3's view, once it lands), Cancel waiting jobs (Escape's, disabled with none waiting). Each
     item calls the command its key calls, with the key at the item's right as the Windows 11 menu does.
     The menus are TwinFM's own (the Windows 11 menu's popup and rows, P7-M2), so they take the look and
     the keys. Disabled items show as in that menu. In the in-pane toolbar (icons only), the same buttons,
     acting on that pane. Where a bar is too narrow for every button, the buttons past its edge go into
     ⋯ More, last first; **each bar overflows on its own**, the bar across the window at a narrow window
     and each in-pane bar at a narrow pane (about 13 icons, some 400 px, so a narrow pane overflows
     often); ⋯ More itself never overflows (the lead's calls; settled with the trace that the bars' layout
     costs nothing per frame). With `toolbar = "none"` the path bar's four buttons stay and nothing else
     is shown: the setting is the choice of no toolbar (the lead's call).
   - **Click-and-drag selection in the details list** (on the roadmap since P10-M1, Jeremy's: "I cannot
     click-and-drag the mouse cursor to highlight multiple items"; the lead's assumption that it belongs to
     "mouse capabilities for all keyboard actions", which Jeremy may reverse): `band.rs` extended to the
     details list, **as Explorer's details view does** (Jeremy, 2026-10-27: "Like Explorer: dragging from
     a row's blank part (past the name, or on Size/Modified/Type) selects; dragging the name or icon moves
     files."). A press on a row's icon or on the name's text (its drawn width, measured as `colfit`
     measures, plus a few px) and dragged past the threshold drags the files as today; a press anywhere
     else in a row (the name cell past the text, the Type, Size and Modified cells, the padding at the
     right) or below the last row starts the band, and a press there released under the threshold
     selects that row as a click does today (the band's grid rule clears the selection only from space
     below the rows). Settled first: how `dnd::hit` learns each row's text width without a per-row cost
     (the rows on screen only, asked when the press lands). With the band: Ctrl toggles, Shift adds, the
     edge auto-scroll by rows, Escape cancels, `[band]` lines as the grid's. Driver `band` works in a
     details pane; `[band] pane=N press on row R <icon|name|blank>: drag|band` per press.
   - **What stays keyboard-only, recorded**: Shift+Delete is Delete with Shift held (the toolbar's Delete
     and the menu's already take Shift); the path entry (Ctrl+L) is a click on the path bar's background
     already; Tab between panes is a click on the other pane. The rail's footer keeps only the chevron
     (Settings and About on the full sidebar), unchanged.
   Driver: `toolbar <pane> <undo|redo|view|split|more>`, `toolmenu <text>` (picks an item in the open View
   or More menu by its text), `navbutton <pane> <back|forward|up|refresh> [right]`. Lines: `[toolbar]
   pane=N <button> -> <command>`, `[toolbar] menu <view|more>: <items, (off) disabled, (on) ticked>`,
   `[toolbar] pane=N picks "<item>" -> <command>`, `[nav] pane=N <Back|Forward|Up|Refresh> button`.
   Tests: which buttons and items are enabled (`toolbar::state` extended), the overflow order, the band's
   start rule in the details list. Soak: per cycle each path-bar button once, Undo and Redo by the toolbar
   on the cycle's recycle, a View menu round trip, each More item that does not need a person (Select all,
   Copy and Move to other pane, Size all folders, Show hidden files twice), and a band in the details list
   that selects two files. Gate: the defaults triplet and the toolbar-pane triplet
   (`C:\traces\toolbar-pane.toml`). Console checks: every new button and item, light and dark, beside
   Explorer; a band from below the last row, with Ctrl and Shift; a file drag still drags; the bar at the
   window's minimum width.

**The sizes stream** (one session; touches `src/fs/sizes.rs`, `src/fs/enumerate.rs`'s size field, a new
`src/sizeview.rs`, its Slint component, the tab kinds in `src/tabs.rs` and `src/model.rs`, the list paths in
`src/main.rs`, `src/winmenu.rs`'s items, `src/settings.rs`, `src/auto.rs`; P13-M2's More menu takes its
item when both have merged):

3. **P13-M3, the sizes view, and size on disk.** For freeing space (Jeremy: "find where the most
   capacity is being consumed on disk"). Option A, as the mockup drew it:
   - **Where**: a tab of its own kind in the pane, titled `Sizes: <folder>`, opened by "Show sizes" in the
     Windows 11 menu on a folder or drive (each selected one, a tab each) and on a list's background (the
     folder shown), and by ⋯ More's item. Opened on This PC it sizes every fixed drive, one row each,
     **except Google Drive's volume**. Refused inside an archive and on Google Drive's volume (its
     allocated sizes say nothing about space here, and a walk there may list remotely; content search
     refuses it for the same reason); a OneDrive folder is walked, its cloud-only files at 0 (the lead's
     calls). It restores like other tabs (the root saved, walked again at start; the lead's call, so
     nothing stale is shown).
   - **What the walk keeps** (the lead's call): a new tree walker beside `sizes::measure`, on the `sizes`
     worker, keeping **every folder** with its totals (on-disk bytes, files, folders, skipped) and no file
     entries; a level's files are listed again when the level is opened (one folder's listing), each with
     its size on disk. A ▸ on a folder therefore opens at once with its subfolders' totals, and its files
     appear as that one listing lands. The bar for a walk of `C:\` on this machine: under 150 MB more
     working set than before the walk, recorded with its time in the in-app run (the lead's limit; a walk
     over it is reported to Jeremy before the milestone merges).
   - **What it shows**: the root's children, **biggest first**, each with a share bar (its size against
     the root's), its size on disk, its file count, and for a folder a ▸ that opens it in place one level
     down (▾ to close), each level sorted biggest first within its parent. A status line: `Walking… F
     files, S so far` while the walk runs, then `S on disk in F files, D folders, in T s` and `K folders
     could not be read` when any were skipped. The numbers fill in live as the walk runs (partial totals as
     today's, every 250 ms) and rows move to keep the order, the cursor row kept on its item.
   - **Size on disk**: the allocated size of each file, not its length. Settled first: how to read it
     without a call per file. Candidates: `NtQueryDirectoryFile` / `GetFileInformationByHandleEx` with
     `FileFullDirectoryInfo` (each entry carries its `AllocationSize`, one call per folder as today's
     listing), against `GetCompressedFileSizeW` per file (a call per file). The lead's recommendation is
     the directory information class, measured against today's walk of `C:\traces\bigdir` and of `C:\` in
     the sizes worker. A cloud-only file counts 0 (it takes no space here, which is the answer for freeing
     space; the lead's call); hard links count once per walk where the walk can tell (settled first:
     the file ID costs), else each name counts. Reparse points stay counted at 0 and not entered.
   - **Acting on it**: rows are entries as in any list, so the Windows 11 menu (Open, Open in other pane,
     Delete, Properties, Copy path and the shell's), Delete and Shift+Delete, drag out, and Return or a
     double-click on a file work; Return or a double-click on a folder opens it in a normal tab (a ▸ click
     opens it in place). After a delete or a move out finishes, the item's row goes and its size is taken
     off every total above it at once, with no new walk (the lead's call); a change from elsewhere under
     the root is followed by the subtree watch as folder sizes are today, re-walking the changed folder.
   - **Also in the normal list**: a **Size on disk** column in the details list, off by default, a
     setting `size_on_disk` in the dialog (the lead's call: a fourth column, last, the first to drop in
     P13-M1's order). **The listing itself is not changed** (the lead's call: `FindFirstFileExW` stays the
     gate's hot path, and the sync-root placeholder threads, `stat_entry` and the watch's edits stay as they
     are); only while the column is on, a second read of the folder (one directory-information call per
     folder, after the listing lands, on the listing's thread) fills the column, and a watch edit's entry
     gets its value by the same read of its folder. Settled first with P13-M1's stream: a column of its own
     only if P13-M1's binding route holds (else it waits). Folder sizes (Alt+Shift+Enter) keep showing
     logical size.
   - Driver: `sizeview <pane> [<path>]` (opens the view on the cursor row's folder, or the path),
     `sizeopen <row>` (the ▸ of a row), `waitsizeview <ms>` (until the walk is done, then 300 ms). Lines:
     `[sizeview] pane=N gen=G start <root>`, `partial: F files, S bytes on disk`, `done: S bytes on disk
     (L logical), F files, D folders, K skipped, in T ms`, `cancelled (<why>)`, `pane=N opened <folder> in
     place (K children)`, `pane=N removed <path>: S bytes off K totals`. Tests: the ordering and in-place
     opening (pure, on a tree), the share bars' arithmetic, the total update after a delete, the size on
     disk of a compressed and a sparse file in a scratch folder (`%TEMP%\twinfm-p13m3-test-<pid>\`).
     Soak: per cycle a sizes view of `src`, a level opened, a file under it deleted from the view and the
     totals checked by the line, the tab closed. Gate: the defaults triplet (the scenario never opens the
     view: it proves only that the rest is untouched, and the record says so), plus an in-app run of the
     view on `C:\` recorded for its time and memory. Console checks: `C:\` from This PC beside TreeSize or
     Explorer's own numbers (Properties of a folder: "Size on disk"), a OneDrive folder (cloud-only files
     at 0), a delete from the view, the column in the normal list.

**The coordinating session** (this one) serialises the machine, hands each gate to Jeremy, merges on his
word, keeps `docs/roadmap.md`, and does P13-M4:

4. **P13-M4, the checks still owed, and the small clean-ups.**
   - **The dead network share check**, owed since P5-M1, on the Ceph-backed SMB share the Homelab session
     sets up: a share name that does not resolve (`no watch`, the pane's message), then one that resolves
     and does not answer (the SMB service stopped or port 445 dropped while the name resolves: `stalled:
     no watch`, the 30 s cool-down, F5), each with the share watched in a pane before it goes dead, and
     back again after. Waits for that session's share and its two switches.
   - **The console checks owed from Phase 12**, with Jeremy at the machine once on a build with every
     Phase 13 milestone: the password questions' Tab order (P12-M3); Tab between the panes with the
     active pane's accent cursor, and the 44 px comfortable thumbnail rows (ab64); whether the shell draws
     the "Bookmark" drop text (P12-M5).
   - **A long soak** of the merged code on the segment heap (60 minutes of `soak-p5m3.ps1`), at the
     phase's end, so it covers Phase 13's code too.
   - **Memory**: the preview's styled text for a big file built per chunk (fewer large temporary
     allocations, the Phase 12 memory check's finding); its own branch and a preview triplet.
   - **Clean-ups**: the `[menu] "<label>" found after T ms (a submenu filled late)` line only for commands
     in a late submenu (P12-M6); the soak report's "tabs they opened" counting what its label says (P12-M5).
   - Not done, recorded: OneDrive's error mark stays drawn only by its test unless a file reaches the
     error state in daily use; the Recycle Bin enumeration's shell cache (small) stays on the roadmap.

Where the streams meet: P13-M1's column rule and P13-M3's Size on disk column touch the same row
components; P13-M2's More menu takes P13-M3's item. The second to merge takes main in first, and the
coordinating session orders those merges (the lead's expectation: M1, M3, M2).

Not in Phase 13, on the roadmap: saving a search tab (Jeremy: "roadmap"), the Slint fork's whole-row snap
and upstreaming its two accessibility patches, accessibility.

Each milestone brings its driver steps and stderr lines (in `CLAUDE.md` § Measuring), tests for its pure
parts, and its soak cycle in `tools/soak-p5m3.ps1`. Every item in a bundle is a commit of its own, and the
record says per item: done, checked and found right, or left with the reason. Console checks after each
triplet, unelevated.

**The gate is re-run after every milestone** under the protocol of 2026-10-12, as Phase 12 ran it:
`tools/wpr-triplet.ps1` elevated at the console by Jeremy, accessibility on, Narrator off, every other
TwinFM window closed, the split-and-scroll scenario on `C:\traces\bigdir`, clean binaries built with
`--locked` into their own `--target-dir target-<name>` from detached `git worktree add` checkouts. Three
runs per binary, interleaved, compared by median; a gap counts only when every reference run sits below
every candidate run or the medians differ by more than one point; each run's `[perf] ... render[...
p50=...]` read first; a fixed-order gap the code cannot explain gets the order-swapped rerun first. The
reference for the first Phase 13 gate is **`target-p12seg`** (3698cd2); after each merge the new
milestone's gate build becomes the next reference. **Tags continue from ab66.** One wrapper per milestone
(`gate-<milestone>.ps1`). The Phase 9, 11 and 12 rules for two streams hold (every stream reports to the
coordinating session before any cargo run, gate build, soak, TwinFM window or hand-over and waits for its
reply; one soak at a time; tests run and read, then the commit, as two steps; CI waited for before a merge
unless Jeremy says otherwise; a menu's check goes through the menu, not its callback). The architecture
rules from Phase 1 and P12-M3 hold, and nothing per frame or per row without a trace first.

## Calls made in this kickoff, for Jeremy to check

Decisions Jeremy did not make and the lead settled, each reversible before its milestone starts:

- Columns drop in the order Type, Size, Modified, and come back in reverse, each at the width it had; the
  name keeps at least 120 px before any goes.
- Two panes is a toolbar button, drawn as on while split; F3 stays.
- A right click on Back or Forward opens the history menu.
- The toolbar's buttons that do not fit at a narrow window go into ⋯ More.
- Click-and-drag selection in the details list is part of the mouse work (where it starts is Jeremy's:
  as Explorer, a row's blank part or below the rows; the name or icon drags files).
- The sizes view counts size on disk; a cloud-only file counts 0; it opens as a tab titled `Sizes:
  <folder>`, from the Windows 11 menu's "Show sizes" and from ⋯ More; on This PC it sizes every fixed
  drive; it walks again when restored at start.
- A delete from the sizes view takes the item off the totals at once, with no new walk.
- Size on disk becomes an optional column in the normal list, off by default, read by a second pass only
  while it is on; the listing that the gate measures is not changed.
- The sizes view keeps every folder's totals and lists a level's files when it is opened; a walk of `C:\`
  holds under 150 MB more. Google Drive's volume is not walked; OneDrive is, its cloud-only files at 0.
- Each toolbar (across the window, or in each pane) overflows into ⋯ More on its own; with no toolbar,
  only the path bar's four buttons show.
- From the builder check, settled here: the four calls just above.
- Two streams: the mouse (M1, M2) and the sizes view (M3), split by files; the owed checks and clean-ups
  with the coordinating session; the long soak at the phase's end.

## Open questions

Each brief settles first: how hidden columns reach the rows (P13-M1), the bar's overflow (P13-M2), how to
read size on disk without a call per file and how hard links count (P13-M3), and the share's switches
(P13-M4, from the Homelab session).
