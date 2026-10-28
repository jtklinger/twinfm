# Phase 13: the mouse, the name's room, and the sizes view

Written 2026-10-27, the day Phase 12 closed and 0.3.0 was released (#126) (`phase-12-plan.md` § Phase 12 close).
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
   - **A long soak** of Phase 13's merged code (60 minutes of `soak-p5m3.ps1`), at the phase's end. The
     one owed from Phase 12 was run after its close on `target-p12seg` (`phase-12-plan.md` § The long
     soak of the merged code: 60.6 minutes, no panics, about 259 MB private, flat), so this one compares
     against it.
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

## P13-M1: the name gets its room, and the second pane by mouse

The mouse stream (one session, worktree `p13-mouse-c54e0b`), from 2026-10-27.

### Settled first: how a hidden column reaches the rows (2026-10-27)

Route (b), the lead's recommendation, agreed with the coordinating session before any code ran. **No row
component changes and no row gains a binding.** The details rows already read the pane's three widths for
their Type, Size and Modified cells (P9-M1). The pane now hands them a width of 0 for a hidden column
(pane.slint's `eff-type`, `eff-size` and `eff-modified`), and Slint draws nothing for a Text 0 px wide:
`sharedparley::draw_text` returns before laying out any glyph when the item's width is 0 or less
(`internal/core/textlayout/sharedparley.rs:144` in the fork at c1d3a8d, the same path for femtovg and the
software renderer). The rows' cells sit in a HorizontalLayout with no spacing, so a 0-wide cell takes no
room and Name stretches into it. The rows re-read their widths only when the shown set changes: a resize,
never a frame or a scroll. Options (a) (more components) and (c) (cells drawn and clipped) were not needed.
The gate triplet is the trace that settles it (below).

For the sizes stream (P13-M3), agreed through the coordinating session: its Size on disk column would be a
fourth width each row reads, 0 while off or hidden, dropped first (Size on disk, Type, Size, Modified).
That is one more width binding per row, so it needs its own trace. `cols::shown` and `Widths` are shaped
for it (`ALL`, `taken`), and P13-M3 builds the column itself.

### What landed

- **Columns that make way** (commit 1): `cols::shown(widths, show_type, room)` is the rule, pure and
  tested at each boundary both ways. `room` is the list's width less the rows' paddings (`ListGeom::name_x`
  and `ROW_PAD_RIGHT`). Columns go Type first, then Size, then Modified, until Name keeps 120 px; they come
  back in the reverse order, each when it fits at its set width, which hiding never changes. Each pane
  reports its list's width (the host's `init` and `changed width`, `list-width`), and `update_shown` sets
  the window's `left-shows-*` and `right-shows-*` on change, with the line `[cols] pane=N shows <columns> at
  W px`. The hidden right pane is skipped (it is laid out at the list's 50 px minimum). The header is one
  layout with a conditional cell per column (it was two, by `show_type`). The grips follow the shown
  columns: `cols::grips(shown)`, `grip_x` and `left_of`, so Modified's grip beside a hidden Size is Name |
  Modified and its drag moves against Name. The rename editor follows, and so does `dnd::name_cell` through
  `Widths::taken`. The driver's `colwidth`, `coldrag` and `colfit` on a hidden column log `no such grip`. A
  new driver step, `splitter <ratio>`, is a real drag of the split line through Slint's pointer path, for
  the soak.
- **Two panes** (commit 2): a toolbar button (Lucide `columns-2`, before Preview pane, tooltip "Two panes
  (F3)", drawn as on while both panes show; on each pane's bar in the in-pane toolbar), calling F3's
  `toggle-split`. `toolbar <pane> split`, and `[toolbar] pane=N split -> toggle`.

### Scripted checks (2026-10-27, the branch's release build, `SLINT_BACKEND=winit-software`, no settings file, 1200 px window at 100%)

- At start the panes report a first-layout width (`shows name at 210 px`), then the real one (`name, type,
  size, modified at 1010 px`).
- **Split at 0.5, each list 499 px: Type is hidden in both panes** (`shows name, size, modified at 499
  px`), since 499 less 38 px of padding is 461 px of room, under the 470 that Type, Size, Modified and
  Name's 120 need. So the default window split in two already drops Type. The gate's own window is larger
  (the plan's estimate: panes about 590 px), so the triplet's stderr records what its panes show.
- Splitter to 0.30: the left list at 382 px shows name and Modified, at 296 px name only; the right list
  at 515 px shows all four. Back to 0.50: name and Modified at 304 px, then name, Size and Modified at
  390 px. Each line lands once, at its boundary.
- Splitter to 0.35 (the soak's): name and Modified at 385 px; back at 0.50, Size returns at 392 px.
- `colfit 0 type`, `colfit 0 size` and `coldrag 0 modified -30` with only Name shown: `no such grip` each.
- Two panes off: the left list at 1010 px shows all four. On again: back to 499 px and name, Size and
  Modified. The right pane logs nothing while hidden.
- Screenshots (both panes at 499 px; the left pane at 0.30 with Name alone beside the right's four):
  hidden cells draw nothing, the header shows only the shown columns, and the grips sit on their edges.

### Soak

Per cycle in `tools/soak-p5m3.ps1`:

- Two panes off and on by the toolbar (`toolbar 0 split` twice).
- The split line to 0.35 and back to 0.5 (`splitter`).
- Reported: Two panes presses (two per cycle), the left pane narrowed to the name and Modified (three per
  cycle), and name, Size and Modified (four per cycle at 100%). Every width change of the left list makes
  a line: the preview cycle's two, the split and the splitter.
- The P9-M1 cycle runs with the split line at 0.6, where all four columns show, so its drag moves the Type
  | Size boundary as it was written to.

**First run** (2026-10-27, 534a711, the worktree's own build):
- 10.1 minutes, exit 0, no panics. 10 cycles started, 3 complete.
- 256 jobs, none failed. Every must-be-0 count was 0.
- Memory: working set 167 to 239 MB, private bytes 193 to 273 MB.
- Two panes presses: 6.
- **It found a bug.** At 499 px the P9-M1 cycle's Size grip drag shrank Size, so Type fitted again and came
  back mid-drag. The grip's neighbour then switched from Name to Type under the pointer: Type flickered
  in and out, and the drag ended at Type 120 -> 150 instead of Name growing.
- **Fixed in commit 4:** the shown set waits while a grip is held and is applied on the release. The new
  test `a_drag_keeps_its_neighbour_while_a_column_could_come_back` pins the case.
- **The soak changed too:** the P9-M1 cycle moved to the 0.6 split, and the `[cols] shows` counts were
  relabelled by what makes each line.

**Second run** (2026-10-27, 936336a):
- 10.1 minutes, exit 0, no panics. 256 jobs, none failed. Every must-be-0 count was 0.
- Memory: working set 143 to 213 MB, private bytes 178 to 248 MB.
- The fix held. At the 0.6 split the Size grip drag read Type 120 -> 150 and Size 90 -> 60 (P9-M1's
  boundary, as written), and the widths came back to 120, 90, 140.
- Every `[cols] pane=0 shows` line came from the preview column, the splitter, the split or the start;
  none from the drag.
- "Widths back at 120, 90, 140" still read 4, not 7, for the same interaction in another cycle. P11-M2's
  reset cycle narrows Type with `colwidth 0 type 100` first. At 499 px Type is hidden, so the step was
  refused (`no such grip`, three times) and the reset had nothing to undo.
- **Soak changed again** (commit 5): that cycle runs at the 0.6 split too. The name, Size and Modified
  count is now five per cycle.

**Third run** (2026-10-27, 936336a's build with 6665c2c's script):
- It ran beside the `target-p13m1` gate build, on the coordinating session's word: a soak checks
  correctness, not speed.
- 10.1 minutes, exit 0, no panics. 10 cycles started, 3 complete.
- 237 jobs, none failed. Every must-be-0 count was 0.
- Memory: working set 139 to 206 MB, private bytes 177 to 245 MB.
- Column widths: drags read back as Size 90 -> 60, 3. Fits, 3. **Widths back at 120, 90, 140, 7** (two
  per cycle and one at start). Resets through the dialog, 6.
- Columns making way: Two panes presses 6. Narrowed to the name and Modified 9 (three per cycle). Name,
  Size and Modified 16 (five per cycle, and the start's split).

### Gate

ab66, 2026-10-27, Jeremy at the console at 60 Hz: 936336a (`target-p13m1`, built `--locked` from the
detached `gate-p13m1` worktree) against `target-p12seg`, by `tools/gate-p13m1.ps1`, 17 minutes; summary
`C:\traces\gate-ab66-summary.txt`. WPR main-thread busy:

| Run | Defaults | With `cols.toml` |
| --- | --- | --- |
| p12seg 1, 2, 3 | 21.4, 21.4, 21.1 | 21.0, 20.4, 20.5 |
| p13m1 1, 2, 3 | 19.9, 19.6, 19.4 | 19.4, 19.0, 19.8 |
| Median | 21.4 against **19.6** | 20.5 against **19.4** |

Every check ok, every candidate run under 20%, no gap (the candidate is the cheaper one). Render p50
2.45 to 2.56 ms against 2.72 to 2.95 ms, so no run sat in the costlier state. Every candidate run ended
with both panes showing name, Size and Modified at 499 px: at the gate's 1200 px window with the split,
each list is 499 px and Type is already hidden, so the candidate draws no Type cells and its lower
number is that narrower draw, not a saving at equal columns. The full-width comparison below checks the
route with every column shown.

**The full-width comparison** (2026-10-27, after the gate, unelevated, in-app figures only, so it decides
nothing on its own): the gate's split-and-scroll scenario on `C:\traces\bigdir` with the window
maximised first, so the split panes are 1010 px wide and both show name, Type, Size and Modified (the
candidate's `[cols]` lines, and `widths ... (name 622 px)` for both panes). Six runs, the order swapped
between pairs: p12seg, p13m1, p13m1, p12seg, p12seg, p13m1. The UI thread's busy share over the 10 s
after the split read p12seg 21.9, 15.9, 16.7 and p13m1 15.2, 16.1, 17.5 (medians 16.7 and 16.1; the
first run of the session reads high whichever binary sits in it). The render p50 of the scrolling
seconds read 4.83 to 4.84 ms for p12seg and 4.82 to 4.88 ms for p13m1. With every column shown the
route costs nothing measurable per frame, which is what the trace-first rule asked of it: the 0 px
widths change nothing when nothing is hidden. Script: the session's `fullwidth.ps1` (not in the repo;
the shape is this paragraph).

## P13-M2: everything by mouse

The mouse stream, from 2026-10-27, branch `feat/p13-m2-mouse`: built on P13-M1's branch, then moved onto main
at 34449ee once P13-M1 (#128) and P13-M3 (#129) had merged, as two commits (the bars, then the band).

### Settled first

- **The bar's overflow, nothing per frame.** Each bar reports its width once per change (`width-changed`,
  the Toolbar's `init` and `changed width`: a window resize, the splitter, the split, the sidebar, the
  preview column). `toolbar::fit` turns the width into a count, and the bar shows its first `fit` buttons
  through one `if` per button. Nothing is evaluated per frame or per row; a count change rebuilds only the
  bar's buttons. The labelled widths are measured once per width change, in the heavier weight, so a
  label is never clipped. More never overflows.
- **The name's width for the band, without a per-row cost.** Nothing new sits in the rows. A left press on
  a details row asks Rust where it landed (`row-press`, a callback returning a bool), and Rust measures that
  one row's name with the window's hidden measuring Text, as `colfit` does (`invoke_measure`), when the
  press lands. The rows keep their single TouchArea; its handler sends the moves and the release to the
  band when the press began one.
- **One commit for the two bars.** The path bar's buttons and the toolbar's new ones read the same new
  facts (`ToolState`'s fields and `toolbar::with_facts`), so they went in as one commit. The band is a
  commit of its own.

### What landed

- **The path bar's buttons and the toolbar's new buttons and menus** (commit 1):
  - The path bar: Back, Forward, Up and Refresh before the crumbs.
  - The toolbar: Undo and Redo with their records' text in the tooltips, View, and More with its six
    items.
  - The two menus are the Windows 11 menu's own popup (`winmenu::Cmd::Tool`).
  - Each bar overflows into More on its own.
  - With `toolbar = "none"` only the path bar's buttons show.
  - The menu rows' key column is now 96 px, 12 px wider, for "Alt+Shift+Enter".
  - Driver: `navbutton`, `toolmenu`, and `toolbar <pane> <undo|redo|view|more>`.
- **The band in the details list** (commit 2):
  - `band::press_on`, `touched_rows` and `cursor_rows`.
  - The press decision in `main.rs` `row_press`.
  - The band's rectangle now draws over either view.
  - The driver's row aim moved onto the name's first letters. The pane's middle, where it aimed before, is
    now a row's blank part and would start a band, so the old `dragrow`, `click` and `rclick` aims keep
    their old meaning.

### Scripted checks (2026-10-27, the branch's release build, `SLINT_BACKEND=winit-software`, no settings file unless named)

- **The path bar**, in `C:\Windows\System32`:
  - Up went to `C:\Windows`, Back to System32, Forward to `C:\Windows`, and Refresh listed it again, each
    with its `[nav] ... button` line.
  - A right click on Back opened the history menu with its three places.
  - Screenshot: the four buttons before "This PC > C: > Windows", and Refresh's tooltip "Refresh (F5)".
- **The View menu**: Details is ticked and the keys are at the right. Large icons switched the pane to the
  grid, and Details switched it back (screenshot).
- **The More menu** (labelled bar at 1192 px):
  - Fits 11 of 12 buttons, so Preview pane comes first, then a separator and More's six items.
  - Show hidden files is ticked after its first pick.
  - Copy and Move to other pane are off with nothing selected.
  - Select all, then Copy to other pane: "Copied 2 items".
  - In the other pane, Select all, then Move to other pane: "Moved 2 items".
  - Size all folders sized the three folders.
  - Undo by the toolbar: "Undid: Moved 2 items", the files back where they were.
- **The in-pane toolbar** (`C:\traces\toolbar-pane.toml`):
  - Splitter to 0.30: the left pane's bar fell from 12 to 7 buttons, one line per change (468 to 319 px).
  - Its More listed Undo and Redo (off), the three views (Details ticked), Two panes (ticked) and Preview
    pane, then the six items (screenshot).
  - The right pane's bar kept all twelve.
  - In the narrow pane the crumbs have no room left beside the four buttons and the 150 px search box.
- **The band in the details list** (`C:\Windows`):
  - A drag straight down from row 2's blank part selected rows 2 to 5, the cursor on 5.
  - With Ctrl from row 7: 6 selected. With Shift from row 9: 8 selected.
  - A press on row 0's name and on row 1's icon, released in place: a click on each row (selected alone).
  - A band from row 11 held, then Escape: back to the selection before it.
  - A press on row 4's blank part released in place: row 4 alone, as a click.
  - In `C:\Windows\Boot`, from below the last row up past the top: all 9 entries, the cursor on row 1.
  - A first run of a straight-down drag selected nothing, because the grid's rule touches nothing at zero
    width. Since then the details band touches rows by its height alone (`touched_rows`), as Explorer's
    does.

### Soak

Per cycle in `tools/soak-p5m3.ps1`:

- The toolbar cycle's Delete undone and redone by the toolbar.
- Each path bar button once, in `src\tree`.
- A View menu round trip.
- In `m2` and `m2b` (made by the setup): More's Select all, Copy to other pane, and back by Move to other
  pane, replacing (TWINFM_SOAK asks nothing).
- Size all folders in `src`, and Show hidden files twice.
- A band from row 0's blank part to row 1 in `m2`, selecting the two files.
- Reported: path bar buttons (four per cycle), View picks (two), More picks (seven), details bands done with
  2 selected (one), presses on a blank part (one).
- The toolbar presses' count now leaves out the picks and the `fits` lines: twelve per cycle (P6-M3's
  eight, P13-M1's two, P13-M2's Undo and Redo).

Two 10-minute runs on 2d94fed's code (the branch's own release build, moved onto main at 34449ee, so P13-M1
and P13-M3 are in it), on the coordinating session's go:

- **The first, disturbed** (2026-10-27, 23:12 to 23:22): the sizes stream ran a scripted TwinFM window about
  60 to 76 s in and `cargo test --release` (which overwrites the clipboard and uses the Recycle Bin) about 268
  to 275 s in, a hold that reached it late. The run read clean all the same: 10.1 minutes, closed by the
  script, exit code 0, 0 panicked lines, 237 jobs done (0 failed, 0 cancelled), 0 soak refusals, 0 waitops
  timeouts, no `another program took the clipboard` line, every count as its line says and every "must be 0"
  0. Its working set went 145 to 238 MB with two samples at 320 and 318 MB (305 s and 485 s in; P13-M3's
  soak also read at most 321); the clean run below has no such sample.
- **The clean run** (nothing else on the machine, both other streams holding): 10.1 minutes, closed by the
  script, exit code 0, **0 panicked lines**, 16 cycles started (3 whole), **245 jobs done, 0 failed, 0
  cancelled**, 0 soak refusals, 0 waitops timeouts, every "must be 0" 0. Working set 146.5 MB at 5 s, 221.1
  MB at 601 s, **at most 229.1 MB**; private 164.7 to 228.8 MB.
  - **Mouse: path bar buttons 12 (four per cycle), View picks 6 (two), More picks 21 (seven), details bands
    done with 2 selected 3 (one), presses on a row's blank part 3 (one).**
  - Toolbar presses 46 (twelve per whole cycle), refused as disabled 0.
  - Grid bands: 8 started, 8 done with 2 selected, 0 cancelled; medium grid's done with the cursor on tile 0:
    8. Drags: 19 cancelled, 0 dropped.
  - P13-M1's lines as its soak read them: drags read back as Size 90 -> 60 3, widths back at 120, 90, 140 7,
    Two panes presses 6, narrowed 9, name, Size and Modified 16. P13-M3's: sizes view walks 3, levels opened
    in place 6, `sz.txt` off three totals 3.

### Gate

ab68, 2026-10-28, Jeremy at the console: 2d94fed (`target-p13m2`, built `--locked` from the detached
`gate-p13m2` worktree) against `target-p13base` (main at 34449ee, P13-M1 and P13-M3 merged, so P13-M2
alone), by `tools/gate-p13m2.ps1`, 18 minutes; summary `C:\traces\gate-ab68-summary.txt`. WPR main-thread
busy:

| Run | Defaults | With `toolbar-pane.toml` |
| --- | --- | --- |
| p13base 1, 2, 3 | 18.0, 23.3, 23.4 | 23.4, 23.5, 24.5 |
| p13m2 1, 2, 3 | 24.9, 23.4, 23.8 | 23.2, 24.5, 24.5 |
| Median | 23.3 against **23.8** | 23.5 against **24.5** |

Every check ok, the runner exited 0, and **no gap counts**: the runs overlap in both settings, and the
toolbar-pane medians differ by exactly 1.0 point, which the rule does not count. Two things to read with it:

- **p13base-1 ran in the cheaper state** (render p50 1.95 ms, WPR 18.0); every run after it sat in the
  costlier one (2.74 to 3.06 ms). Like for like, without it, the defaults' reference runs are 23.3 and 23.4
  against the candidate's 23.4, 23.8 and 24.9.
- **The candidate's render p50 sits a little higher in both settings**: 2.95 to 3.06 ms against 2.74 to
  3.04 ms. The candidate always ran in the second slot of its pair, which reads high in console triplets too
  (ab53 against ab54), so this small cost is not separated from the slot bias. An order-swapped rerun would
  separate it, if Jeremy wants one.

Every candidate run ended with both panes showing name, Size and Modified at 499 px (the scenario's split
at 1200 px; P13-M1's rule, unchanged).

### Console checks (for Jeremy, unelevated, after the gate, light and dark, beside Explorer)

- Every new button and item.
- A band from below the last row, with Ctrl and with Shift.
- A file drag from the name and from the icon still drags.
- The bar at the window's minimum width.

### Calls made in this milestone, for Jeremy to check

- More's first items are the buttons that did not fit, in the bar's order, and the View button that does
  not fit becomes its three items, not a submenu.
- Copy and Move to other pane need two panes and a selection, as their keys do.
- A press on a row's blank part selects that row on the release, not on the press (the press may become a
  band).
- At the default 1200 px window the labelled bar has no room for Preview pane, which sits in More.

## P13-M3: the sizes view, and size on disk

Started 2026-10-27 from main at d29bdac, branch `feat/p13-m3-sizes-view`, the sizes stream. The coordinating
session gave the go for the settle-first measurement (a `C:\` walk), and allowed tests, builds and scripted
TwinFM windows in the stream's own worktree; soaks, gate builds and the in-app `C:\` run still ask first.

### Settled first (2026-10-27)

**Size on disk: the directory information class, one call per 64 KB of entries** (`src/fs/dirinfo.rs`).
`GetFileInformationByHandleEx` on the folder's handle with `FileIdBothDirectoryInfo` gives every entry's
`AllocationSize` (its clusters) and its file ID, and `FileFullDirectoryInfo` the same without the ID.
Checked against `GetCompressedFileSizeW`, a call per file, on files the test makes
(`compressed_and_sparse_files_take_less_on_disk`). A 1 MB NTFS-compressed file reads 65,536 bytes both ways,
once the cache has written it out. A 64 MB sparse file with nothing written reads 0 both ways. A plain 1 MB
file reads 1,048,576. A tiny file that lives in the MFT reads 8 bytes; Explorer shows 0. Measured by the
ignored `measure_walks` test, a release build, each root walked twice (round 0 cold, round 1 warm). The mouse
stream's builds ran at the same time, so these times are rough:

| Root | Today's walk (FindFirstFileExW) | Directory info, no IDs | Directory info, with IDs |
|---|---|---|---|
| `C:\traces\bigdir` (40,000 files) | 126 ms, 48 ms | 23 ms, 20 ms | 1,289 ms (first use), 37 ms |
| `C:\` (1.26 M files, 370 k folders, 201 unreadable) | 112.5 s, 63.2 s | 37.2 s, 59.1 s | 53.5 s, 46.7 s |

The directory information is no slower than today's walk, and it is the same one call per folder (per
64 KB of entries). WOF compact files (CompactOS): none on this machine (0 met), so their directory sizes
could not be checked. They are counted at what the directory says (tag 0x80000017 is not treated as a link).

**Hard links: once per walk, by file ID.** The directory information has no link count, so every file's ID
goes into a set (`dirinfo::HardLinks`). On `C:\` that is 1.2 M IDs, and the test process grew from 18 MB to
36 MB of working set (5 to 23 MB private) against the walk without IDs, at no extra time worth noting. The
walk met 59,344 more names of files already counted (WinSxS and System32 share theirs), 15.7 GB. Counted
once, `C:\` holds 752 GB on disk. Counted per name, it holds 768 GB, which is what Explorer's per-folder numbers
add up to. Which name counts is the first one the walk meets; the others count 0. So a folder of nothing but
links to files elsewhere under the root reads small, and the files show under one name, as TreeSize's
"count hard links once" does.

### What landed

- **The view** (`src/sizeview.rs`, pane.slint `SizeList`): a tab of its own kind, `Sizes: <folder>`, with a
  "Sizes" crumb after the root's. Its rows are each level's folders and files, biggest first, then by name.
  Each row shows a share bar (its size on disk against the root's), its size on disk, and for a folder its
  file count and a chevron. The chevron, Right and Left open a folder in place one level down or close it.
  Left on a file or a closed folder goes to its parent's row. Return or a double-click on a folder opens it
  in a normal tab. The header reads Name, Share, Size on disk (sorted, the arrow down) and Files. The status
  line reads `Walking… F files, S so far`, then `S on disk in F files, D folders, in T s`, then `, K folders
  could not be read` when any were skipped. The numbers and the order are rebuilt every 250 ms while the walk
  runs, the cursor and the selection kept on their items by path.
- **Where**: the Windows 11 menu's "Show sizes", after "Add to bookmarks", on folders and drives (a tab each)
  and on a folder's or This PC's background. On This PC every fixed drive but Google Drive's volume is sized,
  one row each. Refused with a note inside an archive and on Google Drive's volume. OneDrive is walked, its
  cloud-only files at 0. ⋯ More's item comes with P13-M2, once both have merged.
- **The walk**: a thread of its own per view (the lead's call, changed with the coordinating session's
  word: a `C:\` walk on the shared `sizes` worker would hold up Alt+Shift+Enter for a minute). It keeps
  every folder in a `Tree` (a linked list of children, no Vec per folder) with its totals, and no file
  entries. A level's files are read again when it is opened (one folder's listing, on a thread). Reparse
  points other than compact files count 0, and a junction or directory link is counted as a folder and not
  entered. Folders deeper than 64 levels are not entered, as folder sizes do.
- **Acting on it**: the rows are the pane's entries, named by their path under the root as search results
  are, so the Windows 11 menu, Delete, Shift+Delete, drag out and Return work unchanged. Rename, New, Paste and
  drops into the view are refused, as in search results. After a delete or a move out of the root, each item
  comes off every total above it at once (`sizes_removed`), with no new walk. The scripted check deleted
  `big\b.bin`: 1,003,520 bytes came off 2 totals. F5 then walked again, and its total was 3,609,160 bytes,
  exactly 4,612,680 less 1,003,520.
- **Restored tabs**: a sizes tab is saved with its tab list (`sizes = true`) and walked again when it is shown.
- **The view setting**: a pane set to icons shows a sizes tab as a list (`sync_grid`). Ctrl+Shift+1, 2 and 3
  there set the view for the pane's other tabs.

### Calls made in this milestone, for Jeremy to check

- **A call changed (accepted by the coordinating session, 2026-10-27): the walk runs on a thread of its
  own per view**, as a search's does, not on the shared `sizes` worker the plan named. A long walk of `C:\`
  never holds up Alt+Shift+Enter.
- **Changes from elsewhere are not followed**: the plan had the subtree watch re-walk a changed folder, as
  folder sizes do. The watch only names which child of the root changed. On `C:\`, `Windows` and `Users`
  change every few seconds (logs, caches), so the view would keep walking tens of GB. Instead the numbers
  stand as of the walk, and F5 walks again. A move within the root is not followed either, since the root's
  own total does not change. **Open with Jeremy**: the coordinating session puts this to him before the
  merge; the branch keeps F5 until he answers.
- **Hard links count once per walk** (above), so `C:\` reads 752 GB rather than Explorer's 768 GB.
- **The icon column stays at the left**, and the chevron and the name are indented 16 px per level. The pane's
  icon strip stays one image, so nothing per row was added for it.

### Scripted checks (2026-10-27, the branch's release build, `TWINFM_SCRIPT`, no settings file)

A scratch tree in `%TEMP%\twinfm-p13m3-check` (4.6 MB in five files, three folders): the view opened by
`sizeview`, `big` and `big\deep` opened in place (`opened ... in place (1 children)`, `listed ...: 1 files`),
Left twice (deep closed, the cursor to big's row), `big\b.bin` deleted from the view (to the Recycle Bin: the
check runs without `TWINFM_SOAK`, so Shift+Delete would ask), F5, the menu's "Show sizes" on a row (`[winmenu]
pane=0 runs Sizes`, a new tab on `big`), Ctrl+Shift+2 in the view (`kept for the pane's other tabs`), and
the tabs closed (`[view] pane=0 shows large icons` as the folder tab came back). Screenshots light and dark
(`SLINT_BACKEND=winit-software`, PrintWindow) show the rows, bars, numbers and status line as the mockup drew
them.

### Size on disk in the normal list

Built after P13-M1 and the sizes view merged (main at 34449ee), on its own branch
`feat/p13-m3-size-on-disk`, by the route agreed with the coordinating session: a fifth cell in each
details row component, reading a width property, `ondisk-w`, as the others read theirs (P13-M1's route:
0 px when off or dropped, and a zero-width Text draws nothing), first to make way for the name
(`cols::ondisk_shown`: Size on disk, then Type, Size, Modified), filled by a second read of the folder
(`fs::dirinfo::disk_by_name`) only while the setting `size_on_disk` is on. The dialog's "Size on disk
column" switch, under the Type column's, turns it on and lists the panes again. A watch edit reads a
changed file's size on disk from its own handle (`fs::dirinfo::disk_of`, the same figure as the
directory's, tested on a plain, a compressed and a sparse file).

**Calls made here, for Jeremy to check:**

- **At the row's right end, after Modified**, where Explorer puts a column added, so every grip and the
  name's width shift by the column's width and nothing else changes.
- **A fixed 100 px, no grip and no sort**: the least code. A grip would make it a fourth `cols::Col`
  through the drags, fits, settings and driver; a sort would need the size on disk in every listing.
- **Folders show nothing in it**, as the Size column shows nothing for them; a folder's size on disk is
  the sizes view's.
- **A file small enough to live in the MFT reads 8 B**, the directory's figure; Explorer shows 0.

**Scripted check** (2026-10-27, the branch's release build, a scratch folder with a 1 MB plain file, a
1 MB zero-filled file compressed by `compact /c`, a 5-byte file and a folder): the column on through the
dialog (`[cols] pane=0 shows name, type, size, modified, size on disk at 1010 px`, `[enum] ... size on
disk: 5 entries read in 0.1 ms`); 980.0 KB, 0 B and 8 B in the column, the folder blank; a `touch` became a
watch edit (`applied: +1`); `colwidth`, `coldrag` and `colfit` hit their grips with the column shown; the
split made the left pane 499 px and the column went first, with Type (`shows name, size, modified`);
`splitter 0.8` brought it back at 608 px; off through the dialog. Screenshot in the light theme: the column
at the row's end, the header "Size on disk" unclipped.

**Soak** (2026-10-28, the branch's release build, code as cb91532, on the coordinating session's go):
`tools/soak-p5m3.ps1 -Minutes 10`: 10.1 minutes, closed by the script, exit code 0, 0 panicked lines, 259
jobs done (0 failed, 0 cancelled), 0 waitops timeouts, 0 soak refusals; 13 cycles started, 3 whole. Every
"must be 0" is 0. **Size on disk column: on 3, off 3, the left pane shown with it 3, listings read with it
3** (one each per cycle: the right pane sits in `a.zip` when the column goes on, and an archive level is
not read), the watch edit of `od.txt` applied as a row edit. P13-M1's "name, Size and Modified" lines 19
(six per cycle with this cycle's return to the 0.5 split). The sizes view's counts as before (3 walks, 6
levels, 3 removals). Working set 147 MB at 5 s, 228 MB at 601 s (at most 268); private 164 to 222 MB.

**Gate ab69** (2026-10-28), its own triplet (the extra cell per row), `tools/gate-p13m3b.ps1`: `target-p13od`
(c25173c, code as cb91532) against `target-p13base` (main at 34449ee, so the column alone differs), the
defaults and `C:\traces\ondisk.toml` (`size_on_disk = true`, read-only). WPR medians **p13od 17.4%** (column off)
and **p13od-ondisk 17.6%** against **p13base 16.6%**; every reference run (16.4 to 16.7) below every
candidate run (17.1 to 18.1): a gap that counts, with the column off too. Render p50 1.82 to 1.94 ms in
every run (one state). The traces' main-thread buckets (ms in the 10 s after the split, medians): total
1665 against 1735; the ListView's rows (`ensure_updated_listview`) 379 against 420 (+41); the
accessibility tree's full rebuild 134 against 159 (+25, +19%); render 885 against 894 and text 445
against 432, flat. So it was the fifth `Text` in every row: one more element made per row as the list
scrolls, and one more accessibility node (a row's five became six, the +19%). The code explained the gap,
so the order-swapped rerun the coordinating session had prepared (ab70) was dropped.

**Zero cost when off** (Jeremy's choice, 2026-10-28, of the two the coordinating session put to him: the
cell hidden from the accessibility tree, or lists of its own): the four details lists are main's again,
and the cell lives in two more, `FileListOnDisk` (22 px rows) and `FileListOnDisk32` (thumbnails), each a
rows component and its focus layer, chosen by `FileListHost` once per pane while `size_on_disk` is on (the
window's `size-on-disk`), as the Type setting chooses its lists. They keep the Type cell at the pane's
Type width (0 with Type off or hidden), so no copy without it is needed: two components, not four. With
the setting on, the column still makes way for the name through its width. Gates on the change: build
clean; 336 tests passed. Scripted check (a scratch folder; the widths set to Type 100, Size 110 first):
the column on (`shows ..., size on disk`), thumbnails on with it (the 32 px rows with the cell), Type off
with it (the cell kept beside Name, Size and Modified), then everything back and the column off (the plain
rows, no cell); the cursor (row 1) and the scroll (0) kept, and the widths kept (Size moved from 110 by
the last `colwidth`). Screenshots of the four states. Soak of the change (2026-10-28, the branch's release
build, code as 0a835c8, main's P13-M2 merged in): `tools/soak-p5m3.ps1 -Minutes 10`: 10.1 minutes, exit
code 0, 0 panicked lines, 251 jobs (0 failed, 0 cancelled), 0 waitops timeouts, 0 refusals, 16 cycles
started, 3 whole; every "must be 0" at 0; the column on 3, off 3, shown 3, listings read 3; P13-M2's mouse
counts as its line expects; working set 149 MB at 5 s, 234 MB at 601 s (at most 238), private 184 to 252
MB. **ab70** (`tools/gate-p13m3b-ab70.ps1`, 2026-10-28 10:32): `target-p13od2` from 1824ea8 against
`target-p13base`, the defaults and `ondisk.toml`. **Run at 22 Hz, not the gate**: after the move to the
console, frames came every 46 ms, not the protocol's 60 Hz (ab69 ran at 16.4 ms). Read like for like it
showed no gap: WPR medians p13od2 9.3% and p13od2-ondisk 9.6% against p13base 9.0% (reference runs 9.8,
9.0, 8.9; p13od2 9.4, 9.3, 9.2; the column on 9.5, 9.6, 9.6), no reference run below every candidate run,
every check ok, render p50 1.85 to 2.11 ms. Summary `C:\traces\gate-ab70-summary.txt`. The gate is ab71,
the same wrapper with its tag changed (the coordinating session's `C:\traces\gate-p13m3b-ab71.ps1`), at
60 Hz.

**Gate ab71** (2026-10-28 10:51, 60 Hz: every run drew at 16.35 to 16.44 ms; every check ok; every run
under 20%). WPR runs: p13base 19.2, 16.8, 16.6; p13od2 16.8, 17.0, 17.2; p13od2-ondisk 18.4, 17.9, 18.0.
Summary `C:\traces\gate-ab71-summary.txt`.

- **Column off: p13od2 17.0% against p13base 16.8%, no gap. Zero cost when off, confirmed.**
- **Column on: 18.0% against 16.8%, +1.2 points**, a gap that counts by the medians (no reference run
  below every candidate run). Render p50 1.99 to 2.11 ms against the reference's 1.87 to 2.12 ms.
  Whether this cost is accepted is Jeremy's call.

Where the column-on cost goes, from the traces' main-thread buckets (ms in the 10 s after the split,
medians, the column on against the same build with it off): total 1700 against 1798 (+98); the ListView's
rows 367 against 414 (+47); the accessibility tree 191 against 212 (+21, the full rebuild +20); text
drawing 419 against 443 (+24); the render pass 931 against 939 (+8). In that window the column is **not
shown**: the gate's split leaves both panes at 499 px, where it has made way for the name (`[cols] pane=0
shows name, size, modified at 499 px` in every column-on run). So the +1.2 points is the column's lists
carrying their cell at 0 px in every row: one more element made per row as the list scrolls, one more
accessibility node, and the zero-width text still passed through text drawing (ab69's mechanism, now only
with the setting on). The second read is not on the main thread: the listing thread read 40,000 entries'
sizes on disk in 20 to 29 ms per listing, before the window, besides the 67 to 92 ms listing. A way to
take the hidden case to zero too, for Jeremy if he wants it: have the host pick the lists with the cell
by whether the column shows (`shows-ondisk`, already per pane) rather than by the setting, so a pane where
it has made way uses the plain lists; a column coming back would then swap the pane's list.

**Cut the on cost first** (Jeremy, 2026-10-28, after ab71): `FileListHost` picks the cell's lists by
whether that pane shows the column (`ondisk` is the pane's `shows-ondisk`), not by the setting; the window's
`size-on-disk` property and its two setters, which fed only the old pick, went. A pane where the column has
made way uses main's plain lists. Nothing per row or per frame was added: `shows-ondisk` changes only in
`update_shown`, on a list width change. Gates: build clean; 336 tests passed. A swap cannot feed back into
the width it was picked by: the width is reported by the host (`list-width`, its `init` and `changed
width`), which stays when its inner list swaps; a grip drag's freeze (`COL_DRAG`, P13-M1) holds the set,
this pick with it, until the release; the splitter has no freeze and needs none, a swap being one per
crossing. Scripted check (`C:\Windows\System32` in the left pane, the column on, Size set to 110, the list
scrolled to row 218 and the cursor on row 221): the split (both panes 499 px, the column gone from the
left), `splitter 0.85` (the left pane at 610 px showing it beside the right at 294 px without it), back to
0.5, three more crossings each way, the window maximised (both panes showing it at 1947 px) and restored.
Every crossing changed the set once, at 8 px steps (no back and forth); the cursor stayed on row 221 and the
widths were kept (the last `colwidth` moved Size from 110). The scroll moved once, at the first swap,
from -4800 to -4796 (row 218's top): a new list lands on a whole row, and main's swap for the Type setting
does the same (-4800 to -4796 on `target-p13base`); with the column off the split kept -4800. Every later
swap kept -4796 exactly. Screenshots of the left pane showing the column beside the right without it, and
of both showing it maximised (found by the run's own process id: the first capture, by the title prefix
alone, found Jeremy's everyday window, open at the time, and was deleted unread; CLAUDE.md's screenshot
note now says so). Soak (code as 3c42b79): `tools/soak-p5m3.ps1 -Minutes 10`: 10.1 minutes, exit code 0, 0
panicked lines, 250 jobs (0 failed, 0 cancelled), 0 waitops timeouts, 0 refusals, 16 cycles started, 3
whole; every "must be 0" at 0; the column on 3, off 3, shown 3, listings read 3; working set 149 MB at 5 s,
233 MB at 601 s (at most 238), private 184 to 252 MB. **Gate ab72** (`tools/gate-p13m3b-ab72.ps1`):
`target-p13od3` against `target-p13base`, the defaults and `ondisk.toml`. Run 2026-10-28 at the console's
60 Hz (an earlier attempt at 22 Hz is set aside in `C:\traces\ab72-22hz\`, not counted); summary
`C:\traces\gate-ab72-summary.txt`. WPR main-thread busy: p13base 18.5, 19.6, 19.3 (median 19.3); p13od3
19.1, 19.7, 20.7 (median 19.7, no gap); p13od3 with the column on 20.6, 19.2, 19.1 (median 19.2, no gap).
Render p50 2.41 to 2.64 ms across all nine runs, no change of state. **Passed**: with the lists picked by
whether the pane shows the column, the column's setting costs nothing in the gate's 499 px panes, where it
makes way, and ab71's +1.2 points with it on is gone. The merge waits for Jeremy.

### Soak (2026-10-27, the branch's own release build, code as 8cdda35)

`tools/soak-p5m3.ps1 -Minutes 10`, on the coordinating session's go (the mouse stream editing only): 10.1
minutes, closed by the script, exit code 0, 0 panicked lines, 259 jobs done (0 failed, 0 cancelled), 0
waitops timeouts, 0 soak refusals; 13 cycles started, 3 whole. Every count is one per cycle or as its line
says, and every "must be 0" is 0. **Sizes view: 3 walks done, 6 levels opened in place, 3 removals of
`sz.txt` off three totals**, each one per cycle; no `could not list` or `cancelled` line. Working set 167 MB
at 5 s, 239 MB at 601 s (at most 321); private 193 to 273 MB. The 60-minute soak at the phase's end is the
memory check.

### Gate (2026-10-27, tag ab67)

`tools/gate-p13m3.ps1`: `target-p13m3` (8cdda35, built `--locked` from the detached `gate-p13m3` checkout)
against `target-p12seg` (3698cd2), the defaults only, on the console at 60 Hz, run by Jeremy after ab66.
WPR medians: **p13m3 21.1% against p12seg 21.2%**. Runs: reference 21.2, 21.4, 20.9; candidate 21.2, 21.1,
21.1. No gap counts; every check ok (no panic, no `[sizeview]` line in any run). Render p50 2.84 to 2.94 ms
in every run, so the session kept one state. Summary `C:\traces\gate-ab67-summary.txt`, traces
`C:\traces\twinfm-ab67-*`.

### The in-app `C:\` run (2026-10-27, after the gate, the machine free)

`target-p13m3`'s exe, a script of `sizeview 0 C:\` and `waitsizeview 900000`, no settings file, the other
streams holding. **74 s** for 756 GB on disk (761 GB logical) in 1,273,588 files and 373,474 folders, 202
folders unreadable. The `done:` line: working set 178 MB, **+72 MB since the start** (the 150 MB limit
holds), private 166 MB, 373,423 folders and 1,214,052 file IDs kept. The window's peak working set, sampled
every 2 s from outside, was 202 MB. 279 `partial` lines (under four a second), no panic.
