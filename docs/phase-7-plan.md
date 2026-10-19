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

P7-M1 merged 2026-10-18 as a6391c8 (#63); `target\release` rebuilt from it. The reference for
P7-M2 is `target-p7m1` (3f8e786); `target-p6m2` deleted.

## P7-M2: the Windows 11 style context menu

Branch `feat/p7-m2-win11-menu`, from a6391c8.

### Settled first (2026-10-18, before any code)

Checked against the machine and the code first:

- **The packaged apps' items can be found.** Each package declares them in its manifest
  (`AppxManifest.xml`, `desktop4:Extension Category="windows.fileExplorerContextMenus"`): an
  `ItemType` (`*`, an extension, `Directory`, `Directory\Background`) and per verb a `Clsid`, the
  packaged COM class that implements `IExplorerCommand`. On this machine 11 packages declare them
  (Windows Terminal, Notepad, Paint, Photos, OneDrive, three PowerToys, Python's manager, Nearby
  Share, AMD's). TwinFM reads the manifests once at start on a worker (the installed packages and
  their paths from the package registry) and creates each class with `CoCreateInstance` when a menu
  opens, as Explorer's new menu does; a package change re-reads them.
- **Slint draws TwinFM's popups inside the main window.** With the breadcrumb dropdown open
  TwinFM still has one top-level window (`EnumWindows`: the main window, winit's event target, the
  pseudo-console), so this Slint build puts a `PopupWindow` in the window (`ChildWindow`), clipped
  to its edges.

Then put to Jeremy with a recommendation, his answers:

- **Drawing**: inside the window, as the bookmark menu and the breadcrumb dropdown are; near an
  edge the menu opens upward or leftward to fit, and in a small window a long menu scrolls.
  Jeremy: "Inside the window" (the recommendation; a borderless window of its own, which could
  pass the edge as Explorer's does, was the alternative).
- **Items**: mirror Explorer's Windows 11 list for the item (the icon row Cut, Copy, Rename,
  Share, Delete; Open; Open with; the packaged apps' items; Compress to; Copy as path; Properties),
  with TwinFM's own three (Open in other pane, Open in terminal, Copy path) after Open with, and
  "Show more options" (Shift+F10) for the rest. Jeremy: "Mirror Explorer", with this layout:

  ```
  [Cut] [Copy] [Rename] [Share] [Delete]
  ----------------------------------
  Open                     Enter
  Open with              >
  Open in other pane
  Open in terminal
  ----------------------------------
  Open in Terminal  (app)
  Edit in Notepad   (app)
  Compress to...         >
  Copy path
  Properties         Alt+Enter
  ----------------------------------
  Show more options  Shift+F10
  ```

- **The classic menu directly**: Shift+right-click (and Shift+F10 inside the new menu) opens
  today's full classic menu at once, with the extended verbs as Shift gives them today, as in
  Explorer. Jeremy: "Shift+right-click".

Checked further against the machine and the code before any code (2026-10-18, the implementer):

- **The packaged handlers answer an ordinary process.** All 17 classes the 11 manifests name are
  created by `CoCreateInstance` from an unpackaged STA (a PowerShell probe), and `GetState`,
  `GetTitle`, `GetIcon` and `GetFlags` answer for an `IShellItemArray` of a `.txt`, a `.png` and a
  folder: the first creation of a class takes 48 to 74 ms (its DLL loading), later ones 4 to 7 ms.
  The handlers do not filter by type themselves (IDLE's "Edit in IDLE", declared for `.py` and
  `.pyw`, says enabled for a `.txt`), so the manifest's `ItemType` decides which classes are asked,
  and `GetState` (enabled 0, disabled 1, hidden 2) decides the rest. Titles carry `&` access keys
  ("Open in &Terminal"); icons are `path,-id` or a plain `.ico` or `.exe` path (one with a `/` in
  it, Notepad's). IDLE's command has subcommands (`ECF_HASSUBCOMMANDS`). The classic menu already
  carries the same commands (Edit in Notepad, Open in Terminal, Rename with PowerRename, Send with
  Quick Share), so "Show more options" keeps them either way.
- **The installed packages** come from the packaged COM catalog, `HKCR\PackagedCom\Package` (93
  packages here, exactly the installed ones that register classes; the AppModel repository's 402
  entries include superseded versions), each package's folder from `GetPackagePathByFullName`;
  a package change shows as a new last-write time on that key, checked when a menu opens.
- **Explorer's "Compress to" is not reachable**: it is built into Explorer (nothing in the
  registry's `shell` or `shellex` keys names it). The classic menu offers "Send to > Compressed
  (zipped) folder", and `tar.exe` here is bsdtar 3.8.9 (libarchive, which writes zip, 7z and tar).
- **Share** has no canonical verb TwinFM can invoke by name ("Windows.ModernShare" by verb returns
  0x80070483 on the verb worker), so the worker finds the classic menu's item by its label.
- **Alt+Enter** was TwinFM's "size every folder" (P3-M3), and the layout shows it for Properties.

Then put to Jeremy with a recommendation, his answers:

- **Compress to**: recommended "Compress to > ZIP file" through the shell's own "Send to >
  Compressed (zipped) folder" on the verb worker, 7z and TAR in P7-M3 (the archive writing
  milestone). Jeremy: "ZIP now, 7z/TAR in P7-M3".
- **Alt+Enter**: recommended keeping folder sizes and showing Properties with no key. Jeremy chose
  the other option, "Properties, like Explorer"; the sizes move to **Alt+Shift+Enter** (Total
  Commander's key for the same thing; recommended, Jeremy: "Alt+Shift+Enter").
- **Open with >**: recommended the apps Windows recommends for the type, with their icons, read on
  the verb worker, then "Choose another app". Jeremy: "Apps list, like Explorer".

### What landed

- **The menu** (`src/winmenu.rs`, pure and unit-tested; app.slint `wm-popup`): a Slint
  `PopupWindow` covering the window (`no-auto-close`; a press outside the menu reaches its
  background and closes it), with the menu and one submenu drawn at the places Rust computes. The
  rows for a selection (`winmenu::tools`, `winmenu::items`): the icon row (Cut, Copy, Rename, Share,
  Delete, each only where it applies, by the toolbar's rule; Share for files only; Paste on a
  background with files on the clipboard), Open (one item), Open with > (one file), Open in other
  pane (one folder or drive), Open in terminal, then the packaged commands, Compress to > (folders
  and files), New folder and New file (a folder's background), Copy path, Properties, and Show
  more options. Inside an archive: Open, Open in other pane (a folder), Copy path and Show more
  options; on This PC's background: Properties and Show more options. Placement
  (`winmenu::place`, `place_sub`): right of and below the pointer, leftward or upward when it would
  pass the window's edge, pinned inside a 4 px margin when neither fits, cut to the window with
  its rows scrolling; a submenu right of the menu or else left of it, level with its item.
- **Keyboard**: Down and Up move the highlight over enabled items, skipping separators and wrapping
  (`winmenu::next_hl`), Home and End, Right opens the highlighted item's submenu and moves into it,
  Left and Escape leave it, Escape closes the menu, Return and Space pick, Shift+F10 opens the
  classic menu; other keys stay in the menu. The pointer's hover moves the highlight and opens a
  submenu at once. No access keys (a call; the console check beside Explorer says whether they are
  missed).
- **One path per command**: each item calls the command its key, toolbar button or P7-M1 route
  calls (`winmenu::Cmd::key_command`; the icon row's Delete with Shift held is Shift+Delete), on the
  next turn of the event loop after the popup closed (closing it gives the keys back to the list,
  which would otherwise take them from the name editor New folder or Rename opens). Properties is
  `command "properties"`, also Alt+Enter's: the shell's `properties` verb on the verb worker for the
  cursor row's selection or the folder. Share and Compress to > ZIP file go to the verb worker as
  jobs that find the classic menu's item by its label path (`VerbRequest::find`,
  `shellmenu::find_command`, unit-tested): "Share", and "Send to > Compressed (zipped) folder";
  "Choose another app" is the `openas` verb. Show more options opens the classic menu for the same
  row at the same point one frame later (so the window is drawn without the new menu before the
  classic menu's modal loop starts); Shift+right-click opens it at once with the extended verbs.
- **The packaged commands and the Open with apps** (`src/explorercmd.rs`), on the verb worker: the
  catalog from the manifests (`roxmltree`, already in the lockfile through Slint; the namespace
  prefixes ignored), read at the first menu and again when `HKCR\PackagedCom\Package` changes; for
  a menu, each class whose item types cover every target is created and asked for its state, title,
  icon and subcommands with an `IShellItemArray` of the targets, and the shell's recommended
  handlers for a single file's extension (`SHAssocEnumHandlers`) for their names and icons. The
  objects are held on the worker until the next menu and invoked from there (`IExplorerCommand::
  Invoke`, `IAssocHandler::Invoke` with the items' data object), and released before the worker
  leaves its apartment. Icons (`icons::location_pixels`): `path,-id` and plain `.ico`/`.exe` paths
  through `SHDefExtractIconW`, a packaged app's indirect string (`@{...}`, Notepad's) through
  `SHLoadIndirectString` and the shell's image factory. The menu shows when the worker answers or
  after 250 ms, and an answer after that rebuilds it.
- **Alt+Enter is Properties; folder sizes moved to Alt+Shift+Enter** (Jeremy's decision above), in
  the list's key handler (`pane.slint`), the driver (`sizeall` presses `AltShiftReturn`) and the
  `[size]` line.
- **Found on the way, fixed**: on This PC a drive is named by its root (`C:\`), and the search
  results' path split (`shellmenu::common_parent`, P3-M2) cut it at the backslash, so every menu on a
  drive failed with `parsing "" failed: 0x80070057`, the classic one on main included. A name ending
  in `\` is now kept whole (test added).
- **Found on the way, recorded**: the soak's toolbar cycle pastes within a millisecond of its copy,
  and two P7-M2 soak runs failed there (`clipboard: the clipboard is in use by another program`,
  then the rows shifted and later steps deleted the soak's own source tree, 74 failed jobs).
  `GetOpenClipboardWindow` at the failure named a `CLIPBRDWNDCLASS` window of `RadeonSoftware.exe`
  (AMD's driver software, a clipboard listener that opens the clipboard after each change), not
  TwinFM; the same failure came with the classic menu alone, so P7-M1's passing soaks were the
  race won. The soak now pauses 200 ms between a copy or cut and its paste. TwinFM's paste does not
  retry; a person never pastes that fast, so it stays as it is.
- **Driver**: `rclick`, `rclickempty` and the keys open the new menu; `waitmenu <ms>`; `menupick`,
  `menuopen`, `menukey` and `menuclose` act on it as ordinary steps (`main::script_winmenu`; a pick
  of anything that runs on the verb worker is refused, except Compress to > ZIP file inside the soak
  fence); `menuopen Show more options`, `menukey ShiftF10` and `rclick ... shift` open the classic
  menu with the in-menu steps after them, as before. `CLAUDE.md` § Measuring has the lines.
- **Tests**: 194 to 210 (`winmenu`: the item lists for a file, a folder, a drive, several items,
  an archive, search results and backgrounds, the packaged commands' submenus and states, ids,
  heights, placement, submenu placement, the highlight, scrolling, the log line; `explorercmd`: the
  manifest's commands, the item types, GUIDs and titles; `shellmenu::find_command`; the icon
  locations; This PC's drives in `common_parent`).

### Decisions

- **Open** and **Open with** show for one item only (Explorer opens several at once; TwinFM's
  Return opens the cursor row).
- **Properties is left out inside an archive** (the zip folder's menu is reached through Show more
  options) and **Open in terminal** too (an archive's path is no folder for a terminal).
- **Nothing is asked of the packaged apps inside an archive or on This PC's background**; on a drive
  the `Directory` type covers it (Open in Terminal shows there, as in Explorer).
- **The wait**: 250 ms before the menu shows without the worker's answer. Once the handlers'
  DLLs are loaded an answer takes 40 to 90 ms here; the first menu of a run takes 200 to 280 ms
  (the classes' first creation and the Open with apps' icons), so it may show and then be rebuilt.
- **The menu covers the window**: while it is open a click anywhere else only closes it (Explorer
  also starts a new menu on a right click elsewhere; not copied).
- **The driver's Show more options opens the classic menu at once** (inside the step, so its
  in-menu steps can be armed); a person's pick waits one frame first. The rest of the path is the
  same function.

### Scripted checks

`%TEMP%\twinfm-m5\p7m2` (folder `d`, `a.txt`, `b.txt`), the right pane on System32, `TWINFM_SOAK=1`,
the release build of this branch:

```
[winmenu] catalog: 11 of 93 packages declare context menu commands, 17 classes, read in 15 ms
[winmenu] query id=1: 5 of 11 commands shown (6 hidden), 5 open-with app(s), in 277 ms (verb worker)
[winmenu] pane=0 seq=1 shown after 315 ms (rebuilt with the worker's answer): [Cut Copy Rename Share Delete] Open | Open with > | Open in terminal | --- | Move to OneDrive | Unlock with File Locksmith | Rename with PowerRename | Edit in Notepad | Send with Quick Share | Compress to > | Copy path | Properties | --- | Show more options
[winmenu] submenu "Open with" at (444, 215): [] Notepad | The main calibre program | The calibre e-book viewer | Antigravity | Antigravity IDE | --- | Choose another app
[winmenu] pane=0 seq=2 shown after 51 ms (the worker answered): [Cut Copy Rename Delete] Open | Open in other pane | Open in terminal | --- | Move to OneDrive | Unlock with File Locksmith | Rename with PowerRename | Open in Terminal | Send with Quick Share | Compress to > | Copy path | Properties | --- | Show more options
[winmenu] placed at (692, 145) 280 x 326 for (692, 471) in a 1200 x 800 window: rightward, upward
[menu] InvokeCommand verb="" offset=0 label="Send to>Compressed (zipped) folder" by offset = 0x00000000 in 600 ms (verb worker, id 3)
[fileops] done id=1 pane=0 tab=1 Done: Created New folder
[fileops] done id=3 pane=0 tab=1 Done: Undid: Deleted 1 item to the Recycle Bin
[clipboard] pane=0 Escape: cut cancelled, marks cleared, clipboard emptied
[winmenu] pane=0 seq=7 shown after 0 ms (no query): [] Open | --- | Copy path | --- | Show more options
[winmenu] pane=0 seq=9 shown after 74 ms (the worker answered): [] Open | Open in other pane | Open in terminal | --- | Unlock with File Locksmith | Rename with PowerRename | Open in Terminal | Send with Quick Share | Copy path | Properties | --- | Show more options
[winmenu] pane=0 seq=10 shown after 0 ms (no query): [] Properties | --- | Show more options
```

The first menu of the run showed after 250 ms without the worker's answer and was rebuilt with it
at 315 ms (the classes' first creation and the Open with icons); later menus showed when the
worker answered, after 40 to 150 ms. A file, a folder (Open in Terminal; Notepad's handler says
hidden), the background (upward, near the window's bottom), inside a zip (seq 7, no query), a drive
on This PC (seq 9) and its background (seq 10), and a search result each got the layout the
tests describe; Copy path, Compress to > ZIP file (`a.zip` beside `a.txt`), New folder from the
background, Delete and Ctrl+Z, Cut and Escape, Shift+F10 inside the menu (the classic menu) and
Shift+right-click (the classic menu with the extended verbs) ran as their keys do. A screenshot
with the software renderer (`SLINT_BACKEND=winit-software`; a capture of the femtovg window comes out
blank) showed the menu as Explorer draws its own: rounded, shadowed, the icon row, the apps' own
icons (Notepad's from its package after the indirect string was resolved), the keys right-aligned.

The generated list and grid components (`FileList`, `FileListNoType`, `FileList32`,
`FileListNoType32`, `FileGridList`, `FileGridMediumList`) are the reference's (`target-p7m1`), line
for line once Slint's numbering of sub-components and globals is set aside; `FileListHost` differs
in its key handler only (Alt+Return: `properties`, or `sizeall` with Shift).

`tools/soak-p5m3.ps1 -Minutes 8` (with the P7-M2 cycle and the clipboard pause): exit code 0, no
panics, 6 cycles started, 262 jobs done (0 failed, 0 cancelled, 0 soak refusals, 0 waitops
timeouts); Windows 11 menu: shown 42 (seven per cycle), rebuilt 0, picks run 24, ZIPs made 6, new
folders 12, submenus 12, classic menus from Show more options 6, menupicks refused 0; verbs 18
queued (three per cycle), 6 slept, 6 shortcuts, 6 routed deletes, 12 restores, 12 cuts cancelled;
the earlier counts as in P7-M1's soak (toolbar presses 48, none refused; drags dropped 0;
spring-loaded tabs 0); working set 115.9 MB at 5 s, 209.6 at most, 182.1 at 480 s (P7-M1's: 217.8
at most).

### Gate

Tag ab27, 2026-10-19, `C:\traces\gate-p7m2.ps1` elevated at the console, Narrator off: the defaults
triplet, `target-p7m1` (3f8e786) against `target-p7m2` (f66ce33), interleaved.

| Run | WPR % | In-app % | Render p50 ms |
|---|---|---|---|
| p7m1-1 | 18.5 | 18.4 | 2.29 |
| p7m2-1 | 19.1 | 18.1 | 2.29 |
| p7m1-2 | 19.1 | 21.4 | 2.33 |
| p7m2-2 | 18.8 | 20.0 | 2.33 |
| p7m1-3 | 18.4 | 17.7 | 2.30 |
| p7m2-3 | 18.8 | 19.5 | 2.33 |

Medians 18.5 (reference) and 18.8 (candidate): no gap counts (the reference's 19.1 sits above two
candidate runs, and the medians are 0.3 apart), every candidate run under 20%. One session state
throughout (render p50 2.29 to 2.33 ms in every run). The scenario opens no menu, as expected flat.

## P7-M3: archives beyond zip, browse and write

Branch `feat/p7-m3-archives`, from 193be73.

### Settled first (2026-10-19, before any code)

Checked against the machine first, with throwaway probes (an ignored test that drove the file
operation worker, the shell folders' interfaces and `Shell.Application` on small archives of every
format made by `C:\Windows\System32\tar.exe`, bsdtar 3.8.9; Git Bash's own `tar` is GNU tar and
writes no 7z):

- **Which formats `ArchiveFolder` takes.** Its registration names `.7z`, `.bz2`, `.cpio`, `.gz`,
  `.mtree`, `.nupkg`, `.rar`, `.tar`, `.tbz2`, `.tgz`, `.txz`, `.tzst`, `.uu`, `.xar`, `.xz` and
  `.zst` (a `.tar.gz` is a `.gz`); `.zip` is `CompressedFolder`'s and `.cab` `CABFolder`'s (not in
  scope). Every format lists through `IShellFolder::EnumObjects` (`BindToHandler(BHID_EnumItems)`
  on the archive's shell item, 6 to 23 ms for a two-entry archive), a lone `.gz`, `.xz` or `.bz2`
  as one file named without the extension, and extracts through an `IFileOperation` copy of its
  items (each copied item reports twice to the progress sink, first with a success code
  0x00270011 and no destination, then with it).
- **`ArchiveFolder` refuses every write**, for every format: a copy or move into it fails with
  0x8007010B (`ERROR_DIRECTORY`, "The directory name is invalid"), new folder, rename and delete
  inside with 0x80004001 (`E_NOTIMPL`); its folder has no drop target (`CreateViewObject
  (IDropTarget)` 0x80004002) and no `IStorage`, and `SetNameOf` returns `E_NOTIMPL`. Its items'
  verbs are Open and Copy only.
- **The zip folder writes only through its drop target.** `IFileOperation` fails a copy or move
  into a zip (0x8007010B), a new folder (0x80004005) and a rename (0x80070011) inside it, and a
  copy within one zip (0x80070002); a delete inside works. Its drop target (`CreateViewObject
  (IDropTarget)`) copies and moves into the zip's root and its folders: `Drop` returns at once and
  the zip folder writes on a thread of its own (a temporary `x.zip~RF....TMP` beside the zip, then
  replaced), holding the process reference (`SHSetInstanceExplorer`) from the drop to the end of
  the write (100 MB in 4.5 s here); it never calls `IDataObjectAsyncCapability::EndOperation`, and
  a move leaves the source on disk. `SetNameOf` and `IStorage::CreateStorage` fail (0x80004001,
  0x80004005); its items' verbs are Open, Cut, Copy, Delete and Properties, no Rename. A move out
  of a zip (`IFileOperation`) extracts files and folders but leaves the folders inside the zip.
- **Undo**: the zip folder's drop reports nothing about what it wrote, and a zip has no Recycle Bin.
- **Compress to > 7z / TAR**: Explorer's own "Compress to" is not reachable (P7-M2). Windows'
  `tar.exe` writes both, but crashes (0xC0000005, an empty output file) on any name outside the
  system code page (1252 here: "café.txt" works, "日本語.txt" crashes), in every format it writes.
- **The foreign synchronous zip drop.** A shell folder's data object carries the items' PIDLs
  (`Shell IDList Array`) beside the virtual files; read in `Drop`, they name the items by their
  paths inside the archive, which the extraction job takes on the file operation worker.

Then put to Jeremy with a recommendation, his answers:

- **The formats other than zip**: recommended read-only, as Windows' handler allows (browse, copy
  out; every write refused with a note saying Windows cannot write into that format); the
  alternative was TwinFM unpacking and repacking the whole archive with `tar.exe` on every write.
  Jeremy: "Read-only".
- **How zips are written**: recommended through Windows' zip folder (copy and move in through its
  drop target, delete through the job; rename, new folder and move out refused as the zip folder
  refuses them). Jeremy chose the other option, **TwinFM's own zip writer** (the `zip` crate that
  already reads the listings), so rename, new folder and move out work too, with TwinFM's own
  percentage.
- **Undo of a write into a zip**: recommended "cannot be undone" (as the kickoff planned; Explorer
  cannot either). Jeremy: "Cannot be undone".
- **Compress to > 7z / TAR**: recommended `tar.exe`, refusing names it would crash on. Jeremy chose
  the other option, **TwinFM's own writers** (two new libraries, one for TAR and one for 7z), so
  every name works.

### What landed

- **Other formats open as folders** (`src/fs/archive.rs`, `src/fs/archivefolder.rs`): a file whose
  extension `ArchiveFolder` owns on this machine opens as a zip does. The extensions are the sixteen
  its registration names, each kept only while `HKCR\.ext` still names `ArchiveFolder` (read once,
  1.1 ms), so an archive another program owns (7-Zip's `.7z`) keeps opening in that program. `split`
  asks the file system only about components carrying one of those extensions or `.zip`
  (`split_with`, tested by counting the queries). Their levels are listed by the shell on the
  `archive` STA thread (the shell item, `BindToHandler(BHID_EnumItems)`, each item's name,
  attributes, size and date through `IShellItem2`; every COM object made and released there), one
  level at a time as it is asked for, and cached beside the zips (eight archives, keyed by path, size
  and modified time); the `[enum]` line names the handler. The crumb dropdown and the recursive
  search inside them go through the same levels. Zip keeps its own central-directory listing.
- **Read-only formats are refused once** (`fileops::archive::classify`, unit-tested): a job's
  archive is decided from the job itself, before it is queued (`submit`, with the note "Windows'
  archive handler cannot write into .7z archives") and again on the worker; the keys, the toolbar,
  paste and drops into such a level refuse early with the same words. Nothing goes from one archive
  into another (copy them out first). A copy out of any format is an ordinary extraction job.
- **TwinFM writes zips** (`src/fs/archivewrite.rs`, Jeremy's choice): copy and move into a zip
  (paste, drops, the other pane), new folder and new file, rename (a folder with everything below
  it), delete (permanent, asked first: a zip has no Recycle Bin), and cut or move out of it (the shell
  extracts, then TwinFM removes what came out). Each write rewrites the archive into a temporary file
  beside it (existing entries copied raw, so encrypted entries and other zippers' extra fields stay as
  they were; new files deflated) and swaps it in with `ReplaceFileW`; a failure removes the temporary
  file and leaves the zip as it was (tested). Files that would replace files in the zip are asked
  about once per job (Yes replace, No skip, Cancel). The jobs run on the file operation worker with
  the status bar's percentage, drop the archive's cached levels, and the panes are listed again. A
  zip level's toolbar and Windows 11 menu offer what a folder's do, without Open with, the terminal,
  Properties or the packaged apps' commands.
- **No write into a zip is undone** (Jeremy): its record says "Cannot undo: changes to a zip
  archive cannot be undone".
- **Compress to > 7z file and TAR file** (Jeremy's choice of TwinFM's own writers): a
  `Job::Compress` on the file operation worker, the archive beside the first item and named after it
  as Explorer names it (`report.7z`, `photos (2).tar`), through a temporary file; 7z by
  `sevenz-rust2` (LZMA2), TAR by `tar` with a pax `path` record for a non-ASCII name, so Windows'
  own reader shows Japanese and accented names right (tested through `ArchiveFolder`). Undone by
  recycling the archive.
- **The foreign zip drop moved into the extraction job**: a drag out of a zip or 7z view in
  Explorer carries the items' PIDLs beside the virtual files, and `target_offer` now takes those
  first, so the drop is the extraction job on the file operation worker (the shell's progress, an
  undo record) instead of the synchronous reader inside `Drop`.
- **Dependencies**: `tar` 0.4.46 and `sevenz-rust2` 0.24.1 (pure Rust, no compression feature
  beyond LZMA2), and the `zip` crate's `deflate-flate2-zlib-rs` and `unreserved` features.
- **Tests**: 210 to 219 (the split with the new extensions, the handlers by extension, a 7z listed
  through `ArchiveFolder` with its cache, search and crumb folders, the zip writer's edits read back
  (a zip `tar.exe` made among them), the 7z and TAR writers, new archive names, the job
  classification, zip jobs and a Compress through the real worker, FILETIME conversion), and the
  existing drop test now asserts the zip folder's data object becomes the extraction job.

### Decisions

- **The original of a move into a zip goes to the Recycle Bin**, not deleted outright (Explorer's
  zip folder leaves it for the source to delete; the bin keeps it recoverable while the zip write
  cannot be undone).
- **A zip edit always rewrites the whole archive** (into a temporary file swapped in), as
  Explorer's zip folder does; appending in place would be faster for a large zip, at the price of a
  damaged archive if the write stopped half-way.
- **Rename and delete inside a zip ignore case** and act on a folder's whole subtree, including
  folders a zip only implies by its paths.
- **Refused, recorded**: copying from one archive into another (or within one zip), drops of
  virtual files into a zip, spring-loaded folders inside a zip, an archive inside an archive opened
  as a folder (it is a file there; Return opens it through the shell, which extracts it first).
- **The zip crate's `unreserved` feature**: without it a raw copy refuses entries carrying extra
  fields the crate knows of but does not use (Info-ZIP's Unix 0x7875, which Windows' `tar.exe`
  writes, as Linux and macOS zippers do); found by the scripted check, test added (fails without
  it).

### Scripted checks

`%TEMP%\twinfm-m5\p7m3` (a.zip, a.7z, a.tar.gz made by `tar.exe` from a.txt and d\c.txt), the
release build of this branch, `TWINFM_SOAK=1`:

```
[archive] ArchiveFolder takes 16 of 16 extensions (.7z .bz2 .cpio .gz .mtree .nupkg .rar .tar .tbz2 .tgz .txz .tzst .uu .xar .xz .zst), read in 1.1 ms
[enum] pane=1 done: 2 entries (archive level, ArchiveFolder, listed in 21.5 ms, fresh), worker 27.0 ms, request-to-done 30.8 ms
[enum] pane=1 done: 2 entries (archive level, ArchiveFolder, listed in 19.2 ms, cached), worker 4.9 ms, request-to-done 8.9 ms
[fileops] done id=1 pane=1 tab=2 Done: Copied 2 items
[archive] refused: copy into ...\p7m3\t.7z (Windows' archive handler cannot write into .7z archives)
[archive] job id=2 "copy in" on ...\p7m3\t.zip: Done in 15 ms
[archive] job id=3 "new folder" on ...\p7m3\t.zip: Done in 14 ms
[archive] job id=4 "rename" on ...\p7m3\t.zip: Done in 14 ms
[archive] job id=5 "move in" on ...\p7m3\t.zip: Done in 87 ms
[archive] job id=6 "delete" on ...\p7m3\t.zip: Done in 13 ms
[archive] job id=7 "move out" on ...\p7m3\t.zip: Done in 109 ms
[winmenu] pane=1 seq=2 shown after 0 ms (no query): [] New folder | New file | Copy path | --- | Show more options
[winmenu] pane=1 seq=3 shown after 0 ms (no query): [Cut Copy Rename Delete] Open | Open in other pane | --- | Copy path | --- | Show more options
[winmenu] submenu "Compress to" at (714, 442): [] ZIP file | 7z file | TAR file
[fileops] done id=8 pane=0 tab=1 Done: Compressed 1 item to d.7z
[fileops] done id=9 pane=0 tab=1 Done: Compressed 1 item to d.tar
[fileops] done id=10 pane=0 tab=1 Done: Undid: Compressed 1 item to d.tar
```

The first run of this script found every zip write failing on the zip `tar.exe` made (`Extra data
header ID 030837 (0x7875) requires crate feature "unreserved"`; each failure left the zip as it was
and no temporary file); the feature fixed it, above.

Listing times of the fixtures (40,000 files in `bigdir`; the same session, unelevated):

| Archive | Size | Root level | `bigdir` level, fresh | `bigdir` level, cached |
|---|---|---|---|---|
| `bigdir.zip` (own reader) | 4.26 MB | central directory 75.4 ms (all levels) | worker 103 ms | worker 30 ms |
| `bigdir.7z` (ArchiveFolder) | 105 KB | 85.4 ms | 312.8 ms (worker 340 ms) | worker 40 ms |
| `bigdir.tar.gz` (ArchiveFolder) | 431 KB | 215.7 ms | 295.0 ms (worker 322 ms) | worker 25 ms |

`bigdir.7z` and `bigdir.tar.gz` were made with `C:\Windows\System32\tar.exe` (9 s and 5 s). The list
kept rendering while the levels were listed (on the archive worker).

`tools/soak-p5m3.ps1 -Minutes 6` (with the archive cycle): exit code 0, no panics, 4 cycles started,
210 jobs done (0 failed, 0 cancelled, 0 soak refusals, 0 waitops timeouts); archives: 12 levels
listed by ArchiveFolder, zip jobs done 3 of each kind (copy in, new folder, rename, delete, move out,
move in; the fourth cycle stopped at the time limit before its archive steps), 0 not done, 4 writes
into the 7z refused, 6 archives compressed and 6 undone; every earlier count as in P7-M2's soak
(menupicks refused 0, toolbar presses refused 0, drags dropped 0, spring-loaded tabs 0); working set
122.3 MB at 5 s, 229.6 at most, 225.1 at 360 s (P7-M2's 8-minute soak: 209.6 at most).

### Gate

Tag ab28, 2026-10-19, `C:\traces\gate-p7m3.ps1` elevated at the console, Narrator off: the defaults
triplet, `target-p7m2` (f66ce33) against `target-p7m3` (2cde49f), interleaved.

| Run | WPR % | In-app % | Render p50 ms |
|---|---|---|---|
| p7m2-1 | 18.0 | 19.2 | 2.32 |
| p7m3-1 | 17.7 | 17.3 | 2.18 |
| p7m2-2 | 17.5 | 19.7 | 2.21 |
| p7m3-2 | 17.8 | 19.8 | 2.20 |
| p7m2-3 | 18.0 | 18.7 | 2.23 |
| p7m3-3 | 17.5 | 20.2 | 2.19 |

Medians 18.0 (reference) and 17.7 (candidate): no gap counts (the runs interleave, and the
candidate's median is the lower), every candidate run under 20% by WPR. One session state
throughout (render p50 2.18 to 2.32 ms in every run). `split` now knows the ArchiveFolder
extensions and runs on every path the scenario lists; as expected, flat.

### After the console check

Jeremy, unelevated, `target-p7m3` (2026-10-19), four findings:

- **A 7z copied into a zip was refused** as "copying from one archive into another". Cause: an
  archive file on disk is the root of its own archive to `split`, so `classify` took it for an item
  inside one; the same mistake would have named a deleted zip file "deleted from the archive" and
  refused Compress of an archive file. Fix: `fs::archive::within` (an item stored below an
  archive's root) where items are asked about; `jobs_are_classified` extended first, failing.
  An archive inside an archive is then a file in the zip; Return opens it through the shell, which
  extracts it first (no folder level, as recorded above).
- **"Compress to > ZIP file" refused** ("no "Send to>Compressed (zipped) folder" in the menu").
  Older than P7-M3 (P7-M2's path). Windows fills the Send to submenu in the background, about 450 ms
  after it is first asked (a probe on the verb worker's steps: a lone placeholder, then "Bluetooth
  device" at 303 ms, "Compressed (zipped) folder" at 454 ms); P7-M2's soak found it filled only
  because the menu was warm. Fix: `shellmenu::fill_until` pumps the verb worker's messages and asks
  again every 50 ms, up to 3 s (`SUBMENU_WAIT_MS`), for both the find by label and a Send to target
  picked in the classic menu; `[menu] "<label>" found after T ms (a submenu filled late)` or `not in
  the menu after T ms`. Scripted, a fresh process: found after 82 ms, the ZIP made.
- **No note for a refused write**: a drag into a 7z showed the no-drop cursor and logged the reason
  only. The pane under the pointer now notes it once per hover (`drop: Windows' archive handler
  cannot write into .7z archives`).
- **No Paste in a zip item's menu**: as in a folder, Paste is on the background menu's icon row
  (shown in a zip with files on the clipboard, scripted: `[Paste] New folder | New file | ...`), on
  the toolbar and on Ctrl+V; Explorer's item menu has none either.

Jeremy's second console check on ab43a49 (2026-10-19): as expected, no issues, except the
password-protected archive he had none of. Made for him, `C:\traces\password-twinfm.zip` (ZipCrypto,
password `twinfm`, by `tar.exe --options zip:encryption=zipcrypt --passphrase twinfm`; `secret.txt`
and `sub\inner.txt`), then checked by script on a copy, which found damage:

- **A write into a password-protected zip broke its encrypted entries.** The `zip` crate's raw copy
  writes the entry again without its general-purpose flags (encrypted, bit 0, and data descriptor,
  bit 3; the bytes stay encrypted), so after a copy in and a rename `tar.exe --passphrase` failed on
  `secret.txt` ("ZIP decompression failed (-3)"). It also drops Info-ZIP's extended timestamp field
  (0x5455), which Windows does not read. **Decision**: TwinFM refuses every write into a zip that
  holds an encrypted entry (`archivewrite::encrypted`, read on the worker before the job does
  anything, a move out included: `Failed: this zip is password-protected: TwinFM does not write
  into it`); `zip_jobs_on_the_worker` makes one with `tar.exe` and checks copy in, rename and move
  out are refused with the zip byte for byte and nothing extracted (written first, failing).
  Scripted again: both writes refused, and `secret.txt` and `sub\inner.txt` still extract with the
  password. Browsing it and copying out (the shell asks for the password) are unchanged. A byte
  for byte copy of the entries of our own would let TwinFM write into such zips; not done (a
  candidate if it is ever missed).

Jeremy's third console check on d0f74eb: copy out of the password zip and copy into it both
"failed" with no message and no password prompt. Reproduced by script: the copy out's job stood at
0% with the shell's "Password needed" dialog open **behind TwinFM's window** (class `#32770`, on a
thread of the shell's own, owned by its unowned `OperationStatusWindow`, neither brought forward),
and the copy in waited queued behind it, so neither ever ended. Older than P7-M3 in kind (any
dialog the copy engine raises on its own threads could stay behind). Fix: while file jobs are in
flight a 500 ms timer on the UI thread (`raise_job_dialogs`, stopped when none is) looks for this
process's visible `#32770` windows that are not in front (`fileops::hidden_dialogs`) and, when one
of TwinFM's windows is the foreground window, brings them to the front (`SetForegroundWindow`,
allowed to the foreground process); with another program in front they are left and logged once
(`[fileops] dialog "<title>" is open while another program is in front: left there`). Scripted with
TwinFM in front: `[fileops] dialog "Password needed" was behind the window: brought to the front
(ok)`.

Jeremy's fourth console check on 264770a (2026-10-19): the password prompt comes up in front on a
copy out of the password zip, and a copy into it is refused with the note: as expected. The gate
(ab28, on 2cde49f) was not run again: none of the console check's fixes is on the scenario's path
(the scenario runs no file job, opens no menu and drops nothing).

## P7-M4: the polish bundle and the drag cursor check

Branch `feat/p7-m4-polish`, from 589db37.

### Settled first (2026-10-19, before any code)

Each item checked against the code; three clashes and the drag cursor check went to Jeremy with a
recommendation, his answers:

- **Tooltips.** The kickoff said Slint has none built in; the pinned Slint (c1d3a8d, master a1ce0d8)
  has one, the `Tooltip` element (a hover area the compiler adds to its parent, shown after 500 ms
  under the pointer, drawn inside the window). Tried on the toolbar first (a scripted hover through
  Slint's pointer path, captured with the software renderer): it shows. Recommended Slint's
  built-in over a popup of our own. Jeremy: "Slint's built-in".
- **Two-line tile names do not fit today's tiles** (19 px under the icon). Recommended taller tiles,
  as Explorer's: large 96 x 120 to 96 x 136, medium 80 x 72 to 80 x 88 (a screen holds about 12% and
  18% fewer grid rows); the alternative was two lines at a smaller font in today's space. Jeremy:
  "Taller tiles".
- **The sidebar's arrows.** Explorer's navigation pane opens each folder the selection passes;
  recommended the kickoff's move-only, Return opens (nothing is listed while moving). Jeremy:
  "Move only, Return opens".
- **The drag cursor console check** (Jeremy, his everyday build, a file dragged from one pane over a
  folder in the other, "Move to ..." showing): **both show**, the description's icon and OLE's arrow
  with its box. So `GiveFeedback` gets code (below).

Calls made by the lead, told to Jeremy with the questions:

- Back and Forward over the toolbar across the window go to the active pane; over a pane's own
  toolbar, to that pane; over a disabled button and the gaps between buttons too.
- Subtree records become row edits for the active tab with folder sizes too, not only a background
  one: both listed the whole folder again on a change in the folder itself, and the edit path is the
  same (`window_refusal`, its cap of 500 changes).
- Search results sort by the file's own name (folders still first), a tie by the `sub\dir` path;
  the other columns' tie-break is the own name too.
- PageDown in the details list: to the last row wholly on screen, then by a screenful less one row
  (the old last row ends at the top), as Explorer; PageUp the same upwards. The grid's paging stays.
- The full name of the cursor tile, when it does not fit two lines, shows over the tiles below it
  (Explorer's), drawn by one overlay per pane, never by the tiles.

### What landed

- **Toolbar tooltips** (pane.slint `ToolButton`): Slint's `Tooltip` in each button, its name and
  key ("New file (Ctrl+Alt+N)", "New folder (Ctrl+Shift+N)", "Cut (Ctrl+X)", "Copy (Ctrl+C)", "Paste
  (Ctrl+V)", "Rename (F2)", "Delete (Delete)"), in both placements, over disabled buttons too; the
  tooltip's content reports itself when it is made (`[toolbar] top|pane=N tooltip "..."`). Driver
  `hover <x> <y>` (Slint's pointer path; winit drops a posted `WM_MOUSEMOVE` while the real cursor is
  outside the window, found while trying the tooltip).
- **Back and Forward over the toolbar**: the button's `TouchArea` stays enabled (a disabled button
  refuses only the click) and reports the side buttons; a `TouchArea` behind the bar across the
  window takes its gaps; the in-pane bar's gaps were the pane's already. Driver `backclick <pane>
  tool`, `forwardclick <pane> tool`.
- **Subtree records as edits** (`fs::watch`, `main.rs` `take_window`): a subtree watch keeps the
  records in the folder itself beside the children it names (past 1,000 it stops keeping them, as a
  plain watch does), and its event carries their net changes; `watch_event` sends them through the
  plain watch's path (`window_refusal`, the edit worker, `land_edits`), for the active and the
  background tab alike. An overflow is the whole-folder refresh as before. Before, a tab with folder
  sizes listed its folder again on every change in it, including the lazy "modified" NTFS reports
  for a subfolder after a walk has opened it (now a `~` edit).
- **Search results sorted by the file's own name** (`model::cmp_names`, used by `cmp_entries` for
  every column's name and tie-break; a folder listing's names hold no `\`, so it is `natural_cmp`
  there, with no extra work).
- **The sidebar's keys** (`sidebar::step`, `row_top`): Up, Down, Home and End move past the headers
  and scroll the row into view; Return opens the row in the active pane, as a click does, keeping
  the keys in the sidebar.
- **Explorer's PageDown and PageUp in the details list** (`model::page_target`, called from the
  list's key handler through a pure `Paging` global, so the Slint key path is unchanged): to the last
  (first) row wholly on screen, then a screenful less one row.
- **Two-line tile names** (`FileGrid`, `FileGridMedium`): tiles 96 x 136 and 80 x 88 (`TileGeom`),
  the name under the icon in a 38 px box with `wrap: word-wrap`, `max-lines: 2`, `overflow: elide`,
  top-aligned; Slint's word wrap breaks a word longer than the line anywhere, as Explorer does. The
  cursor tile's whole name, when its wrapped height passes the two lines, shows in one overlay per
  pane over the tiles below (the grid's clip, after the strip, no input). The name editor sits over
  the name's first line.
- **The drag cursor** (`dnd.rs` `ds_give_feedback`, `own_cursor`): while the drag image shows a
  target's drop description (`IsShowingLayered` and a `DropDescription` other than
  `DROPIMAGE_INVALID` on our data object), `SetCursor(IDC_ARROW)` and `S_OK`; otherwise
  `DRAGDROP_S_USEDEFAULTCURSORS` as before; `[dnd] cursor: ...` on each change.
- **Tests**: 219 to 223 (`page_target`, the search sort, the sidebar's step, the cursor rule), the
  subtree watch's test extended (the folder's own record as an edit), and the tile geometry in five
  tests moved to the new heights.

### Decisions

- **Every tooltip is the button's name and key**; New file's key is TwinFM's own (Ctrl+Alt+N).
- **The overlay shows on the cursor tile whether or not the list has the keys** (Explorer shows it
  on the focused item); it takes no input, so a click on it lands on the tile under it.
- **The grid keeps its own PageDown** (a viewport of grid rows); Explorer's rule is for the details
  list, as the kickoff wrote it.

### Scripted checks

The release build of this branch, scratch folders under `%TEMP%\twinfm-p7m4-*`:

```
[toolbar] top tooltip "Cut (Ctrl+X)"                      (hover 240 18, a disabled button)
[toolbar] pane=0 tooltip "Copy (Ctrl+C)"                  (toolbar-pane.toml, hover 300 20)
[auto] scrollpos pane=0 content-y 0.0 current-row 0       (C:\Windows\System32, row 0)
[auto] scrollpos pane=0 content-y 0.0 current-row 29      PageDown: the last row on screen
[auto] scrollpos pane=0 content-y -620.0 current-row 58   PageDown: a screenful less one row
[auto] scrollpos pane=0 content-y -620.0 current-row 29   PageUp: the first row on screen
[auto] scrollpos pane=0 content-y 0.0 current-row 0       PageUp
[nav] pane=0 Back button (active pane was 0, now 0)       (backclick 0 tool)
[nav] pane=0 history back: C:\Windows -> C:\Windows\System32 (1 back, 1 forward left)
[nav] pane=0 Forward button (active pane was 0, now 0)    (forwardclick 0 tool)
[sidebar] key Down: row 2 (Google Drive (G:))
[sidebar] key Down: row 4 (Home)                          past the Places header
[sidebar] key Up: row 2 (Google Drive (G:))
[sidebar] key End: row 11 (Videos)
[sidebar] key Home: row 1 (Local Disk (C:))
[sidebar] Return on row 2
[sidebar] row 2 click: G:\
[watch] pane=0 tab=1 subtree records 2 in the folder, changes 2, window 250 ms   (the sized tab, active: NTFS's lazy "modified" for two subfolders)
[watch] pane=0 tab=1 applied: +0 -0 ~2 (stat 0.2 ms, apply 0.0 ms)
[watch] pane=0 tab=4 applied: +1 -0 ~0 (stat 3.3 ms, apply 0.0 ms)                 (touch new.txt: the new tab)
[watch] pane=0 tab=1 subtree records 2 in the folder, changes 1, window 500 ms   (the sized tab, now in the background)
[watch] pane=0 tab=1 applied: +1 -0 ~0 (stat 3.2 ms, apply 0.0 ms)
[watch] pane=0 tab=1 applied: +0 -1 ~0 (stat 0.0 ms, apply 0.0 ms)                 (rmfile new.txt)
```

Search for `*.txt` over `a\zeta.txt`, `a\alpha.txt`, `b\beta.txt`, `b\alpha.txt` and `gamma.txt`
lists `alpha.txt (a)`, `alpha.txt (b)`, `beta.txt (b)`, `gamma.txt`, `zeta.txt (a)` (captured with the
software renderer). Large and medium tiles captured the same way, with names of one line, two lines,
a 61-character name with spaces and a 42-character name without: two lines, the second elided, the
spaceless name broken anywhere, and the cursor tile's whole name over the tile below.

**The generated lists against the reference's** (`target-p7m3`, 264770a, and this build's
`app.rs`, from `InnerFileList_root_` up to `InnerFileListHost_root_`, generated numbers in identifiers
mapped to their order of first appearance, field lists split into fields, each component compared as
a multiset of lines): the four details lists identical (6,518 lines, none on one side only); each grid
list 1,673 lines against 1,716, the reference's 7 lines only its own being the tile's 120 px (three
places), the name's 19 px height and centred alignment, and one layout-info read; the build's 50 its
own being 136 px, the name's new properties, and the compiler's height-for-width layout function for
the grid component (a wrapped text inside makes its height depend on its width), which the grid
triplets measure.

`tools/soak-p5m3.ps1 -Minutes 6` (a copy of this build): exit code 0, no panics, 4 cycles started (the
fourth cut short by the time limit before its archive steps and sidebar walk), 205 jobs done (0
failed, 0 cancelled, 0 soak refusals, 0 waitops timeouts); toolbar tooltips 12 (three per cycle),
Back and Forward over the toolbar 4 each, sidebar keys 15 and Returns 3; toolbar presses 32, 0
refused as disabled; drags dropped 0, spring-loaded tabs 0, menupicks refused 0, zip jobs not done 0;
every other count as in P7-M3's soak; working set 152.8 MB at 5 s, 241.8 at most, 235.1 at 361 s.

`compress_seven_z_and_tar` (P7-M3's) failed once in five full test runs here, at its last line, with
"used by another process" removing its scratch folder (Windows' archive handler still holding the 7z
it had listed); recorded, not changed in this milestone.

### Gate

`C:\traces\gate-p7m4.ps1`: `target-p7m3` (264770a) against `target-p7m4`, four pairs interleaved
(defaults, `grid.toml`, `grid-medium.toml`, `toolbar-pane.toml`, the reference run with the same
file as the candidate each time).

**ab29 (2026-10-19, 15:54 to 16:28, on 8b9c488) is void**: a side session (the archive test fix,
PRs #71 and #72) ran its test loop, six test processes at once, and rebuilt `target\release` on the
same machine throughout the gate. Some runs reached the split 22 s late, so the 10 s window caught an
idle app (WPR 1.5 to 2.2% beside in-app figures of 20 to 37%), two in-pane toolbar runs could not be
analysed ("could not find the right-pane enumeration worker"), and render p50 ranged 1.48 to 6.15 ms
across runs; the wrapper's verdicts were computed from those figures and read nothing. The lesson,
already in the projects-root rules ("worktrees isolate the repo, not shared targets") and now in the
wrapper: it refuses to start while `cargo`, `rustc` or a test binary runs. Re-run as ab30 on the
branch after main's two merges.

**ab30**, 2026-10-19 (finished 17:24, 31 min), on c796139, nothing else running; each pair reference
then candidate, the four pairs interleaved round by round:

| Pair | Reference runs (WPR %) | Candidate runs (WPR %) | Medians | Render p50 ms (ref / cand) |
|---|---|---|---|---|
| defaults | 18.4, 17.2, 20.4 | 19.3, 18.3, 20.5 | 18.4 / 19.3 | 2.16 2.13 2.87 / 2.15 2.31 2.88 |
| large grid | 11.4, 11.5, 11.5 | 10.2, 11.4, 11.6 | 11.5 / 11.4 | 1.21 1.41 1.42 / 1.12 1.39 1.41 |
| medium grid | 12.8, 15.0, 15.8 | 12.4, 14.3, 13.9 | 15.0 / 13.9 | 1.58 2.07 2.03 / 1.48 1.84 1.86 |
| in-pane toolbar | 17.3, 20.5, 20.0 | 18.3, 21.1, 21.2 | 20.0 / 21.1 | 2.17 2.84 2.86 / 2.22 2.94 2.88 |

The session changed state part-way (render p50 about 2.2 against 2.9 ms, a third higher, as in
ab23): round 3 of the defaults and rounds 2 and 3 of the toolbar pair ran in the slower state, both
builds alike, so each pair compares like for like. The grids: the candidate at or below the
reference (the taller tiles show fewer grid rows; the generated height-for-width function costs
nothing visible). The defaults: 0.9 points, under the rule's one point. The in-pane toolbar: 1.1
points, a gap by the letter of the rule, with no run under 20% for either build in the slower
state. The traces show no new work: the toolbar pair's main-thread breakdowns hold the same
categories in the same shares (render pass 60.6 and 60.6%, text 34.3 and 33.6%, ListView rows 17.4
and 17.7%, timers 34.4 and 35.0%, round 1), nothing of the tooltip or the new layout function among
the frames, the extra 100 ms of 2,000 spread evenly. In every pair the candidate ran second, the
slot that read one to three points higher whichever binary sat in it (2026-10-15, P4-M2); the
swapped-order re-run (ab31, `C:\traces\gate-p7m4-swap.ps1`: the defaults and toolbar pairs with the
candidate first) is to tell that bias from a cost.

**ab31**, 2026-10-19 (finished 17:48, 15 min), the same binaries, the candidate first in each pair:

| Pair | Candidate runs (WPR %) | Reference runs (WPR %) | Medians (cand / ref) | Render p50 ms (cand / ref) |
|---|---|---|---|---|
| defaults | 19.8, 22.5, 20.3 | 19.1, 20.3, 20.9 | 20.3 / 20.3 | 2.23 3.04 2.85 / 2.36 2.88 2.90 |
| in-pane toolbar | 29.4, 20.6, 20.3 | 27.2, 20.6, 20.5 | 20.6 / 20.6 | 3.72 2.88 2.85 / 3.58 2.90 2.86 |

The session ran mostly in the slower state (render p50 2.85 to 3.04 ms, and round 1 of the toolbar
pair slower still, 3.6 to 3.7, both builds), so both builds sit about 20%; pair by pair, like for
like, the candidate reads +0.7, +2.2 and -0.6 on the defaults and +2.2, 0.0 and -0.2 on the toolbar,
now with the first slot's lead. Equal medians in both pairs: **no gap counts** in either order, and
ab30's 1.1 points on the toolbar pair were the second slot's. Read together: P7-M4 passes the gate
(the grids at or below the reference, the defaults and in-pane toolbar level with it; in ab30's
faster state every defaults and toolbar run of both builds in round 1 under 20%).

### Console checks (for Jeremy, unelevated, after the gate)

1. **The drag cursor**: drag a file from one pane over a folder in the other: the "Move to ..." label
   with only the arrow beside it, as when dragging from Explorer; over Explorer's window the same; over
   a place that refuses the drop, the no-drop image.
2. **Tooltips**: hover each toolbar button in both placements (Settings > Toolbar): the name and key
   after half a second, under the pointer, never past the window's edge awkwardly.
3. **Back and Forward mouse buttons** over a toolbar button, a dimmed one and the gap between groups.
4. **Sidebar keys**: click a row, then Up, Down, Home, End, Return; Escape gives the keys back.
5. **PageDown and PageUp** in a long details list beside Explorer's.
6. **Tile names** in large and medium icons beside Explorer's: long names, names without spaces, the
   cursor tile's whole name.
7. **Search results** sorted by name.
8. **Folder sizes kept**: size a folder's subfolders (Alt+Shift+Enter), open a new tab, create a file
   in the first folder from Explorer, go back: the file is there without the list being listed again.

## Open questions

Written 2026-10-18 at the kickoff.

- **How the packaged apps' context menu items are found** (P7-M2): the first thing the milestone
  settles.
- **Which archive formats Windows' handler writes into** (P7-M3): settled 2026-10-19, none
  (`ArchiveFolder` refuses every write; the zip folder writes only through its drop target); TwinFM
  writes zips itself and the other formats stay read-only (§ P7-M3 Settled first).
- **Carried from Phase 6**: Google Drive's volume found by its name; a sync-root scratch folder for
  the soak (a console check only, Jeremy's call); the Slint whole-row snap patch and upstreaming
  (Jeremy's); the dead share console check owed from P5-M1.
