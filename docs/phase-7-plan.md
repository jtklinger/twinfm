# Phase 7: TwinFM finishes its context menu, opens every archive, and polishes the rest

Written 2026-10-18, when Phase 6 closed with P6-M1 to P6-M3 merged and the P6-M3 build at a median
of 18.6% against the reference's 20.3% on the gate under the triplet protocol (see
`phase-6-plan.md` § Phase 6 close). Phase 1 made TwinFM usable for a day instead of Files, Phase 2
made it trustworthy every day, Phase 3 made it find and show what Explorer can, Phase 4 showed a
folder as large icons with thumbnails, Phase 5 finished the Phase 2 lists, Phase 6 showed what the
cloud knows, finished the views and added the toolbar. Phase 7's goal, chosen by Jeremy on
2026-10-18 from the Phase 6 close's candidates: **the context menu finished (shell verbs off the UI
thread, the menu's verbs through TwinFM's jobs, a Windows 11 style menu), archives beyond zip, and
the polish bundle with the drag cursor check**. Not in Phase 7, by the same decision: content
search, thumbnails beyond the listing, cloud polish, a daily-use stretch, the Slint fork's third
patch and upstreaming. Accessibility stays deferred for much later (2026-10-15).

What Jeremy decided at the kickoff (2026-10-18), from the choices put to him:

- Shell verbs off the UI thread was decided before the kickoff (2026-10-17: "Run the shell-menu fix
  as phase 7 item"; `phase-6-plan.md` § Recorded for Phase 7 holds the design notes).
- Beside it, both menu candidates: the shell menu's verbs through TwinFM's jobs (with Escape
  cancelling a cut) and the compact Windows 11 context menu.
- The compact menu is **a Windows 11 look-alike**: TwinFM draws its own short menu as Explorer's
  (an icon row for Cut, Copy, Rename and Delete, the common items, the packaged apps' items), with
  "Show more options" opening today's full menu. Offered the cheaper "classic menu plus icons", he
  chose the look-alike.
- Archives beyond zip, **browse and write all**: 7z, rar, tar, gz and the rest open as folders and
  copy out, and writing into them is tried for every format, zip included, as far as Windows'
  handlers allow (checked first; a format whose handler refuses stays read-only, recorded).
- The polish bundle, all of it (toolbar tooltips, Back and Forward over the toolbar, background tabs
  with sizes taking records as edits, search results sorted by the file's own name, arrow keys in
  the sidebar, Explorer's PageDown in the details list, tile names on two lines), and the drag
  cursor check.
- The order: the menu first (the decided item and the verbs through jobs together, then the compact
  menu), archives third, the bundle last.
- `CLAUDE.md` gains the Phase 6 lesson on layout checks with unequal panes (approved 2026-10-18; in
  § Measuring with this kickoff).

The starting point is main at 33d1c1c (the Phase 6 close). What the four milestones rest on today:
the context menu is the shell's classic `IContextMenu` (`src/shellmenu.rs`), built, shown with
`TrackPopupMenuEx` and invoked on the UI thread inside one `show` call, with TwinFM's own items
(ids 1 to 20) above the shell's (100 to 0x7FFF) and the "rename" verb sent to TwinFM's name editor;
every other shell verb, Delete, Cut and Copy included, runs as the shell's own operation, outside
the file operation worker, its progress sink, the undo stack and the soak fence. The jobs
(`src/fileops/mod.rs`, `Job`: copy, move, move-as, delete, rename, new folder, new file, restore)
run on one worker with an undo and redo stack. Escape in a list stops a search and cancels queued
jobs, or else clears the selection; a cut's marks clear on a paste, a copy or another program
taking the clipboard, never on Escape. Zip archives are listed by TwinFM itself (`src/fs/archive.rs`:
the central directory read with the `zip` crate on workers, eight archives cached, `split` asking
the file system only for components named `*.zip`), extracted by the shell's zip folder
(`CompressedFolder`), and every write into one is refused (`[archive] refused: ... (writing into a
zip archive is not supported)`); a foreign zip view dropped on TwinFM still extracts synchronously
inside `Drop`. On this machine (Windows 11 26H2) `.7z`, `.rar`, `.tar`, `.gz` and `.tgz` belong to
the shell's `ArchiveFolder` ({0C1FD748-B888-443D-9EC3-AD7E22D48808}), which Explorer browses as a
folder; TwinFM treats them as plain files. Our own drag source's `GiveFeedback` returns
`DRAGDROP_S_USEDEFAULTCURSORS`.

Four milestones, in the order Jeremy gave on 2026-10-18 (do not fold them):

1. **P7-M1, shell verbs off the UI thread, and the menu's verbs through TwinFM's jobs.** A shell
   verb picked in the context menu runs on a dedicated verb worker, an STA thread with its own
   message loop that rebuilds the menu for the same items and invokes the verb there, so a verb
   waiting on a slow file system (Google Drive's "Create shortcut" on `G:`, a minute of white
   window on 2026-10-17) no longer holds the UI thread. The design notes in `phase-6-plan.md`
   § Recorded for Phase 7 are the brief: invoke by verb string, by offset only when the rebuilt
   item carries the same label (a pure function with unit tests), TwinFM's own items and the
   rename verb decided on the UI thread from the menu shown, `GetCommandString` on the UI thread,
   the owner window chosen after trying the hung case on `G:` (a hidden window owned by the worker
   or no owner, recorded with why), `CMIC_MASK_ASYNCOK` only with the worker pumping until the
   shell lets go and a bounded, logged wait at quit, the result back through
   `invoke_from_event_loop`, and the `[menu] InvokeCommand` line on the worker with the verb's
   time. Then the routing, decided on the UI thread before anything goes to the worker: the
   canonical verbs `delete`, `cut`, `copy` and `rename` picked in the menu run TwinFM's own
   commands (the ones the keys and the toolbar call: Delete recycles, Shift held makes it
   permanent as the key does, Cut and Copy write the clipboard and the marks, Rename opens the
   name editor as today), so they get an undo record, the progress percentage and the soak fence;
   every other verb goes to the worker. Inside an archive level the routing keeps today's refusals.
   Escape in a list, with nothing to stop or cancel, clears a cut's marks before it clears the
   selection; whether it also empties the clipboard is checked against Explorer first and recorded.
   The scripted seam: a driver step that queues a verb job on the worker directly (a test-only
   verb that sleeps there, behind the `TWINFM_SOAK` fence) while the script scrolls, passing when
   `[perf]` frames keep coming through the sleep, and the same step with `copy` on a scratch file
   end to end; a second step that feeds the routing a canonical verb as if picked (the driver
   never presses Return in a shell menu). Gate: the defaults triplet (the scenario opens no menu,
   so it is expected flat; it runs anyway). Soak: a verb cycle (the sleeping verb, the routed
   delete and its undo, the routed cut and Escape) added to `tools/soak-p5m3.ps1`. Console checks:
   "Create shortcut" on `G:` while the other pane scrolls; Delete from the menu then Ctrl+Z; Cut
   from the menu then Escape; Properties and "Open with" from the worker.
2. **P7-M2, the Windows 11 style context menu.** A right click, Shift+F10 or the Menu key opens
   TwinFM's own short menu, laid out as Explorer's Windows 11 menu: a row of icon buttons for Cut,
   Copy, Rename, Share and Delete where they apply, then Open, Open with, the packaged apps' items
   (the `IExplorerCommand` handlers registered by apps such as Terminal and VS Code, which today
   appear only in Explorer's new menu), TwinFM's own items, Properties, and "Show more options"
   (Shift+F10 inside it, as Explorer) opening today's full classic menu at the same point. Every
   item calls the command its key, its toolbar button or its P7-M1 route already calls (the P6
   one-path rule); shell items run on the P7-M1 verb worker. Settled first, before any code, each
   put to Jeremy with a recommendation: how the packaged items are found (`IExplorerCommand` has
   no public enumeration for a host; the package manifests' `FileExplorerContextMenus` entries and
   their `PackagedCom` registration are the candidates, read once per process and on a package
   change) and what the menu shows if they cannot be found reliably; what the menu is drawn with
   (a Slint popup inside the window, which cannot pass its edge, or a window of our own that can,
   as Explorer's does); the icons (the shell's for its items, Segoe Fluent Icons for the icon row
   as the toolbar's); keyboard (arrows, access keys, Escape) and the menu inside an archive level,
   on This PC, on a drive and on a search result. The classic menu, its submenus and the
   `IContextMenu2/3` routing stay as they are behind "Show more options". The scripted steps
   (`rclick` and friends) open the new menu; the in-menu steps gain the new menu's items, and
   `menuopen` with "Show more options" reaches the classic one. Gate: the defaults triplet (the
   menu lives outside the lists; a Slint popup is a new sibling of the panes, so the generated list
   components are diffed against the reference's). Soak: the P5-M3 soak's menus go through the new
   menu and "Show more options". Console checks: the new menu beside Explorer's on a file, a folder,
   a drive, the background, inside a zip and on `G:`, at 100% and 150% scaling.
3. **P7-M3, archives beyond zip, browse and write.** Settled first: what Windows' `ArchiveFolder`
   does for each format on this machine (7z, rar, tar, tar.gz, tgz, gz alone, and whatever else its
   registration names): listing through `IShellFolder::EnumObjects`, extraction through
   `IFileOperation` from its items, and writing into it (`IFileOperation` copy, move and new folder
   with the archive's shell item as the destination, and the zip folder's own for zip), each
   format's answer recorded; a format whose handler refuses writes stays read-only with the
   refusal logged as today's. Then: other formats list as folders on a dedicated archive STA
   worker (the shell folder made, used and released there; never the UI thread), their levels
   cached as zip's are, with the `[enum] ... done:` line naming the handler; zip keeps its own
   central-directory listing (the 40,000-entry `bigdir.zip` is a map lookup today) and gains
   writing through the shell's zip folder; `split` learns the new extensions without asking the
   file system for paths that cannot be an archive. Writes into an archive (paste, drop, copy and
   move into, new folder, and rename and delete inside where the handler allows) run as jobs on
   the file operation worker with the progress sink; the archive's cache entry is dropped when the
   job ends, and the level is listed again (archives have no watch). Undo of a write into an
   archive: refused as "cannot be undone" unless the settled-first check finds the shell reports
   the new items' paths, recorded either way. The foreign synchronous zip drop (P3-M4 Deferred)
   moves into the extraction job as our own drags' do, if the data object can be read off the UI
   thread; else recorded. Fixtures: `C:\traces\bigdir.7z` and `C:\traces\bigdir.tar.gz` (the
   40,000 files of `C:\traces\bigdir`, made with `tar.exe`, not in the repo) beside `bigdir.zip`,
   with the listing time of each in the record. Gate: the defaults triplet (`split` runs on every
   path the scenario lists). Soak: an archive cycle in `%TEMP%\twinfm-m5\` (a 7z and a zip made by
   the setup: list, copy out, copy in, new folder in, delete inside where allowed). Console checks:
   each format browsed and written beside Explorer, a password-protected archive, an archive inside
   an archive.
4. **P7-M4, the polish bundle and the drag cursor check.** Toolbar tooltips (Slint has none built
   in: a small popup of our own after a hover delay, the button's name and its key, in each
   placement); Back and Forward mouse buttons over a toolbar button go back and forward as over the
   list; a background tab with folder sizes takes subtree records as edits instead of going stale
   and being listed again when shown (the P5-M1 record path, under its cap); search results sorted
   by the file's own name (the sort key, not the shown `sub\dir` label); Up and Down in the sidebar
   move along its rows once it has the keys (P6-M3 gave it the keys), Return opens; Explorer's
   PageDown in the details list (to the last row on screen first, a viewport from there; PageUp
   the same upwards); tile names on two lines in the large and medium tiles (Explorer's wrap,
   ellipsis on the second line, the full name on the cursor tile) chosen per tile component,
   never a per-tile binding. The drag cursor: a console check first (with TwinFM as the source and
   a description showing, does the OLE cursor glyph show beside the description's icon?); if it
   does, `GiveFeedback` returns `S_OK` and sets the cursor itself while a description shows, as
   Explorer's sources do; if not, nothing changes and the record says so. Gate: the defaults
   triplet, the grid and medium triplets (`C:\traces\grid.toml`, `C:\traces\grid-medium.toml`,
   since the tile's name changes) and the toolbar triplet (`C:\traces\toolbar-pane.toml`, since the
   toolbar gains tooltips), with the generated tile components diffed against the reference's.
   Soak: the P6-M3 toolbar cycle with hovers, and a sidebar walk. Console checks: every item beside
   Explorer where Explorer has it, the layouts set up unequal.

Each milestone brings its driver steps and stderr lines (named in its brief and recorded in
`CLAUDE.md` § Measuring), tests for its maths (the verb match by string or label, the routing of
canonical verbs, the Escape order, the menu's item list for a selection, the archive split with
the new extensions, the PageDown target, the two-line name break) and its soak cycle. Console
checks after each triplet, unelevated.

**The gate is re-run after every milestone**, under the protocol agreed on 2026-10-12 and
unchanged since: `tools/wpr-triplet.ps1` elevated at the console by Jeremy, accessibility on,
Narrator off, every other TwinFM window closed, the split-and-scroll scenario on
`C:\traces\bigdir`, on clean binaries built with `--locked` into their own `--target-dir
target-<name>` from detached `git worktree add` checkouts with default features. Three runs per
binary, interleaved, compared by median; a gap counts only when every reference run sits below
every candidate run or the medians differ by more than one point (one-sided: a cost), with each
run's `[perf] ... render[... p50=...]` read first so a session that changes state part-way is read
like for like. The reference for P7-M1 is `target-p6m3` (c632e93, P6-M3's gate build); after each
merge the new milestone's gate build becomes the next reference and the previous one is deleted
(`target-p6m2` goes once `target-p6m3` has served). Tags continue from ab25. Bar: main thread
under 20% busy over the 10 s after the split, judged against the reference's spread in the same
session, never against a recorded number. The implementer builds the gate binary and hands Jeremy
one PowerShell wrapper (a copy of `C:\traces\gate-p6m3.ps1`) for the triplet; Jeremy runs only
that script and the console checks. A milestone is not closed until its table is recorded here.
When a milestone fails: stop, symbolize, name the frames, and either fix the mechanism or report
the options; never trade the bar off silently.

**Settings triplets stay part of the protocol** (Phase 4's rule): a milestone that touches the
list, the grid, the icons or the pane's layout runs the defaults triplet and the settings triplets
its entry above names, each candidate's stderr checked for `[settings] file <toml> (read)` and the
line the setting implies before its triplet is read.

The architecture rules from Phase 1 hold (see `phase-1-plan.md`): only the UI thread touches a
Slint model and workers wake it with `slint::invoke_from_event_loop`; every COM object lives on the
thread that made it; no app-state `RefCell` borrow is held across a modal loop; no `unwrap` on
`Weak<App>`; `windows-sys` with hand-written calls; nothing per frame or per row without a trace
first.

Amendment (P2-M4), carried over: the modal-loop rule is load-bearing. Our own `DoDragDrop` runs
from a posted message outside winit's handler, so `invoke_from_event_loop` closures run inside our
own drag loop; every such closure takes `with_state`, the drag code holds no borrow while the loop
runs, and no closure may itself open a modal loop. P7-M1 moves the shell verbs' modal loops to the
verb worker, which touches no app state; the classic menu's `TrackPopupMenuEx` stays on the UI
thread under the rule.

Amendment (P2-M5), carried over: rows carry no image; icons reach the screen only through each
pane's strip and the grid's composed rows, and every per-key cost is paid on a worker, once per key
per process. The P7-M2 menu's icons are not rows: they are fetched per menu on the verb worker or
the icon workers, never on the UI thread.

Amendment (P3-M1 and P3-M5), carried over: nothing in the row or the tile may depend on a runtime
property. The row's structure is chosen once per pane, outside the `for`, and proven by diffing the
generated code against the reference's. P7-M4's two-line names are a change to the tile components
themselves, the same in every tile.

Amendment (P5), carried over: a record is applied to the model only when the listing is settled
and the name is accounted for; anything else is the whole-folder refresh, logged with its reason.
P7-M4's background tabs follow it.

Amendment (P6), carried over: a command has one path. The menu's routed verbs (P7-M1), the new
menu's items (P7-M2), the toolbar's buttons and the keys call the same functions.

Each milestone runs as in Phase 6: settle the brief against the code first and put every clash to
Jeremy with a recommendation before any code, implementer brief, lead review, gates, push, PR, a
gate build from a detached checkout with `--locked`, the wrapper script, console checks after the
triplet, merge on Jeremy's word from the main checkout (never `gh pr merge --delete-branch` from a
session's own worktree), `target\release` rebuilt from main and said so.

## Calls made in this kickoff, for Jeremy to check

Decisions Jeremy did not make and the lead settled, each reversible before its milestone starts:

- The decided item and the verbs through jobs are one milestone (P7-M1), since the routing is
  decided on the UI thread right before a verb goes to the worker; the Phase 6 close sized them at
  one and a half milestones together.
- The routed verbs are exactly `delete`, `cut`, `copy` and `rename`, matched by canonical verb
  string; a handler's verb with another name goes to the shell (P7-M1).
- Escape clears a cut's marks before it clears the selection, after stopping a search or
  cancelling queued jobs; the clipboard question is checked against Explorer first (P7-M1).
- The Windows 11 menu's icon row holds Cut, Copy, Rename, Share and Delete, as Explorer's; Share is
  the shell's own verb and runs on the worker (P7-M2).
- If the packaged apps' items cannot be found reliably, the new menu ships without them and
  "Show more options" stays the way to everything else; that would be put to Jeremy first (P7-M2).
- Zip keeps TwinFM's own listing (it is fast on 40,000 entries); only the other formats list
  through the shell's `ArchiveFolder` (P7-M3).
- A write into an archive is not undoable unless the check shows the shell reports what it wrote
  (P7-M3).
- Tooltips are TwinFM's own small popup, since Slint has none (P7-M4).
- Two-line tile names apply to the large and medium tiles both (P7-M4).
- P7-M4's drag cursor stays as it is unless the console check shows the doubled glyph.

## P7-M1: shell verbs off the UI thread, and the menu's verbs through TwinFM's jobs

Branch `feat/p7-m1-verb-worker`, from 5a05fda (the kickoff).

### Settled first (2026-10-18, before any code)

Three points of the brief clashed with the code; each went to Jeremy with a recommendation.

- **Escape over a cut and the clipboard.** Recommended: Escape cancels the cut completely, the
  marks and the clipboard (emptied only while it still holds TwinFM's cut), so a later paste cannot
  move the files. Jeremy: "Also empty it".
- **Paste on the background menu.** The kickoff routed four verbs; the background menu's Paste is
  the shell's own operation too (no undo, no progress). Recommended routing it to Ctrl+V's path.
  Jeremy: "Yes, route Paste".
- **The routed verbs inside an archive.** The menu's Delete inside a zip ran the shell's own delete
  (which works); TwinFM's commands refuse inside archives until P7-M3. Recommended leaving the
  shell's verbs there until then. Jeremy chose the other option, "Route and refuse": inside an
  archive the menu's Delete, Cut and Rename are refused with the keys' note until P7-M3.

### What landed

- **The verb worker** (`src/verbs.rs`): one STA thread named `verbs` with its own message loop,
  started beside the file operation worker. A job carries the folder, the item names, the canonical
  verb, the command's offset and label in the menu that was shown, the invoke point and the
  extended flag. The worker builds the menu again (`shellmenu::run_verb`: `parse_items`,
  `GetUIObjectOf` or `CreateViewObject`, `QueryContextMenu` into a menu of its own with the same id
  range) and invokes there; every COM object of a job is made, used and released on the worker.
  Results come back through `invoke_from_event_loop` (`verb_done`: a note on failure or refusal,
  then the panes refreshed, as the UI thread did before).
- **Invoke by verb, else by a matching label** (`shellmenu::choose`, unit-tested): a command with a
  canonical verb is invoked by it; one without (the "Send to" targets) by its id, only when the
  rebuilt menu's item at that id has the same label, after the rebuilt menu's dynamic submenus are
  filled (`WM_INITMENUPOPUP` to the handler, two levels; not inside an archive, whose menus crash on
  `IContextMenu2/3`); otherwise refused and logged. A handler that names its verb but fails an
  invoke by it gets one retry by id under the same label rule.
- **The routing** (`shellmenu::route`, unit-tested), decided on the UI thread from the menu shown:
  `delete`, `cut`, `copy`, `paste` and `rename` run `main::command`'s `delete` (Shift held:
  `sdelete`), `cx`, `cc`, `cv` and `rename`, the functions the keys and the toolbar call; every
  other verb goes to the worker. `context_menu` and the driver share `menu_request` and
  `menu_picked`.
- **Escape cancels a cut** (`cancel_cut`): after the band, the search and the queued jobs, before
  the selection; the marks go in every tab and the clipboard is emptied while its sequence number
  is still TwinFM's last write.
- **No owner window** for the worker's invokes: a dialog owned by the main window from another
  thread attaches the two threads' input queues while it is up. A dialog a verb shows is unowned,
  as Explorer's verbs' are to another program. `CMIC_MASK_ASYNCOK` is not asked for, so the shell
  holds no reference that needs the worker past the invoke; at exit a running verb is not waited
  for (logged).
- **Driver**: `menuverb <pane> <verb>` (a canonical verb as if picked for the cursor row's
  selection; `sleep:<ms>` the test verb that sleeps on the worker); `waitops` waits for the verb
  worker as well (`verbs::busy`).
- **Tests**: 190 to 194 (`choose`, `route`, `verbs::tests::worker_runs_a_real_verb_and_the_test_verb`,
  which runs a real "Create shortcut" through the worker in a scratch folder, and
  `grid_rows_keep_their_tile_model_across_changes`, below).

### Decisions

- No owner window, recorded above; the console check on `G:` says whether its dialogs come up in
  front.
- The test verb is not behind the `TWINFM_SOAK` fence: it touches no file, and only the driver can
  name it.
- The menu's `copy` is routed, so the scripted real verb end to end is `link` (Create shortcut),
  which makes a file and shows no dialog in a writable folder.

### Scripted checks

A scratch folder in `%TEMP%\twinfm-m5\p7m1` with three files, the right pane on System32, run
with `TWINFM_SOAK=1`:

```
[menu] verb "sleep:2500" ("sleep:2500") queued on the verb worker, id 1
[menu] verb worker: test verb sleeps 2500 ms (id 1)
[perf] frames_total=51 render[n=51 p50=2.48ms ...] interval[n=44 p50=16.91ms ...]
[menu] InvokeCommand verb="sleep:2500" offset=0 label="sleep:2500" slept in 2501 ms (verb worker, id 1)
[menu] InvokeCommand verb="link" offset=0 label="link" by verb = 0x00000000 in 768 ms (verb worker, id 2)
[fileops] pane=0 note: Cut 1 item to the clipboard
[clipboard] pane=0 Escape: cut cancelled, marks cleared, clipboard emptied
[select] pane=0 Escape: selection cleared
[fileops] submit id=1 pane=0 ... Delete { sources: [...\p7m1\b.txt], permanent: false }
[fileops] done id=2 pane=0 tab=1 Done: Undid: Deleted 1 item to the Recycle Bin
[fileops] submit id=3 pane=0 ... Copy { sources: [...\p7m1\a - Shortcut.lnk], ... }
[fileops] done id=4 pane=0 tab=1 Done: Undid: Copied 1 item
```

The list kept rendering through the test verb's 2.5 s (51 frames while the right list scrolled),
Create shortcut ran on the worker by its verb, the routed Cut was cancelled by the first Escape and
the second cleared the selection, and the routed Delete, Copy and Paste went through the jobs with
their undo records. An earlier run that stayed on `C:\` by mistake (a byte-order mark in the script
file hid its first `nav`) showed two things worth keeping: the soak fence refused the routed Delete
there, as it refuses Delete's key, and Create shortcut in `C:\` returned 0x80070005 after 9.9 s on
the worker while the window kept rendering.

`tools/soak-p5m3.ps1 -Minutes 8` (with P7-M1's verb cycle, see `CLAUDE.md` § Measuring): exit code
0, no panics, 7 cycles started, 262 jobs done (0 failed, 0 cancelled, 0 soak refusals, 0 waitops
timeouts); verbs: 12 jobs queued (two per cycle), 6 slept, 6 shortcuts made by verb with S_OK, 6
routed deletes and 6 restores, 6 cuts cancelled by Escape (the seventh cycle stopped at the time
limit before its verb steps); every earlier count as in P6-M3's soak (toolbar presses 50, none
refused; drags dropped 0; spring-loaded tabs 0); working set 113.4 MB at 5 s, 231.9 at most, 211.1
at 480 s (P6-M3's soak: 237.3 at most, 212 at the end).

### Gate

Tag ab25, 2026-10-18, `C:\traces\gate-p7m1.ps1` elevated at the console, Narrator off: the defaults
triplet, `target-p6m3` (c632e93) against `target-p7m1` (755eeb2), interleaved.

| Run | WPR % | In-app % | Render p50 ms |
|---|---|---|---|
| p6m3-1 | 19.6 | 22.3 | 2.35 |
| p7m1-1 | 29.1 | 29.7 | 3.87 |
| p6m3-2 | 28.4 | 28.4 | 4.06 |
| p7m1-2 | 28.9 | 28.1 | 3.91 |
| p6m3-3 | 21.1 | 22.7 | 2.57 |
| p7m1-3 | 20.9 | 23.4 | 2.47 |

Medians 21.1 (reference) and 28.9 (candidate); the wrapper's rule read "a gap that counts". The
session changed state part-way, as in ab23: three runs at a render p50 of 3.9 to 4.1 ms, three at
2.35 to 2.57. Like for like, the candidate sat within a point of the reference in both states
(low: 20.9 against 19.6 and 21.1; high: 29.1 and 28.9 against 28.4); the candidate's median came
from two of its runs falling in the slow state. The scenario opens no menu, so the verb worker
idles in `GetMessageW`. Jeremy accepted the gate as read like for like (2026-10-18, "I accept the
ab25 gate").

### Console checks

Jeremy, unelevated, `target-p7m1` (2026-10-18): Create shortcut on `G:`, Delete from the menu and
Ctrl+Z, Cut from the menu and Escape, Properties and Open with: all as expected, and the shell's
message box for Create shortcut on `G:` opened in front of TwinFM (no owner window). One finding:
in the large and medium icon views a folder took three clicks to open (one selected it, then a
double-click opened it); the details list was fine.

### After the console check: a double-click in the grid

**Cause.** `GridModel::row_data` built a new `VecModel` of tiles on every call. A press that selects
a tile notifies its grid row (`grid_notify.row_changed`), Slint asks for the row again, gets a
different tile model and builds the row's tiles anew; the new tile under the pointer had not seen
the first click, so the second click was a first click again. Older than P7-M1 (P4-M1's grid);
reproduced with real mouse messages (`WM_LBUTTONDOWN`/`UP` twice, 150 ms apart, posted to the
window) on an unselected tile in `C:\Windows`: two `[select]` lines and no navigation.

**Fix.** `GridModel` keeps one tile model per grid row and `row_data` updates it in place (tiles
that changed are set, the length follows a width change), so Slint keeps the row's tiles and only
re-renders what changed (`model::tests::grid_rows_keep_their_tile_model_across_changes`, written
first, failing). The same posted double-click then opened the folder at once, in the large grid on
`C:\Windows` and the medium grid on `G:\My Drive`. The tile components are unchanged; the grid's
update path changed, so the gate re-runs with the grid and medium triplets (ab26).
`tools/soak-p5m3.ps1 -Minutes 8` on the fix: exit code 0, no panics, 7 cycles, 267 jobs (0 failed);
the grid counts as before (14 bands done with 2 selected, 14 with the cursor on tile 0, 7
Shift+Right ranges, 14 tile drops, drags dropped 0); verbs 14 queued, 7 slept, 7 shortcuts, 7
routed deletes and restores, 7 cuts cancelled; working set 113.9 MB at 5 s, 217.8 at most and at
the end.

**Gate ab26** (2026-10-18, the wrapper's second version: 3f8e786 against `target-p6m3`, the
defaults, grid and medium triplets interleaved; one session state throughout, render p50 1.11 to
2.49 ms by view and 2.27 to 2.49 for every details run):

| Binary | WPR runs % | Median % | Render p50 ms |
|---|---|---|---|
| `target-p6m3` | 20.0, 18.4, 18.8 | 18.8 | 2.27 to 2.36 |
| `target-p7m1` defaults | 18.2, 18.6, 20.1 | 18.6 | 2.29 to 2.49 |
| `target-p7m1` grid (`C:\traces\grid.toml`) | 10.9, 10.8, 11.3 | 10.9 | 1.11 to 1.21 |
| `target-p7m1` medium (`C:\traces\grid-medium.toml`) | 14.2, 14.0, 13.8 | 14.0 | 1.63 to 1.65 |

No gap counts: the defaults runs interleave with the reference's (median 18.6 against 18.8), and
the grid and medium triplets sit far below it with every run under 20%, as in ab23 (10.3 and 13.5
in that session). One defaults run read 20.1 against the reference's own 20.0, so the bar is read
against the reference's spread, as the protocol says. Each settings run's stderr showed its toml
read and its view. Console check after the triplet (Jeremy, unelevated, `target-p7m1` at 3f8e786):
a double-click on an unselected folder opens it in the large and the medium icon views.

## Open questions

Written 2026-10-18 at the kickoff.

- **How the packaged apps' context menu items are found** (P7-M2): the first thing the milestone
  settles.
- **Which archive formats Windows' handler writes into** (P7-M3): the first thing the milestone
  settles; Jeremy asked for all, as far as Windows allows.
- **Carried from Phase 6**: Google Drive's volume found by its name; a sync-root scratch folder for
  the soak (a console check only, Jeremy's call); the Slint whole-row snap patch and upstreaming
  (Jeremy's); the dead share console check owed from P5-M1.
