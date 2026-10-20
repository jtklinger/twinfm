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

## Open questions

Written 2026-10-20 at the kickoff.

- **hayro on real PDFs**: how many of Jeremy's own PDFs it renders right, and how fast; the P8-M1
  console check is the measure. If it falls short, PDFium is the recorded alternative (Jeremy's call).
- **Carried from Phase 7**: Google Drive's volume found by its name; a sync-root scratch folder for
  the soak; the Slint whole-row snap patch and upstreaming; the dead share console check (P5-M1).
