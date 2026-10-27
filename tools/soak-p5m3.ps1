# P5-M3 soak: the P4-M3 soak (file operation cycle, drops through the drop target, scripted context
# menus, a bookmark and a tab as drop targets, the drop menu, a right-button drag, the grid cycle,
# the big-folder scroll scenario) plus a spring-load and a crumb drop: the right pane goes to
# spring (made by the setup, holding the folder inner), a drag held 1300 ms over its row 0 opens
# inner there, two files are dropped on the breadcrumb's parent crumb (a copy into spring), a drag
# hovers the sidebar list's bottom edge for 300 ms (its auto-scroll, when the sidebar overflows)
# and the left pane's inactive tab for 300 ms (under SPRING_MS: armed, never opened), then the
# right pane goes back to spring and Shift+Delete removes the two copies.
#
#   powershell -File tools\soak-p5m3.ps1 -Minutes 30
#
# Same scratch tree, fence (TWINFM_SOAK=1, %TEMP%\twinfm-m5\), setup and reports as soak-p4m3.ps1,
# plus: spring armed, spring-loaded (rows, tabs: tabs must be 0), spring disarmed, crumb drops,
# and the sidebar and tab-strip auto-scroll lines (counted, not required: the sidebar scrolls only
# when it overflows). Never set TWINFM_SOAK outside a soak. One Recycle Bin entry per cycle; the
# clipboard is overwritten.
#
# P4-M3's grid steps run between the P2-M4 steps and the scroll scenario, while the left pane shows
# src (the tree alone) and the right pane dst: two files are dropped into dst, both panes go to
# the grid (Ctrl+Shift+2 on the active pane: a click makes a pane active), the drops land on tile
# 0 (a file: into the pane's folder, replacing) and on empty space, the band selects both files
# from the gap after the last tile (x 202, y 60 of the right list: the grid holds 5 tiles to a
# row at this window width), the right pane goes back to details (it is active after the band),
# Shift+Delete removes the selection, and the left pane goes back to details.
#
# P6-M2's medium-grid cycle follows: two files are dropped into dst again, the right pane goes to
# medium tiles (Ctrl+Shift+3), a drop lands on tile 0, a left-button drag out of it is cancelled
# (its 80 px drag image), the band selects both files from the gap after the last tile (x 202, y
# 40: tile 2 of a medium grid row starts at 160) and leaves the cursor on tile 0, Shift+Right
# extends the selection from there (anchor 0 to 1), the right pane goes back to details, and
# Shift+Delete removes the selection. Reported: medium switches, 80 px drag images, the bands' and
# the Shift+Right's lines.
#
# P6-M3's toolbar cycle follows, in dst (empty again), each button through the callback a press
# calls (`toolbar <pane> <button>`): New file (the editor's name kept with Return) and its undo,
# New folder, Rename (to t), Copy and Paste (t - Copy beside it), Cut of t - Copy and Paste into
# src, Delete of it there (the Recycle Bin), and Shift+Delete of t. Reported: toolbar presses
# (eight per cycle), presses refused as disabled (must be 0), new files made and undone, renames.
# The cycle adds two Recycle Bin entries (the new file's undo and the Delete button), three in all.
#
# P7-M1's verb cycle follows, in dst (empty again): two files are dropped there, the test verb
# sleeps 1500 ms on the verb worker while the left list scrolls (`menuverb 1 sleep:1500`), the
# menu's Delete recycles f01 through TwinFM's job and Ctrl+Z restores it, the menu's Cut of f02 is
# cancelled by Escape (marks cleared, clipboard emptied), Create shortcut (`link`) runs on the
# worker by its verb, and Shift+Delete removes the three files. Reported: verb jobs queued, slept,
# invoked by verb with S_OK, routed deletes and their restores, cuts cancelled by Escape. One more
# Recycle Bin entry per cycle (the restore takes it out again), four in all.
#
# P7-M2: the context menus above open the Windows 11 style menu: the one on row 0 goes on to "Show
# more options" (the classic menu, closed from the in-menu timer), the one on the right pane's
# empty space is closed as it is, and the grid's tile menu is the classic one at once
# (`rclick ... shift`). Then a menu cycle in dst (after P7-M1's): two files dropped there, the
# menu's Delete on f01 and Ctrl+Z, its Cut on f02 cancelled by Escape, Compress to > ZIP file on
# f01 (the shell's Send to > Compressed (zipped) folder, found by its label on the verb worker),
# New folder from the background menu (the editor's name kept with Return), Open with's submenu
# opened and the menu closed with Escape twice, and Shift+Delete of everything. Reported: menus
# shown, rebuilt with the worker's answer, picks, ZIPs made, classic menus from Show more options,
# menupicks refused (must be 0). One more Recycle Bin entry per cycle that the undo takes out.
#
# P7-M3's archive cycle follows the P5-M3 steps, in arch (made by the setup with Windows' tar.exe:
# a.7z, a.tar.gz and a.zip of a.txt and d\c.txt, and c.txt): the left pane on dst, the right pane
# browses a.tar.gz and a.7z (listed by Windows' ArchiveFolder), copies a.txt out of the 7z into dst,
# a copy of it back into the 7z is refused (read-only), then in a.zip (TwinFM's writer): a.txt
# copied in (replacing), a new folder nz, renamed nz2, deleted, a.txt moved out to dst and dropped
# back in as a move (the original recycled), and from arch Compress to > 7z file and TAR file on
# c.txt, each undone (recycled). Reported: archive levels listed by ArchiveFolder, zip jobs done by
# kind, refusals of writes into the 7z, archives compressed and undone, archive jobs failed (must be
# 0). Three more Recycle Bin entries per cycle.
#
# P7-M4: the toolbar cycle starts with three hovers over the bar across the window (New file, Copy
# and Delete, 700 ms each: their tooltips show) and the mouse's Back and Forward over its first
# button (the active right pane goes back and forward again). After the archive cycle, a sidebar
# walk: the left pane made active, a click on the sidebar's first drive (C:), then Down, Down, Up,
# End, Home and Return (C: again), and Escape gives the keys back. Reported: tooltips shown (three
# per cycle), Back and Forward over the toolbar (one each per cycle), sidebar keys (five moves and
# one Return per cycle).
#
# P8-L2, after the archive cycle: the left pane on a.zip, the right on pw.zip (the setup's, made by
# tar.exe with ZipCrypto and the password twinfm, holding a.txt). With `zippassword twinfm`, a.txt
# goes from a.zip into pw.zip (replacing, encrypted on the way: the job asks the script for the
# password and checks it), is renamed a2.txt there, copied back into a.zip as it is (encrypted, no
# password asked), moved from a.zip into pw.zip again (replacing; it leaves a.zip), renamed a.txt
# again; then in a.zip a.txt is copied and pasted into its own folder (a - Copy.txt) and that copy
# deleted. Reported: copies from a zip (three per cycle), moves from a zip (one per cycle),
# passwords from the script and checked right (one each per cycle).
#
# P8-L3, after P8-L2's steps: the left pane on n.zip\a.zip (the setup's n.zip holding a copy of
# a.zip: an archive inside an archive, extracted to TwinFM's copy and browsed read-only), a.txt
# copied out of it into dst and removed there again, then the left pane on a.7z and a.txt copied
# into a.zip (the shell extracts it to a temporary folder, TwinFM adds it, replacing). Reported:
# copies out of an archive inside an archive, copies from an archive into a zip (one each per cycle).
#
# P8-M1: the preview cycle, after the sidebar walk: Alt+P shows the preview column, the left pane
# goes to preview (made by the setup: a-pages.pdf with four pages, b-one.pdf with one, c-damaged.pdf
# that is no PDF, d-notes.txt; the PDFs written here with a hand-made cross-reference table), the
# cursor goes through the four files waiting for each preview, the first PDF is scrolled, the
# column's width is changed and set back, and Alt+P hides it. Reported: PDFs opened (two per
# cycle), pages rendered, damaged messages (one per cycle), not-a-PDF lines (one per cycle),
# superseded lines, waitpreview timeouts and worker panics (both must be 0). No file is written.
# P8-M2: the setup adds e-dot.png (a picture, no preview), f-readme.md (headings, a list, a table,
# a code block, the local picture and links) and g-big.md (over 2 MB); after d-notes.txt the cursor
# goes on through the three, f-readme.md's anchor link is followed and g-big.md is scrolled.
# Reported besides: Markdown previews (two per cycle), pictures shown and alt texts (from the
# `pictures done` lines; g-big.md repeats the one picture thousands of times), styled-text
# fall-backs, cuts at 2 MB (one per cycle).
# P9-M2: d-notes.txt is previewed as text and e-dot.png as a picture now (no "no preview" line is
# left in the cycle), and the setup adds h-notes.txt (60 lines, tabs and one 20,000-character line),
# i-photo.jpg (1200 x 800, made with System.Drawing) and j-page.html (a head, a script and a style
# to drop, headings with ids, a list, a table, a pre, e-dot.png, an #anchor link); after g-big.md the
# cursor goes on through the three and j-page.html's anchor is followed. Reported besides: text
# previews (two per cycle: d-notes.txt and h-notes.txt), pictures (two per cycle), picture failures (must be 0), HTML previews
# (one per cycle).
# P10-M2: each cycle also uses the search box in the preview folder: `filter 0 notes` then
# `searchreturn 0` (Return: a search tab, closed), the same with `searchbutton 0` (the button), and
# `searchreturn 0` with the box blank (the list takes the focus, no tab); the report counts the
# three (one each per cycle).
# P11-M1: each cycle also runs a content search in src (`csearch 0 scratch file 3/7`: the box's
# content toggle on, the text, Ctrl+Shift+F), closes its tab and turns the toggle off; the report
# counts content searches done (one per cycle), the files they read (src's 200 at least) and the
# files they found (one per cycle: tree\d03\f07.txt).
# P11-M2: each cycle also narrows the left pane's Type column to 100 px and puts both panes back
# through the settings dialog's "Reset column widths" (`settings set cols_reset`, OK); the report
# counts the resets (two lines per cycle, one per pane), and the widths-back lines are two per
# cycle since (the P9-M1 cycle's and the reset's).
# P11-M3: each cycle also takes the left pane from src into src\tree, opens its history menu
# (`histmenu 0`, Alt+Down's command) and picks the place before (`histpick 1`, src); the report
# counts the history jumps (one per cycle).
# P12-M4: each cycle also turns comfortable rows on through the settings dialog in src\tree (`settings
# set row_height comfortable`, OK), clicks a row and pages down, and turns them off again; the report
# counts the switches each way and the 30 px geometry lines (one each per cycle). Since 2026-10-26
# (Jeremy: "comfortable should make rows taller") thumbnails go on between, a click and PageDown in the
# 44 px rows, then compact (32 px rows) and thumbnails off; the report counts the 44 px geometry lines
# (one per cycle).
# P12-M5: each cycle also drops src into the Bookmarks gap above bm (`dropfiles sidegap 0`: src
# bookmarked first), drags it below bm (`bookmark drag 0 1`), removes it again (`bookmark remove 1`,
# bm the sidebar's last row as before), and opens a tab with the "+" (`tab plus 0`) and closes it; the
# report counts the bookmarks added by the drop, the drags, the removals and the "+" tabs (one each per
# cycle). Every pane shows its tab strip since P12-M5, so every list starts 32 px lower (the driver
# aims follow). Since 2026-10-26 (Jeremy: "tab switches panes") the cycle ends with Tab and Shift+Tab
# from the left list, the keys to the right pane's list and back (the next cycle's scroll scenario
# lists both panes again, so the cursor Tab may have put on a row goes); the report counts the
# `[focus]` lines each way (one each per cycle).
# P13-M1: each cycle then hides the second pane with the toolbar's Two panes and shows it again
# (`toolbar 0 split` twice: the right pane comes back on the left's folder, which the next cycle's
# `nav 1` replaces), drags the split line to 0.35 of the room, where the left pane is too narrow for
# Type and Size (only Modified stays beside the name), and back to 0.5; the report counts the Two panes
# presses (two per cycle) and the `[cols] pane=0 shows` lines: narrowed to the name and Modified (one per
# cycle), and name, Size and Modified (two per cycle at the soak's 1200 px window and 100%: the
# re-split and the widening; the whole window shows Type too between them).
# P9-M3: the setup adds k-report.docx, l-table.xlsx and m-deck.pptx, zips of hand-written parts
# (a heading, a list, a table, a link and e-dot.png in the Word file; two sheets of shared strings,
# numbers and a date; two slides with a title, bullets and the picture); after j-page.html the cursor
# goes on through the three and l-table.xlsx's second sheet is followed (`previewlink #sheet=2`).
# Reported besides: docx, xlsx (two per cycle: the first sheet and the second) and pptx previews,
# sheet links followed (one per cycle), Office files unreadable (must be 0).
# P11-M4: on a-pages.pdf the zoom goes two steps in (110%, 125%), one out and back to 100%
# (`previewzoom`), and on f-readme.md "Copy text" copies everything and the first block
# (`previewcopy`, the clipboard overwritten). Reported besides: zoom lines (four per cycle), copies
# (two per cycle), copy failures and copies with no text shown (both must be 0).
# P11-M5: the setup adds n-code.rs (a block comment over 40 lines, so over a chunk's end) and
# o-script.ps1; after m-deck.pptx the cursor goes on through the two. Reported besides: files
# coloured (two per cycle), chunks Slint refused (must be 0); text previews are four per cycle.
# P11-M6: the setup adds p-locked.pdf (AES-256, user password "twinfm", written by pypdf, kept here
# as hex); after o-script.ps1 the cursor goes on to it, the column shows "Password-protected PDF",
# and `pdfpassword twinfm` gives the password and presses the button. Reported besides: passwords
# right (one per cycle), wrong or refused (both must be 0), password PDFs opened (one per cycle).
#
# P8-L1, after the preview cycle: the left pane on src\tree (ten folders) goes to medium tiles,
# PageDown and PageUp move its cursor by grid rows (Explorer's), it goes back to details, two
# folders are selected and Return opens both as new tabs (placed after the active tab 0), Ctrl+2
# and Ctrl+W twice close them, and Ctrl+1 makes the pane's first tab active again. Reported:
# several-item opens (one per cycle) and the tabs they opened (two per cycle).
#
# P9-M1, after the P8-L1 cycle: in the left pane on src\tree (details, the default widths 120, 90,
# 140), the Type | Size grip is dragged 30 px right (Type 150, Size 60), the Modified column is
# fitted by a double-click on its grip (Size takes the difference), then colwidth sets Modified to
# 140 and Size to 90, which puts every width back. Reported: drags read back as Size 90 -> 60 (one
# per cycle), fits (one per cycle), and widths lines back at 120, 90, 140 (one per cycle).
#
# P9-M4, after the column cycle: thumbnails on (the dialog), the left pane on preview (its rows in
# 32 px with thumbnails), a cancelled drag out of i-photo.jpg's row (row 8), a search for "photo"
# from there (a search tab whose result is keyed for its thumbnail), the search tab closed, the
# filter cleared and thumbnails off again. Reported: drag images from the thumbnail (one per cycle)
# and searches for photo (one per cycle).
param(
    [double]$Minutes = 30,
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'),
    [string[]]$ScrollFolders = @('C:\traces\bigdir', 'C:\Windows\System32', 'C:\Windows\WinSxS\Manifests'),
    [int]$Cycles = 500
)
$ErrorActionPreference = 'Stop'
# The copy engine's progress window is an unowned top-level window of the same process, so
# Process.CloseMainWindow could pick it; close the app's own window (title "TwinFM - <folder>") instead.
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices;
public static class SoakWinP5m3 {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  public static bool Close(uint pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      uint p; GetWindowThreadProcessId(h, out p);
      var t = new StringBuilder(64); GetWindowText(h, t, 64);
      if (p == pid && t.ToString().StartsWith("TwinFM - ")) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found != IntPtr.Zero && PostMessage(found, 0x10, IntPtr.Zero, IntPtr.Zero);
  }
}
"@
$Exe = (Resolve-Path $Exe).Path
$root = Join-Path ([System.IO.Path]::GetTempPath()) 'twinfm-m5'
$src = Join-Path $root 'src'
$dst = Join-Path $root 'dst'
$bm = Join-Path $root 'bm'
$tabdir = Join-Path $root 'tabdir'
$spring = Join-Path $root 'spring'
$arch = Join-Path $root 'arch'
$prev = Join-Path $root 'preview'
foreach ($d in $src, $dst, $bm, $tabdir, $spring, $arch, $prev) {
    if (Test-Path $d) { Remove-Item -LiteralPath $d -Recurse -Force }
}
for ($i = 1; $i -le 10; $i++) {
    $sub = Join-Path $src ("tree\d{0:00}" -f $i)
    New-Item -ItemType Directory -Force $sub | Out-Null
    for ($j = 1; $j -le 20; $j++) {
        [System.IO.File]::WriteAllText((Join-Path $sub ("f{0:00}.txt" -f $j)), "scratch file $i/$j`r`n")
    }
}
foreach ($d in $dst, $bm, $tabdir, (Join-Path $spring 'inner')) { New-Item -ItemType Directory -Force $d | Out-Null }
# P7-M3: the archives, made with Windows' own tar.exe (bsdtar) from a small tree.
$archSrc = Join-Path $root 'arch-src'
if (Test-Path $archSrc) { Remove-Item -LiteralPath $archSrc -Recurse -Force }
New-Item -ItemType Directory -Force (Join-Path $archSrc 'd'), $arch | Out-Null
[System.IO.File]::WriteAllText((Join-Path $archSrc 'a.txt'), "archive file a`r`n")
[System.IO.File]::WriteAllText((Join-Path $archSrc 'd\c.txt'), "archive file c`r`n")
[System.IO.File]::WriteAllText((Join-Path $arch 'c.txt'), "compress me`r`n")
$bsdtar = Join-Path $env:SystemRoot 'System32\tar.exe'
& $bsdtar -cf (Join-Path $arch 'a.7z') --format 7zip -C $archSrc a.txt d
& $bsdtar -cf (Join-Path $arch 'a.zip') --format zip -C $archSrc a.txt d
& $bsdtar -czf (Join-Path $arch 'a.tar.gz') -C $archSrc a.txt d
# P8-L2: a password-protected zip (ZipCrypto, password twinfm) of a.txt.
& $bsdtar -cf (Join-Path $arch 'pw.zip') --format zip --options zip:encryption=zipcrypt --passphrase twinfm -C $archSrc a.txt
# P8-L3: n.zip holds a copy of a.zip (an archive inside an archive).
& $bsdtar -cf (Join-Path $arch 'n.zip') --format zip -C $arch a.zip
# P8-M1: the preview folder. A PDF of $n pages of 200 x 300 pt, each a red rectangle on white, with
# its cross-reference table's offsets counted here (as preview.rs's test writes it).
function Write-Pdf([string]$path, [int]$n) {
    $content = '1 0 0 rg 20 20 160 260 re f'
    $objs = @('<< /Type /Catalog /Pages 2 0 R >>', ('<< /Type /Pages /Kids [{0}] /Count {1} >>' -f ((0..($n - 1) | ForEach-Object { "$(3 + 2 * $_) 0 R" }) -join ' '), $n))
    for ($i = 0; $i -lt $n; $i++) {
        $objs += "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Contents $(4 + 2 * $i) 0 R >>"
        $objs += "<< /Length $($content.Length) >>`nstream`n$content`nendstream"
    }
    $sb = New-Object System.Text.StringBuilder
    [void]$sb.Append("%PDF-1.4`n")
    $offs = @()
    for ($i = 0; $i -lt $objs.Count; $i++) {
        $offs += $sb.Length
        [void]$sb.Append("$($i + 1) 0 obj`n$($objs[$i])`nendobj`n")
    }
    $xref = $sb.Length
    [void]$sb.Append("xref`n0 $($objs.Count + 1)`n0000000000 65535 f `n")
    foreach ($o in $offs) { [void]$sb.Append(("{0:D10} 00000 n `n" -f $o)) }
    [void]$sb.Append("trailer`n<< /Size $($objs.Count + 1) /Root 1 0 R >>`nstartxref`n$xref`n%%EOF`n")
    [System.IO.File]::WriteAllText($path, $sb.ToString(), [System.Text.Encoding]::ASCII)
}
New-Item -ItemType Directory -Force $prev | Out-Null
Write-Pdf (Join-Path $prev 'a-pages.pdf') 4
Write-Pdf (Join-Path $prev 'b-one.pdf') 1
[System.IO.File]::WriteAllText((Join-Path $prev 'c-damaged.pdf'), "%PDF-1.4`ncut short: no PDF`n")
[System.IO.File]::WriteAllText((Join-Path $prev 'd-notes.txt'), "plain text`r`n")
Add-Type -AssemblyName System.Drawing
$bmp = New-Object System.Drawing.Bitmap 200, 60
$g = [System.Drawing.Graphics]::FromImage($bmp); $g.Clear([System.Drawing.Color]::SeaGreen); $g.Dispose()
$bmp.Save((Join-Path $prev 'e-dot.png'), [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
$readme = "# Soak README`n`nSome **bold**, *italic*, ``code`` and a [link](#usage).`n`n- one`n- two`n  - [x] done`n`n" +
    "| Name | Size |`n|:--|--:|`n| a | 1 |`n| b | 2 |`n`n" + '```' + "`nfn main() {}`n" + '```' + "`n`n![dot](e-dot.png)`n`n## Usage`n`nThe end.`n"
[System.IO.File]::WriteAllText((Join-Path $prev 'f-readme.md'), $readme)
$sb = New-Object System.Text.StringBuilder
for ($i = 0; $sb.Length -lt 2300000; $i++) { [void]$sb.Append("## Section $i`n`n" + $readme) }
[System.IO.File]::WriteAllText((Join-Path $prev 'g-big.md'), $sb.ToString())
# P9-M2: text, a photo and a page.
$sb = New-Object System.Text.StringBuilder
for ($i = 0; $i -lt 60; $i++) { [void]$sb.Append("`tline $i of the soak's notes`r`n") }
[void]$sb.Append(('x' * 20000) + "`r`n")
[System.IO.File]::WriteAllText((Join-Path $prev 'h-notes.txt'), $sb.ToString())
# P11-M5: source to colour, a block comment across the first chunk's end.
$rs = "/// A fixture for the syntax colours.`nfn main() {`n    let s = ""text"";`n" + ((1..36 | ForEach-Object { "    let x$_ = $_; // line $_" }) -join "`n") + "`n    /* a comment`n    over the chunk's end`n    */`n}`n"
[System.IO.File]::WriteAllText((Join-Path $prev 'n-code.rs'), $rs)
# P11-M6: a password-protected PDF.
$hex = '255044462d312e330a25e2e3cfd30a312030206f626a0a3c3c0a2f50726f6475636572203c623439323930636165323030656333323939306361396238623137386461636465653465666366393538316666633863633261373132303537383032383665323e0a3e3e0a656e646f626a0a322030206f626a0a3c3c0a2f54797065202f50616765730a2f436f756e7420310a2f4b696473205b203420302052205d0a3e3e0a656e646f626a0a332030206f626a0a3c3c0a2f54797065202f436174616c6f670a2f50616765732032203020520a3e3e0a656e646f626a0a342030206f626a0a3c3c0a2f54797065202f506167650a2f5265736f7572636573203c3c0a3e3e0a2f4d65646961426f78205b20302e3020302e302032303020333030205d0a2f506172656e742032203020520a2f436f6e74656e74732035203020520a3e3e0a656e646f626a0a352030206f626a0a3c3c0a2f4c656e6774682034380a3e3e0a73747265616d0ad9865c0a9cafeaec97f94a580586522c54a8821d8f3f38280227ad08450da0dff94684f12e0f8d2c1baf5a73472784900a656e6473747265616d0a656e646f626a0a362030206f626a0a3c3c0a2f5620350a2f5220360a2f4c656e677468203235360a2f5020343239343936373239320a2f46696c746572202f5374616e646172640a2f4f203c3139323437326631633732343861303238636362346465383466393336346262656262616230323830343531333734366664663631356433646132653966353062303934346363333538373164323936346138323237396266336439633532313e0a2f55203c6434393839666230303537343830613134316266663163636532396131613537393033633338633631663862663164316433386561623339633639383938666131353138376135393862663264323464383661333061636464386432353062393e0a2f4346203c3c0a2f5374644346203c3c0a2f417574684576656e74202f446f634f70656e0a2f43464d202f41455356330a2f4c656e6774682033320a3e3e0a3e3e0a2f53746d46202f53746443460a2f53747246202f53746443460a2f4f45203c373766396434386237336262643761333864626237616534616131323938303736373164353931306565613638623038373961396332626431636433383862643e0a2f5545203c663163613164313763373832633566626366333462613031646635393366643538323038613131326566353839636235363864346336633961613166373133383e0a2f5065726d73203c35383536383138613032363834623830386637636361616165336264663931373e0a3e3e0a656e646f626a0a787265660a3020370a303030303030303030302036353533352066200a30303030303030303135203030303030206e200a30303030303030313133203030303030206e200a30303030303030313732203030303030206e200a30303030303030323231203030303030206e200a30303030303030333331203030303030206e200a30303030303030343239203030303030206e200a747261696c65720a3c3c0a2f53697a6520370a2f526f6f742033203020520a2f496e666f2031203020520a2f4944205b203c333233373631363133393338333933363336333536323634333333393634363336363631333133303338363333333636363133303334363136313334363536353e203c333233373631363133393338333933363336333536323634333333393634363336363631333133303338363333333636363133303334363136313334363536353e205d0a2f456e63727970742036203020520a3e3e0a7374617274787265660a3938340a2525454f460a'
[System.IO.File]::WriteAllBytes((Join-Path $prev 'p-locked.pdf'), [byte[]](0..($hex.Length / 2 - 1) | ForEach-Object { [Convert]::ToByte($hex.Substring($_ * 2, 2), 16) }))
[System.IO.File]::WriteAllText((Join-Path $prev 'o-script.ps1'), "<# help #>`r`nparam([int]`$Minutes = 5)`r`nif (`$Minutes -gt 0) { Write-Host ""run `$Minutes"" } # done`r`n")
$bmp = New-Object System.Drawing.Bitmap 1200, 800
$g = [System.Drawing.Graphics]::FromImage($bmp)
$brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush((New-Object System.Drawing.Point 0, 0), (New-Object System.Drawing.Point 1200, 800), [System.Drawing.Color]::SteelBlue, [System.Drawing.Color]::Orange)
$g.FillRectangle($brush, 0, 0, 1200, 800); $brush.Dispose(); $g.Dispose()
$bmp.Save((Join-Path $prev 'i-photo.jpg'), [System.Drawing.Imaging.ImageFormat]::Jpeg); $bmp.Dispose()
$page = @'
<!DOCTYPE html>
<html><head><title>Soak page</title><style>body { font: 12px sans-serif }</style>
<script>if (1 < 2) { document.write("<p>never shown</p>"); }</script></head>
<body><h1 id="top">Soak &amp; page</h1>
<p>Some <b>bold</b>, <i>italic</i>, <code>code</code> and a <a href="#end">link to the end</a>.</p>
<ul><li>one<li>two<ul><li>nested</li></ul></ul>
<table><tr><th>Name</th><th>Size</th></tr><tr><td>a</td><td>1</td></tr></table>
<pre>fn main() {
	println!("hi");
}</pre>
<p><img src="e-dot.png" alt="dot"></p>
<h2 id="end">The end</h2><p>Done.</p>
</body></html>
'@
[System.IO.File]::WriteAllText((Join-Path $prev 'j-page.html'), $page)
# P9-M3: Word, Excel and PowerPoint files as zips of hand-written parts (only those TwinFM reads).
Add-Type -AssemblyName System.IO.Compression
function Write-Parts($path, $parts) {
    $fs = [System.IO.File]::Create($path)
    $zip = New-Object System.IO.Compression.ZipArchive($fs, [System.IO.Compression.ZipArchiveMode]::Create)
    foreach ($name in $parts.Keys) {
        $w = $zip.CreateEntry($name).Open()
        $data = $parts[$name]
        if ($data -is [string]) { $data = [System.Text.Encoding]::UTF8.GetBytes($data) }
        $w.Write($data, 0, $data.Length); $w.Dispose()
    }
    $zip.Dispose(); $fs.Dispose()
}
$dot = [System.IO.File]::ReadAllBytes((Join-Path $prev 'e-dot.png'))
$wns = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="r" xmlns:wp="wp" xmlns:a="a"'
$wp = { param($style, $text) "<w:p><w:pPr>$style</w:pPr><w:r><w:t xml:space=`"preserve`">$text</w:t></w:r></w:p>" }
$doc = "<w:document $wns><w:body>" + (& $wp '<w:pStyle w:val="Heading1"/>' 'Soak report') + (& $wp '' 'A paragraph of the report.') +
    (& $wp '<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>' 'first point') + (& $wp '<w:numPr><w:ilvl w:val="1"/><w:numId w:val="1"/></w:numPr>' 'a detail') +
    '<w:tbl><w:tr><w:tc>' + (& $wp '' 'Name') + '</w:tc><w:tc>' + (& $wp '' 'Size') + '</w:tc></w:tr><w:tr><w:tc>' + (& $wp '' 'a') + '</w:tc><w:tc>' + (& $wp '' '1') + '</w:tc></w:tr></w:tbl>' +
    '<w:p><w:hyperlink r:id="rId1"><w:r><w:t>a link</w:t></w:r></w:hyperlink></w:p>' +
    '<w:p><w:r><w:drawing><wp:inline><wp:docPr id="1" name="dot" descr="A green bar"/><a:graphic><a:graphicData><a:blip r:embed="rId2"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p></w:body></w:document>'
Write-Parts (Join-Path $prev 'k-report.docx') ([ordered]@{
    '[Content_Types].xml' = '<Types/>'; 'word/document.xml' = $doc
    'word/_rels/document.xml.rels' = '<Relationships><Relationship Id="rId1" Target="https://example.com/" TargetMode="External"/><Relationship Id="rId2" Target="media/image1.png"/></Relationships>'
    'word/media/image1.png' = $dot })
Write-Parts (Join-Path $prev 'l-table.xlsx') ([ordered]@{
    '[Content_Types].xml' = '<Types/>'
    'xl/workbook.xml' = '<workbook xmlns:r="r"><sheets><sheet name="Totals" sheetId="1" r:id="rId1"/><sheet name="Detail" sheetId="2" r:id="rId2"/></sheets></workbook>'
    'xl/_rels/workbook.xml.rels' = '<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/sheet2.xml"/></Relationships>'
    'xl/styles.xml' = '<styleSheet><cellXfs><xf numFmtId="0"/><xf numFmtId="14"/></cellXfs></styleSheet>'
    'xl/sharedStrings.xml' = '<sst><si><t>Item</t></si><si><t>Amount</t></si><si><t>Date</t></si><si><t>rent</t></si></sst>'
    'xl/worksheets/sheet1.xml' = '<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c><c r="C1" t="s"><v>2</v></c></row><row r="2"><c r="A2" t="s"><v>3</v></c><c r="B2"><v>1250.5</v></c><c r="C2" s="1"><v>45200</v></c></row></sheetData></worksheet>'
    'xl/worksheets/sheet2.xml' = '<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>detail sheet</t></is></c></row></sheetData></worksheet>' })
$sp = { param($ph, $text) "<p:sp><p:nvSpPr><p:cNvPr id=`"2`" name=`"s`"/><p:nvPr>$ph</p:nvPr></p:nvSpPr><p:txBody>$text</p:txBody></p:sp>" }
$slide = { param($title, $body, $pic) "<p:sld xmlns:p=`"p`" xmlns:a=`"a`" xmlns:r=`"r`"><p:cSld><p:spTree>" + (& $sp '<p:ph type="title"/>' "<a:p><a:r><a:t>$title</a:t></a:r></a:p>") + (& $sp '<p:ph idx="1"/>' $body) + $pic + '</p:spTree></p:cSld></p:sld>' }
$pic = '<p:pic><p:nvPicPr><p:cNvPr id="9" name="Picture" descr="A green bar"/></p:nvPicPr><p:blipFill><a:blip r:embed="rId1"/></p:blipFill></p:pic>'
Write-Parts (Join-Path $prev 'm-deck.pptx') ([ordered]@{
    '[Content_Types].xml' = '<Types/>'
    'ppt/presentation.xml' = '<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rId2"/><p:sldId id="257" r:id="rId3"/></p:sldIdLst></p:presentation>'
    'ppt/_rels/presentation.xml.rels' = '<Relationships><Relationship Id="rId2" Target="slides/slide1.xml"/><Relationship Id="rId3" Target="slides/slide2.xml"/></Relationships>'
    'ppt/slides/slide1.xml' = (& $slide 'Soak deck' '<a:p><a:r><a:t>one</a:t></a:r></a:p><a:p><a:pPr lvl="1"/><a:r><a:t>two</a:t></a:r></a:p>' $pic)
    'ppt/slides/_rels/slide1.xml.rels' = '<Relationships><Relationship Id="rId1" Target="../media/image1.png"/></Relationships>'
    'ppt/slides/slide2.xml' = (& $slide 'The end' '<a:p><a:r><a:t>done</a:t></a:r></a:p>' '')
    'ppt/media/image1.png' = $dot })

$scroll = @($ScrollFolders | Where-Object { Test-Path $_ })
$wait = 'waitops 60000'
# Paths for the drops; `|` separates them (`;` separates script steps).
$pick = (Join-Path $src 'tree\d01\f01.txt') + '|' + (Join-Path $src 'tree\d01\f02.txt')
$back = (Join-Path $dst 'f01.txt') + '|' + (Join-Path $dst 'f02.txt')
$cycle = @(
    "nav 0 $src", "nav 1 $dst", 'sleep 1000',
    'click 0 0', 'key CtrlA', 'copy', $wait,
    'click 1 0', 'rename moved', $wait,
    'click 1 0', 'move', $wait,
    'click 0 0', 'delete', $wait,
    'click 0 0', 'copy', $wait, 'click 1 0', 'sdelete', $wait,
    'newfolder nf', $wait, 'click 1 0', 'rename nf2', $wait,
    'click 1 0', 'cc', 'click 0 0', 'cv', $wait, 'click 0 0', 'sdelete', $wait,
    'click 1 0', 'cx', 'click 0 0', 'cv', $wait, 'click 0 0', 'sdelete', $wait,
    'key CtrlR', $wait,
    # M5 drops: copy two files into the empty right pane, move them back into the left pane's
    # folder (after the tree: rows 1 and 2), delete them.
    "dropfiles 1 -1 copy $pick", $wait,
    "dropfiles 0 -1 move $back", $wait,
    'click 0 1', 'shiftclick 0 2', 'sdelete', $wait,
    # Context menus on a row (on to the classic menu, closed from the in-menu timer) and on empty
    # space (P7-M2).
    'rclick 0 0', 'waitmenu 2000', 'menuopen Show more options', 'sleep 300', 'menuclose', 'sleep 200',
    'rclickempty 1', 'waitmenu 2000', 'menuclose', 'sleep 200',
    # P2-M4: a bookmark and a tab as drop targets, the right-button drop menu, a right-button drag.
    "dropfiles side -1 copy $pick", $wait,
    "dropfiles tab 0 1 copy $pick", $wait,
    "dropfiles tab 0 1 menu copy $pick", $wait,
    "dropfiles 1 -1 menu cancel $pick", 'sleep 200',
    'dragrow 0 0 0 60 right', 'sleep 300', 'dragcancel', 'sleep 300',
    # P4-M3: the grid cycle (see the header).
    "dropfiles 1 -1 copy $pick", $wait,
    'click 1 0', 'view icons', 'click 0 0', 'view icons', 'sleep 300',
    "dropfiles 1 0 copy $pick", $wait,
    "dropfiles 1 -1 copy $pick", $wait,
    'rclick 0 0 shift', 'sleep 300', 'menuclose', 'sleep 200',
    'dragrow 0 0 0 60', 'sleep 300', 'dragcancel', 'sleep 300',
    'dragrow 0 0 0 60 right', 'sleep 300', 'dragcancel', 'sleep 300',
    'band 1 202 60 5 10', 'sleep 200',
    'view details', 'sdelete', $wait,
    'click 0 0', 'view details', 'sleep 200',
    # P6-M2: the medium-grid cycle (see the header).
    "dropfiles 1 -1 copy $pick", $wait,
    'click 1 0', 'view medium', 'sleep 300',
    "dropfiles 1 0 copy $pick", $wait,
    'dragrow 1 0 0 40', 'sleep 300', 'dragcancel', 'sleep 300',
    'band 1 202 40 5 10', 'sleep 200',
    'key ShiftRight', 'sleep 200',
    'view details', 'sdelete', $wait,
    # P6-M3: the toolbar cycle (see the header), after P7-M4's hovers and side buttons over it.
    'hover 55 22', 'sleep 700', 'hover 300 22', 'sleep 700', 'hover 545 22', 'sleep 700', 'hover 600 400',
    'backclick 1 tool', 'sleep 800', 'forwardclick 1 tool', 'sleep 800',
    'toolbar 1 newfile', 'sleep 300', 'key Return', $wait,
    'undo', $wait,
    'toolbar 1 newfolder', 'sleep 300', 'key Return', $wait,
    'click 1 0', 'toolbar 1 rename', 'sleep 300', 'key t', 'key Return', $wait,
    # A pause after the copy: another program (AMD's Radeon Software here) opens the clipboard
    # right after each change, and a paste in that instant finds it in use (P7-M2).
    'click 1 0', 'toolbar 1 copy', 'sleep 200', 'toolbar 1 paste', $wait,
    'click 1 1', 'toolbar 1 cut', 'click 0 0', 'sleep 200', 'toolbar 0 paste', $wait,
    'click 0 0', 'toolbar 0 delete', $wait,
    'click 1 0', 'sdelete', $wait,
    # P7-M1: the verb cycle (see the header).
    "dropfiles 1 -1 copy $pick", $wait,
    'click 1 0', 'menuverb 1 sleep:1500', 'wheel 0 -20 30', $wait,
    'click 1 0', 'menuverb 1 delete', $wait, 'undo', $wait,
    'click 1 1', 'menuverb 1 cut', 'sleep 200', 'key Escape', 'sleep 200',
    'click 1 0', 'menuverb 1 link', $wait,
    'click 1 0', 'key CtrlA', 'sdelete', $wait,
    # P7-M2: the Windows 11 menu cycle (see the header).
    "dropfiles 1 -1 copy $pick", $wait,
    'click 1 0', 'rclick 1 0', 'waitmenu 2000', 'menupick Delete', $wait, 'undo', $wait,
    'click 1 1', 'rclick 1 1', 'waitmenu 2000', 'menupick Cut', 'sleep 200', 'key Escape', 'sleep 200',
    'click 1 0', 'rclick 1 0', 'waitmenu 2000', 'menuopen Compress to', 'sleep 200', 'menupick ZIP file', $wait,
    'rclickempty 1', 'waitmenu 2000', 'menupick New folder', 'sleep 300', 'key Return', $wait,
    'click 1 1', 'rclick 1 1', 'waitmenu 2000', 'menuopen Open with', 'sleep 200', 'menukey Escape', 'menukey Escape', 'sleep 200',
    'click 1 0', 'key CtrlA', 'sdelete', $wait,
    # P5-M3: the spring-load into spring\inner, the crumb drop into spring, the sidebar's bottom
    # edge, the inactive tab under the spring time, and the cleanup (see the header).
    "nav 1 $spring", 'sleep 500',
    "dragover row 1 0 1300 $pick", 'sleep 800',
    "dropfiles crumb 1 -2 copy $pick", $wait,
    "dragover side bottom 300 $pick", 'sleep 200',
    "dragover tab 0 1 300 $pick", 'sleep 200',
    "nav 1 $spring", 'sleep 500', 'click 1 1', 'shiftclick 1 2', 'sdelete', $wait,
    # P7-M3: the archive cycle (see the header). Rows: a.7z and a.zip list d then a.txt; arch
    # lists a.7z, a.tar.gz, a.zip, c.txt.
    "nav 0 $dst", "nav 1 $arch\a.tar.gz", 'sleep 800', "nav 1 $arch\a.7z", 'sleep 800',
    'click 1 1', 'copy', $wait,
    'click 0 0', 'copy', 'sleep 300',
    "nav 1 $arch\a.zip", 'sleep 800', 'click 0 0', 'copy', $wait,
    'click 1 0', 'newfolder nz', $wait,
    'click 1 1', 'rename nz2', $wait,
    'click 1 1', 'delete', $wait,
    'click 1 1', 'move', $wait,
    "dropfiles 1 -1 move $dst\a.txt", $wait,
    "nav 0 $arch", 'sleep 500',
    'click 0 3', 'rclick 0 3', 'waitmenu 2000', 'menuopen Compress to', 'sleep 200', 'menupick 7z file', $wait, 'undo', $wait,
    'click 0 3', 'rclick 0 3', 'waitmenu 2000', 'menuopen Compress to', 'sleep 200', 'menupick TAR file', $wait, 'undo', $wait,
    # P8-L2: between zips and within one, a password zip among them (see the header). a.zip lists
    # d, then its files; pw.zip its one file.
    "nav 0 $arch\a.zip", "nav 1 $arch\pw.zip", 'sleep 800',
    'zippassword twinfm', 'click 0 1', 'copy', $wait,
    'click 1 0', 'rename a2.txt', $wait,
    'click 1 0', 'copy', $wait,
    'click 0 2', 'move', $wait,
    'click 1 0', 'rename a.txt', $wait,
    'click 0 1', 'cc', 'sleep 200', 'cv', $wait,
    'click 0 1', 'sdelete', $wait,
    # P8-L3: an archive inside an archive, and a 7z item into a zip (see the header). n.zip\a.zip
    # and a.7z list d, then a.txt.
    "nav 0 $arch\n.zip\a.zip", "nav 1 $dst", 'sleep 1000', 'click 0 1', 'copy', $wait, 'rmfile 1 a.txt', 'sleep 300',
    "nav 0 $arch\a.7z", "nav 1 $arch\a.zip", 'sleep 1000', 'click 0 1', 'copy', $wait,
    # P7-M4: the sidebar walk (see the header).
    'click 0 0', 'sidebar click 1', 'sleep 500', 'key Down', 'key Down', 'key Up', 'key End', 'key Home', 'key Return', 'sleep 500', 'key Escape', 'sleep 200',
    # P8-M1: the preview cycle (see the header). Rows: a-pages.pdf, b-one.pdf, c-damaged.pdf, d-notes.txt.
    "nav 0 $prev", 'sleep 500', 'preview toggle', 'click 0 0', 'waitpreview 5000', 'previewscroll 600', 'waitpreview 5000',
    # P11-M4: the zoom on a-pages.pdf.
    'previewzoom in', 'previewzoom in', 'waitpreview 5000', 'previewzoom out', 'previewzoom reset', 'waitpreview 5000',
    'key Down', 'waitpreview 5000', 'key Down', 'waitpreview 5000', 'key Down', 'waitpreview 5000',
    # P8-M2: e-dot.png, f-readme.md (its anchor), g-big.md (scrolled).
    'key Down', 'waitpreview 5000', 'key Down', 'waitpreview 5000', 'previewlink #usage', 'waitpreview 5000', 'previewcopy all', 'previewcopy 0',
    'key Down', 'waitpreview 10000', 'previewscroll 20000', 'sleep 300',
    # P9-M2: h-notes.txt, i-photo.jpg, j-page.html (its anchor).
    'key Down', 'waitpreview 5000', 'key Down', 'waitpreview 5000', 'key Down', 'waitpreview 5000', 'previewlink #end', 'waitpreview 5000',
    # P9-M3: k-report.docx, l-table.xlsx (its second sheet), m-deck.pptx.
    'key Down', 'waitpreview 5000', 'key Down', 'waitpreview 5000', 'previewlink #sheet=2', 'waitpreview 5000', 'key Down', 'waitpreview 5000',
    # P11-M5: n-code.rs and o-script.ps1, coloured.
    'key Down', 'waitpreview 5000', 'key Down', 'waitpreview 5000',
    # P11-M6: p-locked.pdf, its password from the script.
    'key Down', 'waitpreview 5000', 'pdfpassword twinfm', 'waitpreview 5000',
    'preview width 0.5', 'waitpreview 5000', 'preview width 0.35', 'waitpreview 5000', 'preview toggle', 'sleep 200',
    # P8-L1: the grid's PageDown and Return on two folders (see the header).
    "nav 0 $src\tree", 'sleep 500', 'click 0 0', 'view medium', 'sleep 300', 'key PageDown', 'sleep 100', 'key PageUp', 'sleep 100',
    'view details', 'sleep 300', 'click 0 0', 'shiftclick 0 1', 'key Return', 'sleep 800', 'tab 2', 'sleep 200', 'tab close', 'sleep 300', 'tab 2', 'sleep 200', 'tab close', 'sleep 300',
    'tab 1', 'sleep 300',
    # P9-M1: a grip drag, a fit, and the widths set back (see the header).
    "nav 0 $src\tree", 'sleep 500', 'coldrag 0 size 30', 'colfit 0 modified', 'colwidth 0 modified 140', 'colwidth 0 size 90', 'sleep 200',
    # P9-M4: a thumbnail drag and a search tab with thumbnails (see the header).
    'settings open', 'settings set thumbnails true', 'settings ok', 'sleep 300', "nav 0 $prev", 'sleep 1500', 'click 0 8', 'sleep 800',
    'dragrow 0 8 40 0', 'sleep 400', 'dragcancel', 'sleep 300', 'filter 0 photo', 'sleep 300', 'search 0', 'waitsearch 5000', 'sleep 1500',
    'tab close', 'sleep 300', 'filter 0', 'sleep 300', 'settings open', 'settings set thumbnails false', 'settings ok', 'sleep 300',
    # P10-M2: the search box: Return with text (a search tab), the button with text (another), Return
    # with the box blank (the list takes the focus, no tab); see the header.
    'filter 0 notes', 'sleep 300', 'searchreturn 0', 'waitsearch 5000', 'sleep 800', 'tab close', 'sleep 300',
    'filter 0 notes', 'sleep 300', 'searchbutton 0', 'waitsearch 5000', 'sleep 800', 'tab close', 'sleep 300',
    'filter 0', 'sleep 200', 'searchreturn 0', 'sleep 300',
    # P11-M1: a content search in src (see the header).
    "nav 0 $src", 'sleep 500', 'csearch 0 scratch file 3/7', 'waitsearch 5000', 'sleep 800', 'tab close', 'sleep 300',
    'contentmode 0 off', 'filter 0', 'sleep 300',
    # P11-M2: the widths reset through the dialog (see the header).
    "nav 0 $src\tree", 'sleep 500', 'colwidth 0 type 100', 'sleep 200', 'settings open', 'settings set cols_reset', 'settings ok', 'sleep 300',
    # P11-M3: the history menu (see the header).
    "nav 0 $src", 'sleep 400', "nav 0 $src\tree", 'sleep 400', 'histmenu 0', 'waitmenu 2000', 'histpick 1', 'sleep 600',
    # P12-M4: comfortable rows on and off (see the header).
    "nav 0 $src\tree", 'sleep 400', 'settings open', 'settings set row_height comfortable', 'settings ok', 'sleep 400',
    'click 0 2', 'key PageDown', 'sleep 300', 'settings open', 'settings set thumbnails true', 'settings ok', 'sleep 1200',
    'click 0 2', 'key PageDown', 'sleep 300', 'settings open', 'settings set row_height compact', 'settings ok', 'sleep 400',
    'settings open', 'settings set thumbnails false', 'settings ok', 'sleep 800',
    # P12-M5: a bookmark by a drop, dragged, removed; a tab by the "+" (see the header).
    "dropfiles sidegap 0 auto $src", 'sleep 400', 'bookmark drag 0 1', 'sleep 300', 'bookmark remove 1', 'sleep 300',
    'tab plus 0', 'sleep 400', 'tab close', 'sleep 300',
    'click 0 0', 'sleep 200', 'key Tab', 'sleep 300', 'key ShiftTab', 'sleep 300',
    # P13-M1: Two panes off and on by the toolbar, the split line narrowed and widened (see the header).
    'toolbar 0 split', 'sleep 500', 'toolbar 0 split', 'sleep 1000', 'splitter 0.35', 'sleep 400', 'splitter 0.5', 'sleep 400'
)
if ($scroll.Count -ge 1) { $cycle += @("nav 0 $($scroll[0])", 'sleep 1500', 'wheel 0 -40 30') }
if ($scroll.Count -ge 2) { $cycle += @("nav 1 $($scroll[1])", 'sleep 1500', 'wheel 1 -40 30') }
if ($scroll.Count -ge 3) { $cycle += @("nav 1 $($scroll[2])", 'sleep 1500', 'drag 1 300 1000') }
# Setup, before the split (the left pane is the active one): bookmark bm (the sidebar's last row),
# a second tab on tabdir in the left pane (Ctrl+1 goes back to its tab 0), then the split.
$steps = @('sleep 1500', "nav 0 $bm", 'sleep 500', 'bookmark add', 'tab new', "nav 0 $tabdir", 'sleep 500',
    'tab 1', 'sleep 500', 'split', 'sleep 1000')
for ($c = 0; $c -lt $Cycles; $c++) { $steps += $cycle }
$steps += 'quit'
$scriptFile = Join-Path $root 'soak-p5m3-script.txt'
[System.IO.File]::WriteAllText($scriptFile, ($steps -join ';'))

$stderr = Join-Path $root 'soak-p5m3-stderr.txt'
$csv = Join-Path $root 'soak-p5m3-memory.csv'
$env:TWINFM_SCRIPT = "@$scriptFile"
$env:TWINFM_SOAK = '1'
try {
    $p = Start-Process -FilePath $Exe -ArgumentList 'C:\' -RedirectStandardError $stderr -PassThru
    $null = $p.Handle # keep the handle so ExitCode is available after exit
} finally {
    $env:TWINFM_SOAK = $null
    $env:TWINFM_SCRIPT = $null
}
$t0 = Get-Date
$deadline = $t0.AddMinutes($Minutes)
$samples = New-Object System.Collections.Generic.List[object]
function Sample {
    $p.Refresh()
    $samples.Add([pscustomobject]@{
        seconds        = [int]((Get-Date) - $t0).TotalSeconds
        working_set_mb = [math]::Round($p.WorkingSet64 / 1MB, 1)
        private_mb     = [math]::Round($p.PrivateMemorySize64 / 1MB, 1)
    })
}
Start-Sleep -Seconds 5
if (-not $p.HasExited) { Sample }
$next = (Get-Date).AddSeconds(30)
while (-not $p.HasExited -and (Get-Date) -lt $deadline) {
    Start-Sleep -Seconds 1
    if ((Get-Date) -ge $next -and -not $p.HasExited) { Sample; $next = $next.AddSeconds(30) }
}
$closed = $false
if (-not $p.HasExited) {
    Sample
    $closed = [SoakWinP5m3]::Close([uint32]$p.Id)
    if (-not $p.WaitForExit(60000)) { $p.Kill(); $p.WaitForExit(); $closed = 'killed' }
}
$samples | Export-Csv -NoTypeInformation -Path $csv

$log = Get-Content $stderr
$panics = @($log | Select-String -SimpleMatch 'panicked').Count
$done = @($log | Select-String -SimpleMatch '[fileops] done')
$failed = @($done | Select-String -SimpleMatch 'Failed')
$cancelled = @($done | Select-String -SimpleMatch 'Cancelled')
$refused = @($log | Select-String -SimpleMatch 'soak: refused').Count
$stuck = @($log | Select-String -SimpleMatch 'waitops: still busy').Count
$cyclesRun = @($log | Select-String -SimpleMatch "[auto] Nav { pane: 0, path: `"$($src.Replace('\', '\\'))`" }").Count
$drops = @($log | Select-String -SimpleMatch -CaseSensitive '[auto] dropfiles')
$dropCopy = @($drops | Select-String -SimpleMatch 'Row(1, -1) copy: 2 path(s), DragEnter effect copy, Drop reported copy').Count
$dropMove = @($drops | Select-String -SimpleMatch 'Row(0, -1) move: 2 path(s), DragEnter effect move, Drop reported none').Count
$dropSide = @($drops | Select-String -SimpleMatch 'Side(-1) copy: 2 path(s), DragEnter effect copy, Drop reported copy').Count
$dropTab = @($drops | Select-String -SimpleMatch 'Tab(0, 1) copy: 2 path(s), DragEnter effect copy, Drop reported copy').Count
$menuCopy = @($drops | Select-String -SimpleMatch 'Tab(0, 1) menu copy: 2 path(s), DragEnter effect move, Drop reported copy').Count
$menuCancel = @($drops | Select-String -SimpleMatch 'Row(1, -1) menu cancel: 2 path(s), DragEnter effect move, Drop reported none').Count
$dropJobs = @($log | Select-String -SimpleMatch '[dnd] drop into pane').Count
$dropMenus = @($log | Select-String -SimpleMatch '[dnd] drop menu:').Count
$dropMenuBusy = @($log | Select-String -SimpleMatch '[dnd] drop menu loop idle: app state free=false').Count
$rightDrags = @($log | Select-String -SimpleMatch 'DoDragDrop starts (right button)').Count
$dragsCancelled = @($log | Select-String -SimpleMatch '(cancelled)').Count
$dragsDropped = @($log | Select-String -SimpleMatch '(dropped)').Count
$dragBusy = @($log | Select-String -SimpleMatch 'inside the drag loop: app state free=false').Count
$images = @($log | Select-String -SimpleMatch 'InitializeFromBitmap = 0x00000000').Count
# P4-M3 counts.
$gridOn = @($log | Select-String -SimpleMatch 'details -> large icons').Count
$gridOff = @($log | Select-String -SimpleMatch 'large icons -> details').Count
$dropTile = @($drops | Select-String -SimpleMatch 'Row(1, 0) copy: 2 path(s), DragEnter effect copy, Drop reported copy').Count
$tileImages = @($log | Select-String -SimpleMatch 'drag image 128x128 px, 96 px icon').Count
$leftDrags = @($log | Select-String -SimpleMatch 'DoDragDrop starts (left button)').Count
$bandsStarted = @($log | Select-String -SimpleMatch '[band] pane=1 start at').Count
$bandsDone = @($log | Select-String -SimpleMatch '[band] pane=1 done: 2 selected').Count
$bandsCancelled = @($log | Select-String -SimpleMatch '[band] pane=1 cancelled').Count
# P6-M2 counts.
$mediumOn = @($log | Select-String -SimpleMatch 'details -> medium icons').Count
$mediumOff = @($log | Select-String -SimpleMatch 'medium icons -> details').Count
$mediumImages = @($log | Select-String -SimpleMatch 'drag image 80x80 px, 48 px icon').Count
$bandsCursor = @($log | Select-String -SimpleMatch '[band] pane=1 done: 2 selected, cursor 0').Count
$shiftFromCursor = @($log | Select-String -SimpleMatch '[select] pane=1 Range row=1 selected=2').Count
# P6-M3 counts.
$toolPresses = @($log | Select-String -SimpleMatch '[toolbar] pane=' | Select-String -SimpleMatch -NotMatch ' tooltip "').Count
$toolDisabled = @($log | Select-String -SimpleMatch 'disabled, not pressed').Count
$newFiles = @($log | Select-String -SimpleMatch 'Done: Created New Text Document.txt').Count
$newFilesUndone = @($log | Select-String -SimpleMatch '[undo] done' | Select-String -SimpleMatch 'Undid: Created New Text Document.txt').Count
$renamedT = @($log | Select-String -SimpleMatch 'Done: Renamed to t').Count
# P7-M4 counts.
$tooltips = @($log | Select-String -SimpleMatch '[toolbar] top tooltip "').Count
$toolBack = @($log | Select-String -SimpleMatch '[nav] pane=1 Back button').Count
$toolForward = @($log | Select-String -SimpleMatch '[nav] pane=1 Forward button').Count
$sideKeys = @($log | Select-String -SimpleMatch '[sidebar] key ').Count
$sideReturns = @($log | Select-String -SimpleMatch '[sidebar] Return on row 1').Count
# P8-L1 counts.
$openMany = @($log | Select-String -SimpleMatch '[open] pane=0 2 items: 2 folder(s) to tabs').Count
$openTabs = @($log | Select-String -SimpleMatch '[tab] pane=0 opened tab' | Select-String -SimpleMatch 'twinfm-m5\src\tree\d0').Count
# P7-M1 counts.
$verbJobs = @($log | Select-String -SimpleMatch '[menu] verb "' | Select-String -SimpleMatch 'queued on the verb worker').Count
$verbSlept = @($log | Select-String -SimpleMatch 'slept in').Count
$verbOk = @($log | Select-String -SimpleMatch 'verb="link"' | Select-String -SimpleMatch 'by verb = 0x00000000').Count
$routedDeletes = @($log | Select-String -SimpleMatch 'menuverb pane=1 "delete"').Count
$restores = @($log | Select-String -SimpleMatch '[undo] done' | Select-String -SimpleMatch 'Undid: Deleted 1 item to the Recycle Bin').Count
$cutsCancelled = @($log | Select-String -SimpleMatch 'Escape: cut cancelled, marks cleared, clipboard emptied').Count
# P7-M2 counts.
$wmShown = @($log | Select-String -SimpleMatch '[winmenu] pane=' | Select-String -SimpleMatch ' shown after ').Count
$wmRebuilt = @($log | Select-String -SimpleMatch "rebuilt with the worker's answer").Count
$wmPicks = @($log | Select-String -SimpleMatch '[winmenu] pane=' | Select-String -SimpleMatch ' runs ').Count
$wmZips = @($log | Select-String -SimpleMatch 'label="Send to>Compressed (zipped) folder" by offset = 0x00000000').Count
$wmMore = @($log | Select-String -SimpleMatch 'closed (Show more options)').Count
$wmSubs = @($log | Select-String -SimpleMatch '[winmenu] submenu "').Count
$wmRefused = @($log | Select-String -SimpleMatch 'menupick refused').Count
$wmFolders = @($log | Select-String -SimpleMatch 'Done: Created New folder').Count
# P5-M3 counts.
$springArmed = @($log | Select-String -SimpleMatch '[dnd] spring armed:').Count
$springRows = @($log | Select-String -SimpleMatch '[dnd] spring-loaded:' | Select-String -SimpleMatch ' opens ').Count
$springTabs = @($log | Select-String -SimpleMatch '[dnd] spring-loaded:' | Select-String -SimpleMatch ' shows tab ').Count
$springDisarmed = @($log | Select-String -SimpleMatch '[dnd] spring disarmed').Count
$springInner = @($log | Select-String -SimpleMatch '[nav] pane=1 gen=' | Select-String -SimpleMatch "path=$(Join-Path $spring 'inner')").Count
$dropCrumb = @($drops | Select-String -SimpleMatch 'Crumb(1, -2) copy: 2 path(s), DragEnter effect copy, Drop reported copy').Count
$sideScrolls = @($log | Select-String -SimpleMatch '[dnd] auto-scroll sidebar').Count
$stripScrolls = @($log | Select-String -SimpleMatch 'tab strip left', 'tab strip right').Count
# P7-M3 counts.
$arLevels = @($log | Select-String -SimpleMatch '(archive level, ArchiveFolder, listed in').Count
$arJobs = @($log | Select-String -SimpleMatch '[archive] job id=')
$arKinds = ('copy in', 'new folder', 'rename', 'delete', 'move out', 'move in' | ForEach-Object { "$_ " + @($arJobs | Select-String -SimpleMatch "`"$_`"" | Select-String -SimpleMatch ': Done in').Count }) -join ', '
$arFailed = @($arJobs | Select-String -SimpleMatch -NotMatch ': Done in').Count
$fromZip = @($arJobs | Select-String -SimpleMatch '"copy from a zip"' | Select-String -SimpleMatch ': Done in').Count
$fromZipMoves = @($arJobs | Select-String -SimpleMatch '"move from a zip"' | Select-String -SimpleMatch ': Done in').Count
$pwScript = @($log | Select-String -SimpleMatch '[archive] password for' | Select-String -SimpleMatch 'from the script').Count
$pwRight = @($log | Select-String -SimpleMatch '[archive] password for' | Select-String -SimpleMatch ': right').Count
$nestedIds = @($log | Select-String -SimpleMatch '[fileops] submit id=' | Select-String -SimpleMatch 'n.zip\\a.zip\\' | ForEach-Object { [regex]::Match($_.Line, 'id=(\d+)').Groups[1].Value })
$nestedOut = @($log | Select-String -SimpleMatch '[fileops] done id=' | Where-Object { $nestedIds -contains [regex]::Match($_.Line, 'done id=(\d+)').Groups[1].Value -and $_.Line -match ' Done: ' }).Count
$fromShell = @($arJobs | Select-String -SimpleMatch '"copy from an archive"' | Select-String -SimpleMatch ': Done in').Count
$arRefused = @($log | Select-String -SimpleMatch 'cannot write into .7z archives' | Select-String -SimpleMatch '[archive] refused').Count
$arCompressed = @($log | Select-String -SimpleMatch '[archive] compress' | Select-String -SimpleMatch ': Ok(').Count
$arUndone = @($log | Select-String -SimpleMatch '[undo] done' | Select-String -SimpleMatch 'Undid: Compressed').Count
# P8-M1 counts.
$pvOpened = @($log | Select-String -SimpleMatch '[preview] pane=' | Select-String -SimpleMatch ': pdf, ').Count
$pvPages = @($log | Select-String -SimpleMatch '[preview] gen=' | Select-String -SimpleMatch ' rendered ').Count
$pvDamaged = @($log | Select-String -SimpleMatch '[preview] pane=' | Select-String -SimpleMatch ': damaged: ').Count
$pvOther = @($log | Select-String -SimpleMatch '[preview] pane=' | Select-String -SimpleMatch ': no preview for this type').Count
$pvMd = @($log | Select-String -SimpleMatch '[preview] pane=' | Select-String -SimpleMatch ': markdown, ').Count
$pvText = @($log | Select-String -SimpleMatch '[preview] pane=' | Select-String -SimpleMatch ': text, ').Count
$pvPicture = @($log | Select-String -Pattern '\[preview\] pane=.*: picture \d+x\d+ px, ').Count
$pvPicFailed = @($log | Select-String -SimpleMatch ': picture failed: ').Count
$pvHtml = @($log | Select-String -SimpleMatch '[preview] pane=' | Select-String -SimpleMatch ': html, ').Count
$pvDocx = @($log | Select-String -SimpleMatch '[preview] pane=' | Select-String -SimpleMatch ': docx, ').Count
$pvXlsx = @($log | Select-String -SimpleMatch '[preview] pane=' | Select-String -SimpleMatch ': xlsx, ').Count
$pvPptx = @($log | Select-String -SimpleMatch '[preview] pane=' | Select-String -SimpleMatch ': pptx, ').Count
$pvSheets = @($log | Select-String -Pattern '\[preview\] link "#sheet=\d+": sheet ').Count
$pvOfficeBad = @($log | Select-String -Pattern '\[preview\] pane=.*\.(docx|xlsx|pptx): (unreadable|password)').Count
$pvPics = 0; $pvAlt = 0
foreach ($m in ($log | Select-String -Pattern 'pictures done: (\d+) shown, (\d+) alt text')) { $pvPics += [int]$m.Matches[0].Groups[1].Value; $pvAlt += [int]$m.Matches[0].Groups[2].Value }
$pvFell = @($log | Select-String -SimpleMatch 'styled text fell back to plain').Count
$pvCut = @($log | Select-String -SimpleMatch ', cut at 2 MB').Count
$pvSuperseded = @($log | Select-String -SimpleMatch '[preview] gen=' | Select-String -SimpleMatch ' superseded: ').Count
$pvTimeouts = @($log | Select-String -SimpleMatch 'waitpreview: not settled').Count
$thumbDrags = @($log | Select-String -SimpleMatch 'icon from the thumbnail').Count
$thumbSearches = @($log | Select-String -SimpleMatch '[search] pane=0' | Select-String -SimpleMatch 'start "photo"').Count
# P10-M2: the search box's Return and button searches, and the blank Return.
$boxReturns = @($log | Select-String -SimpleMatch '[search] pane=0 box Return: searches "notes"').Count
$boxButtons = @($log | Select-String -SimpleMatch '[search] pane=0 search button').Count
$boxEmpty = @($log | Select-String -SimpleMatch '[search] pane=0 box Return: empty, the list takes the focus').Count
# P11-M1: the content searches, the files they read and found.
$contentDone = @($log | Select-String -Pattern '^\[search\] pane=0 gen=\d+ done: (\d+) found in (\d+) files read')
$contentRead = ($contentDone | ForEach-Object { [int]$_.Matches[0].Groups[2].Value } | Measure-Object -Sum).Sum
$contentFound = ($contentDone | ForEach-Object { [int]$_.Matches[0].Groups[1].Value } | Measure-Object -Sum).Sum
$colDrags = @($log | Select-String -SimpleMatch '[cols] pane=0 size 90 -> 60 px (drag)').Count
$colFits = @($log | Select-String -SimpleMatch '[cols] pane=0 fit modified: ').Count
$colResets = @($log | Select-String -SimpleMatch '[cols] pane=0 widths reset to 120, 90, 140').Count + @($log | Select-String -SimpleMatch '[cols] pane=1 widths reset to 120, 90, 140').Count
$histJumps = @($log | Select-String -SimpleMatch '[nav] pane=0 history jump -1:').Count
# P12-M4: comfortable rows.
$rowsOn = @($log | Select-String -SimpleMatch '[settings] dialog: row_height compact -> comfortable').Count
$rowsOff = @($log | Select-String -SimpleMatch '[settings] dialog: row_height comfortable -> compact').Count
$rows30 = @($log | Select-String -SimpleMatch '[icons] list geometry: 30 px rows, 16 px icons').Count
$rows44 = @($log | Select-String -SimpleMatch '[icons] list geometry: 44 px rows, 32 px icons').Count
# P12-M5: bookmarks by mouse and the "+".
$bmDropped = @($log | Select-String -SimpleMatch "[sidebar] bookmark added $src at 0 (drop)").Count
$bmDragged = @($log | Select-String -SimpleMatch '[sidebar] bookmark moved 0 -> 1 (drag)').Count
$bmRemoved = @($log | Select-String -SimpleMatch '[sidebar] removed bookmark 1: ').Count
$plusTabs = @($log | Select-String -SimpleMatch '[tab] pane=0 + button: new tab').Count
$tabRight = @($log | Select-String -SimpleMatch '[focus] pane=1 has the keys by Tab').Count
$tabLeft = @($log | Select-String -SimpleMatch '[focus] pane=0 has the keys by Tab').Count
# P13-M1: Two panes and the columns making way.
$splitPresses = @($log | Select-String -SimpleMatch '[toolbar] pane=0 split -> toggle').Count
$colsNarrow = @($log | Select-String -SimpleMatch '[cols] pane=0 shows name, modified at ').Count
$colsWide = @($log | Select-String -SimpleMatch '[cols] pane=0 shows name, size, modified at ').Count
$colBack = @($log | Select-String -SimpleMatch '[cols] pane=0 widths type 120 size 90 modified 140 ').Count
$pvPanics = @($log | Select-String -SimpleMatch '[preview] worker panicked').Count
# P11-M4: the zoom and "Copy text".
$pvZooms = @($log | Select-String -Pattern '\[preview\] zoom \d+%: ').Count
$pvCopies = @($log | Select-String -Pattern '\[preview\] copy (all|block \d+): \d+ characters').Count
$pvCopyBad = @($log | Select-String -Pattern '\[preview\] copy( (all|block \d+): failed|: no text shown)').Count
# P11-M5: the syntax colours.
$pvColoured = @($log | Select-String -Pattern '\[preview\] gen=\d+ coloured \d+ chunk').Count
$pvColourBad = @($log | Select-String -SimpleMatch 'colours refused, plain kept').Count
# P11-M6: password PDFs.
$pvPwRight = @($log | Select-String -SimpleMatch ': pdf password right').Count
$pvPwBad = @($log | Select-String -Pattern ': pdf password (wrong|refused)').Count
$pvPwOpened = @($log | Select-String -SimpleMatch 'p-locked.pdf: pdf, ').Count
$menus = @($log | Select-String -SimpleMatch '[menu] closed').Count
$ticks = @($log | Select-String -SimpleMatch '[auto] in-menu tick')
$ticksBusy = @($ticks | Select-String -SimpleMatch 'free=false').Count
$first = $samples[0]; $last = $samples[$samples.Count - 1]
''
"ran {0:N1} min, window closed by script: {1}, exit code {2}" -f ((Get-Date) - $t0).TotalMinutes, $closed, $p.ExitCode
"panicked lines: $panics"
"cycles started: $cyclesRun; jobs done: $($done.Count) (failed $($failed.Count), cancelled $($cancelled.Count)); soak refusals: $refused; waitops timeouts: $stuck"
"drops: $($drops.Count) (pane copy $dropCopy, pane move $dropMove, bookmark copy $dropSide, tab copy $dropTab, menu copy $menuCopy, menu cancel $menuCancel, as expected: pane copy is three per cycle here), drop jobs submitted: $dropJobs"
"drop menus: $dropMenus (loops that found the app state busy: $dropMenuBusy)"
"right-button drags: $rightDrags; drags cancelled: $dragsCancelled, dropped: $dragsDropped (must be 0); drag loops that found the state busy: $dragBusy; drag images set: $images"
"menus opened and closed: $menus; in-menu ticks: $($ticks.Count), with state or driver busy: $ticksBusy"
"grid: switched on $gridOn, off $gridOff; tile drops: $dropTile (two per cycle: large and medium); 128 px drag images: $tileImages (left-button drags started: $leftDrags); bands: $bandsStarted started, $bandsDone done with 2 selected (two per cycle), $bandsCancelled cancelled"
"medium grid: switched on $mediumOn, off $mediumOff; 80 px drag images: $mediumImages; bands done with the cursor on tile 0: $bandsCursor (two per cycle); Shift+Right from the band's cursor: $shiftFromCursor"
"toolbar: presses $toolPresses (eight per cycle), refused as disabled $toolDisabled (must be 0); new files $newFiles, undone $newFilesUndone; renamed to t $renamedT"
"toolbar tooltips shown $tooltips (three per cycle); Back over the toolbar $toolBack, Forward $toolForward (one each per cycle); sidebar keys $sideKeys (five per cycle), Return $sideReturns (one per cycle)"
"several-item opens $openMany (one per cycle), tabs they opened $openTabs (two per cycle)"
"verbs: jobs queued $verbJobs (three per cycle: two of P7-M1's, P7-M2's ZIP), slept $verbSlept, link by verb with S_OK $verbOk; routed deletes $routedDeletes, restored $restores; cuts cancelled by Escape $cutsCancelled"
"Windows 11 menu: shown $wmShown (seven per cycle, and each rebuild again), rebuilt with the worker's answer $wmRebuilt, picks run $wmPicks (four per cycle), ZIPs made $wmZips, new folders $wmFolders (two per cycle: the menu's and the toolbar's), submenus $wmSubs (two per cycle), classic menus from Show more options $wmMore, menupicks refused $wmRefused (must be 0)"
"spring: armed $springArmed, spring-loaded rows $springRows (navigations into spring\inner: $springInner), tabs $springTabs (must be 0), disarmed $springDisarmed; crumb drops: $dropCrumb; auto-scroll lines: sidebar $sideScrolls, tab strip $stripScrolls"
"working set: start $($first.working_set_mb) MB at $($first.seconds) s, end $($last.working_set_mb) MB at $($last.seconds) s; private: $($first.private_mb) -> $($last.private_mb) MB"
"min/max working set: {0} / {1} MB" -f ($samples | Measure-Object working_set_mb -Minimum).Minimum, ($samples | Measure-Object working_set_mb -Maximum).Maximum
"memory samples: $csv"
"stderr: $stderr"
"between zips: copies from a zip $fromZip (three per cycle), moves from a zip $fromZipMoves (one per cycle); passwords from the script $pwScript, right $pwRight (one each per cycle)"
"archives inside archives: copies out $nestedOut, copies from an archive into a zip $fromShell (one each per cycle)"
"archives: levels listed by ArchiveFolder $arLevels; zip jobs done: $arKinds (one each per cycle); zip jobs not done $arFailed (must be 0); writes into the 7z refused $arRefused (one per cycle); compressed $arCompressed (two per cycle), undone $arUndone"
"preview: PDFs opened $pvOpened (three per cycle), pages rendered $pvPages, damaged $pvDamaged (one per cycle), no preview for the type $pvOther (0 since P9-M2), markdown $pvMd (two per cycle), text $pvText (four per cycle), pictures $pvPicture (two per cycle), picture failures $pvPicFailed (must be 0), html $pvHtml (one per cycle), docx $pvDocx, xlsx $pvXlsx (two per cycle), pptx $pvPptx, sheet links $pvSheets (one per cycle), Office files unreadable $pvOfficeBad (must be 0), pictures shown $pvPics (f-readme.md's one, g-big.md's repeats, the file decoded once, j-page.html's one and the Word and PowerPoint files' one each), alt text $pvAlt (must be 0), styled-text fall-backs $pvFell, cut at 2 MB $pvCut (one per cycle), superseded lines $pvSuperseded, waitpreview timeouts $pvTimeouts (must be 0), worker panics $pvPanics (must be 0), zooms $pvZooms (four per cycle), copies $pvCopies (two per cycle), copy failures $pvCopyBad (must be 0), files coloured $pvColoured (two per cycle), chunks refused $pvColourBad (must be 0), passwords right $pvPwRight (one per cycle), wrong or refused $pvPwBad (must be 0), password PDFs opened $pvPwOpened (one per cycle)"
"column widths: drags read back as Size 90 -> 60 $colDrags (one per cycle), fits $colFits (one per cycle), widths back at 120, 90, 140 $colBack (two per cycle and one at start), resets through the dialog $colResets (two per cycle)"
"thumbnails beyond the listing: drag images from the thumbnail $thumbDrags (one per cycle), searches for photo $thumbSearches (one per cycle)"
"search box: Return searches $boxReturns (one per cycle), button searches $boxButtons (one per cycle), blank Returns $boxEmpty (one per cycle)"
"history menu: jumps back one place $histJumps (one per cycle)"
"comfortable rows: on $rowsOn, off $rowsOff, 30 px geometry lines $rows30, 44 px (thumbnails) $rows44 (one each per cycle)"
"bookmarks by mouse: added by a drop $bmDropped, dragged $bmDragged, removed $bmRemoved; tabs by the + $plusTabs (one each per cycle)"
"Tab between the panes: to the right $tabRight, back $tabLeft (one each per cycle)"
"columns making way: Two panes presses $splitPresses (two per cycle); left pane narrowed to name and Modified $colsNarrow (one per cycle), name, Size and Modified $colsWide (two per cycle at 100%)"
"content search: done $($contentDone.Count) (one per cycle), files read $contentRead (200 or more per cycle), found $contentFound (one per cycle)"
if ($failed.Count -gt 0) { 'failures:'; $failed | Select-Object -First 5 | ForEach-Object { "  $_" } }
