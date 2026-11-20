# TwinFM roadmap

The one list of known bugs and wanted features that are not built yet. Started 2026-10-25, after
0.2.0, from the deferred lists of the Phase 3 to Phase 11 closes and Jeremy's requests from daily use.
Each item says where it came from; the phase records (`docs/phase-N-plan.md`) hold the detail. An item
leaves this list when a phase picks it up, and the phase's own record carries it from there; each phase
close adds back what the phase put off (Jeremy's rule, 2026-10-26). Phase 12 took its items at its
kickoff (2026-10-25) and closed 2026-10-27 (`phase-12-plan.md` § Phase 12 close); what it put off is
below, marked P12. Phase 13 took its items at its kickoff (2026-10-27, `phase-13-plan.md`): the details
view's click-and-drag selection, the TreeSize-style view with size on disk, the owed checks (the dead
share, Phase 12's console checks; the long soak was run after the Phase 12 close), the styled-text
memory follow-up and the two small log and soak clean-ups. It closed 2026-10-29 (`phase-13-plan.md` § Phase 13 close); what it put off is below,
marked P13. Phase 14 took its items at its kickoff (2026-10-29, `phase-14-plan.md`): saving a search
(as a sidebar group, run again when opened), the sizes view following changes (with its columns' place),
hover on file rows and tiles, and the scrollbar track's repeat. It closed 2026-11-13 (`phase-14-plan.md` §
Phase 14 close); what it put off is below, marked P14. Phase 15 took its items at its kickoff (2026-11-13,
`phase-15-plan.md`): Extract in the Windows 11 menu (new, from Jeremy's screenshot, never on this list), the
tab menu and the visible note for a saved search, the folder sizes' progress line, the Size on disk column's
grip and sort, and glob classes and regular expressions in search. It closed 2026-11-14 (`phase-15-plan.md` §
Phase 15 close); what it put off is below, marked P15. Phase 16 took its items at its kickoff (2026-11-15,
`phase-16-plan.md`): a drag that says whether it will move or copy (new, from Jeremy's daily use, never on
this list), Explorer's extra large icons, small icons and List views, plain content search on the regex
engine, a new match in an open search tab, the `own_results` slowdown, the mouse's Back and Forward over the
tab strip, a text box and the scrollbars, and the check of the blank-part click against Explorer. It closed
2026-11-20 (`phase-16-plan.md` § Phase 16 close); what it put off, and what Jeremy asked for during it, is
below, marked P16.

## Look and feel

- **The title bar under Light or Dark can flip when Windows' own mode changes** until the settings
  dialog's OK or a restart; a switch back to System at runtime keeps Windows' mode of that moment
  (`phase-10-plan.md` § Phase 10 close).
- **Corners still in the Fluent style**: the list and scroll areas, the About dialog's text (P10 close;
  Phase 12 did the tooltips and the scrollbars).
- **Hover**: no fade on the sidebar's and the crumb dropdown's rows (they scroll); Slint's tooltip keeps
  its 500 ms delay and sits under the pointer, not under its control (P12-M1).
- **Scrollbars**: no arrow buttons; the
  hovered thumb covers the last pixel of the Modified column; a grid pane with under 10 px past its last
  tile shows one tile fewer; a long Markdown file's bar is the list's own estimate, since Phase 8 (P12-M2).
- **TwinFM's questions**: the dialog has no icon; a drop from another program while a question shows
  starts a job that waits behind it (P12-M3).
- **Rows and the sidebar**: a third row density (P12-M4 left room for one); the rail's footer has only
  the chevron, with Settings and About on the full sidebar only; a drop into the bookmarks gap from a
  program that allows no link shows the copy cursor and adds nothing (P12-M5).
- **Symbols outside the UI font draw as empty boxes** in the preview (rare entities such as ∳); the
  characters are right when copied. Font fallback is a Slint question (P11-M4 console check).
- **Small icons and the List view use one cell width, 220 px**, and a long name elides; Explorer sizes the
  cells to the longest name on screen. With thumbnails on, both show 16 px thumbnails where Explorer shows
  type icons (P16-M3, P16-M5; calls Jeremy can reverse).

## Search

- An index (a search of `C:\Windows` takes about 39 s).
- The `word in *.ext` syntax, offered and not taken (P11-M1).
- No folder sizes for search results or for the other drives (P3-M3).
- Search tabs left open are not restored at start; a saved search in the sidebar is the way to keep one (P14-M1).
- **A search tab stops following at the first window of changes it cannot account for** (past 10,000
  records, or an overflow) and says "Not following changes"; a search of a whole drive will often get
  there, and a background search tab is last in line for the window's eight watches. An index would lift
  this (P16-M6).

## Preview

- Zoom for anything but a PDF; copying a PDF's text; selecting text (waits for Slint).
- Colours for `.txt`, `.csv` and Markdown's fenced code blocks.
- HTML charsets beyond Windows-1252, ISO-8859-1 and -15; UTF-16 named in a meta tag; `colspan`'s
  repeated cells drawn empty; the 106 legacy entity forms without `;`.
- The old binary Office formats (`.doc`, `.xls`, `.ppt`); carried since Phase 9.
- A revision 6 PDF password with characters hayro's unfinished SASLprep would change.
- **Google Docs and Sheets** (Jeremy, during Phase 16: "For next phase, can we add in previews for google docs &
  sheets?"). A `.gdoc` or `.gsheet` on Google Drive's volume is a small pointer, not the document: a preview
  means signing in to Google and fetching the document over the network, neither of which TwinFM does
  today. A design discussion first (P16 close).

## Files, folders and watching

- **Thumbnails inside archives**: Windows has no thumbnail provider there; carried since Phase 9.
- **A cold `.mp4` thumbnail miss** on Jeremy's Downloads, not reproduced; the log line added in the
  P11 video follow-up names the cause if it returns.
- **Explorer's other views**: tiles, content (P4-M1; medium icons came in P6-M2; extra large, small and
  list in Phase 16).
- **The wait before a big move or copy starts**: 40,000 files out of one folder waited about 28 s on
  Jeremy's folders before the first file moved, 19.7 s of it TwinFM handing the items to the shell one by
  one and 8.8 s the shell's own preparation; the undo the same. Handing them over as one batch is the
  candidate (P16-M2, the big move).
- **A drag of 40,000 items holds the window about 2 s at its start**: the items are turned into shell items
  on the UI thread at the press (`parse_items`; P16-M2, the big move).
- **The sizes view and hard links**: when the name a hard-linked file was counted under is deleted, its other
  names are not counted until F5 (P14-M2).
- A deleted watched folder stays `Lost`; the subtree watch's overflow could not be forced (the test
  covers its handling); the log's cut timer's per-minute string (a clean-up) (P11 close).

## Health and owed checks

- **Accessibility**: a Narrator pass over everything since Phase 2 and the grid's tiles as nodes of
  their own; deferred for much later (Jeremy, 2026-10-15).
- **Memory**: the Recycle Bin enumeration's shell cache, small (P12 memory check).
- **A OneDrive folder for the soak**: not approved, since it would upload every cycle's files (P12
  kickoff).
- **The dead share, one case not reached**: a watched share whose name has gone after Windows dropped
  its connection (the session's `net use` kept it; the check needs that entry deleted and the share's
  password typed again) (P13-M4).
- **A scripted right-button drag that never returned**, once: P15-M2's ten-minute soak, about 590 s in,
  after 15 clean drags (`[auto] the scripted drag is still running after 20 s: posting Escape`). Not seen
  in three later ten-minute soaks nor in the Phase 15 hour's 90 drags; not reproduced (P15 close).
- **`fs::watch::tests::bursts_are_coalesced` failed once under load** (P15-M2): a timing the test does not
  allow for.
- **OneDrive's error mark** has been drawn only by its test: no file here has reached the error state; a
  refused `_vti_` name stays "sync pending" (P12-M7).
- **The List view's accessibility tree**: its columns just outside the window are in the tree, and each
  column that scrolls in rebuilds it; about two thirds of the 2.8 points the list costs over small icons
  (ab88). Accepted by Jeremy ("Accept it"); leaving those columns out, or a narrower ring, is the candidate
  (P16-M5).
- **`fileops::archive`'s worker tests failed once with os error 32** (a file in use) while two sessions' test
  suites overlapped on the machine; not seen in a suite run alone (P16 close).
- **Small clean-ups**: the driver's `tabdrag` presses at y 16 of the window, which has been the toolbar
  since P6-M3 (no script uses it); the `[watch] ... records in the folder itself: refresh` line reads
  wrongly for a search tab, where nothing is refreshed (P16-M2, P16-M6).

## Other platforms

- **Linux (Fedora)** (Jeremy, 2026-11-19: "can you tell me if its possible to port twinfm to fedora linux?
  What's the effort level required?", then "add it to the roadmap"). Possible: Rust and Slint both run
  there, and about 53,000 lines of Rust and 6,200 of Slint would carry over in part. What is bound to
  Windows is everything that talks to the shell: drag and drop, file operations with the Recycle Bin and
  undo, the context menus, icons and thumbnails, the cloud marks, and the archive formats Windows' own
  handler reads. **A feasibility spike first**, in this order: whether a 40,000-file folder is a problem at
  all in the file managers Fedora already has (Dolphin, Krusader), since that is why TwinFM exists; whether
  Slint can drag files to and from other programs on Wayland; then how much of the Windows-only code can sit
  behind one boundary. The coordinating session's estimate, not measured: about one phase to draw that
  boundary, two for a first usable build, three to five for parity with the Windows version (P16 close).

## The Slint fork

- **The whole-row snap**: Slint snaps a list rebuilt while scrolled to a whole row (a settings switch,
  a tab switch, a thumb drag). The one-line fork patch is proven (`target-zcfix`) and would be the
  fourth fork patch (the third, since 2026-10-28, is the ListView fix), with its own triplet (P3-M1, P3-M5).
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
