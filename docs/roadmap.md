# TwinFM roadmap

The one list of known bugs and wanted features that are not built yet. Started 2026-10-25, after
0.2.0, from the deferred lists of the Phase 3 to Phase 11 closes and Jeremy's requests from daily use.
Each item says where it came from; the phase records (`docs/phase-N-plan.md`) hold the detail. An item
leaves this list when a phase picks it up, and the phase's own record carries it from there; each phase
close adds back what the phase put off (Jeremy's rule, 2026-10-26). Phase 12 took its items at its
kickoff (2026-10-25) and closed 2026-10-27 (`phase-12-plan.md` § Phase 12 close); what it put off is
below, marked P12. Phase 13 took its items at its kickoff (2026-10-27, `phase-13-plan.md`): the details
view's click-and-drag selection, the TreeSize-style view with size on disk, the owed checks (the dead
share, Phase 12's console checks, the long soak), the styled-text memory follow-up and the two small log
and soak clean-ups.

## Asked for in daily use, not scheduled

- **Save a search results tab** (Jeremy, 2026-10-23: "be able to save a search results tab as a
  custom tab"). Settled first, with Jeremy: whether the query is kept (re-run when opened, as Windows'
  saved searches do) or the results as found; where it lives (a sidebar row, a restored tab, or
  both); its name; whether content searches are saved the same way.

## Look and feel

- **The title bar under Light or Dark can flip when Windows' own mode changes** until the settings
  dialog's OK or a restart; a switch back to System at runtime keeps Windows' mode of that moment
  (`phase-10-plan.md` § Phase 10 close).
- **Corners still in the Fluent style**: the list and scroll areas, the About dialog's text (P10 close;
  Phase 12 did the tooltips and the scrollbars).
- **Hover**: no fade on file rows, tiles, or the sidebar's and the crumb dropdown's rows (they scroll);
  Slint's tooltip keeps its 500 ms delay and sits under the pointer, not under its control (P12-M1).
- **Scrollbars**: a press on the track pages once and does not repeat while held; no arrow buttons; the
  hovered thumb covers the last pixel of the Modified column; a grid pane with under 10 px past its last
  tile shows one tile fewer; a long Markdown file's bar is the list's own estimate, since Phase 8 (P12-M2).
- **TwinFM's questions**: the dialog has no icon; a drop from another program while a question shows
  starts a job that waits behind it (P12-M3).
- **Rows and the sidebar**: a third row density (P12-M4 left room for one); the rail's footer has only
  the chevron, with Settings and About on the full sidebar only; a drop into the bookmarks gap from a
  program that allows no link shows the copy cursor and adds nothing (P12-M5).
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
- **Memory**: the Recycle Bin enumeration's shell cache, small (P12 memory check).
- **A OneDrive folder for the soak**: not approved, since it would upload every cycle's files (P12
  kickoff).
- **OneDrive's error mark** has been drawn only by its test: no file here has reached the error state; a
  refused `_vti_` name stays "sync pending" (P12-M7).

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
