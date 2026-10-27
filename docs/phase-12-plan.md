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
