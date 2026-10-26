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
- **Clean-up**: the worktrees `festive-driscoll-27496a` (Phase 10's) and `twinfm-public-release-9e4613`
  (0.1.0's), their sessions archived, removed on Jeremy's word; `gate-p11close` stays as the reference.

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
- **The clean-up of the two old worktrees** (Jeremy's word before they go).
- **Why Google Drive's menu ids move, the tooltip, the scrollbar, the rows and OneDrive's way in**: each
  milestone's Settled first.

## P12-M1: the look's small pieces

The look stream's first milestone, opened 2026-10-25 on main at 76b5d35 (this plan merged), branch
`feat/p12-m1-look-pieces`. Four pieces, a commit each, all inside today's boxes: no height, width or
position the driver, the drop target or the soaks read changed.

### Settled first (2026-10-25, before any code; the Slint fork's source at the pinned rev c1d3a8d)

- **Tooltips: painted over Slint's frame, not a tooltip of TwinFM's own.** The compiler's tooltip pass
  (`lower_tooltips.rs`) puts any custom content inside a `ToolTipImpl` (`common/tooltip-base.slint`: the
  fluent alternate background, a 1 px `Palette.border`, 4 px corners, its own empty `StyledText` padded 9
  and 6 px), sizes the popup from that element's preferred size, and places it at the pointer plus the
  8 px offset. Two facts made painting over it whole: Slint's layout leaves a child placed by `x` or `y`
  out of its parent's size (`default_geometry.rs` `gen_layout_info_prop`), so a face drawn 1 px outside
  the frame on every side changes nothing in the tooltip's size; and a 6 px corner 1 px further out
  covers a 4 px corner wholly (every point of the 4 px arc lies within 5.4 px of the 6 px arc's centre).
  The popup is not clipped to its bounds: the face's hairline, 1 px outside them, draws in the capture.
  So the look's tooltip is three lines at each use (`TipCard` in theme.slint), with no timer, overlay or
  hover reporting of TwinFM's own; the plan's alternative would have needed all three. Slint's stay: the
  500 ms delay, the place under the pointer (not under the control), the popup's size from the content.
- **The fade's colours**: Slint animates a brush through `Color::mix`, Sass's mix, which weights the
  channels by alpha (`color.rs`): from `transparent` to the muted fill only the alpha moves, so a ghost
  button never darkens on its way in light mode. No token needed for the "off" end.
- **Which controls fade**: the plan's list, read against every `has-hover` in the three UI files: `Btn`,
  `Choice`'s border and its options' fill (theme.slint); the toolbar's buttons, the tab segment and its
  close button, the crumb labels and chevrons, the header cells, the search box's clear, content and
  search buttons and the path-mode button (pane.slint); TwinFM's own menu items, the Windows 11 menu's
  rows and icon-row buttons, the sidebar footer's Settings and About text and its hide chevron (colour
  and colorize, the hover changing those), the splitter and the preview handle (app.slint); and two the
  survey did not name, the sidebar's resize edge and the collapsed sidebar's strip (chrome, never
  scrolled). Not faded, as the plan says: the sidebar rows and the crumb dropdown's rows (in ListViews
  that scroll); rows and tiles have no hover.
- **The badges' line**: the 28 px status line keeps its height, so `LIST_BOTTOM_GAP` (33) stands. The
  badges sit in a start-aligned inner layout padded 3 px top and bottom, so they are 22 px; in the fork's
  box layout (`layout.rs` `solve_box_layout`, `adjust_items`) a start-aligned layout gives spare width to
  nothing, and when the line is short it shrinks only the items with a stretch, so the left text elides
  before a badge loses a pixel (the badges and the free space never stretch). `statusbar::selection_text`
  returns the count and the size apart; nothing read the joined text (the soaks read `[band]` lines, not
  the status bar).
- **The preview's icons**: the message text is the identity of each state (every message is a constant in
  preview.rs), so a pure `message_icon` maps it to an icon name the column draws, with no change to the
  worker's replies. The plan names four kinds; the two other "not previewed" messages, inside an archive
  and no HEIF decoder, take the file icon. Lucide's `lock` and `triangle-alert` were added to
  `src/ui/icons/` (ISC, as the folder's licence and README's notice already cover); `file` and `cloud`
  were there.

### What landed (2026-10-25)

Four code commits, each built and tested before it was made (302, 302, 302 and 303 tests), then the docs
and the gate wrapper. Per item:

- **A fade on hover**: done (02a5f64). `animate background { duration: 100ms; }` (and `border-color`,
  `color`, `colorize` where the hover changes those) on the controls listed above; the Switch knob's
  100 ms was the one animation before.
- **Tooltips in the look**: done (84acac3). `TipCard` in theme.slint: the card colour, a hairline, 6 px
  corners, 12 px text in `Theme.fg` with shadcn's 12 and 6 px padding (28 px tall), a soft shadow (blur
  8 px, 2 px down, black at 20%); the toolbar's buttons, the content toggle and the search button use it.
  The `[toolbar] top|pane=N tooltip "<name> (<key>)"` line is unchanged (the card's `init`).
- **The status bar's badges**: done (b196b77). The item count muted, then "M selected" on the muted fill
  and the size in an outline badge (the folder-size variants, "so far...", "(updating...)", inside it), 22
  px tall, 6 px corners, 12 px at weight 500, 8 px apart, right after the items as the Parts board draws
  them; the free space stays on the right. `Badge` in theme.slint; `StatusInfo` is `{ left, count, size,
  right }`; `statusbar::mid_text` became `selection_text`, its tests the split
  (`folder_sizes_in_the_size_badge` renamed from `folder_sizes_in_the_middle_text`).
- **The preview column's frame**: done (e0c9a28). The title in `Theme.fg` at weight 500 (fluent's default
  colour at 600 before); each message under its Lucide icon, 20 px in `Theme.fg` on a 40 px muted tile
  with 10 px corners (shadcn's empty state): file for "No preview", inside an archive and no decoder, lock
  for both password messages, cloud for a cloud-only file, the warning triangle for the four "could not
  be read" messages (`preview::message_icon`, tested for every message; `preview-icon` beside
  `preview-message`); the page note's `#444` is `Theme.paper-fg` and the page's `#FFFFFF`
  `Theme.paper`, both the same in either scheme since the paper stays white.

No new driver step and no new stderr line.

### Calls made in this milestone, for Jeremy to check

- The tooltip stays where Slint puts it, under the pointer; a tooltip under its control would have been
  TwinFM's own overlay, which painting over the frame made unneeded.
- The tooltip's text is 12 px with shadcn's padding, so it is 28 px tall, a little taller than before.
- The preview's messages sit under an icon on a small muted tile (shadcn's empty state) rather than a bare
  icon.
- The PDF page note ("Page N took too long") is the light scheme's muted grey, `#737373`, on the white
  page, a little lighter than the `#444` before.
- The sidebar's resize edge and the collapsed sidebar's strip fade too, though the plan's list did not
  name them.

### Scripted checks (2026-10-25, the worktree's own build at e0c9a28, not a gate build)

Runs of the branch's build with `SLINT_BACKEND=winit-software` and `TWINFM_SCHEME=dark`, then `light`,
captured by `PrintWindow` of that process's own window, over a scratch folder
(`%TEMP%\twinfm-p12m1-check`: a folder of 1.4 MB, a file of no previewed type, a damaged PDF, a text file
marked offline, the P11-M6 test's revision 3 password PDF, a text file and a 1.3 MB file):

- **Tooltip**: `hover 300 22` over Copy logged `[toolbar] top tooltip "Copy (Ctrl+C)"`; at eight times
  zoom the card is `#171717` with the hairline in dark and white with `#E5E5E5` in light, 6 px corners,
  and no pixel of Slint's frame anywhere; the Copy button behind it in its hover fill. The shadow does not
  show in these captures (dark ground, and the software renderer); femtovg's is the console check's.
- **Badges**: two files selected, "7 items", then "2 selected" on the muted fill and "1.3 MB" in the
  outline; then the folder (sized, `[size] done ... 1403000 bytes`) and a file, "2 selected" and "2.6 MB";
  both schemes, the free space on the right, the texts centred in the line.
- **The preview's messages**: `preview toggle` and the cursor through the four files gave `no preview for
  this type`, `password`, `cloud-only` and `damaged: Invalid` lines, and the file, lock (with the "Enter
  password…" button under it), cloud and triangle tiles over their messages, the titles in the
  foreground, both schemes.
- No `panicked` line in any run.

### Soak

None, by the plan: nothing new to drive (the tooltips, badges and messages are drawn, not driven), and
`tools/soak-p5m3.ps1` is unchanged.

### Gate

ab56, passed (2026-10-25, Jeremy at the console; the six Phase 12 gates of both streams, ab56 to ab61,
ran in one elevated script from 22:10 to 23:33, this one 22:10 to 22:26): `tools/gate-p12m1.ps1`,
`target-p12m1` (e0c9a28) against `target-p11close` (a99f2f2), the defaults and the preview column
(`C:\traces\preview.toml`).

| Setting | p11close (WPR) | p12m1 (WPR) | Medians | Render p50 |
|---|---|---|---|---|
| defaults | 21.3, 21.7, 21.4 | 21.5, 21.0, 21.0 | 21.4 / 21.0 | 2.86 to 3.07 ms |
| preview | 20.1, 19.8, 20.0 | 20.0, 20.3, 20.3 | 20.0 / 20.3 | 2.65 to 2.75 ms |

No gap that counts in either (no setting has every reference run below every candidate run, and the
medians differ by 0.4 and 0.3 of a point); no panic, every settings file read. The whole session ran in
the costlier state (render p50 2.86 to 3.07 ms in every binary's defaults runs, against about 2.2
normally; as ab42 in Phase 10), so the 20% bar was met by neither build; read like for like.

### Console checks (for Jeremy, unelevated, after the gate, `target-p12m1`, light and dark)

- **The fade**: the pointer slowly across the toolbar's buttons, a tab and its close cross, the crumbs
  and their chevrons, the column headers, the search box's three buttons and the path-mode button, the
  Settings dialog's buttons and choices, a right-click menu's rows and its icon row, the sidebar's
  Settings and About and its hide chevron, the splitter, the preview handle and the sidebar's edge: each
  fill fades in and out quickly, never flickers dark; the sidebar's rows still light at once.
- **The three tooltips**: a toolbar button (both toolbar placements), the search box's content toggle (on
  and off) and its search button: the card colour, the hairline and the soft shadow, no grey frame
  around it.
- **The badges**: files selected ("2 selected" and the size), folders sized with Alt+Shift+Enter (the
  size "so far..." while they are walked, then the total), a long note on the left with a narrow pane
  (the note shortens before the badges move).
- **Each preview message**: a file of no previewed type, a password PDF (the lock, the button), a file
  in OneDrive that is online only (the cloud), a damaged PDF (the triangle), a file inside a zip.

## P12-M2: scrollbars easier to see

The look stream's second milestone, Jeremy's request of 2026-10-23 ("the scroll bars are too transparent
and need to be more visible"), opened 2026-10-25 on the P12-M1 branch's head (46c6385; P12-M1 not yet
merged), branch `feat/p12-m2-scrollbars`. One change, every list's scrollbar, inside today's boxes: no
geometry constant, driver aim or soak row moved.

### Settled first (2026-10-25: the fork's source at c1d3a8d, then the spike in the coordinating session's slot)

- **What the Fluent bar is** (`fluent/scrollview.slint`): `ScrollBar`, not exported, a 14 px overlay at
  the view's edge with `clip: true`; its thumb `FluentPalette.border`, 2 px wide at rest and 6 px on hover
  (`states [hover]`, `animate size 150ms`), 4 px from the edge, 16 px from each end for the arrow buttons
  that show on hover. The app sets only the policy. Its thumb's place binds to the Flickable's content-y,
  so it is laid out again every frame a list scrolls, as any bar's would be.
- **One bar for the file lists**: `FileListHost` owns `content-y` (two-way into whichever of its six list
  components shows) and can reckon the content's height (rows × row-h, grid rows × tile-h, as the pane's
  `rows-h` already does), so one `VScrollBar` after the six `if`s covers the details lists with and
  without the Type column, the 32 px lists, both grids; each list sets `vertical-scrollbar-policy:
  always-off`. The sidebar, the crumb dropdown and the preview's views take the same component bound to
  the view's own `content-y`, `content-height` and `visible-height`; the preview's pages past 100% take
  `HScrollBar`, the same on its side.
- **The driver**: `thumb()` aims 10 px in from the pane's edge and 17 px below the list's top; the gate
  scenario drags both panes' thumbs there. A thumb that starts 2 px below the list's top and is at least
  24 px long sits under that aim, so the aim, the gate scenario and the soaks stay as they are.
- **The text beside it**: the rows end their right-aligned Modified text 6 px from the list's edge
  (`dnd::name_cell`'s 6.0). An overlay bar keeps every geometry constant; a 6 px thumb 1 px from the edge
  then meets the last digit with no gap (the scripted check, below). Padding the rows 4 px more would
  move the columns, the header's offset, `name_cell` and its tests: not done, put to Jeremy (§ Calls
  made).
- **Contrast** (3:1 asked, on the card): light needs a thumb at or darker than about `#949494` (`ring`
  `#A3A3A3` is 2.5:1), dark at or lighter than about `#636363`. The tokens: `scroll-thumb` `#8A8A8A` light
  (3.4:1 on white) and `#737373` dark (3.8:1 on `#171717`), `scroll-thumb-hover` one step stronger (`#737373`
  light, `#A3A3A3` dark), `scroll-track` black or white at 5%, shown only under the pointer.
- **The spike** (the plan's: own bar against a fork patch, decided by a trace): three builds were planned,
  the fork patch only if the own bar cost anything. Base, the P12-M1 build (e0c9a28's code, the Fluent
  bar), against own, `VScrollBar` over every file list; twelve in-app runs of the gate scenario on
  `C:\traces\bigdir` back to back, base, own, own, base three times, unelevated, no WPR, in the
  coordinating session's slot with the machine otherwise idle (2026-10-25). Nobody was at the console:
  every run presented at a frame interval p50 of 46.3 to 47.8 ms (about 21 Hz, not the 60 Hz of gate
  days), so `[busy]` reads near 11 rather than 19; the state never changed across the twelve (render p50
  within 0.1 ms), so they compare like for like.

| Build | `[busy] 10 s after split` | Median | Render p50 (ms) | Frames |
|---|---|---|---|---|
| base (Fluent bar) | 11.1 / 12.0 / 11.4 / 9.1 / 9.2 / 13.9 | 11.3 | 2.71 to 2.77 | 384 to 385 |
| own (`VScrollBar`) | 10.0 / 12.1 / 13.4 / 9.3 / 11.1 / 8.4 | 10.6 | 2.67 to 2.71 | 382 |

  No cost: the own bar's median is lower, its spread the same, its time per frame no higher. **TwinFM's own
  scrollbar is taken; no fork patch was built**, so `docs/patches/`, the `rev` and the snap patch stay as
  they are (the snap patch rides only with a fork change, Jeremy's rule of 2026-10-13). The console
  triplets at the gate decide as usual. A scripted drag under the unchanged aim moved both lists as the
  Fluent thumb did: `drag 0 400 1000` to content-y −567,314 (the base −586,234; the own track is 28 px
  longer and its thumb 24 px at least, so the same drag moves 3% less), `drag 1 200` to −283,646
  (−293,106).

### What landed (2026-10-25)

- **c36ae6a**: `VScrollBar` and `HScrollBar` in theme.slint: a 14 px strip at the view's right (bottom)
  edge, shown only when the content overflows; the thumb 6 px, 1 px from the edge, 2 px from the strip's
  ends, at least 24 px long, round-ended, `Theme.scroll-thumb`; under the pointer or while dragged 10 px
  and `scroll-thumb-hover` over the faint track (`scroll-track`), the width and colour fading over 100 ms.
  A press on the thumb drags it (the thumb keeps the point it was taken by); a press on the track pages a
  screenful towards the pointer (once per press: Explorer repeats while held, TwinFM does not); the wheel
  over the strip goes on to the view. Placed: one in `FileListHost` over the six list components, one over
  the sidebar's list, one in the crumb dropdown, and in the preview one over the pages (with the
  horizontal one past 100%, each short of the corner when both show) and one over the Markdown list; every
  one of those views `always-off`.
- **472ceae**: `thumb()`'s comment (the aim lands on the new thumb; no aim changed).

No new driver step, no new stderr line, no pure part to test (the bar is all Slint; its arithmetic is
checked by the scripted drags and track clicks below). Tests 303, as before.

### Calls made in this milestone, for Jeremy to check

- At rest the thumb meets the last digit of the Modified column with no gap (it never covers it); under
  the pointer it widens over the last few pixels of the text. The alternative is 4 px more right padding
  in the rows, which moves the columns.
- A press on the track pages once; holding it does not repeat.
- The faint track shows only under the pointer; at rest there is the thumb alone.
- No arrow buttons (Fluent's showed on hover).

### Scripted checks (2026-10-25, the branch's build at c36ae6a, `SLINT_BACKEND=winit-software`)

- **`C:\traces\bigdir` in the details list, both schemes**: at rest the 6 px thumb at the list's top
  right; `hover` over the strip widened it with the track behind; `drag 0 400 1000` to content-y
  −567,314; a press on the track below the thumb (`band 0 1003 600 1003 600`, a press and release at that
  point of the list) paged down 648 px, one screenful (−567,962), and above it back up (−567,314);
  `wheelat` over the strip scrolled the list 120 px (−567,434), so the wheel passes through.
- **The grid with thumbnails** (`TWINFM_CONFIG=C:\traces\grid-thumbs.toml`, `[view] pane=0 starts as
  large icons`): `drag 0 200 600` to −175,168, the thumb inside the grid's 12 px right margin.
- **The sidebar** with fourteen bookmarks (`bookmark add` in fourteen folders of `C:\Windows`; a scripted
  run saves none) and **the crumb dropdown** of `C:\Windows` (79 folders): each shows the thumb, both
  schemes.
- **The preview**: a four-page PDF at 250% (`previewzoom in` six times) shows both bars, the horizontal
  one along the bottom short of the corner; this repository's `CLAUDE.md` as Markdown shows the list's
  thumb clear of the text.
- No `panicked` line in any run.

### Soak

None added: the gate scenario already drags both panes' thumbs, and a track press has no driver step of
its own (the scripted check used `band`'s press and release); `tools/soak-p5m3.ps1` is unchanged.

### Gate

ab58, passed (2026-10-25, Jeremy at the console, 22:35 to 22:59 in the six-gate script):
`tools/gate-p12m2.ps1`, `target-p12m2` (472ceae) against `target-p12m1` (e0c9a28, its base), the
defaults, the grid with thumbnails and the preview column.

| Setting | p12m1 (WPR) | p12m2 (WPR) | Medians | Render p50 |
|---|---|---|---|---|
| defaults | 21.3, 22.0, 21.1 | 21.2, 20.7, 21.1 | 21.3 / 21.1 | 2.86 to 2.90 ms |
| grid-thumbs | 11.8, 11.8, 11.8 | 11.9, 11.8, 11.8 | 11.8 / 11.8 | 1.46 to 1.55 ms |
| preview | 20.3, 20.2, 20.0 | 19.7, 20.3, 20.0 | 20.2 / 20.0 | 2.68 to 2.73 ms |

No gap that counts in any setting; the candidate's medians at or under the reference's, as the spike read
in-app (the scenario drags both panes' thumbs in every run); no panic. The whole session ran in the
costlier state (render p50 2.86 to 3.07 ms in every binary's defaults runs, against about 2.2 normally;
as ab42 in Phase 10), so the 20% bar was met by neither build; read like for like.

### Console checks (for Jeremy, unelevated, after the gate, `target-p12m2`, light and dark)

- `C:\traces\bigdir` beside Explorer: the thumb easy to find at rest; dragged top to bottom; a click on
  the track above and below it (a screenful each); the wheel over it.
- The grid at large and medium tiles with thumbnails: the thumb in the right margin, a drag, a track
  click.
- The sidebar with enough bookmarks to scroll, and a crumb dropdown of a big folder (`C:\Windows`).
- A PDF at 400%: both bars, each dragged; a long Markdown file's bar (the drift recorded since Phase 8 is
  the list's own height estimate, unchanged).
- Whether the thumb meeting the date's last digit at rest bothers you (§ Calls made).

## P12-M3: TwinFM's own questions in the look

The look stream's third milestone, opened 2026-10-25 on the P12-M2 branch's head (fee789f; P12-M1 and
P12-M2 not yet merged), branch `feat/p12-m3-questions`. Windows' three message boxes and the hand-built
password dialog become one question dialog of TwinFM's own.

### Settled first (2026-10-25, reading the code before any)

- **Who asks, and on which thread**: the file operation worker (`fileops`, an STA thread with a message
  loop) asks the zip's replace question (the conflict closure `archivewrite::add`, `copy_from_zip` and
  `from_shell` call), the permanent delete inside a zip (`confirm_delete`) and a zip's password
  (`key_for`); the preview worker (`preview`) asks a PDF's password (`pdf_password`, after the "Enter
  password…" button); the UI thread asks "Open N items at once?" (`open_rows`). Each was a modal loop
  on its own thread (`MessageBoxW`, `DialogBoxIndirectParamW`), owned by the app's window.
- **The plan's rule holds in the code as it is**: nothing on the UI thread waits for the file operation
  or preview worker (their replies come through `invoke_from_event_loop`; the driver's `waitops` and
  `waitpreview` poll on timers); at exit the UI thread posts the workers' loops `WM_QUIT` and joins
  neither (the watcher's stop is its one wait, and the watcher never asks). So a worker waiting in
  `ask` cannot deadlock with the UI thread, and `ask::quit`, right after `app.run()` returns, answers
  Cancel to anything still waiting. While it waits, the file operation worker owns no window (it asks
  before or between its shell operations), so blocking on a channel instead of pumping a message box's
  loop leaves no sent message waiting on it.
- **Slint, at the pinned rev**: showing a popup takes the focus and gives it to nothing, so the focus is
  set by a token bumped after `show()` (the settings dialog's way); an element inside a `for` cannot be
  named from outside it, so the buttons are three fixed slots, right-aligned (the buttons in the last
  ones, an empty slot's button not visible), focused by id; a layout keeps the space of an element that
  is not visible, so the password field's row has a height only when asked for; `TextInput` has
  `input-type: password` (masked, not copyable), which `Field` now passes on.
- **`Btn` took no keyboard focus** (a Rectangle and a TouchArea), so Tab could not move between a
  question's buttons: it gains a `FocusScope` (Space or Return presses it, a 2 px accent ring shows it,
  `enabled` only while visible). The settings dialog's and About's buttons take the focus too.
- **The PDF's window handle**: `Req::Open`'s `ask` carried the window handle only to own the dialog; it
  becomes a flag, and `pdf_password` and `ask_password` lose their handle (`ask_password(title, prompt)`).

### What landed (2026-10-25)

- **541bdf9**: `src/ask.rs` (new): `Question` (a kind for the lines, title, text, a password field or
  not, one to three buttons with the primary and the Cancel), `ask` for a worker (the question handed to
  the UI thread, the answer awaited on a one-slot channel; Cancel when the UI thread is gone, and a
  refusal with a line if it is ever called on the UI thread, which would hang it), `ask_here` for the UI
  thread (the reply runs on the answer), `Queue` (one shown, the rest in order), the scripted run's tick
  (`overdue`, `SCRIPT_CANCEL` 30 s), `quit`, and the driver's `shown` and `answer_label`; five tests. In
  app.slint `question-popup`: a popup over the whole window on `Theme.scrim` (black at 40%), the card
  centred (420 px, the look's 10 px corners and the dialogs' shadow), the title at 16 px weight 600, the
  text muted, the masked field when asked, the three slots of `Btn`s; Escape presses the Cancel slot,
  Return the primary (a focused button presses itself), the window's keys do nothing while it shows.
  The four questions: the zip's replace ("Replace files in z.zip?", the names, Cancel, Skip, Replace),
  the zip delete ("Permanently delete N items?", "From z.zip. A zip archive has no Recycle Bin, and this
  cannot be undone.", Cancel, Delete), the password ("Password for <name>", the prompt as before, a
  masked field, Cancel, OK; the zip's and the PDF's), open-many ("Open N items at once?", "Each folder
  opens in a new tab, and each file in its program.", Cancel, Open), its items kept in the reply
  (`open_rows` then `open_items`). Driver `waitquestion <ms>` and `answer <button>`; `[ask]` lines
  (CLAUDE.md § Measuring). Unchanged: `TWINFM_SOAK`'s answers and the password questions' script rule.

Tests 308 (303 and the five of `ask.rs`).

### Calls made in this milestone, for Jeremy to check

- The buttons read left to right Cancel, then the others, the primary last and filled (the settings
  dialog's order); the replace question's are Cancel, Skip, Replace, Replace the default as the message
  box's Yes was; the zip delete's default is Delete, as before.
- The window is dimmed behind a question (black at 40%); the dialog has no icon (the message boxes
  had a warning or question mark).
- Return on a focused button presses that button (Windows' rule); in the password field, or with the
  focus on the primary, it presses the primary.
- Buttons everywhere now show a ring when Tab reaches them (the settings dialog's too).
- A file dropped onto the window from another program while a question shows is not refused (the
  scrim stops TwinFM's own pointer, not OLE's drop target); the job it starts waits behind the question.

### Scripted checks (2026-10-25, the branch's build at 541bdf9, `TWINFM_SCRIPT` without `TWINFM_SOAK`)

A scratch folder, `%TEMP%\twinfm-p12m3-check` (sixteen folders, `a.txt`, and `z.zip` holding `a.txt`
and `b.txt`), made afresh for each run:

- **Replace**: `copy` of `a.txt` into the zip in the right pane: `[ask] shown replace "Replace files in
  z.zip?" (from fileops, 0 waiting)`, `answer Replace`, `[archive] 1 file(s) already in ...: replace`,
  the job Done.
- **Zip delete**: `delete` of `a.txt` inside the zip: `[ask] shown zip delete "Permanently delete 1
  item?" (from fileops, 0 waiting)`, `answer Delete`, the job Done (the zip left holding `b.txt`).
- **Open many**: sixteen folders and Return: `[ask] shown open many "Open 16 items at once?" (from the
  UI thread, 0 waiting)`; `answer Cancel` gave `[open] ... not opened (the question was declined)`; again
  with `answer Open`, sixteen `[tab] pane=0 opened tab` lines.
- **30 s**: the open-many question left alone: `[ask] open many unanswered after 30 s: Cancel (scripted
  run)`, answered after 30,013 ms, nothing opened.
- **Quit**: `quit` with the question shown: `[ask] quit: 1 question(s) answered Cancel`, nothing opened.
- **The queue**: the open-many question shown, then a file dropped into the zip (`dropfiles 1 -1 copy`):
  the worker's replace question waited; after `answer Cancel` it showed (`from fileops, 0 waiting`),
  `answer Skip`, the job Done 2.7 s after it started (its wait included).
- Screenshots of the replace, zip delete and open-many questions in both schemes (the scrim, the card,
  the primary focused with its ring). No `panicked` line in any run. The password questions are the
  console's: a scripted run never shows them (the script's rule).

### Soak

None: as the plan says, the soak never meets a question (`TWINFM_SOAK` answers the replace, zip delete
and open-many questions itself, and its passwords come from `zippassword`); `tools/soak-p5m3.ps1` is
unchanged.

### Gate

ab60, passed (2026-10-25, Jeremy at the console, 23:08 to 23:16 in the six-gate script):
`tools/gate-p12m3.ps1`, `target-p12m3` (541bdf9) against `target-p12m2` (472ceae, its base), the
defaults.

| Setting | p12m2 (WPR) | p12m3 (WPR) | Medians | Render p50 |
|---|---|---|---|---|
| defaults | 20.6, 20.7, 20.7 | 21.1, 21.2, 20.6 | 20.7 / 21.1 | 2.86 to 2.97 ms |

No gap that counts: not every reference run sits below every candidate run (the candidate's 20.6 under
the reference's 20.7), and the medians differ by 0.4 of a point. The scenario asks no question, so it
proves only that the rest is untouched; no panic. The whole session ran in the costlier state (render p50
2.86 to 3.07 ms in every binary's defaults runs, against about 2.2 normally; as ab42 in Phase 10), so the
20% bar was met by neither build; read like for like.

### Console checks (for Jeremy, unelevated, after the gate, `target-p12m3`, light and dark)

- Each question: a file pasted into a zip that holds one of the same name (Replace, Skip, Cancel); a file
  deleted inside a zip; a zip with a password (`C:\traces\password-twinfm.zip`, password `twinfm`) given a
  file, with a wrong password first (asked again), then the right one; a password PDF's "Enter
  password…", wrong then right; more than fifteen items opened at once.
- In each: Tab between the buttons (the ring), Return, Escape (Cancel), a click outside the card (nothing
  happens).
- A question while a copy runs: a large copy started, then a file pasted into a zip that holds it: the
  zip's job waits behind the copy, and its question shows when the job starts, not hidden behind the
  shell's progress window.

## P12-M4: comfortable rows

The look stream's fourth milestone, opened 2026-10-25 on the P12-M3 branch's head (1a11a6e), branch
`feat/p12-m4-rows`. A setting for 30 px rows in the details list, the canvas's compact density.

### Settled first (2026-10-25: reading, then the spike in the coordinating session's slot)

- **Where the row height lives**: `FileRows` and `FileRowsNoType` (and their focus layers) each set the
  row's 22 px as a constant, and their `init` sets `content-height` to rows × 22 px; `FileListHost`
  chooses one of six components per pane and keeps `row-h` for its own arithmetic and the pane's
  overlays (the rename editor, the drop highlight, paging, the empty space all read `list.row-h`).
- **Rust read "taller than 22 px" as "thumbnails"**: `icons::ListGeom` was `new(thumbnails)` and its
  `name_x` was `6 + row_px (+ 4)`, and `dnd::name_cell` rebuilt the geometry from `row_px > 22`. A 30
  px row would have moved the name to 36 px and the drop target's name cell with it. So `ListGeom`
  keeps the icon column apart from the row: `new(thumbnails, comfortable)`, `name_x` from the icon
  size (28 px for the 16 px icons in either row height, 42 with a thumbnail), and `of_row(row_px)` for
  dnd's layout, which carries the height alone (32 px rows are the thumbnails'). The strip overlay
  already centres an icon in its row (`(row_px - icon_px) / 2`, 7 px of a 30 px row).
- **The spike: a height binding or four more components** (the plan's: a binding only if it costs
  nothing the in-app runs see). Base, P12-M3's build (541bdf9's code), against binding, the two 22 px
  row components' height bound to `row-h` (22 px in the run); the four-component shape costs nothing by
  construction at 22 px (today's components unchanged), so it was not built. Two batches of twelve
  in-app runs of the gate scenario on `C:\traces\bigdir` in the coordinating session's slot, the first
  base, binding, binding, base three times, the second swapped, unelevated, no WPR. Nobody was at the
  console: every run presented at a frame interval p50 of 47.0 to 47.5 ms (about 21 Hz), render p50
  2.72 to 2.95 ms, one state throughout.

| Batch | base `[busy]` | median | binding `[busy]` | median |
|---|---|---|---|---|
| 1 | 8.9 / 9.8 / 12.5 / 10.2 / 13.9 / 8.4 | 10.0 | 12.2 / 8.3 / 12.0 / 7.6 / 11.6 / 12.8 | 11.8 |
| 2 (swapped) | 11.1 / 10.3 / 10.8 / 9.0 / 7.5 / 7.8 | 9.65 | 11.7 / 9.8 / 9.5 / 8.9 / 11.7 / 10.0 | 9.9 |
| all 24 | 7.5 to 13.9, mean 10.0 | 10.0 | 7.6 to 12.8, mean 10.5 | 10.8 |

  Render p50 per frame the same (base 2.73 to 2.95 ms, binding 2.72 to 2.84). The first batch's 1.8-point
  median gap is the binding's runs falling in two clusters (about 8 and about 12; means 10.6 / 10.75);
  swapped, the medians are 0.25 apart; over all 24 the means differ by half a point inside each build's
  own spread of five to six. **The binding is taken**, as P9-M1's width binding was: the 22 px
  components' rows are `row-h` tall. The console triplets at the gate decide as usual; the four-component
  shape is the fallback if they read a gap.

### What landed (2026-10-25)

- **24e8425**: the setting `row_height` (`"compact"`, the default, or `"comfortable"`; anything else and
  a file without the key read as compact; `Config::comfortable`), the dialog's Rows row under View (a
  choice, Compact (22 px) or Comfortable (30 px); the dialog 710 px tall, 670 before), `settings set
  row_height compact|comfortable` through the dialog's stage function, `[settings] dialog: row_height
  compact -> comfortable` and `[icons] list geometry: 30 px rows, 16 px icons`. On OK
  (`set_row_height`): the window's `comfortable` property (the host's `row-h` 30 px for the 22 px lists;
  the 32 px thumbnail lists and the grids unchanged), the strips composed again at the new pitch (the
  icons stay 16 px, so their keys stand and nothing is listed again), and each list scrolled so its
  first row stays at the top. `ListGeom::new(thumbnails, comfortable)`, `of_row`, `name_x` from the icon
  column; `StripGeom::new(scale, list)`; `main.rs` `list_geom` for every reader of the geometry;
  `dnd::name_cell` through `of_row`. Tests: the geometry for both heights (the strip's icon places at 1
  and 1.5 times, the name's x, thumbnails winning, `of_row`), the name cell at 30 px, the setting's
  default and round trip, the dialog's change; two lines of `shellmenu.rs`'s menu-point test take the
  new `ListGeom::new`. The soak cycle (below).

Tests 309 (308, the setting's new one; the geometry's and the name cell's extend existing tests).

### Calls made in this milestone, for Jeremy to check

- The setting is a choice of two in the dialog's View section ("Rows"), not a switch, so a third
  density could join it later.
- Turning it on or off keeps each list's first row at the top (the list scrolls in pixels, so without
  that the view would jump).
- With thumbnails on, the rows stay 32 px whatever this setting says (the plan's).

### Scripted checks (2026-10-25, the branch's build, `C:\traces\bigdir`)

Through the dialog (`settings open`, `settings set row_height comfortable`, `settings ok`):
`[settings] dialog: row_height compact -> comfortable` and `[icons] list geometry: 30 px rows, 16 px
icons`; `click 0 5` put the cursor on row 5; PageDown went to row 20, the last row wholly on screen at
30 px (648 px of list); back to compact, `[icons] list geometry: 22 px rows, 16 px icons`, and `click 0
5` on row 5 again. Screenshots in both schemes: the 30 px rows with their hairlines, the 16 px icons
centred, the names at the same x as in 22 px rows, the selection and cursor ring on the taller row.

### Soak

`tools/soak-p5m3.ps1` gains a cycle: comfortable rows on through the dialog in `src\tree`, a click and
PageDown, and off again; the report counts the switches each way and the 30 px geometry lines (one each
per cycle). Not run by this stream: the coordinating session schedules soaks.

### Gate

To run (the coordinating session builds `target-p12m4` from the code head 24e8425):
`tools/gate-p12m4.ps1`, the defaults and comfortable rows (`C:\traces\rows-comfortable.toml`, read-only,
holding the one line `row_height = "comfortable"`; the coordinating session makes it), against
`target-p12m3`, the wrapper's tag ab61 unless the coordinating session allocates another. The reference
reads the file too (it ignores the key it does not know) and runs its 22 px rows, so the comfortable
pair compares 30 px rows with 22 px ones over the same scenario; the wrapper checks the 30 px geometry
line in the candidate's runs only. The defaults pair is the spike's question at the console.

### Console checks (for Jeremy, unelevated, after the gate, `target-p12m4`, light and dark)

- Both heights beside Explorer's details view, switched in Settings > Rows; the first row stays at the
  top across the switch.
- In 30 px rows: a drop onto a row (it lights the row it lands on), a rename (F2: the editor centred in
  the row), PageDown and PageUp, a right-click and Shift+F10 (the menu under the row).
- Thumbnails on with comfortable rows set: the rows stay 32 px.
