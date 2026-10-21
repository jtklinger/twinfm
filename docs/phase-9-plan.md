# Phase 9: TwinFM takes its column widths from the user, previews six more types, and finishes thumbnails and cloud marks

Written 2026-10-21, when Phase 8 closed with P8-M1, P8-M2 and the Phase 7 leftovers P8-L1 to P8-L3
merged (`phase-8-plan.md` § Phase 8 close). Jeremy uses TwinFM daily from `target\release`, and the
Phase 8 close asked for the other half of daily use: a written list of what gets in the way. Phase 9's
first item is that list's first entry. Phase 9's goal, chosen by Jeremy on 2026-10-21: **adjustable
column widths in the details list, the preview pane reading plain text, pictures, HTML, Word, Excel
and PowerPoint, and the two half-milestones carried since Phase 7, thumbnails beyond the listing and
cloud polish**. Not in Phase 9, by the same decision: content search, zoom and text selection in the
preview, password PDFs through PDFium, the Markdown details deferred at the Phase 8 close, the Slint
fork's whole-row snap patch and upstreaming. Accessibility stays deferred for much later (Jeremy,
2026-10-15).

What Jeremy decided at the kickoff (2026-10-21), from the choices put to him:

- **Adjustable column widths** is new, from daily use, and goes first: "One item that needs added for
  phase 9 is to allow for adjustable column widths."
- **The preview types**, in his words: "txt, jpg/other images, html, docx, xlsx, pptx".
- **Office documents are read by TwinFM into its own text and tables** (option 1 of three): a Word
  file's paragraphs, headings, tables and pictures, an Excel file's sheets as tables, a PowerPoint
  file's slide text and pictures, all drawn through the block list the Markdown preview already draws.
  Nothing to install, works without Office; layout, fonts and charts are lost. Offered beside it:
  Windows' own preview handlers (faithful, but Office must be installed, it runs in another process
  and lays a foreign window over TwinFM's, the path Jeremy turned down for PDF at the Phase 8
  kickoff), and a picture of the first page from the shell's thumbnail cache (an afternoon, one blurry
  page, nothing to scroll).
- **Thumbnails beyond the listing and cloud polish**, both carried from the Phase 6 and Phase 7
  closes: "Add more thumbnails and cloud polish." All six recorded pieces are taken as wanted (Jeremy
  struck none when the list was put to him): thumbnails in search result tabs, in archive levels and
  in the details list's drag image; OneDrive's sync states beyond the tick and the cloud, Google
  Drive's mark drawn smaller in the tiles, and the hydration check.
- **The order**: column widths, then the three cheap preview types, then the three Office formats,
  then the bundle. "That order is fine."
- **Plain text wraps its long lines** as the Markdown code blocks do (his 2026-10-20 choice), no
  horizontal scroll in the column: "wrap like MD" (asked after the builder check).

The starting point is main at 219b4dc (the Phase 8 close). What the four milestones rest on today:

- **The details list's columns** are fixed in `src/ui/pane.slint`: in every row (`FileRows` and its
  variants without the Type cell and with 32 px rows) the Type, Size and Modified `Text`s are 120, 90
  and 140 px wide and Name takes the rest; the header is three `HorizontalLayout`s of `HeaderCell`s
  (one per Type column state and one for the grid) with the same widths; the rename editor's width
  subtracts the fixed columns (350 px, 230 px without Type) from the right; `dnd.rs` `Layout` and
  `window_layout` in `main.rs` mirror the geometry for hit testing, and the driver (`src/auto.rs`)
  aims at rows and cells from the same figures. A binding per row on a pane property cost 0.7 of a
  point on the gate when the Type column was made per pane (P3-M1), which is why the list variants
  are chosen once per pane and never per row. Settings are `%APPDATA%\twinfm\config.toml`
  (`src/settings.rs`, `Config`), with per-pane keys already (`view_left`, `tile_left`,
  `show_protected_left`); a scripted run reads none of it.
- **The preview** (`src/preview.rs`, `src/markdown.rs`, `src/ui/preview.slint`): one worker thread
  owning the open document; `Class` names what the cursor file is (`Folder`, `Other`, `Archive`,
  `CloudOnly`, `Pdf`, `Markdown`) and `classify` decides from the listing's facts without a read; a
  Markdown file becomes a flat `Vec<Block>` (`Kind`: `Para`, `Heading`, `Code`, `Rule`, `Table`,
  `Image`, `Note`; list and quote depth, a bullet, number or task box on an item's first block;
  tables as rows of cells with column shares; code and tables over 40 lines split into chunks), shown
  in a ListView in `mode` 1; the inline text of a paragraph, heading or cell is written back as
  Markdown and made a `StyledText`; local pictures are decoded one at a time on the worker (`image`
  with png, jpeg, gif, webp; `resvg` for SVG; 8,192 px a side, a 64 MB pixel budget per file,
  `PIXEL_BUDGET`), the generation checked between them; the 2 MB cap (`markdown::MAX_BYTES`) cuts at
  a line break and puts a `Note` block first; a UTF-16 BOM is decoded and other invalid UTF-8
  replaced. Crates in the build: `pulldown-cmark =0.13.4`, `image =0.25.10`, `resvg =0.48.1`,
  `roxmltree =0.21.1` (the packaged apps' manifests, P7-M2), `zip =9.0.3` (archives), `hayro =0.8.0`.
  Every dependency added since P7-M3 is pinned by exact version with its licence noted in
  `Cargo.toml`.
- **Thumbnails** (`src/icons.rs`): a plain file in a folder listing whose extension is in
  `THUMB_EXTS` is keyed `thumb:<path>|<modified>|<size>` when `thumbnails` is on, and the icon workers
  ask the shell's thumbnail cache at the list's or the grid's icon size; a search tab's entries and an
  archive level's entries take their type's key (`ext:`), so they show the type's icon in the 32 px
  lists and the tiles (P3-M5 Deferred); the details list's drag image carries the shell's 64 px icon
  (`icon from the shell`, or `the worker` when the icon asked at the press has landed, P5-M3), the
  grid's the tile's own picture (`icon from the tile`, P4-M3).
- **Cloud marks** (`src/cloud.rs`, `icons::Mark`): a sync root file's `s<N>` mark comes from its
  attributes on the enumeration worker (tick for on this disk, cloud for cloud-only; the other
  states, syncing, pending, error and warning, draw the shell's imageres glyph for the state the
  attributes give, unfiltered, P6-M1); a Google Drive entry's `o<N>` mark is the shell's overlay
  index, looked up when the row is on screen, and in a tile the overlay is drawn from the image list
  chosen by icon size (the jumbo list at 96 px), so a large cloud covers the tile's lower left (P6-M1
  Deferred); whether the folder's watch delivers a record when a file is hydrated or freed (so its mark
  changes without a refresh) was never checked, since it means changing a file's state in Jeremy's
  OneDrive (P6-M1 Deferred, "the hydration check").

Four milestones, in the order Jeremy gave on 2026-10-21 (do not fold them):

1. **P9-M1, adjustable column widths.** The details list's Type, Size and Modified columns take their
   widths from the user: a 6 px grip on each boundary between two header cells (Name | Type, Type |
   Size, Size | Modified; with the Type column off, Name | Size and Size | Modified; none at the
   list's right edge, which is pinned), the pointer the east-west resize cursor over the grip, the
   whole list following live as the drag moves. **A grip moves the boundary under it**, as Explorer's
   do: the column to its left grows by what the column to its right gives up, so the grip stays under
   the pointer (Name has no width of its own and takes what the three leave, so dragging the Name |
   Type boundary right shrinks Type and the list reads as Name growing). A column has a floor of 40 px
   and the Name column one of 120 px (a drag past them stops there; no column can be hidden by
   dragging, that is the Type column's setting). A double-click on a grip fits the column to its left
   to the widest cell on screen plus padding (the rows in the viewport, never the whole listing, so a
   40k folder costs nothing more), taking the difference from the column to its right (Name for the
   first grip). The widths are per pane, logical px, saved as `col_type_left`, `col_size_left`,
   `col_modified_left` and the `_right` three in `config.toml` (defaults 120, 90, 140, so a file
   without the keys loads today's layout; out of range values clamped on load), and apply to every
   tab of the pane and to both row heights. They are **requested** widths: a pane narrower than the
   three plus Name's floor clips the columns at its right edge as the fixed 350 px are clipped today
   (Name's cell at zero, `dnd`'s name-cell extent clamped at zero), nothing shrinks to fit, so no row
   reads a pane-width-dependent figure. The
   header cells, every row variant (with and without Type, 22 and 32 px), the rename editor's width,
   `dnd::Layout` (the name cell's extent for `hit`, the drop highlight and the band's hit test) and
   the driver's cell aim read the pane's widths from one place (pane properties set once per pane,
   never a per-row binding on a changing value beyond the width itself; the exact binding shape is
   settled first against the gate, below). Driver: `colwidth <pane> <type|size|modified> <px>` sets a
   width as a drag's end does, `coldrag <pane> <type|size|modified> <dx>` drags the grip `<dx>` px
   through Slint's pointer path, `colfit <pane> <col>` double-clicks it; `[cols] pane=N <col> W ->
   W2 px (drag | fit | script | config)` on every change, `pane=N fit <col>: widest of K rows on
   screen = W px`, and `pane=N widths type T size S modified M (name N px)` once per layout change.
   Tests: the clamp and floors, the fit from a set of cell widths, the save and load round trip, the
   name cell's extent in `dnd` for several widths. Soak: a `coldrag` and a `colfit` per cycle in
   `tools/soak-p5m3.ps1`, the widths read back from the `[cols]` line. Gate: the defaults triplet
   (every row now reads a property instead of a constant: this is the cost the triplet measures; a
   gap means another binding shape, settled before the merge) and a settings triplet with
   `C:\traces\cols.toml` (read-only, both panes' widths at other values, say 200, 110, 180; its
   stderr shows `[settings] file ... (read)` and the `[cols] ... widths` line with those figures).
   Console checks: a drag in each pane with the Type column on and off, at 100% and 150% scaling,
   with thumbnails on, the fit on a folder of long type names, the grip's cursor, a width that
   survives a restart, the layouts set up unequal (Phase 6's rule).
2. **P9-M2, text, pictures and HTML in the preview.** Three new `Class`es, each a message when inside
   an archive or cloud-only as a PDF is. **Plain text** (`.txt`, `.log`, `.ini`, `.cfg`, `.csv`,
   `.json`, `.xml`, `.yaml`, `.yml`, `.toml`, and source: `.rs`, `.py`, `.ps1`, `.sh`, `.c`, `.h`,
   `.cpp`, `.cs`, `.js`, `.ts`, `.css`, `.slint`, `.bat`, `.cmd`; `.md` stays Markdown, the full list
   is a constant settled in the brief): read at most 2 MB as Markdown is, the encoding decided as
   Markdown's is (a UTF-8 or UTF-16 BOM, else UTF-8, invalid sequences replaced; a file over 2 MB cut
   at a line break with the Note first), shown as `Code` blocks in the monospace font in chunks of 40
   lines (the Markdown preview's own chunking, so a 2 MB log builds only the rows on screen), tabs
   drawn as four spaces, no syntax colours in this phase, **long lines wrapped as the Markdown code
   blocks wrap them** (Jeremy, 2026-10-21: "wrap like MD"; no horizontal scroll in the column).
   **Pictures** (`.jpg`, `.jpeg`, `.jpe`, `.png`,
   `.gif` (the first frame), `.webp`, `.svg`, and `.bmp`, `.tif`, `.tiff`, `.ico` added through the
   `image` crate's `bmp`, `tiff` and `ico` features; HEIC, HEIF and AVIF are not previewed, "No
   preview"): decoded on the worker through the P8-M2 picture path (8,192 px a side, scaled down to
   fit; the 64 MB budget), a JPEG's EXIF orientation applied (the `image` crate reads it; a phone's
   portrait photo shows upright), shown as one `Image` block fitted to the column's width and never scaled
   up past its own size, with a `Note` block under it reading the pixel size, the file size and the
   format (`1920 x 1080, 2.3 MB, JPEG`); a file the decoder refuses shows "This picture could not be
   read". **HTML** (`.html`, `.htm`): parsed by an HTML parser into the Markdown block list, the
   mapping fixed before any code: h1 to h6 headings, p and div-with-text paragraphs, ul, ol and li as
   list items (nested depth kept), blockquote as quote depth, pre as code, hr as a rule, table, tr,
   th and td as a table, img with a local src as a picture (against the file's folder, as Markdown's
   are; a remote one its alt text), a and strong, em, b, i, code inside a block as inline markup;
   script, style, head, nav, noscript and comments dropped; every other element's text flowed into
   the enclosing block; entities decoded; the 2 MB cap applies to the file. The parser is `html5ever`
   (the Servo parser, MIT OR Apache-2.0) or `tl` (one crate, no dependencies) by the pin check the
   brief makes first; a hand-written tag scanner is the fallback if both pull in too much. Links in
   the blocks work as Markdown's do (`preview::link`: `http`, `https`, `mailto` to the browser, a
   local path opened or shown; scripted runs never launch). Driver: no new step (`waitpreview` covers
   the three); `[preview] pane=N gen=G <path>: text, L lines, B bytes, <encoding>[, cut at 2 MB] |
   picture WxH px, <format>, decoded in T ms | html, B blocks (H headings, C code, T tables, I
   images), parsed in T ms[, cut at 2 MB]`, `picture failed: <why>`. Tests: the text chunking and
   encodings, the picture fit (never scaled up; a 10,000 px side scaled to 8,192), the HTML mapping
   on a fixture with every mapped element, a script and a style dropped. Soak: three files added to
   `%TEMP%\twinfm-m5\preview` (`h-notes.txt`, `i-photo.jpg` made with System.Drawing, `j-page.html`
   with a local picture) and the cursor through them with `waitpreview`; the report counts text,
   picture and HTML previews. Gate: the defaults triplet and the preview triplet
   (`C:\traces\preview.toml`). Console checks: a Rust source file, a 2 MB log, a UTF-16 text file,
   JPEGs from a phone, a 10,000 px panorama, an animated GIF, an SVG with text, a saved web page
   beside Edge, a page of nested lists and tables, a page of nothing but a script.
3. **P9-M3, Word, Excel and PowerPoint in the preview.** The three Office Open XML formats (`.docx`,
   `.xlsx`, `.pptx`; the binary `.doc`, `.xls` and `.ppt` are not previewed, "No preview", and a
   macro-enabled `.docm`, `.xlsm`, `.pptm` reads as its plain form) read by TwinFM on the preview
   worker into the block list: the file opened as a zip (the `zip` crate, already in), the parts read
   with a **streaming XML reader** (`quick-xml`, BSD-like MIT, pinned; `roxmltree` builds a whole
   tree and cannot stop after 500 rows or hand back a partial document, so it is wrong for this: a
   100,000-row sheet is tens of MB of XML and its tree a multiple of that on the worker), so each
   part is read once from the zip's stream and reading stops at the cap: 500 rows of a sheet, and 64
   MB of any one part (`OFFICE_MAX_XML`; over it the blocks so far and a Note "Only the first part of
   this document is shown"). No Office crate unless the pin check finds one that adds nothing beyond
   `zip` and a streaming reader. **Word**: `word/document.xml` paragraphs in order, a paragraph's style
   (`Heading1` to `Heading6`, `Title`) a heading of that level, list paragraphs (`numPr`) list items
   at their level with a bullet or a number counted by TwinFM, tables as tables, runs' bold, italic
   and inline code (a monospace font name) as inline markup, hyperlinks (`w:hyperlink` with the
   relationship's target) as links, inline pictures (`a:blip` through `word/_rels/document.xml.rels`
   to `word/media/*`) decoded from the zip through the P8-M2 picture path and the budget, page
   breaks as rules, footnotes and comments dropped, headers and footers dropped. **Excel**:
   `xl/workbook.xml` lists the sheets; one sheet is shown at a time, the first by default, with a
   `Heading` block naming it and a row of links `Sheet1 | Sheet2 | ...` (an `Anchor`-like link kind
   that switches the sheet on the worker and re-answers the blocks, `#sheet=N`), the sheet's used
   range as a table of at most 500 rows and 50 columns (a Note says what was cut), shared strings
   resolved, numbers formatted by their cell's number format where it is a date (serial to
   `yyyy-mm-dd`) or a plain number (as stored, trailing zeros trimmed), formulas showing their cached
   value, merged cells as their top-left value, empty leading columns and rows dropped; a chart sheet
   shows "Chart sheets are not previewed". **PowerPoint**: `ppt/presentation.xml` orders the slides;
   each slide a `Heading` block `Slide N` then its title placeholder as a level-2 heading, its text
   bodies as paragraphs and list items (a text body's paragraphs with `a:buChar` or `a:buAutoNum`
   are list items at their level), its tables as tables, its pictures decoded through the budget, a
   rule between slides; notes pages dropped. Every shape's text is in document order (the
   `p:spTree` order). Tests on fixtures written by the tests themselves (a zip of hand-written parts
   for each format, as P8-M1's hand-made PDF was), one per format plus a file with a missing part
   and a file that is not a zip ("This document could not be read"). Driver: `previewlink
   #sheet=N` (the existing `previewlink`) switches the sheet; `[preview] pane=N gen=G <path>: docx, B
   blocks (H headings, T tables, I images), parsed in T ms | xlsx, S sheets, sheet "<name>" R x C
   (cut at 500 rows|50 columns), parsed in T ms | pptx, N slides, B blocks, parsed in T ms`,
   `unreadable: <why>` as today. Soak: `k-report.docx`, `l-table.xlsx` and `m-deck.pptx` written by
   the setup from hand-made parts into `%TEMP%\twinfm-m5\preview`, the cursor through them, the
   second sheet followed; the report counts the three. Gate: the defaults triplet and the preview
   triplet. Console checks: Jeremy's own documents beside Word, Excel and PowerPoint (or Edge, which
   opens all three), a 200-page Word file, a workbook with ten sheets and 100,000 rows, a deck of
   60 slides with pictures, a password-protected Office file (it is not a zip of parts: "This
   document could not be read" with the password noted if the OLE wrapper says so), a `.doc`.
4. **P9-M4, the bundle: thumbnails beyond the listing and cloud polish.** Six pieces, each small,
   each settled first as P7-M4's were. **Thumbnails in search result tabs**: a result's entry keyed
   `thumb:` as a folder listing's is when `thumbnails` is on (the result's full path is known; the
   search walker's entries carry modified time and size already). **Thumbnails in archive levels**:
   an entry inside a zip or another format keyed by its type (`ext:`) stays, since the shell's
   thumbnail cache cannot see inside an archive without extracting; what changes is that a nested
   archive's extracted copy (P8-L3) and a level the shell lists are not asked either, and the record
   says so, so this piece is a check and a note rather than code unless the pin check finds the zip
   folder's own thumbnail provider cheap (it extracts the file; if it does, not done). **The details
   list's drag image with a thumbnail**: the `drag:` request the press already sends to the icon
   worker at 64 px (`main.rs`, today with `thumbnail: false`) asks for the thumbnail when the pressed
   row shows one, so the drag image carries a sharp 64 px picture (never the table's 32 px entry
   scaled up); the shell's icon as today otherwise, and when the worker has not answered by the
   drag's start; `[dnd] drag image ... icon from the thumbnail`, so a photo dragged out of a 32 px
   list carries its picture as a tile drag does. **OneDrive's sync states
   beyond tick and cloud**: the glyph for syncing, pending, error and warning is already drawn from
   the attributes (P6-M1); what is missing is the property read that tells pending from error when
   the attributes alone cannot (`System.StorageProviderState`, about 23 ms per file on the icon
   workers): read it only for a sync root file whose attributes say "in progress" or "error" (the
   `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` and pinned combinations settled in the brief), never for
   the plain tick and cloud rows, so a folder of 40k synced files costs nothing more;
   `[cloud] state read <path>: S in T ms`. **Google Drive's mark smaller in the tiles**: the overlay
   drawn in a large or medium tile is taken from the image list by the mark's size, not the icon's
   (the 32 px list for a 96 px picture, 24 px for a 48 px one, at 100%), placed at the picture's lower
   left; the link arrow in the tiles takes the same rule (it has been the jumbo overlay since P4-M1),
   a console check deciding whether Explorer's look is matched. **The hydration check**: a console
   check in Jeremy's OneDrive, with the watch's lines read: a file made cloud-only through Explorer's
   "Free up space" and downloaded again through "Always keep on this device", and whether the folder's
   watch delivers a record for each (the attribute change) so the mark changes without a refresh. If
   it does not, the fix is a stat of the file on a `FILE_NOTIFY_CHANGE_ATTRIBUTES` record, which the
   watch asks for already; the record says what was found. Driver: `dragrow` out of a 32 px row with a
   thumbnail (cancelled, as every scripted drag is) for the drag image; a `search` with thumbnails on
   for the search tab; `[icons]` lines count the thumbnails asked for a search tab as the listing's.
   Tests: the drag image's icon choice, the state read's gate (which attribute combinations ask),
   the overlay list's size rule. Soak: the search tab with thumbnails on and the thumbnail drag in the
   P3-M5 fixture folder's copy; the report counts drag images from the thumbnail. Gate: the defaults
   triplet and the thumbnails triplet (`C:\traces\thumbs.toml`), since the drag image and the search
   tab change nothing the scenario touches but the icon workers' table does. Console checks: a photo
   dragged out of a 32 px list and out of a search tab onto the desktop (unelevated), the Google Drive
   tiles beside Explorer's, the OneDrive states on a folder set to "Free up space", the hydration
   check.

Each milestone brings its driver steps and stderr lines (in `CLAUDE.md` § Measuring), tests for its
maths, and its soak cycle, added to `tools/soak-p5m3.ps1`. Console checks after each triplet,
unelevated.

**The gate is re-run after every milestone** under the protocol agreed on 2026-10-12, unchanged:
`tools/wpr-triplet.ps1` elevated at the console by Jeremy, accessibility on, Narrator off, every
other TwinFM window closed, the split-and-scroll scenario on `C:\traces\bigdir`, clean binaries built
with `--locked` into their own `--target-dir target-<name>` from detached `git worktree add`
checkouts. Three runs per binary, interleaved, compared by median; a gap counts only when every
reference run sits below every candidate run or the medians differ by more than one point (one-sided:
a cost), each run's `[perf] ... render[... p50=...]` read first, and a fixed-order pair's gap of about
a point checked with the order swapped (P7-M4). The reference for P9-M1 is `target-p8m2` (db30820);
after each merge the new milestone's gate build becomes the next reference and the previous one is
deleted. Tags continue from ab37. Bar: main thread under 20% busy over the 10 s after the split,
judged against the reference's spread in the same session. Nothing else runs on the machine during a
gate (the wrapper refuses to start while `cargo`, `rustc` or a test binary runs; a Claude desktop
thread spinning a core slowed two Phase 8 gates, so the app's CPU is checked before a handover). One
gate at a time on this machine; Jeremy is asked before a wrapper is handed over, and the gate is one
PowerShell wrapper file per milestone (`gate-<milestone>.ps1`: the checks, the triplet, the analysis,
the summary), never bare commands.

**Settings triplets stay part of the protocol** (Phase 4's rule): each candidate's stderr is checked
for `[settings] file <toml> (read)` and the line the setting implies (`[cols] ... widths ...`,
`[preview] shown at ...`, `[icons] list geometry: 32 px rows ...`) before its triplet is read.

**P9-M1 is a list change and is settled against the gate before its plan hardens**: the brief's first
step is a spike of two binding shapes for the row widths (a pane property read by each row's cells,
and a per-pane list variant chosen once as the Type column's is, which would multiply the variants
and is the fallback) measured in-app back to back with the order swapped, then the triplet decides.
The lesson of P3-M1 (0.7 of a point for one per-row binding) is the reason this comes first.

The architecture rules from Phase 1 hold: only the UI thread touches a Slint model and workers wake
it with `slint::invoke_from_event_loop`; every COM object lives on the thread that made it; no
app-state `RefCell` borrow is held across a modal loop; no `unwrap` on `Weak<App>`; nothing per frame
or per row without a trace first. For the preview: every document is opened, parsed and decoded only
on the preview worker, under `catch_unwind` as PDFs are, the generation checked between pictures and
between an Office file's parts; the UI thread receives block lists and pixel buffers; a newer cursor
file supersedes the older at every step. Every crate added is pinned by exact version with its
licence noted in `Cargo.toml`, with default features off and only the needed ones on, and a crate is
not added where `zip`, `roxmltree`, `image` and `resvg` already do the job.

## Calls made in this kickoff, for Jeremy to check

Decisions Jeremy did not make and the lead settled, each reversible before its milestone starts:

- The column grips are 6 px wide on the boundaries between header cells and move the boundary under
  them, Explorer's rule (the left column grows by what the right one gives up); a column's floor is
  40 px, Name's 120 px; a double-click fits to the rows on screen, not the whole listing; the widths
  are logical px per pane, applied to every tab of the pane, saved under six keys with today's
  values as the defaults; they are requested widths, clipped on a pane too narrow for them as today's
  fixed columns are, nothing shrinking to fit; no "reset to defaults" item in Phase 9 (the keys can be
  deleted from the file). (The builder check asked all three.)
- Office parts are read with a streaming XML reader (`quick-xml`) rather than `roxmltree`, because
  reading must stop at 500 rows and at 64 MB of one part with a partial result, which a tree parser
  cannot give; the pin check names the crate. (The builder check.)
- The details list's drag image asks the icon worker for the 64 px thumbnail at the press rather
  than scaling the 32 px table entry; a JPEG's EXIF orientation is applied in the preview. (The
  builder check.)
- Plain text in the preview has no syntax colours in Phase 9 (wrapping is Jeremy's, above); the extension
  list is a constant, and an extension not on it (a `.dat`, say) is "No preview" rather than a guess
  at its bytes.
- Pictures are never scaled up past their own size; HEIC, HEIF and AVIF are "No preview" (the pure
  Rust decoders do not cover them; the shell's thumbnail cache does, at thumbnail size only).
- HTML is read into the Markdown block list, scripts and styles dropped, nothing executed; the
  parser crate is chosen by the pin check in the P9-M2 brief, a hand-written scanner the fallback.
- Office files are read with `zip` and `roxmltree` and no Office crate unless the pin check finds one
  that adds nothing beyond those two; Excel shows one sheet at a time (500 rows by 50 columns), the
  first by default, the others through links at the top; PowerPoint shows one block group per slide
  with a rule between; the binary `.doc`, `.xls`, `.ppt` are not previewed.
- Thumbnails inside archive levels stay off (the shell cannot see inside an archive without
  extracting) unless the P9-M4 pin check finds the zip folder's provider cheap; the record says what
  was found.
- The OneDrive state property is read only for files whose attributes say in progress or error,
  never for the tick and cloud rows.
- The Google Drive mark and the link arrow in the tiles take the overlay list by the mark's size,
  with a console check deciding whether it stays.

## P9-M1: adjustable column widths

Branch `claude/p9-m1-column-widths-0ab0c4`, from ab95aaf (the kickoff).

### Settled first (2026-10-21, before any code)

Checked against the code at ab95aaf and the pinned Slint (c1d3a8d):

- **Where the widths are read today.** The four details row components (`FileRows`, `FileRowsNoType`,
  `FileRows32`, `FileRowsNoType32`) give their Type, Size and Modified `Text`s the constants 120, 90
  and 140 px; the two details headers (`HeaderCell`s in a `HorizontalLayout`, chosen by `show-type`)
  the same; the rename editor's width subtracts 350 px (230 without Type). That is all. The kickoff
  named `dnd::Layout` (the name cell's extent for `hit`, the drop highlight and the band) as a reader
  too, but none of them reads a column today: `dnd::hit` returns the row for any x inside the list,
  the drop highlight spans the row, and the band runs in the grid only. So the widths reach Rust
  through a new pure function, `dnd::name_cell` (the name cell's x range in window px for a pane,
  from the pane's widths and row height, clamped at zero), which the drag's Name floor and the
  driver's aims use; `hit` keeps the whole row (a drop on the Type cell still lands on the row, as
  today and as in Explorer).
- **Header and rows sit 6 px apart**, as today: the header's cells end at the list's right edge, the
  rows' at its right edge less their 6 px right padding. The grips follow the header's boundaries.
- **The grips** are one overlay per pane over the header (like the rename editor and the drop
  highlight: siblings of the list, never in the rows), a 6 px `TouchArea` centred on each boundary
  with `mouse-cursor: ew-resize`, three with the Type column on, two with it off, none in the grid.
  A drag reports the pointer's travel since the press in window px (absolute position, so the grip
  following the boundary does not feed back into the delta) to Rust, which applies it to the widths
  at the press, clamps, and sets the pane's three properties; the list follows on the next frame.
  `double-clicked` is the fit.
- **A grip is named by the column on its right**: `type` is Name | Type, `size` is Type | Size (Name |
  Size with the Type column off), `modified` is Size | Modified. Dragging grip `c` by dx makes `c`
  narrower by dx and its left neighbour wider by dx (Name absorbing it for the first grip): the
  kickoff's rule, the boundary moves under the pointer. Floors: 40 px for a column, and the Name cell
  may not shrink below 120 px by a drag (below it already, on a narrow pane, it may not shrink
  further). `colwidth <pane> <col> <px>` sets column `c` to px through the same function, as a drag's
  end does. **The fit** of grip `c` fits column `c` itself (Type, Size or Modified) to the widest of
  its cells on screen plus 12 px (the header label included, so a fit never hides it), moving the
  same boundary, so the difference comes from the column on the grip's left (Name for `type`). The
  kickoff's wording, "fits the column to its left", would make Modified unfittable and Name the
  first grip's target; the driver's vocabulary (`colfit <pane> <type|size|modified>`) and the console
  check's "fit on a folder of long type names" read as this. A call for Jeremy to check.
- **Measuring the cells**: Slint has no text measurement in its Rust API, so one hidden `Text` per
  window takes the cells' strings joined by line breaks, and Rust reads its `preferred-width`, the
  widest line in the window's own font and scale (one layout of K short lines, K the rows on screen,
  at a double-click only).
- **Settings**: six `f32` keys, `col_type_left`, `col_size_left`, `col_modified_left` and the
  `_right` three, defaults 120, 90, 140; `Config::clamped` pulls each into 40 to 2,000 px (a non-finite
  value takes its default). Applied to the pane's properties at start; a scripted run reads none,
  except through `TWINFM_CONFIG`.
- **The binding shape is settled by the spike below before the rest is written**: shape A, the
  details list components take `type-w`, `size-w` and `modified-w` as `in` properties from the
  host (from the pane, from the window's six properties set from Rust), and each row's three `Text`s
  read `root.type-w` and the rest; shape B, per-pane copies of the list components each reading a
  `global` of its own pane's widths directly (`ColsLeft`, `ColsRight`), the host choosing the copy once
  per pane as it chooses by `show-type`, which doubles the details components from eight to sixteen.
  The spike builds each shape for the gate's variant only (22 px rows with the Type column), widths
  set from Rust to the defaults, and measures the gate scenario in-app against the unchanged build,
  in both orders.

Driver steps and lines, as the kickoff names them:

- `colwidth <pane> <type|size|modified> <px>`, `coldrag <pane> <type|size|modified> <dx>` (press on
  the grip's centre, moves in 8 px steps through Slint's pointer path, release), `colfit <pane>
  <type|size|modified>` (a double-click on the grip through the same path).
- `[cols] pane=N <col> W -> W2 px (drag | fit | script | config)` for each column that changed (a
  drag of the `size` grip changes Type and Size: two lines; logged at the release, not per move;
  `config` at start for a value other than the default), `[cols] pane=N fit <col>: widest of K rows
  on screen = W px`, and `[cols] pane=N widths type T size S modified M (name N px)` once after each
  change and once per pane at start (N the name cell's width in the pane as laid out then).

### The spike (2026-10-21)

Three builds of ab95aaf in the worktree's own `target`, each copied out before the next: the
unchanged build (base), shape A and shape B, each for the gate's variant only (`FileRows`, 22 px rows
with the Type column), the widths set from Rust to 120, 90, 140 at start. The gate scenario on
`C:\traces\bigdir` in-app, twelve runs back to back in the order base, A, B, B, A, base, twice.
Jeremy's console session was connected (`query session`: console Active) and every run presented at
60 Hz (interval p50 16.39 to 16.48 ms) with render p50 2.46 to 2.55 ms, so one session state
throughout. `[busy] 10 s after split`:

| Build | Runs | Median |
|---|---|---|
| base | 18.0 / 22.5 / 18.4 / 18.7 | 18.55 |
| A, the list's properties | 17.7 / 19.1 / 16.9 / 19.8 | 18.4 |
| B, a global per pane | 17.7 / 18.1 / 16.6 / 21.2 | 17.9 |

No shape costs anything the in-app runs can see (each build's spread is two to five points, its own
noise). **Shape A is chosen**: the same cost as B, with four details components instead of eight
(B's per-pane copies would double them again with every future row change), and no global per
pane. The console triplet decides as usual. Caveat: the runs (10:54 to 10:58) went ahead before the
coordinator's hold on the preview stream's builds reached it, so that stream may have been compiling
during some of them; the run script checked for `cargo` and `rustc` processes before each run and
found none, but a compile starting mid-run would not show there. The spreads are the usual ones, so
the choice stands. Narrator was not running (checked afterwards, not at the time). Nothing of the spike was kept: shape A was written again
for all four row variants. Unlike P3-M1's 0.7 of a point, this binding is a value read when a row is
made and never changes while the lists scroll; P3-M1's switch decided what each row held.

### What landed

- **`src/cols.rs`** (new, pure, four tests): `Col` (type, size, modified; a grip named by the column
  on its right), `Widths` with `clamped` (40 to 2,000 px, a non-finite value its default) and
  `shown` (what the columns take from a row, with or without Type), `left_of` (the grip's left
  neighbour, None for Name), `drag` (the boundary moved from the widths at the press, stopping at a
  column's 40 px floor, at 2,000 px, and at Name's 120 px, or at Name's own width when it is already
  below that), `set_width` (a width set through the same boundary: `colwidth` and the fit),
  `fit_width` (the widest cell plus 12 px), `grip_x` and `grips`.
- **`dnd::name_cell`** (one test, five layouts): the name cell's x range in window px from the pane,
  the row height and the columns shown, clamped at zero. The drag's Name floor reads it, and so does
  the `[cols] ... widths` line's name figure. `hit` is unchanged (a drop on the Type cell lands on
  the row).
- **Settings**: `col_type_left`, `col_size_left`, `col_modified_left` and the `_right` three
  (`Config::cols`, `set_cols`), defaults 120, 90, 140, clamped on load; one round-trip test (the
  defaults without the keys, a write and read back, out of range values pulled in).
- **pane.slint**: the four details row components take `type-w`, `size-w` and `modified-w` (the
  NoType two without `type-w`) from `FileListHost`, which takes them from the pane's `col-type`,
  `col-size` and `col-modified`; the two details headers' cells and the rename editor's width read
  the same three. `ColGrip`: a 6 px `TouchArea` with the east-west cursor, reporting the pointer's
  travel since the press from its absolute position (so the grip moving with the boundary does not
  feed back), phases 0, 1, 2 for press, move and release, and the double-click; the pane draws three
  (two without Type) in one overlay over the header, none in the grid.
- **app.slint**: the six window properties, `col-drag(pane, col, dx, phase)`, `col-fit(pane, col)`,
  and `measure(text, bold)`: two hidden `Text`s (the lists' font, and the header's bold) whose
  preferred width Rust reads at once, the widest line of the cells joined by line breaks.
- **main.rs**: `cols_of`, `set_cols`, `name_px`, `col_drag` (the widths and name cell at the press
  kept in a thread-local, cleared at the release), `col_set`, `col_fit` (the rows in the viewport from
  the list's scroll and height, never the listing; the double-click's own press and release are not
  a drag), `cols_changed` (the lines, the config, `schedule_save`); at start the config's widths
  applied with their `config` lines, and 400 ms later the widths line for both panes.
- **Driver** (`src/auto.rs`): `colwidth`, `coldrag`, `colfit` as Settled first names them.
- **Soak**: `tools/soak-p5m3.ps1` adds the column cycle after P8-L1's (a drag of the `size` grip by
  30 px, a fit of Modified, the widths set back) and its report line.
- **`C:\traces\cols.toml`** (read-only, not in the repo): both panes 200, 110, 180.
- **Tests**: 249 to 255.

### Decisions

- **The grip is named by the column on its right, and a fit fits that column** (Settled first): the
  kickoff's "fits the column to its left" would leave Modified unfittable. Jeremy, 2026-10-21: "Keep
  the fit rule as built: a double-click fits the column on the boundary's right (Type, Size or
  Modified)."
- **A fit includes the header label** (bold), so a column of empty cells (folders' sizes) fits to
  "Size" plus 12 px rather than to nothing; a fit never goes under 40 px.
- **The drag's travel is measured from the press**, not move by move (the sidebar's resize edge adds
  each move's delta): a pointer that overshoots a floor and comes back meets the boundary where it
  stopped instead of dragging it early.
- **A drag is logged at the release only**, each column that changed (two lines for a grip between
  two columns), so a drag of 200 moves writes three lines.
- **Widths are kept as the drag leaves them** (fractional at 150% scaling); the lines print whole px.
- **The start line waits 400 ms** for the window's size: logged straight away, the name cell read
  0 px, before the first layout. The right pane's figure before a split is its would-be width.
- **A double-click's own release must not undo the fit**: the second press records the widths, the
  fit changes them, and the release then put them back (seen in the first scripted run as a `(drag)`
  line after each `(fit)`); the fit now clears the drag in progress.

### Scripted checks

All on this machine, `target\release` of the branch, Jeremy's console session.

On `C:\Windows\System32` (folders first, so the rows on screen are folders): `colwidth 0 type 200`,
`size 110`, `modified 180` with the default 1,200 px window, then drags and fits:

```
[cols] pane=0 widths type 120 size 90 modified 140 (name 626 px)
[cols] pane=0 type 120 -> 180 px (drag)                        coldrag 0 type -60
[cols] pane=0 fit type: widest of 31 rows on screen = 53 px     colfit 0 type ("File folder")
[cols] pane=0 type 180 -> 65 px (fit)
[cols] pane=0 fit modified: widest of 31 rows on screen = 94 px
[cols] pane=0 size 90 -> 124 px (fit)
[cols] pane=0 modified 140 -> 106 px (fit)
[cols] pane=0 size 114 -> 180 px (drag)                        coldrag 0 modified 500: Modified at 40
[cols] pane=0 modified 106 -> 40 px (drag)
[cols] pane=0 size 180 -> 40 px (drag)                         coldrag 0 modified -2000: Size at 40
[cols] pane=0 modified 40 -> 180 px (drag)
[cols] pane=1 fit size: widest of 31 rows on screen = 4 px      folders: no sizes; "Size" + 12 = 40
[cols] pane=1 type 120 -> 40 px (script)                        colwidth 1 type 5: the floor
```

With the Type column off (the dialog), `coldrag 0 size -30` moved Name | Size (Size 40 -> 70, the
name cell 245 -> 215 px) and `colwidth 0 type 150` logged `[auto] col pane=0 type: no such grip in
this pane's header`; with thumbnails on (32 px rows, `[icons] list geometry: 32 px rows`) a drag and
a fit ran the same, and in the grid (`view icons`) the grips are refused. Screenshots with the
software renderer (`PrintWindow`, the window found by this build's path, since the preview stream's
TwinFM was open beside it): after the widths change the header cells and every row's cells stand
6 px apart as before; in a split the left pane, already narrower than Name's 120 px, kept Type
from growing; the rename editor (F2) ends at the moved Type boundary; `coldrag 0 type 200` (Name |
Type right) stopped Type at 40.

The settings triplet's file: `C:\traces\cols.toml` on the gate scenario shows `[settings] file
C:\traces\cols.toml (read)`, six `(config)` lines and `[cols] pane=0 widths type 200 size 110
modified 180 (name 486 px)` for both panes, and `could not save ... Access is denied` as every
read-only test config does.

A six-minute run of `tools/soak-p5m3.ps1` (three cycles started, two completed): exit code 0, no
panicked line, every earlier count as before (jobs done 147, failed 0; menupicks refused 0; drags
dropped 0; zip jobs not done 0; waitpreview timeouts 0). Its column cycle navigated to a folder that
does not exist (a tab character had replaced the path's backslash in the script), so the drag ran
on an empty listing and the fit saw no rows (`widest of 0 rows on screen = 4 px`); the drag read back
as Size 90 -> 60 and the widths went back to 120, 90, 140 in both cycles. Path fixed in 797617a.

The re-run, six minutes after P9-M2's gate (three cycles started, two completed): exit code 0, no
panicked line, jobs done 192 (failed 0, cancelled 0), drags dropped 0, toolbar presses refused 0,
menupicks refused 0, zip jobs not done 0, waitpreview timeouts 0, worker panics 0; working set 126.5
to 212.7 MB (233 at most). The column cycle in `src\tree`: `[cols] pane=0 size 90 -> 60 px (drag)`
in both cycles, `fit modified: widest of 10 rows on screen = 94 px` with Size 60 -> 94 and Modified
140 -> 106, then the widths back at 120, 90, 140 (two cycles, and the start line).

### Gate

Run by Jeremy on 2026-10-21, finished at 13:28 (tag ab38 for both settings, `C:\traces\gate-p9m1.ps1`,
the same file as `tools/gate-p9m1.ps1`, 17 min): `target-p9m1` (b9fbd9d, main with P9-M2 merged in,
built `--locked` from a detached checkout) against `target-p9m2` (95ace9f), three rounds of
reference, candidate, reference with `C:\traces\cols.toml`, candidate with it. Every run passed the
wrapper's checks (no panic; `[settings] file C:\traces\cols.toml (read)` in the cols runs; the
candidate's `[cols] pane=0 widths` line at 120, 90, 140 and at 200, 110, 180).

| Run | WPR | In-app | Render p50 (ms) |
|---|---|---|---|
| p9m2-1 / 2 / 3 | 17.7 / 23.3 / 21.1 | 16.4 / 25.8 / 22.5 | 2.23 / 3.15 / 2.91 |
| p9m1-1 / 2 / 3 | 18.4 / 21.7 / 21.9 | 16.4 / 20.9 / 21.4 | 2.23 / 2.96 / 2.92 |
| p9m2-cols-1 / 2 / 3 | 18.5 / 21.4 / 21.2 | 20.2 / 18.6 / 19.2 | 2.25 / 2.92 / 2.93 |
| p9m1-cols-1 / 2 / 3 | 17.7 / 21.0 / 20.9 | 17.2 / 23.0 / 22.0 | 2.09 / 2.84 / 2.80 |

The session changed state after round 1 (the ab23 state): every run of rounds 2 and 3 had render p50
2.80 to 3.15 ms against 2.09 to 2.25 in round 1, and read 21 to 23% whichever binary sat in it, the
reference above 20% too. Medians as the wrapper reads them: defaults 21.1 (reference) / 21.7
(candidate), cols 21.2 / 20.9; no gap that counts in either. Read like for like: round 1, the normal
state, defaults 17.7 / 18.4 and cols 18.5 / 17.7, every run under 20%, the two pairs within 0.8 of a
point in opposite directions; rounds 2 and 3, the slow state, defaults 23.3, 21.1 against 21.7, 21.9
and cols 21.4, 21.2 against 21.0, 20.9, the candidate below the reference in three of four pairs.
Passed by the protocol's rule: the per-row width binding adds no cost the gate can see. Only one
round ran in the normal state, so the evidence there is one pair per setting (as in ab23, which
Jeremy accepted read this way).

### Console checks (for Jeremy, unelevated, after the gate)

With `C:\Users\jtkli\projects\twinfm\target-p9m1\release\twinfm.exe`, started from a normal (not
elevated) window, the panes set up unequal (the split on, two tabs in one pane only):

1. **The cursor**: over each boundary in the header (Name | Type, Type | Size, Size | Modified) the
   pointer turns into the left-right arrows; none at the right edge of the list, none in the grid views.
2. **A drag in each pane**: drag Type | Size right and left; the list follows the pointer as it moves,
   the boundary stays under the pointer, and the other pane does not change. Header and rows stay
   lined up.
3. **The floors**: drag a boundary as far as it goes each way; no column goes below about 40 px and
   the Name column stops at about 120 px (it never vanishes).
4. **The fit**: in a folder of long type names (`C:\Windows\System32` scrolled to its files, or a
   Downloads folder of mixed files), double-click the Name | Type boundary: Type fits its longest
   name on screen. Double-click Size | Modified: Modified fits its dates.
5. **The Type column off** (Settings, Type column unticked): two grips, Name | Size and Size |
   Modified, and a drag works as before; ticked again, the Type column comes back at its width.
6. **Thumbnails on** (Settings): the 32 px rows follow a drag the same way.
7. **150% scaling** (Display settings, Scale 150%, TwinFM restarted): a drag and a fit still line up.
8. **Rename** (F2) after a drag: the edit box ends where the Type column starts.
9. **A restart**: close TwinFM with the window's close button and start it again; each pane's widths
   are as you left them (in `%APPDATA%\twinfm\config.toml` as `col_type_left` and the rest).
10. **The fit's rule**: settled by Jeremy on 2026-10-21 (Decisions): kept as built.

## Open questions

Written 2026-10-21 at the kickoff.

- **The cost of a per-row width binding**: whether a pane property read by every row's cells holds
  the gate, or the per-pane variant fallback is needed (P9-M1's spike decides).
- **Which HTML parser pins cleanly**: `html5ever` brings Servo's string cache and tendril crates;
  `tl` is one crate; the P9-M2 brief's pin check decides.
- **Office files Jeremy actually has**: whether his Word files use heading styles (so headings show)
  or direct formatting (so they show as paragraphs), and what his workbooks' number formats are;
  the P9-M3 console check is the measure.
- **The hydration check**: never run; P9-M4 runs it.
- **Carried from Phase 7 and Phase 8**: Google Drive's volume found by its name; a sync-root scratch
  folder for the soak; the Slint whole-row snap patch and upstreaming the two accessibility patches;
  the dead share console check owed since P5-M1; the preview's deferred items (zoom, selection,
  password PDFs, the Markdown details).

## P9-M2: text, pictures and HTML in the preview

Branch `claude/p9-m2-preview-text-pictures-html-f01bd4`, from ab95aaf (the kickoff).

### Settled first (2026-10-21, before any code)

**The pin check** (each crate added to a scratch copy of the manifest with default features off,
`Cargo.lock` compared, then put back):

- **`html5ever =0.40.1`** (MIT OR Apache-2.0): 12 new crates, 125 lines of `Cargo.lock`:
  `markup5ever`, `web_atoms`, `string_cache` and `string_cache_codegen`, `tendril`, `phf`,
  `phf_shared`, `phf_generator` and `phf_codegen` at 0.14 (a second `phf` beside the 0.13 Slint
  builds), `precomputed-hash`, `new_debug_unreachable`; its atom tables are generated at build time,
  and it has no tree of its own (a `TreeSink` would have to be written). Too much for one preview
  type.
- **`tl =0.7.8`** (MIT): one crate, 7 lines. But it has no raw-text state for `<script>` and
  `<style>` (a `<` inside a script reads as a tag, so script text leaks into the page), decodes no
  entities, and has had no release since 2023 (its inline vectors are built on `unsafe`). It does
  not do the two jobs a parser was wanted for.
- **So the fallback: a hand-written scanner**, `src/html.rs`, pure and tested: a tokenizer (text,
  start and end tags with quoted or bare attributes, comments, `<!DOCTYPE>` and `<?...>` and
  `<![CDATA[...]]>` dropped, raw text up to the matching end tag for `script`, `style`, `textarea`,
  `title`, `xmp`, `iframe`, `noembed`, `noframes`, `noscript`), the named entities of Latin-1
  (`&nbsp;` to `&yuml;`, 96) plus the common typographic ones (`&mdash;`, `&hellip;`, the quotes,
  `&euro;`, `&trade;`, arrows and a few more) and every numeric one, then the mapping below straight
  into `markdown::Block`s. No crate added for HTML. The entity table replaces the seven entities of
  `markdown::entities`, so Markdown's HTML blocks gain them too.
- **`image =0.25.10` features**: `bmp` and `ico` add nothing to `Cargo.lock` (`ico` needs `bmp` and
  `png`, both in); `tiff` adds two crates, `tiff 0.11.3` (MIT) and `fax 0.2.7` (MIT), its `flate2`,
  `weezl`, `half`, `quick-error` and `zune-jpeg` already being in. All three are taken.
- **EXIF orientation**: `image` reads it itself (`ImageDecoder::orientation`, from the JPEG's, PNG's,
  WebP's or TIFF's EXIF chunk by `metadata::Orientation::from_exif_chunk`; no `exif` crate) and
  `DynamicImage::apply_orientation` turns the pixels. `ImageReader::decode` does not apply it, so the
  picture path becomes `into_decoder`, `orientation`, `DynamicImage::from_decoder`,
  `apply_orientation`. It is applied in the one decoding function, so Markdown's and HTML's pictures
  show upright too (as a browser shows them).

**Checked against Slint (c1d3a8d)**: a `Text` with `wrap: word-wrap` breaks a word longer than the
line anywhere (`linebreaker.rs` falls back to `break_anywhere`; the parley path sets `OverflowWrap`
anywhere), so a minified JSON line wraps in the code block as Jeremy chose, with no new element.
Nothing else new is drawn: text is `Code` blocks, a picture an `Image` block and a `Note` block, HTML
the blocks Markdown draws.

**The extension lists** (`preview.rs`, any case; anything else is "No preview"):

- `TEXT_EXTS`: the plan's `txt log ini cfg csv json xml yaml yml toml rs py ps1 sh c h cpp cs js ts
  css slint bat cmd`, and `tsv conf reg inf diff patch psm1 psd1 hpp cc go java sql jsx tsx lock`
  (40 in all; `.reg` files are UTF-16, the BOM decides). A file with no extension (`Makefile`,
  `LICENSE`) is "No preview", the kickoff's call.
- `PICTURE_EXTS`: `jpg jpeg jpe jfif png gif webp bmp tif tiff ico svg svgz` (`.jfif` is the name
  browsers save JPEGs under; `.svgz` is resvg's gzip). HEIC, HEIF and AVIF are not on it.
- `HTML_EXTS`: `html htm`.

**Plain text**: read at most 2 MB + 1 byte as Markdown is, `markdown::decode` (UTF-8 or UTF-16 BOM,
else UTF-8 with invalid bytes replaced, cut at the last line break before 2 MB with the note first),
tabs as four spaces, `\r\n` and `\r` as line ends, then `Code` blocks of at most 40 lines. **One
addition**: a chunk also ends before it passes 8 KB (`TEXT_CHUNK_BYTES`), and a single line over 8 KB
is split at a character boundary into 8 KB pieces, each its own block drawn as one. A minified 2 MB
JSON is one line, and one `Text` of 2 MB would be shaped whole on the UI thread at every width
change; the split's only visible trace is one early line break per 8 KB of a line already wrapped.
The encoding for the line: `UTF-8`, `UTF-8 BOM`, `UTF-16LE`, `UTF-16BE`, and `UTF-8 (invalid bytes
replaced)` when the shown part is not valid UTF-8.

**Pictures**: the shared decoder (attributes first: missing or cloud-only never opened; the format
sniffed from the bytes, the extension only choosing the SVG path) with the orientation applied. A
lone picture is not held to "over the budget means alt text": its decode is allowed up to 512 MB of
pixels (`DECODE_MAX`, `image`'s own default limit; a 48-megapixel photo is 192 MB), and the result is
scaled down so that neither side passes 8,192 px and its pixels fit the 64 MB budget
(`fit_px`: the factor is the smaller of 1, 8192 over the longer side and the square root of the
budget over the area; never above 1, so never scaled up). A 10,000 x 1,000 panorama becomes 8,192 x
819 (the side decides); a 10,000 x 3,000 one 7,478 x 2,243 and a 48-megapixel photo 4,729 x 3,547 (the
budget decides). **Never scaled up in the
column** is read in physical pixels: the block's width is the picture's pixels divided by the
window's scale factor (a 100 px icon at 150% shows 100 screen px, not 150), and wider ones fit the
column as Markdown's do. (Markdown's and HTML's pictures keep the browser's rule, P8-M2's.) The
`Note` under it: `1920 x 1080, 2.3 MB, JPEG` (the upright size before any scaling; the file size in
bytes under 1 KB, KB under 1 MB, else MB with one decimal, 1024-based; the format from the bytes:
JPEG, PNG, GIF, WebP, BMP, TIFF, ICO, SVG). A file the decoder refuses, or over 512 MB of pixels:
"This picture could not be read". A GIF shows its first frame.

**HTML, the mapping** (the file decoded as text is, the 2 MB cut and note as for Markdown; a
`<meta charset>` is not read):

- Dropped with everything inside: `script`, `style`, `head`, `nav`, `noscript`, comments (the plan's),
  and `title`, `template`, `svg` (a page may omit `<head>`, so its `<title>` would read as text; a
  template is never rendered; an inline SVG's `<title>` and `<text>` are drawn by a browser as a
  picture, not as words).
- `h1` to `h6`: a heading of that level (bold through `**`, as Markdown's).
- Paragraph breaks: `p`, `div` and every other block container (`section`, `article`, `main`,
  `header`, `footer`, `aside`, `figure`, `figcaption`, `address`, `details`, `summary`, `dl`, `dt`,
  `dd`, `form`, `fieldset`, `center`, `body`, `caption`) end the paragraph being built at their start
  and end; text between becomes a paragraph, and one with no text (an empty `div`) none. `br` is a
  hard break.
- `ul` and `ol` open a list level (bullets by depth as Markdown's; `ol` numbered from its `start`,
  default 1); `li` starts an item (its marker on its first block; an `li` before the previous one's
  `</li>` simply starts the next, as the parser's implied end does). Nested lists keep their depth.
  An `li` outside any list is a plain paragraph.
- `blockquote`: one quote level.
- `pre`: a code block of its text, whitespace kept (a newline right after `<pre>` dropped, `<br>` a
  newline, tags inside ignored, entities decoded), chunked as Markdown's code is.
- `hr`: a rule.
- `table`, `tr`, `th`, `td`: a table, chunked as Markdown's; a row whose cells are all `th` (or one
  in `thead`) is a head row; a new `td` or `tr` ends the cell or row before it; `colspan`, `rowspan`
  and alignment are ignored (every column left-aligned); a block element or `br` inside a cell is a
  line break in the cell; a table inside a cell flows its text into that cell; text outside any cell
  (a browser's foster parenting) becomes a paragraph before the table; `caption` a paragraph before it.
- `img`: a local `src` (resolved against the file's folder, as Markdown's) a picture block after the
  current block; a remote one (a scheme, `data:`, `//host`) its `alt` text where it stands.
- Inline markup: `a href` a link, `strong` and `b` bold, `em` and `i` italic, `code`, `kbd`, `samp`
  and `tt` a code span; every other element's text flows into the enclosing block. Emphasis runs keep
  their spaces outside the delimiters (`<b> x </b>` is ` **x** `, which Markdown reads as bold), an
  empty run writes nothing, and a run left open when its block ends is closed there.
- Whitespace collapsed as a browser does (a run of spaces, tabs and newlines one space, none at a
  block's start); `&nbsp;` kept.
- Anchors: an element's `id` (and `<a name>`) on the block it starts, so `#id` links scroll to it;
  a heading with no id gets its Markdown slug as before.

**Seams**: `Class` gains `Text`, `Picture` and `Html` (type before place, as before: only these and
PDF and Markdown ask the archive and cloud questions). The worker's Markdown path becomes one
`serve_blocks` for Markdown, text and HTML (read, decode, parse under `catch_unwind`, the note, the
`StyledText`s, the blocks answered, then the pictures one at a time with the generation checked
between them); a picture file is answered as two blocks (the picture, the note) and its pixels, the
`Picture` reply carrying whether its size is physical. No new driver step: `waitpreview` waits for the
blocks and the picture as it does for Markdown.

**The `[preview]` lines** (beside P8-M2's):

- `pane=N gen=G <path>: text, L lines, B bytes, <encoding>[, cut at 2 MB]` (L the lines shown, B the
  file's size)
- `pane=N gen=G <path>: picture WxH px, <format>, decoded in T ms[, scaled to WxH]`
- `pane=N gen=G <path>: picture failed: <why>` (the column reads "This picture could not be read")
- `pane=N gen=G <path>: html, B blocks (H headings, C code, T tables, I images), parsed in T ms[,
  cut at 2 MB]`
- `unreadable: <why>` as for Markdown, for a text or HTML file that cannot be read.

### What landed

- **`src/html.rs`** (new, pure, four tests): the tokenizer (`tokens`: comments, `<!...>`, `<?...>` and
  CDATA dropped, quoted attributes with `>` inside, raw text up to `</name` for the raw-text
  elements, a `<` that starts no tag kept as text), `attr` (quoted, bare and empty values, entities
  decoded) and the mapping as Settled first lists it, built on Markdown's `Inline` writer (made
  `pub(crate)`) through an `Ink` that collapses whitespace and writes emphasis and links only around
  content. Ids go to the block they start (`Block::anchor`).
- **`src/markdown.rs`**: `text_blocks` (the text chunks), `encoding` (the name for the line),
  `table_chunks` (Markdown's table chunking as a function, so HTML's tables are chunked the same
  way), the entity table (`LATIN1`, `NAMED`, `entity`) behind `entities`, and `Block::anchor`.
- **`src/preview.rs`**: `Class::Text`, `Picture`, `Html` with `TEXT_EXTS`, `PICTURE_EXTS`,
  `HTML_EXTS`; `serve_md` became `serve_blocks` for the three block types, with `shown_blocks` and
  `decode_pictures` split out of it; `serve_picture`, `lone_picture`, `fit_px`, `size_text`;
  `decode_picture` (shared by documents and picture files) applies the EXIF orientation;
  `Reply::Markdown` is `Reply::Blocks`, `Reply::Picture` carries `physical`; an anchor matches any of
  a block's space-separated anchors; an empty document gets a note. The UI side changed in one place
  (the picture's size divided by the scale factor when `physical`); `preview.slint` is unchanged.
- **`Cargo.toml`**: `image` gains `bmp`, `ico` and `tiff` (`tiff 0.11.3` and `fax 0.2.7` in
  `Cargo.lock`, both MIT; 21 lines). `main.rs` gains one line, `mod html;`.
- **Tests**: 249 to 259: four in `html.rs` (the tokenizer; a page with every mapped element, a
  script, a style, a head, a nav, a noscript and comments; runs and links across blocks; odd pages:
  script only, no head, a stray `li`, a nested table, an unclosed `pre`, text outside the cells,
  anchors), three in `markdown.rs` (named entities; the text chunking with a 20,000-byte line and a
  character across the cut; the encodings), three in `preview.rs` (the fit rule and the size text; a
  JPEG with EXIF orientation 6 shown upright, a BMP, a broken file; the worker on text, UTF-16, a
  file over 2 MB, a JPEG, a broken PNG, a page, a script-only page and an empty file), and the
  classification test covers the three types.
- **Soak**: `tools/soak-p5m3.ps1`'s preview cycle goes on through `h-notes.txt`, `i-photo.jpg` and
  `j-page.html` (its `#end` followed) and reports text, picture, picture-failure and HTML counts.

### Decisions

- **No HTML parser crate** (the pin check above): the fallback scanner, which the tests hold to the
  mapping. What it does not do that a browser does: `<meta charset>` (a page saved in Windows-1252
  shows its accented letters replaced), the full named-entity table (2,231 names; the ones not on the
  list stay as written), implied end tags beyond the list, cell and paragraph breaks, `colspan`, CSS.
- **Plain text chunks also end at 8 KB**, a longer line split (Settled first). A 300 KB minified
  JSON became 41 blocks and showed at once; its wrapping breaks after commas, as word wrap does.
- **A picture file is held to the 64 MB budget by scaling, not refused**: a 10,000 x 3,000 PNG decoded
  in 804 ms and showed at 7,478 x 2,243 (`scaled to` in its line). Only over 512 MB of pixels while
  decoding is it "This picture could not be read".
- **A picture file's size counts in screen pixels** (never drawn larger than itself at 150%); a
  document's pictures keep the browser's logical pixels.
- **An empty file or a page with no text shows a note** ("This file is empty.", "This page has no
  text to show.") rather than a blank column. Found by the scripted check's script-only page; the
  note counts as the one block in the `[preview]` line.
- **Dropped beyond the plan's list**: `title` (a page without `<head>` would show it), `template`
  (never rendered), `svg` (an inline icon's `<title>` is no text a reader sees), `iframe`, `noembed`,
  `noframes` (their content is fallback a browser does not show).
- **An `li` outside any list is a plain paragraph**; a local picture inside a table cell is its alt
  text (a cell has no place for a picture block); text between a table's cells goes before the table.
- **A heading's anchors are its ids and its slug**, so `#id` and Markdown-style `#slug` links both
  reach it.

### Scripted checks

All on this machine (RDP session), `target\release` of the branch, over `%TEMP%\twinfm-p9m2-check\`
(written by a scratch script: this branch's `preview.rs`, a 2.3 MB log, a UTF-16 file, a 300 KB
one-line JSON, a JPEG stored landscape with EXIF orientation 6, a 10,000 x 3,000 PNG, a GIF, an SVG
with text, a TIFF, a BMP, an icon, a broken JPEG, a page of nested lists and tables with a local and
a remote picture, a script-only page, a `.dat` and the repo's README), the column at 0.45, the cursor
through every file with `waitpreview`, links followed in the page, then four Up presses (three
without a wait, one 20 ms later):

```
[preview] pane=0 gen=2 ...\a-main.rs: text, 1776 lines, 82541 bytes, UTF-8
[preview] gen=2 45 blocks in the list in 0.2 ms (UI thread)
[preview] pane=0 gen=3 ...\b-big.log: text, 27740 lines, 2400066 bytes, UTF-8, cut at 2 MB
[preview] gen=3 695 blocks in the list in 3.6 ms (UI thread)
[preview] pane=0 gen=4 ...\c-utf16.txt: text, 3 lines, 138 bytes, UTF-16LE
[preview] pane=0 gen=5 ...\d-minified.json: text, 1 lines, 327784 bytes, UTF-8
[preview] gen=5 41 blocks in the list in 0.5 ms (UI thread)
[preview] pane=0 gen=6 ...\e-phone.jpg: picture 1200x1600 px, JPEG, decoded in 10 ms
[preview] pane=0 gen=7 ...\f-pano.png: picture 10000x3000 px, PNG, decoded in 804 ms, scaled to 7478x2243
[preview] pane=0 gen=8 ...\g-anim.gif: picture 120x80 px, GIF, decoded in 1 ms
[preview] pane=0 gen=9 ...\h-logo.svg: picture 300x120 px, SVG, decoded in 1 ms
[preview] pane=0 gen=10 ...\i-scan.tif: picture 400x300 px, TIFF, decoded in 2 ms
[preview] pane=0 gen=11 ...\j-small.bmp: picture 64x48 px, BMP, decoded in 0 ms
[preview] pane=0 gen=12 ...\k-icon.ico: picture 64x64 px, ICO, decoded in 0 ms
[preview] pane=0 gen=13 ...\l-broken.jpg: picture failed: failed: Format error decoding Jpeg: I/O errors Not enough bytes, expected 5 but found 1
[preview] pane=0 gen=14 ...\m-page.html: html, 20 blocks (2 headings, 1 code, 1 tables, 1 images), parsed in 1 ms
[preview] gen=14 image https://example.com/x.png: alt text (remote)
[preview] gen=14 image 16 img/dot.png: 200x60 px in 0 ms
[preview] link "#bottom": anchor -> block 18
[preview] link "#top": anchor -> block 0
[preview] link "https://example.com/": browser (scripted: not launched)
[preview] link "img/dot.png": open ...\img\dot.png (scripted: not launched)
[preview] link "#nope": refused: no heading #nope
[preview] pane=0 gen=15 ...\n-script.html: html, 0 blocks (0 headings, 0 code, 0 tables, 0 images), parsed in 0 ms
[preview] pane=0 gen=16 ...\o-data.dat: no preview for this type
[preview] pane=0 gen=17 ...\p-readme.md: markdown, 33 blocks (7 headings, 3 code, 1 tables, 0 images), parsed in 11 ms
[preview] pane=0 gen=18 ...\l-broken.jpg: picture failed: ...
```

Every `waitpreview` settled; exit code 0, no panic. The four Up presses opened one generation (gen
18). The script-only page's blank column (gen 15) led to the empty-document note (Decisions); the
README's blocks are as P8-M2 recorded them. Screenshots with the software renderer (`PrintWindow`),
the column at 0.45: `preview.rs` in Consolas with its long comment lines wrapped; the JSON wrapped at
its commas; the photo upright (portrait, the red half on top) with `1200 x 1600, 35 KB, JPEG` under
it; the page with its heading, inline styles and links, the link spanning two blocks as two linked
paragraphs, the nested bullets and the numbers from 5, the two quote levels, the caption above the
table with its bold head row, the cell's two lines and the inner table's text in its cell, the code
block with its tab as four spaces and `&amp;` decoded, the local picture and the remote one's alt
text, the rule and the last heading.

A six-minute run of `tools/soak-p5m3.ps1` (three cycles started, the third stopped by the clock before its
preview cycle): exit code 0, no panicked line, every other count as before; preview: PDFs opened 4,
markdown 4, text 4, pictures 4, picture failures 0, html 2, pictures shown 19,078 (g-big.md's repeats),
alt text 0, fall-backs 0, cut at 2 MB 2, superseded 0, waitpreview timeouts 0, worker panics 0, no
preview for the type 0; working set 126 to 220 MB.

### Gate

Run by Jeremy on 2026-10-21 at 11:36 (tag ab37 for both settings, `C:\traces\gate-p9m2.ps1`, the
same file as `tools/gate-p9m2.ps1`, 17 min): `target-p9m2` (95ace9f, built `--locked` from a detached
checkout) against `target-p8m2` (db30820), three rounds of reference, candidate, reference with
`C:\traces\preview.toml`, candidate with it. Every run passed the wrapper's checks (no panic,
`[settings] file C:\traces\preview.toml (read)` and `[preview] shown at` in the preview runs).

| Run | WPR | In-app | Render p50 (ms) |
|---|---|---|---|
| p8m2-1 / 2 / 3 | 17.8 / 17.5 / 17.6 | 18.7 / 17.7 / 19.1 | 2.21 / 2.19 / 2.19 |
| p9m2-1 / 2 / 3 | 18.0 / 17.6 / 21.7 | 19.4 / 16.1 / 20.2 | 2.19 / 2.18 / 2.90 |
| p8m2-preview-1 / 2 / 3 | 17.1 / 16.7 / 20.1 | 18.3 / 17.0 / 17.8 | 2.06 / 2.04 / 2.71 |
| p9m2-preview-1 / 2 / 3 | 16.9 / 17.1 / 20.2 | 17.8 / 15.3 / 21.7 | 2.05 / 2.01 / 2.73 |

The session changed state in round 3, from its second run on (render p50 2.71 to 2.90 ms against
2.01 to 2.21 in every run before, the ab23 state; p8m2-3, the round's first run, still read 2.19), and
those three runs read 20 to 22% whichever binary sat in them. Medians as the wrapper reads them:
defaults 17.6 (reference) / 18.0 (candidate), preview 17.1 / 17.1; no gap that counts in either.
Read like for like, rounds 1 and 2: defaults 17.8 / 17.5 against 18.0 / 17.6, preview 17.1 / 16.7
against 16.9 / 17.1, every run under 20%, the pairs within 0.5 of a point in both orders. Passed;
P9-M2 adds no cost while the lists scroll, with the column hidden or shown.

### Console checks (for Jeremy, unelevated, after the gate)

With `C:\Users\jtkli\projects\twinfm\target-p9m2\release\twinfm.exe`, the preview shown (Alt+P),
each beside the program that normally opens it where there is one:

1. A Rust source file (one of TwinFM's own, `src\preview.rs`): Consolas, long comment lines wrapped,
   tabs as four spaces, scrolls smoothly.
2. A log of about 2 MB or more: the note "Only the first 2 MB ..." at the top, scrolling to the end
   without stalls.
3. A UTF-16 text file (a `.reg` export from Registry Editor is one): the letters right, no gaps
   between them.
4. JPEGs from a phone, some taken upright and some sideways: each shown the way Photos shows it, with
   its size, file size and JPEG in the note.
5. A panorama 10,000 px or wider: shown whole across the column, the note giving its full size.
6. An animated GIF: its first frame, still.
7. An SVG with text in it: the shapes drawn; the text is not (known, P8-M2's rule).
8. A web page saved from Edge ("Save as", complete) beside the same page in Edge: headings, lists,
   tables, links and the saved pictures in the column; scripts, menus and styling gone.
9. A page of nested lists and tables (a documentation page): the nesting kept, tables readable.
10. A page of nothing but a script: "This page has no text to show."
11. A `.heic` photo and a file with no extension: "No preview".
12. The column's width dragged with a picture and with a long text shown: both follow the width.
