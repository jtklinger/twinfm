# Phase 11: TwinFM sweeps its deferred list and picks up the cut features

Written 2026-10-22, the day after Phase 9 closed (`phase-9-plan.md` § Phase 9 close) and while Phase
10, the UI/UX polish, runs in its own session (`phase-10-plan.md`; P10-M1 the rust-ui look, PR #91,
not yet merged when this was written). Phase 11's goal, chosen by Jeremy on 2026-10-22: **pick up the
remaining deferred bugs and issues, the sweep and the cut features both**. Before the plan, an
inventory of every Deferred and Open-questions list from the Phase 1 to Phase 9 closes was compiled
and checked against the later closes (36 open items; almost none a bug a user hits: the backlog is
gaps cut from milestones and checks owed but never run; the four real bugs are cosmetic or edge
cases). Not in Phase 11, by Jeremy's decision: OneDrive's syncing and error glyphs and the Slint fork
items (the whole-row snap patch, upstreaming the two accessibility patches), both deferred to Phase
12; the binary Office formats (`.doc`, `.xls`, `.ppt`); thumbnails inside archive levels (Windows has
no provider); a sync-root scratch folder for the soak. Accessibility stays deferred for much later
(Jeremy, 2026-10-15).

What Jeremy decided at the kickoff (2026-10-22), from the choices put to him:

- **The scope**: "sweep and cut features", not the sweep alone: the four bugs, the edge cases and the
  owed checks, plus content search and the preview's cut features.
- **Password PDFs are in as P11-M6**: "Add in the password PDFs as P11-m6." When the builder check
  found the library already in the build opens them, PDFium was declined: "keep the library already
  in twinfm, let's not add anything we don't absolutely require."
- **OneDrive's syncing and error glyphs and the Slint fork defer to Phase 12**: "1 and 2 defer to
  phase 12."
- **The order as proposed**: the lists and file-system stream first (P11-M1 to P11-M3), the preview
  stream (P11-M4 to P11-M6) starting once Phase 10's restyle has merged, since both touch
  `src/ui/preview.slint`: "proceed with your suggested order".

The starting point is main at 0a41cda (the Phase 9 close) with P10-M1 in flight on its own branch.
What the six milestones rest on today:

- **Search** (`src/fs/enumerate.rs` `walk`, `src/main.rs`): a recursive name search in a new tab,
  cancelled by Escape, F5, a navigation or a new search; results carry `sub\dir\file.txt`, shown as
  `file.txt  (sub\dir)`, sorted by the file's own name; since P9-M4 their thumbnails are asked as a
  folder's are. The P9-M2 text reader (`markdown::text_blocks` and the encoding rule in
  `src/preview.rs`: a UTF-8 or UTF-16 BOM, else UTF-8 with invalid bytes replaced) decides encodings.
  Results from several folders in one menu or drag are refused (relative PIDLs), results do not follow
  changes (no watch on a search tab), the cursor after a rename in a search tab is unchecked, and there
  is no index (`C:\Windows` takes about 39 s).
- **Cloud marks** (`src/cloud.rs`, `src/icons.rs`): a refresh of a Google Drive or sync-root folder
  puts the rows back to their placeholders until the lookups return (about 0.1 s for a screen of 40);
  the thumbnail cache costs 300 to 570 ms per request on Google Drive; Google Drive's volume is found
  by its name.
- **Column widths** (`src/cols.rs`, P9-M1): six keys, grips, the fit; no reset to the defaults.
- **Watching, file operations, folder sizes** (`src/fs/watch.rs`, `src/fileops/`, `src/undo.rs`,
  `src/fs/sizes.rs`): the Phase 5 close's lists of edge cases stand as written there (a USB hard disk
  gets no device registration so a tab on it blocks "Safely remove"; a folder moved to another parent
  is `Lost`, not `Moved`; up to 8 background handles may block renaming an ancestor; a restore with
  one failed item records nothing; undo of a copy or move that replaced a file does not restore the
  replaced file; Ctrl+Y is not taken from the sidebar, tab strip or crumb button; the subtree-watch
  overflow path never triggered; no size on disk; `waitsize` does not wait for a stale re-walk).
- **Drag and drop, navigation** (`src/dnd.rs`, `src/main.rs`, `src/settings.rs`): no spring-load on a
  sidebar row or a crumb; no history menu and history not saved with restored tabs; mouse Back and
  Forward do nothing over a text entry, the tab strip or the list scrollbar; no browse button or
  validation for the start folders in the settings dialog.
- **The preview** (`src/preview.rs`, `src/markdown.rs`, `src/html.rs`, `src/office.rs`,
  `src/ui/preview.slint`): PDF through hayro `=0.8.0` (no password PDFs, no zoom), Markdown, text (no
  syntax colours), pictures through `image` and `resvg` (HEIC, HEIF and AVIF are "No preview"), HTML
  through TwinFM's scanner (`<meta charset>` not read, the common entity names only, `colspan`
  ignored), Office through `zip` and `quick-xml`. Slint's text elements have no selection at the
  pinned rev. The Markdown details: a centred logo left-aligned, SVG text not drawn, 2 px table
  borders where neighbours meet, the scrollbar thumb drifting on a long file.
- **Owed checks**: the dead network share (P5-M1: `stalled: no watch`, the 30 s cool-down, F5 on a
  dead share), renaming an ancestor of a watched folder, a right-button drag out of Explorer, video
  and Office thumbnails in the lists and tiles; the 187 MB first candidate run of ab20 and ab21.

Six milestones, in the order Jeremy gave on 2026-10-22 (do not fold them), in two streams:

**The lists and file-system stream** (starts at once on the parts that need no screen file: the
walker, the model, the tests, the driver, `src/fs/`, `src/icons.rs`, `src/cloud.rs`, `src/dnd.rs`,
`src/main.rs`, `src/settings.rs`. Every change to `src/ui/pane.slint` or `src/ui/app.slint`, the
content box, the dialog's reset and browse rows, the history menu, the spring hooks on sidebar rows
and crumbs, waits until P10-M1 has merged and is built on the restyled files, so the dialog is never
rebased twice: the builder check's point. P10-M1's gate was the next on the machine when this was
written, so the wait is short):

1. **P11-M1, content search.** The search tab finds text inside files: a second box, or a mode of
   the filter box, settled first against the toolkit and the Phase 10 restyle (the brief checks how
   Ctrl+Shift+F's box is drawn on main at the time). The walker (`fs::enumerate::walk`) opens each
   file whose name matches the name pattern (or every file when it is empty) up to a size cap
   (`CONTENT_MAX_BYTES`, 16 MB; larger files are skipped and counted), decides binary by the first
   8 KB (a NUL byte, or over 10% of bytes outside text ranges after the encoding rule), decodes as
   the preview's text reader does, and searches case-insensitively (a plain substring; a glob or
   regular expression is not in P11-M1) with the matching line kept for the result. Archive levels,
   cloud-only files (by their attributes, never recalled) and junctions are skipped as the name walk
   skips them. **A content search never reads a cloud location**: a search rooted in one (a sync root
   or the Google Drive volume, P9-M4's `search_thumbnails` rule) is refused with a note in the box,
   and a walk that meets a cloud folder on its way skips everything below it, counted as `cloud` in
   the done line, since reading a streamed file downloads it (the builder check). Results show as the
   name search's do, the matching line appended to the name label as `search_label` appends
   `(sub\dir)` (`file.txt  (sub\dir)  the matching line…`, the line cut at 200 characters; one row
   per file, the first match; the count of matches in the status bar; no new list variant and no
   Type-cell change, so the per-row path is untouched), sorted as the name search sorts; Return opens
   the file; the same cancellations apply. Driver: `csearch
   <pane> <text>` (the content box's text and Ctrl+Shift+F), `waitsearch` covers it. `[search]` lines
   gain `content "<text>" in <root>`, `batch K (N so far)`, `done: N found in F files read, S skipped
   (B over the cap, C binary, O cloud-only), D folders walked, in T ms`. Tests: the binary test, the
   cap, the encodings, the first-match line, a tree with a junction and a cloud-attribute file.
   Soak: a content search in `%TEMP%\twinfm-m5\src` per cycle; the report counts content searches and
   files read. Gate: the defaults triplet (the search tab is not in the scenario; the triplet checks
   the walker's and the model's changes cost nothing at rest). Console checks: a search of
   `C:\Users\jtkli\projects` for a word, beside Windows Search or `findstr`; a UTF-16 file found; a
   binary skipped; Escape mid-search; the matching line for a long line.
2. **P11-M2, the cloud and search sweep.** Six small items, each settled first: a Google Drive or
   sync-root refresh keeps each row's old mark by name until the new lookup lands (no placeholder
   flash; `[icons] marks carried: N of M across the refresh`); the Google Drive thumbnail cost
   measured (where the 300 to 570 ms goes: the cache, the provider or `WTS_INCACHEONLY`) and, if it is
   the provider, thumbnails on Google Drive asked only for rows on screen for 500 ms (settled first;
   recorded if not); the search tab's results from several folders in one context menu and one drag
   (absolute PIDLs through `SHParseDisplayName` per item, `shellmenu::parse_items`); results that
   follow changes (a watch per result folder is too many: a stat of each visible result on the tab's
   show and every 5 s while active, with `[search] pane=N results re-stated: R rows, D gone`); the
   cursor after a rename in a search tab (checked, fixed if wrong); and "Reset column widths" in the
   settings dialog beside the Type column row (`settings set cols_reset`, `[cols] pane=N widths reset
   to 120, 90, 140`). Tests for the pure parts (the mark carry, the re-stat diff, the reset). Soak: the
   reset per cycle. Gate: the defaults triplet and the thumbnails triplet (`C:\traces\thumbs.toml`).
   Console checks: F5 on `G:\My Drive` and a OneDrive folder beside the before build (no flash), a
   menu and a drag over results from two folders, a result renamed in another program, the reset.
3. **P11-M3, the edge-case bundle and the owed checks.** From the Phase 5 lists, chosen for what a
   user meets: a tab on a USB hard disk no longer blocks "Safely remove" (device registration for
   `DRIVE_FIXED` removable media, as the `DRIVE_REMOVABLE` path has since P5-M1); a folder moved to
   another parent follows as a `Moved` where the record allows, else the tab goes `Lost` as today
   (recorded either way); a partial Recycle Bin restore records what it restored so it can be redone;
   undo of a copy or move that replaced a file restores the replaced file where the shell's progress
   sink named it (settled first: the sink reports the replaced item's destination; if it does not, a
   note in the undo's refusal); Ctrl+Y from the sidebar, tab strip and crumb button as Ctrl+Z is
   taken; the subtree-watch overflow path exercised by a test (1,000 records in one window) and the
   `waitsize` wait for a stale re-walk; a history menu (TwinFM has no on-screen Back or Forward
   button, the builder check noted: the mouse's side buttons and Alt+Left/Right are the controls) opened
   by Alt+Down in the active pane and from the Windows 11 menu on the path bar's background (the
   pane's history as that menu's list, P7-M2's component, the current entry marked) and history saved
   with restored tabs (`settings::SavedTab` gains `back` and `forward` lists of places, capped at 50;
   a search tab's place is kept as a place that re-runs the search); a browse
   button for the start folders in the settings dialog (the shell's folder picker,
   `IFileOpenDialog` with `FOS_PICKFOLDERS`, on the verb worker) and a note for a path that does not
   exist; spring-loading a sidebar row and a crumb (the P5-M3 rule applied to those spots). The owed
   checks, run at last and recorded: the dead network share (a share name that does not resolve, then
   one that resolves but does not answer: `stalled: no watch`, the 30 s cool-down, F5), renaming an
   ancestor of a watched folder (the "folder in use" case: which handles hold it and whether the
   background handles should close on the parent's rename record), a right-button drag out of
   Explorer (without Logi Options+ running), video and Office thumbnails in a 32 px list and the
   tiles (a fixture made for it), and the 187 MB first candidate run (one heap-sampling trace of a
   first run against a second, recorded as answered or as noise). Each item is a commit of its own;
   the record names which were done, which were checked and found right, and which were left with a
   reason. Driver steps and lines per item as each needs (`histmenu <pane> back|forward`, `histpick
   <index>`; `[nav] history menu: K entries`, `[devices] registered <drive> (fixed, removable
   media)`). Gate: the defaults triplet. Console checks: each item beside Explorer.

**The preview stream** (starts when P10-M1 has merged; touches `src/preview.rs`, `src/markdown.rs`,
`src/html.rs`, `src/ui/preview.slint`, `Cargo.toml`):

4. **P11-M4, the preview gaps bundle.** PDF zoom (Ctrl+wheel and Ctrl+plus/minus over the column,
   50% to 400% in steps, `preview_zoom` saved; the pages re-rendered at the zoom times the column's
   width, the 8,192 px cap kept, a horizontal scroll inside the column for a page wider than it;
   `[preview] zoom Z%`); HTML's `<meta charset>` and the `Content-Type` meta read for Windows-1252,
   ISO-8859-1 and UTF-16 (a table of the single-byte code pages TwinFM knows: 1252 and 8859-1 and -15
   in P11-M4, others shown with replaced bytes and noted), the full HTML5 named-entity table (2,231
   names, a generated `match` or a sorted table, settled first for binary size), and `colspan` as
   repeated cells; HEIC, HEIF and AVIF through Windows' own image decoders (`IWICImagingFactory`,
   settled first: the HEIF extension from the Store must be installed, which the brief checks on this
   machine; absent, the message "No decoder for this format is installed"); the Markdown details (a
   centred `<p align=center>` or `<div align=center>` honoured for pictures, the 2 px borders drawn as
   1 px, SVG text drawn with the system font through `resvg`'s font database when it costs under 50
   ms at first use, else left as recorded); and "Copy text" in a context menu on the preview column
   (a right-click on a block offers "Copy this block" and "Copy all"; the column takes no keyboard
   focus, so Ctrl+A and Ctrl+C stay the lists', the builder check's point), the nearest thing to
   selection at the pinned Slint rev, recorded as such. Tests: the zoom's page sizes, the charset table, the entity table's lookup, the
   `colspan` repeat, the copy text of a block list. Soak: a zoom in and out and a copy per cycle.
   Gate: the defaults triplet and the preview triplet (`C:\traces\preview.toml`). Console checks: a
   PDF zoomed beside Edge at 200%, a Windows-1252 page, a HEIC from a phone, a long Markdown file's
   scrollbar, the copy pasted into Notepad.
5. **P11-M5, syntax colours.** Text and source in the preview coloured by language, settled first
   between a library with its own grammars (`syntect` with its Sublime grammars, which brings a regex
   engine and about 1 MB of grammars, pinned, licence noted; or `tree-sitter` with per-language
   crates) and a hand-written tokenizer for the dozen languages Jeremy opens (Rust, PowerShell,
   Python, JSON, TOML, YAML, Markdown is already drawn, HTML, CSS, JavaScript and TypeScript, C and
   C++, C#, shell, the `.slint` language, `.log` files with their levels): keywords, strings,
   comments, numbers and types in five colours from the Phase 10 theme's tokens (`src/ui/theme.slint`
   once P10-M1 is merged), per line, as `StyledText` runs on the existing code blocks, so the block
   list and the ListView stay as they are; a 2 MB file coloured in chunks as it is drawn, never whole
   at open; a setting `syntax_colours` (default true) in the dialog. The pin check decides the
   engine (the hand-written one is the fallback as HTML's scanner was); its size and the first-open
   cost on a 2 MB log are in the brief. `[preview] pane=N gen=G <path>: text, L lines ... <language>
   | plain` and `coloured K chunks in T ms`. Tests: each language's tokenizer on a fixture, the chunk
   boundary inside a block comment or a string. Soak: the cursor on the `.rs` and `.ps1` fixtures.
   Gate: the defaults triplet and the preview triplet. Console checks: a Rust file, a PowerShell
   script, a JSON file, a 2 MB log, a file with a 20,000-character line.
6. **P11-M6, password PDFs through hayro.** A small milestone (an afternoon), kept as its own so
   the record holds Jeremy's number. The Phase 8 close said hayro cannot open password PDFs; the
   builder check of this kickoff found that the pinned `hayro-syntax 0.8.0` has
   `Pdf::new_with_password` with user-password authentication for revisions 2 to 6 (RC4 and AES, its
   `crypto` module), and `preview.rs` already maps `LoadPdfError::Decryption(PasswordProtected)` to
   the "Password-protected PDF" message. So: when hayro reports a password, the column asks for it
   (a masked dialog of TwinFM's own on the preview worker, as the zip password's is since P8-L2;
   `pdfpassword <text>` for the driver, the answer to the next question; a scripted run without one
   is refused, never shown the dialog), opens with `new_with_password`, asks again while it is wrong
   (Cancel leaves the message), and renders as any PDF; the password is kept for the file while the
   document is open and never written anywhere. An owner-password-only file (opens with the empty
   password) is opened without asking. PDFium was offered as the alternative and declined (Jeremy,
   2026-10-22: "keep the library already in twinfm, let's not add anything we don't absolutely
   require"); no new dependency. `[preview] pane=N gen=G <path>: pdf password asked | asked again |
   from the script | right | wrong | cancelled`. Tests: a password PDF fixture written by the test
   with hayro's own writer path or a hand-made encrypted fixture (settled first: which revision the
   test can produce), the wrong password refused, the empty owner password accepted. Soak: the
   fixture with `pdfpassword` per cycle. Gate: the defaults triplet and the preview triplet. Console
   checks: Jeremy's own password PDFs beside Edge, a wrong password, Cancel.

**Added 2026-10-22, at the 0.1.0 public release** (lists stream, after P11-M3; it touches
`src/settings.rs`, `src/main.rs` and the settings dialog in `src/ui/app.slint`):

7. **P11-M7, logging as a setting.** Since 0.1.0 `twinfm.exe` is a windowed program
   (`#![windows_subsystem = "windows"]` in `src/main.rs`; Jeremy: "I want the 0.1.0 version to ship
   without the logging. Let's add a p11 task to enable logging via settings."): no console window
   opens, and the stderr lines (`[perf]`, `[enum]` and the rest, `CLAUDE.md` § Measuring) go nowhere
   unless the launcher redirects stderr, as `Start-Process -RedirectStandardError` in the gate and
   soak scripts does. P11-M7 lets a user turn the log on: a setting (`log`, default false, a row in
   the settings dialog) that writes the same lines to a file under `%APPDATA%\twinfm\` while it is on,
   applied on OK without a restart. Settled first: the file's name and rotation (one file per run, or
   one file cut at a size, and how many are kept), whether a redirected stderr keeps receiving the
   lines as now with the setting off (it must: the gate tools read stderr, and a scripted run neither
   reads nor writes the settings file), whether every `eprintln!` goes through one sink or the stderr
   handle is swapped for the file's (`SetStdHandle`) so no call site changes, and a dialog button that
   opens the log's folder. `[settings] dialog: log false -> true`; the file's first line names the
   version and the start time. Tests: the sink writes, rotates and closes; the setting round-trips.
   Gate: the defaults triplet (the setting off: no file, no cost) and one in-app run with it on.
   Console checks: the setting on and off without a restart, the file read while TwinFM runs.

Each milestone brings its driver steps and stderr lines (in `CLAUDE.md` § Measuring), tests for its
maths, and its soak cycle, added to `tools/soak-p5m3.ps1`. Console checks after each triplet,
unelevated. Every item in the two bundles is a commit of its own, and the record says per item: done,
checked and found right, or left with the reason; an item found to be already finished (the
inventory marked several "status unclear": the search leftovers, the Phase 4 and 5 small items, the
P2-M5 scale-change case) is recorded as such with where, not re-done.

**The gate is re-run after every milestone** under the protocol agreed on 2026-10-12, unchanged:
`tools/wpr-triplet.ps1` elevated at the console by Jeremy, accessibility on, Narrator off, every
other TwinFM window closed, the split-and-scroll scenario on `C:\traces\bigdir`, clean binaries built
with `--locked` into their own `--target-dir target-<name>` from detached `git worktree add`
checkouts. Three runs per binary, interleaved, compared by median; a gap counts only when every
reference run sits below every candidate run or the medians differ by more than one point (one-sided:
a cost), each run's `[perf] ... render[... p50=...]` read first, and a fixed-order pair's gap of about
a point checked with the order swapped (P7-M4). The reference for the first Phase 11 gate is the last
merged milestone's build at that time: `target-p9m4` (3374f2b) until P10-M1 merges, then
`target-p10m1`; after each merge the new milestone's gate build becomes the next reference and the
previous one is deleted once the new one has served. Tags continue from ab44 (ab41 is P10-M1's;
ab42 and ab43 were P10-M2's and P10-M3's, `phase-10-plan.md` § Phase 10 close).
Bar: main thread under 20% busy over the 10 s after the split, judged against the reference's spread
in the same session. The gate is one PowerShell wrapper file per milestone (`gate-<milestone>.ps1`:
the checks, the triplet, the analysis, the summary), written on `tools/gate-p9m4.ps1`'s pattern; it
refuses to start while cargo, a TwinFM window, Narrator, wpr or xperf runs or a Claude process spins a
core. One gate at a time on this machine, handed to Jeremy only through the coordinating session.

**Settings triplets stay part of the protocol** (Phase 4's rule): each candidate's stderr is checked
for `[settings] file <toml> (read)` and the line the setting implies before its triplet is read.

**The Phase 9 rules for two streams hold** (`phase-9-plan.md` § Phase 9 close, Lessons): every stream
reports to the coordinating session before any gate build, soak or hand-over and waits for a reply;
one soak at a time (the soaks share `%TEMP%\twinfm-m5`); nothing builds, soaks or opens a TwinFM
window from "gate running" to "machine free"; in-app numbers are taken on a quiet machine only; the
tests are run, the result read, then the commit made, as two steps, never chained; settle-first probes
run in TwinFM's own process; a fix after the console check needs no triplet when it stays in event
handlers, a stat on a watch record or the listing's keys (one in-app run recorded), and a triplet when
it touches the rows, the icon workers' per-row work or the compose path. With Phase 10 running in a
third session, its session reports to the same coordinator, and three streams share the one machine
by the same rules.

The architecture rules from Phase 1 hold: only the UI thread touches a Slint model and workers wake it
with `slint::invoke_from_event_loop`; every COM object lives on the thread that made it; no app-state
`RefCell` borrow is held across a modal loop; no `unwrap` on `Weak<App>`; nothing per frame or per
row without a trace first. Every crate added is pinned by exact version with its licence noted in
`Cargo.toml`, default features off, and nothing is added that is not required (Jeremy, 2026-10-22):
every milestone's Settled first asks first whether a crate already in the build does the job.

## Calls made in this kickoff, for Jeremy to check

Decisions Jeremy did not make and the lead settled, each reversible before its milestone starts:

- Content search is a case-insensitive substring, one result row per file with its first matching
  line, a 16 MB cap per file, binary files skipped by a NUL or a non-text ratio in the first 8 KB;
  no glob, no regular expression, no index in P11-M1.
- The search tab's results follow changes by a periodic stat of the visible rows (every 5 s while
  active), not a watch per result folder.
- The bundles' items are each a commit of their own, and an item may be left with a reason; the
  record, not the plan, says which.
- A history menu opens on a long press or right-click of Back and Forward, as a browser's does, and
  restored tabs keep at most 50 entries each way.
- The preview's "Copy text" (a block, or the whole document) stands in for text selection while Slint
  has none; selection itself waits for the fork or a Slint release.
- Zoom is 50% to 400% in steps with a horizontal scroll inside the column; the 8,192 px cap stays.
- HEIC through Windows' own decoders rather than a crate (none pure-Rust exists); absent the Store
  extension, a message naming it.
- Syntax colours use five colours from the Phase 10 theme and colour per chunk as drawn; the engine is
  chosen by the pin check, the hand-written tokenizer the fallback.
- The PDF password dialog is TwinFM's own, on the preview worker, as the zip password's is; the
  password is held for the open document only.
- The preview stream waits for P10-M1's merge before its first commit; the lists stream starts at
  once on the parts that need no screen file and lands every screen-file change after that merge, on
  the restyled files (the builder check).
- From the builder check, settled here: a content search never reads a cloud location (refused at a
  cloud root, skipped below a cloud folder met on the way); the matching line is appended to the name
  label, cut at 200 characters, no list variant touched; the history menu opens by Alt+Down and from
  the path bar's background menu, since there are no on-screen Back and Forward buttons; the preview's
  "Copy text" is a context menu on a block, the column taking no keyboard focus; restored history
  keeps search places as places that re-run the search; P11-M5 adds any theme token P10-M1 did not
  define.

## Open questions

Written 2026-10-22 at the kickoff.

- **Which items are already finished**: the inventory marked the search leftovers, the Phase 4 and 5
  small items and the P2-M5 scale-change case "status unclear"; each bundle's Settled first checks the
  code and records "already done (where)" or does the work.
- **Where the Google Drive thumbnail cost goes**: the cache, the provider or the in-cache-only call;
  P11-M2's measurement decides whether anything changes.
- **Whether the shell's progress sink names a replaced file's destination**, so undo can restore it:
  P11-M3's Settled first.
- **The syntax engine**: a library's size and first-open cost against a hand-written tokenizer's
  coverage; P11-M5's pin check.
- **Which encryption revisions hayro's password path handles on Jeremy's own files**, and how the
  test produces an encrypted fixture: P11-M6's Settled first.
- **Whether the HEIF Store extension is installed on this machine**: P11-M4's first check.
- **Deferred to Phase 12 (Jeremy, 2026-10-22)**: OneDrive's syncing and error glyphs (the property
  read per file on screen, or the placeholder compatibility mode); the Slint fork's whole-row snap
  patch and upstreaming the two accessibility patches.
- **Carried**: Google Drive's volume found by its name; a sync-root scratch folder for the soak; the
  binary Office formats; thumbnails inside archives.

## P11-M1: content search

The lists and file-system stream's first milestone, started 2026-10-22 from main at d1abbe3 (0.1.0,
after the Phase 10 close), branch `claude/p11-m1-content-search-3bb55d`, in a session of its own
started by the coordinating session. Corrections to the kickoff for this stream, from the
coordinating session's brief: Phase 10 has merged, so `pane.slint` and `app.slint` are free to
change on their restyled versions; the gate reference is `target-p10m3` (4f07cba), not
`target-p9m4`; tags start at ab44, allocated by the coordinator; the reference predates 0.1.0's
windowed exe and the candidate does not, which the gate record says; the Calls made line "a history
menu opens on a long press or right-click of Back and Forward" is superseded by the builder check's
line below it (Alt+Down and the path bar's background menu; P11-M3's).

Jeremy's decisions, 2026-10-22, from the choices put to him in this session (each the recommended
option):

- **The mode switch**: "Toggle button" (a button inside the box beside the search button, lit while
  on), not a dropdown beside it.
- **Typing in content mode**: "No, just hold the text": typing filters nothing while the toggle is
  on; name mode keeps the live filter.
- **Which files are read**: "Every file below": no name pattern with a content search in P11-M1
  (the `word in *.ext` syntax was offered and not taken).
- **Remembered**: "No, start in name mode": the toggle is the pane's for the session, never saved.

### Settled first (2026-10-22, before any code)

- **A second box or a mode of the box**: a mode, by P10-M2's settlement (one box, its button the
  hook); the form, a toggle, is Jeremy's choice above. Drawn as the look's own control (a 26 px
  ghost button with the Lucide `file-text` icon, added to `src/ui/icons/`, the accent and the muted
  face while on), not a toolkit widget (the Phase 10 lesson).
- **What reads the files**: no crate. `std::fs::File` and the decoding rule are enough; the
  preview's `markdown::decode` stops at 2 MB and is the preview stream's file, so
  `src/fs/content.rs` holds its own decode (the same rule: a UTF-16 byte order mark, else UTF-8 with
  invalid bytes replaced, the UTF-8 mark dropped) up to the 16 MB cap. Case-insensitivity is
  `char::to_lowercase` per character on both sides (`str::to_lowercase` turns a final sigma into
  `ς` in one and not the other; `ς` is read as `σ`); the text is folded once and, when no character
  changed its length (nearly always), the match's offset is the text's, so the line is found without
  a second pass; otherwise line by line.
- **Binary**: after the encoding rule, a NUL in the first 8 KB, or over 10% of its characters
  invalid UTF-8 or control characters other than tab, line breaks, form feed and escape. A UTF-16
  file with a byte order mark is text (its NULs are half its bytes); one without a mark reads as
  binary, as the preview reads it as UTF-8.
- **Cloud**: a content search rooted in a cloud location (`cloud::Locations::of`: a sync root or
  the Google Drive volume, the P9-M4 rule) is refused with a note, and a walk from elsewhere does not
  enter a folder under a sync root (`Locations::roots`, normalised, checked per folder entered, only
  in a content walk), counted as `cloud` in the done line. The Phase 9 lesson (OneDrive files list
  as plain files in TwinFM's process) is why the folder rule exists beside the attribute rule: the
  attributes alone would not keep a walk out of OneDrive. A file whose attributes say cloud-only
  (recall on data access, recall on open, offline; the preview's test) or whose reparse tag is a
  cloud one is never opened, counted as `cloud-only`. The Google Drive volume is a drive of its own,
  so no walk from another root meets it.
- **Archives and links**: a content search rooted inside an archive is refused with a note (the
  name walk's pass over cached levels has no file to read); an archive file met on the way is read
  as a file and is binary. Junctions are not entered (the name walk's rule, unchanged); file
  symbolic links, app aliases and other reparse points are not read.
- **The row**: `Entry` gains `found` (the matching line, empty everywhere else: one more
  `SharedString` per entry, an empty one costing a pointer), and `search_label` appends it after the
  `(sub\dir)` part, so the label path of search tabs carries it and folder tabs' rows are untouched;
  no list variant, no Type cell change. One row per file, the first match; the status bar's "N found
  in T s" is the count of files holding the text (the "count of matches" of the kickoff).
- **History**: `tabs::Place` gains `content`, so Back and Forward re-run a content search as one,
  and a content search and a name search for the same text are two places.

### What landed (2026-10-22)

- `src/fs/content.rs` (new): `sniff`, `decode`, `fold`, `first_match` and its cut (200 characters, a
  match deeper than fits shown from 40 characters before it behind `…`), `search_file`, `cloud_only`;
  four tests (binary, encodings, the first line and its cut, files read up to the cap).
- `fs::enumerate::walk` takes `Option<&Content>` (the folded text and the sync roots): in a content
  walk only plain files that hold the text are results, `cancel` is read before each file opened,
  folders under a sync root are not entered; `WalkStats` counts files read, over the cap, binary,
  cloud-only, unreadable and cloud folders; `spawn_search` logs the content done line. A test: a
  tree with a junction to its root, a UTF-16 file, a binary file, a file marked offline and a folder
  standing in for a sync root.
- `model::Entry::found`, `search_label(name, found)`; `tabs::Place::content`; `cloud::Locations::roots`
  and `cloud::normalise` public.
- `main.rs`: `Search::content`, `search_spec` (the matcher and the `Content` for a tab), the refusals
  in `start_search`, `content_mode` and `content_toggled`, the titles `Text: <text>` (tab, crumb,
  window).
- `pane.slint`: the toggle (`content-mode`, `content-toggled`), `edited` ignored while it is on, the
  placeholder; `app.slint` the two panes' properties and the callback.
- The driver's `contentmode` and `csearch`; CLAUDE.md § Measuring; the soak's cycle; the gate
  wrapper `tools/gate-p11m1.ps1`.

### Scripted checks (2026-10-22, the branch's own build, `target\release` of the worktree)

- A scratch tree (a UTF-8 file, a UTF-16 file in a subfolder, a binary file, a 6,000-character log
  line with the word in its middle): `csearch 0 needle` gave `done: 3 found in 4 files read, 1
  skipped (0 over the cap, 1 binary, 0 cloud-only, 0 unreadable), 3 folders walked, 0 cloud, 0 not
  listed, in 27 ms`; the toggle off applied the box's text as a filter, on cleared it; Return with
  the toggle off ran a name search, and Back re-ran the content search (`history back: search "note"
  in ... -> content "needle" in ...`).
- `csearch 1 needle` in `C:\Users\jtkli\OneDrive`: `refused (A search inside files does not read
  cloud folders: reading a file there downloads it)`, nothing walked.
- A screenshot (light, `winit-software`): the toggle lit, the placeholder "Text in files, Enter", the
  tab and crumb `Text: "needle"`, the rows `notes.txt  the Needle sits here`, `wide.txt  (sub)  wide
  needle line`, and the long line shown around its match.

**Soak** (`tools/soak-p5m3.ps1 -Minutes 10`, 2026-10-22, on the gate build `target-p11m1`, the
coordinator's go, the preview stream waiting): ran 10.1 min, exit 0, 0 panics; 264 jobs (0 failed, 0
cancelled, 0 refused), 0 waitops timeouts; every must-be-0 count 0 (drags dropped, toolbar presses
refused, menupicks refused, spring-loaded tabs, zip jobs not done, picture failures, Office
unreadable, alt text, waitpreview timeouts, worker panics); search box 4 Return searches, 4 button
searches, 4 blank Returns; **content search: done 4 (one per cycle), files read 806, found 4 (one
per cycle)**; working set 172 to 261 MB, peak 279 MB (above P10-M2's 136 to 226 on a longer cycle;
not read as a leak of this change).

### Gate

Run by Jeremy on 2026-10-22, finished 19:43 (tag ab44, `C:\traces\gate-p11m1.ps1`, the same file as
`tools/gate-p11m1.ps1`, 8 min): `target-p11m1` (79410ab, built `--locked` from a detached checkout)
against `target-p10m3` (4f07cba), the defaults, three runs each interleaved. Every run passed the
wrapper's checks, no panic. The reference predates 0.1.0's windowed exe: a console build against a
windowed one.

| Run | WPR | In-app | Render p50 (ms) |
|---|---|---|---|
| p10m3-1 / 2 / 3 | 18.3 / 18.3 / 18.6 | 18.1 / 19.4 / 18.9 | 2.18 / 2.22 / 2.23 |
| p11m1-1 / 2 / 3 | 18.6 / 18.6 / 18.6 | 20.9 / 18.0 / 17.2 | 2.21 / 2.20 / 2.20 |

Medians 18.3 (reference) / 18.6 (candidate): no gap (0.3 points, under the one-point rule; p10m3-3
equals the candidates, so not every reference run sits below every candidate run). Render p50 2.18
to 2.23 ms in every run: the session in its normal state throughout. Every candidate run under 20%.
**Passed.**

### Console checks (for Jeremy, unelevated, after the gate)

`target-p11m1\release\twinfm.exe`, from a normal console:

- **A word in `C:\Users\jtkli\projects`**: the toggle on, the word, Return; the results beside
  Windows Search or `findstr /s /i /m <word> *` in the same folder (the same files, give or take
  binaries and files over 16 MB).
- **A UTF-16 file found** (a `.reg` export or a PowerShell `Out-File` from 5.1).
- **A binary skipped**: a word you know is in an `.exe` or a `.dll`'s strings does not find it.
- **Escape mid-search** in a large folder: the walk stops, the results so far stay.
- **The matching line for a long line** (a minified `.js` or a one-line JSON): the word visible on
  the row.
- **The toggle**: typing with it on filters nothing; off again, the box's text filters; its tooltip.
- **A cloud folder**: the toggle on in OneDrive or on `G:\My Drive`, Return: the note, no search.

**Results** (2026-10-23, Jeremy at the console, `target-p11m1` started unelevated from this session
with stderr kept; his word: "All fine; I couldn't check #5"):

- **A word in `projects`**: "jeremy" searched inside files under `C:\Users\jtkli\projects`, then
  stopped with Escape after 919 folders (the build folders are large), so the comparison with
  `findstr` was run by the session on a smaller tree with the same build: `csearch 0 jeremy` in
  `C:\Users\jtkli\projects\claude` gave `done: 5 found in 15 files read, 0 skipped, 7 folders
  walked, in 131 ms`, and `findstr /s /i /m jeremy *` there the same 5 files. **Found right.**
- **Escape mid-search**: twice ("jeremy" after 919 folders, "apple" after 251), `cancelled ...
  (Escape)`, the results so far kept. **Found right.**
- **The toggle**: on and off four times, typing with it on filtering nothing, its tooltip. **Found
  right.**
- **A cloud folder**: `content "anything" in G:\My Drive: refused (...)`, the note, no tab. **Found
  right.** (OneDrive's refusal was checked in the scripted run.)
- **The long line**: not checked at the console (no file to hand); covered by the scripted check
  and its screenshot above (a 6,000-character line shown from 40 characters before the word).
- **A UTF-16 file** and **a binary skipped**: not separate console steps this time; covered by the
  tests and the scripted checks (the UTF-16 file found, the binary counted as skipped). No panic in
  the console run's stderr.

**Merged** 2026-10-23 as 8a23a4e (#98), on Jeremy's word ("merge 98,100,101 but don't rebuild the
daily"), by the coordinating session; `target\release` not rebuilt, by the same word.

## P11-M2: the cloud and search sweep

Started 2026-10-23 from main at 6615f9a (P11-M1 and the app icon merged), branch
`feat/p11-m2-cloud-search-sweep`, the coordinating session's go: "start P11-M2 ... settle each first.
Reference for its gate is still target-p10m3". Brought up to date with main after P11-M4 merged
(3cd18cd; conflicts in CLAUDE.md's test count and the soak's report lines, both kept). Six items,
each settled first, each a commit of its own.

### The items (2026-10-23)

1. **A cloud refresh keeps each row's mark: done** (66fee1e). Settled: a refresh's new entries in a
   Google Drive or sync-root listing carry `look:` keys, and the rows went back to their placeholders
   until each lookup returned. Now `finish_refresh` gives each such entry the icon its name had on
   screen when the old key is the new one's plain key with some mark (`icons::same_but_mark`: the
   same type, the same thumbnail path, time and size); the lookup still runs (`resolve_icons` asks for
   a kept row's `look:` key, but only while some row is kept, so no listing outside a cloud location
   pays a key check), and `relabel` keeps the icon when the mark came back the same or puts the
   placeholder back when it changed. Scripted on this machine: `G:\My Drive`, F5 twice, `marks
   carried: 41 of 126 across the refresh` (the rows that had been on screen); OneDrive `9 of 9`. A
   test covers the carry, a changed thumbnail, a new name, a changed mark and the empty case.
2. **The Google Drive thumbnail cost: measured, recorded, nothing changed.** The kickoff's 300 to
   570 ms did not reproduce. With thumbnails on (asked `WTS_INCACHEONLY` there since P6-M1), the
   `[icons]` line's average over two back-to-back runs on `G:\My Drive\Reagyn\100VTECH` (926
   pictures) was 50 to 58 ms a thumbnail, against 9 to 14 ms for the same kind of request on the local
   fixture `%TEMP%\twinfm-p3m5-thumbs` (both from the shell's cache, both runs alike, so no cold
   start). The extra 40 ms is the Google Drive volume answering the shell's item and property reads,
   not the cache and not an extraction (`INCACHEONLY` runs none). The kickoff's remedy (thumbnails on
   Google Drive asked only for rows on screen for 500 ms) was for a cost about ten times this; rows
   scrolled away are already dropped (`dropped` 0 in these runs), so a delay would only show the
   thumbnails later. Left as it is; the measurement is the record.
3. **Search results from several folders in one menu and one drag: done** (b1f8bfd). Settled by a
   probe in TwinFM's test process: the desktop's `IShellFolder` with each item's absolute PIDL as its
   "child" gives a full shell menu (`GetUIObjectOf`) and a data object whose `CF_HDROP` names both
   files (`SHCreateDataObject` with the desktop's empty PIDL), the way Explorer's own search results
   are built. `parse_items` takes that path (`parse_absolute`) when `common_parent` finds more than
   one folder; inside an archive the refusal stays. Scripted: two results in two folders selected,
   the Windows 11 menu (with the packaged apps' commands), "Show more options" (the classic menu) and
   a drag (cancelled) all for both items. Two tests (the menu, the data object).
4. **Results that follow changes: done** (0c461a5). As the kickoff called it: no watch per result
   folder; every 5 s (`RESTAT_EVERY`) and when a finished search tab comes to the front, its rows on
   screen are statted again on a worker thread and `land_restat` applies `model::restat_edits`
   (gone rows removed, changed ones updated in place under their row name, the content search's line
   kept) through `apply_edits`, the watch's path. A new match still needs F5. Scripted: a result
   deleted and another touched from outside, `results re-stated: 4 rows, 1 gone, 1 changed, in 4.0
   ms` on the next tick; the tab brought to the front states at once. A test for the edits.
5. **The cursor after a rename in a search tab: checked, found wrong, fixed** (c0ce2ab). The cursor
   was lost (row -1, nothing selected): the rename asked for the bare new name, and a search tab's
   rows are named by their path under the root. `search_row_name` gives the result's own folder back;
   scripted: `s2\hit-b.txt` renamed `hit-y.txt`, the cursor on row 4 (`s2\hit-y.txt`); `s1\hit-a.txt`
   renamed `hit-0.txt`, row 0. A test.
6. **Reset column widths: done** (f29aef1). A button under the Type column switch in the settings
   dialog, staged like every field (its label reads "Widths reset on OK" once pressed; Cancel drops
   it): on OK both panes' widths go back to 120, 90, 140 (`[cols] pane=N widths reset to 120, 90,
   140`, then the `(reset)` change lines). The dialog grew 40 px for the row (550 to 590); a
   screenshot against `target-p10m3`'s dialog showed the rest of its layout unchanged. No pure part
   beyond `cols::DEFAULT`; checked by the scripted run (the cancel, then OK) and the soak.

Also: CLAUDE.md § Measuring's P11-M2 paragraph, the tests list and the soak paragraph (which had
missed P11-M1's cycle too, added now); the soak's reset cycle; `tools/gate-p11m2.ps1`.

### Gate

Pending: the defaults and the thumbnails triplets (`C:\traces\thumbs.toml`), `target-p11m2` against
`target-p10m3`, after the coordinating session's go and tag. The thumbnails setting is there for
item 1's change in `resolve_icons`.

### Console checks (for Jeremy, unelevated, after the gate)

- **F5 on `G:\My Drive` and on a OneDrive folder**, beside `target-p10m3`: the icons and their marks
  stay put through the refresh (the old build flashes them to plain icons).
- **A search whose results lie in two folders**: select one from each, right-click (the menu acts on
  both: Copy path, Properties), then drag them to Explorer or the desktop (Copy: both arrive).
- **A result changed in another program**: with a search tab in front, delete one result in Explorer
  and edit another; within 5 s the first leaves the list and the second shows its new size and time.
- **Rename in a search tab** (F2): the cursor stays on the renamed result.
- **Settings > Reset column widths**: drag a column wider, open Settings, press the button, OK: both
  panes back to the default widths; Cancel instead leaves them.

## P11-M4: the preview gaps bundle

The preview stream's first milestone, opened 2026-10-22 on main at d1abbe3 (0.1.0, Phase 10 merged),
branch `feat/p11-m4-preview-gaps`. Two corrections to the kickoff, from the coordinating session's
brief: Phase 10 had merged, so the stream started at once on the restyled `preview.slint` and
`theme.slint`; the gate reference is `target-p10m3` (4f07cba), not `target-p9m4`, and it predates
0.1.0's windowed exe while the candidate does not (a console build against a windowed one,
`CLAUDE.md` § Measuring).

### Settled first (2026-10-22, before any code; probes in TwinFM's own test process)

- **The HEIF Store extension on this machine**: installed. `Get-AppxPackage` lists
  Microsoft.HEIFImageExtension 1.2.48.0, HEVCVideoExtension 2.5.33.0 and AV1VideoExtension 2.0.35.0.
  In TwinFM's process the Windows Imaging Component decoded a 256 x 256 AVIF (425 ms cold: COM, the
  factory and the packaged codec loading; 112 ms in the app's scripted run) and a HEIC written by WIC's
  own HEIF encoder (76 ms first, 8 ms after). No HEIC from a phone was on the machine: Jeremy's own is
  the console check. No crate: `windows-sys` gains its `Win32_System_Variant` feature for
  `PROPVARIANT` (a feature of a crate already in the build).
- **The entity table's binary size**: zero. pulldown-cmark, already in the build for Markdown, carries
  the whole HTML5 table (2,125 names: the 2,231 of the list less the 106 legacy forms without a
  semicolon, which pulldown-cmark does not take either) because CommonMark decodes every HTML5 entity;
  its table is private, so `markdown::entity` parses `&name;` and keeps the text when it changed. The
  alternative found, quick-xml's `escape-html` feature (also in the build), holds 1,448 names in one
  generated `match` and adds its compile time, so it was not taken.
- **SVG text through resvg's font database**: under 50 ms, so done. The font database in the build has
  no "load the system fonts" (fontdb's `fs` feature is off in Slint's tree); reading the whole Fonts
  folder by hand took 5.5 s cold and 0.16 s warm (381 faces, 397 MB), seven chosen files 1 to 3 ms (3 ms
  in the app's scripted run).
- **Charsets**: the kickoff's list held: Windows-1252, ISO-8859-1 (read as 1252, the HTML standard's
  mapping) and ISO-8859-15 by a table each; UTF-16 named in a meta is read as UTF-8, as the standard
  says (a page that is UTF-16 has its BOM, which `markdown::decode` already reads).
- **Which items were already finished**: none of the six.

### What landed

Five code commits (each built and tested before it was made: 277, 279, 280, 281 and 283 tests), then
the docs, soak and gate wrapper. Per item:

- **PDF zoom**: done (2d3a096). Ctrl+wheel over the pages, Ctrl+plus, minus and 0 with the pointer
  over the column; 13 steps from 50% to 400% (Edge's, without 33% and 500%); `preview_zoom` saved;
  pages laid out and rendered at the zoom times the column's width, the 8,192 px cap kept, centred
  when narrower, the column scrolled sideways when wider, the scroll kept at the same place in the
  document. Markdown, text and the rest have no zoom (the kickoff's item is the PDF's).
- **HTML's meta charset**: done (cc6683f). Windows-1252, ISO-8859-15 and UTF-8 by the first 1,024
  bytes; any other label read as UTF-8 with its name in the `[preview]` line.
- **The full entity table**: done (7652ad1), as settled above.
- **colspan**: done (cc6683f), as repeated cells: the cell, then `colspan - 1` empty ones, so the cells
  after it line up (at most 64; the kickoff's "repeated cells" read as empty ones following it, since a
  repeated text would read as data). It shares the charset's commit: their tests share one hunk.
- **HEIC, HEIF and AVIF**: done (19ffd0f), through WIC (`src/wic.rs`), oriented by
  `System.Photo.Orientation`; "No decoder for this format is installed" without the extension (told
  from a damaged file by asking WIC for the HEIF container's decoder).
- **The Markdown details**: done (41352d7). A picture inside a centred `p` or `div`, or `<center>`, is
  centred (text stays left, Jeremy's 2026-10-20 call on text); table lines are 1 px where cells meet
  (each cell draws its right and bottom line, the table its left and top); an SVG's text is drawn with
  seven Windows fonts. The scrollbar's drift on a long Markdown file is the console check's (the
  kickoff lists it there).
- **Copy text**: done (2d3a096), as the kickoff's call: a right-click on a block offers "Copy this
  block" (a code block or table whole, whatever chunk was clicked) and "Copy all"; plain text, CRLF
  line ends, a table's cells tab-separated so it pastes into Excel as cells. The nearest thing to
  selection at the pinned Slint rev; selection itself waits for the fork or a Slint release. It shares
  the zoom's commit: both use the column's callbacks, the driver and app.slint's property block.
  A PDF has no "Copy text" (hayro renders pages; extracting their text is not in this milestone).

### Calls made in this milestone, for Jeremy to check

- The zoom steps are Edge's from 50% to 400%; Ctrl+0 is back to 100%.
- Ctrl+plus and Ctrl+minus zoom only with the pointer over the column, so they never reach the lists
  by surprise; Ctrl+wheel needs the pointer there by its nature.
- The full entity table comes from the Markdown library's own copy rather than a table of TwinFM's.
- SVG text uses seven fonts (Segoe UI, Arial, Times New Roman and Consolas, with bold), not every font
  on the machine: the whole folder costs up to 5.5 s at the first SVG.
- The `<p align=center>` centring is for pictures only, as the kickoff wrote it.

### Scripted checks (2026-10-22, the worktree's own build, not a gate build)

A scripted run over a scratch folder (`%TEMP%\twinfm-p11m4-check`: a four-page PDF, an AVIF, a
Windows-1252 page with entities and a colspan, a Markdown file with a centred logo and a table, the PNG,
an SVG with text in `Helvetica, sans-serif`): the zoom went 110, 125, ..., 300 and down to 67, every
step rendering its pages at the new width (337 px at 100% to 1,010 px at 300%), and back to 100 with
Ctrl+0's path; `previewzoom` on the AVIF logged "no PDF shown"; the AVIF decoded in 112 ms; the page
read as `windows-1252` (Café, the curly quotes and the euro sign right in the screenshot); `previewcopy
all` gave 45 characters and the clipboard held the page's text, `previewcopy 3` on the Markdown file the
table as `x<TAB>y`, `1<TAB>2`, `3<TAB>4`; the SVG fonts loaded in 3 ms and "SVG text" was drawn.
Screenshots (software renderer): the logo centred, the table's lines 1 px, the PDF at 200% wider than
the column. One thing for the console check: in that capture the rarest symbols of the entity table
(the contour integral, the Fraktur A) drew as small or missing glyphs; the characters are right (the
test), so it is the renderer's font fallback, to be read with femtovg at the console.

### Soak (2026-10-22, `target-p11m4`, before the gates by the coordinating session's order)

`tools/soak-p5m3.ps1 -Minutes 10`: 10.1 min, exit 0, no panic, 270 jobs done (none failed or
cancelled), every must-be-0 count at 0. The preview: PDFs opened 8, pages rendered 76, zooms 16 (four
per cycle), copies 8 (two per cycle), copy failures 0, picture failures 0, waitpreview timeouts 0,
worker panics 0. Working set 170.8 MB at 5 s, 245.4 MB at 600 s (max 276.6; private 155.6 to 225.2).

CI's first run failed one test: on the runner, with no HEIF extension, the test's fallback checked a
file it had never written (file not found) where it meant one no decoder takes. Fixed in the test only
(1 file, `#[cfg(test)]`), after the gate build: the gate binary at 2d3a096 is the program the PR ships.

### Gate

ab45, passed (2026-10-22 20:07, 17 min, Jeremy at the console after ab44): `tools/gate-p11m4.ps1`,
`target-p11m4` (2d3a096) against `target-p10m3` (4f07cba), the defaults and the preview column
(`C:\traces\preview.toml`). The reference is a console build (before 0.1.0) and the candidate a
windowed one (`CLAUDE.md` § Measuring).

| Setting | p10m3 (WPR) | p11m4 (WPR) | Medians | Render p50 |
|---|---|---|---|---|
| defaults | 18.0, 18.7, 19.1 | 18.4, 18.7, 19.1 | 18.7 / 18.7 | 2.15 to 2.40 ms (p10m3-3's 2.40, a reference run) |
| preview | 17.8, 18.0, 17.8 | 17.6, 17.9, 17.9 | 17.8 / 17.9 | 1.97 to 2.01 ms |

No gap that counts in either; every candidate run under 20%; one session state throughout.

### Console checks (Jeremy, 2026-10-23, unelevated, `target-p11m4` on `C:\traces\p11m4-console`)

Jeremy: "All checks look good." Every item below passed: the zoom by Ctrl+wheel and the keys with the
sideways scroll, the AVIF, the Windows-1252 page, the centred logo and thin table lines, Copy this
block into Excel and Copy all into Notepad, SVG text, the long Markdown file's scrollbar. One finding:
of the rare entities, ∉ draws small and ∳, 𝔄, ⫅̸ draw as empty boxes in TwinFM's window (his
screenshot), since the UI's font has no glyphs for them and Slint's fallback found none. The
characters are right: "Copy this block" of that line pasted into Word showed them correctly (Jeremy:
"Pasting into word displays the characters correctly"). Deferred: font fallback for symbols outside
the UI font, a Slint question, not this milestone's; the common entities all draw.

The list as handed to Jeremy:

- A PDF zoomed with Ctrl+wheel and Ctrl+plus beside Edge at 200%; the sideways scroll at 400%.
- A Windows-1252 page (any old saved web page or a hand-made one) beside Edge.
- A HEIC from a phone (upright?) and an AVIF.
- A long Markdown file's scrollbar (the drift recorded since Phase 8).
- "Copy this block" and "Copy all" pasted into Notepad, a table pasted into Excel.
- Entity symbols (`&notin;`, `&CounterClockwiseContourIntegral;`) as femtovg draws them.
