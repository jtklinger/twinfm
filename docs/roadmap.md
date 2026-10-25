# TwinFM roadmap

The one list of known bugs and wanted features that are not built yet. Started 2026-10-25, after
0.2.0, from the deferred lists of the Phase 3 to Phase 11 closes and Jeremy's requests from daily use.
Each item says where it came from; the phase records (`docs/phase-N-plan.md`) hold the detail. An item
leaves this list when a phase picks it up, and the phase's own record carries it from there.

## Phase 12 (planned 2026-10-25, `docs/phase-12-plan.md`)

Jeremy, 2026-10-25: "Plan phase 12 by starting with the "later" list from phase 10, then add in the
onedrive sync icons and improving the look of google drive." These items are in Phase 12's plan, and
its record carries them from here:

- **The Phase 10 later list**: a fade on hover, tooltips in the look, the status bar's badges, the
  preview column's frame (P12-M1); TwinFM's own questions in the look (P12-M3); comfortable 30 px rows
  as a setting (P12-M4); the sidebar's icons-only rail, bookmarks reordered by dragging, a Recent group
  (P12-M5).
- **From daily use**: scrollbars easier to see (P12-M2); a new tab by mouse, a "+" with the tab strip
  always shown, and a bookmark made with the mouse (Jeremy: "Include it with m5") (P12-M5).
- **Google Drive**: its own commands in the classic menu are refused (Jeremy: "Files can't be marked
  available for offline use."), and its volume is found by its label alone (P12-M6).
- **OneDrive's syncing and error icons on files** (P12-M7).
- **The memory check**: heap snapshots of a 60-minute soak of 0.2.0 (the coordinating session).
- **Carried, on conditions the plan names**: a OneDrive folder for the soak, the dead network share
  check, the stale worktrees.

## Asked for in daily use, not scheduled

- **Click-and-drag selection in the details view** (Jeremy, P10-M1 console check: "On folders that
  are configured with details view, I cannot click-and-drag the mouse cursor to highlight multiple
  items"). The band (`src/band.rs`) works in the icon grids only; a press on a row starts a drag, so
  where the band starts (empty space, or past a threshold on a row) is settled first.
- **Save a search results tab** (Jeremy, 2026-10-23: "be able to save a search results tab as a
  custom tab"). Settled first, with Jeremy: whether the query is kept (re-run when opened, as Windows'
  saved searches do) or the results as found; where it lives (a sidebar row, a restored tab, or
  both); its name; whether content searches are saved the same way.
- **A TreeSize-style view** (Jeremy, 2026-10-25: "add a treesize viewer so we can see what folders
  are the largest consumers of capacity"). Folders listed largest first, opened down level by level,
  each with its share of the whole. Builds on the folder sizes TwinFM already walks (Alt+Shift+Enter,
  `src/fs/sizes.rs`). Settled first: a sorted tree or a map of boxes; a whole drive or the folder
  shown; size or size on disk (below); where the view lives (a tab, the preview column, its own
  window).

## Look and feel

- **The title bar under Light or Dark can flip when Windows' own mode changes** until the settings
  dialog's OK or a restart; a switch back to System at runtime keeps Windows' mode of that moment
  (`phase-10-plan.md` § Phase 10 close).
- **Corners still in the Fluent style**: the list and scroll areas, the About dialog's text (P10 close;
  the tooltip and the scrollbars are Phase 12's).
- **Symbols outside the UI font draw as empty boxes** in the preview (rare entities such as ∳); the
  characters are right when copied. Font fallback is a Slint question (P11-M4 console check).

## Search

- Glob and regular expression patterns, and an index (a search of `C:\Windows` takes about 39 s).
- A new match in an open search tab needs F5 (its rows on screen follow changes since P11-M2; new
  matches do not appear on their own).
- The `word in *.ext` syntax, offered and not taken (P11-M1).
- No folder sizes for search results or for the other drives (P3-M3).

## Preview

- Zoom for anything but a PDF; copying a PDF's text; selecting text (waits for Slint).
- Colours for `.txt`, `.csv` and Markdown's fenced code blocks.
- HTML charsets beyond Windows-1252, ISO-8859-1 and -15; UTF-16 named in a meta tag; `colspan`'s
  repeated cells drawn empty; the 106 legacy entity forms without `;`.
- The old binary Office formats (`.doc`, `.xls`, `.ppt`); carried since Phase 9.
- A revision 6 PDF password with characters hayro's unfinished SASLprep would change.

## Files, folders and watching

- **Size on disk** beside size (P3-M3); pairs with the TreeSize view.
- **The mouse's Back and Forward over a text box, the tab strip or the list's scrollbar** do nothing
  (P2-M3, P3-M1).
- **Thumbnails inside archives**: Windows has no thumbnail provider there; carried since Phase 9.
- **A cold `.mp4` thumbnail miss** on Jeremy's Downloads, not reproduced; the log line added in the
  P11 video follow-up names the cause if it returns.
- **Explorer's other views**: extra large and small icons, list, tiles, content (P4-M1; medium icons
  came in P6-M2).
- A deleted watched folder stays `Lost`; the subtree watch's overflow could not be forced (the test
  covers its handling); the log's cut timer's per-minute string (a clean-up) (P11 close).

## Health and owed checks

- **Accessibility**: a Narrator pass over everything since Phase 2 and the grid's tiles as nodes of
  their own; deferred for much later (Jeremy, 2026-10-15).

## The Slint fork

- **The whole-row snap**: Slint snaps a list rebuilt while scrolled to a whole row (a settings switch,
  a tab switch, a thumb drag). The one-line fork patch is proven (`target-zcfix`) and would be the
  third fork patch, with its own triplet (P3-M1, P3-M5).
- **Upstreaming the two accessibility patches**: `docs/upstream-issue-draft.md` and `docs/patches/`
  are the record. Jeremy decides when; no session posts to slint-ui/slint.

## Older edge cases (not re-checked)

The Phase 3, 4 and 5 closes list many small edge cases in directory watching, file operations, drag
and drop, the band and folder sizes. Phases 6 to 11 finished most of them; the Phase 11 kickoff's
inventory left these marked "status unclear", and they are not re-checked against the code here. The
phase that picks one checks it first.

- A move to another monitor with a different scale keeps the old icon size on rows already shown until
  their tab lists again (P2-M5, P3 close).
- Rows added or removed above the visible part of a list shift the visible rows (P5 close).
- An archive tab does not follow changes to its archive file (P3-M4).
- Items copied from inside an archive pasted into Explorer, and drops of Outlook messages (P3-M4).
- The band scrolls at one speed however far past the edge the pointer goes, where Explorer speeds up
  (P4-M3).
- The full lists: `phase-3-plan.md` § Phase 3 close, `phase-4-plan.md` § Phase 4 close,
  `phase-5-plan.md` § Phase 5 close.
