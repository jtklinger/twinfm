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
#
# P8-L1, after the preview cycle: the left pane on src\tree (ten folders) goes to medium tiles,
# PageDown and PageUp move its cursor by grid rows (Explorer's), it goes back to details, two
# folders are selected and Return opens both as new tabs (placed after the active tab 0), Ctrl+2
# and Ctrl+W twice close them, and Ctrl+1 makes the pane's first tab active again. Reported:
# several-item opens (one per cycle) and the tabs they opened (two per cycle).
#
# P9-M1, after the P8-L1 cycle: in the left pane on src	ree (details, the default widths 120, 90,
# 140), the Type | Size grip is dragged 30 px right (Type 150, Size 60), the Modified column is
# fitted by a double-click on its grip (Size takes the difference), then colwidth sets Modified to
# 140 and Size to 90, which puts every width back. Reported: drags read back as Size 90 -> 60 (one
# per cycle), fits (one per cycle), and widths lines back at 120, 90, 140 (one per cycle).
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
    'hover 55 18', 'sleep 700', 'hover 300 18', 'sleep 700', 'hover 545 18', 'sleep 700', 'hover 600 400',
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
    'key Down', 'waitpreview 5000', 'key Down', 'waitpreview 5000', 'key Down', 'waitpreview 5000',
    # P8-M2: e-dot.png, f-readme.md (its anchor), g-big.md (scrolled).
    'key Down', 'waitpreview 5000', 'key Down', 'waitpreview 5000', 'previewlink #usage', 'waitpreview 5000',
    'key Down', 'waitpreview 10000', 'previewscroll 20000', 'sleep 300',
    'preview width 0.5', 'waitpreview 5000', 'preview width 0.35', 'waitpreview 5000', 'preview toggle', 'sleep 200',
    # P8-L1: the grid's PageDown and Return on two folders (see the header).
    "nav 0 $src\tree", 'sleep 500', 'click 0 0', 'view medium', 'sleep 300', 'key PageDown', 'sleep 100', 'key PageUp', 'sleep 100',
    'view details', 'sleep 300', 'click 0 0', 'shiftclick 0 1', 'key Return', 'sleep 800', 'tab 2', 'sleep 200', 'tab close', 'sleep 300', 'tab 2', 'sleep 200', 'tab close', 'sleep 300',
    'tab 1', 'sleep 300',
    # P9-M1: a grip drag, a fit, and the widths set back (see the header).
    "nav 0 $src	ree", 'sleep 500', 'coldrag 0 size 30', 'colfit 0 modified', 'colwidth 0 modified 140', 'colwidth 0 size 90', 'sleep 200'
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
$pvPics = 0; $pvAlt = 0
foreach ($m in ($log | Select-String -Pattern 'pictures done: (\d+) shown, (\d+) alt text')) { $pvPics += [int]$m.Matches[0].Groups[1].Value; $pvAlt += [int]$m.Matches[0].Groups[2].Value }
$pvFell = @($log | Select-String -SimpleMatch 'styled text fell back to plain').Count
$pvCut = @($log | Select-String -SimpleMatch ', cut at 2 MB').Count
$pvSuperseded = @($log | Select-String -SimpleMatch '[preview] gen=' | Select-String -SimpleMatch ' superseded: ').Count
$pvTimeouts = @($log | Select-String -SimpleMatch 'waitpreview: not settled').Count
$colDrags = @($log | Select-String -SimpleMatch '[cols] pane=0 size 90 -> 60 px (drag)').Count
$colFits = @($log | Select-String -SimpleMatch '[cols] pane=0 fit modified: ').Count
$colBack = @($log | Select-String -SimpleMatch '[cols] pane=0 widths type 120 size 90 modified 140 ').Count
$pvPanics = @($log | Select-String -SimpleMatch '[preview] worker panicked').Count
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
"preview: PDFs opened $pvOpened (two per cycle), pages rendered $pvPages, damaged $pvDamaged (one per cycle), no preview for the type $pvOther (two per cycle), markdown $pvMd (two per cycle), pictures shown $pvPics (f-readme.md's one and g-big.md's repeats, the file decoded once), alt text $pvAlt (must be 0), styled-text fall-backs $pvFell, cut at 2 MB $pvCut (one per cycle), superseded lines $pvSuperseded, waitpreview timeouts $pvTimeouts (must be 0), worker panics $pvPanics (must be 0)"
"column widths: drags read back as Size 90 -> 60 $colDrags (one per cycle), fits $colFits (one per cycle), widths back at 120, 90, 140 $colBack (one per cycle and one at start)"
if ($failed.Count -gt 0) { 'failures:'; $failed | Select-Object -First 5 | ForEach-Object { "  $_" } }
