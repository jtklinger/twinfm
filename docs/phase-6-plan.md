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
  which is the argument for 48.
- **The in-pane toolbar's height against the bar** (P6-M3): it takes a row from each list, so
  the gate scenario scrolls one row fewer per pane; a cost within the noise is expected, and the
  toolbar triplet says.
