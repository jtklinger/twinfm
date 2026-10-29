# TwinFM

A dual-pane file manager for Windows 11, written in Rust with [Slint](https://slint.dev). It is
built to stay smooth on very large folders: a 40,000-file folder lists, scrolls, sorts and splits
without stalling, where Windows' own Files app struggles.

TwinFM uses the Windows shell for the things Explorer users rely on, so copies, moves, the Recycle
Bin, the context menu, drag and drop, the clipboard and file icons behave as they do in Explorer.

**Status: 0.3.0.** TwinFM is used daily by its author, but it is young: expect rough edges, and
keep backups as you would with any file manager.

## Features

- **Two panes, each with tabs.** Every tab keeps its own folder, sort, hidden-files setting,
  selection, scroll position and Back/Forward history (Alt+Left, Alt+Right, the mouse's side
  buttons; Alt+Down lists the places to jump to). The "+" after the tabs opens a new one, and Tab
  moves the keyboard to the other pane. Tabs with their history, the split and the window are
  restored at the next start.
- **Fast listings.** Folders are read on a worker thread and streamed in; icons and thumbnails are
  fetched only for the rows on screen. Folders are watched, so changes made elsewhere appear
  without a refresh.
- **Views:** details (with adjustable column widths, and compact or comfortable rows), large icons
  and medium icons, with thumbnails, cloud status marks for OneDrive (syncing and errors too) and
  Google Drive, and a rubber-band selection.
- **File operations through the shell** (`IFileOperation`): copy, move, rename, new folder, new
  file, delete to the Recycle Bin or permanently, with Explorer's progress and conflict dialogs,
  and **undo and redo** (Ctrl+Z, Ctrl+Y), including restoring from the Recycle Bin.
- **Drag and drop** between the panes, tabs, the sidebar, breadcrumbs and other programs, with
  spring-loaded folders.
- **A Windows 11 style context menu** with the packaged apps' commands, Open with, Compress to
  ZIP, 7z or TAR, and "Show more options" for the classic shell menu.
- **Archives as folders:** zip (read and written), and 7z, RAR, tar, tar.gz and the other formats
  Windows can open (read-only), including archives inside archives.
- **Filter and search:** the search box filters the listing as you type; Return searches the
  folder tree by name in a new tab. With the box's "Search inside files" button on, Return searches
  the text of the files instead, and each result shows its matching line.
- **Folder sizes** on demand (Alt+Shift+Enter, or by selecting folders).
- **Preview pane** (Alt+P): PDF (with zoom, and password-protected files on request), Markdown,
  text and source code (coloured for about 20 languages), pictures (HEIC and AVIF too), HTML, and
  Word, Excel and PowerPoint documents, all rendered by TwinFM itself (no Office or browser
  needed). Right-click a block to copy its text.
- **Sidebar** with drives, the usual places, recent folders and your bookmarks (drop a folder on
  Bookmarks to add one, drag bookmarks to reorder them); it collapses to a rail of icons. A
  toolbar; light and dark themes that follow Windows.

## Install

Download `twinfm-0.3.0-x86_64-windows.zip` from the
[latest release](https://github.com/jtklinger/twinfm/releases/latest), unzip it anywhere, and run
`twinfm.exe`. There is no installer; settings are kept in `%APPDATA%\twinfm`.

Requirements: Windows 11 on x64. (Windows 10 may work but is not tested.) HEIC and AVIF pictures
are decoded by Windows itself, so their preview needs Microsoft's HEIF Image Extensions (with the
HEVC extension for HEIC, the AV1 extension for AVIF) from the Microsoft Store.

The executable is not code-signed, so Windows SmartScreen may warn the first time you run it;
choose "More info", then "Run anyway".

## Build from source

Stable Rust with the MSVC toolchain (`x86_64-pc-windows-msvc`) and the Visual Studio Build Tools
"Desktop development with C++" workload.

```
cargo build --release
target\release\twinfm.exe
```

An optional argument names the starting folder for the left pane. Slint comes from a fork with two
accessibility patches and a list fix (see [License](#license)); Cargo fetches it, so no manual steps are needed.

## Keys

| Key | Action |
|---|---|
| F3 | Show or hide the second pane |
| Tab, Shift+Tab, Ctrl+Shift+Right | Focus the other pane |
| Return, double-click | Open a folder or archive; open a file with its default program |
| Backspace, Alt+Up | Up one folder |
| Alt+Left, Alt+Right | Back, forward |
| Alt+Down | History menu: jump back or forward several places |
| Ctrl+L | Type a path |
| Ctrl+F | Filter the listing (the search box); Return there searches subfolders |
| Ctrl+Shift+F | Search subfolders in a new tab, by name or (with the box's button on) inside files |
| F5, Ctrl+R | Refresh |
| Ctrl+H | Show or hide hidden files |
| Ctrl+A, Space, Shift+arrows | Select all, toggle the cursor row, extend the selection |
| Ctrl+C, Ctrl+X, Ctrl+V | Copy, cut, paste (works with Explorer) |
| Shift+F5, Shift+F6 | Copy, move the selection to the other pane |
| F2 | Rename |
| F7, Ctrl+Shift+N | New folder |
| Ctrl+Alt+N | New file |
| Delete, Shift+Delete | Delete to the Recycle Bin, delete permanently |
| Ctrl+Z, Ctrl+Y | Undo, redo |
| Alt+Enter | Properties |
| Alt+Shift+Enter | Size every folder in the listing |
| Shift+F10, Menu key | Context menu (Shift+right-click: the classic shell menu) |
| Ctrl+Shift+1, 2, 3 | Details, large icons, medium icons |
| Alt+P | Preview pane |
| Ctrl+wheel, Ctrl+plus, Ctrl+minus, Ctrl+0 | Zoom a PDF in the preview (pointer over it) |
| Ctrl+T, Ctrl+W | New tab, close tab |
| Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+1 to 9 | Next, previous, numbered tab |
| Ctrl+Shift+PageUp, PageDown | Move the tab left, right |
| Ctrl+D | Bookmark the folder |
| F9 | Sidebar: full, or a rail of icons |
| Ctrl+, | Settings |
| F1 | About TwinFM |

## Settings

Settings live in `%APPDATA%\twinfm\config.toml` and are saved automatically: the open tabs, the
split, the window placement, bookmarks, recent folders, views, column widths, the theme and the choices in the
settings dialog (Ctrl+,). The file is plain TOML and can be edited while TwinFM is closed; a file
that does not parse is kept as `config.toml.bad` and TwinFM starts with the defaults.

For troubleshooting, the dialog's Log switch writes TwinFM's log to `twinfm.log` in the same folder
(the previous run's is kept as `twinfm.previous.log`), and "Open log folder" shows it.

## How it stays fast

- The list model holds plain structs: no per-row handles, callbacks or allocation after a folder
  is read.
- Folders are read on a worker and streamed in batches of 2,000 entries; sorting runs on a worker
  and swaps the whole list in one step.
- Only the UI thread touches the Slint model; workers wake it when they have results.
- Icons and thumbnails come from shell worker threads, only for the rows on screen, cached by
  type. Each pane draws its icons as one composed image rather than one image per row.
- File operations, shell verbs, archives and the preview each run on their own worker, so the UI
  thread never waits on them.

The original one-week prototype's verdict and measurements are in
[PROTOTYPE-RESULT.md](PROTOTYPE-RESULT.md); each development phase has its record in `docs/`.
`TWINFM_SCRIPT` drives the app for unattended measurements and checks (syntax in `src/auto.rs`).

## License

TwinFM's own code is MIT (see [LICENSE](LICENSE)).

It is built with a modified Slint, used under the
[Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0](https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md),
whose section 2 permits distributing Slint "as part of an Application, modified or unmodified"
provided the Slint attribution is shown. This README is that attribution (option (b) of section
2), and so is the About dialog (F1, or "About TwinFM" at the foot of the sidebar), which shows
Slint's `AboutSlint` widget.

The modifications to Slint are the three commits on the `rustfm` branch of
[jtklinger/slint](https://github.com/jtklinger/slint), also carried as `docs/patches/`. Slint's
licensing FAQ states that modifications may be released under any license of the modifier's
choice, so those three commits are offered under MIT like the rest of this repository; the Slint code
around them keeps Slint's own triple license (GPL-3.0-only, Royalty-free 2.0, or commercial), and
the fork as a standalone copy of Slint is redistributed under Slint's GPL-3.0 option, as section 3
of the Royalty-free license directs.

Other bundled material:

- The interface icons (`src/ui/icons/*.svg`) are drawn after [Lucide](https://lucide.dev), under its
  ISC license (with Feather's MIT license for the icons derived from it); see
  [src/ui/icons/LICENSE](src/ui/icons/LICENSE). File type icons and thumbnails are Windows' own.
- The interface font is [Inter](https://rsms.me/inter/) 4.1 (Regular, Medium, SemiBold), embedded
  from `src/ui/fonts/` under the SIL Open Font License 1.1 ([OFL.txt](src/ui/fonts/OFL.txt)).
- The Rust crates TwinFM is built from are listed with their licenses in
  `THIRD-PARTY-LICENSES.html`, shipped in each release zip (generated with
  [cargo-about](https://github.com/EmbarkStudios/cargo-about) from `about.toml`). The PDF
  renderer's embedded fonts, CMaps and colour profile, and Slint's license, are in
  [licenses/](licenses/).

[![Made with Slint](https://github.com/slint-ui/slint/raw/master/logo/MadeWithSlint-logo-whitebg.png)](https://slint.dev)
