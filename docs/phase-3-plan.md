# Phase 3: TwinFM finds and shows what Explorer can

Written 2026-10-13, when Phase 2 closed with P2-M1 to P2-M5 merged and the P2-M5 build at a
median of 20.4% on the gate under the triplet protocol (see `phase-2-plan.md` § Phase 2 close).
Phase 1 made TwinFM usable for a day instead of Files; Phase 2 made it trustworthy every day.
Phase 3's goal, accepted by Jeremy on 2026-10-13: **TwinFM finds and shows what Explorer can**,
without giving back the gate.

Five milestones, in the order Jeremy agreed on 2026-10-13 (the compact Windows 11 context menu
was cut from the phase; upstreaming the Slint patches stays Jeremy's decision, outside any
milestone):

1. **P3-M1, history and the settings dialog.** A back/forward history per tab (Alt+Left,
   Alt+Right, the mouse's Back and Forward buttons; Backspace and Alt+Up stay "up a folder"), and
   a settings dialog for what is today chosen only by editing `config.toml` (start folders, hidden
   files, the Type column, the sidebar, the path bar). Finishes two P2-M3 deferrals and gives the
   Type column its switch in case a later milestone needs its 3.4 gate points.
2. **P3-M2, search.** A name filter in the pane first (type-to-filter the current listing on the
   existing pass path, nothing per keystroke per row without a trace), then a recursive name
   search on a worker with results streamed into a tab, then content search only if wanted.
3. **P3-M3, folder sizes** in the status bar and the Size column for selected folders: a
   recursive size worker with cancellation, and the subtree watches P2-M1 deferred.
4. **P3-M4, archives.** Zip folders navigable as folders (the shell's zip namespace or a Rust
   reader; decide with a trace of enumeration cost), extraction through the existing file
   operation path, and the virtual-file drop path (`CFSTR_FILEDESCRIPTOR`) P2-M4 deferred.
5. **P3-M5, thumbnails and the large-icon view** (trace first). The first per-row rendering since
   M1: it starts with a trace of a 32 px per-row layout against the reference, and the strip
   pattern must carry it or the milestone stops at the trace.

**The gate is re-run after every milestone**, under the protocol agreed on 2026-10-12:
`tools/wpr-gate.ps1` elevated at the console by Jeremy, accessibility on, Narrator off, every
other TwinFM window closed, the split-and-scroll scenario on `C:\traces\bigdir`, on a clean binary
built into its own `--target-dir target-<name>` from a detached `git worktree add` checkout with
default features. Three runs per binary, interleaved (reference, candidate, reference, candidate,
reference, candidate), compared by median; a gap counts only when every reference run sits below
every candidate run or the medians differ by more than one point. The reference for the phase's
first milestone is `target-p2m5` (011430e, the P2-M5 PR head); after each merge the new
milestone's gate build becomes the next reference and the previous one is deleted. Bar: main
thread under 20% busy over the 10 s after the split, judged against the reference's spread in the
same session (single runs of one binary range about two points on a busy day, so no recorded
number from another session decides anything). A milestone is not closed until its table is
recorded here. When a milestone fails: stop, symbolize, name the frames, and either fix the
mechanism or report the options; never trade the bar off silently.

The architecture rules from Phase 1 hold (see `phase-1-plan.md` and the kickoff brief they came
from): only the UI thread touches a Slint model and workers wake it with
`slint::invoke_from_event_loop`; every COM object lives on the thread that made it; no app-state
`RefCell` borrow is held across a modal loop; no `unwrap` on `Weak<App>`; `windows-sys` with
hand-written calls; nothing per frame or per row without a trace first.

Amendment (P2-M4), carried over: the modal-loop rule is load-bearing, not precautionary. Our own
`DoDragDrop` runs from a posted message outside winit's handler, so `invoke_from_event_loop`
closures (enumeration results, job completions, icon replies, watch and clipboard events) run
INSIDE our own drag loop. Every such closure takes `with_state`; the drag code holds no borrow
while the loop runs (it takes everything out first and checks `state_is_free`), and no closure may
itself open a modal loop. Context menus and the shell's dialogs still run inside a handler or on
the worker.

Amendment (P2-M5), carried over: rows carry no image; icons reach the screen only through each
pane's strip (`icons::compose`), and every per-key cost is paid on a worker, once per key per
process. Anything that would put a per-row image or a per-row shell call back is a trace first,
not a feature.

## Open questions

- **Milestone order** was agreed on 2026-10-13 with P3-M1 first; Jeremy can still reorder or cut
  P3-M2 to P3-M5.
- **The Type column** stays on by default (3.4 gate points in M1). P3-M1 gives it a switch
  (`show_type`, default true); turning it off by default is a later decision, taken only if a
  milestone needs the headroom.
- **Thumbnails and the large-icon view** (P3-M5) are in the phase; they start with a trace of a
  32 px per-row layout against the reference before any UI is built.
- **Exposing OneDrive placeholders** (`PHCM_EXPOSE_PLACEHOLDERS`, P2-M5 Deferred) changes what
  every file API in the process reports; it needs its own check before it is turned on, and a
  decision on whether cloud status marks are wanted at all. Not part of any milestone yet.
- **Upstreaming the two Slint accessibility patches** (`docs/upstream-issue-draft.md`,
  `docs/patches/`): still deferred; Jeremy decides when.
- **Explorer's right-button drag never starts on this desktop**: outside TwinFM; worth one check
  without Logi Options+ running if Jeremy cares.
- **The compact Windows 11 context menu (`IExplorerCommand`)**: cut from Phase 3 on 2026-10-13;
  stays on the deferred list from M4.

## P3-M1: history and the settings dialog

The backlog was two P2-M3 deferrals (`phase-2-plan.md` § P2-M3 Deferred): a back/forward history
per tab, with the mouse's Forward button as history forward and Back as history back rather than
up a folder, and a settings dialog (M6b Deferred too: everything was chosen by using the app or by
editing `config.toml`). The dialog also gives the Type column its switch (`show_type`), in case a
later milestone needs the 3.4 gate points the column cost in M1 (`phase-1-plan.md` § M1, Gate).

### What landed (2026-10-13)

- **Back/forward history per tab** (`tabs.rs`, `main.rs`). `tabs::History` is a back stack and a
  forward stack of folders, pure and unit-tested without a window: `push(from, to)` puts the
  folder being left on the back stack, drops the oldest past 100 entries (`HISTORY_CAP`) and clears
  the forward stack; it does nothing when `from` is empty (a tab's first folder) or is `to` itself
  (the same folder twice in a row, compared ignoring ASCII case); `back(current)` and
  `forward(current)` pop one stack and push `current` on the other. Each `Tab` holds one, empty
  for every new and restored tab and never saved. `navigate` is now a wrapper over
  `navigate_to(pane, path, push)`, still the only place folders change: every ordinary navigation
  (a click, Return, a crumb, the sidebar, the path entry, Backspace and Alt+Up, a drive) goes
  through `navigate` and pushes; `history_move(pane, forward)` takes the target from the history
  and calls `navigate_to(.., false)`, so a move through the history records nothing. A tab opened
  with Ctrl+T or a middle-click starts with an empty history (its first `navigate` is to the folder
  it already holds, which pushes nothing). A folder that fails to list still counts as visited, as
  in Explorer; F5 and tab switches do not touch the history; This PC is a folder like any other.
- **Input.** Alt+Left and Alt+Right in the list's `key-pressed` (`pane.slint` FileList, next to
  Backspace and Alt+Up, which stay up a folder) call `pane-history(pane, forward)`. The mouse's
  Back button now goes back in the history and Forward goes forward: every `TouchArea` that
  reported a Back release also reports `PointerEventButton.forward` (the rows as gesture 7, the
  empty space, the header cells, the crumbs and chevrons, the path bar's mode glyph, the
  `TouchArea` behind everything), through `pane-forward(pane)` to `history_button(pane, true)`;
  `back_button` became `history_button(pane, false)`. The pane under the pointer becomes the active
  pane, as before. With nothing to go to, one line and nothing else: no fallback to up a folder.
- **Settings dialog** (`app.slint`, `main.rs`, `settings.rs`). A `PopupWindow` at the window level
  built like `about` (centred, bordered, 18 px padding), opened by Ctrl+, (the window-level
  `key-pressed`) or by "Settings" at the foot of the sidebar, left of "About TwinFM". Labels on the
  left, std-widgets controls on the right: two `LineEdit`s (the left and right start folders), two
  `CheckBox`es for hidden files (left pane, right pane), "Type column", "Show the sidebar" and
  "Breadcrumbs (off: a text entry)"; OK and Cancel at the bottom right; a note under them, "Saved
  to <config path>" or "Not saved in a scripted run". Opening it (`settings_open`) copies the live
  values into `dlg-*` properties and keeps them as `State::dialog`; each control stages its value
  through `settings-stage(field, value)`; nothing is applied while the dialog is open. OK
  (`settings_closed(true)`) reads the staged values, normalises them (`settings::Dialog::normalized`:
  start folders trimmed, an empty left one is `C:\`, an empty right one stays empty, meaning the
  left pane's folder; no other path check, so a folder that does not exist is saved as typed) and
  applies every field `Dialog::changes` reports, through the path the app already uses for it:
  hidden files through `toggle_hidden` (the Ctrl+H handler, now a function), the sidebar through
  `toggle-sidebar` (F9's path), the path bar through `path-entry-sticky` (what the bar's mode
  glyph sets), the start folders into the `Config`, and the Type column through `set_show_type`;
  then `schedule_save`. Cancel, Escape, and a second Ctrl+, while it is open change nothing. The
  popup is `no-auto-close`, so a click on a checkbox never closes it and Rust always hears how it
  closed. Every control has an `accessible-label`, the title `accessible-role: text`.
- **The Type column switch.** `show_type` in `Config` (default true, so a file without the key
  loads with the column on) and a `show-type` property on the window, passed to both panes and
  their lists. Off, the "Type" `HeaderCell` and each row's type `Text` sit behind `if
  root.show-type`, so they are not instantiated at all; the Name column, the only flexible one,
  takes the 120 px in the header and the rows alike, and the rename editor's width allows for
  230 px of fixed columns instead of 350. A header click can no longer sort by Type; turning the
  column off re-sorts every tab sorted by Type by name (`request_pass`, background tabs too) and
  sets `sort_column = "name"`; a saved `sort_column = "type"` with the column off is read as name
  at start, with one `[settings]` line. With the column on nothing in the row changed but the
  `if` around the type cell.
- **Start folders are now the dialog's.** `save_now` no longer copies the active tabs' folders
  into `start_left` / `start_right` (see Decisions).
- **Driver** (`auto.rs`): `history back|forward` (Alt+Left / Alt+Right through the Slint key path;
  key names `AltLeft`, `AltRight`), `forwardclick <pane>`, `settings open` (key name `CtrlComma`),
  `settings set <field> <value>` (fields `start_left`, `start_right`, `hidden_left`,
  `hidden_right`, `type_column`, `sidebar`, `path_bar`; it calls the same `settings-stage` the
  controls call), `settings ok`, `settings close` (the Cancel button's function), and key name
  `Down`. `backclick` now posts real messages (see Deviations).
- **Logging.** `[nav] pane=N history push <folder> (K back)`, `history back|forward: <from> ->
  <to> (K back, M forward left)`, `history back: nothing to go back to` (and forward), `Back|Forward
  button (active pane was A, now B)`; `[settings] dialog opened`, `dialog: <key> <old> -> <new>`
  per applied change, `dialog closed (ok), N change(s)` or `dialog closed (cancel), N change(s)
  discarded`, `Type column shown|removed from the header and the rows`, `pane=N tab I was sorted by
  Type: sorted by name instead`, `the Type column is off: sorting by name instead of the saved type
  sort`. The existing `[nav] pane=N gen=G path=` line is unchanged.
- **Tests**, 95 to 99: `History` push, back, forward, no push for an empty or identical folder,
  forward cleared by a push, a folder reached twice not in a row kept twice; the cap drops the
  oldest five of 105; `show_type` absent in an older file loads as true and `false` round trips
  through TOML; `Dialog::changes` reports exactly the differing fields in order, a blank left start
  folder normalises to `C:\` (no change), a right one is trimmed, a missing folder is accepted.
- **Docs**: `CLAUDE.md` (this record named in the first paragraph, the test count, the new driver
  steps, key names, `[nav]` and `[settings]` lines, `show_type` and the start folders in the
  Settings paragraph); `docs/handoffs/2026-10-13-phase-3-kickoff.md` deleted, as its Cleanup
  section asked once this plan existed.

### Decisions

- **The mouse's Back button is history back now, not up a folder.** This changes the P2-M3
  behaviour Jeremy asked for on 2026-10-12 (Back = up a folder) to the one the Phase 3 kickoff
  proposed and he accepted on 2026-10-13 (Back = history back, Forward = history forward;
  Backspace and Alt+Up stay up a folder). With an empty back stack the button does nothing and
  logs one line, so its behaviour is predictable.
- **`start_left` / `start_right` are no longer rewritten by every save.** Before, `save_now` copied
  the active tabs' folders into them (M6b: with no dialog, a fixed setting would rarely be right).
  With the dialog that copy would overwrite the value the user just typed 500 ms later. Since
  P2-M3 the tab lists hold the folders last open, so the start folders only matter for a pane with
  no saved list (a fresh file, a blanked list, the right pane before its first split), which is
  what the dialog's field describes.
- **The path bar is applied through `path-entry-sticky`**, the property the bar's mode glyph sets
  for both panes, rather than `set-path-mode`: `set-path-mode(p, true)` is Ctrl+L's temporary
  entry, which also takes the focus, not the lasting mode the setting means.
- **Hidden files and the sidebar are applied only when the live value differs from the staged
  one**, not toggled blindly: both are toggles, and Ctrl+H or F9 could have changed the live state
  since the dialog opened (the list behind a `no-auto-close` popup still takes clicks).
- **The dialog puts the focus in its first field after it is shown.** Slint's `show_popup` takes
  the focus from the window and gives it to nothing (`WindowInner::show_popup`), and a `focus()`
  in an `init` runs before that, which is why the crumb dropdown's own `FocusScope` never got a
  key (P2-M3 Console check). The first scripted run showed it here: Escape and Down after `settings
  open` reached nothing. Fix: `show-settings` bumps a counter after `show()`, and a `changed`
  handler inside the popup focuses the "Left start folder" field. Escape then bubbles from the
  field to a `FocusScope` around the dialog's content, which cancels; the window-level
  `key-pressed` also cancels on Escape while the dialog is shown, for the case where a click behind
  it moved the focus back to a list. Closing restores the focus to the list (`focus-pane`).
- **Cancel's log line counts what it discarded** (`closed (cancel), 1 change(s) discarded`), so a
  scripted check can see that a staged change was dropped rather than never made.
- **The row's accessible label still includes the type text** with the column off. It is evaluated
  only when the accessibility tree asks for it, and taking it out would restructure the row, which
  the brief ruled out for the default-on path.
- **The forward stack has no cap of its own**: it only grows by moves back, so it never holds more
  than the back stack's 100.

### Deviations

- **`backclick` was not a posted message; it is now.** The brief said both side buttons must go
  through a posted `WM_XBUTTON*` message and that `backclick` already did; it dispatched Slint
  `PointerPressed`/`PointerReleased` events directly. Both `backclick` and the new `forwardclick`
  now post `WM_MOUSEMOVE`, `WM_XBUTTONDOWN` and `WM_XBUTTONUP` (XBUTTON1 or XBUTTON2, with
  `MK_XBUTTON1`/`MK_XBUTTON2` held on the down message) to the main window at the middle of the
  pane's list in physical pixels, so the press goes through winit's window procedure as a real
  mouse's does, and the step waits 150 ms for the queue to drain.
- **The dialog's staged values are filled by Rust** (`settings_open` sets the `dlg-*` properties
  before `show-settings`), not copied by Slint on open: the hidden-file flags live in each tab's
  model, which Slint cannot see.
- **`settings set` calls the `settings-stage` function**, which is what every control's `edited` or
  `toggled` handler calls, rather than a callback (the controls have no callback of their own to
  share).
- **A `Down` key name** was added to the driver for the brief's focus check (`key Down` did not
  exist; `ShiftDown` did).
- **Extra logging** beyond the brief's list: the Back/Forward button line names the pane that was
  active, as the P2-M3 Back line did, and the Type column switch logs what it re-sorted.

### Deferred

- **Back and Forward over a text entry, the tab strip or the list's scrollbar** still do nothing
  (their own `TouchArea`s take the press), as in P2-M3.
- **No history menu** (a right-click or long press on a Back button listing the stack): there is
  no toolbar to hang it on.
- **History is per tab and not saved**; a restored tab starts with none.
- **The dialog has no browse button** for the start folders, and does not validate them.
- **Keyboard use of the dialog by Tab order** was not checked by a person (the scripted runs used
  the staging function and Escape); a console check should Tab through it with Narrator on.

### Scripted checks

RDP session, `target\release` of this branch (the worktree's own build), every run with
`TWINFM_SCRIPT` and without `TWINFM_SOAK`; script files and logs in the session scratchpad; no
run read or wrote `%APPDATA%\twinfm\config.toml`. All exit code 0, no panic.

- **History, keys and buttons.** Script `sleep 800; nav 0 C:\Windows; sleep 700; nav 0
  C:\Windows\System32; sleep 900; history back; sleep 700; history back; sleep 700; history
  forward; sleep 700; backclick 0; sleep 700; forwardclick 0; sleep 700; key Backspace; sleep 700;
  key AltRight; sleep 300; tab new; sleep 700; key AltLeft; sleep 300; nav 0 This PC; sleep 700;
  key AltLeft; sleep 700; key AltLeft; sleep 300; key AltRight; sleep 700; quit`. stderr:

  ```
  [nav] pane=0 gen=1 path=C:\
  [nav] pane=0 history push C:\ (1 back)
  [nav] pane=0 gen=2 path=C:\Windows
  [nav] pane=0 history push C:\Windows (2 back)
  [nav] pane=0 gen=3 path=C:\Windows\System32
  [nav] pane=0 history back: C:\Windows\System32 -> C:\Windows (1 back, 1 forward left)
  [nav] pane=0 gen=4 path=C:\Windows
  [nav] pane=0 history back: C:\Windows -> C:\ (0 back, 2 forward left)
  [nav] pane=0 gen=5 path=C:\
  [nav] pane=0 history forward: C:\ -> C:\Windows (1 back, 1 forward left)
  [nav] pane=0 gen=6 path=C:\Windows
  [auto] backclick pane=0: posted at (692, 400) logical, (692, 400) physical
  [nav] pane=0 Back button (active pane was 0, now 0)
  [nav] pane=0 history back: C:\Windows -> C:\ (0 back, 2 forward left)
  [nav] pane=0 gen=7 path=C:\
  [auto] forwardclick pane=0: posted at (692, 400) logical, (692, 400) physical
  [nav] pane=0 Forward button (active pane was 0, now 0)
  [nav] pane=0 history forward: C:\ -> C:\Windows (1 back, 1 forward left)
  [nav] pane=0 gen=8 path=C:\Windows
  [nav] pane=0 history push C:\Windows (2 back)
  [nav] pane=0 gen=9 path=C:\
  [nav] pane=0 history forward: nothing to go forward to
  [tab] pane=0 opened tab 2 of 2 (id 3) on C:\
  [nav] pane=0 gen=10 path=C:\
  [nav] pane=0 history back: nothing to go back to
  [nav] pane=0 history push C:\ (1 back)
  [nav] pane=0 gen=11 path=This PC
  [nav] pane=0 history back: This PC -> C:\ (0 back, 1 forward left)
  [nav] pane=0 gen=12 path=C:\
  [nav] pane=0 history back: nothing to go back to
  [nav] pane=0 history forward: C:\ -> This PC (1 back, 0 forward left)
  [nav] pane=0 gen=13 path=This PC
  ```

  Alt+Left and Alt+Right went through Slint's key routing (`key`), the side buttons through posted
  `WM_XBUTTON*` messages. Backspace (gen 9) went up a folder from `C:\Windows` and pushed it: the
  keys are told apart, and the push cleared the forward stack (the next Alt+Right had nothing).
  The new tab started with an empty history; This PC entered and left the history like a folder.
  The list's middle is a row in both folders here (row gestures 5 and 7).
- **The side buttons over the empty space.** With an empty scratch folder
  `%TEMP%\twinfm-p3m1-empty` (removed afterwards): `nav 0 <empty>; nav 0 C:\; backclick 0;
  forwardclick 0; backclick 0; backclick 0`. The forward press and the last back press landed on
  the empty folder's empty space, the others on a row of `C:\`:

  ```
  [nav] pane=0 history back: C:\ -> C:\Users\jtkli\AppData\Local\Temp\twinfm-p3m1-empty (1 back, 1 forward left)
  [nav] pane=0 Forward button (active pane was 0, now 0)
  [nav] pane=0 history forward: C:\Users\jtkli\AppData\Local\Temp\twinfm-p3m1-empty -> C:\ (2 back, 0 forward left)
  [nav] pane=0 history back: C:\ -> C:\Users\jtkli\AppData\Local\Temp\twinfm-p3m1-empty (1 back, 1 forward left)
  [nav] pane=0 Back button (active pane was 0, now 0)
  [nav] pane=0 history back: C:\Users\jtkli\AppData\Local\Temp\twinfm-p3m1-empty -> C:\ (0 back, 2 forward left)
  ```
- **The pane under the pointer becomes active.** `nav 0 C:\Windows; split; nav 1
  C:\Windows\System32; backclick 0; backclick 1; forwardclick 1; forwardclick 0`:

  ```
  [nav] pane=0 Back button (active pane was 1, now 0)
  [nav] pane=0 history back: C:\Windows -> C:\ (0 back, 1 forward left)
  [nav] pane=1 Back button (active pane was 0, now 1)
  [nav] pane=1 history back: C:\Windows\System32 -> C:\Windows (0 back, 1 forward left)
  [nav] pane=1 Forward button (active pane was 1, now 1)
  [nav] pane=1 history forward: C:\Windows -> C:\Windows\System32 (1 back, 0 forward left)
  [nav] pane=0 Forward button (active pane was 1, now 0)
  [nav] pane=0 history forward: C:\ -> C:\Windows (1 back, 0 forward left)
  ```

  Pane 1's first folder (the split opening it on `C:\Windows`) pushed nothing; its own navigation
  did.
- **Settings dialog, OK, Cancel and Escape.** Script `sleep 1500; settings open; sleep 2500;
  settings set type_column false; settings set hidden_left true; sleep 300; settings ok; sleep
  2500; settings open; sleep 500; settings set type_column true; sleep 300; settings close; sleep
  1500; settings open; sleep 700; key Escape; sleep 500; key Down; sleep 400; key Down; sleep 1000;
  quit`, with screenshots (`tools/shot.ps1`) and a UIA dump (`tools/uia-dump.ps1`) taken while it
  ran:

  ```
  [auto] Key("CtrlComma")
  [settings] dialog opened
  [auto] SettingsSet("type_column", "false")
  [auto] SettingsSet("hidden_left", "true")
  [auto] SettingsClose(true)
  [settings] dialog: show_hidden_left false -> true
  [hidden] pane=0 show_hidden=true
  [settings] dialog: show_type true -> false
  [settings] Type column removed from the header and the rows
  [settings] dialog closed (ok), 2 change(s)
  [sort] pane=0 34 of 34 entries by Name asc=true hidden=0 cursor=None selected=0 in 0.2 ms
  [auto] Key("CtrlComma")
  [settings] dialog opened
  [auto] SettingsSet("type_column", "true")
  [auto] SettingsClose(false)
  [settings] dialog closed (cancel), 1 change(s) discarded
  [auto] Key("CtrlComma")
  [settings] dialog opened
  [auto] Key("Escape")
  [settings] dialog closed (cancel), 0 change(s) discarded
  [auto] Key("Down")
  [select] pane=0 Only row=0 selected=1
  [auto] Key("Down")
  [select] pane=0 Only row=1 selected=1
  ```

  Ctrl+, opened it through the key path. The screenshot taken after OK shows the header as Name,
  Size, Modified with the rows' columns under it and 34 items (hidden files shown); the one after
  the cancelled `type_column true` still has no Type column. Escape, a real key event, closed the
  dialog, and the two Downs moved the list's cursor: the list had the focus back. Before the focus
  fix (Decisions) the same script logged no `closed` line after Escape and no `[select]` after
  Down. The UIA dump with the dialog open: `focused element: Left start folder (ControlType.Edit)`,
  and the controls as `Edit 'Left start folder'`, `Edit 'Right start folder'`, `CheckBox 'Show
  hidden files in the left pane'`, `CheckBox 'Show hidden files in the right pane'`, `CheckBox
  'Show the Type column'`, `CheckBox 'Show the sidebar'`, `CheckBox 'Breadcrumbs in the path
  bar'`, `Button 'OK'`, `Button 'Cancel'`, the title `Text 'Settings'` and the note `Text 'Not
  saved in a scripted run'`.
- **The other fields.** `sort 0 3; settings open; settings set type_column false; settings set
  sidebar false; settings set path_bar entry; settings set start_right C:\Users; settings ok;
  settings open; settings set sidebar true; settings set path_bar crumb; settings set start_right ;
  settings ok`:

  ```
  [sort] pane=0 23 of 34 entries by Type asc=true hidden=11 cursor=None selected=0 in 0.3 ms
  [settings] dialog: start_right  -> C:\Users
  [settings] dialog: show_type true -> false
  [settings] Type column removed from the header and the rows
  [settings] pane=0 tab 1 was sorted by Type: sorted by name instead
  [settings] dialog: sidebar_visible true -> false
  [sidebar] hidden
  [settings] dialog: path_bar breadcrumb -> entry
  [settings] dialog closed (ok), 4 change(s)
  [sort] pane=0 23 of 34 entries by Name asc=true hidden=11 cursor=None selected=0 in 0.2 ms
  [settings] dialog: start_right C:\Users ->
  [settings] dialog: sidebar_visible false -> true
  [sidebar] shown
  [settings] dialog: path_bar entry -> breadcrumb
  [settings] dialog closed (ok), 3 change(s)
  ```
- **Saved to a file.** `TWINFM_CONFIG` at a new scratch file; `settings open; settings set
  type_column false; settings set start_left D:\nowhere-p3m1; settings ok; nav 0 C:\Windows; window
  close`: `[settings] dialog: start_left C:\ -> D:\nowhere-p3m1`, `dialog: show_type true ->
  false`, `dialog closed (ok), 2 change(s)`, then `[settings] saved ...p3m1-cfg.toml` (the
  debounce, the navigation, and the close). The file then held `start_left = 'D:\nowhere-p3m1'`
  (not overwritten by the later navigation to `C:\Windows`, which went to `left_tabs =
  ['C:\Windows']`) and `show_type = false`. With `sort_column` then edited to `"type"` by hand, the
  next start logged `[settings] the Type column is off: sorting by name instead of the saved type
  sort`, sorted `C:\Windows` `by Name`, and saved `sort_column = "name"`.

Not checked by a script, needs a person at the console: the physical side buttons of Jeremy's
mouse (Logi Options+ may map them, as it did the middle button in P2-M3), Alt+Left and Alt+Right
on the keyboard, clicking the dialog's checkboxes and typing in its fields, Tab through the dialog
with Narrator, and the "Settings" link in the sidebar with the mouse.

Optional measurement for Jeremy (whether turning the Type column off gives back M1's 3.4 points):
in the elevated console PowerShell, with a read-only config holding only that key,

```
Set-Content C:\traces\p3m1-notype.toml "show_type = false"
Set-ItemProperty C:\traces\p3m1-notype.toml -Name IsReadOnly -Value $true
$env:TWINFM_CONFIG = "C:\traces\p3m1-notype.toml"
powershell -ExecutionPolicy Bypass -File C:\Users\jtkli\projects\twinfm\tools\wpr-gate.ps1 -Exe C:\Users\jtkli\projects\twinfm\target-p3m1\release\twinfm.exe -Etl C:\traces\twinfm-p3m1-notype.etl
Remove-Item Env:TWINFM_CONFIG
```

(the gate script sets `TWINFM_SCRIPT`, so only `TWINFM_CONFIG` makes it read a file; the file being
read-only, the run cannot rewrite it and logs `could not save` at exit), interleaved with
`target-p3m1` runs without `TWINFM_CONFIG` under the triplet protocol.

### Gate

Pending: the lead builds `target-p3m1` from the PR head and Jeremy runs the triplets against
`target-p2m5`.
