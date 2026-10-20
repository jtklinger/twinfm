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

The P8-M1 merge (#76, a01ad14) was run from the main checkout with `gh pr merge --squash
--delete-branch`, which also removed the session worktree holding the branch; every commit was pushed,
so nothing was lost. Later merges leave out `--delete-branch` and delete the branch afterwards.

## P8-M2: Markdown

Branch `feat/p8-m2-markdown`, from a01ad14 (P8-M1 merged).

### Settled first (2026-10-20, before any code)

Checked against the code and the pinned Slint (c1d3a8d):

- Slint's `StyledText` has two constructors, `from_plain_text` and `from_markdown`; no span builder is
  public. `from_markdown` takes paragraphs, emphasis, strikethrough, inline code, links, lists, `<u>`
  and `<font color>`, and fails the whole text on anything else (headings, images, quotes, code
  blocks, tables, task markers, footnote references, rules, other HTML). So each block's inline events
  from pulldown-cmark are written back as inline Markdown with every ASCII punctuation character in
  text escaped, emphasis always as `*`, code spans fenced one backtick longer than any run inside,
  links as `[text](<dest>)`, a soft break as a space, a hard break as a line, U+E541 (Slint's
  placeholder) removed; on an error the block falls back to `from_plain_text`, logged. `StyledText` is
  `Send` and works as a field of a model struct. The element always wraps, has no font weight (headings
  are bold by wrapping their text in `**`), no selection, and `link-clicked` gives the destination as
  written.
- pulldown-cmark 0.13.4 is in the lock through Slint (`default-features = false`); TwinFM depends on
  `=0.13.4` the same way, so no second copy. Tables, task lists, strikethrough, footnotes, GFM and YAML
  front matter (skipped) are runtime options.
- The blocks are a flat list (kind, level, indent, quote depth) in a **ListView**, which builds only the
  rows on screen: a 2 MB file is some 10,000 to 20,000 blocks, which a ScrollView with a `for` would
  build and shape all at once, and again at every width change. The cost is a scrollbar thumb that
  drifts with uneven row heights. Code blocks are a plain `Text` in Consolas (the font Slint already
  maps monospace to) on a tinted rectangle; quotes one 3 px bar per depth; rules 1 px; list markers
  and numbers drawn from Rust; task boxes Segoe Fluent Icons E739 / E73A; tables a `GridLayout` with a
  nested `for` (supported at this rev), columns aligned from the table's alignments.
- Images: `image` 0.25.10 (png, jpeg), `gif`, `image-webp` and `resvg` 0.48.1 are already built for
  Slint; TwinFM uses them on the worker, under `image::Limits`, at most 8,192 px a side (P8-M1's rule)
  and 64 MB of pixels per document; an image file that is cloud-only shows its alt text and is never
  read. Remote images show their alt text.
- Links reuse `shellmenu::open_item` and the double-click path; URLs (`http`, `https`, `mailto` only)
  open without the shell's IDList mask; local links resolve against the file's folder after
  percent-decoding and dropping `?` and `#`. Scripted runs never launch a link (a driver step logs
  what a link would do).
- Seams in P8-M1: `Class` gains `Markdown` (`.md`, `.markdown`; the archive and cloud checks apply);
  the worker gains `serve_md`, answering the blocks, then each image as it is decoded (checking the
  generation between images); the UI state's PDF fields move into a `Doc` enum. Nothing runs per
  frame; the gate's cursor sits on no `.md`.

Jeremy's answers (2026-10-20), each the recommendation:

- **Long code lines wrap** inside the code block.
- **HTML blocks show their text, and local `<img>` pictures**: the tags are stripped (no centring).
- **A link to a local program or script** (`.exe`, `.bat`, `.cmd`, `.ps1`, `.lnk`, `.vbs`, `.msi` and
  the like) **shows it in the pane** (its folder, the cursor on it); nothing runs. Documents and folders
  open as a double-click does, web links in the browser.
- **`#anchor` links scroll to the heading** (GitHub's heading slugs).

Lead's calls: footnote references shown as `[n]`, footnotes at the end as GitHub does; a UTF-16 file
(by its BOM) decoded, other invalid UTF-8 decoded lossily; the 2 MB cut at the last newline before
2 MB (else the last character boundary) with a note block; text selection is not in P8-M2 (Slint's
text elements have none at this rev).

## Open questions

Written 2026-10-20 at the kickoff.

- **hayro on real PDFs**: how many of Jeremy's own PDFs it renders right, and how fast; the P8-M1
  console check is the measure. If it falls short, PDFium is the recorded alternative (Jeremy's call).
- **Carried from Phase 7**: Google Drive's volume found by its name; a sync-root scratch folder for
  the soak; the Slint whole-row snap patch and upstreaming; the dead share console check (P5-M1).
