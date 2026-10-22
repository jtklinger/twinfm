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
previous one is deleted once the new one has served. Tags continue from ab42 (ab41 is P10-M1's).
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
