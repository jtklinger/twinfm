# Phase 12: TwinFM's look finished, and its cloud drives

Written 2026-10-25, the day after 0.2.0 was released (`phase-11-plan.md` § Phase 11 close; release PR
#112, the checksum fix #113). Phase 12's goal, chosen by Jeremy on 2026-10-25: **finish the look Phase
10 started, from its "later" list, then OneDrive's syncing and error icons and Google Drive.** The same
day the known bugs and wanted features became one list, `docs/roadmap.md`; the items this phase takes
are its § Phase 12 and leave the roadmap when this plan merges.

What Jeremy decided at the kickoff (2026-10-25), his words:

- **The scope**: "Plan phase 12 by starting with the "later" list from phase 10, then add in the
  onedrive sync icons and improving the look of google drive."
- **The roadmap**: "Let's create a roadmap.md with these known bugs/enhancements." and "to the
  roadmap, add a treesize viewer so we can see what folders are the largest consumers of capacity."
  The TreeSize view is on the roadmap, not in this phase.
- **Go**: "All right, let's proceed with creating the phase 12 plan."
- **What is wrong with Google Drive**, his answer to the kickoff's question, with a log of the attempt:
  "Files can't be marked available for offline use." The log shows the cause (§ P12-M6): Google
  Drive's own commands in the classic menu are refused on the verb worker.
- **The additions** the lead proposed, all three taken: "Easier-to-see scrollbars", "The "+" new-tab
  button", "The memory check". Not taken: the rubber band in the details list.
- **The "+" with one tab**: "Always show the tab strip".
- **The sidebar's group**: "Recent".
- **A bookmark made with the mouse**, his own roadmap item of the same day: "Include it with m5".

The starting point is main at a64b537 (0.2.0 and the checksum fix). Three read-only surveys of the code
(2026-10-25) and the Phase 10 canvas (https://claude.ai/artifact/CeGZSFc6YSVEwqSydHqHc4, its Parts and
Modern boards) found what each item rests on today:

- **Hover fills** (`src/ui/theme.slint`, `pane.slint`, `app.slint`): about twenty chrome controls change
  their fill or border on hover with no animation; the one `animate` in the UI is the Switch knob
  (theme.slint:100). File rows and tiles have no hover. Sidebar rows (`SideRow`) and the crumb
  dropdown's rows hover inside ListViews, which scroll; the sidebar carries `cache-rendering-hint`.
- **Tooltips**: three, all Slint's built-in `Tooltip` with custom content (the toolbar's `ToolButton`, the
  content toggle and the search button in pane.slint). At the pinned rev the compiler wraps any content
  in `ToolTipImpl`'s Fluent frame (alternate background, 1 px Fluent border, 4 px radius), which the app
  cannot recolour; the 500 ms delay and 8 px offset are fixed.
- **The status bar** (pane.slint:1691-1697, `StatusInfo { left, mid, right }`, `statusbar.rs`,
  `update_status` in main.rs): three 12 px texts in a 28 px line, the selection as "M selected, <size>"
  in `mid` with the folder-size variants. The canvas's Parts board draws "10 items" muted, the count as
  a muted-fill badge, the size as an outline badge, 22 px tall.
- **The preview column** (preview.slint:214-337): a 32 px header whose title has no colour set (Fluent's
  default text colour), a hairline under it, the messages in `Theme.fg-muted`, the "Enter password…"
  button a theme `Btn`; the PDF page fill `#FFFFFF` (paper, kept) and the page note `#444` hard-coded.
- **Scrollbars**: every one is the Fluent `ScrollView`'s (the eight details components and four grid
  components in pane.slint inherit `ListView`; the sidebar and crumb dropdown lists; the preview's
  `body` ScrollView and Markdown `ListView`). Its `ScrollBar` (fluent/scrollview.slint:39-150 in the
  fork's checkout) is **not exported**: a 14 px overlay area whose thumb is 2 px wide at rest and 6 px on
  hover, coloured `#FFFFFF14` in dark and `#00000073` in light; the app can set only the scrollbar
  policy (as-needed, always-on, always-off). It does not fade. The driver's `thumb()` (auto.rs:725-730)
  aims at the pane's right edge less 10 px.
- **TwinFM's own questions**: four Windows dialogs, three `MessageBoxW` and one hand-built
  `DialogBoxIndirectParamW`: the replace question for a zip (`ask_conflicts`, fileops/archive.rs:205)
  and the permanent delete inside a zip (`confirm_delete`, :232), both on the file operation worker; the
  masked password dialog (`ask_password`, :529), on the file operation worker for a zip and on the preview
  worker for a PDF; and "Open N items at once?" (`ask_open_many`, main.rs:4216) on the UI thread. The
  replace, zip delete and open-many questions are skipped only under `TWINFM_SOAK` (`no_confirm`), so a
  scripted run outside the soak would show them; the two password questions have a script path
  (`zippassword`, `pdfpassword`) that refuses, never shows, when the script gave no password, so a script
  never waits for a person. Nothing in TwinFM has a worker wait for an answer from the UI thread: every
  `invoke_from_event_loop` is fire and forget. `raise_job_dialogs` (main.rs:2571) raises every `#32770`
  window of the process, TwinFM's own questions included.
- **Row heights**: compile-time constants in each component (22 px rows with 16 px icons, 32 with 32 for
  thumbnails; pane.slint:459-461 says they are copies on purpose, P3-M1's 0.7-point cost of a per-row
  switch). `FileListHost` picks one of six components; the icons are drawn by the strip overlay that Rust
  composes from `icons::ListGeom` (icons.rs:58-73). P9-M1's spike measured a **width** binding read when
  a row is made as free (`phase-9-plan.md` § The spike); a height binding was never measured. The
  canvas's density lever drew compact rows at 30 px and default at 36.
- **The sidebar** (app.slint:22-90 `SideRow`, :783-923; `sidebar.rs::compose`): Drives, Places (eight
  known folders, OneDrive among them) and Bookmarks, 32 px rows; collapsed (F9 or its chevron) it is a
  12 px strip. Bookmarks move only by the menu's Move up / Move down (`sidebar::move_bookmark`); no
  pointer drag on rows. No list of recent folders exists anywhere (only each tab's own history).
- **The tab strip** (pane.slint:1368-1419) shows with two tabs or more; Ctrl+T opens the active tab's
  own folder next to it (`open_tab(pane, None)`, main.rs:5397). The canvas puts a 32 px ghost "New tab"
  button with Lucide's plus right after each pane's strip; `src/ui/icons/plus.svg` is in the tree, unused.
- **OneDrive** (`cloud.rs`, `icons.rs`): a sync-root file's mark comes from its attributes (cloud,
  outlined tick, solid tick); folders, and files being freed, are looked up by a property read
  (`System.StorageProviderState`, 23 ms) and show every state, the syncing arrows (state 6, pending 4, 5
  and 10), error (7) and warning (8) among them. Files that are syncing or in error keep their attribute
  mark because Windows hides placeholders from TwinFM's process, so the in-sync test always reads 0
  (`phase-9-plan.md` § P9-M4 Deferred). OneDrive is set up on this machine (personal,
  `C:\Users\jtkli\OneDrive`).
- **Google Drive** (`cloud.rs`, `icons.rs`): recognised by its volume label "Google Drive"; every entry
  looked up for the shell's overlay (Google's blue cloud, `o12`, 4 to 5 ms each), drawn full size over 16
  and 32 px icons and at two thirds on the lower left over tiles (Jeremy kept that in P9-M4); every file
  asked for its thumbnail with `WTS_INCACHEONLY`, so a photo shows a picture only when Google Drive has
  one cached (4 of 23 in the P6-M1 check); `G:` sits in the sidebar's Drives as "Google Drive (G:)", with
  no Places row as OneDrive has. On this machine `G:` reports FAT32 with a 256-character limit and remote
  storage, its file system driver is `googledrivefs31931`, and `HKCU\Software\Google\DriveFS\Share`
  holds the mount point in an undocumented binary value; the root holds `My Drive` and
  `.shortcut-targets-by-id`. **Its own commands do not run** (Jeremy's log, 2026-10-25, `twinfm.log`
  of 0.2.0): Google Drive's items exist only in the classic menu (Show more options: "Offline access >"
  with "Available offline" and "Online only", "Manage versions with Google Drive", "Share with Google
  Drive" and the rest); a pick there is run on the verb worker, which builds the menu again and invokes
  the command by its id when the rebuilt item at that id has the picked label (`shellmenu::run_verb`,
  `choose`, `fill_until` waiting up to 3 s for a late fill). Both picks were refused: `"Manage versions
  with Google Drive" not in the menu after 3034 ms` and `the rebuilt menu has "Online only" at that id,
  not "Available offline"`, each with the pane's note "... was not run". The ids had moved by one in the
  rebuilt menu (the item one after the picked one sat at its id); the UI thread's menu lists "Move to
  OneDrive" twice, one plausible cause. Every watch of `G:` also logs `no device notification for G:\:
  The service has returned a service-specific error code. (os error 1066)` (P11-M3's registration of a
  hot-pluggable fixed drive, refused by Google Drive's volume).
- **Memory**: no heap tooling exists in `tools/`; the Phase 11 soak's creep (`phase-11-plan.md` § Phase
  11 close) is unexplained.

## The milestones

Seven milestones in two streams, with a coordinating session, as Phase 11 ran (the lead's proposal;
§ Open questions). Within a stream the order is the order of work; the streams run at once.

**The look stream** (one session; touches `src/ui/*.slint`, `src/statusbar.rs`, the chrome's callbacks in
`src/main.rs`, `src/fileops/archive.rs` and `src/preview.rs` for the questions, `src/dnd.rs`,
`src/auto.rs`, `src/icons.rs`'s `ListGeom`, `src/settings.rs`, `src/sidebar.rs`, `src/tabs.rs`):

1. **P12-M1, the look's small pieces.** Everything in it stays inside today's boxes (the Phase 10 rule:
   no height, width or position moves, so no driver aim or soak changes). **A fade on hover**: `animate
   background 100ms` (and the border or text colour where the hover changes it) on the chrome's hover
   fills: `Btn` and `Choice` in theme.slint; the toolbar's buttons, the tab segment and its close button,
   the crumb labels and chevrons, the header cells, the search box's buttons and the path-mode button in
   pane.slint; the menu items, menu rows and menu tool buttons, the sidebar footer and the splitter and
   preview handles in app.slint. Never on sidebar rows or the crumb dropdown's rows (they scroll), and
   file rows and tiles have no hover. **Tooltips in the look**: the card colour, a hairline border, a
   6 px radius, 12 px text in `Theme.fg`, the soft shadow; settled first between painting the look over
   `ToolTipImpl`'s frame and a tooltip of TwinFM's own (one overlay in app.slint shown by a 500 ms timer at
   the hovered control), whichever hides the Fluent frame wholly; the `[toolbar] ... tooltip` line stays.
   **The status bar's badges**, as the Parts board draws them: the item count muted, the selection count
   a muted-fill badge ("2 selected"), its size an outline badge ("1.3 MB", the folder-size variants inside
   it), 22 px tall inside the 28 px line, so `LIST_BOTTOM_GAP` stands; `StatusInfo` carries the count and
   the size apart (`statusbar.rs` splits what `mid_text` joins today). **The preview column's frame**: the
   title in `Theme.fg` at weight 500, the messages as the look's empty state (a Lucide icon above the muted
   text: file for "No preview", lock for a password, cloud for a cloud-only file, triangle for a damaged
   file), the page note's `#444` a theme token; the paper stays white. Each piece a commit of its own.
   Tests: the status text split. Soak: unchanged (nothing new to drive). Gate: the defaults triplet and the
   preview triplet (`C:\traces\preview.toml`); both schemes checked by scripted screenshots
   (`TWINFM_SCHEME`). Console checks: each hover in light and dark, the three tooltips, the badges with a
   selection of files and of sized folders, each preview message.
2. **P12-M2, scrollbars easier to see** (Jeremy's request of 2026-10-23, taken 2026-10-25). Since the Fluent bar
   takes no colour or width, two ways, settled first by a spike on one details list with a trace (the bar
   is drawn every frame while the list scrolls): **TwinFM's own scrollbar** (a component in theme.slint
   laid over each `ListView` and `ScrollView`, the Fluent bar turned off with `always-off`, bound to the
   view's `viewport-y`, `viewport-height` and `visible-height`, with a thumb drag, a click on the track
   paging, and a wider thumb on hover; the lead's recommendation, by the Phase 10 lesson "draw the look's
   own controls rather than restyling a toolkit's"), or **a fork patch** to `fluent/scrollview.slint`
   (wider thumb, stronger colour; a third patch on the `rustfm` branch, `docs/patches/` regenerated and
   the `rev` bumped, which also changes what upstreaming would carry). The thumb 6 px at rest and 10 px
   on hover, in a colour with at least 3:1 contrast on the card in both schemes, a faint track. Every list
   kind gets it: the details lists, both grids, the sidebar, the crumb dropdown, the preview's pages (both
   bars at a zoom over 100%) and its Markdown list. The driver's `thumb()` and `drag <pane> <dy> <ms>`
   aim at the new thumb; the grid's 12 px empty-space margin and the rename editor's 12 px keep their
   meaning. Gate: the defaults triplet, the grid-thumbs triplet (`C:\traces\grid-thumbs.toml`) and the
   preview triplet. Console checks: bigdir scrolled by dragging the thumb and by clicking the track, in
   both schemes, beside Explorer; a PDF at 400%.
3. **P12-M3, TwinFM's own questions in the look.** One question dialog in app.slint for all four (a
   `PopupWindow` covering the window as the Windows 11 menu's does, so the lists take no click while it
   shows, with the card centred on it): a title, the text, an optional password field (`Field` gains a
   password input type), up to three buttons (outline, and one primary), Return the primary, Escape
   Cancel, the focus set by a token as the settings dialog's is. A Rust module `src/ask.rs`: a worker calls
   `ask(question)`, which hands the UI thread the question with a reply sender (`invoke_from_event_loop`
   and a one-slot channel) and waits for the answer; one question shows at a time and the others queue in
   order; quitting or closing the window answers Cancel to every question waiting, so no worker waits for
   ever; the open-many question, which is the UI thread's own, becomes asynchronous (`open_rows` goes on
   from the answer with its items kept). While a question shows, the window's keys check it as Ctrl+, and
   Ctrl+Z check the settings dialog today. The password field is cleared when the question closes and is
   never written to a line. Unchanged: `TWINFM_SOAK`'s answers, and the password questions' script rule
   (`zippassword` and `pdfpassword` answer them unseen; a script that gave none is refused, never shown
   the question). New driver steps: `waitquestion <ms>` (until a question shows) and `answer <button>`
   (presses the button by its label). In a scripted run a question left unanswered for 30 s is answered
   Cancel, so a script never hangs on one. Lines: `[ask] shown <kind> "<title>" (from <thread>, K
   waiting)`, `[ask] <kind> answered <button> after T ms`, `[ask] <kind> unanswered after 30 s: Cancel
   (scripted run)`, `[ask] quit: K question(s) answered Cancel`.
   A question asked while the UI thread is inside a modal loop (the classic menu, a drag) shows when that
   loop ends; recorded. Windows' own dialogs stay Windows': the shell's replace and merge questions for
   plain copies, its permanent-delete warning and its progress window, which `raise_job_dialogs` keeps
   bringing forward. Tests: the queue (two askers, one shown at a time, in order), Cancel at quit, the
   open-many path's kept items, the scripted run's 30 s Cancel. Scripted checks: the replace, zip delete
   and open-many questions answered by `answer` under `TWINFM_SCRIPT` without `TWINFM_SOAK`, in `%TEMP%`
   scratch folders, with screenshots in both schemes; the password questions at the console. Soak: as today
   (the soak answers by `no_confirm`). Gate: the defaults triplet. Console checks: each question in both
   schemes, Tab between the buttons, Escape, a wrong password, a question while a copy runs.
4. **P12-M4, comfortable rows.** A setting `row_height`, `"compact"` (22 px, the default) or
   `"comfortable"` (30 px, the canvas's compact density), in the dialog's View section, for the details
   lists without thumbnails; the 32 px thumbnail lists and the grids are unchanged; icons stay 16 px,
   centred in the taller row, and the name's x does not move. Settled first by a spike with a trace:
   **a height binding** on the 22 px row components (P9-M1 measured only a width binding), or **four more
   components** (30 px copies of `FileRows`, `FileRowsNoType` and their focus layers; `FileListHost`
   choosing among eight), the shape P3-M1 proved; the trace decides, a binding only if it costs nothing
   the triplet sees. Rust: `ListGeom` takes the row height apart from the icon size (the strip overlay
   composes at the row pitch), `dnd::Layout.row_px`, `window_layout`, the driver's `row_pos`, the rename
   editor, paging; `[icons] list geometry: 30 px rows, 16 px icons`; `settings set row_height
   compact|comfortable`; `[settings] dialog: row_height compact -> comfortable`. Tests: the geometry for
   both heights. Soak: one cycle each way. Gate: the defaults triplet and a settings triplet
   (`C:\traces\rows-comfortable.toml`, read-only, `row_height = "comfortable"`). Console checks: both
   heights beside Explorer, a drop onto a row, a rename, PageDown.
5. **P12-M5, the sidebar, bookmarks by mouse and the new-tab button.** **An icons-only rail**: collapsed, the sidebar shows
   each row's icon in a rail about 48 px wide (headers as hairlines, the row's name as a tooltip; a click
   and a drop work as on the full row), settled first whether the rail replaces today's 12 px strip or is
   a third state (full, rail, hidden), with the saved settings still loading. **Bookmarks reordered by
   dragging**: a pointer drag on a bookmark row past the drag threshold carries it, an insertion line
   shows where it lands, the release moves it through `sidebar::move_bookmark`'s path; inside the sidebar
   this is Slint's pointer, not an OLE drag, and OLE drops onto rows are as today. Driver `bookmark drag
   <from> <to>`; `[sidebar] bookmark moved <from> -> <to> (drag)`. **A bookmark made with the mouse**
   (Jeremy's roadmap item of 2026-10-25, "add a way to create a bookmark using the mouse (click/drag?)",
   put in P12-M5 by his word; today a bookmark takes Ctrl+D, `bookmark_add` in main.rs, which appends the
   active tab's folder): **a folder dropped between two bookmark rows or on the Bookmarks header** is
   added there, at that place, under the same insertion line the reorder draws, from TwinFM's own lists
   and grids or from another program; a drop onto a bookmark row still copies or moves files into that
   folder, as today, so the gap is a spot of its own in `dnd::hit_all` (settled first: how many pixels at
   a row's top and bottom edge count as the gap, against a drop onto the row, in the full sidebar and the
   rail). Only folders and drives are added (a drag of files alone is refused there, a mixed one adds its
   folders); nothing is copied or moved (the effect a link, the drop description "Bookmark"); a folder
   already bookmarked stays where it is, with the note Ctrl+D gives. **"Add to bookmarks" in the Windows
   11 menu**: on a folder's row or tile (each selected folder) and on a list's background (the folder
   shown, as Ctrl+D); settled first whether the Bookmarks header's own menu gets "Bookmark this folder"
   too. `sidebar::add_bookmark` takes the place to insert at; Ctrl+D still appends. Driver `dropfiles
   sidegap <index> <paths>` (the gap above bookmark `<index>`, the count for below the last) and `menupick
   Add to bookmarks`; `[sidebar] bookmark added <path> at K (drop | menu)`, `[dnd] spot: sidebar bookmarks
   gap K`. **A Recent group** (Jeremy's pick):
   the last eight folders shown in either pane (not archive levels, search tabs or This PC), newest first,
   under Places, saved as `recent` in the settings, "Clear recent" in its header's menu; `[sidebar]
   recent: <path> (K kept)`. **The new-tab button** (Jeremy's request of 2026-10-23, taken 2026-10-25):
   the canvas's ghost "+" right after each pane's tab strip, opening the pane's own folder as Ctrl+T does;
   **the tab strip shows always**, with one tab too (Jeremy: "Always show the tab strip"), so every pane's
   list starts 32 px lower than today with one tab: `TAB_STRIP` is always in `list_top`, and the driver's
   aims, the drop hit test (`dnd::hit_all`), the soaks' row aims and the gate scenario's geometry move in
   the same commit (the Phase 10 rule for a change that moves a box). Driver `tab plus <pane>`; `[tab]
   pane=N + button: new tab`. Tests: the rail's hit test, the drag's landing index, the gap against the
   row at each edge, the insert place and a folder bookmarked already, the recent list's rules, the
   layout with one tab. Soak: per cycle a bookmark drag, a folder dropped into the Bookmarks gap and the
   bookmark removed again, and the new tab button. Gate: the defaults triplet, and the full soak, since
   every aim moves. Console checks: the rail with drives and bookmarks, a drop onto a rail row, a drag of a
   bookmark, a folder dragged from TwinFM and from Explorer onto the Bookmarks header and between two
   bookmarks, a file dragged there (refused), a drop onto a bookmark row (still a copy into it), "Add to
   bookmarks" on a folder and on a background, the "+" with one tab and with several.

**The cloud stream** (one session; touches `src/cloud.rs`, `src/icons.rs`'s marks, `src/fs/enumerate.rs`,
the icon paths in `src/main.rs`, `src/shellmenu.rs`'s verb path, `src/fs/watch.rs`'s device registration,
`Cargo.toml`'s windows-sys features, `tools/soak-p5m3.ps1`):

6. **P12-M6, Google Drive's commands, and its volume.** First in the stream: it is a daily-use failure.
   **Its own commands run** (Jeremy, 2026-10-25: "Files can't be marked available for offline use."):
   a classic-menu pick that has no canonical verb is run on the verb worker by its id only when the
   rebuilt menu's item at that id has the picked label, so when the rebuilt menu's ids differ from the
   shown menu's, every such pick below the difference is refused (§ the starting point's Google Drive).
   Settled first, on a `G:` file in TwinFM's own process: why the ids differ (the doubled "Move to
   OneDrive" is the first suspect: compare the shown menu's items with the rebuilt one's, item by item),
   and whether "Manage versions with Google Drive" is in the rebuilt menu at all. The fix: when the item
   at the picked id has another label, find the picked item by its label path (its submenus' labels and
   its own, as `find_command` already does for Share and the ZIP) in the rebuilt menu, and invoke it there
   when exactly one item matches; refuse as today when none or several do. `[menu] "<label>" moved from
   offset A to B in the rebuilt menu: by offset B`. A command missing from every rebuilt menu is recorded
   with what the brief found; invoking from the menu object the UI thread showed is the fallback to
   consider only then (a command's dialog would then run on the UI thread). It mends every program's
   classic-menu commands whose ids move, not only Google Drive's; the Windows 11 menu's packaged commands
   are unaffected (they invoke the objects they asked). **Recognised by something sturdier than its
   label**: settled first which of the volume's file system driver (`googledrivefs*`), the DriveFS
   registry mount point or the root's fingerprint (`My Drive` and `.shortcut-targets-by-id`) holds on this
   machine, the label kept as the fallback; `[cloud] volume <key>: google drive (by <how>)`. **The device
   notification refused by `G:`** (os error 1066 at every watch): the refusal noted once per volume, the
   registration not tried again there. Tests: finding a moved item by its label path (one match, none,
   two), the recognition's fallback order. Soak: unchanged (the soak never touches `G:`). Gate: the
   defaults triplet (the scenario never opens a menu or a cloud folder: it proves only that the rest is
   untouched, and the record says so). Console checks, on `G:\My Drive` beside Explorer: Offline access >
   Available offline and then Online only on a file (Google's mark changes as in Explorer), Manage
   versions with Google Drive, Share with Google Drive; one other program's classic-menu command picked
   below the doubled "Move to OneDrive" on a local file.
7. **P12-M7, OneDrive's syncing and error icons.** Files that are syncing, waiting to sync, in error or
   with a warning show the provider's glyph (arrows, the red cross, the warning), as their folders already
   do, and **the mark follows the file on its own**, as Explorer's does (the arrows while it uploads, then
   the tick, with no F5). Settled first, measured in TwinFM's own process on Jeremy's OneDrive, the ways
   in, cheapest first: **(a)** the placeholder compatibility mode, set per thread
   (`RtlSetThreadPlaceholderCompatibilityMode`, windows-sys's `Wdk_Storage_FileSystem` feature, no new
   crate) on every thread that builds entries through `entry_of` (each listing's own thread, the watch's
   edit worker `watch-edits`, a search tab's re-stat), so those see which files are not in sync and only
   those get the property read; what else the mode changes (the kinds from the reparse tag, the counts,
   the thumbnail keys, since a `Kind::Cloud` entry gets none today, and the content search's skip) is
   checked and kept as today; **(b)** a read of the listed folder's own state first, and the
   files' reads only when it is not in sync; **(c)** the property read for every file on screen (about
   0.5 s for a screen of 40 on the two icon workers, read again at every refresh), only if (a) and (b)
   fail. Also settled first: whether OneDrive's switch of a file to in sync reaches the directory watch as
   a record (P9-M4 checked only "Always keep on this device" and "Free up space"); if it does not, the rows
   on screen still syncing are read again every few seconds, which is per-row work on the icon workers and
   takes a trace first. P9-M4's console check 4 ("shows the syncing or pending mark while it uploads",
   answered "as expected") is re-checked: it may have been the folder's mark. Lines: `[cloud] pane=N not in sync: K of
   M files`, the `[cloud] state read` line as today. Tests: the state-to-glyph table, the not-in-sync
   rule. Soak: the OneDrive folder if Jeremy agrees (§ Open questions), else unchanged. Gate: the defaults
   triplet (the scenario never lists a cloud folder: it proves only that the rest is untouched, and the
   record says so) and an in-app run before and after on a OneDrive folder, comparing the `[icons]` line's
   `cloud N` and `slow N`. Console checks: a file copied into OneDrive shows the arrows while it uploads,
   then the tick; a file OneDrive refuses shows the error glyph (how to make one is part of the brief);
   beside Explorer.

**The coordinating session** (this one) serialises the machine, hands each gate to Jeremy, merges on his
word and keeps `docs/roadmap.md`, and does three tasks of its own:

- **The memory trace** (Jeremy's word, 2026-10-25): `tools/wpr-heap.ps1` takes WPR's heap
  snapshots of TwinFM during `soak-p5m3.ps1 -Minutes 60` on `target-p11close`, so it measures 0.2.0
  before any Phase 12 change: one snapshot after five minutes, one at 55; the outstanding allocations
  by stack compared between the two, with `target-p11close`'s symbols. Settled first: snapshots against a
  full heap trace (a full trace of an hour is too large to read). One elevated run by Jeremy; it takes the
  soak slot before the first milestone's soak. The record says what grows; a fix, if one is needed, is a
  follow-up of its own.
- **The dead network share check**, owed since P5-M1, if Jeremy has a share that can be switched off
  during the check (§ Open questions): a share name that does not resolve, then one that resolves but
  does not answer (`stalled: no watch`, the 30 s cool-down, F5).
- **Clean-up**: the worktree `festive-driscoll-27496a` (Phase 10's), its session archived, removed on
  Jeremy's word; `gate-p11close` stays as the reference. (`twinfm-public-release-9e4613`, 0.1.0's, was
  named here too, but the harness gave it to the look stream when that session started: it stays for the
  phase.)

Not in Phase 12, unless Jeremy says otherwise: **the Slint fork's whole-row snap patch and upstreaming
the two accessibility patches** (deferred to Phase 12 at the Phase 11 kickoff, but not named in this
phase's scope; on the roadmap). If P12-M2 takes the fork route, the snap patch goes in with it, by
Jeremy's rule of 2026-10-13, "take the patch when the fork is next touched" (`phase-3-plan.md`): one
regeneration of `docs/patches/` and a triplet for each patch. **The rubber band in the details list and
saving a search tab** (both on the Phase 11 close's Phase 12 list) are left for a later phase: the lead
proposed it and Jeremy took the other additions without the band.

Where the streams meet: P12-M4 and P12-M7 both change `src/icons.rs` (the list geometry, the marks), and
P12-M5's always-shown tab strip moves the driver's aims that every later soak uses; the second of each to
merge takes main in first, and the coordinating session orders those merges.

Each milestone brings its driver steps and stderr lines (in `CLAUDE.md` § Measuring), tests for its pure
parts, and its soak cycle in `tools/soak-p5m3.ps1`. Every item in a bundle is a commit of its own, and the
record says per item: done, checked and found right, or left with the reason. Console checks after each
triplet, unelevated.

**The gate is re-run after every milestone** under the protocol of 2026-10-12, as Phase 11 ran it:
`tools/wpr-triplet.ps1` elevated at the console by Jeremy, accessibility on, Narrator off, every other
TwinFM window closed, the split-and-scroll scenario on `C:\traces\bigdir`, clean binaries built with
`--locked` into their own `--target-dir target-<name>` from detached `git worktree add` checkouts. Three
runs per binary, interleaved, compared by median; a gap counts only when every reference run sits below
every candidate run or the medians differ by more than one point (one-sided: a cost); each run's `[perf]
... render[... p50=...]` read first; a fixed-order gap the code cannot explain gets the order-swapped rerun
before anything else (ab53, ab54). The reference for the first Phase 12 gate is **`target-p11close`**
(a99f2f2); after each merge the new milestone's gate build becomes the next reference, and the previous one
goes once the new one has served. **Tags continue from ab56.** Bar: main thread under 20% busy over the
10 s after the split, judged against the reference's spread in the same session. One wrapper per milestone
(`gate-<milestone>.ps1`, on `tools/gate-p11m7.ps1`'s pattern), which refuses to start while cargo, a TwinFM
window, Narrator, wpr or xperf runs or a Claude process spins a core. Before handing a gate over, the
coordinator has every stream's "holding" and samples every process's CPU over 5 s. Settings triplets stay
part of the protocol (Phase 4's rule): each candidate's stderr is checked for `[settings] file <toml>
(read)` and the line the setting implies. A PR in conflict with main gets no CI run: merge main in before
reading CI's silence.

**The Phase 9 and 11 rules for two streams hold**: every stream reports to the coordinating session before
any cargo run, gate build, soak, TwinFM window or hand-over and waits for its reply; one soak at a time;
nothing builds, soaks or opens a TwinFM window from "gate running" to "machine free"; the tests are run, the
result read, then the commit made, as two steps, never chained; settle-first probes run in TwinFM's own
process; a fix after the console check needs no triplet when it stays in event handlers (one in-app run
recorded) and a triplet when it touches the rows, the icon workers' per-row work or the compose path.

The architecture rules from Phase 1 hold: only the UI thread touches a Slint model and workers wake it with
`slint::invoke_from_event_loop`; every COM object lives on the thread that made it; no app-state `RefCell`
borrow is held across a modal loop; no `unwrap` on `Weak<App>`; nothing per frame or per row without a trace
first. P12-M3 adds one: **a worker may wait for the UI thread only through `ask`, and the UI thread never
waits for a thread that can call `ask`**, so the two can never wait on each other (the UI thread's 1 s
waits for the watcher, at a drive's removal and at exit, stay: the watcher never asks). Every crate added is pinned by exact version
with its licence noted, default features off, and nothing is added that is not required (Jeremy,
2026-10-22): every Settled first asks first whether something already in the build does the job.

## Calls made in this kickoff, for Jeremy to check

Decisions Jeremy did not make and the lead settled, each reversible before its milestone starts:

- Two streams split by files: the look (the screen files, the questions, the rows, the sidebar) and the
  cloud drives (Google Drive's commands and volume, OneDrive's marks), so the two rarely touch one file.
- Google Drive's commands go first in the cloud stream, ahead of OneDrive's icons, since they fail in
  daily use; the fix finds a moved item by its label path and still refuses when no item, or more than
  one, matches, so a wrong command is never run.
- The look's details come from the canvas Jeremy chose in Phase 10: the badges, the "+" after the tab
  strip, comfortable rows at 30 px. The canvas's 36 px "default" density is not offered.
- The hover fade is on chrome only; nothing that scrolls animates.
- The badges fit today's 28 px status line, so nothing moves; the canvas's 40 px footer is not taken.
- TwinFM's own scrollbar over a fork patch, if the spike shows no cost.
- One question dialog for all four questions, a queue of one at a time, and Cancel at quit; Windows' own
  dialogs from the shell's file operations stay Windows'.
- Comfortable rows apply to the 22 px lists only; icons stay 16 px.
- A bookmark by mouse is a drop into the gap between bookmark rows or onto the Bookmarks header (a drop
  onto a row keeps copying files into that folder) and "Add to bookmarks" in the Windows 11 menu; a
  folder already bookmarked stays where it is.
- OneDrive's cheapest way in is tried first, and the per-file read for every row only as the last resort.
- The memory trace measures 0.2.0's build, before any Phase 12 change.
- From the builder check, settled here: a OneDrive file's mark follows its state on its own, as
  Explorer's does, so the placeholder mode goes on every thread that builds entries and the thumbnail keys
  stay as today; the password questions keep the script's rule (a script never waits for a person), so
  they are checked at the console and the other three by `answer`, with a 30 s Cancel for a scripted run;
  the Slint fork items stay on the roadmap unless P12-M2 takes the fork route.
- Until Jeremy says otherwise: the two streams run as Phase 11's did; the soak does not write into
  OneDrive, and P12-M7's console check covers the sync root; the dead share check waits for a share.

## Open questions

Written 2026-10-25 at the kickoff, after Jeremy's answers to the first four (recorded at the top); the
first ones are his, the rest each brief settles first.

- **The two streams** (Jeremy's): Phase 11's shape again, the lead's proposal.
- **The OneDrive soak folder** (Jeremy's): a soak writing into `%OneDrive%\twinfm-soak` would upload every
  cycle's files and fill OneDrive's online recycle bin; behind its own variable, removed afterwards. Not
  used unless he agrees.
- **A share that can be switched off** for the dead share check (Jeremy's).
- **The roadmap's upkeep** (Jeremy's): an item leaves `docs/roadmap.md` when a phase picks it up, and each
  phase close adds its deferred items back; written into `CLAUDE.md` only on his word.
- **The clean-up of the old Phase 10 worktree** (Jeremy's word before it goes).
- **Why Google Drive's menu ids move, the tooltip, the scrollbar, the rows and OneDrive's way in**: each
  milestone's Settled first.

## The memory check (the coordinating session, 2026-10-25)

Jeremy's word at the kickoff: "The memory check". The question, from the Phase 11 close's 60-minute soak: what
the slow growth of private bytes after the first five minutes is (about 0.6 MB a minute), with the "187 MB
first run" of ab20 and ab21.

**The tools** (branch `ops/p12-heap-check`): `tools/wpr-heap.ps1` switches WPR's heap stack tracking on for
`twinfm.exe` (`wpr -snapshotconfig heap -name twinfm.exe enable`, through IFEO, off again at the end), records
the HeapSnapshot and VirtualAllocation profiles, runs the `gate-p11close` checkout's `soak-p5m3.ps1` on
`target-p11close` (a99f2f2, 0.2.0's code) and takes `wpr -singlesnapshot heap <pid>` twice, after the caches
have filled and as long before the end. `tools/analyze-heap.py` reads the trace without elevation: xperf's
`heapsnapshot -data` with TwinFM's symbols, every live allocation's stack totalled per snapshot and ranked by
growth (by the deepest frame in `twinfm.exe` outside the allocator, by the deepest `twinfm::` frame, by
stack), then xperf's `virtualalloc` summary between the two snapshot times (committed and still outstanding,
by image and by stack). UMDH and gflags are not installed here; WPR and xperf were enough. Jeremy ran both
elevated at the console.

**The dry run** (`heapdry`, 5 minutes, snapshots at 1 and 4 minutes): exit 0, 4 cycles, the trace 90 MB, the
dump 175 MB of text in 43 s. The live heap was 91.4 then 96.4 MB while private bytes went 177 to 289 MB: most
of the growth was outside the live allocations, so VirtualAllocation was added to the recording before the
long run (Jeremy skipped a second trial).

**The long run** (`heap1`, 60.4 minutes, snapshots at 5 and 55): exit 0, no panics, 55 cycles (76 in the
Phase 11 run without tracking), 1,192 jobs, none failed or cancelled, every must-be-0 count 0; the trace 1.3
GB. Private bytes 289 MB at 5 minutes, 853 at 55 (working set 334, 889): far steeper than the Phase 11 run's
157 to 300, because stack tracking adds its own record to every allocation. The numbers below compare the
two snapshots of this one run.

- **What TwinFM holds barely grows.** Live heap allocations: 196,037 (96.90 MB) at 5 minutes, 323,295
  (110.54 MB) at 55. The growth is the 40,000-entry `C:\traces\bigdir` listing, open at the second snapshot
  and not at the first: `PaneModel::append` 4.9 MB, Slint's shared strings for its rows 4.1 MB (120,008
  allocations, three per entry), `request_pass` 3.1 MB, `LocalZone::format` 1.6, `entry_of` 1.5,
  `Entry::with_zone` 1.1. The Recycle Bin enumeration in `fileops::perform_inner` (the shell's `CBitBucket`
  folder cache, found in the dry run) grew 0.5 MB over the 50 minutes. **No leak of TwinFM's own data.**
- **The growth is memory committed and not held.** Committed through `VirtualAlloc` between the snapshots and
  still committed at the end: 943 MB, 534 of it on stacks in `twinfm.exe`. The largest of those run through
  the preview worker's `StyledText::from_markdown` (`twinfm::preview::styled`, then Slint's
  `styled_text::parse_interpolated`), the styled text of the soak's 2 MB Markdown file `g-big.md` each cycle:
  the process heap grows its segments for those large temporary allocations and keeps them committed after
  they are freed. (Frame names like `Vec<winmenu::Entry>::grow_one` in those stacks are the linker's folded
  copies of the same code, not menus.) The graphics stack adds its own: `dxgmms2.sys` 69 MB and the AMD driver
  `atio6axx.dll` 66 MB still committed.
- **Reading**: the private-bytes creep is the heap's retained commit after the preview's large temporary
  allocations, and the graphics driver's, not data TwinFM keeps. Stack tracking magnifies the heap's share
  (its per-allocation record), so this run's 853 MB is not a figure for 0.2.0 without it. The "187 MB first
  run" was not measured on its own.

**Follow-up candidates** (not done; Jeremy decides): the segment heap for `twinfm.exe` (its manifest's
`<heapType>SegmentHeap</heapType>`, which returns freed memory to the system more readily), measured by the
same 60-minute soak without tracking against `target-p11close`; fewer large temporary allocations in the
preview's styled text for a big file (built per chunk); the Recycle Bin enumeration's shell cache (small).
