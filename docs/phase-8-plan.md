# Phase 8: TwinFM previews documents beside the lists

Written 2026-10-20, when Phase 7 closed with P7-M1 to P7-M4 merged (`phase-7-plan.md` § Phase 7
close). Jeremy now uses TwinFM daily from `target\release` while development continues. Phase 8's
first and major item, chosen by Jeremy on 2026-10-20: **a preview pane**, so a document can be read
beside the lists without opening it. PDF and Markdown first; other types later. Other Phase 8 work
(the Phase 7 leftovers a parallel session may carry) records under its own section here.

What Jeremy decided (2026-10-20), from the choices put to him:

- **TwinFM draws and manages the renderers itself**: a PDF rendering library and a Markdown renderer
  inside TwinFM, not Windows' shell preview handlers (`IPreviewHandler`). Decided before the kickoff.
- **PDF through hayro** (pure Rust, MIT OR Apache-2.0): nothing beside the exe to ship. Offered
  PDFium (Chrome's engine, `pdfium.dll` beside the exe) as the recommendation and Windows'
  `Windows.Data.Pdf` as a fact sheet, he chose hayro, knowing its stated gaps: no password-protected
  PDFs, no non-embedded CID fonts (some CJK files), no knockout groups, blending or colour-key
  masks, speed not its focus, "experimental, work-in-progress" by its own description.
- **Markdown in TwinFM's own block layout**: pulldown-cmark (already in the build through Slint)
  parses the file into blocks (headings, paragraphs, lists, code blocks, quotes, tables, rules), each
  drawn as Slint text with its own size and weight; bold, italic, inline code and links inside a
  block go through Slint's `StyledText`. Slint's `StyledText::from_markdown` alone was ruled out: it
  refuses a whole file with any heading, code block, table or quote.
- **A column at the window's right edge**, with its own drag handle as the split has; the panes
  narrow to make room. It previews the active pane's cursor file, split or not (Explorer's layout).
- **Alt+P, a toolbar button and a setting**: Alt+P as in Explorer, a Preview button on the toolbar,
  remembered across runs and in the settings dialog. Off by default.
- **The order: P8-M1 the pane with PDF, P8-M2 Markdown** (offered Markdown first as the lower-risk
  start, he chose PDF first).
- **The cursor file, after a 150 ms settle**: the active pane's cursor row, rendered on a worker once
  the cursor has rested 150 ms, so holding Down through 40,000 rows renders nothing; a newer cursor
  cancels the older render. Folders and other types show the name and "No preview". A PDF shows every
  page in one scroll, each page rendered when it comes into view, at the pane's width.
- **Caps with a message**: Markdown over 2 MB shows its first 2 MB with a note. PDF has no size cap
  (pages load as needed); a page over 5 s, a damaged file or a password PDF shows a plain message
  instead. Files inside archives and cloud-only files (not downloaded) are not previewed, so a
  preview never starts a download.

The starting point is main at 32b483e (the Phase 7 close). What the milestones rest on today: the
window is the toolbar across the top (`toolbar = "top"`, the default), the sidebar, and the `panes`
rectangle in `src/ui/app.slint` holding the left pane, the right pane when `split`, and the splitter
(`split-ratio`, 0.15 to 0.85); `dnd.rs` `Layout` mirrors that geometry for hit testing. Window keys
are taken by the `outer` FocusScope in `app.slint` (F3 split, F9 sidebar; Alt+P is free). Each pane's
cursor row is `current-row` (`left-current-row`, `right-current-row`), and `update_status` in
`main.rs` runs when the selection or cursor changes. Images reach Slint as
`SharedPixelBuffer<Rgba8Pixel>` built on a worker and turned into an `Image` on the UI thread inside
`invoke_from_event_loop` (the icon strips' path). Slint at the pinned rev has `Flickable`, `Image`
and a stable `StyledText` element (`slint::StyledText`); femtovg checks no texture size, so the GPU's
limit (likely 16,384 px on a side) applies to any one image.

Two milestones, in the order Jeremy gave on 2026-10-20:

1. **P8-M1, the preview pane with PDF.** The column (`src/ui/preview.slint`, a component the window
   places right of `panes` with a 6 px handle, its width a ratio saved as `preview_width`, 0.2 to
   0.6 of the space right of the sidebar, default 0.35), Alt+P and a Preview toolbar button in both
   placements toggling it (`preview`, saved, in the settings dialog), and the pipeline in
   `src/preview.rs`: the active pane's cursor file, a 150 ms settle timer on the UI thread (restarted
   by every cursor move or active-pane change, nothing per row), then a request to one dedicated
   preview worker thread that owns the open document. The worker classifies the file (extension
   `.pdf`; inside an archive, cloud-only by its attributes (`FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`,
   `RECALL_ON_OPEN`, `OFFLINE`), a folder, or another type: a message, no read), opens it with
   hayro, answers the page count and each page's size in points, and renders pages on request to RGBA
   at the column's width in physical pixels; the UI thread lays the pages out from their sizes before
   any is drawn (placeholders of the right height) and asks for the pages in view plus one either
   side. A newer request supersedes the older at every step (a generation number; a render whose
   generation is stale is dropped before it is sent back). Page images kept: those in view and one
   either side, the rest dropped (memory bounded by the window, not the document). A page whose
   render takes over 5 s, a damaged file, or a password PDF shows its message in place. A width change
   re-renders the visible pages after the drag settles. The rule for every page image: one image per
   page, never over 8,192 px on a side (a tall page is scaled down to it and logged). Driver steps,
   stderr lines (`[preview]`), the soak addition and the fixtures are named in the brief and recorded
   in `CLAUDE.md` § Measuring. Gate: the defaults triplet (preview off: expected flat, it checks the
   cursor hook costs nothing) and a preview triplet (`C:\traces\preview.toml`, read-only,
   `preview = true`: the column shown while the lists scroll, the scenario's cursor on no PDF). Console
   checks: PDFs of several kinds beside Edge (text, scanned, forms, CJK, a password one, a damaged
   one, a 500-page one), holding Down through a folder of PDFs, the column's width dragged, 100% and
   150% scaling, the layouts set up unequal.
2. **P8-M2, Markdown.** pulldown-cmark on the preview worker into a block list (headings 1 to 6,
   paragraphs, ordered and unordered lists with nesting, task lists, code blocks in a monospace font
   on a tinted background, quotes, tables, rules), each block's inline spans as `StyledText`; the UI
   thread shows the blocks in the column's scroll. Links open in the default browser (`ShellExecute`
   on `http`, `https` and `mailto` only) and a link to a local file opens it as a double-click does;
   local images (relative to the file) are decoded on the worker and shown at the column's width,
   remote images as their alt text. Files over 2 MB show the first 2 MB with a note. The block list's
   maths (spans, list nesting, the 2 MB cut at a character boundary) is tested. Gate: the defaults
   triplet and the preview triplet. Console checks: READMEs from real repos beside a browser's
   render, a 2 MB file, a file with broken syntax.

Each milestone brings its driver steps and stderr lines (in `CLAUDE.md` § Measuring), tests for its
maths (P8-M1: the settle, the classification, the page layout from sizes, the pages in view, the
8,192 px rule, the column's width bounds; P8-M2: the block list) and its soak cycle, added to
`tools/soak-p5m3.ps1`. Console checks after each triplet, unelevated.

**The gate is re-run after every milestone** under the protocol agreed on 2026-10-12, unchanged:
`tools/wpr-triplet.ps1` elevated at the console by Jeremy, accessibility on, Narrator off, every
other TwinFM window closed, the split-and-scroll scenario on `C:\traces\bigdir`, clean binaries built
with `--locked` into their own `--target-dir target-<name>` from detached `git worktree add`
checkouts. Three runs per binary, interleaved, compared by median; a gap counts only when every
reference run sits below every candidate run or the medians differ by more than one point (one-sided:
a cost), each run's `[perf] ... render[... p50=...]` read first, and a fixed-order pair's gap of about
a point checked with the order swapped (P7-M4). The reference for P8-M1 is `target-p7m4` (67160ac);
after each merge the new milestone's gate build becomes the next reference and the previous one is
deleted. Tags continue from ab32. Bar: main thread under 20% busy over the 10 s after the split,
judged against the reference's spread in the same session. Nothing else runs on the machine during a
gate (the wrapper refuses to start while `cargo`, `rustc` or a test binary runs). One gate at a time
on this machine; Jeremy is asked before a wrapper is handed over.

**Settings triplets stay part of the protocol** (Phase 4's rule): each candidate's stderr is checked
for `[settings] file <toml> (read)` and the line the setting implies (`[preview] shown at ...`)
before its triplet is read.

The architecture rules from Phase 1 hold: only the UI thread touches a Slint model and workers wake
it with `slint::invoke_from_event_loop`; every COM object lives on the thread that made it; no
app-state `RefCell` borrow is held across a modal loop; no `unwrap` on `Weak<App>`; nothing per frame
or per row without a trace first. For the preview: the document is opened, parsed and rendered only
on the preview worker; the UI thread receives page sizes, pixel buffers and (P8-M2) block lists, and
builds `Image`s and models from them; the cursor hook is one timer restart, never a read.

## Calls made in this kickoff, for Jeremy to check

Decisions Jeremy did not make and the lead settled, each reversible before its milestone starts:

- hayro is pinned by exact version (`=0.8.0` or the release current when P8-M1 starts) as every
  dependency added since P7-M3 is, with its licence noted beside it in `Cargo.toml`. Its default
  fonts (about 240 KB of substitutes for the 14 standard PDF fonts) and CMaps stay in.
- One preview worker thread for every type, owning at most one open document; the UI thread never
  opens a file for the preview.
- The column is 0.35 of the space right of the sidebar by default, draggable between 0.2 and 0.6, and
  previews whichever pane is active; the split's own ratio applies to what is left.
- The toolbar's Preview button goes at the end of the bar in both placements (across the window and
  in each pane); with `toolbar = "none"` Alt+P and the dialog remain.
- A PDF page is rendered at the column's width in physical pixels (the scale factor applied), capped
  at 8,192 px on a side; zoom inside the preview is not in P8-M1.
- The settle timer is 150 ms after the last cursor move, and the active pane switching counts as a
  move.
- Plain text, images and other types are not previewed in Phase 8's first two milestones ("other
  types later"); they show "No preview".

## P8-M1: the preview pane with PDF

Branch `feat/p8-m1-preview-pdf`, from be759e0 (the kickoff).

### What landed

- **The column** (`src/ui/preview.slint` `Preview`, placed by app.slint): right of the `panes`
  rectangle and a 6 px handle, at `preview_width` of the space right of the sidebar (default 0.35,
  0.2 to 0.6, `preview::clamp_ratio`); the panes narrow to the rest less 3 px, and the split's ratio
  applies to what is left. A header with the file's name, then a `ScrollView` of pages (each a white
  rectangle placed at the top and height Rust laid out from the page sizes, its image fitted inside
  once rendered) or a message. The element is always there and hidden with the setting, so its
  scroll functions are always callable. `dnd::Layout` has a `preview` field (`dnd::preview_w`: the
  share times the space, plus 3 px), and `pane_x` ends the panes there; everything that mirrors the
  geometry (hit testing, the band, the menu point, the driver) goes through `pane_x`, so nothing
  else changed.
- **The toggle**: Alt+P in the `outer` scope (free: no list scope takes Alt with a letter), a
  Preview button at the end of the toolbar in both placements (Segoe Fluent Icons E90D, "Preview
  pane (Alt+P)", always enabled, drawn as on while the column shows; `toolbar_clicked` sends
  `preview` to the toggle, not to a pane command), the setting `preview` (saved, a "Preview" row in
  the settings dialog; the dialog is 36 px taller for it). Scripted runs start from the defaults as
  before.
- **The pipeline** (`src/preview.rs`): app.slint's `changed` handlers on both cursor rows and the
  active pane call `preview-cursor` only while the column shows, and `show_tab` calls it too (a tab
  switch can keep the row number); each restarts one single-shot timer. 150 ms after the last move
  the cursor entry (path, attributes, folder or not, from the listing; no file system call) goes to
  the `preview` thread as an `Open` with a new generation. The worker classifies, opens a PDF with
  hayro, answers the page sizes, and renders the pages asked for at the column's width in physical
  pixels, each capped at 8,192 px on a side. The UI lays the pages out, asks for those in view and
  one either side (the in-view ones first), keeps only those images, and on a width change re-lays
  out and re-renders after 200 ms of rest. A shared `Keep` (generation, kept range, width) is read by
  the worker before each page: a page of an older generation is dropped, one no longer kept or at an
  old width is answered `Skipped` (so the UI may ask again). A newer `Open` drops every request
  queued before it unseen. Answers come back through `invoke_from_event_loop`; the UI thread drops
  one of an older generation and builds the `Image` from the `SharedPixelBuffer`. Opening and each
  page render run under `catch_unwind`.
- **Messages**: "No preview" (a folder, a type other than PDF), "Not previewed inside an archive",
  "Not previewed: the file is in the cloud and not downloaded", "Password-protected PDF: not
  previewed" (hayro's `DecryptionError::PasswordProtected`), "This PDF could not be read" (an I/O
  error, any other load error, no pages, a panic), and "Page N took too long" in a page's place.
- **hayro `=0.8.0`** (MIT OR Apache-2.0), default features (substitute fonts, CMaps, SIMD). It
  brings vello_cpu, kurbo, pic-scale and its own decoders (JPEG 2000, JBIG2, CCITT); Cargo.lock
  grows by about 270 lines.
- **Driver**: `preview toggle` (key name `AltP`), `preview width <ratio>`, `previewscroll <px>`,
  `waitpreview <ms>`, `settings set preview true|false`, `toolbar <pane> preview`. `[preview]`
  lines as listed in `CLAUDE.md` § Measuring.
- **Tests**: 224 to 231: the settle, the classification, the page layout, the pages kept, the
  8,192 px rule, the width bounds, and one worker test that writes a hand-made two-page PDF, a
  damaged one and a text file into `%TEMP%\twinfm-p8m1-test-<pid>\`, renders page 2 at 100 px
  (red rectangle in the middle, white margin), checks a skipped page and a superseded queue, and
  removes the folder. The settings tests carry `preview` and `preview_width`, and the hit test the
  column.
- **Soak**: `tools/soak-p5m3.ps1` adds the preview cycle (setup in `%TEMP%\twinfm-m5\preview`, the
  PDFs written by the script itself) and its report line.

### Decisions

- **Type before place in the classification.** A non-PDF shows "No preview" wherever it lies; only a
  `.pdf` is asked whether it lies inside an archive (`fs::archive::within`, which may cost a file
  system query) or is cloud-only. The archive and cloud messages appear for PDFs only, and no
  non-PDF costs a query.
- **The cloud test uses the listing's attributes** (the entry's `attrs` from the enumeration), sent
  with the request, so the worker makes no call that could start a download.
- **The column is a `ScrollView`, not a bare `Flickable`**: the same element with scrollbars. Its
  `content-y` is what `previewscroll` sets.
- **The scroll handler compares, nothing more**: every scroll movement calls `preview::scrolled`,
  which finds the pages in view by a binary search over the slots and returns when the kept range
  is unchanged. No timer throttle was needed for that.
- **One page image per page at the column's width**, physical px = logical width times the scale
  factor; the page is drawn inside a white rectangle with `image-fit: contain`, so during a column
  drag the old image letterboxes until the re-render lands.
- **Skipped answers** (a page no longer kept when the worker reaches it) go back to the UI rather
  than being dropped silently, so a page scrolled away and back before the worker got to it is asked
  for again.
- **`shown at W px` is logged once the column's width has rested 200 ms**: at start the window takes
  its size in steps (the first report was 73 px), so the line reads the settled width. W is the
  column's width (the page width plus its 8 px margins).
- **Pages are counted from 1 in the lines and the message**, as a reader counts them.
- **The Preview button's glyph** is E90D (DockRight in Segoe Fluent Icons: a window with a pane at
  its right), at the end of the bar after a separator; "at the end" read as after Delete, not pushed
  to the bar's right edge.
- **The worker test does not cover a password PDF**: writing an encrypted file by hand needs the
  RC4 or AES key derivation; the path is one match arm on hayro's error, and the console check's
  password PDF is the check.

### Scripted checks

All on this machine (RDP session), `target\release` of the branch.

A run over `%TEMP%\twinfm-p8m1-check\` (written by a scratch script: `a-eight-pages.pdf`, eight
pages of Helvetica text, page 4 A4 landscape; `b-tall.pdf`, one 200 x 6000 pt page; `c-damaged.pdf`,
46 bytes; `d-notes.txt`): Alt+P, click row 0, `waitpreview`, `previewscroll 2000`, `preview width
0.5`, Down through the four files, Up three times without waiting then Down twice 20 ms apart, the
dialog's Preview off, the toolbar's button on, Alt+P off. Every `waitpreview` settled (no timeout):

```
[preview] shown at 353 px (ratio 0.35)
[preview] pane=0 gen=2 ...\a-eight-pages.pdf: pdf, 8 pages, opened in 0 ms
[preview] gen=2 pages in view 1..2, kept 0 image(s)
[preview] gen=2 page 1 rendered 337x436 px in 0 ms
[preview] gen=2 page 2 rendered 337x436 px in 0 ms
[preview] gen=2 page 3 rendered 337x436 px in 0 ms
[preview] gen=2 pages in view 5..7, kept 0 image(s)
[preview] gen=2 page 5 rendered 337x436 px in 0 ms
[preview] gen=2 page 6 rendered 337x436 px in 0 ms
[preview] gen=2 page 7 rendered 337x436 px in 0 ms
[preview] gen=2 page 4 rendered 337x238 px in 0 ms
[preview] gen=2 page 8 rendered 337x436 px in 0 ms
[preview] width 489 px: re-render after settle
[preview] gen=2 pages in view 4..5, kept 3 image(s)
[preview] gen=2 page 4 rendered 489x346 px in 0 ms
[preview] gen=2 page 5 rendered 489x633 px in 0 ms
[preview] gen=2 page 3 rendered 489x633 px in 0 ms
[preview] gen=2 page 6 rendered 489x633 px in 0 ms
[preview] pane=0 gen=3 ...\b-tall.pdf: pdf, 1 pages, opened in 0 ms
[preview] gen=3 pages in view 1..1, kept 0 image(s)
[preview] gen=3 page 1 capped at 8192 px (was 489x14670)
[preview] gen=3 page 1 rendered 273x8192 px in 3 ms
[preview] pane=0 gen=4 ...\c-damaged.pdf: damaged: Invalid
[preview] pane=0 gen=5 ...\d-notes.txt: not a pdf
[preview] pane=0 gen=6 ...\a-eight-pages.pdf: pdf, 8 pages, opened in 0 ms
[preview] gen=6 page 1 rendered 489x633 px in 0 ms   (pages 1 to 3)
[preview] pane=0 gen=7 ...\c-damaged.pdf: damaged: Invalid
[preview] hidden
[toolbar] pane=0 preview -> toggle
[preview] shown at 505 px (ratio 0.50)
[preview] pane=0 gen=10 ...\c-damaged.pdf: damaged: Invalid
[preview] hidden
```

The three Up presses without a wait opened one generation (gen 6, the file the cursor rested on),
and the two Down presses 20 ms apart one more (gen 7): the settle let nothing through in between.
No `superseded` line showed here, since these pages render in under 5 ms; the worker test checks
the drop. A screenshot with the software renderer (`PrintWindow`) shows the column at 0.35 with pages
1 and 2 of `a-eight-pages.pdf` rendered beside the list.

The gate scenario on `C:\traces\bigdir` (in-app, indicative only: single runs, an RDP session whose
cadence changed between the pairs, 31 ms then 47 ms frames), the default and `preview = true` (a
read-only scratch toml; its stderr has `[settings] file ... (read)` and `[preview] shown at 353 px
(ratio 0.35)`, and no file line, since the cursor sits on no row): `[busy] 10 s after split` 7.5%
off then 11.4% on; in the swapped order 8.0% on then 10.3% off; a later single run with the column
9.8%. The render p50 sat between 2.2 and 3.4 ms in all four. Read as noise and run order; the
console triplet decides.

A six-minute run of `tools/soak-p5m3.ps1` (three cycles completed): exit code 0, no panicked line,
every other count as before; preview: PDFs opened 6, pages rendered 15, damaged 3, not a PDF 3,
superseded 0, waitpreview timeouts 0, worker panics 0.

### Gate

Run by Jeremy on 2026-10-20 at 14:01 (tag ab32, `C:\traces\gate-p8m1.ps1`, 16 min): `target-p8m1`
(47fc959, built `--locked` from a detached checkout) against `target-p7m4` (67160ac), three rounds of
reference, candidate, reference with `C:\traces\preview.toml`, candidate with it. Every run's stderr
passed the wrapper's checks (`[settings] file C:\traces\preview.toml (read)` in the preview runs,
`[preview] shown at 353 px (ratio 0.35)` in the candidate's).

| Run | WPR | In-app | Render p50 (ms) |
|---|---|---|---|
| p7m4-1 / 2 / 3 | 18.7 / 18.3 / 18.5 | 16.4 / 18.9 / 19.2 | 2.19 / 2.20 / 2.22 |
| p8m1-1 / 2 / 3 | 18.3 / 19.2 / 18.8 | 18.0 / 19.2 / 18.4 | 2.17 / 2.31 / 2.19 |
| p7m4-preview-1 / 2 / 3 | 18.6 / 18.7 / 18.9 | 18.7 / 19.7 / 18.6 | 2.22 / 2.18 / 2.26 |
| p8m1-preview-1 / 2 / 3 | 18.4 / 18.4 / 18.5 | 21.1 / 20.8 / 19.5 | 1.99 / 2.10 / 2.04 |

One session state throughout (render p50 1.99 to 2.31 ms). Defaults: medians 18.5 (reference) / 18.8
(candidate), no gap that counts. Preview: 18.7 / 18.4, the candidate below. Every candidate run under
20% on WPR. Passed; P8-M1 adds no cost with the column hidden or shown while the lists scroll. (The
reference ignores the `preview` key, so its preview runs are its defaults; the pair measures the
column's cost, the narrower panes included.)

### After the console check

Jeremy, 2026-10-20: in the preview column, the wheel with Shift held did not scroll; it did again once
Shift was released. Cause: Slint's Flickable turns the wheel sideways while Shift is held
(`FlickableData::scroll_delta`), and the column has no sideways room, so the event went unused. Fix,
in the column only (not the lists, not the fork): a TouchArea behind the pages in `preview.slint`
takes the wheel the Flickable passes on and, with Shift held and no sideways room, moves the scroll by
its vertical delta, clamped; anything else it rejects, so the plain wheel, the scrollbar and
`scrolled` run as before. New driver step `wheelat <x> <y> <dy> [shift]`; over a 20-page PDF, Shift
and -600 px scrolled 0 -> 600 -> 1200 with `pages in view 2..3`, `3..4` and pages 4 and 5 rendered,
the plain wheel 1200 -> 1800, Shift and +900 back to 900, and the two ends clamped (9534, 0). No new
triplet: the gate scenario never turns the wheel with Shift held, and the handler does nothing per
frame.

## Open questions

Written 2026-10-20 at the kickoff.

- **hayro on real PDFs**: how many of Jeremy's own PDFs it renders right, and how fast; the P8-M1
  console check is the measure. If it falls short, PDFium is the recorded alternative (Jeremy's call).
- **Carried from Phase 7**: Google Drive's volume found by its name; a sync-root scratch folder for
  the soak; the Slint whole-row snap patch and upstreaming; the dead share console check (P5-M1).

## Phase 7 leftovers

Written 2026-10-20 by the leftovers session, which runs beside the preview pane's (this section is
its own; the sections above are the preview's). The items are the Phase 7 close's Deferred list
(`phase-7-plan.md` § Phase 7 close) less what stays out: writing into 7z, rar, tar and the other
`ArchiveFolder` formats (read-only, Jeremy 2026-10-19) and Explorer's own "Compress to" (not
reachable for a host). The milestones are named **P8-L1 to P8-L3** (L for leftovers, so the preview's
P8-M numbers stay its own; a name proposed by the session, open to Jeremy's change).

### Settled first (2026-10-20, before any code)

Checked against the code:

- **Writing into a password-protected zip.** The `zip` crate's raw copy (`raw_copy_file_rename`,
  9.0.3) does not copy an entry's records: it starts a new entry from the old one's options and
  copies only the compressed bytes, so the general-purpose flags (encrypted, bit 0; data descriptor,
  bit 3) and fields it does not model (Info-ZIP's 0x5455 timestamp) are lost. A copy of the local
  record and the central directory record as bytes (names and offsets patched) keeps everything, and
  the same copier serves a copy from one zip into another and within one zip, with no
  recompression. The crate writes ZipCrypto (`with_deprecated_encryption`) and, with its
  `aes-crypto` feature, AES; it reads with a password (`by_index_decrypt`), which can check one.
- **An archive inside an archive** is a file inside the outer one today (`fs::archive::split` stops at
  the first archive component that is a file on disk); Return opens it through the shell, which
  extracts it first.
- **Between archives**: `fileops::archive::classify` refuses any job whose sources lie inside an
  archive and whose destination is a zip (`BETWEEN_ARCHIVES`), the same zip included.
- **Open and Open with for several items**: `winmenu::items` shows Open for one item and Open with for
  one file; Return opens the cursor row. The verb worker already runs a canonical verb for several
  items; Open with's apps are invoked with the items' data object (`explorercmd::run`).
- **The grid's PageDown** moves a viewport of grid rows; `model::page_target` is the details list's
  Explorer rule (P7-M4).
- **Medium tiles**: `icons::TileGeom::MEDIUM` is 80 x 88 with a two-line name (`max-lines: 2` in
  `FileGridMedium`); Explorer's medium tile is 82 x 99 with up to three lines (measured with UI
  Automation, 2026-10-17).

Put to Jeremy with a recommendation, his answers:

- **The milestones**: recommended three, in this order: **P8-L1** the small bundle (Open and Open
  with for several items, the grid's PageDown, three-line medium names), **P8-L2** a zip writer that
  copies existing entries byte for byte (password zips, zip to zip, within one zip), **P8-L3**
  extraction to a temporary folder (an archive inside an archive, copies from 7z, tar and the rest
  into a zip). Jeremy: as recommended.
- **New files in a password zip**: recommended added unprotected with a note (Explorer's zip folder
  does so). Jeremy chose **protected, the password asked**. Then, recommended: **the zip's own
  method** (a ZipCrypto zip gets ZipCrypto entries, which Windows' zip folder still opens; an AES zip
  AES-256), the password asked once per job and **checked against an existing entry** before anything
  is written. Jeremy: as recommended. Rename, delete and move out need no password (the entries are
  copied as they are).
- **An archive inside an archive**: recommended **browsed read-only** in place (`C:\a.zip\inner.7z\sub`),
  the inner archive extracted to a temporary cache cleared at exit, any depth, copy out working and
  writes refused with a note. Jeremy: as recommended.
- **Between archives**: recommended **every copy into a zip**: from another zip (entries copied as
  bytes, passwords kept), from a 7z, tar, rar and the rest (extracted to a temporary folder first),
  within the same zip; a move the same, then the originals removed (from a zip, deleted from it; from
  a read-only format the move is refused). Nothing goes into the read-only formats. Jeremy: as
  recommended.
- **Open for several items**: recommended Explorer's: **the menu's Open and Return both open every
  selected item**, files with their apps, folders as new tabs in that pane, a question first above
  15 items. Jeremy: as recommended.
- **Medium names on three lines**: recommended **a taller tile, 80 x 107** (three lines at today's
  font; about 18% fewer medium rows on a screen; his P7-M4 choice for two lines). Jeremy: as
  recommended.

Checked on the machine after those answers, before P8-L1's code (an ignored probe test building the
classic menu for several files, removed again): **Explorer offers no Open with for several items**,
neither for two `.txt` files nor for a `.txt` and a `.png`; its default command (Open, verb `open`)
covers several. Put back to Jeremy: recommended **Open with for several files of one type anyway**
(the type's apps, the picked app given every file at once, no "Choose another app" since Windows'
chooser takes one file); the alternative was Explorer's one-file rule. Jeremy: as recommended.

## P8-L1: Open for several items, the grid's PageDown, three-line medium names

### What landed

- **Open for several items.** Return, a double-click and the Windows 11 menu's Open open every
  selected item when the row is in a selection of several (`model::open_set`; Explorer's): folders,
  drives and archives each in a new tab of the pane, files with the shell's default command for all
  of them at once on the verb worker (`VerbRequest::default_item`: `GetMenuDefaultItem` on the menu
  the worker builds, invoked by its offset; one job per folder, since a search tab's files can lie
  in several and one menu takes one folder's items), files inside an archive each through its PIDL.
  Above 15 items (`model::OPEN_ASK_ABOVE`, Explorer's `MultipleInvokePromptMinimum`) a message box
  asks "Open N items at once?" first; none under `TWINFM_SOAK`. The menu shows Open for any
  selection.
- **Open with for several files of one type** (`winmenu::same_type`: one file, or several whose
  extensions match ignoring case, none empty): the worker asks for that extension's apps as for one
  file, the submenu lists them alone (or a disabled "No apps for this type"), and the picked app is
  invoked with the data object of every selected file (`explorercmd::run`, unchanged).
- **The grid's PageDown and PageUp** are the details list's rule by grid rows, the cursor keeping its
  column (`model::grid_page_target`, the `Paging` global's `grid-target`); a target past a short
  last grid row is the last entry.
- **Medium tiles 80 x 107** (`icons::TileGeom::MEDIUM`; 88 before) with names on up to three lines
  (`max-lines: 3`, a 57 px name box); the cursor tile's whole-name overlay reads the list host's
  `name-h` (38 px large, 57 px medium), so it shows when three lines do not hold the name.
- Tests 224 to 227 (231 to 234 with P8-M1 merged in): `grid_paging_keeps_the_column`, `open_takes_the_selection_with_its_row`,
  `open_with_takes_files_of_one_type`; the menu's layouts, the submenu's rows and every medium
  geometry test updated.
- `tools/soak-p5m3.ps1`: after the sidebar walk, the left pane on `src\tree` goes to medium tiles,
  PageDown and PageUp, back to details, Return on two selected folders, Ctrl+2 and Ctrl+W twice
  (new tabs are placed after the active one, so plain Ctrl+W twice would close a tab the soak set
  up: found by running the steps alone), Ctrl+1; reported: several-item opens and their tabs.

### Calls made

- Folders open as tabs in the pane, after the active tab and in their order, the last one shown
  (Explorer opens a window each).
- Return with the cursor on a row outside the selection opens that row alone (Explorer's focused
  item is always in its selection; TwinFM's cursor can leave it).
- The question above 15 items is TwinFM's own message box; files open on the verb worker, folders
  at once.

### Scripted checks

The release build of this branch, scratch folders under `%TEMP%\twinfm-p8l1\`:

```
[open] pane=0 3 items: 3 folder(s) to tabs, 0 file(s)          (CtrlA on a, b, c; key Return)
[tab] pane=0 opened tab 2 of 2 (id 3) on ...\dirs\a
[tab] pane=0 opened tab 3 of 3 (id 4) on ...\dirs\b
[tab] pane=0 opened tab 4 of 4 (id 5) on ...\dirs\c
[auto] scrollpos pane=0 content-y 0.0 current-row 1             (C:\traces\bigdir, large tiles, ten to a grid row)
[auto] scrollpos pane=0 content-y 0.0 current-row 31            PageDown: the last grid row on screen, same column
[auto] scrollpos pane=0 content-y -302.0 current-row 61         PageDown: a screenful less one grid row
[auto] scrollpos pane=0 content-y -302.0 current-row 31         PageUp: the first grid row on screen
[auto] scrollpos pane=0 content-y 0.0 current-row 1             PageUp
[winmenu] ... shown ...: [Cut Copy Share Delete] Open | Open with > | Open in terminal | ...   (a.txt and b.txt)
[winmenu] submenu "Open with" ...: [] Notepad | The main calibre program | ... | Zed           (no Choose another app)
[winmenu] ... shown ...: [Cut Copy Share Delete] Open | Open in terminal | ...                  (a.txt and c.png)
[winmenu] submenu "Open with" ...: [] Notepad | ... | Zed | --- | Choose another app            (a.txt alone)
```

Medium tiles captured with the software renderer over names of one, two and three lines, a
92-character name and a 45-character name without spaces: three lines, the third elided, and with
the cursor on the long name its whole name over the tiles below. Opening several files was not
scripted (it starts their apps on the desktop): a console check.

### Gate

ab33 (Jeremy, console, 2026-10-20 15:39, 28 min): `target-p8l1` (d5216db) against `target-p8m1`
(47fc959, P8-M1's gate build: P8-M1 merged first, so main was merged into this branch and the gate
build made again), three pairs interleaved, the candidate second in every pair:

| Setting | p8m1 (WPR) | p8l1 (WPR) | Render p50 (ms), ref / cand | Medians |
|---|---|---|---|---|
| defaults | 18.2, 27.3, 26.9 | 18.3, 29.0, 27.2 | 2.21, 3.79, 3.45 / 2.25, 4.07, 3.84 | 26.9 / 27.2 |
| large grid | 10.3, 14.2, 14.3 | 10.7, 14.4, 14.4 | 1.10, 1.65, 1.68 / 1.11, 1.68, 1.71 | 14.2 / 14.4 |
| medium grid | 19.0, 19.4, 17.4 | 16.2, 16.8, 16.6 | 2.27, 2.27, 2.31 / 2.04, 1.99, 2.04 | 19.0 / 16.6 |

**The session changed state after the first round** (ab23's note): the defaults' and the large grid's
render p50 rose by about half for both binaries from round 2 on, so the defaults' medians sit over the
20% bar for the reference as for the candidate. Read like for like: round 1 (the normal state) 18.2 /
18.3 and 10.3 / 10.7; rounds 2 and 3 (the slow state) 27.3 / 29.0 and 26.9 / 27.2, and 14.2 / 14.4 and
14.3 / 14.4, each pair with the candidate in the second slot (about a point high in ab30). No gap
counts by the rule (medians 0.3 and 0.2 apart; not every reference run below every candidate run).
The medium grid sits 2.4 points lower: its taller tiles hold fewer grid rows to compose and draw.
Jeremy accepted ab33 read like for like (2026-10-20). His console checks below, on `target-p8l1`
unelevated: all six as expected (2026-10-20).

### Console checks (for Jeremy, unelevated, after the gate)

1. Select three text files and press Return: each opens (in one editor window if the editor takes
   several). Then the same through the right-click menu's Open.
2. Select two `.txt` files, right-click: Open with lists the apps; pick one: both files open in it.
   A `.txt` and a `.png` together: no Open with.
3. Select two folders and press Return: two new tabs after the current one, the second shown.
4. Select 20 files and press Return: TwinFM asks "Open 20 items at once?"; No opens nothing.
5. The large and the medium grid on a big folder: PageDown and PageUp move by screenfuls and keep
   the column, as in Explorer's icon views.
6. Medium tiles in a folder with long names: three lines, then "..."; the cursor tile shows the whole
   name below it.

P8-L1 squash-merged on Jeremy's word as #77 (20a140a); `target\release` rebuilt from it. Its gate build
`target-p8l1` (d5216db) is the next reference for either stream.

## P8-L2: zips edited byte for byte, password zips written, zip to zip

### What landed

- **`fs::zipraw`**: a zip's central directory read without the `zip` crate's model, and an edit's
  writer. Each kept entry's local record, data and data descriptor, and its central record, are
  copied as bytes; only the name (a rename: `\` read as `/`, the UTF-8 flag set for a name that is
  not ASCII, Info-ZIP's Unicode path field dropped) and the local record's offset change. An offset
  past 4 GiB goes into the zip64 extra field (rebuilt with the sizes already there); 65,535 entries
  or more, or an archive past 4 GiB, get the zip64 end records; a self-extractor's leading bytes are
  read past and not written. New entries are still the crate's: its `ZipWriter` appends after the
  copied ones (it records offsets from the stream's position), and its central directory is read
  back, cut off and merged into ours (`Out::adopt`). Every edit of `fs::archivewrite` (add, new
  item, rename, delete) goes through it; the names are read here too (UTF-8 flag, else the Unicode
  path field, else code page 437, as the crate and Windows' zip folder read them).
- **Password-protected zips are written** (P7-M3 refused them). `archivewrite::protection` reads the
  zip's first encrypted file: ZipCrypto, WinZip AES (its strength), or PKWARE's strong encryption
  (refused). A job that adds files, a new file, or plain files from another zip asks for the
  password once: a dialog of TwinFM's own on the file operation worker (`ask_password`, an in-memory
  dialog template, a masked box, OK and Cancel, owned by the app's window as the conflict question
  is; it is a `#32770`, so P7-M3's raising of hidden dialogs covers it), or the driver's
  `zippassword <text>`; `check_password` reads the zip's smallest encrypted file whole (a ZipCrypto
  entry's one-byte check lets one wrong password in 256 through, its checksum does not; an AES
  entry's authentication code), and a wrong password asks again ("That password does not open
  ..."); Cancel cancels the job. New files get the zip's own method: ZipCrypto (`zip::unstable`'s
  `with_deprecated_encryption`), or AES-256 for an AES zip (the crate's `aes-crypto` feature, new in
  the build: `aes`, `hmac`, `pbkdf2`, `sha1`, `zeroize`, `constant_time_eq`). Rename, delete and move
  out ask nothing; a scripted job without the script's password, or with a wrong one, is refused,
  never shown the dialog.
- **Between zips and within one** (`Plan::FromZip`, `archivewrite::copy_from_zip`; `BETWEEN_ARCHIVES`
  is gone): every entry below the items copied as bytes under its new name, an encrypted one keeping
  its own password; a plain file going into a zip whose new files are encrypted is read and written
  again with the key; a copy into the items' own folder is named Explorer's way ("a - Copy.txt",
  "a - Copy (2).txt"); a clash asks the P7-M3 question (Replace, Skip, Cancel). A move from another
  zip copies, then deletes the items from the source zip; a move within one zip is one rewrite that
  renames the items' paths, refused into an item itself and onto a file already there (no file by
  file merge). From the other formats into a zip is still refused (`between_why`: "copying from .7z
  archives into a zip is not supported yet"; P8-L3). The clipboard, Shift+F5/F6 and drops of archive
  items already named the in-archive paths, so no new path was needed to reach the job.
- Tests 234 to 238 (with P8-L1 and P8-M1 merged in): `zipraw`'s extra field rebuilding and its copy
  of every byte, `password_zips_and_copies_between_zips` (crate-made ZipCrypto and AES zips; the
  password check right and wrong; adds, a new file, a rename and a delete read back with the
  password; a plain file encrypted on the way in; an encrypted one keeping its password on the way
  out; a copy beside itself; Skip on a clash; a move within one zip, and into itself refused),
  `code_page_437_has_128_characters`; `jobs_are_classified` and `zip_jobs_on_the_worker` (a wrong
  script password refusing with the zip byte for byte, the right one adding, the tar.exe entries
  still readable after a rename; copies and moves between two zips and within one) changed.
- `tools/soak-p5m3.ps1`: after the archive cycle, a.zip and the setup's `pw.zip` (tar.exe, ZipCrypto,
  password twinfm): a.txt into pw.zip with `zippassword twinfm`, renamed, copied back, moved back,
  renamed again, and a copy pasted beside itself in a.zip then deleted; reported: copies and moves
  from a zip, passwords from the script and right.

### Calls made

- A file added to a password zip is encrypted even where the zip also holds plain files (the
  zip's first encrypted file decides the method).
- A move within one zip onto a file already there is refused rather than asked about (a rename moves
  whole items; merging file by file is not done).
- The password dialog is TwinFM's own on the file operation worker, not Windows' credential dialog.

### Scripted checks

The release build of this branch, a copy of `C:\traces\password-twinfm.zip` and a plain zip made by
`tar.exe`, under `%TEMP%\twinfm-p8l2\`:

```
[archive] password for ...\pw.zip (ZipCrypto): from the script        (zippassword nope)
[archive] password for ...\pw.zip: wrong
[archive] job id=1 "copy in" on ...\pw.zip: refused (the password does not open this zip)
[archive] password for ...\pw.zip: right                               (zippassword twinfm)
[archive] job id=2 "copy in" on ...\pw.zip: Done in 17 ms
[archive] job id=3 "copy from a zip" on ...\pw.zip: Done in 16 ms      (plain.txt, encrypted on the way)
[archive] job id=4 "copy from a zip" on ...\plain.zip: Done in 15 ms   (sub\inner.txt, keeps its password)
[archive] job id=5 "copy from a zip" on ...\plain.zip: Done in 13 ms   (Ctrl+C, Ctrl+V: inner - Copy.txt)
```

Read back by Windows' `tar.exe --passphrase twinfm`: `secret.txt` and `sub\inner.txt` (the fixture's)
and `added.txt` and `plain.txt` (TwinFM's) extract with their contents. `plain.zip`: .NET's
`ZipFile` reads `plain.txt`; `inner.txt` and `inner - Copy.txt` extract with the password through
`tar.exe` (.NET reads no encrypted entry at all); Windows' zip folder lists all three. The soak's new
steps alone, twice, under `TWINFM_SOAK` in `%TEMP%\twinfm-m5\p8l2-arch`: fourteen jobs Done, both
zips as they started, pw.zip's a.txt extracting with the password.

### Gate

ab34 (Jeremy, console, 2026-10-20 17:04, 9 min): `target-p8l2` (278d47f) against `target-p8l1`
(d5216db), the defaults only (no list, grid or layout change), interleaved, the candidate second:

| Run | p8l1 (WPR) | p8l2 (WPR) | Render p50 (ms), ref / cand |
|---|---|---|---|
| 1 | 19.2 | 19.8 | 2.27 / 2.32 |
| 2 | 28.5 | 27.0 | 3.88 / 3.80 |
| 3 | 27.0 | 27.0 | 3.82 / 3.80 |

Medians 27.0 / 27.0; no gap counts. As in ab33, the session changed state after the first round
(render p50 up by about two thirds for both binaries), so both medians sit over the bar; like for
like the pairs read 19.2 / 19.8 in the normal state and 28.5 / 27.0, 27.0 / 27.0 in the slow one.
Two gates in a row changed state at the same point (after round 1), which is worth a look before
the next phase's gates (the machine, not either binary: the reference moves with it).

**Why (looked into at Jeremy's request, 2026-10-20)**: the traces' own samples (`xperf -a profile
-detail` and `-a dumper`, per process, per CPU) show the Claude desktop app (`claude.exe`, the app
these sessions run in, started 09:23) using 48 to 56 s of CPU in each ab33 and ab34 trace against
about 12 s in ab32's, almost all of it one thread (39,000 of a 40 s trace's samples: a full core,
still spinning after the gate, 4.9 s of CPU in 5 s). Every part of TwinFM's main thread grew by the
same factor in the slow runs (render pass, text, ListView rows, timers, accessibility: no code path),
which is what losing a core's throughput looks like; the 12 logical CPUs are six cores with two
threads each, and the spinning thread sat mostly on CPU 2 (its sibling, CPU 3, also loaded). In
ab34's normal run TwinFM's main thread ran mostly on CPU 8; in its slow run on CPUs 2 and 3, the
spinning thread's core. Where the scheduler puts the main thread decides each run, so it reads as a
"state change" that the reference and the candidate share. Not TwinFM: both binaries moved alike,
and ab32, before the spin, has no such runs. **Before a gate, the Claude app must be idle or
restarted** (a thread spinning there takes a core from the scenario); a wrapper check for a process
holding a core is a candidate for the next gate's wrapper.

### Console checks (for Jeremy, unelevated, after the gate)

1. Copy a file into `C:\traces\password-twinfm.zip` (a copy of it): TwinFM asks for the password;
   a wrong one asks again, `twinfm` adds it. Open the zip in Explorer: the new file asks for the
   password and opens with it; `secret.txt` still opens.
2. Rename and delete inside that zip: no password asked, the rest still opens.
3. Copy a file from one zip into another (Shift+F5, Ctrl+C/Ctrl+V, a drag), and move one between
   two zips: it arrives, and leaves the source on a move.
4. Ctrl+C and Ctrl+V of a file inside one zip: "x - Copy.ext" beside it. Move a file into a folder of
   the same zip.
5. A 7z item copied into a zip: refused with a note ("not supported yet").
6. Cancel at the password dialog: nothing written.
