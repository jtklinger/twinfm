# Phase 6: TwinFM shows what the cloud knows, finishes its views, and takes the mouse

Written 2026-10-16, when Phase 5 closed with P5-M1 to P5-M3 merged and the P5-M3 build at a median
of 17.8% against the reference's 17.9% on the gate under the triplet protocol (see
`phase-5-plan.md` § Phase 5 close). Phase 1 made TwinFM usable for a day instead of Files, Phase 2
made it trustworthy every day, Phase 3 made it find and show what Explorer can, Phase 4 showed a
folder as large icons with thumbnails, Phase 5 finished the Phase 2 lists. Phase 6's goal, chosen
by Jeremy on 2026-10-16 from the Phase 5 close's candidates, three of them in this order: **cloud
status marks and the remaining icon items, the views finished, and a polish bundle led by a
toolbar**. Not in Phase 6, by the same decision: the shell menu's verbs through TwinFM's jobs and
Escape over a cut, the drag cursor beside the description, content search, archives beyond zip,
the compact Windows 11 context menu and menu icons, thumbnails beyond the listing, the Slint
fork's third patch. Accessibility (the grid read by a screen reader, the Narrator passes) stays
deferred for much later.

What Jeremy decided at the kickoff (2026-10-16), in his words where he gave them:

- The cloud marks are "an icon that's similar to the way it's currently displayed in File
  Explorer": a green tick for a file that is on this disk, a cloud for a file that is only in the
  cloud. "Doesn't need to be anything fancy." Google Drive is in this phase beside OneDrive.
- The grid gains medium tiles; the other sizes are not wanted ("I am mainly in the list view
  about 95% of the time"). The view and the tile size go in the settings dialog, thumbnails get a
  frame. PageUp and PageDown in the details list must work: "when I tested that earlier, it
  didn't seem to be working correctly" (they were never mapped there, `phase-4-plan.md` § P4-M1
  Deferred).
- The toolbar is pulled forward from the Phase 1 deferral with seven buttons, "create new files
  and folders, copy, paste, cut, as well as delete. Having a rename button would be nice as well",
  for the hand that is already on the mouse. Shown three mockups (one labelled toolbar across the
  top of the window; the same with icons only; a compact icon-only toolbar inside each pane
  above its path bar), he chose the first and the third: "can we have both as options?"
- Nothing else bites in daily use ("nothing else now").

The starting point is main at f6a18bd (the Phase 5 close and a CLAUDE.md note). What the three
milestones rest on today: icons reach the rows only through each pane's composed strip and the
grid's composed rows (`src/icons.rs`; `TILE_W` 96, `TILE_H` 120, `GRID_ICON_PX` 96), keyed per
type for ordinary files and per full path for links, reparse points, drives and customised
folders, with the shell's overlay (`SHGFI_OVERLAYINDEX`, the image from the system image list,
fetched once per index and size) drawn for links and reparse kinds only (`wants_overlay`); a
cloud placeholder inside a sync root is a plain file to the process (the cloud files filter hides
placeholders until the process opts in with `PHCM_EXPOSE_PLACEHOLDERS`), so OneDrive files share
their type's key and carry no mark, and Google Drive's files on its virtual volume (`G:`, FAT32,
`GoogleDriveFS.exe`, no sync root registered; on this machine its badges come from its four
`ShellIconOverlayIdentifiers`, as OneDrive's seven do for files the cloud filter does not mark)
are ordinary files that are never asked for an overlay; the grid at one tile size, switched by
Ctrl+Shift+1 / Ctrl+Shift+2 and saved as `view_left` / `view_right`, not in the settings dialog;
PageUp and PageDown handled in the grid and unmapped in the details list; the settings dialog
(`src/settings.rs`, `Dialog`) with start folders, hidden files per pane, the Type column,
thumbnails, the sidebar and the path bar; no toolbar; bookmarks renamed through the context
menu's "Rename bookmark" and its inline editor, with no F2; `Job::NewFolder` (Ctrl+Shift+N) and no
new-file job.

Three milestones, in the order Jeremy gave on 2026-10-16 (do not fold them):

1. **P6-M1, cloud marks and the icon items.** A file in a cloud location shows the shell's own
   badge over its icon: OneDrive's and Google Drive's green tick for a file on this disk, their
   cloud for a file that is only in the cloud, drawn by the overlay path the links already use
   (`overlay_index` on the worker, `overlay` at compose), so the marks look exactly as Explorer's
   do and nothing is drawn by hand. A cloud location is decided once per folder on the UI thread,
   never per file: a folder under a registered sync root (`HKLM\...\Explorer\SyncRootManager`'s
   `UserSyncRoots`, read once per process and on `WM_DEVICECHANGE`) or on a volume whose
   file system reports itself as Google Drive's (the volume name, checked through the P5-M3
   volume cache); every other folder keeps today's keys and today's cost, so the 40,000-file gate
   does not move. Inside a cloud location, the key of a file is its type plus its overlay index
   (`ext:<index>:<ext>`, a folder's `folder:<index>`), never its path: the base icon with that
   overlay is fetched once per (type, index) pair per process, as every key is today, and what is
   paid per file is one `SHGetFileInfoW` for the index (`overlay_index`), on a worker, after the
   listing is shown with its plain type icons (the marks land as the lookups finish, as icons do;
   the P2-M5 placeholder cost of 568 ms was `GetImage` on a placeholder, not the index call, and
   the milestone measures the index call on a thousand-file OneDrive folder and on `G:` before the
   path is fixed; a file whose attributes already say its state, `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`
   or `OFFLINE` on a OneDrive placeholder, may skip the call where the attribute decides the index
   the same way, if the check shows it does). A file whose index is 0 shares today's key. The
   index is looked up again for a file a refresh or a watch record touches in a cloud location (a
   hydrated file keeps its path, so the key must change with the state, as `thumb:` keys change
   with the modified time), so a mark is stale only between a change and the next record or
   refresh, recorded as the limitation; the once-per-process rule covers the overlay images and
   the (type, index) icons, not the index lookups, and the `[icons]` count on a refresh of a cloud
   folder reports the lookups as `cloud N` on the once-a-second line. A cloud-only file (its
   overlay the cloud) is never asked for a thumbnail extraction, which would download it: with
   thumbnails on, such a file takes the thumbnail the shell's cache already holds
   (`WTS_INCACHEONLY`) or its type's icon, in the P3-M5 request path. Exposing placeholders (`PHCM_EXPOSE_PLACEHOLDERS`,
   `SetProcessPlaceholderCompatibilityMode`) is turned on only if the OneDrive overlay handlers
   turn out to need it, after its own check of what it changes for enumeration, `stat_entry` and
   the file operations (the P2-M5 note: it changes what every file API in the process reports);
   if the handlers mark the files without it, it stays off, and the record says which. The two
   other P2-M5 items ride along: the one `SHGetFileInfoW` per overlaid key is made once per key
   per process and never repeated on a refresh that keeps the key (the table already keys it; the
   record shows the count on a refresh of a cloud folder of a thousand files), and "other"
   reparse points are checked for an overlay on a real one (a Windows Projected File System or a
   WCI reparse point, whichever can be made on this machine). The sync state that is neither tick
   nor cloud (syncing, error, "always keep on this device") shows whatever overlay the shell
   gives it, unfiltered: Jeremy asked for tick and cloud, and the shell's third glyphs cost
   nothing extra. Not in this milestone: a Status column, our own glyphs, cloud marks in the
   search tab or inside an archive, marks on the sidebar's rows. Gate: the defaults triplet and
   the thumbnails triplet (`C:\traces\thumbs.toml`), since the key rules and the compose path
   change; a console check on a OneDrive folder with files in both states and on `G:\My Drive`,
   with the marks compared against Explorer's side by side, and a `[icons]` count of overlay
   fetches on a refresh.
2. **P6-M2, the views finished.** PageUp and PageDown in the details list move the cursor a
   viewport of rows (the visible row count, floor of the list's height over the row height, as the
   grid's move by a viewport of grid rows; Shift extends the range as Shift+Up does), on the
   focused list's key path, with a driver check of the content-y and the cursor row after each.
   The grid gains a second tile size, medium: the same `FileGridList` at a smaller tile with a
   smaller picture, chosen per pane and saved beside the view (`tile_left` / `tile_right`,
   `"large"` the default or `"medium"`; the implementer measures Explorer's medium tile and picks
   the icon size the shell renders without scaling, 48 px being the likely one, with the tile
   around 64 x 84 at 100%; `TILE_W`, `TILE_H` and `GRID_ICON_PX` become a per-pane `TileGeom` that
   `window_layout`, `dnd::hit`, the band, the rename editor, the menu point and the driver read,
   as they read the view today). Thumbnails in the tiles get a one-pixel frame in the shell's
   neutral grey, drawn at compose time around the picture's fitted rectangle (never around a type
   icon), with the medium grid's thumbnails asked at the medium icon size through the P3-M5 path.
   The settings dialog gains, per pane, the view (details, large icons, medium icons) as one
   choice, beside the hidden files; Ctrl+Shift+1 and Ctrl+Shift+2 keep their meaning and a new
   Ctrl+Shift+3 picks medium. The cursor follows the band: when the band selects, the cursor moves
   to the last tile the band touched, so Shift and the keys continue from there (Explorer). Extra
   large icons are out. Gate: the defaults triplet (the details list's key path changes), the grid
   triplet (`C:\traces\grid.toml`) and a medium triplet (`C:\traces/grid-medium.toml`, both panes
   `"icons"` at `"medium"`), with the generated row and tile diffed against the reference's before
   the gate as every milestone since P3-M5.
3. **P6-M3, the toolbar and the polish bundle.** A toolbar with seven buttons, New file, New
   folder, Cut, Copy, Paste, Rename, Delete, in that order in three groups with thin separators,
   placed by a setting (`toolbar`: `"top"`, a labelled toolbar across the window under the title,
   acting on the active pane; `"pane"`, a compact icon-only toolbar inside each pane above its path
   bar, acting on that pane and making it active; `"none"`), chosen in the settings dialog beside
   the sidebar and path bar, `"top"` the default. Each button calls the command the key already
   calls (Ctrl+X, Ctrl+C, Ctrl+V, F2, Delete, Ctrl+Shift+N), so there is one path per command and
   the toolbar adds no behaviour of its own; buttons that cannot apply are disabled (Paste with no
   files on the clipboard, Cut, Copy, Rename and Delete with nothing selected, Rename with more
   than one, everything but the two New buttons where the pane shows This PC or a search tab, and
   everything inside an archive level, which refuses writes). The button icons are the shell's
   stock images (`SHGetStockIconInfo`) where Windows has them, at the toolbar's size, fetched once
   per process, else the shell's small icon for the type. New file is new: `Job::NewFile` makes an
   empty `New Text Document.txt` (Explorer's name, numbered on collision as the folder job's is)
   through the same `IFileOperation` path, undoable, with the rename editor opened on it as the
   folder job does; its key is Ctrl+Alt+N. Toolbar state (which buttons are enabled) is composed
   in `update_status`'s rhythm from the selection and the clipboard listener, never per row or per
   frame. Then the polish bundle: F2 on a focused bookmark row opens its title editor (the
   "Rename bookmark" path); the breadcrumb dropdown takes the keyboard (Down into it, Up and Down
   along it, Return picks, Escape closes); restored tabs keep their sort, hidden filter, cursor and
   scroll (saved per tab in `left_tabs` / `right_tabs` as tables, an old string list still
   loading); the tab strip re-reveals the active tab on resize; the search tab's Name column takes
   the width the longest visible result needs, capped at the pane's half; folder sizes survive a
   tab switch (the subtree watch of a background tab stays, its cached sizes with it, under the
   P5-M1 cap); the Back mouse button over the path entry or the rename editor is ignored instead
   of navigating; the protected-files switch (Ctrl+H showing system files too today) becomes two
   switches, as Explorer's: hidden files (Ctrl+H, per tab as today, the `show_hidden_*` keys
   keeping their name) shows files with the hidden attribute, and protected files (a per-pane
   setting in the dialog only, `show_protected_left` / `show_protected_right`, default false, no
   key) shows files with both the hidden and the system attribute; a file with the system
   attribute alone is always shown, as Explorer shows it, which changes what a plain Ctrl+H
   lists today (the `[hidden]` counts follow). Kept on Jeremy's word (2026-10-16). Gate: the defaults triplet (the
   toolbar is outside the lists but the pane's layout changes with `"pane"`) and a toolbar triplet
   (`C:\traces\toolbar-pane.toml`, both panes with the in-pane toolbar); console checks of every
   button against both panes, with the toolbar on and off, and of the new file's undo.

Each milestone brings its driver steps and stderr lines (named in its brief and recorded in
`CLAUDE.md` § Measuring), tests for its maths (the cloud-location rule, the tile geometry, the
viewport size, the enabled-button rule, the new file's name) and a soak: P6-M1 adds a OneDrive
scratch folder to `tools/soak-p2m1.ps1`'s writer if one can be fenced (`%TEMP%` is not in a sync
root, so this is a check, not a soak, unless a sync root scratch folder is agreed), P6-M2 adds a
medium-grid cycle to `tools/soak-p5m3.ps1`, P6-M3 adds a toolbar cycle (each button through its
callback, a new file and its undo) to the same soak. Console checks after each triplet: the marks
beside Explorer's (P6-M1), medium tiles with thumbnails on a photo folder (P6-M2), the toolbar in
both placements and the new file (P6-M3).

**The gate is re-run after every milestone**, under the protocol agreed on 2026-10-12 and
unchanged since: `tools/wpr-triplet.ps1` elevated at the console by Jeremy, accessibility on,
Narrator off, every other TwinFM window closed, the split-and-scroll scenario on
`C:\traces\bigdir`, on clean binaries built with `--locked` into their own `--target-dir
target-<name>` from detached `git worktree add` checkouts with default features. Three runs per
binary, interleaved (reference, candidate, reference, candidate, reference, candidate), compared by
median; a gap counts only when every reference run sits below every candidate run or the medians
differ by more than one point. The reference for the phase's first milestone is `target-p5m3`
(074ccb2, P5-M3's gate build); after each merge the new milestone's gate build becomes the next
reference and the previous one is deleted (`target-p5m2` goes once `target-p5m3` has served).
Tags continue from ab22. Bar: main thread under 20% busy over the 10 s after the split, judged
against the reference's spread in the same session, never against a recorded number. Console
checks run after the triplet, never before, and start TwinFM unelevated (f6a18bd). A milestone is
not closed until its table is recorded here. When a milestone fails: stop, symbolize, name the
frames, and either fix the mechanism or report the options; never trade the bar off silently.

**Settings triplets stay part of the protocol** (Phase 4's rule): a milestone that touches the
list, the grid or the icons runs the defaults triplet and the settings triplets its entry above
names (each a read-only toml under `C:\traces\`, each candidate's stderr checked for `[settings]
file <toml> (read)` and the `[view]` or `[icons] list geometry` line the setting implies before
its triplet is read). The gate scenario has no cloud folder, no medium grid and no toolbar
button press, so what the triplets measure in this phase is the wiring: the key rule's branch,
the tile geometry read per layout, the toolbar's row in the pane; a milestone whose scenario cost
is zero by construction still runs its triplet, because P3-M1 found 1.5 points in a per-row `if`
nobody expected to cost anything.

The architecture rules from Phase 1 hold (see `phase-1-plan.md` and the kickoff brief they came
from): only the UI thread touches a Slint model and workers wake it with
`slint::invoke_from_event_loop`; every COM object lives on the thread that made it; no app-state
`RefCell` borrow is held across a modal loop; no `unwrap` on `Weak<App>`; `windows-sys` with
hand-written calls; nothing per frame or per row without a trace first.

Amendment (P2-M4), carried over: the modal-loop rule is load-bearing, not precautionary. Our own
`DoDragDrop` runs from a posted message outside winit's handler, so `invoke_from_event_loop`
closures run INSIDE our own drag loop; every such closure takes `with_state`, the drag code holds
no borrow while the loop runs, and no closure may itself open a modal loop. Context menus and the
shell's dialogs still run inside a handler or on the worker.

Amendment (P2-M5), carried over: rows carry no image; icons reach the screen only through each
pane's strip (`icons::compose`, and the grid's `compose_grid`), and every per-key cost is paid on
a worker, once per key per process. Anything that would put a per-row or per-tile image or shell
call back is a trace first, not a feature. P6-M1's per-file keys are the exception the rule
allows, bounded to cloud locations and paid on the workers as the links' overlays are.

Amendment (P3-M1 and P3-M5), carried over: nothing in the row or the tile may depend on a runtime
property. The row's structure is chosen once per pane, outside the `for`, and proven by diffing
the generated code against the reference's row. P6-M2's medium tile and P6-M3's in-pane toolbar
follow this: the tile size is a per-pane choice of geometry read at layout, never a binding in
the tile, and the toolbar is a sibling of the list, never a row property.

Amendment (P5), carried over: a record is applied to the model only when the listing is settled
and the name is accounted for; anything else is the whole-folder refresh, logged with its reason.

Amendment (P6): a command has one path. The toolbar's buttons, the keys and the context menu's
own items call the same functions; a button never re-implements what its key does, and the
enabled state of the buttons is derived from the same selection and clipboard facts the status
bar already reads, in its rhythm.

Each milestone runs as in Phase 5: implementer brief, lead review, gates, push, PR, a gate build
from a detached checkout with `--locked`, `tools/wpr-triplet.ps1 -Reference p5m3 -Candidate
<name>[,<name>@C:\traces\<toml>] -Tag <next>`, console checks after the triplet, merge on
Jeremy's word, `target\release` rebuilt from main and said so.

## Recorded for Phase 7

Items found during Phase 6 that Jeremy sent to Phase 7, for the Phase 6 close to carry into its
candidates.

### Shell verbs off the UI thread

Decided 2026-10-17, in Jeremy's words: "Run the shell-menu fix as phase 7 item". It waits because
the kickoff put the shell menu's verbs out of Phase 6 (the first section above); the P6-M1
record lists it under its Deferred as well.

**What happens today.** A shell command picked from TwinFM's context menu runs on the UI thread:
`shellmenu::show` (called from `main.rs` with no app-state borrow) shows the menu with
`TrackPopupMenuEx`, then for `Pick::Shell` reads the verb with `GetCommandString(GCS_VERBW)` and
calls `IContextMenu::InvokeCommand` right there. A verb that waits on a slow file system holds the
UI thread for as long as it waits. On 2026-10-17 Jeremy picked "Create shortcut" on `G:` (Google
Drive's virtual volume) and TwinFM went white for about a minute before the shell's dialog
appeared.

**Design notes for the implementer** (a starting point, each to be checked against a trace):

- Marshalling the menu object to another thread does not help: the shell's menus are
  apartment-threaded, so a call through a proxy still runs on the UI thread that made the object,
  and the UI thread would block on the proxy as well. The menu has to be rebuilt where the verb
  runs.
- A dedicated verb worker: one STA thread (`CoInitializeEx` apartment-threaded, with a message
  loop, since shell verbs post messages and some show their own windows). It takes a job of the
  folder, the item names, the verb string, the command offset and its menu label, the invoke
  point and the menu flags (extended or not); it runs `parse_items`, `GetUIObjectOf` (or
  `CreateViewObject` for the background), `QueryContextMenu` into a hidden menu of its own with
  the same base and id range, and `InvokeCommand`. Every COM object the job makes is made,
  used and released on the worker; the UI thread's menu objects stay on the UI thread, as now.
- Invoke by verb string when the command has one (most do: `link`, `properties`, `copy`,
  handler verbs), since a rebuilt menu can number its items differently (handlers loading in a
  different order, a state change between the two builds). With no verb string, invoke by
  offset only if the rebuilt menu's item at that id carries the same label; otherwise refuse and
  log. That choice is a pure function and takes the unit tests.
- Stays as it is: TwinFM's own items (`Pick::Own`) and the "rename" verb (`Picked::Rename`, to
  TwinFM's name editor) are decided on the UI thread from the menu that was shown and never go
  to the worker. `GetCommandString` stays on the UI thread too; it is cheap and needs the
  shown menu.
- The owner window: passing TwinFM's HWND as `CMINVOKECOMMANDINFOEX.hwnd` from another thread
  attaches the two threads' input queues while a dialog is up, which can let a hung verb stall
  TwinFM's input. The alternatives are a hidden window owned by the worker, or no owner (the
  dialog then is not modal to TwinFM, as when Explorer's verbs run). The milestone tries the
  hung case on `G:` and records which it chose and why.
- `CMIC_MASK_ASYNCOK` lets a verb hand itself off; if used, the worker must stay alive and pump
  until the shell lets go (the process reference the shell asks for through
  `SHSetInstanceExplorer`), and quitting TwinFM while a verb runs needs a bounded wait, logged.
- The result comes back to the UI thread through `slint::invoke_from_event_loop` (only the UI
  thread touches Slint models); the `[menu] InvokeCommand verb=... = 0x...` line moves to the
  worker and gains the time the verb took. No `RefCell` borrow is held across the worker's modal
  loops, since they are on another thread and touch no app state.
- The scripted check: the driver never presses Return in a menu by design, so it needs a seam.
  A driver step that queues a verb job on the worker directly (no menu), with a test-only verb
  that sleeps on the worker for a given time, run behind the `TWINFM_SOAK` fence, while the
  script scrolls a list; the check passes when frames keep rendering (`[perf]` lines) through the
  sleep. A real verb end to end through the worker with the same step: `copy` on a scratch file
  (it overwrites the clipboard, as the soak already does). The gate scenario opens no menu, so
  the triplet is expected flat; it runs under the protocol anyway.

## Calls made in this kickoff, for Jeremy to check

Decisions Jeremy did not make and the lead settled, each reversible before its milestone starts:

- The sync states beyond tick and cloud show the shell's own glyph, unfiltered (P6-M1).
- A cloud location is found per folder from the sync root registry and the volume name, never
  by asking the shell about every file (P6-M1).
- Placeholders are exposed to the process only if the marks need it, after a check (P6-M1).
- A cloud file's key is its type plus its overlay index, so icons stay shared and the per-file
  cost is one index lookup on a worker, repeated when a refresh or record touches the file; a
  mark may be stale until then (P6-M1, from the builder check).
- A cloud-only file is never asked for a thumbnail extraction, only for one the cache already
  holds (P6-M1, from the builder check).
- Medium is the only new tile size, at the shell's 48 px icon if that is what Explorer's medium
  uses; the exact tile is measured, not guessed (P6-M2).
- The thumbnail frame is one pixel, grey, around the picture only (P6-M2).
- Ctrl+Shift+3 for medium icons; the view choice in the dialog has three values (P6-M2).
- The cursor follows the band and PageUp and PageDown in the list ride in P6-M2 from the Phase
  4 close's list, as they bite daily.
- The toolbar's default is `"top"`; Delete recycles; the icons are the shell's stock images;
  New file makes `New Text Document.txt` on Ctrl+Alt+N and opens the rename editor (P6-M3).
- The bundle also carries the Back button over a text entry, a P2-M3 leftover Jeremy was not
  asked about, since it is small and on the same list (P6-M3). The protected-files switch was
  put to him and kept; its shape (Ctrl+H for hidden only, protected per pane in the dialog with no
  key, system-only files always shown) is the lead's call (P6-M3).

## P6-M1: cloud marks and the icon items

The first milestone of the phase (2026-10-16): a file or folder in a cloud location shows a status
mark over its icon, the cloud for what is only in the cloud and the tick for what is on this disk,
in the details list, the 32 px list and the grid; the two P2-M5 icon items ride along. Branch
`feat/p6-m1-cloud-marks`, from b1a688d (the Phase 6 kickoff). The brief is the P6-M1 entry of the
kickoff above. One implementer (Opus 5.5); the lead reviews the PR.

### Settled first (2026-10-16, before any code)

The kickoff's three questions, answered with probes in this session (Windows PowerShell with C#
P/Invoke, each probe on its own MTA thread as the icon workers are, the placeholder mode set per
probe with `RtlSetProcessPlaceholderCompatibilityMode` and `RtlSetThreadPlaceholderCompatibilityMode`;
Windows PowerShell itself runs in expose mode, 2, so every "TwinFM's mode" run set disguise, 1,
which is what TwinFM gets: `phase-2-plan.md` § P2-M5 Deviations). The folders: `OneDrive` (13
entries), `OneDrive\Documents` (21, local files and folders of both states), `OneDrive\Archive
Docs_Online_only\...\jfoschi\15` (987 `.tif` files, every one cloud-only), `G:\My Drive` (125).

1. **OneDrive's overlay handlers mark nothing, in either placeholder mode.** `SHGetFileInfoW(path, 0,
   SHGFI_ICON | SHGFI_SMALLICON | SHGFI_ADDOVERLAYS | SHGFI_OVERLAYINDEX)`, TwinFM's call, gave overlay
   index 0 for all 987 cloud-only files and for every local file and folder, with disguise and with
   expose, on an MTA thread and on an STA thread; only `Personal Vault.lnk` and `Jeremy's
   Notebook.url` got an index (2, the link arrow). `FileSyncShell64.dll` (OneDrive 26.178) was loaded
   in the probe each time, so its seven `ShellIconOverlayIdentifiers` were asked and answered no.
   Explorer's marks come from elsewhere: the shell property `System.StorageProviderState` (read
   through `SHGetPropertyStoreFromParsingName`), which the property's own description maps to
   imageres.dll icons (read through `PSGetPropertyDescription` and its enumeration): 1 "Available
   when online" -1404 (the cloud), 2 "Available on this device" -1400 (the outlined tick), 3 "Always
   available on this device" -1405 (the solid tick), 4, 5 and 10 "Sync pending" and 6 "Syncing"
   -1401, 7 "Error" -1402, 8 "Warning" -1403, 9 "Excluded (not synced)" no image. The same store's
   `System.StorageProviderUIStatus` names `System.PropList.StatusIcons =
   prop:System.StorageProviderState;System.StorageProviderCustomStates` with
   `StatusIconsDisplayFlag = 3`: these are the Status column's icons and the ones Explorer draws over
   the icons in its icon views. **So exposing placeholders cannot help the marks and stays off**
   (`PHCM_EXPOSE_PLACEHOLDERS` is never set; enumeration, `stat_entry` and the file operations see
   what they saw in Phase 5). Google Drive's handlers do answer: every entry of `G:\My Drive` but its
   `desktop.ini` got index 12, which the image list draws as the blue cloud
   (`GoogleDriveCloudOverlayIconHandler`; nothing in My Drive is kept offline here).
2. **What a lookup costs.** The overlay index call, one thread, per file: the 987 cloud-only files
   3.0 to 3.9 ms (p50 2.7 to 3.5 ms, p99 4.5 to 7.1 ms, the first call 80 to 125 ms; 3.0 to 3.9 s for
   the folder), the same in both modes; `G:\My Drive` 4.2 to 4.9 ms p50 (0.74 to 0.84 s for 125). The
   property read that does carry OneDrive's state: 23 to 24 ms per entry p50 (24.9 s for the 987,
   GPS_DEFAULT; `GPS_FASTPROPERTIESONLY` fails with 0x80004005 for this property), about seven times
   the index call. P2-M5's 568 ms was neither.
3. **The attributes decide a file's state exactly; they say nothing about a folder's.** In disguise
   mode the cloud filter hides the reparse point and `FILE_ATTRIBUTE_OFFLINE` but keeps
   `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`: all 987 cloud-only files carry it and read state 1, every
   local file lacks it and reads 2 (Documents 5 of 5, the root 1 of 1, the archive folder 1 of 1), and
   OneDrive's own files (`Personal Vault.lnk`, `.849C9593-...`) carry both `PINNED` and `UNPINNED` and
   read 9 or nothing. Folders carry none of these: Documents' 5 folders in state 1 and 10 in state 2
   have the same attributes (no `RECALL_ON_OPEN` either). A pinned file (state 3) was not available
   to check without changing a file in the cloud; `FILE_ATTRIBUTE_PINNED` is the documented mark for
   it. So a file's mark costs no call at all, and only folders are looked up, with the property.

The record of the probes (scripts and their output) is in the session scratchpad, not the repo.

### What landed

- **Cloud locations** (`src/cloud.rs`, `icons::Cloud`). Decided per listing on the UI thread
  (`main.rs` `cloud_of`, from `navigate` and `refresh`; the watch's stat jobs ask the same
  `Locations::of` quietly), never per file: a folder at or under a registered sync root is
  `Cloud::SyncRoot`; one on a volume named "Google Drive" is `Cloud::Overlays`; This PC, archive
  levels, search tabs, UNC paths and mapped network drives are `Cloud::None`. The sync roots are
  every `UserSyncRoots` value under `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\
  SyncRootManager\*` (two here: OneDrive and Phone Link's `CrossDevice\Jeremy Pixel 10`), read at
  start and again on every device change (`devices_changed`, the debounced `WM_DEVICECHANGE` volume
  path, which also forgets the volume names). A volume's name is read once per volume key
  (`dnd::volume_key`, P5-M3's mount-point key, then `GetVolumeInformationW`) and cached; the root
  test is component-wise (`c:\onedrive2` is not under `c:\onedrive`).
- **Keys** (`icons::Mark`, `marked`, `key_mark`, `IconKeys::with_cloud`). Outside a cloud location
  every key is as before (one extra comparison per entry on the enumeration worker). Inside one, the
  key carries the mark: `ext:<mark>:.txt`, `folder:<mark>`, `thumb:<mark>:<path>|<modified>|<size>`,
  and `cloud:<mark>:<path>` for an entry keyed by its full path (an `.exe`, a customised folder such
  as OneDrive's own Documents), with the mark `s<N>` (a storage provider state) or `o<N>` (a shell
  overlay index). So the type icons stay shared per (type, mark): the 987 `.tif` files are one key,
  `ext:s1:.tif`. No mark (state 9, overlay 0) is today's key. Links (`.lnk`, `.url`), junctions,
  symbolic links, app aliases, other reparse points and drives keep their own keys and the shell's
  arrow (`wants_overlay`), so a shortcut in OneDrive shows the arrow, not a status.
- **Where a mark comes from.** A sync root file: its attributes, on the enumeration worker
  (`state_from_attrs`: recall on data access or offline, 1; pinned and unpinned both, none; pinned,
  3; else 2), so it costs nothing and is in the key when the listing lands. A sync root folder and
  every Google Drive entry: a lookup. Such an entry is listed with `look:<s|o>:<plain key>|<path>`;
  when its row is on screen (plus the usual margin) `resolve_icons` asks the icon workers for it as
  it asks for an icon, and the worker makes one call (`storage_state`: the property store's
  `System.StorageProviderState`; or `overlay_index`) and replies `Outcome::Looked(mark)` with the
  row; the UI thread (`drain_icons`) gives the entry `looked`'s key in the listing and its row
  (`PaneModel::relabel`, found at the row or else by key), and the next resolve asks for that key's
  icon. A lookup is never `taken` (it runs every time a row asks), so a refresh or a watch record,
  which list the entry again with its `look:` key, look it up again; an icon key is still rendered
  once per process. Lookups are bounded by what is scrolled to, not by the folder's size.
- **The marks' images.** `Overlay(i)`: the shell's overlay image from the system image list, the
  P2-M5 path. `State(s)`: imageres.dll's icon for the state (`STATE_GLYPHS`, `SHDefExtractIconW` at
  the glyph size) drawn into the bottom-left corner of a transparent icon square
  (`state_glyph`, `icon_pixels_at`), `glyph_px` square: 9 px over a 16 px icon, 18 over 32, a quarter
  of a tile's 96 (24 px). Both are fetched once per (mark, size) per process
  (`Pool::overlays`, now keyed by `Mark`) and composed by the existing `overlay`, so nothing is
  drawn by hand and the strips are unchanged.
- **Thumbnails** (`icons::Thumb`, `Mark::cache_only`). A thumbnail request whose key carries the
  cloud state (1) or any Google Drive overlay goes to `IThumbnailCache::GetThumbnail` with
  `WTS_INCACHEONLY`, so a cached thumbnail shows and nothing is extracted; without one the row shows
  its type's icon with the mark. Google Drive's index does not tell its handlers apart (only the
  cloud one fires here), so every Google file is held to the cache.
- **`[cloud]` and `[icons]` lines**: `[cloud] sync roots: N (...)`, `volume <key> named "..."
  (google drive|not cloud)`, `pane=N <folder>: sync root|google drive`; `[icons] status glyph state S
  (imageres -R) fetched at G px over P px`, `slow cloud lookup T ms: <path>`, and `cloud N` on the
  once-a-second line after `dropped D`. No new driver step.
- **The two P2-M5 items.** The one `SHGetFileInfoW` per overlaid key is made once per key per
  process: the mark is in the key and the key is `taken` once, and the scripted refreshes below show
  `overlays 0` with every key already held (the lookups, which are per row, are what a refresh
  repeats). "Other" reparse points checked on a real one: a Unix domain socket bound in
  `%TEMP%\twinfm-p6m1-wof\sock` (C# `socket(AF_UNIX)` and `bind`; `fsutil reparsepoint query` shows
  tag 0x80000023, which `Kind::from_tag` maps to `Reparse`) gets overlay index 0 from the shell, so
  it shows its plain icon; the rule (ask the shell, draw what it gives) stays. A WOF-compressed file
  (`compact /c /exe:xpress4k`) is not a reparse point to a process (the WOF filter hides the tag:
  "not a reparse point"), and neither ProjFS (`ProjectedFSLib.dll` absent) nor a WCI point can be
  made here.
- **Tests**, 176 to 183: `cloud::tests::cloud_location_rule` (normalising, component-wise roots,
  `\\?\` paths, a UNC path never asking for a volume name); `cloud::tests::reads_this_machines_sync_roots_and_volume_names`
  (reads the registry and the system drive's label only; one name lookup per volume);
  `icons::tests::cloud_mark_keys` (every key form, `key_mark` against drive-letter paths such as
  `S:\` and `o:\`, the `look:` round trip, the thumbnail cap's fallback keeping the mark);
  `sync_root_state_from_attributes` (the attribute rule, `cache_only`, `glyph_px`);
  `cloud_location_keys` (no location: today's keys; a sync root: shared marked keys per type and
  state, folders looked up, links and junctions untouched, thumbnails marked; Google Drive: every
  entry looked up); `pool_keeps_cloud_thumbnails_to_the_cache_and_looks_up_every_time` (through the
  `spawn_with` seam: the render function sees `Thumb::CacheOnly` for a cloud-only and a Google file,
  `Extract` otherwise; a lookup asked twice is made twice and carries its row; lookups are not
  renders; the table interns nothing for one); `status_glyphs_come_from_imageres` (reads imageres.dll:
  every state's glyph at 16, 32 and 96 px is visible and only in its corner, the cloud differs from
  the tick); `model` `relabel` inside `resolve_icons_requests_each_key_once` (found by key after a
  pass moved it, the row then asks for the marked key). No test needs a cloud folder.
- **OneDrive in the sidebar** (after the console check, 2026-10-17): a Places row "OneDrive" after
  Home, the `FOLDERID_SkyDrive` known folder (`C:\Users\jtkli\OneDrive` here), so the folder the
  marks are for is one click away as in Explorer's navigation pane. Known folders that resolve to a
  path that does not exist are now left out (OneDrive's on a machine without it set up). The sidebar's
  rows below Home move down by one; no script or soak aims at a Places row by number.
- **Docs**: `CLAUDE.md` (test count, the cloud lines and key forms under § Measuring, the reference
  build `target-p5m3`).

### Decisions

- **OneDrive's marks are its storage provider state, not a shell overlay.** Settled-first (1): the
  overlay path gives nothing for OneDrive in any mode, and the state is what Explorer shows. The
  glyphs are the Windows images Explorer's Status column draws for each state, so they look as
  Explorer's do without our own drawing.
- **A file's state from its attributes, a folder's from the property.** (3) shows the attributes
  agree with the property for files, at no cost; a property read per file would be 23 ms per row on
  a worker, a full screen of 40 rows about half a second on the two workers before the marks land.
  Folders are few in most listings, so their read is affordable, and nothing else tells their state.
- **Lookups ride the icon queue and only for rows on screen.** The workers already serve the rows on
  screen first (newest first) and skip what a pane navigated away from; a separate pass over a
  whole folder would make a 10,000-file Google folder 50 s of shell calls whoever scrolls. The cost
  is that a row's mark lands a few milliseconds after the listing, with the placeholder in between,
  as icons do.
- **The row keeps the placeholder until its marked icon lands**, rather than first showing the plain
  type icon: a `look:` row never resolves (its key is no icon), and its key changes once. Without a
  per-row flag in `resolve_icons` (per-row work the amendment forbids without a trace) there is no
  way to show the plain icon and still ask again.
- **Every registered sync root counts, not only OneDrive's.** The state property and its glyphs are
  Windows' own for any cloud files provider (Phone Link's CrossDevice folder is one here); filtering
  by provider name would need a list of names.
- **Google Drive's thumbnails come from the cache only, whatever the file's state**: the index alone
  does not say whether a file is offline, and an extraction of a streamed file downloads it.
- **The glyph sizes** (9 / 18 / 24 px over 16 / 32 / 96) are ours: Explorer draws the status glyphs
  in its icon views but not over the details list's 16 px icons (it has the Status column there);
  half the icon for the lists matches where the shell's link arrow sits, a quarter for the tiles keeps
  the picture visible.

### Deviations

- **The kickoff's mechanism for OneDrive ("drawn by the overlay path the links already use",
  `ext:<index>:<ext>` with the overlay index) cannot work**: the handlers answer index 0 for every
  OneDrive file (settled-first 1). OneDrive's marks are the storage provider state's glyphs instead,
  and the key's mark is `s<state>`; Google Drive's follow the kickoff (`o<index>`, the overlay path).
  What Jeremy sees is what he asked for (the cloud, the tick, Explorer's own images), but **two
  things differ from the kickoff's "unfiltered" call** and are his to accept: (a) a OneDrive *file*
  that is syncing, pending, in error or with a warning shows the tick, because its attributes do not
  say so and a per-file property read was not paid for (folders do show those states); (b) the
  glyph's size over the 16 px list icon is our choice, since Explorer shows no glyph there. Asked in
  the PR.
- **Key forms**: `ext:s1:.txt` and `ext:o12:.txt` rather than `ext:1:.txt` (two kinds of mark), and
  `cloud:<mark>:<path>` and the thumbnail form, which the kickoff did not name.
- **`[icons]`'s `cloud N` counts lookups only**, so for OneDrive files it stays 0 (they need none);
  the lookups of folders and of Google Drive entries are what it shows.
- **The soak's OneDrive writer** was not added (the kickoff's default: a console check only; the
  fence stays `%TEMP%`).

### Deferred

- **The sync states of OneDrive files beyond tick and cloud** (syncing, pending, error, warning):
  a property read per file on the workers (23 ms each), if Jeremy wants them.
- **Google Drive's overlay at tile size** is the shell's jumbo overlay drawn over the whole 96 px
  picture, as the link arrow in the grid has been since P4-M1: a large cloud covering the tile's lower
  left (screenshot `shots/run3.png`). A smaller glyph would mean choosing the overlay image list by
  glyph size instead of icon size, which changes the link arrow too; a console check decides.
- **A mark goes stale between a change and the next record or refresh** (the kickoff's recorded
  limitation). A hydration or a "Free up space" changes a file's attributes; whether the folder's
  watch delivers a record for that (so the edit path stats the file again and its key changes) was
  not checked, since it means changing a file's state in Jeremy's OneDrive: a console check. Google
  Drive entries and sync root folders are looked up again only on a refresh, a navigation or a
  record that touches them.
- **A refresh of a Google Drive folder (or of a sync root's folders) puts the rows on screen back
  to the placeholder** until their lookups return (the new listing carries `look:` keys, which never
  match the marked keys a refresh would keep): about 5 ms a row on two workers, a screen of 40 rows
  in about 0.1 s, then every marked key is already held. Keeping the old icon would need the old
  mark carried across the refresh by name.
- **A shell command from the context menu freezes the window while it runs** (seen at the console on
  2026-10-17: "Create shortcut" on `G:`, picked by accident, left a white window for about a minute
  before the shell's dialog came up; Jeremy cancelled it). Not new in P6-M1: `shellmenu::show` calls
  `IContextMenu::InvokeCommand` on the UI thread, as it has since Phase 1, so a verb that waits on a
  slow file system (Google Drive's virtual volume here, most likely) holds the window. Not
  reproduced by script (the driver never invokes a shell verb, by design, and creating a shortcut on
  `G:` would write into Jeremy's Google Drive). Running the shell's verbs off the UI thread is the
  item the kickoff left out of Phase 6 ("the shell menu's verbs through TwinFM's jobs"); Jeremy sent it to
  Phase 7 on 2026-10-17 (§ Recorded for Phase 7).
- **A renamed Google Drive volume loses its marks** (the kickoff's open question, unchanged).
- **The thumbnail cache's 300 to 570 ms on Google Drive** per cache-only request (`[icons] thumbnail
  ... ms` in the grid run below), on the workers; the shell's own cost of asking the cache about a
  file on that volume, not an extraction (nothing downloaded: the OneDrive check below).

### Scripted checks

Harness session (no console session attached: frame interval p50 about 47 ms, about 21 Hz), 1216 x
800 window, scale 1.0, this worktree's `target\release\twinfm.exe` copied to the session scratchpad
and run from there; stderr kept; screenshots with `SLINT_BACKEND=winit-software` and `PrintWindow`
(the femtovg window captures black). Exit code 0 and no `panicked` line in every run.

- **(i) OneDrive and Google Drive side by side**, `nav 0 C:\Users\jtkli\OneDrive\Documents; split;
  nav 1 G:\My Drive; sleep 5000; key F5`: `[cloud] sync roots: 2 (c:\users\jtkli\crossdevice\jeremy
  pixel 10; c:\users\jtkli\onedrive)`, `[cloud] pane=0 ...\OneDrive\Documents: sync root`, `[icons]
  status glyph state 2 (imageres -1400) fetched at 9 px over 16 px`, `status glyph state 1 (imageres
  -1404) ...`, `[cloud] volume g: named "Google Drive" (google drive)` (logged as `g::` before the
  wording fix), `[cloud] pane=1 G:\My Drive: google drive`, `[icons] overlay index 12 fetched at 16
  px`; the first listing of My Drive made `cloud 72` then `cloud 16` lookups (88 for the rows on
  screen and their margin of 125), `overlays 11` (one per marked key: the types and folders), one
  `slow cloud lookup 107 ms: ...\Documents\DeDRM` (the first property read); the F5 made `cloud 39`
  and `cloud 5` with `overlays 0` and `worker 0 rendered`: the lookups again, no icon fetched twice.
- **(ii) The cloud-only files** (`nav 0 ...\Archive Docs_Online_only`, software renderer,
  `shots/run2.png`): the eight folders show the cloud or the tick from their lookups, the seven
  `Jeremy_Backup_*` files and the two `.xlsx` files the 9 px cloud over their type icons; My Drive's
  folders and files Google's cloud.
- **(iii) The grid with thumbnails** (`TWINFM_CONFIG=C:\traces\grid-thumbs.toml`, read-only:
  `[settings] file C:\traces\grid-thumbs.toml (read)`, `[view] pane=0 starts as large icons`, `[icons]
  list geometry: 32 px rows, 32 px icons`; `shots/run3.png`): `OneDrive\Documents`' folders with 24 px
  ticks and clouds, its files with the tick, `Jeremy's Notebook.url` with the arrow only;
  `G:\My Drive\Wedding`'s 23 files with Google's cloud, four of them (two `.docx`, two `.jpg`) with
  real thumbnails from the cache, the rest their type icons; `thumbs 34`, `cloud 33` and the
  `thumbnail 302 ms` to `571 ms` lines of Deferred.
- **(iv) Nothing is downloaded.** `C:\traces\thumbs.toml` (read-only) on the 987-file cloud-only
  folder, scrolled through with `drag 0 300 1500` and refreshed: `thumbs 34`, `31`, `34`, `28`
  (127 cache-only requests, `dropped 341` for rows scrolled past), `[icons] status glyph state 1
  (imageres -1404) fetched at 18 px over 32 px`, `cloud 0` (files need no lookup); counted with
  `Get-ChildItem` before and after the run: 987 of 987 files still carry
  `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`. The F5: `+0 requests +0 replies`, the keys kept.
- **(v) The lists are the reference's.** The generated `app.rs` of `target-p5m3` (074ccb2) and this
  build's, with quoted drive paths blanked (the checkout paths Slint embeds), differ in 0 lines: no
  `.slint` file changed, and the whole generated app, not only the list and grid components, is
  the reference's.

### Measurements

In-app, RELATIVE only: the same harness session, no console session attached (frame interval p50
about 47 ms, so these busy figures sit far below a console run's and say nothing against the bar),
1216 x 800 window, scale 1.0, accessibility on, the gate scenario (`tools/wpr-gate.ps1`'s script) on
`C:\traces\bigdir`, `[busy]` over the 10 s after the split, `target-p5m3` (074ccb2, the reference)
and this build interleaved by one runner, in two sets per settings file: the reference in the first
slot of each pair, then the candidate first. No cargo or agent ran beside.

Defaults, six pairs per set:

| Pair | set 1: reference first | set 1: P6-M1 second | set 2: P6-M1 first | set 2: reference second |
|---|---|---|---|---|
| 1 | 7.0 | 8.4 | 12.6 | 9.3 |
| 2 | 13.2 | 8.3 | 10.0 | 10.6 |
| 3 | 16.1 | 8.6 | 11.4 | 9.0 |
| 4 | 9.7 | 10.3 | 14.0 | 7.2 |
| 5 | 10.4 | 6.7 | 7.8 | 15.4 |
| 6 | 8.4 | 8.1 | 5.1 | 9.5 |
| **median** | **10.05** | **8.35** | **10.7** | **9.4** |
| composes | 371 strips at 21.1 to 21.8 us | 371 at 21.5 to 22.3 us | 371 at 21.4 to 22.6 us | 371 at 21.2 to 22.3 us |

Thumbnails (`TWINFM_CONFIG=C:\traces\thumbs.toml`, read-only; every candidate log shows `[settings]
file C:\traces\thumbs.toml (read)` and `[icons] list geometry: 32 px rows, 32 px icons`), four pairs
per set:

| Pair | set 1: reference first | set 1: P6-M1 second | set 2: P6-M1 first | set 2: reference second |
|---|---|---|---|---|
| 1 | 7.8 | 9.2 | 10.3 | 9.0 |
| 2 | 6.8 | 8.3 | 5.8 | 8.0 |
| 3 | 8.7 | 6.1 | 8.3 | 7.8 |
| 4 | 8.4 | 8.6 | 7.8 | 4.5 |
| **median** | **8.1** | **8.45** | **8.05** | **7.9** |
| composes | 251 strips at 36.3 to 38.3 us | 251 at 37.4 to 39.7 us | 251 at 36.5 to 37.8 us | 251 at 36.6 to 38.0 us |

Over all twelve defaults pairs the reference's median is 9.6 and the candidate's 8.5; over all eight
thumbnails pairs 7.9 and 8.3. Each sits inside the other's spread (defaults 7.0 to 16.1 against 5.1
to 14.0; thumbnails 4.5 to 9.0 against 5.8 to 10.3), and the first slot of a defaults pair read
higher in eight of twelve pairs whichever binary sat in it (the P4-M2 note's run-order bias, the
other way round in this session). What the scenario runs of the branch: one registry read at start
(`[cloud] sync roots: 2 (...)`), one volume name for `c:` (`[cloud] volume c: has no name (not
cloud)`), one `Locations::of` per listing, and on the enumeration worker one comparison per entry;
no `[cloud] pane=` line (bigdir is no cloud location), and the strips compose the same number of
times in the same time. The console triplets decide.

### Gate

For Jeremy, elevated at the console, accessibility on, Narrator off, no other TwinFM window: a gate
build of the PR head from a detached checkout with `--locked` into `target-p6m1`, then

```
tools/wpr-triplet.ps1 -Reference p5m3 -Candidate p6m1,p6m1@C:\traces\thumbs.toml -Tag ab22
```

(the defaults triplet and the thumbnails triplet; the thumbnails runs are labelled `p6m1-thumbs`).
Before reading the thumbnails triplet, each `p6m1-thumbs` run's stderr must show `[settings] file
C:\traces\thumbs.toml (read)` and `[icons] list geometry: 32 px rows, 32 px icons`; every candidate
run should show `[cloud] sync roots: 2 (...)` and `[cloud] volume c: has no name (not cloud)` and no
`[cloud] pane=` line, `cloud 0` on every `[icons]` line, and no `panicked` line.

Run on 2026-10-17 (09:56 to 10:05), Jeremy elevated at the console, accessibility on, Narrator off,
no other TwinFM window, through a one-off wrapper (`gate-p6m1.ps1`, not in the repo: the preconditions,
the triplet script of the detached checkout, then `analyze-trace.py --symbols --exe` and the stderr
checks on every run). `target-p5m3` (074ccb2) against `target-p6m1` (0d4a23f, the PR head, built with
`--locked` from the detached checkout `.claude\worktrees\gate-p6m1`), each round reference, defaults,
thumbnails. Every candidate run's stderr shows `[cloud] sync roots: 2 (...)`, no `[cloud] pane=` line
and `cloud 0`; the thumbnails runs `[settings] file C:\traces\thumbs.toml (read)` and `[icons] list
geometry: 32 px rows, 32 px icons`; all nine show the two `watching` lines and no `panicked` line.
Traces `C:\traces\twinfm-ab22-*.etl`, reports under `C:\traces\export\`, summary
`C:\traces\gate-ab22-summary.txt`.

| Run | reference `target-p5m3` | P6-M1 defaults | P6-M1 `thumbs.toml` | in-app (ref / defaults / thumbs) |
|---|---|---|---|---|
| 1 | 18.4% (1835 ms) | 18.7% (1868 ms) | 14.9% (1490 ms) | 19.8 / 19.7 / 14.2 |
| 2 | 18.9% (1891 ms) | 18.8% (1881 ms) | 14.9% (1489 ms) | 18.3 / 19.8 / 16.6 |
| 3 | 19.0% (1904 ms) | 19.1% (1907 ms) | 15.2% (1518 ms) | 20.3 / 21.2 / 14.5 |
| **median** | **18.9%** | **18.8%** | **14.9%** | |

**P6-M1 passes the gate.** Defaults: every run of both binaries under the 20% bar, the runs interleave
(the reference's 18.4 below the candidate's 18.7, its 19.0 above the candidate's 18.8), and the medians
differ by 0.1, so neither half of the rule calls a gap. Thumbnails: 14.9 to 15.2, read against the same
reference runs as in P3-M5 and P4-M2 (32 px rows, so fewer rows to draw: not like for like). Frame
shares, reference runs 1 to 3 / defaults runs 1 to 3 (sampled ms in the window): accessibility 207,
225, 215 / 219, 218, 207; render 1053, 1087, 1117 / 1090, 1090, 1108; draw_text 605, 607, 661 / 636,
627, 619; ListView rows 358, 371, 352 / 352, 360, 357; timers 662, 698, 673 / 678, 678, 682; heap 202,
178, 179 / 202, 172, 175. No `twinfm::cloud` frame and none of the key functions (`marked`,
`key_mark`, `looked`, `is_look_key`) appear in any candidate's window: the scenario lists no cloud
folder, and the strips compose as the reference's (371 strips at 24.6 to 25.3 us against 25.0 to 25.1;
251 at 46.2 to 48.2 us with thumbnails). Peak working set 150 to 153 MB in every run but the
reference's first (164 MB). The in-app lines (14.2 to 21.2) do not decide.

**Console checks, 2026-10-17, Jeremy at the console:** `OneDrive\Archive Docs_Online_only` in File
Explorer (details, Status column) beside `G:\` in TwinFM; Google Drive's marks show in TwinFM, and
taking a Google Drive file offline and back online changes its mark as expected. OneDrive could not
be compared: TwinFM's sidebar had no OneDrive row (Explorer lists it as "Jeremy - Personal"), so the
OneDrive row above was added and the OneDrive checks are to be repeated with it. Jeremy prefers the
mark on the icon to a Status column, which answers the glyph question for the lists (the marks stay
over the 16 and 32 px icons). The "Create shortcut" freeze is under Deferred.

**OneDrive console checks, 2026-10-17, Jeremy at the console** with this worktree's build (0cd85e6,
the sidebar's OneDrive row): "Console checks for onedrive pass as expected" (the OneDrive items of the check list below:
the marks beside Explorer's and a file's state change). **P6-M1 is closed**; it merges on
Jeremy's word. The gate triplet ran on 0d4a23f; the commit after it adds the sidebar row only
(outside the lists, not in the gate scenario), so the triplet was not repeated.

Console checks after the triplet, TwinFM started unelevated:

1. `C:\Users\jtkli\OneDrive\Documents` in TwinFM beside the same folder in File Explorer (details
   view, then Large icons; TwinFM's grid with Ctrl+Shift+2): the folders' ticks and clouds match
   Explorer's Status column one for one, the files show the outlined tick, `Jeremy's Notebook.url` the
   shortcut arrow.
2. `C:\Users\jtkli\OneDrive\Archive Docs_Online_only` the same way: the cloud on the `Jeremy_Backup_*`
   files and the two `.xlsx` files.
3. A file's state changed by hand in Explorer ("Always keep on this device" on one small file, then
   "Free up space"): in TwinFM the mark should go from cloud to the solid tick and back, after the
   watch's record or an F5 (whether the watch alone follows is unchecked, Deferred).
4. `G:\My Drive` beside Explorer: Google's cloud on every entry, in the list and in the grid (where
   it is drawn large, as the link arrow is; say whether that is acceptable).
5. An F5 on `G:\My Drive`: the `[icons]` lines show `cloud N` lookups and `overlays 0`, and
   `worker 0 rendered`.
6. Whether the 9 px glyph over the 16 px list icons reads well, or should be larger or absent in the
   details list (Explorer shows none there).

## P6-M2: the views finished

The second milestone (2026-10-17): PageUp and PageDown in the details list, the grid's medium tiles,
a frame around the tiles' thumbnails, the view in the settings dialog, and the cursor following the
band. Branch `feat/p6-m2-views`, from 5212b87 (main after P6-M1 and its two docs PRs). The brief is
the P6-M2 entry of the kickoff above. One implementer (Opus 5.5, the session that wrote this); the
lead's review is the PR's.

### Settled first (2026-10-17, before any code)

- **Explorer's tiles, measured** with UI Automation on a scratch folder (`%TEMP%\twinfm-p6m2-explorer`,
  twelve `.txt` files with 31-character names and a folder; Explorer at 96 dpi, Ctrl+Shift+3 then
  Ctrl+Shift+2): medium items are 74 px wide on an 82 x 99 px pitch (98 px tall with a name on two or
  three lines, 68 px with one); large items 105 px wide on a 110 x 132 pitch. Explorer wraps names;
  TwinFM's tiles hold one elided line (P4-M1), so the medium tile keeps the large tile's layout at
  the medium icon: 2 px, the 48 px icon, 1 px, the 19 px name, 2 px = **80 x 72 px**, a 76 px name,
  the icon centred (16 px each side). The kickoff's "around 64 x 84" assumed a narrower tile with
  more room under the icon; 64 px leaves a 60 px name, about seven characters.
- **The icon size is 48 px**, the shell's `SHIL_EXTRALARGE` and one of the thumbnail cache's sizes,
  so neither the icons nor the thumbnails are scaled at 100% (the open question, answered below).

### What landed

- **PageUp and PageDown in the details list** (`pane.slint` `FileListHost.key`): the grid's handler,
  no longer limited to the grid, moves the cursor by `max(1, floor(visible height / row height))`
  lines times `step` (1 in the list, the tiles to a row in the grid), clamped to the first and last
  entry; Shift extends the range from the anchor (`cursor-moved`), and the cursor row is scrolled into
  view as Up and Down do. With Ctrl held the key passes, so Ctrl+Shift+PageUp / PageDown reach the
  window's tab keys from either list (in the grid the list used to take them: a latent P4-M1 bug).
  Driver key names `ShiftPageUp`, `ShiftPageDown` (`PageUp`, `PageDown` existed).
- **Medium tiles** (`icons::TileGeom`, `LARGE` 96 x 120 at 96 px and `MEDIUM` 80 x 72 at 48 px, icon
  2 px from the top). `GridGeom` carries its tile; `State::tile` holds each pane's, `Layout::tile`
  the window's view of it (`window_layout` reads `left-medium` / `right-medium`), and every reader of
  the old `TILE_W` / `TILE_H` / `GRID_ICON_PX` constants takes the pane's tile instead: `line_of`,
  `resolve_view`, `compose_grid_rows` (the ring of slots is sized by the tile's height), `set_view`,
  `dnd::hit`, `Layout::row_px_of`, the band (`band::touched`), `shellmenu::tile_menu_point`, the drag
  image (`ImageGeom::tile(scale, tile)`: 80 px with the 48 px picture out of medium tiles, 128 px with
  96 px out of large ones) and the driver's `row_pos`. In Slint the medium grid is its own component,
  `FileGridMedium` / `FileGridMediumList`, a copy of `FileGrid` / `FileGridList` with the medium
  constants (`x: c * 80px`, the name at 51 px, 76 px wide); `FileListHost` gains `medium`, `tile-w`
  and `tile-h` (its `cols` and `row-h` follow them) and picks the large or the medium list with `if
  root.grid && !root.medium` / `if root.grid && root.medium`, outside any `for`; the pane's overlays
  (the drop highlight, the grid's empty space, the rename editor: `list.tile-h - 25px` down, `list.tile-w
  - 4px` wide) read `list.tile-w` and `list.tile-h`. Ctrl+Shift+3 picks medium (`set-view` now takes
  0, 1 or 2); Ctrl+Shift+1 and Ctrl+Shift+2 keep their meaning; the driver's `view medium` presses it.
- **The view is `settings::View`** (`Details`, `Large`, `Medium`): `Config::view(pane)` reads
  `view_*` and the new `tile_left` / `tile_right` (`"large"`, the default, or `"medium"`; anything
  else large; a file from before P6-M2 has a large grid), `Config::set_view` writes them, keeping the
  tile size while the pane shows details, so Ctrl+Shift+1 and back to the grid returns to the same
  size. `[view]` lines name `details`, `large icons` or `medium icons`, and `[view] pane=N starts as
  medium icons` at start.
- **The settings dialog's View row**: a ComboBox per pane ("Left: details", "Left: large icons",
  "Left: medium icons", and the same for the right), beside the hidden files; staged as `view_left` /
  `view_right` (`details|large|medium`, also the driver's `settings set view_left medium`), applied on
  OK through `set_view`, the path the keys take (`[settings] dialog: view_left details -> medium`).
  The dialog is 38 px taller (368 px).
- **The frame around thumbnails in the tiles** (`icons::FRAME`, a neutral grey (190, 190, 190), one
  logical pixel, at least one physical): the worker reports where a thumbnail's picture lies in its
  square (`letterbox_rect`, `Outcome::Picture(pixels, rect)`; a type icon, including the icon a
  thumbnail request falls back to, stays `Outcome::Pixels`), the table keeps the rectangle per buffer
  (`IconTable::picture`, rescaled when the buffer is fitted to another size), and `compose_grid`
  draws the rectangle's edge pixels after copying the icon, dimmed with a cut icon. Large and medium
  tiles; never the 32 px lists. The medium grid asks for its thumbnails at 48 px through the P3-M5
  path, as the large one asks at 96.
- **The cursor follows the band** (`band::cursor`, `Band::cursor`): on every move that selects, the
  cursor goes to the touched tile under the band's moving corner (clamped into the touched columns
  and grid rows; in a partial last grid row, the last touched entry before it), and Shift's anchor
  goes there too (`PaneModel::set_anchor`), so the arrow keys, Shift+arrow and Shift+click continue
  from it (Explorer). `[band] pane=N done: K selected, cursor R`.
- **Icons asked again after a view switch** (`PaneModel::forget_icons`, from `set_view` for every
  tab of the pane): found while checking the medium grid. `resolve_icons` resolves an entry once per
  listing, so after Ctrl+Shift+2 the first screenful of the details list (the rows it had resolved,
  44 at the default window) kept its 16 px buffer, which `fitted` blew up to 96 px (the reference
  build shows it the same way, Scripted checks (iv)); the other rows asked at 96 px. Now every entry goes
  back to its placeholder on a switch and asks again at the new size; a key already held at that
  size is taken without a request (16 px and 96 px buffers both stay in the table, as P3-M5 keeps
  every key per size).
- **Tests**, 183 to 185: `band::tests::band_cursor_follows_the_moving_corner` (the cursor rule, large
  and medium); `icons::tests::tiles_frame_thumbnails_and_never_icons` (the frame on a letterboxed
  picture's edge and nowhere else, none on an icon, two physical pixels at 200%, dimmed with a cut
  icon, `IconTable::picture` rescaled); grown: `grid_compose_places_icons_in_their_tiles` (every scale
  at both tiles, the medium tile's positions), `pool_serves_others_while_one_icon_is_slow` (a picture's
  rectangle comes back from the worker), `worker_renders_exe_drive_and_extension` (the shell's PNG
  thumbnail arrives as `Outcome::Picture` with its rectangle), `band_touches_the_tiles_it_overlaps`,
  `dnd::tests` hit testing (medium tiles), `tile_drag_image_holds_the_tile_picture` (the 80 px image),
  `tile_menu_point_sits_under_the_tile` (medium), `view_defaults_to_details_and_round_trips_icons`
  (the `View` rules, a pre-P6-M2 file), `dialog_changes_are_the_differing_fields` (the view fields),
  `resolve_icons_requests_each_key_once_and_applies_replies` (`forget_icons`).
- **Soak**: `tools/soak-p5m3.ps1` gains the medium-grid cycle (see `CLAUDE.md`).
- **Docs**: `CLAUDE.md` (test count, `view medium`, the key names, the settings keys and dialog, the
  band's cursor, the frame, the soak).

### Decisions

- **Medium is a second component, not a tile size bound in the tile.** The P3-M1 amendment: nothing
  in a row or tile may depend on a runtime property. The large grid's generated code is the
  reference's (Scripted checks (v)); the cost is a 50-line copy in `pane.slint` that must change
  with `FileGrid`.
- **The frame is drawn at compose from a rectangle the worker reports**, not baked into the
  thumbnail's pixels. The table keys a buffer by key and size in px, and a 32 px list thumbnail at
  150% and a medium tile's at 100% are both 48 px: a frame baked in would show in the list after a
  move between monitors of different scale. The extra cost is the rectangle's edge pixels per tile
  composed (192 writes for a 48 px picture), on grid rows that are composed anyway.
- **PageDown moves the cursor a viewport**, as the grid's has since P4-M1 and as the brief says.
  Explorer's details view first moves the cursor to the last row on screen and pages only from there;
  not copied (Deferred).
- **The band's cursor is the tile under its moving corner**, so a band dragged down and right leaves
  the cursor bottom right and one dragged up and left top left, which is where Explorer leaves its
  focus rectangle. The anchor goes with it; Explorer's Shift+click after a band extends from that
  focused item.
- **A pane on details keeps its tile size** (`tile_*` is written only for a grid), so the size is the
  pane's preference and the view the current choice.
- **The dialog's choice is a ComboBox per pane**, the "one choice" of the brief; the labels carry the
  pane's name because the GridLayout row has one label for both controls.

### Deviations

- **The medium tile is 80 x 72 px, not about 64 x 84** (Settled first): Explorer's measured tile is
  wider than the kickoff's guess, and TwinFM's one-line names need no room for three lines.
- **Not in the brief**: the icons asked again after a view switch (a P4-M1 bug found here, fixed
  because the medium size made it worse: large to medium showed downscaled 96 px icons, medium to
  large upscaled 48 px ones), and Ctrl+Shift+PageUp / PageDown passing through the grid's list.

### Deferred

- **Explorer's PageDown in the details list** (to the last row on screen first, then a page).
- **Names on more than one line** in either tile (Explorer wraps up to three lines in medium tiles).
- **The ComboBox in the settings popup** is checked by script only through `settings set`; a mouse
  click opening its list inside the popup is for the console check.
- Accessibility of the medium grid, with the rest of the grid's (deferred for much later).
- **Escape clears the selection** (Explorer does; TwinFM's Escape in a list cancels the queued
  jobs, and in the grid a band under way, and leaves the selection). Found at the P6-M2 console
  check; Jeremy sent it to P6-M3's polish bundle on 2026-10-17 ("Put it in P6-M3").

### Scripted checks

Harness session (frame interval near a console's this time: the defaults pairs below read about 20%
busy), 1216 x 839 window, scale 1.0, this branch's `target\release\twinfm.exe` (d0db2ee plus the
docs) copied to the session scratchpad and run from there; stderr kept; screenshots with
`SLINT_BACKEND=winit-software` and `tools/shot.ps1`. Exit code 0 and no `panicked` line in every run.

- **(i) PageUp and PageDown in the details list** on `C:\traces\bigdir` (`click 0 0`, then `key
  PageDown|ShiftPageDown|ShiftPageUp|PageUp|End|Home` with `scrollpos 0` after each): the cursor goes
  0, 32, 64 (a viewport is 32 rows at this height), Shift+PageDown 96 with `[select] pane=0 Range
  row=96 selected=33`, Shift+PageUp back to 64 (`selected=1`), PageUp 32 and 0; from End (39999)
  PageDown stays at 39999 and PageUp goes to 39967; the content-y follows the cursor (-12, -716,
  -1420, ...). Then `key CtrlT`, `click 0 0`, `key CtrlShiftPageUp` with the list focused: `[tab]
  pane=0 moved tab 2 to 1`.
- **(ii) Medium tiles** (`view medium` on bigdir): `[view] pane=0 details -> medium icons (12 tiles
  to a row, 3334 grid rows)`; `click 0 5`, Right, Down: cursor 6 then 18; PageDown: 126 (nine grid
  rows of twelve); `band 0 990 10 500 150` from the gap right of the last column: `[band] pane=0 done:
  18 selected, cursor 30`, `scrollpos` reads current-row 30, and `key ShiftRight` gives `[select]
  pane=0 Range row=31 selected=2` (the anchor moved with the cursor); F2 opens the rename editor over
  tile 31's name (`shots`: `c-medium.png` in the scratchpad); `view icons`, `view details`, `view
  medium`, `view medium` log `medium icons -> large icons (10 tiles to a row, 4000 grid rows)`,
  `large icons -> details`, `details -> medium icons`, `already medium icons`.
- **(iii) Medium tiles with thumbnails, through the settings dialog**: `TWINFM_CONFIG` a scratch file
  with `thumbnails = true`, on `%TEMP%\twinfm-p3m5-thumbs` (the P3-M5 fixture, 200 JPEGs), `settings
  open`, `settings set view_left medium`, `settings ok`: `[settings] dialog: view_left details ->
  medium`, `[view] pane=0 details -> medium icons (12 tiles to a row, 17 grid rows)`, `dialog closed
  (ok), 1 change(s)`, the file saved with `view_left = "icons"`, `tile_left = "medium"`, `tile_right =
  "large"`; `thumbs 80 avg 16.29 ms` then `thumbs 62 avg 12.32 ms` (the 48 px thumbnails); every
  photo letterboxed in its 48 px square with the grey frame on the picture's edge, the selected tile's
  too (`d-medium-thumbs.png`).
- **(iv) Icons after a view switch**: the reference build (`target-p6m1`) on bigdir, `view icons`:
  the first 44 tiles show the 16 px `.txt` icon blown up to 96 px, the rest the 96 px icon
  (`ref-large.png`); this build: every tile the 96 px icon, one request at 96 px (`[icons] +1
  requests +1 replies (total 2 / 2)`, `e-large.png`).
- **(v) The lists and the large grid are the reference's.** The generated `app.rs` of `target-p6m1`
  (0d4a23f) and this build's, from `InnerFileList_root_60` up to the first component after the five
  lists (the reference's `FileListHost`, this build's `FileGridMedium`), with quoted drive paths
  blanked and two counters normalised (the global singletons' numbers, `FluentPalette_552` against
  `_650`, since every component added before them shifts them, and the compiler's
  `returned_expression` / `return_check_merge` temporaries): 7,654 lines each, 0 differing. The span
  holds `FileList`, `FileListNoType`, `FileList32`, `FileListNoType32` and `FileGridList` with their
  repeated rows and tiles (`InnerComponent_rectangle_63`, `_101`, `_138`, `_176`, `_213`, `_214`).
  The palette global itself gained fields (the ComboBox reads more of the palette); the host and the
  pane changed as intended.
- **(vi) Soak**: `tools/soak-p5m3.ps1 -Minutes 4`: exit code 0, 0 `panicked`, 5 cycles started, 127
  jobs done (0 failed, 0 cancelled), 0 soak refusals, 0 `waitops` timeouts; drags dropped 0; medium
  grid switched on 4 and off 4, 80 px drag images 4, bands done with the cursor on tile 0: 8 (large
  and medium), Shift+Right from the band's cursor 4; every P4-M3 and P5-M3 count as expected (tile
  drops 8, 128 px images 8, bands 8 of 8 done with 2 selected, spring-loaded rows 4, tabs 0, crumb
  drops 4); working set 149.7 MB at 5 s, 204.5 at 35 s, then 220.8 to 238.6 MB, 229.9 at 241 s (a
  plateau, as the earlier soaks' first minutes).

The UI Automation measure of Explorer used a scratch folder that is removed again.

### Measurements

In-app, RELATIVE only: the same harness session, 1216 x 839 window, scale 1.0, accessibility on, the
gate scenario (`tools/wpr-gate.ps1`'s script) on `C:\traces\bigdir`, `[busy]` over the 10 s after the
split, `target-p6m1` (0d4a23f, the reference) and this build interleaved by one runner, in two sets
per settings file: the reference in the first slot of each pair, then the candidate first. No cargo
or agent ran beside.

| Settings | pairs | reference first: ref / P6-M2 | P6-M2 first: P6-M2 / ref | median over all, ref / P6-M2 |
|---|---|---|---|---|
| defaults | 6 + 6 | 20.2/17.2, 17.5/16.2, 21.1/19.4, 16.4/23.1, 17.8/18.7, 21.1/19.4 | 18.4/20.0, 21.6/21.2, 19.4/17.3, 17.7/18.9, 21.6/21.6, 19.7/20.0 | **20.0 / 19.4** |
| `grid.toml` | 4 + 4 | 10.0/11.2, 11.1/11.4, 10.5/13.6, 10.5/12.5 | 10.5/10.0, 11.9/10.2, 10.6/9.8, 10.8/11.9 | 10.35 / 11.3 |
| `grid.toml`, again | 4 + 4 | 10.0/10.9, 10.2/11.4, 10.0/11.2, 12.2/12.5 | 9.7/11.6, 11.2/10.3, 11.7/11.9, 12.0/12.0 | 10.95 / 11.3 |
| `grid-medium.toml` | 4 + 4 | 10.3/13.6, 10.8/17.5, 12.2/16.2, 10.2/14.5 | 14.8/10.0, 15.0/11.6, 15.5/8.9, 15.0/10.8 | 10.55 / 15.0 |

Defaults: the two binaries interleave (16.4 to 21.6 against 16.2 to 23.1). The large grid: the
candidate reads higher in the second slot in all eight pairs and in the first slot in four of eight;
over the eight candidate-first pairs the medians are 11.0 (P6-M2) and 10.95 (the reference), so the
gap follows the slot (the P4-M2 note's run-order bias); the grid rows compose in the same time (1,422
rows at 72.7 to 79.2 us against 70.8 to 75.2 us: about 3.5 ms over a run, against 100 ms per point).
`grid-medium.toml`: the reference does not know `tile_*` and shows its large grid, so this compares
medium tiles with large ones, as P6-M1's thumbnails triplet did 32 px rows with 22 px: the medium
grid draws 12 tiles to a row on 11 visible grid rows against 10 on 6, about twice the tiles and
names on screen, and reads 4.5 points above the large grid here, about 4.5 below the details list.
Every candidate medium run shows `[settings] file C:\traces\grid-medium.toml (read)` and `[view]
pane=0 starts as medium icons`; the grid runs `starts as large icons`. The console triplets decide.

### Gate

For Jeremy, elevated at the console, accessibility on, Narrator off, no other TwinFM window: a gate
build of the PR head from a detached checkout with `--locked` into `target-p6m2`, then

```
tools/wpr-triplet.ps1 -Reference p6m1 -Candidate p6m2,p6m2@C:\traces\grid.toml,p6m2@C:\traces\grid-medium.toml -Tag ab23
```

(the defaults triplet, the grid triplet and the medium triplet; the settings runs are labelled
`p6m2-grid` and `p6m2-grid-medium`; `C:\traces\grid-medium.toml` is new, read-only, both panes
`"icons"` at `"medium"`). Before reading the settings triplets, each `p6m2-grid` run's stderr must
show `[settings] file C:\traces\grid.toml (read)` and `[view] pane=0 starts as large icons`, each
`p6m2-grid-medium` run's `[settings] file C:\traces\grid-medium.toml (read)` and `[view] pane=0
starts as medium icons`; no run may show a `panicked` line. The grid triplet reads against the
defaults reference runs as P4-M1's did (its reference is the list); the medium triplet the same way.

Run on 2026-10-17 (15:55 to 16:07), Jeremy elevated at the console, accessibility on, Narrator off,
no other TwinFM window, through a one-off wrapper (`gate-p6m2.ps1`, not in the repo: the
preconditions, the triplet script of the detached checkout `.claude\worktrees\gate-p6m2` at ca12be0,
then `analyze-trace.py --symbols --exe` and the stderr checks on every run). `target-p6m1` (0d4a23f)
against `target-p6m2` (ca12be0, the PR head, built with `--locked`), each round reference, defaults,
grid, medium. Every grid run's stderr shows `[settings] file C:\traces\grid.toml (read)` and `[view]
pane=0 starts as large icons`, every medium run's `[settings] file C:\traces\grid-medium.toml (read)`
and `starts as medium icons`; no `panicked` line in any of the twelve. Traces
`C:\traces\twinfm-ab23-*.etl`, reports under `C:\traces\export\`, summary
`C:\traces\gate-ab23-summary.txt`.

| Run | reference `target-p6m1` | P6-M2 defaults | P6-M2 `grid.toml` | P6-M2 `grid-medium.toml` | render p50 per frame (ref / defaults / grid / medium) |
|---|---|---|---|---|---|
| 1 | 24.4% (2440 ms) | 24.1% | 14.3% | 17.7% | 3.19 / 3.10 / 1.46 / 2.06 ms |
| 2 | 23.1% | 17.8% | 10.3% | 13.5% | 3.02 / 2.24 / 1.14 / 1.60 ms |
| 3 | 18.3% | 18.7% | 10.3% | 13.3% | 2.30 / 2.29 / 1.10 / 1.56 ms |
| **median** | **23.1%** | **18.7%** | **10.3%** | **13.5%** | |

**The session changed state part-way, so the medians are not like for like.** Every run kept the
console's cadence (frame interval p50 16.4 to 16.5 ms, 132 to 137 frames), but in four runs each
frame cost about a third more: all of round 1 (the reference 3.19 ms render p50, the candidate 3.10,
and the grid and medium runs above their own later runs, 1.46 against 1.10 to 1.14 and 2.06 against
1.56 to 1.60) and the reference's run 2 (3.02); from the candidate's run 2 on, every run sat at the
lower cost. The frame shares move together in those runs (render 1,437 to 1,474 sampled ms against
1,067 to 1,104, `draw_text` 829 to 864 against 631 to 663, `ListView` rows 401 to 421 against 337
to 375, timers 745 to 806 against 625 to 675), the pattern of a machine-wide slowdown, not of code
(the lists are the reference's: Scripted checks (v)). Read like for like: at the higher cost, the
reference 24.4 and 23.1 against the candidate's 24.1; at the lower, the reference 18.3 against
18.7 and 17.8. The candidate sits inside the reference's spread in both states, and the wrapper's
"a gap that counts: True" is the candidate's median 4.4 points **below** the reference's (the
wrapper tested the difference both ways; the rule is about a cost). The runs at or over 20% are the
three at the higher cost, the reference's two among them; at the lower cost every defaults run is
under the bar. Grid 10.3 and medium 13.5 (runs 2 and 3) against the defaults list's 17.8 to 18.7:
medium about 3 points above large, as the in-app pairs said (twice the tiles), and well under the
bar; frame shares, run 3 grid / medium: render 631 / 889, `draw_text` 298 / 505, `ListView` rows
39 / 68, accessibility 82 / 95 (the medium grid's extra cost is text: twice the names).

**P6-M2 passes the gate.** Jeremy accepted ab23 as recorded (2026-10-17: "Accept ab23 as recorded"),
without a re-run of the defaults triplet. The console checks follow.

**Console checks, 2026-10-17, Jeremy at the console with `target-p6m2`:** 1 (medium tiles and the
frame beside Explorer), 3 (PageUp / PageDown in the details list) and 4 (the band's cursor, the view
switches with no blown-up icons) "work as expected". 2 (the settings dialog's View row): "I don't see
a view row"; not reproduced here: the same build, with the GPU renderer and with a copy of Jeremy's
`config.toml` on the same desktop, shows the row (fourth, "Left: medium icons" / "Right: details",
matching the file's `tile_left = "medium"`). Looked at again: "The View row is there and the
dropdowns work; console checks all pass." Escape not clearing a selection was raised at the same
time; it is under Deferred, for P6-M3. **P6-M2 is closed**; it merges on Jeremy's word.

Console checks after the triplet, TwinFM started unelevated:

1. A photo folder (`%TEMP%\twinfm-p3m5-thumbs` or Pictures) with thumbnails on, Ctrl+Shift+3: the
   medium tiles beside Explorer's Medium icons (Ctrl+Shift+3 there too); the grey frame around each
   picture and none around the type icons; the size and the one-line names acceptable.
2. The settings dialog (Ctrl+,): the View row's two lists open with the mouse inside the dialog, a
   choice applies on OK and not on Cancel, and comes back after a restart.
3. The details list: PageDown and PageUp a screen at a time, Shift+PageDown extending the selection.
4. The grid (large and medium): a band drawn with the mouse, then Shift+Right or Shift+click
   continuing from the tile under the band's last corner; Ctrl+Shift+1, 2 and 3 back and forth with
   no blown-up icons after a switch.

## P6-M3: the toolbar and the polish bundle

The third milestone (2026-10-17): the toolbar in both placements, New file, and the polish bundle
(F2 for a bookmark, the breadcrumb dropdown's keyboard, restored tabs' state, the tab strip on
resize, search result names, folder sizes across a tab switch, the Back button over a text entry,
the protected-files switch), plus Escape clearing the selection (P6-M2 Deferred). Branch
`feat/p6-m3-toolbar-polish`, from b6ecc21 (main after P6-M2 and its two docs PRs). The brief is the
P6-M3 entry of the kickoff above. One implementer (Opus 5.5); the lead's review is the PR's.

### Settled first (2026-10-17, before any code)

Four places where the brief clashed with the code's rules or left a gap were put to Jeremy, each
with a recommendation; he took all four:

- **The button icons are Segoe Fluent Icons glyphs**, not the shell's stock images: Windows has no
  stock image for Cut, Copy or Paste (`SHGetStockIconInfo` has the folder, the document, rename and
  delete). The glyph font is the one Explorer's own command bar uses and ships with Windows 11; no
  shell call, sharp at every scale.
- **Search results show the file name first** (`file.txt  (sub\dir)`) instead of a Name column whose
  width follows the longest result: a width binding in every row is what the P3-M1 amendment rules
  out (0.7 of a point on the gate then).
- **A click on a sidebar row gives the sidebar the keys**, as Explorer's navigation pane takes them,
  so F2 has a bookmark to act on; the sidebar never took keyboard focus before.
- **Escape cancels queued jobs first**: it stops a search walk and cancels queued jobs when there are
  any, and only otherwise clears the selection.

### What landed

- **The toolbar** (`src/toolbar.rs`; pane.slint `ToolButton`, `ToolSeparator`, `Toolbar`): New file,
  New folder | Cut, Copy, Paste | Rename, Delete, each calling its key's command through
  `toolbar::command` and `main.rs` `command` (one path per command, the P6 amendment). The setting
  `toolbar` (`settings::Toolbar`): `"top"`, the default, a labelled bar 36 px tall across the window
  over the sidebar and the panes, acting on the active pane; `"pane"`, an icon-only bar 28 px tall at
  the top of each pane, above its tab strip and path bar, acting on that pane (first placed under the
  tab strip; moved above it after the console check, where a pane with a strip and one without had
  their bars at different heights); `"none"`. In the settings
  dialog as a "Toolbar" row (a ComboBox: none, across the top, in each pane; the dialog is 444 px
  tall). The window layout carries the bars (`Layout::top`, `Layout::bar`; `dnd::TOP_BAR`,
  `dnd::PANE_BAR`, `list_top`, `crumb_top`, and the new `tab_top`, `side_top`), so hit testing, the
  drop target, the band, the auto-scroll and the driver follow; a point on the top bar is no target.
  The in-pane bar is an `if` in the pane's layout, a sibling of the list, never a row property.
- **Which buttons apply** (`toolbar::state`, `main.rs` `update_tools`), set at the end of
  `update_status` and only when they change: in a folder New always, Paste with files on the
  clipboard, Cut, Copy and Delete with a selection, Rename with exactly one; in a search tab the same
  without New and Paste; nothing on This PC or in an archive level. The clipboard fact
  (`State::clip_files`, `IsClipboardFormatAvailable(CF_HDROP)`) is read at start and on every
  `WM_CLIPBOARDUPDATE`, our own writes included (`[toolbar] clipboard holds files`).
- **A press** (`toolbar_clicked`) makes the pane active and focuses its list, then runs the command;
  for Rename and the two New buttons it only makes the pane active, because the list's focus request
  lands a moment later (a `changed` handler) and closed the name editor again (found in the scripted
  check below).
- **New file** (`Job::NewFile`, `fileops::NEW_FILE_NAME` "New Text Document.txt"): Ctrl+Alt+N or the
  button opens the name editor (mode 3, as the folder's mode 2) with that name; Return makes the
  empty file through `IFileOperation::NewItem` with `FILE_ATTRIBUTE_NORMAL` and no template,
  `FOF_RENAMEONCOLLISION` numbering it as the shell numbers a new folder ("New Text Document
  (2).txt"), the cursor on it after; undone to the Recycle Bin whatever was written into it since
  (`must_be_empty: false`).
- **Escape** in a list: the band first (grid), then a search walk and the queued jobs; with neither,
  the selection is cleared (`clear_selection`; the cursor stays).
- **Hidden and protected files** (`model::Show`, `shows`): Ctrl+H (per tab, `show_hidden_*` as
  before) shows files with the hidden attribute; a file both hidden and system needs the pane's
  protected-files setting as well (`show_protected_left` / `show_protected_right`, default false, the
  dialog's "Protected files" row, applied to every tab of the pane); a file with the system attribute
  alone is always shown. The search walker enters folders by the same rule.
- **F2 for a bookmark** (app.slint `side-click`, `side-keys`): a left click on a sidebar row makes it
  current and gives the sidebar the keys (the row is lit while it has them); F2 on a bookmark row
  opens its title editor; Escape or Tab gives the keys back to the active pane's list, and a click in
  a list takes them as before.
- **The breadcrumb dropdown's keyboard** (app.slint `crumb-menu`): Down enters the list and moves a
  highlight, Up moves it back, Return picks the highlighted folder, Escape closes; the highlighted row
  is kept in view. Its key scope now takes the focus after the popup is shown (a token, as the
  settings dialog's: a `focus()` in `init` ran before the popup took the focus away).
- **Restored tabs** (`settings::SavedTab`): `left_tabs` / `right_tabs` are `[[left_tabs]]` tables
  with each tab's sort, direction, hidden switch, cursor row (by name) and scroll; a bare list of
  paths still loads (the defaults new tabs have). A restored tab's first pass after a complete listing
  puts the cursor on the saved name and the pass that lands applies the scroll
  (`Tab::restore_cursor`, `restore_scroll`); a navigation elsewhere first drops both. A saved Type
  sort with the Type column off sorts by name, as the global sort already did.
- **The tab strip on resize** (pane.slint `strip-resized`): when the strip's width changes (the
  window, the split, the splitter, the sidebar) Rust reveals the active tab again
  (`strip_resized`, logged when the offset moves).
- **Search result names** (`model::search_label`, `PaneModel::search_names`): the rows of a search
  tab read the file's name first and its folder under the root after it; the entries, the sort, the
  filter and every command keep the relative path.
- **Folder sizes across a tab switch** (`TabSizes::to_background`, `Tab::background_sizes`): a tab
  sent to the background keeps its finished sizes (its walks in flight and its stale sizes go); its
  background watch is a subtree watch while it has any; a change below a sized folder drops that
  folder's size there (no re-walk in the background); a tab past the watch cap, or whose watch is lost
  or refused, drops them all. A stale background tab with sizes stays in the watch set for them (its
  windows do nothing else; its listing is refreshed on show), because a change inside a sized
  subfolder also changes that subfolder's own entry, which makes the tab stale.
- **The Back button over a text entry**: a `TouchArea` behind the path entry and behind the rename
  editor takes the side buttons, which the `LineEdit` does not, so the pane's and the row's handlers no
  longer see them (Back over the breadcrumb, the header and the list still goes back).
- **Driver**: `toolbar <pane> <button>`, `newfile [name]` (key name `CtrlAltN`), `backclick <pane>
  path|edit`, `sidebar click <row>` as a left click with a negative row from the end, `settings set
  toolbar|protected_left|protected_right`.
- **Tests**, 185 to 190: `toolbar::tests::buttons_apply_where_their_command_would` and
  `each_button_calls_its_keys_command`, `settings::tests::toolbar_defaults_to_top_and_round_trips`,
  `model::tests::search_rows_show_the_file_name_first`,
  `fs::sizes::tests::a_background_tab_keeps_its_finished_sizes`; grown:
  `filter_hides_hidden_and_protected_unless_shown` (renamed), `filter_and_hidden_combine_in_the_pass_and_the_batches`,
  `hit_testing_and_drop_folder` (both bars), `tab_lists_round_trip_and_fall_back` (tables and the bare
  list), `dialog_changes_are_the_differing_fields`, the `fileops` flags and status tests and the real
  shell test (an empty new file, numbered on a collision), the undo table (a new file's undo).
- **Soak**: `tools/soak-p5m3.ps1` gains the toolbar cycle (see `CLAUDE.md`).
- **Docs**: `CLAUDE.md` (test count, the toolbar and its lines, the driver steps, the settings keys,
  the soak).

### Decisions

- **The toolbar's default is `"top"`, so every scripted run has it**, the gate's included: its lists
  are 36 px shorter than the reference's, and the defaults triplet measures that layout (as the
  brief intends: the defaults are what Jeremy runs).
- **Delete recycles**; Shift+Delete has no button (the kickoff's call).
- **Paste is enabled by `CF_HDROP` alone**, the format `clipboard::read` takes; a clipboard holding
  only a shell IDList (a copy inside a zip folder in Explorer) leaves it disabled, as Ctrl+V finds no
  files there either.
- **Search results keep their sort on the relative path**, so a Name sort groups them by folder; the
  label changed, not the order, which keeps the 40,000-row sort path untouched (a leaf-first compare
  would add a scan per comparison to every folder's sort).
- **Protected files need Ctrl+H too**, as Explorer's "Hide protected operating system files" takes
  effect only while hidden files show.
- **A background tab walks nothing**: a changed sized folder loses its size there rather than being
  walked again; it is walked when it is selected or sized again after the tab is shown.
- **The cursor is saved by name**, not by row, so a folder that changed between runs still finds the
  row (or none).

### Deviations

- **Glyphs, not stock images; file name first, not a wider column; the sidebar takes the keys**
  (Settled first).
- **Not in the brief**: the toolbar's focus rule for the editor buttons, the breadcrumb dropdown's
  focus token, a stale background tab with sizes staying watched, and `sidebar click -1`.

### Deferred

- **The Back and Forward buttons over a toolbar button** do nothing (the button's `TouchArea` takes
  the press), as over the tab strip and the scrollbar before.
- **Tooltips** on the icon-only buttons (Slint has none built in); the accessible names carry them.
- **Subtree records as edits** (P5-M1's piece left): a background tab with sizes goes stale on a
  change in its folder itself and is listed again when shown (the sizes stay).
- **Sorting search results by the file's own name.**
- **Up and Down in the sidebar**: the sidebar takes F2, Escape and Tab only.
- Accessibility of the toolbar beyond its buttons' names and roles (deferred for much later).

### Scripted checks

Harness session, 1216 x 839 window, scale 1.0, this branch's `target\release\twinfm.exe`; stderr
kept; screenshots with `SLINT_BACKEND=winit-software` and `tools/shot.ps1`. Exit code 0 and no
`panicked` line in every run.

- **(i) Both placements** (`%TEMP%\twinfm-p6m3-check`, split, `click 0 0`, then `settings set toolbar
  pane`): the labelled bar across the top and the icon bars in the panes render with the glyphs; with
  an empty clipboard Paste is dimmed, the right pane's Cut, Copy, Rename and Delete are dimmed with
  nothing selected there; `click 0 0` lands on row 0 under the top bar (the shifted hit test).
- **(ii) Every button** (`toolbar = "pane"` from a scratch config, split, scratch folders): Paste and
  Copy refused while disabled (`disabled, not pressed`); Copy then Paste across the panes (`Copied 1
  item`), Rename (`Renamed to c`), New file (`Created New Text Document.txt`), New folder, Cut and
  Paste (`Moved 1 item`, then `clipboard holds no files` after the paste of a cut empties it), Delete
  (`Deleted 1 item to the Recycle Bin`) and Ctrl+Z (`Undid: ...`); on This PC New file is refused as
  disabled. The first run of this check found the editor buttons' focus problem (above).
- **(iii) New file and Escape** (`newfile hello.txt`, `newfile` and Return, `key CtrlA`, `key
  Escape`, `undo`): `hello.txt` and `New Text Document.txt` made, `[select] pane=0 Escape: selection
  cleared`, the new file undone to the Recycle Bin.
- **(iv) Back over a text entry**: before the change `backclick 0 path` (the entry open) and
  `backclick 0 edit` (F2 open) both went back in the history; after it neither does, while
  `backclick 0 path` over the breadcrumb and `backclick 0` over the list still go back.
- **(v) The strip on resize**: nine tabs, the last one active, `split`: `[tab] pane=0 strip resized:
  scrolled to -581 (tab 9 of 9 in view)`.
- **(vi) The breadcrumb dropdown**: `crumbmenu 0 1` on `C:\Windows`, Down x3, Up, Return: `picked
  C:\Dell` (the second folder); a second dropdown closed with Escape, after which `key Down` selects
  row 0 of the list (the keys went back).
- **(vii) F2 for a bookmark**: `sidebar click 1` (a drive) then F2 does nothing; `sidebar click -1`
  (the bookmark) then F2, `Z`, Return: `[sidebar] renamed bookmark 0 (...): Some("Z")`; Escape gives
  the list the keys back.
- **(viii) Folder sizes**: two sized folders (`sizeall`), a new tab, a file added in `a` and then in
  `b` from the new tab, back to the first tab: `keeps 2 size(s) in the background`, `invalidated ...\a
  (added a\new.txt): tab in the background, size dropped`, the same for `b` after the tab went stale
  (it stayed watched for its sizes); selecting each walks it again with its new file count. In a run
  with only `a` changed, selecting `b` after the switch queued no walk (its size was kept).
- **(ix) Restored tabs** (a writable scratch config, `window close`, then a second run): saved
  `[[left_tabs]]` tables with `sort = "size"`, `ascending = false`, `cursor = "file_39831.txt"`,
  `scroll = -3600.0` for bigdir and `sort = "modified"`, `hidden = true` for `C:\Windows`; the second
  run lists `C:\Windows` by Modified with hidden files (`hidden=1`: one file both hidden and system
  kept out) and its cursor on row 3, and on `tab 1` bigdir by Size descending with `restored: cursor
  Some(169), scroll -3600 (now -3600)`.
- **(x) The lists are the reference's.** The generated `app.rs` of `target-p6m2` (ca12be0) and this
  build's, from `InnerFileList_root_` up to `InnerFileListHost_root_` (the six lists with their rows
  and tiles), with quoted drive paths blanked and every generated number inside an identifier mapped
  to its order of first appearance (the new toolbar components shift them all by 18): 9,218 lines
  each; the 148 lines a unified diff still shows are whole binding blocks emitted in another order
  (the compiler sorts them by their numbered names), and as multisets of lines (the field lists split
  into fields) the two spans hold the same 11,072 items, none on only one side.
- **(xi) Soak**: `tools/soak-p5m3.ps1 -Minutes 5` (a copy of this build): exit code 0, 0 `panicked`, 5
  cycles started, 168 jobs done (0 failed, 0 cancelled), 0 soak refusals, 0 `waitops` timeouts;
  drags dropped 0; toolbar presses 34 (four full cycles of eight and two of the fifth, which the
  five minutes cut short), 0 refused as disabled, new files 5 made and 5 undone, renamed to `t` 4;
  every P4-M3, P5-M3 and P6-M2 count as expected (tile drops 10, 128 px images 10, bands 10 of 10
  done with 2 selected, medium switches 5, 80 px images 5, bands with the cursor on tile 0 10,
  spring-loaded rows 4, tabs 0, crumb drops 4); working set 138.9 MB at 5 s, 237.3 MB at most, 212
  MB at 300 s (the earlier soaks' first minutes).

### Measurements

In-app, RELATIVE only: one harness session, 1216 x 839 window, scale 1.0, accessibility on, the gate
scenario (`tools/wpr-gate.ps1`'s script) on `C:\traces\bigdir`, `[busy]` over the 10 s after the
split, copies of `target-p6m2` (ca12be0, the reference) and of this build interleaved by one runner,
four pairs with the reference in the first slot and four with the candidate first, per settings
file. This session read low throughout (about 10% busy against the 18 to 20% of P6-M2's), so the
level says nothing; the pairs are read against each other. No cargo or agent ran beside.

| Settings | reference first: ref / P6-M3 | P6-M3 first: P6-M3 / ref | median over all, ref / P6-M3 |
|---|---|---|---|
| defaults | 12.8/12.6, 9.5/9.0, 11.2/11.2, 7.3/11.9 | 9.2/10.5, 8.6/9.7, 11.4/8.7, 10.6/12.0 | **10.1 / 10.9** |
| `toolbar-pane.toml` | 12.0/9.5, 12.0/10.5, 8.3/11.7, 9.7/8.7 | 8.1/11.8, 12.0/9.8, 11.2/12.0, 9.7/9.4 | **10.8 / 10.1** |

The binaries interleave in both sets (defaults 7.3 to 12.8 against 8.6 to 12.6; toolbar 8.3 to 12.0
against 8.1 to 12.0). In the defaults set the candidate has the toolbar across the top and the
reference none; in the toolbar set the reference ignores the key, so it is the reference's list
against the candidate's in-pane bars. Every toolbar run shows `[settings] file
C:\traces\toolbar-pane.toml (read)`; no run shows a `panicked` line. The console triplets decide.

### Gate

For Jeremy, elevated at the console, accessibility on, Narrator off, no other TwinFM window: a gate
build of the PR head from a detached checkout with `--locked` into `target-p6m3`, then

```
tools/wpr-triplet.ps1 -Reference p6m2 -Candidate p6m3,p6m3@C:\traces\toolbar-pane.toml -Tag ab24
```

(the defaults triplet and the toolbar triplet; the settings runs are labelled `p6m3-toolbar-pane`;
`C:\traces\toolbar-pane.toml` is new, read-only, `toolbar = "pane"` only). Before reading the toolbar
triplet, each `p6m3-toolbar-pane` run's stderr must show `[settings] file C:\traces\toolbar-pane.toml
(read)`; no run may show a `panicked` line. The defaults candidate has the toolbar across the top
(the default), the reference none; the toolbar triplet's reference is the same defaults reference
runs (the reference does not know the key). Compare the `[perf] ... render[... p50=...]` figure of
each run before reading the medians (the ab23 note).

Run on 2026-10-18 (09:19 to 09:33), Jeremy elevated at the console, accessibility on, Narrator off,
no other TwinFM window, through a one-off wrapper (`C:\traces\gate-p6m3.ps1`, not in the repo: the
preconditions, the gate build (built beforehand by the implementer from the detached checkout
`.claude\worktrees\gate-p6m3` at 02a9103 with `--locked`; the wrapper's own build was a no-op), the
triplet script of that checkout, then `analyze-trace.py --symbols --exe` and the stderr checks on
every run). `target-p6m2` (ca12be0) against `target-p6m3` (02a9103), each round reference, defaults,
toolbar. Every toolbar run's stderr shows `[settings] file C:\traces\toolbar-pane.toml (read)`; no
`panicked` line in any of the nine. Traces `C:\traces\twinfm-ab24-*.etl`, reports under
`C:\traces\export\`, summary `C:\traces\gate-ab24-summary.txt`.

| Run | reference `target-p6m2` | P6-M3 defaults (toolbar across the top) | P6-M3 `toolbar-pane.toml` | render p50 (ref / defaults / toolbar) |
|---|---|---|---|---|
| 1 | 19.1% | 18.5% | 20.2% | 2.19 / 2.19 / 2.26 ms |
| 2 | 20.3% | 18.6% | 19.8% | 2.29 / 2.24 / 2.33 ms |
| 3 | 22.4% | 21.3% | 23.0% | 2.96 / 2.88 / 2.96 ms |
| **median** | **20.3%** | **18.6%** | **20.2%** | |

**Round 3 ran at a higher cost per frame for all three binaries** (render p50 2.88 to 2.96 ms against
2.19 to 2.33 in rounds 1 and 2), the machine-wide shift ab23 recorded, so the rounds are read like for
like. Rounds 1 and 2: the reference 19.1 and 20.3, the defaults candidate 18.5 and 18.6 (below the
reference in both), the toolbar candidate 20.2 and 19.8 (between the reference's two). Round 3: the
reference 22.4, the defaults candidate 21.3, the toolbar candidate 23.0. Neither candidate has a gap
that counts (the wrapper's one-sided rule: no reference run below every candidate run, no median more
than a point above the reference's). The bar (under 20%) is met by the defaults candidate in both
like-for-like rounds; the toolbar candidate's 20.2 sits beside the reference's 20.3 in the same state,
and the round 3 runs over 20% are all three binaries at the higher cost. The defaults candidate draws
its lists 36 px shorter (the toolbar across the top) and reads a point or two below the reference;
the in-pane bars cost what the list's lost row saves, within the reference's spread.

**P6-M3 passes the gate.** The console checks follow.

**Console checks, 2026-10-18, Jeremy at the console with `target-p6m3`:** everything "seem[s] to be
working as expected" but one: with the toolbar in each pane, a pane with a tab strip had its bar 28
px lower than a pane without one ("The toolbar is not displayed on the same line when using it in
both tabs"). c632e93 moves the in-pane bar to the top of the pane, above the tab strip (`tab_top`
takes `Layout::bar`); a scripted check with three tabs on the left and one on the right shows both
bars on one line, row clicks landing in both panes and a hovering drag finding the left pane's tab
and crumb. With one tab per pane, the gate scenario's layout is the same before and after the move,
so ab24 stands; `target-p6m3` was rebuilt at c632e93 for the console, and `C:\traces\gate-p6m3.ps1`
now targets c632e93 as tag ab25 should a re-run be wanted.

Console checks after the triplet, TwinFM started unelevated:

1. The toolbar across the top (the default): each button against the left and the right pane (the
   active one), the disabled states (nothing selected, two selected for Rename, an empty clipboard
   for Paste, This PC, a search tab, inside a zip); New file and Ctrl+Z.
2. The toolbar in each pane (Settings, Toolbar: in each pane): the same against both panes, a press
   making its pane active; and Toolbar: none.
3. The bundle: F2 on a bookmark after clicking it; the breadcrumb dropdown with Down, Up, Return and
   Escape; Escape clearing a selection; the Back button over the path entry (Ctrl+L) and over the
   rename editor (F2) doing nothing; a search's rows reading name first; folder sizes still shown
   after a tab switch and back; protected files (Settings) with Ctrl+H; tabs, sorts, cursor and
   scroll back after a restart; a narrow window keeping the active tab in view.

## Open questions

Written 2026-10-16 at the kickoff.

- **Whether OneDrive's overlay handlers mark placeholders without `PHCM_EXPOSE_PLACEHOLDERS`**
  (P6-M1): the first thing the milestone checks. If they do not, the mode is turned on for the
  process and the record names every file API whose answers changed. **Answered (P6-M1, settled
  first):** they mark nothing in either mode; Explorer's marks are the storage provider state, so the
  mode stays off and OneDrive's marks come from that state (§ P6-M1 Deviations).
- **Whether Google Drive's volume is stable to find by name** (P6-M1): `G:` carries the volume
  name "Google Drive" on this machine; a user who renamed it would lose the marks. The process
  name of the mount's owner is not available from the volume. Acceptable for now; recorded.
- **A sync-root scratch folder for the soak** (P6-M1): the soak fence allows `%TEMP%` only. A
  folder under OneDrive would need the fence widened for the soak run and the files removed
  from the cloud afterwards. Jeremy's call; the default is a console check only.
- **Where the medium grid's thumbnails come from** (P6-M2): the shell's thumbnail cache has
  fixed sizes (16, 32, 48, 96, 256, 768, 1280, 1920, 2560); medium at 48 px hits one of them,
  which is the argument for 48. **Answered (P6-M2):** 48 px, through the P3-M5 path at the tile's
  icon size (§ P6-M2 Settled first).
- **The in-pane toolbar's height against the bar** (P6-M3): it takes a row from each list, so
  the gate scenario scrolls one row fewer per pane; a cost within the noise is expected, and the
  toolbar triplet says. **Answered (P6-M3, ab24):** the toolbar triplet's median 20.2% against the
  reference's 20.3%, within its spread in both session states (§ P6-M3 Gate).

## Phase 6 close

Written 2026-10-18. P6-M1 to P6-M3 are in, each squash-merged on Jeremy's word: P6-M1 cloud marks
and the icon items (#52, 2f256c5; gate build `target-p6m1`, 0d4a23f), P6-M2 the views finished
(#56, b8f3b06; gate build `target-p6m2`, ca12be0), P6-M3 the toolbar and the polish bundle (#59,
c927594; gate build `target-p6m3`, c632e93, the toolbar move after the triplet's 02a9103). The
phase ran from the kickoff (#51, b1a688d, 2026-10-16) to the last merge on 2026-10-18, three days.
Tests went from 176 to 190 (183 after P6-M1, 185 after P6-M2).

Phase 6's goal, Jeremy's three candidates in his order (cloud status marks and the remaining icon
items, the views finished, a polish bundle led by a toolbar), held: every item of the kickoff's
three entries is in, four calls in P6-M1 and four in P6-M3 changed the mechanism with Jeremy's
agreement, and the gate gave nothing back. Milestone by milestone:

- **P6-M1, cloud marks and the icon items** (#52, 2f256c5, from b1a688d). A listing in a
  registered sync root (OneDrive, Phone Link) or on a volume named "Google Drive" is a cloud
  location, decided once per listing on the UI thread; there a file's key carries its mark. OneDrive's
  marks turned out to be its storage provider state, not a shell overlay (settled first: the overlay
  handlers mark nothing, with or without `PHCM_EXPOSE_PLACEHOLDERS`, which stays off), drawn with the
  imageres.dll glyphs Explorer's Status column uses; Google Drive's are the shell's overlay index,
  looked up on the icon workers for the rows on screen only. Cloud-only and Google Drive files ask
  the thumbnail cache only, so nothing is downloaded. The two P2-M5 items rode along (one overlay
  call per key per process; "other" reparse points checked). OneDrive gained a Places row after the
  console check found none. Gate ab22: WPR medians 18.9% (`target-p5m3`) against 18.8% defaults and
  14.9% with thumbnails, every defaults run under the bar. Console checks: Google Drive's marks
  beside Explorer's and following a file taken offline and back; OneDrive's the same after the
  sidebar row; Jeremy prefers the mark on the icon to a Status column.
- **P6-M2, the views finished** (#56, b8f3b06, from 5212b87). PageUp and PageDown in the details
  list (a viewport of rows, Shift extending), medium tiles (80 x 72 px with the shell's 48 px icon,
  measured against Explorer's; a second grid component, so no tile binds to the size), a one-pixel
  grey frame around thumbnails in the tiles (drawn at compose from the rectangle the worker reports),
  the view in the settings dialog, the cursor following the band, and the icons asked again after a
  view switch (a P4-M1 bug the medium size made worse). Gate ab23: the session changed state part-way
  (four runs at about a third more per frame), so the medians (23.1 reference, 18.7 defaults, 10.3
  grid, 13.5 medium) were read like for like: at the lower cost the reference 18.3 against 18.7 and
  17.8. Jeremy accepted it as recorded. Console checks passed; Escape not clearing a selection was
  raised and went to P6-M3.
- **P6-M3, the toolbar and the polish bundle** (#59, c927594, from b6ecc21). The toolbar (seven
  buttons, each calling its key's command, across the window by default or in each pane or none,
  Explorer's glyph font), New file (Ctrl+Alt+N), Escape clearing the selection, protected files
  apart from hidden ones, F2 for a bookmark (a click on a sidebar row gives it the keys), the
  breadcrumb dropdown's keyboard, restored tabs keeping their sort, hidden files, cursor and scroll,
  the tab strip re-revealing its active tab on resize, search rows showing the file name first,
  folder sizes surviving a tab switch, and the Back button ignored over a text entry. Gate ab24:
  20.3% reference, 18.6% defaults (the toolbar across the top), 20.2% with the in-pane toolbar; round
  3 at a higher cost per frame for all three, read like for like. Console checks passed but one: a
  pane with a tab strip had its in-pane toolbar lower than a pane without, fixed by c632e93 (the bar
  above the tab strip; the gate scenario's layout unchanged, so ab24 stands).

Every milestone passed by the triplet rule against the previous milestone's build (WPR medians,
reference / candidate on defaults / candidate with the settings file):

| Milestone | Tag | Reference | Medians | Note |
|---|---|---|---|---|
| P6-M1 | ab22 | `target-p5m3` (074ccb2) | 18.9 / 18.8 / thumbnails 14.9 | the runs interleave; every defaults run under the bar |
| P6-M2 | ab23 | `target-p6m1` (0d4a23f) | 23.1 / 18.7 / grid 10.3 / medium 13.5 | a state change mid-triplet; like for like 18.3 against 18.7 and 17.8; accepted as recorded |
| P6-M3 | ab24 | `target-p6m2` (ca12be0) | 20.3 / 18.6 / in-pane toolbar 20.2 | round 3 slower for all three; like for like the candidates at or below the reference |

The lists' rows and tiles never moved: before each gate the generated list and grid components were
diffed against the reference's (P6-M2: 7,654 lines, 0 different; P6-M3: 11,072 items, none on one
side only, with block order the only difference once the numbers shifted). What the triplets
measured was the wiring: the cloud key rule's branch (P6-M1), the tile geometry read per layout
(P6-M2), the toolbar's place in the window or the pane and its button state set in `update_status`
(P6-M3). Since P6-M3 the gate scenario runs with the toolbar across the top (the default), so its
lists are 36 px shorter than before; the reference for the next gate, `target-p6m3`, has the same
layout, so the comparison is like for like again.

Lessons of the phase, each in its milestone's record and, where it changes how work is done, in
`CLAUDE.md` or the memory already:

- **A session can change state part-way through a triplet** (ab23, again in ab24's round 3): at the
  same 60 Hz cadence, some runs of every binary cost about a third more per frame. The `[perf] ...
  render[... p50=...]` figure per run says which runs share a state; read the medians like for like.
  In `CLAUDE.md` § Measuring since #58; the gate wrapper prints the figure per run since P6-M3.
- **Settle the brief against the code before writing it** (P6-M1 settled first; P6-M3 settled
  first): the kickoff's mechanism for OneDrive's marks was wrong (they are a provider state, not an
  overlay), and four P6-M3 items clashed with the code's rules (no stock images for Cut, Copy and
  Paste; a per-row width; a sidebar that never took the keys; two meanings for Escape). Each was put
  to Jeremy with a recommendation before any code, and each answer is quoted in its record.
- **A scripted check covers the layouts it sets up, not the ones a person uses** (P6-M3 console
  check): the in-pane toolbar's scripted checks ran with one tab per pane, so a pane with a strip
  beside one without was first seen at the console. A layout change is checked with the panes
  unequal (tabs, filter box, thumbnails) before it goes to the console.
- **The gate binary is built by the implementer every milestone** (Jeremy, 2026-10-18), with the
  gate wrapper handed over as one PowerShell file ready to run (P6-M2, 2026-10-17); Jeremy runs only
  the script and the console checks.
- **`gh pr merge --delete-branch` run from the session's own worktree deletes that worktree's files**
  (P6-M3 merge): the folder stays, locked, and the next git command there falls through to the main
  checkout. Merge from the main checkout, or expect it.
- **The protocol itself is unchanged**: `tools/wpr-triplet.ps1` elevated at the console, three runs
  per binary interleaved, compared by median, the one-point rule (one-sided: a cost), console checks
  after the triplet and unelevated, settings triplets for a milestone that touches the list, the
  grid, the icons or the pane's layout. Tags continue from ab25; the reference is `target-p6m3`
  (c632e93), and `target-p6m2` goes once it has served.

Open questions at the close (§ Open questions is above):

- **Google Drive's volume found by its name** (P6-M1): a renamed volume loses its marks; unchanged.
- **A sync-root scratch folder for the soak** (P6-M1): still a console check only; Jeremy's call.
- **The Slint whole-row snap patch and upstreaming the two accessibility patches**: still Jeremy's;
  nothing in Phase 6 touched the fork.
- **The dead share** (P5-M1): the one console check still owed from Phase 5.

Deferred, pulled together from the three milestones and the console checks, each with where it came
from. The Phase 5 close's list (`phase-5-plan.md` § Phase 5 close) still stands except for what this
phase finished (cloud marks, the P2-M5 icon items, the views list, the P2-M3 polish leftovers, folder
sizes across a tab switch, the search Name column); the Phase 4 and Phase 3 closes' lists stand as
the Phase 5 close left them.

- **Cloud marks** (§ P6-M1 Deferred): OneDrive's sync states beyond tick and cloud (syncing, error:
  a property read per file, 23 ms each); Google Drive's overlay drawn large over a tile (the shell's
  jumbo overlay, as the link arrow); a mark stale between a change and the next record or refresh
  (whether the watch alone follows a hydration is unchecked); a refresh of a Google Drive folder
  showing placeholders until the lookups return; the thumbnail cache's 300 to 570 ms per request on
  Google Drive (on the workers).
- **Views** (§ P6-M2 Deferred): Explorer's PageDown in the details list (to the last row on screen
  first); names on more than one line in the tiles.
- **Toolbar and the bundle** (§ P6-M3 Deferred): Back and Forward over a toolbar button do nothing;
  no tooltips on the icon-only buttons (Slint has none built in); subtree records as edits (a
  background tab with sizes goes stale on a change in its folder itself and is listed again when
  shown); search results sorted by the file's own name; Up and Down in the sidebar.
- **Accessibility**, deferred for much later by Jeremy's word (2026-10-15): not a Phase 7 candidate.

Phase 7 candidates, in no agreed order, for Jeremy's decision. Only the first is decided. Sizes in
this project's units: a milestone is what P6-M2 or P6-M3 was; a small item is an afternoon.

- **Shell verbs off the UI thread** (decided 2026-10-17, "Run the shell-menu fix as phase 7 item";
  § Recorded for Phase 7 holds the design notes): a verb picked in the context menu runs on a
  dedicated STA worker that rebuilds the menu there and invokes by verb string, so a verb waiting on
  a slow file system (Google Drive's "Create shortcut", a minute of white window) no longer holds
  the UI thread. A milestone, with a scripted seam (a test verb that sleeps on the worker while the
  lists scroll).
- **The shell menu's verbs through TwinFM's jobs** (Phase 5 candidate, left out of Phase 6): Delete,
  Cut, Copy and Rename picked in the menu routed to TwinFM's own jobs (undo records, progress, the
  soak fence), with **Escape cancelling a cut**. It sits beside the item above (the same menu code,
  the routing decided on the UI thread before the verb goes to the worker). Half a milestone.
- **The compact Windows 11 context menu and menu icons** (P2-M3): `IExplorerCommand` and the icons
  the shell gives each item. A milestone; it touches the same menu, so it pairs with the two above.
- **The drag cursor beside the description** (P5-M3): a console check first, then perhaps
  `GiveFeedback` setting the cursor. A small item, or nothing.
- **Content search** (P3-M2; out for now since 2026-10-13): text inside files in the search tab. A
  milestone at least, with encoding, size limits and binary files to settle first.
- **Archives beyond zip** (P3-M4): other formats through the shell's handlers, writing into
  archives. A milestone for the formats alone.
- **Thumbnails beyond the listing** (P3-M5): search tabs, archives, the details list's drag image.
  Half a milestone.
- **Cloud polish** (P6-M1 Deferred): the sync states beyond tick and cloud if wanted, a smaller
  overlay in the tiles, the hydration check. Half a milestone after Jeremy's word on each.
- **The polish leftovers of Phases 5 and 6** (above): toolbar tooltips and side buttons, subtree
  records as edits, search sorted by name, sidebar arrows, Explorer's PageDown, multi-line tile
  names. Each small; a bundle as P6-M3 was.
- **The Slint fork**: the whole-row snap patch with its own triplet, and the upstreaming decision.
  Jeremy's.
- **Daily use**: TwinFM in place of Files for a stretch of days, with what bites written down as the
  milestones. Phase 6's last finding (the toolbars not lining up) and P6-M2's (Escape) came from the
  console, not from a plan.
