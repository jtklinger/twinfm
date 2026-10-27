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
as ab42 in Phase 10), so neither build came under the 20% bar in the defaults or the preview column (the
grid with thumbnails is far under it in both); read like for like.

### Console checks (for Jeremy, unelevated, after the gate, `target-p12m2`, light and dark)

- `C:\traces\bigdir` beside Explorer: the thumb easy to find at rest; dragged top to bottom; a click on
  the track above and below it (a screenful each); the wheel over it.
- The grid at large and medium tiles with thumbnails: the thumb in the right margin, a drag, a track
  click.
- The sidebar with enough bookmarks to scroll, and a crumb dropdown of a big folder (`C:\Windows`).
- A PDF at 400%: both bars, each dragged; a long Markdown file's bar (the drift recorded since Phase 8 is
  the list's own height estimate, unchanged).
- Whether the thumb meeting the date's last digit at rest bothers you (§ Calls made).

Jeremy's results (2026-10-26, on `target-p12m5`, which holds this milestone's code): his words for the
zoom item, "#8-There's no ability to zoom so I couldn't test scroll. others looked good." (the PDF zoom
is Ctrl+wheel or Ctrl+plus over the column, P11-M4; the coordinating session told him, no change); and
on the thumb against the date's last digit, his decision, "Add padding".

- **c35133d**, the padding. The four details row components (with and without the Type column, 22 and
  32 px rows) end 10 px from the list's right edge, 6 before (`dnd::ROW_PAD_RIGHT`, in place of
  `name_cell`'s 6.0); the grid keeps 10 px at its right too (`cols`: `floor((width - 10px) / tile-w)`),
  so a last tile never sits under the thumb. The scripted check (`C:\Windows\System32`, both schemes,
  the details list, 32 px thumbnail rows and the large grid): the date's last digit ends 4 px left of
  the thumb at rest (x 1203 against 1208 at 100%), the grid's last tile well clear; under the pointer
  the 10 px thumb still covers the last pixel of the text, as the 6 px padding let it cover five.

  **What moves**: the Type, Size and Modified columns and the name cell's right end, 4 px left in every
  details row; `dnd::name_cell` (the drop target's name cell and the column drags' name floor,
  `main.rs` `name_px`) and its test (each width 4 px less). A grid pane whose width left under 10 px past
  its last tile shows one tile fewer to a row (the tile count is Slint's, which Rust and the driver read,
  so their aims follow; the gate's panes and the soak's left 20 px or more, so their counts stay).
  **What does not**: the header (its cells were already flush with the list's right edge, 6 px right of
  the rows' columns, now 10; the labels sit at the cells' left and the values at the rows' right, so the
  offset does not show), the column grips and `coldrag`'s aims (on the header's boundaries), the rename
  editor (it already ended 6 px short of the Type column; now 2), the driver's row aims (the list's
  middle), `drag`'s thumb aim, every hit test (whole rows). Needs: ab58's settings (the defaults, the
  grid with thumbnails, the preview column) re-run against `target-p12m1`; no aim moves, so no soak for
  this change by itself.

ab63, passed (2026-10-26, Jeremy at the console, done 18:20, 25 min): the padding's re-gate,
`target-p12m2` rebuilt from c35133d against `target-p12m1` (e0c9a28), ab58's settings
(`C:\traces\gate-p12m2-ab63.ps1`: `tools/gate-p12m2.ps1` with its head and tag changed).

| Setting | p12m1 (WPR) | p12m2 (WPR) | Medians | Render p50 |
|---|---|---|---|---|
| defaults | 22.3, 21.6, 21.4 | 21.5, 21.0, 21.2 | 21.6 / 21.2 | 2.86 to 3.10 ms |
| grid-thumbs | 12.3, 12.0, 12.1 | 11.8, 11.9, 11.7 | 12.1 / 11.8 | 1.45 to 1.54 ms |
| preview | 20.8, 21.1, 20.7 | 20.7, 20.7, 21.0 | 20.8 / 20.7 | 2.69 to 2.91 ms |

No gap that counts in any setting; the candidate's medians under the reference's in all three; no panic.
Both builds again in the costlier state (render p50 2.86 to 3.10 ms in the defaults runs), read like for
like.

Jeremy's re-checks (2026-10-26, on `target-p12m5` rebuilt from 3d5e162, which holds the padding), his
words: the padding, "1. looks fine."; the PDF zoom's bars, "2. look good".

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

Jeremy's result (2026-10-26, on `target-p12m5`, which holds this milestone's code), his words for this
item: "#10, tabs do not flow smoothly." A scripted check then pressed Tab four times and Shift+Tab twice
in each question a script can show (open-many, the zip delete, the replace), a screenshot after each
key: after the last button, Tab moved to an invisible stop (no ring anywhere), and only the next Tab
reached Cancel; Shift+Tab from Cancel stopped there too. The stop was the card's own `FocusScope`, the
one that takes Escape and Return from the buttons, which Slint makes a Tab stop by default
(`focus-on-tab-navigation`); the hidden password field was skipped, and Tab never left the card. The fix
(below) sets `focus-on-tab-navigation: false` on it: the same check in both schemes then went Replace,
Cancel, Skip, Replace (open-many: Open, Cancel, Open; the zip delete: Delete, Cancel, Delete), Shift+Tab
the other way, every stop a button with its ring; Escape still answered Cancel in each. The password
questions cannot be shown in a script (`zippassword` and `pdfpassword` answer them unseen): their order
is the field, Cancel, OK, back to the field; Jeremy's console check. The driver gained the key names
`Tab` and `ShiftTab` for it.

- **bc63834**: the question card's key handler takes no Tab stop (`focus-on-tab-navigation: false`); driver
  key names `Tab`, `ShiftTab`.

Jeremy's re-check (2026-10-26, on `target-p12m5` rebuilt from 3d5e162), his words: "3. Tab moves, but
it's tough to determine exactly which icon or pane is the active tab. On some tabs, it doesn't even show
a row or icon that's highlighted with the tab." "Which icon or pane" reads like Tab in the main window
rather than in a question; the coordinating session asked him which he meant, and the look stream
surveys every Tab stop of both meanwhile.

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

## P12-M6: Google Drive's commands, and its volume

Started 2026-10-25 from main at 76b5d35 (the Phase 12 plan), branch `feat/p12-m6-gdrive-commands`, the
cloud stream's first milestone. Three items, each settled first, each a commit of its own.

### Settled first (2026-10-25)

**Why the ids differ.** Settled by a probe in TwinFM's own process (the release exe, a scripted run;
the probe code was never committed): the classic menu `show` builds on the UI thread, dumped command by
command with its id and verb before and after the menu ran, then the same menu built again on the UI
thread (with the main window as owner and with none), and the verb worker's rebuild (`menuverb 0
sleep:10`, so nothing ran), all on Jeremy's PDF in `G:\My Drive\Automobiles\2020 Audi A3`, reading
menus only. `QueryContextMenu` returns 0xED on the UI thread and 0xEC on the worker, one id fewer;
compared command by command (Open with's apps left aside, a submenu the shown menu never opened),
**the one missing is "Scan with Microsoft Defender..."** (id 200 in the shown menu, offset 100);
every command numbered after it sits
one id lower on the worker, and Google Drive's are all after it: "Available offline" 208 shown, 207
rebuilt (so 208 holds "Online only" there, Jeremy's refusal), "Manage versions with Google Drive" 210
shown, 209 rebuilt (and no 210 at all, his other refusal). **The doubled "Move to OneDrive" is not the
cause**: both menus list both (the packaged command, verb `{1FA0E654-...B01}`, and OneDrive's classic
one with no verb). "Manage versions with Google Drive" is in the rebuilt menu, one id lower. A local
file in `%TEMP%` shows the same: Defender's item at 216 shown, absent on the worker
(`QueryContextMenu` 0x102 against 0x101), and "Move to OneDrive" (no verb) at 300 shown, 299 rebuilt.

**Why Defender's item is missing.** Not the apartment: the worker initialised with `OleInitialize` in
place of `CoInitializeEx` still had no Defender item. The owner window: the same worker, given a
hidden window of its own as the owner (`parse_items` and `GetUIObjectOf`), built a menu identical to
the shown one (0x102, Defender at 216); the UI thread with no owner had it too, its own window being
active. So Defender's handler adds its item only when the menu has an owner window it can use, and
the worker has none by P7-M1's decision (a dialog a verb shows is unowned, so the two threads' input
queues are never attached; console-checked then). A throwaway test listed every command without a
verb whose id differs between a menu with an owner and one without, over 24 local file kinds and a
folder: always Defender's item missing and every later command one lower; on local files the moved
commands without a verb are "Move to OneDrive" (all), "Sync or Backup this folder" (a folder),
"Cast to Device" (pictures and video, its device list racing), "Install" for fonts and "Edit in
IDLE" for `.py` files.

**The fix**, as the plan has it, and not the owner window: a pick hands the worker its label path, and
when the item at the picked id has another label, the one command without a verb at that path in the
rebuilt menu is invoked. Giving the worker an owner window would make the ids match (and bring
Defender's scan back), but changes P7-M1's checked rule for every verb's dialog; left as an option
(§ For Jeremy below). Commands with a verb never moved anything: they are invoked by their verb.

**Google Drive's volume.** On this machine: `QueryDosDeviceW("G:")` is `\Device\Volume{f5ae2bcb-...}`,
no driver's name; `GetVolumeInformationW` reads label "Google Drive", file system FAT32, flags 0x106;
the driver `googledrivefs31931` is a file system driver service whose name carries Google's build
number, so asking for it in the volume's path would mean finding the current name first;
`HKCU\Software\Google\DriveFS\Share\SyncTargets` holds the mount point in an undocumented protobuf
(`...12 02 "G:"`); the root holds `$RECYCLE.BIN`, `.Encrypted`, `.shortcut-targets-by-id`, `My Drive`
(a translated name) and `Other computers`. Taken: **`.shortcut-targets-by-id` at the volume's root**,
one attribute read once per volume, language-independent, with the label as the fallback.

**The device notification.** `RegisterDeviceNotificationW` on a `G:` handle fails with os error 1066
at every watch (P11-M3 registers hot-pluggable fixed drives, and `G:` reports hot-pluggable). The
volume is remembered by its GUID path (`GetVolumeNameForVolumeMountPointW`, `\\?\Volume{4ec70a0a-...}\`
for `G:` here), so another disk later given the same letter is asked again.

### What landed

1. **A moved command is found by its label path: done** (f3f3422). `show` hands on the picked
   command's label path (`label_path` over `command_tree`: "Offline access>Available offline"); the
   worker's `choose` checks the item at the id by the path's last label, and when that refuses,
   `find_moved` takes the one command without a verb at the whole path in the rebuilt menu (none or
   several: refused, the reason ending `, and none at its label path` or `, and N at its label path`);
   `fill_until` stops as soon as either is found, so a moved command no longer waits 3 s. `[menu]
   "<label path>" moved from offset A to B in the rebuilt menu: by offset B`. It mends every handler's
   commands numbered after Defender's, not only Google Drive's; the Windows 11 menu's packaged commands
   are unaffected. **The driver's dry test verb** `menuverb <pane> try:<label path>`: the command at
   that path in the menu `show` would show (`find_shown`, built by the same `build` as `show`, never
   shown; of two at one path, the one without a verb), handed to the worker as a pick hands it
   (`verbs::Work::Try`), resolved there, and never invoked (`run_verb`'s `dry` returns before the
   invoke is built). Tests: `choose` with a label path, `find_moved` (one match, none, two, the
   owner-drawn empty label, a command with a verb, a lone label against a path) and `label_path`.
2. **Google Drive's volume by its root folder: done** (0d150f6). `cloud::google_by`: the root's
   `.shortcut-targets-by-id`, else the label; `[cloud] volume g: named "Google Drive": google drive (by
   its root's .shortcut-targets-by-id)` (the line's shape changed: a colon, then `not cloud` or `google
   drive (by <how>)`). Tests: the order (the folder first, a renamed drive, the label exactly, neither)
   and the system drive has no such folder.
3. **The refused device notification noted once per volume: done** (77c4280). `register` skips a
   volume in the watcher's `no_notify` set; the first refusal reads `[watch] pane=N tab=T no device
   notification for <path>: <error>: not asked again on \\?\Volume{...}\`.

Also: `CLAUDE.md` § Measuring (the moved command, the label path in the lines, the dry test verb, the
volume line, the watcher's line, 304 tests), the driver's doc line in `src/auto.rs`,
`tools/gate-p12m6.ps1`.

### Scripted checks (2026-10-25, the branch's release build, `TWINFM_SCRIPT`, no settings file)

- **The moved path on a local file** (`%TEMP%\twinfm-p12m6-probe\a.txt`, the dry test verb):
  `try:Move to OneDrive` reads `"Move to OneDrive" at offset 200 in the menu shown, verb ""`, then
  `"Move to OneDrive" moved from offset 200 to 199 in the rebuilt menu: by offset 199` and `resolved: by
  offset (the test verb: not invoked)`; `a.txt` stayed where it was and nothing reached OneDrive.
  `try:Scan with Microsoft Defender...`: `refused: the rebuilt menu has no item at that id ("Scan with
  Microsoft Defender..."), and none at its label path` after 3 s (Defender's item is in no rebuilt
  menu). `try:Open`: `resolved: by verb`. `try:No such command`: `no "No such command" in the menu`.
  No local command without a verb that moves also changes nothing (the list above), so the moved path
  is shown by the dry test verb rather than by a real invoke.
- **The real case on `G:`, read-only** (the same PDF, the dry test verb, nothing invoked): "Offline
  access>Available offline" 108 to 107, "Offline access>Online only" 109 to 108, "Manage versions with
  Google Drive" 110 to 109, "Share with Google Drive" 104 to 103, each `resolved: by offset (the test
  verb: not invoked)`.
- **The volume and the device notification** (`G:\`, `My Drive`, two folders down, F5, a new tab,
  then `C:\Windows`): one `[cloud] volume g: ... (by its root's .shortcut-targets-by-id)`, one `no
  device notification for G:\: ... (os error 1066): not asked again on \\?\Volume{4ec70a0a-...}\`, and
  no refusal for the four watches of `G:` after it; `C:` reads `has no name: not cloud`.
- `cargo test --release`: 304 passed.

One thing seen, not changed: `fill_until`'s `found after T ms (a submenu filled late)` line appears at
51 to 60 ms for commands in no late submenu, because the first check's own time (filling every
submenu on `G:`, Open with among them) passes its 50 ms threshold; the line is P7-M3's.

### Soak

Unchanged, not run: the soak never touches `G:` or a classic-menu pick without a verb, and the device
registration skips only a volume that refused.

### Gate

The defaults triplet, `target-p12m6` against `target-p11close` (a99f2f2), `tools/gate-p12m6.ps1 -Tag
<tag> -Head <commit>`, the tag and the gate build from the coordinating session. The scenario never
opens a menu or a cloud folder and its drives are never hot-pluggable: it proves only that the rest is
untouched.

- **ab57: passed** (no gap that counts, no panics). Run by Jeremy at the console with ab56 to ab61,
  2026-10-25 22:10 to 23:33, `target-p12m6` (578bf25, built `--locked` from a detached checkout by the
  coordinating session) against `target-p11close` (a99f2f2; nothing in Phase 12 had merged),
  `C:\traces\gate-ab57-summary.txt`.

| Run | WPR | In-app | Render p50 (ms) |
|---|---|---|---|
| p11close-1 / 2 / 3 | 20.8 / 21.1 / 21.1 | 21.7 / 22.0 / 24.4 | 2.89 / 2.89 / 2.98 |
| p12m6-1 / 2 / 3 | 21.0 / 21.4 / 21.2 | 24.1 / 24.1 / 22.5 | 2.93 / 2.88 / 2.88 |

  Medians 21.1 (reference) / 21.2 (candidate), 0.1 points, and not every reference run below every
  candidate run (21.1 above 21.0): **no gap**. The whole session ran in the costlier state (render p50
  about 2.9 ms for every binary, against about 2.2 normally), so neither build is under the 20% bar; read
  like for like.

### Console checks (for Jeremy, unelevated, `target-p12m6`, beside Explorer)

1. On a file in `G:\My Drive`: Show more options > Offline access > Available offline, then Online
   only; Google's mark changes as in Explorer, and no "was not run" note.
2. Show more options > Manage versions with Google Drive, and Share with Google Drive: Google's windows
   open.
3. Another program's moved command on a local file (the plan named one "below the doubled Move to
   OneDrive"; the cause is Defender's item instead): on a `.py` file in a scratch folder, Show more
   options > Edit in IDLE > Python 3.13 (64-bit) opens IDLE with the file.
4. Show more options > Scan with Microsoft Defender... on any file: refused with the note "... was not
   run: ..., and none at its label path" (as before, with the longer reason).

**Jeremy's results** (2026-10-26, `target-p12m6` at 99cb871, through the coordinating session, whose
numbering for him was 1 = check 3 above and 2 = check 4), his words: "Cloud checks-1 & 2 work
correctly." So Edit in IDLE opens the `.py` file in IDLE (check 3), and Defender's scan is still refused
(check 4).

**Checks 1 and 2, Google Drive's commands: passed** (2026-10-26, the same build, the log on), his words:
"Cloud checks all ran as expected, no errors". On a `G:\My Drive` file that was online only, Show more
options > Offline access > Available offline ran with no note and the mark changed as in Explorer, then
Online only took it back to the cloud (check 1); Manage versions with Google Drive and Share with Google
Drive opened Google's windows (check 2). Every console check of P12-M6 has now passed.

### For Jeremy, answered

- **Defender's scan from TwinFM's classic menu** stays refused: its handler adds the item only to a
  menu with an owner window, and the verb worker has none. A hidden window of the worker's own as the
  owner brings it back (and makes every id match), but every dialog a verb shows would then be owned by
  that hidden window, so P7-M1's console checks would be owed again. Jeremy's answer (2026-10-26,
  through the coordinating session): "keep it refused". Not done, and not on the roadmap.

### After the console check: OneDrive's Free up space (2026-10-26)

Jeremy, checking P12-M7 (#26, Free up space, then Always keep on this device, on a OneDrive file): "p12-m7,
free up space does not work, files still show green checks." He ran it again himself with logging on,
his words: "Using the target-p12m6\release\twinfm.exe, I turned on logging and ran the free up space on
several files in one drive." His log (`C:\traces\jeremy-freeup-2026-10-26.log`), Show more options over
five OneDrive files: `[menu] verb "慍敫癁楡慬汢佥汮湩e" ("Free up space") queued on the verb worker`, then
`InvokeCommand ... by verb = 0x80070483` (no association) and the pane's note `"Free up space" failed:
0x80070483`.

**Cause.** Settled by a probe in TwinFM's own process (P12-M6's build, never committed: every command's
`GetCommandString` answers dumped raw, on the UI thread's menu and the worker's, on one OneDrive file
and on four). OneDrive puts Free up space and Always keep on this device in the classic menu twice: in
the block of the packaged commands' bridge, whose `GCS_VERBW` answers are right ("MakeAvailableOnline",
"MakeAvailableOffline"), and in its classic handler's block next to Defender's item, whose `GCS_VERBW`
answers are the same verbs as **ANSI bytes in the wide buffer** (`614D 656B 7641 ...`, "Ma", "ke",
"Av" read as UTF-16, with no wide terminator after them) while its `GCS_VERBA` answers are right.
Jeremy picked the classic handler's item, so TwinFM queued the garbled verb, which no handler takes; the
fallback by offset then met Defender's shift (that handler's items sit after Defender's: offset 168 in
the menu shown, 167 on the worker) and, being the bare-id check, refused. P12-M6's earlier dry check had
resolved the bridge's twin, the first at that label path, which is why it read the verb right.

**The fixes** (one commit): `verb_of` takes a `GCS_VERBW` answer only when it is plain ASCII (canonical
verbs and the packaged commands' CLSIDs all are), else reads `GCS_VERBA` and takes it when that is,
else no verb, so the label path decides (`verb_text`, logged once per text per process); and when
invoking by verb fails, the fallback by offset uses the label path as a command without a verb does
(`find_moved` with the command's verb), never a bare id holding another item. Tests: the exact garbled
text from the log read again as "MakeAvailableOnline", nothing usable in `GCS_VERBA`, a plain verb and a
CLSID never asking it, no verb; `find_moved` with a verb (one match, OneDrive's two twins refused).
305 tests.

**Scripted** (the dry test verb, four OneDrive files in `Documents`, nothing invoked; the files' attributes
unchanged): `[menu] GCS_VERBW gave "慍敫癁楡慬汢佥汮湩e" at offset 150: GCS_VERBA "MakeAvailableOnline"
taken` and the same for "MakeAvailableOffline" at 151, once each; both commands `resolved: by verb`
("MakeAvailableOnline", "MakeAvailableOffline").

**What it needs**: no triplet (the classic menu's verb path only, nothing per row or frame); the real
invoke is Jeremy's console check, which he has already allowed: Free up space on several OneDrive files
from Show more options, then Always keep on this device, beside Explorer.

**Jeremy's re-check** (2026-10-26, `target-p12m6` rebuilt from 99cb871 by the coordinating session, the
log on), his words: "Free up space is now working correctly. I had the log on if you need to review it."
His log (`C:\traces\jeremy-freeup-recheck-2026-10-26.log`): the two repairs, `GCS_VERBW gave
"慍敫癁楡慬汢佥汮湩e" at offset 173: GCS_VERBA "MakeAvailableOnline" taken` and "MakeAvailableOffline" at
174, once each; `InvokeCommand verb="MakeAvailableOnline" offset=173 label="Free up space" by verb =
0x00000000`; the freed files (the `bigdir` archives, a PDF in the OneDrive root) reading state 10, sync
pending, while OneDrive released them (P9-M4's rule); and Always keep on this device twice, `by verb =
0x00000000` (offsets 122 and 178).

## P12-M7: OneDrive's syncing and error icons

Started 2026-10-25 from main at 76b5d35, branch `feat/p12-m7-onedrive-icons`, the cloud stream's second
milestone, its Settled first begun while P12-M6 waited for its gate. Jeremy's word on the one write into
OneDrive the brief needed (2026-10-25, through the coordinating session): "Yes: the cloud session may copy
one 1 KB test file into a OneDrive probe folder and delete it afterwards, after the memory check and the
two gates."

### Settled first (2026-10-25)

**The way in: (a), the placeholder mode per thread.** Settled by a probe in TwinFM's own process (the
release exe, a scripted run; the probe code was never committed): one thread walked
`C:\Users\jtkli\OneDrive` through `entry_of` three times, in Windows' default placeholder mode, with
`RtlSetThreadPlaceholderCompatibilityMode(PHCM_EXPOSE_PLACEHOLDERS)` on that thread only, and in the
default mode again; the walk stopped at 40,023 entries in 3,984 folders. Nothing was opened or
downloaded.

| Mode | Time | Placeholder states | Kinds | Thumbnail keys | `look:` keys |
|---|---|---|---|---|---|
| default | 8.4 s (cold), 6.2 s | all 0 | Directory 4,012, File 36,011 | 20,520 | 4,012 |
| shown | 6.1 s | 0: 13, 0x9: 5,266, 0xD: 343, 0x39: 34,401 | Cloud 40,010, File 13 | 0 | 4,012 |

Shown, every placeholder carries its real state at no cost to the listing; every one of them is in sync
today (`CF_PLACEHOLDER_STATE_IN_SYNC`, 0x8, set: 0x9 a placeholder in sync, 0xD with its essential
properties too, 0x39 with its data not all on this disk, `PARTIAL` and `PARTIALLY_ON_DISK`), so
the steady state adds no property read. The 13 that are no placeholder are the `desktop.ini` files and
one hidden system file (as P9-M4 found). The three entries both modes flag as needing a read are P9-M4's
releasing rule on unpinned folders (`Documents\Stack`, two folders under `Archive Docs_Online_only`),
which are looked up as folders anyway. **What the mode changes**, kept as today: each placeholder gains
`FILE_ATTRIBUTE_REPARSE_POINT` (0x400: `0x20` and `0x420` for the same PDF, `0x10` and `0x410` for a
folder, `0x00100031` and `0x00100431` for `Stack`; a file whose data is not all here is also offline
and sparse when shown, 0x1200 more, which the entry-by-entry check below found) and a cloud tag
(`0x9000601A`, `0x9000E01A`), so it becomes `Kind::Cloud` (folders too), and a `Kind::Cloud` file gets no
thumbnail key (20,520 to 0). So `entry_of` takes the state and lists the entry as the plain file or
folder it is by default (`as_listed`), and only the threads that build a sync root's entries show
placeholders: each listing's own (when its folder is under a sync root) and the watch's edit worker. The
search walker, the size walker and a search tab's re-stat keep Windows' default (a search tab is no
cloud location: its keys carry no marks). Ways (b), the folder's own state first, and (c), the property
read for every file on screen, are not needed.

**Whether OneDrive's switch to in sync reaches the watch: it does.** The probe Jeremy allowed (the
coordinating session's probe slot, 2026-10-25): TwinFM (this branch's build with one timed log line in
`land_edits`, never committed) showed `C:\Users\jtkli\OneDrive\twinfm-p12m7-probe\`, and a PowerShell
poller wrote one 1,022-byte file there and logged its attributes and reparse tag every 250 ms. The file
was written at 0.0 s; its creation reached the watch 0.25 s later (`records 2, changes 1`, the edit
`probe.txt attrs 0x20`). For 5.7 s OneDrive left it a plain file, no placeholder at all (attributes
`Archive`, no tag); at 5.7 s it became an in-sync placeholder (`Archive, ReparsePoint`, tag
`0x9000601A`), and one record reached the watch 0.2 s after that (`records 1, changes 1`, re-statted on
the edit worker, `applied: +0 -0 ~1`). So the mark follows the file through the watch, with no timed
re-read (the plan's fallback, per-row work on the icon workers, is not needed). The file and the folder
were deleted afterwards; nothing else was written. **What the probe also showed**: while it uploads, the
new file is not a placeholder, so the rule "a placeholder not in sync" missed it, and the row drew the
"on this device" tick where Explorer draws the syncing arrows. Hence the second half of the rule below.
**P9-M4's console check 4** ("shows the syncing or pending mark while it uploads", answered "as
expected") must have been the folder's mark: in TwinFM's process the file was a plain file then, keyed
by its attributes (the tick).

**How to make an error for the console check**: OneDrive does not sync a file whose name contains
`_vti_` (a name SharePoint reserves), which Explorer shows with the error mark; if Explorer shows none for
it on Jeremy's OneDrive, the error glyph's check stays open (the glyph table's test covers the drawing).

### What landed

1. **Placeholders shown on the threads that build a sync root's entries: done** (5dfc704).
   `fs::enumerate::expose_placeholders` (`RtlSetThreadPlaceholderCompatibilityMode(PHCM_EXPOSE_PLACEHOLDERS)`,
   windows-sys's `Wdk_Storage_FileSystem` feature, no new crate; `PHCM_EXPOSE_PLACEHOLDERS` declared
   locally, windows-sys has the call but not its modes) runs on a listing's own thread when its folder
   is a sync root, and once on the watch's edit worker; a thread-local says the thread is shown them.
   `entry_of` takes the Cloud Files state from the shown attributes and tag (`placeholder_state`, a
   pure cldapi call, made only for a reparse point or an unpinned entry as before) and lists the entry
   as the plain file or folder it is by default (`as_listed`). `needs_state_read` gains `shown`: a file
   there that is no placeholder (just copied in, uploading) is looked up too. `[cloud] pane=N not in
   sync: K of M files` per sync-root listing; `[cloud] <listing | watch edits>: placeholders not shown
   (RtlSetThreadPlaceholderCompatibilityMode = R)` if Windows refuses the mode.
2. **Listed exactly as before: done** (0c7e516), after the check below found two differences: a
   placeholder whose data is not all here is offline and sparse as well when shown (0x1200), now dropped
   with the reparse bit; and OneDrive's own excluded files (pinned and unpinned both: the `desktop.ini`
   files and one hidden system file at the root, `0x00180026`), which no placeholder rule should look up,
   keep no mark as before.

Tests: `a_shown_placeholder_lists_as_before_with_its_own_state` (a shown in-sync file lists as the plain
file with its thumbnail key and attribute mark, a placeholder out of sync and a file not uploaded yet
are looked up, a shown folder is a folder, a thread not shown keeps today's kinds and marks, `as_listed`
for each case; the out-of-sync tag is the first of the sixteen cloud tags that cldapi reads so), the rule
(`placeholders_out_of_sync_are_looked_up`: the shown cases, the excluded file), and the glyph table
(`status_glyphs_come_from_imageres`: the pending states draw the syncing arrows, and syncing, error,
warning and the tick are four different glyphs). 303 tests.

Also: `CLAUDE.md` § Measuring's P12-M7 paragraph and the test count, `tools/gate-p12m7.ps1`,
`tools/check-p12m7-onedrive.ps1` (the plan's in-app comparison on OneDrive).

### Scripted checks (2026-10-25, the branch's release build, `TWINFM_SCRIPT`, no settings file)

- **Every OneDrive entry lists as before**: the probe again, walking the same 40,023 entries through
  `entry_of` in the default mode and shown, comparing each entry's kind, attributes and icon key. First
  run (5dfc704): 5,614 the same, 34,409 different: the 34,401 files whose data is not all here (offline
  and sparse when shown; their keys the same) and the 8 excluded files (looked up). Second run (with
  both fixed, 0c7e516): **40,023 the same, 0 different**; thumbnail keys 20,520 and `look:` keys 4,012 in
  both modes; kinds Directory 4,012 and File 36,011 in both.
- **The real build on OneDrive** (the root, `Documents`, F5, then `C:\Windows`): `[cloud] pane=0 not in
  sync: 0 of 5 files`, `0 of 6 files` twice; the `[enum]` reparse counts as before (no cloud); the
  folders' `state read` lines as before; no refusal line, no panic.
- **The in-app comparison on OneDrive** (`tools/check-p12m7-onedrive.ps1`: the root, `Documents` with F5,
  `Pictures\Camera Roll` scrolled and F5; reference, candidate, candidate, reference). Two trials on a busy
  machine (the look stream building), this branch against `target-p11close` in both orders, then the
  coordinating session's quiet slot (2026-10-25, nobody at the console, about 21 Hz presentation), the gate
  builds `target-p11close` and `target-p12m7` (6441b31). **Property reads (`cloud N`) 41 and `state read`
  lines 3 in every one of the 16 runs**, `not in sync` 0 in every OneDrive listing, no panic. `slow N`: the
  trials 0, 1, 1, 0 and 1, 16, 0, 0 (the 16 a busy run, both builds slow), the quiet run 0, 2, 1, 0; the
  candidate's slow renders were `Personal Vault.lnk`'s icon (135 to 155 ms, a key both builds share) and
  once the `Pictures` folder's lookup at 104 ms. So far 5 of 6 candidate runs against 1 of 6 reference
  runs, so the slot continued for a probe: the same build with the listing thread's mode on and off by an
  environment variable (never committed), on, off, off, on: `slow N` 0 in all four, the shortcut never
  slow, the root's first 14 icons averaging 29.6 and 27.5 ms on against 27.5 and 29.8 ms off; and the gate
  builds again, reference, candidate, candidate, reference: `slow N` 0, 0, 1, 1, the shortcut slow once in
  each build (138 ms in the candidate's second run, 137 ms in the reference's), the root averages 27.7 and
  31.1 ms (reference) against 27.5 and 31.5 ms (candidate). **No difference the change makes**: that one
  shortcut's first render varies from run to run in both builds.
- `cargo test --release`: 303 passed.

### Soak

Unchanged, not run: the OneDrive soak folder is not approved, and the soak lists no sync root.

### Gate

The defaults triplet, `target-p12m7` against the last merged milestone's gate build,
`tools/gate-p12m7.ps1 -Tag <tag> -Head <commit> -Ref <name>`, the tag and the build from the
coordinating session. The scenario never lists a cloud folder: it proves only that the rest is
untouched. The plan's in-app run on OneDrive, `cloud N` and `slow N` compared: done in the quiet slot
(§ Scripted checks: no difference the change makes).

- **ab59: passed** (no gap that counts, no panics). Run by Jeremy at the console with ab56 to ab61,
  2026-10-25 22:10 to 23:33, `target-p12m7` (6441b31, built `--locked` from a detached checkout by the
  coordinating session) against `target-p11close` (a99f2f2; nothing in Phase 12 had merged),
  `C:\traces\gate-ab59-summary.txt`.

| Run | WPR | In-app | Render p50 (ms) |
|---|---|---|---|
| p11close-1 / 2 / 3 | 20.8 / 21.4 / 21.1 | 23.4 / 21.7 / 20.9 | 2.88 / 2.96 / 2.86 |
| p12m7-1 / 2 / 3 | 21.1 / 21.2 / 21.2 | 20.9 / 23.4 / 20.2 | 3.14 / 3.00 / 2.91 |

  Medians 21.1 (reference) / 21.2 (candidate), 0.1 points, and not every reference run below every
  candidate run (21.4 above 21.2): **no gap**. The whole session ran in the costlier state (render p50
  about 2.9 ms for every binary, against about 2.2 normally), so neither build is under the 20% bar; read
  like for like. The candidate's first run read a higher render p50 (3.14 against 2.86 to 3.00), on the
  render path, which P12-M7 does not touch; its WPR figure (21.1) is the reference's median.

### Console checks (for Jeremy, unelevated, `target-p12m7`, beside Explorer)

1. Copy a large file (a few hundred MB, so the upload lasts) into a OneDrive folder TwinFM shows: the
   syncing arrows while it uploads, then the tick, with no F5, as Explorer.
2. A file OneDrive refuses (a name with `_vti_` in it): the error mark Explorer shows (the red cross),
   in TwinFM too; if Explorer shows none, say so.
3. Free up space and Always keep on this device on a file, as in P9-M4: the marks as Explorer's.
4. A OneDrive folder with thumbnails on (the settings dialog): the pictures as before.

**Jeremy's results** (2026-10-26, `target-p12m7` at 6441b31, through the coordinating session, whose
numbering for him was 3 = check 1 above, 4 = check 2 and 5 = check 4), his words: "3 works correctly.
... 5. works as expected." So the large file showed the syncing arrows while it uploaded and then the
tick, with no F5 (check 1), and the thumbnails in `OneDrive\Pictures` are as before (check 4). Check 3 is
open: it needs P12-M6's fix to Free up space from Show more options (P12-M6's record, After the console
check), which this branch has since main, with #116, was merged in (2026-10-26); Jeremy's re-check of it
ran on `target-p12m6`, which does not show placeholders.

**Check 2, the `_vti_` file: passed against Explorer.** His words: "The icon itself doesn't show red X
but the onedrive sync icon in the taskbar does show red x and prompts to rename the file", and asked
whether Explorer showed it on the file, "The file did not show a red X." His log (`%APPDATA%\twinfm\twinfm.log`,
this build, 2026-10-26 19:23; nothing written into OneDrive to read it) shows why: he made
`Documents\test_vti_.txt` with TwinFM's New file, OneDrive never made it a placeholder (`[cloud] pane=0 not
in sync: 2 of 8 files`, then `1 of 8` once the large file was in sync), and both reads of its state gave
4, sync pending, never 7, error (`[cloud] state read ...\test_vti_.txt: 4 in 43 ms`, then `4 in 22 ms`),
so TwinFM drew the syncing arrows (imageres -1401) until he deleted it. OneDrive reports the refused name
through its own taskbar icon and its rename prompt, not through the file's state, so neither Explorer nor
TwinFM draws the red cross. The error glyph itself stays covered by the glyph table's test only; no file
in an error state has been seen here.
