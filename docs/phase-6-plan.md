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

## Open questions

Written 2026-10-16 at the kickoff.

- **Whether OneDrive's overlay handlers mark placeholders without `PHCM_EXPOSE_PLACEHOLDERS`**
  (P6-M1): the first thing the milestone checks. If they do not, the mode is turned on for the
  process and the record names every file API whose answers changed.
- **Whether Google Drive's volume is stable to find by name** (P6-M1): `G:` carries the volume
  name "Google Drive" on this machine; a user who renamed it would lose the marks. The process
  name of the mount's owner is not available from the volume. Acceptable for now; recorded.
- **A sync-root scratch folder for the soak** (P6-M1): the soak fence allows `%TEMP%` only. A
  folder under OneDrive would need the fence widened for the soak run and the files removed
  from the cloud afterwards. Jeremy's call; the default is a console check only.
- **Where the medium grid's thumbnails come from** (P6-M2): the shell's thumbnail cache has
  fixed sizes (16, 32, 48, 96, 256, 768, 1280, 1920, 2560); medium at 48 px hits one of them,
  which is the argument for 48.
- **The in-pane toolbar's height against the bar** (P6-M3): it takes a row from each list, so
  the gate scenario scrolls one row fewer per pane; a cost within the noise is expected, and the
  toolbar triplet says.
