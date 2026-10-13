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
  loads with the column on) and a `show-type` property on the window, passed to both panes. Since
  the second gate run (§ Gate) the switch is chosen once per pane, by an `if` outside any `for`,
  and no row binds to it. The pane's list is a host, `FileListHost` (`pane.slint`), which keeps
  the id `list` in `Pane`, so every `list.` reference there (the strip, the drop highlight, the
  empty space, the rename editor) and every Rust call (`get_scroll`, `set_scroll`,
  `reset_scroll`, `begin_edit`, `set_strip`, the driver's clicks) is unchanged. The host owns the
  list's state (`rows`, `current-row`, `content-y`, the drag threshold and the press state) and
  its logic (the row gestures, the keys, `bring-into-view`, the `viewport-changed` reports), and
  holds one of two ListViews: `if root.show-type: FileList` or `if !root.show-type:
  FileListNoType`. Each keeps the StandardListView layering: a row layer (`FileRows`,
  `FileRowsNoType`) holding the single `for` and `accessible-delegate-focus`, and a focus layer
  adding the zero-width FocusScope. `FileRows`' row is the reference's row (P2-M5) element for
  element and binding for binding; `FileRowsNoType`'s is the same without the Type `Text`. State
  crosses the `if` by binding (`content-y <=> root.content-y`, `current-row: root.current-row`,
  `rows: root.rows`); events come back by callbacks (`row-pointer`, `row-gesture`, `key`) that the
  host answers with its functions. What the pane asks of the list crosses by a request:
  `focus-list()` bumps a counter the inner list watches (`changed focus-request`), and
  `bring-into-view()` runs in the host on `content-y`. The host's `visible-width` and
  `visible-height` are its own width and height, as a Fluent ListView's are. Switching keeps the
  folder, rows, scroll position, cursor, selection and strip: they are the host's or Rust's, and
  the new list binds to them. The header is chosen the same way: two `HorizontalLayout`s under
  `if`, the default one the reference's four cells, the other without "Type". Off, the Name
  column, the only flexible one, takes the 120 px in the header and the rows alike, and the
  rename editor's width allows for 230 px of fixed columns instead of 350 (per pane, not per row).
  A header click can no longer sort by Type; turning the column off re-sorts every tab sorted by
  Type by name (`request_pass`, background tabs too) and sets `sort_column = "name"`; a saved
  `sort_column = "type"` with the column off is read as name at start, with one `[settings]` line.
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
- **Two copies of the row, not a shared row component.** A component instantiated in each row
  (a `FileRow` with an `@children` slot for the Type cell, as the follow-up brief suggested)
  would carry its own input properties and their bindings (the row, its index, the cursor) into
  every row instance, which is what the gate measures. So the two `for` bodies are copies that
  differ only by the Type `Text`. The generated Rust for `FileRows`' row was compared with the
  reference's (`target-p2m5`'s `app.rs`): the same struct, items and bindings; the only
  difference is the body of the `TouchArea`'s `pointer-event` handler, now one call to
  `row-pointer` (a handler is code run on an event, not a binding).
- **The row gestures and the keys live in the host, once**, not in each variant: the variants
  hold only the rows and the FocusScope, whose `key-pressed` returns the host's `key(e)`. One
  consequence: a press on a row asks for the keyboard focus through the counter, so the list
  takes it at the start of the next event-loop turn instead of inside the press (no key can
  arrive in between; the press's cursor, selection and `focus-requested` are immediate as before).
- **A list created while its pane is active takes the focus in its `init`.** The switch destroys
  the list that had the focus, and the counter bump from `focus-pane` after OK may be consumed by
  the old list before the `if` swaps. A split creates the right pane's list after `focus-pane(1)`,
  so it takes the focus the same way.
- **The new list's `init` sets `content-height` to rows x 22 px.** The Flickable checks its bounds
  on the next turn of the event loop (a delayed change tracker), before the ListView's first
  layout has set `content-height`; with the default content height (the list's own height) the
  kept `content-y` read as out of bounds and the list jumped to the top. The first scripted run
  showed it (pane 0 at -352 before the switch, 0 after); with the height set first, -352 stays.
- **The header is chosen per pane like the list**: two whole `HorizontalLayout`s rather than an
  `if` around the Type cell inside one, so the default header is the reference's layout.

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
- **No `FileRow` component** (the follow-up brief's suggested factoring); see Decisions.
- **`Pane` has no `forward-focus` any more.** Slint refuses to forward the focus to the host (not
  a focusable element), and the FocusScope inside the `if` cannot be named from outside. Nothing
  focuses a `Pane` itself; every caller goes through `focus-list`.

### Deferred

- **Slint snaps a rebuilt list to a whole row.** Jeremy's console check (2026-10-13): switching the
  Type column off moved the list up by part of a row. Reproduced with the `scrollpos` driver
  step (added for it): `-360.0` became `-352.0` (16.4 rows to 16), `-24254.0` became `-24244.0`;
  positions already on a 22 px boundary did not move. The cause is `ensure_updated_listview` in
  Slint's `internal/core/model/repeater.rs`: a ListView created with its viewport already past the
  second row takes the "random seek" branch, lays the first instance out at viewport y 0 and writes
  `content-y` back as `-offset * row_height`, dropping the fraction. The P2-M5 build does the same
  on every tab switch (`scroll -360` then `restored to -352` in the P2-M3 record) and on a
  scrollbar thumb drag, so the switch only made an old snap visible. The one-line fix, proven on a
  local build (`target-zcfix`, every `scrollpos` reading identical before and after, off and on,
  both panes, three rounds), keeps the fraction: `(state.offset, content_y_value + state.offset as
  Coord * element_height)` in place of `(state.offset, 0 as Coord)`. It changes how every ListView
  in the app scrolls (thumb drags no longer snap either), so it is a third fork patch with its own
  gate triplet, and an upstreamable bug fix. Jeremy chose (2026-10-13) to accept the snap for
  P3-M1 and take the patch when the fork is next touched.
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

- **The switch chosen per pane (the follow-up after the second gate run).** All runs on the
  worktree's build of f21c036, RDP, `TWINFM_SCRIPT` set, no `TWINFM_SOAK`, `TWINFM_CONFIG` only
  at scratch files; exit 0 and no `panicked` line in every run. Both panes on
  `C:\Windows\System32`, cursor and scroll moved in each, read through a tab switch (`tab new;
  tab prev`, whose `[tab]` line prints the cursor and scroll), then `settings set type_column
  false; settings ok` and the same read:

  ```
  [tab] pane=1 switched to 1 of 3 (id 2, C:\Windows\System32), cursor 4 scroll -220
  [tab] pane=0 switched to 1 of 3 (id 1, C:\Windows\System32), cursor 7 scroll -352
  [settings] dialog: show_type true -> false
  [tab] pane=0 switched to 1 of 4 (id 1, C:\Windows\System32), cursor 7 scroll -352
  [tab] pane=1 switched to 1 of 4 (id 2, C:\Windows\System32), cursor 4 scroll -220
  ```

  With the column off, in both panes: `key Down` moved the cursor (`[select] pane=0 Only row=8`),
  `click` selected, `rclick` opened the shell menu (`[menu] pane=0 row=14 ...`, `closed ... id 0 =
  Nothing` after `menuclose`); in a `%TEMP%` scratch folder `dropfiles <pane> 0 copy` found the
  folder row (`[dnd] over pane 0 row 0 (...\dest) ... -> effect copy`, `Done: Copied 1 item`) and
  `rename` renamed the clicked row's file through the editor (`Done: Renamed to zc-ren0.txt`);
  the `[icons]` line kept composing strips across the switch. Switched back on: `key Down` and
  `click` again in both panes. Focus, in both states: after `settings ok`, after Ctrl+L then
  Escape, and after F2 then Escape, a `key Down` logged a `[select]` line one row on, in pane 0
  and in pane 1. The gate script on `C:\traces\bigdir` ran to the end with the column on and with
  a read-only scratch config holding `show_type = false`. `SLINT_ACCESSKIT_STATS=1` on the gate
  script: the same counters per second as `target-p2m5` (`full=1 incremental=4 ... nodes_sent=848
  cached=409` against `846 ... 408`; the one extra node is the sidebar's "Settings" link).
  `tools/uia-dump.ps1` on `C:\Windows` with row 3 clicked, `target-p2m5` against this build: the
  same 212 nodes at the same places in the window, the same row names, `AppReadiness, File folder`
  focused and selected in both; the only differences are the "Settings" text and the "About
  TwinFM" text moved right of it. The P3-M1 checks above were re-run on this build with the same
  lines (history, the empty space, the split, the dialog with its UIA dump, the other fields, the
  `TWINFM_CONFIG` round trip).

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

**First run (2026-10-13): failed, mechanism named, fixed.** Console WPR, Jeremy elevated,
`tools/wpr-gate.ps1`, accessibility on, Narrator off, no other TwinFM window, six runs from
11:08 to 11:19, reference and candidate alternating: `target-p2m5` (011430e) and `target-p3m1`
built from d70c3c5 (the PR head at the time). Analysed with `tools/analyze-trace.py --symbols
--exe`.

| Run | P2-M5 (`target-p2m5`) | P3-M1 d70c3c5 |
|---|---|---|
| 1 | 19.9% (1992 ms) | 21.7% (2172 ms) |
| 2 | 20.3% (2026 ms) | 22.7% (2269 ms) |
| 3 | 20.2% (2022 ms) | 20.5% (2048 ms) |
| **median** | **20.2%** | **21.7%** |

Both halves of the rule said gap: every reference run sat below every candidate run, and the
medians differed by 1.5. The category shares (accessibility, render, draw_text, rows, timers,
heap) kept their proportions, which on its own would read as machine speed, but the symbolized
tables held a frame the reference has nowhere: `i_slint_core::model::repeater::Conditional::visit`,
184 to 205 inclusive samples per run, and under it `InnerComponent_text_N::layout_info` (74 to 93
samples) that the reference's rows never call. The cause was the `if root.show-type:` around each
row's type `Text`: a conditional element is its own sub-component per row, so the type cell got its
own instantiation, layout pass and heap allocation (`RtlpLowFragHeapAllocFromContext` +55
exclusive in run 2) on top of the text it drew before. The brief had asked for exactly that `if`,
on the theory that only a non-instantiated cell gives the 3.4 points back when the column is off;
the default-on row paid for it instead.

The fix (the commit after d70c3c5): the row's type cell is the reference's single `Text` again,
with `text: root.show-type ? row.type-text : ""` and `width: root.show-type ? 120px : 0px`. The
item count per row is the reference's; off, nothing is drawn (draw_text was the bulk of the
column's M1 cost) and the Name column takes the width as before. The header cell keeps its `if`
(one per pane). The scripted Type-off check was re-run on the fixed build (`settings open;
settings set type_column false; settings ok; nav 0 C:\Windows; quit`: `dialog: show_type true ->
false`, `Type column removed from the header and the rows`, exit 0, no panic). Traces
`C:/traces/twinfm-ab8-{p2m5,p3m1}-{1,2,3}.etl`, reports under `C:/traces/export/`.

**Second run (2026-10-13): still a gap, smaller, no single frame.** Same protocol, six runs from
11:40 to 11:50, `target-p2m5` against `target-p3m1` rebuilt from cba777e (the fix above).

| Run | P2-M5 (`target-p2m5`) | P3-M1 cba777e |
|---|---|---|
| 1 | 19.9% (1989 ms) | 20.7% (2069 ms) |
| 2 | 19.9% (1994 ms) | 20.6% (2065 ms) |
| 3 | 20.1% (2008 ms) | 20.9% (2092 ms) |
| **median** | **19.9%** | **20.7%** |

The medians differ by 0.8, under the one-point half of the rule, but every reference run sits
below every candidate run, which is the other half, so the gap counts. Both triplets are tight
(0.2 and 0.3 of spread), which makes the 0.7 look real rather than drift. The `Conditional::visit`
frame is gone. No new frame stands out: the extra 70 to 85 inclusive samples per run are spread
over heap allocation (`RtlpLowFragHeapAllocFromContext` +60, +21, 0 exclusive), property
evaluation (`PropertyTracker::evaluate_if_dirty`, `PropertyHandle::update`, `hash_one`, each +8
to +13) and text layout, the profile of two more bindings evaluated per row instance: the type
cell's `text` and `width` now depend on `root.show-type` where the reference's were constants,
and the ListView instantiates rows as they scroll into view, so each new row registers two
dependencies and runs two closures the reference does not. A zero-cost switch would have to
choose the row layout once per pane, outside the `for` (two ListViews under a pane-level `if`,
which the driver's and the focus layering's references to the one `list` element rule out for
this milestone). Traces `C:/traces/twinfm-ab9-{p2m5,p3m1}-{1,2,3}.etl`.

**Elimination run (2026-10-13, 12:14 to 12:21): nothing else in P3-M1 costs anything.** Before
betting a gate run on the rewrite, a binary `target-p3m1x` was built from cba777e with only the
row's Type cell, the Type header cell and the rename editor's width put back to 0ba542c's
constants (a dirty detached checkout, never committed), and run against `target-p2m5` under the
same protocol. The second candidate run had a false start (the exe was still linking; the script
threw on the missing process and left a WPR session open until `wpr -cancel`), and was re-run.

| Run | P2-M5 (`target-p2m5`) | P3-M1x (cba777e, reference row) |
|---|---|---|
| 1 | 21.2% (2120 ms) | 21.2% (2123 ms) |
| 2 | 40.0% (4002 ms), disturbed | 21.9% (2185 ms) |
| 3 | 22.7% (2268 ms) | 20.7% (2066 ms) |
| **median** | **22.7%** (22.0 without run 2) | **21.2%** |

The reference's run 2 doubled in every category at once (machine, not app; the open WPR session
is the likely cause). Run 1 is equal, run 3 has the candidate 2.0 below, and the medians favour the
candidate with or without the disturbed run: no gap. So history, the dialog, the Forward handling
and the start-folder change cost nothing per frame, and the second run's 0.7 was the two per-row
bindings alone. The machine sat about 1.3 points above the second run's session. Traces
`C:/traces/twinfm-ab10x-{p2m5,p3m1x}-{1,2,3}.etl`.

**Third run (2026-10-13, Jeremy at the console with `tools/wpr-triplet.ps1`): passed.** Same
protocol, `target-p2m5` (011430e) against `target-p3m1` built from 537f606 (the per-pane switch,
f21c036, plus docs). Analysed with `tools/analyze-trace.py --symbols --exe`.

| Run | P2-M5 (`target-p2m5`) | P3-M1 537f606 |
|---|---|---|
| 1 | 21.2% (2125 ms) | 20.8% (2080 ms) |
| 2 | 20.3% (2029 ms) | 21.8% (2180 ms) |
| 3 | 28.2% (2823 ms) | 27.7% (2767 ms) |
| **median** | **21.2%** | **21.8%** |

The medians differ by 0.6, and the candidate sits below the reference in runs 1 and 3, so neither
half of the rule calls a gap. The third pair ran about seven points high for both binaries (the
machine, in step; both shares identical), which the pairwise reading tolerates and a single run
would not have. Frame shares match within a few samples in every pair (accessibility 219 / 224,
211 / 239, 280 / 262; render 1246 / 1181, 1175 / 1260, 1606 / 1622; draw_text 700 / 691, 659 / 701,
918 / 876; rows 371 / 390, 350 / 386, 544 / 529; timers 723 / 732, 692 / 756, 968 / 935; heap
205 / 200, 192 / 197, 249 / 225), and no `Conditional::visit`, host or dialog frame appears in any
candidate window. Together with the elimination run this closes the gate for P3-M1: the cost was
the per-row switch in its two earlier forms, and the per-pane form has none. Traces
`C:/traces/twinfm-ab10-{p2m5,p3m1}-{1,2,3}.etl`, reports under `C:/traces/export/`.

Jeremy's console checks on `target-p3m1` (2026-10-13): the side buttons, Alt+Left/Right and the
dialog work; switching the Type column off moved the list up by one row (switching it back on
kept its place): Slint's whole-row snap on a rebuilt list, accepted for P3-M1 and deferred as
a fork patch (Deferred). The console checks pass; the gate is closed.

## P3-M2: search

The backlog was the second item of the Phase 3 list: TwinFM had no way to find a file it does not
show. Explorer has a search box that filters the folder as you type and a search that looks
through every folder below. This milestone gives TwinFM both, the first by reusing the pass that
already filters hidden files, the second by reusing the listing loop and the listing's channel, so
sorting, the cursor, icons and the status bar work on results unchanged. Content search (the plan
said "only if wanted") was not asked for and is deferred.

### What landed (2026-10-13)

- **The name filter** (`model.rs`, `main.rs`, `pane.slint`, `app.slint`). `model::NameMatch` is the
  pattern prepared once: the text as typed, its Unicode lowercase (`to_lowercase`), and with `*`
  or `?` in it the same as characters for a glob. `matches(name)` is true for every name when the
  pattern is empty (nothing is lowercased then); otherwise it lowercases the part of the name after
  the last `\` and tests `contains` (a substring) or an anchored glob (`*` any run, `?` one
  character, backtracking only to the last `*`). `PaneModel` gains `filter: RefCell<NameMatch>`, a
  property of the tab like `show_hidden`, and a `filtered` count beside `hidden`. `visible(e,
  show_hidden, filter)` is the hidden-file test and the filter test; `filter_sort` takes the
  filter and counts the two apart (`PassOutput.filtered`), and `append` (batches while a listing
  streams in) does the same. The pass is the hidden-file switch's pass: `request_pass` clones the
  tab's filter into the worker with the snapshot, and the cursor and selection come back by name.
- **The box.** In the pane's path bar row, at its right end (220 px, the breadcrumb or the text
  entry shrinking by as much): `if root.filter-open: HorizontalLayout { LineEdit, clear glyph }`,
  an `if` at pane level, so the box does not exist while closed and no row knows about it. Ctrl+F
  (the window-level `key-pressed`, so it works from the list and from the box) calls `open-filter`:
  the box opens with the focus in it and its text selected (a new box takes the focus in its
  `init`, an open one through a counter it watches). Each edit calls `filter-edited`; Rust restarts
  a 150 ms single-shot timer per pane (`State::filter_timer`) and applies the text when it fires,
  so a word typed is one pass. Return or Down call `filter-apply` (at once, the focus goes to the
  list; an empty text closes the box); Escape or the clear glyph empty the text, close the box,
  call `filter-clear` (at once) and give the list the focus. When the box closes while it has the
  focus (a navigation or a tab switch closed it), the list takes the focus. Typing in the list does
  not start a filter: there is no type-to-select and none was added.
- **What the user sees.** The status bar's left text reads "N of M items (K hidden)" while a filter
  is on, M being the entries the hidden-file switch lets through. The filter never reveals a hidden
  file. A navigation to another folder clears the filter and closes the box (Explorer's
  behaviour); a tab switch shows the new tab's own filter (`show_tab` sets the box from the tab's
  model); F5 keeps it; nothing is saved to `config.toml`. Ctrl+A, the file operations, the context
  menu, open and drag act on the rows shown, which is what the model holds.
- **The listing loop factored out** (`fs/enumerate.rs`). `list_dir(path, types, icon_keys, zone,
  f)` is the `FindFirstFileExW` / `FindNextFileW` / `FindClose` loop that `run` had, calling `f`
  per entry (false stops it), with `ERROR_FILE_NOT_FOUND` from the first call an empty listing as
  before. `run` now calls it; its batches, its `Done` and its error text are what they were (the
  existing enumeration tests pass unchanged).
- **The recursive name search.** `fs::enumerate::walk(root, matcher, show_hidden, cancel, emit)`
  lists the root and every folder below it with `list_dir`, depth first from an explicit stack,
  and hands every entry whose own name matches to `emit`, renamed to its path relative to the root
  (`sub\dir\file.txt`), in batches of up to 256 (`SEARCH_BATCH`) or whatever matched in the last
  100 ms (`SEARCH_FLUSH`). It enters only `Kind::Directory` entries, so a junction, a symbolic link,
  a cloud placeholder or an app alias is listed when it matches but never entered (a loop through
  a link cannot happen), skips hidden and system folders unless the tab shows hidden files, and
  does not enter folders deeper than 64 (`SEARCH_MAX_DEPTH`). A folder below the root that cannot
  be listed is counted as skipped; a root that cannot be listed is the error. The icon keys come
  from the entry's real folder, so full-path keys (junctions, `.exe`, `.lnk`) are right.
  `spawn_search` runs it on a thread named `search-<pane>-<gen>`, sends each batch as `Msg::Batch`
  with the tab's generation to the enumeration channel and finishes with one `Msg::Done` (the
  count, the time, the error, the root's free space), so `drain`, the pass, the cursor, the icons
  and the status bar handle results as a listing's. The worker logs its own `[search]` lines,
  since the folder counts are its.
- **Search tabs** (`main.rs`). `Tab::search: Option<Search>` holds the pattern and an
  `Arc<AtomicU8>` that is both the cancel flag and its reason (`SEARCH_RUNNING`, or tab closed,
  navigated away, new search, Escape, F5). Ctrl+Shift+F (the window-level `key-pressed`, callback
  `search`) takes the filter box's text: with text, `start_search` opens a new tab after the
  active one whose folder is the current folder (the root) and starts the walk; with an empty box,
  the box opens with the focus and Return there starts the search (`State::search_armed`). In a
  search tab, Ctrl+Shift+F searches again in place. This PC cannot be searched (a note says so).
  The tab's title and the window title read `Search: <pattern>`; the breadcrumb is the root's,
  navigable as ever, followed by a crumb `Search: "<pattern>"` that does nothing when clicked; the
  path entry shows the root. The status bar reads "Searching... N found" (updated on the batch
  path through `update_status`, no timer), then "N found in S s", or "N found, stopped after S s"
  after Escape.
- **Cancellation.** Closing the tab, navigating it (a crumb, the path entry, Back, opening a folder
  result), a new search in it, F5 (which runs the search again) and Escape in its list set the
  flag; the walk reads it before each folder and after each batch, so it stops within one folder
  or one batch. The generation changing still drops late batches. After Escape the generation is
  the same, so the walk's `Done` lands and the results so far are sorted and kept.
- **What a search tab is not.** It has no directory watch (`[watch] pane=N no watch for <root>:
  search results`, once per tab); F5 runs the search again, as does a file operation (the active
  tabs refresh after every job) and a background search tab made stale by one when it is shown;
  switching away from it does not make it stale (it has no watch to lose). It is saved as its root
  folder (the tab list holds `path`) and restored as that folder. A bookmark of it bookmarks the
  root. It is never a drop target: a drop on its list or on its tab in the strip goes nowhere and
  logs why (`[dnd] pane N tab I shows search results: no drop there (it would land in the search's
  root)`, once per hover). Paste, a new folder, Shift+F5 / Shift+F6 into it from the other pane,
  and the background context menu are refused with a note for the same reason.
- **Results act on their real paths.** Every place that builds a path as `folder.join(name)` gets
  `root\sub\file.txt`: open (and the working directory is now the file's own folder), the file
  operations, copy, move, delete, cut and copy to the clipboard, the icons, middle-click into a new
  tab. A folder result opens in the same tab: the search is dropped and the root goes on the back
  stack, so Back returns to the root folder, not to the search. Two places needed the relative name
  split: `shellmenu::parse_items` (the context menu and the drag's data object) now resolves the
  names to their own folder with `common_parent`, since a child PIDL must be one level deep, and
  refuses items from different folders ("the items are in different folders; select items from one
  folder"); the rename editor shows the relative name and a rename keeps the item in its folder
  (`edit_commit` strips the folder prefix when the typed name keeps it).
- **Driver** (`auto.rs`): `filter <pane> <text>` (sets the box's text, opening it without the
  focus, and calls the `edited` callback the control fires), `search <pane>` (the callback
  Ctrl+Shift+F fires), `waitsearch <ms>` (waits until no search tab is still walking, then 300 ms
  for the pass), key names `CtrlF`, `CtrlShiftF` and any single character typed as itself (`key
  a`).
- **Logging.** `[filter] pane=N "<pattern>" (glob|substring): K of M shown, pass P ms` once per pass
  while a filter is on, `[filter] pane=N cleared`; `[search] pane=N gen=G start "<pattern>" in
  <root>`, `batch K (N so far)` (at most one a 100 ms), `done: N found, D folders walked, S skipped,
  in T ms`, `cancelled after D folders (<why>)`, `skipped N folders`, `pane=N Escape: stopping the
  search`, `pane=N no pattern yet: the filter box opens, Return searches`; `[tab] ... for a search`.
  The `[sort]` line is unchanged; a search's `Done` also prints the usual `[enum] ... done:` line.
- **Tests**, 99 to 103: `name_match_substring_glob_and_case` (empty matches all; substring either
  case; `é`/`É` and `ß`; anchored globs, `?` one character, `*` none or more; a relative name matched
  by its own name only); `filter_and_hidden_combine_in_the_pass_and_the_batches` (the filter never
  reveals a hidden file; hidden and filtered counted apart in the pass and in streamed batches; a
  cleared filter shows everything the switch lets through); `relative_names_resolve_to_their_own_folder`
  (`common_parent`); `search_walks_the_tree_without_links_or_hidden_folders` on a tree in
  `%TEMP%\twinfm-p3m2-test-<pid>\` (a match at depth 3 named `d1\d2\d3\HIT-deep.txt`, a matching
  folder, a junction to the root listed as a `Junction` keyed by its real path but not entered, a
  hidden folder entered only with hidden files shown, 4 found in 5 folders and 5 in 6; 620 matches
  in 21 folders; the flag set after the first batch stops the walk inside the first folder with
  that batch only; a missing root is an error), removed afterwards. `status_text_composition`
  covers "N of M items" and the search texts.
- **Docs**: `CLAUDE.md` (test count, the new driver steps and key names, the `[filter]` and
  `[search]` lines, the new scratch folder).

### Decisions

- **The box sits in the path bar's row, not in a row of its own.** A new row under the path bar
  would move the list's top and one in the status line would move its bottom; `dnd.rs`'s
  `LIST_TOP` and `LIST_BOTTOM_GAP`, the drop hit test, the auto-scroll edges and the driver's row
  positions all assume them. In the path bar's row only the breadcrumb (or the entry) gets 220 px
  narrower, through one pane-level binding.
- **Ctrl+F and Ctrl+Shift+F are window-level keys**, beside Ctrl+H and Ctrl+L, so they reach the
  active pane from the list, from the box and from the path entry alike.
- **The box shows the tab's filter.** It is open while the tab's filter is not empty (or right after
  Ctrl+F); an emptied box stays open until Escape, the clear glyph, or Return on the empty text.
- **The filter tests the name after the last `\`**, so in a search tab Ctrl+F filters the results
  by their own names, not by the folders above them; in a folder tab the names have no `\`.
- **Hidden and filtered are counted apart**, so the status reads "N of M items (K hidden)" with M the
  entries the hidden switch lets through, and the hidden count stays what it was.
- **Ctrl+Shift+F only; no Shift+Return.** An empty box opens with the search waiting, and Return
  there searches; one key, one place to document.
- **A search started from a folder tab leaves that tab's filter on**: the filter is the tab's, and
  the search tab starts unfiltered.
- **In a search tab Ctrl+Shift+F searches again in place** (the old walk logs `cancelled ... (new
  search)`) rather than opening yet another tab.
- **The walk hands out hidden matches in the folders it enters**; the tab's hidden-file switch shows
  or hides them like in a folder, so Ctrl+H in a search tab works without searching again. The
  status bar's "N found" counts the rows shown; the `[search] done` line counts every match sent.
  Which folders are entered is fixed when the search starts.
- **Depth first from an explicit stack**, popped last in first: each folder's last subfolder is
  walked first (in `C:\Windows` the first results came from `WinSxS`). Results are sorted by the
  pass anyway.
- **One atomic for the flag and the reason**, set with a compare-exchange from running, so the
  first reason wins and a cancel after the end is a no-op.
- **Folder results navigate the same tab and leave the search**; the search is not in the history,
  so Back returns to the root folder (its listing, not the search). Middle-click opens a folder
  result in a new tab, as before.
- **The refusals go further than the drop**: paste, a new folder, Shift+F5 / Shift+F6 into a pane
  showing search results, and the background context menu ("Paste", "New") would all act on the
  root, which is not what the list shows.
- **Escape in a search tab's list stops the walk and keeps the results**; it still cancels queued
  file operations as before.
- **The filter's per-entry cost is one `to_lowercase` of the name** (a `String` and `contains` for a
  substring; a `Vec<char>` only for a glob), measured below: within the range of the passes without
  a filter.

### Deviations

- **The bigdir names are `file_1.txt` to `file_40000.txt`**, not `file00001`, so `*7*` keeps 13,756
  of 40,000 rather than a tenth. Measured as it is; `1234` (14 kept) and `file0*` (none) were added.
- **Two places needed the relative names after all** (the brief expected none): the shell wants
  child PIDLs one level deep, so `parse_items` resolves the names to their own folder (context menu
  and drag), and the rename editor strips the folder prefix. Items from different folders get a note
  instead of a menu or a drag.
- **`open_row` passes the file's own folder as the working directory** (the tab's folder before;
  the same thing in a folder tab).
- **Refusals beyond the drop** (paste, new folder, copy and move into a search pane, background
  menu); see Decisions.
- **The `filter` driver step opens the box without the focus**, so a `key CtrlA` after it reaches
  the list, as the brief's check needs; `key CtrlF` is the focused path.
- **The search worker logs its own lines** from its thread (the folder counts are its), as the
  watcher thread does.
- **The Fluent `LineEdit` draws its own clear button** inside the box while it has the focus; it
  empties the text (a filter edit). The glyph after the box also closes the box.
- **Extra lines**: `[search] pane=N Escape: stopping the search`, `[search] pane=N no pattern yet:
  ...`, `[tab] ... for a search`, `[dnd] ... no drop there`.

### Deferred

- **Content search**: not asked for.
- **A rename in a search tab does not move the cursor to the renamed row**: the pass looks for the
  new name, the row's name is its relative path.
- **The context menu and drags over results from different folders** are refused (the shell's data
  object for items in several folders needs absolute PIDLs, `SHCreateShellItemArrayFromIDLists`).
- **Search results do not follow changes** (no watch); F5 or a file operation runs the search again.
  During that rerun the old results stay and the status bar shows their count until it ends.
- **No index**: the walk lists every folder; `C:\Windows` takes about 39 s here (157,000 folders).
  The Windows Search index is not used.
- **Not checked by a person**: typing in the box with the keyboard and mouse, the clear glyph by
  mouse, Narrator on the box (its accessible label is "Filter"), and a long search left running
  while working in the other pane.

### Scripted checks

RDP session, this worktree's `target\release\twinfm.exe` (a1d2081), every run with `TWINFM_SCRIPT`
(`@file` in the session scratchpad) and without `TWINFM_SOAK`; no run read or wrote
`%APPDATA%\twinfm\config.toml`. All exit code 0, no `panicked` line.

- **The filter, by the driver and through real key routing.** `nav 0 C:\Windows\System32; sleep
  1500; filter 0 *.dll; sleep 600; key CtrlA; sleep 300; filter 0 ; sleep 600; nav 0
  C:\Windows\System32; sleep 1500; key CtrlF; sleep 200; key a; key b; sleep 600; key Escape; sleep
  500; key Escape; key a; sleep 600; key Down; sleep 300; key CtrlF; sleep 200; key d; key l; key l;
  key Return; sleep 400; key Down`:

  ```
  [sort] pane=0 4904 of 4912 entries by Name asc=true hidden=8 cursor=None selected=0 in 1.5 ms
  [auto] Filter { pane: 0, text: "*.dll" }
  [filter] pane=0 "*.dll" (glob): 3638 of 4904 shown, pass 2.4 ms
  [auto] Key("CtrlA")
  [select] pane=0 All row=-1 selected=3638
  [auto] Filter { pane: 0, text: "" }
  [filter] pane=0 cleared
  [sort] pane=0 4904 of 4912 entries by Name asc=true hidden=8 cursor=None selected=3638 in 1.9 ms
  [auto] Key("CtrlF")
  [auto] Key("a")
  [auto] Key("b")
  [filter] pane=0 "ab" (substring): 51 of 4904 shown, pass 1.2 ms
  [auto] Key("Escape")
  [filter] pane=0 cleared
  [sort] pane=0 4904 of 4912 entries by Name asc=true hidden=8 cursor=None selected=0 in 1.4 ms
  [auto] Key("Escape")
  [auto] Key("a")
  [auto] Key("Down")
  [select] pane=0 Only row=0 selected=1
  [auto] Key("CtrlF")
  [auto] Key("d") ... Key("Return")
  [filter] pane=0 "dll" (substring): 3642 of 4904 shown, pass 1.9 ms
  [auto] Key("Down")
  [select] pane=0 Only row=0 selected=1
  ```

  Counted another way (PowerShell, `Get-ChildItem -Force` with hidden and system entries left
  out): 4,912 entries, 4,904 visible, 3,638 named `*.dll`, 51 containing `ab`, 3,642 containing
  `dll`. The two keys typed 0.2 s apart gave one pass. After Escape in the list, `a` filtered
  nothing (the negative control) and Down moved the list's cursor: the list had the focus. The
  second Ctrl+F found the box closed and created it with the focus (its `init`); Return applied at
  once and gave the list the focus.
- **Operations on filtered rows.** A scratch tree `%TEMP%\twinfm-p3m2-ops\` (removed afterwards),
  `src` holding `a1.txt`, `a2.txt`, `b1.log`: `split; nav 1 ...\dest; nav 0 ...\src; click 0 0;
  filter 0 *.txt; key CtrlA; copy; waitops 10000; filter 0 a2; key CtrlA; delete; waitops 10000`:

  ```
  [filter] pane=0 "*.txt" (glob): 2 of 3 shown, pass 0.3 ms
  [select] pane=0 All row=0 selected=2
  [fileops] submit id=1 pane=0 hwnd=0x230a64 Copy { sources: ["...\\src\\a1.txt", "...\\src\\a2.txt"], dest_dir: "...\\dest" }
  [fileops] done id=1 pane=0 tab=1 Done: Copied 2 items
  [filter] pane=0 "a2" (substring): 1 of 3 shown, pass 0.2 ms
  [select] pane=0 All row=-1 selected=1
  [fileops] submit id=2 pane=0 hwnd=0x230a64 Delete { sources: ["...\\src\\a2.txt"], permanent: false }
  [fileops] done id=2 pane=0 tab=1 Done: Deleted 1 item to the Recycle Bin
  [filter] pane=0 "a2" (substring): 0 of 2 shown, pass 0.2 ms
  ```

  Afterwards `src` held `a1.txt` and `b1.log`, `dest` `a1.txt` and `a2.txt`. The deleted 1-byte
  `a2.txt` is in the Recycle Bin. The refresh after the job kept the filter.
- **Search of `C:\Windows`.** `nav 0 C:\Windows; sleep 1500; filter 0 kernel32.dll; sleep 500;
  search 0; waitsearch 120000`:

  ```
  [filter] pane=0 "kernel32.dll" (substring): 0 of 103 shown, pass 0.3 ms
  [tab] pane=0 opened tab 2 of 2 (id 3) on C:\Windows for a search
  [watch] pane=0 no watch for C:\Windows: search results
  [search] pane=0 gen=3 start "kernel32.dll" in C:\Windows
  [search] pane=0 gen=3 batch 1 (1 so far)
  [enum] pane=0 first batch after 1916.1 ms
  [search] pane=0 gen=3 batch 2 (5 so far)
  ...
  [search] pane=0 gen=3 batch 9 (20 so far)
  [search] pane=0 gen=3 done: 20 found, 156836 folders walked, 45 skipped, in 38925 ms
  [search] pane=0 gen=3 skipped 45 folders
  [enum] pane=0 done: 20 entries, worker 38924.7 ms, request-to-done 38925.6 ms
  [sort] pane=0 20 of 20 entries by Name asc=true hidden=0 cursor=None selected=0 in 0.2 ms
  ```

  `dir /s /b C:\Windows\*kernel32.dll*` lists the same 20: `System32\kernel32.dll`,
  `System32\en-US\kernel32.dll.mui`, `SysWOW64\kernel32.dll`, and 17 in `WinSxS` (its `Backup`
  folder included). Screenshots (`tools/shot.ps1`) during and after a search of `C:\Windows`
  stopped with Escape after 4 s show the tab `Search: kernel32.dll`, the window title `TwinFM -
  Search: kernel32.dll`, the crumbs `This PC > C: > Windows > Search: "kernel32.dll"`, the rows
  named `WinSxS\wow64_microsoft-windows-kernel32_...\kernel32.dll`, and the status bar
  "Searching... 5 found", then "5 found, stopped after 4.0 s"; one taken with the box open on
  `ker` in System32 shows the box at the bar's right end and "96 of 4904 items (8 hidden)".
- **Results act on their full paths.** A scratch tree `%TEMP%\twinfm-p3m2-search\` (removed
  afterwards): `root\hit-top.txt`, `root\miss.txt`, `root\sub\hit-folder\`, and
  `root\sub\deep\hit-run.cmd`, a one-line script that writes `ran-marker.txt` next to itself.
  `split; nav 1 ...\dest; nav 0 ...\root; click 0 0; filter 0 hit; search 0; waitsearch 10000;
  click 0 2; key Return; click 0 2; copy; waitops 5000; rclick 0 2; menuclose; dragrow 0 2 0 40;
  dragcancel; dropfiles 0 -1 copy ...\drop-me.txt; dropfiles tab 0 1 copy ...\drop-me.txt;
  rclickempty 0; menuclose; key F5; waitsearch 10000; click 0 0; key Return; history back`:

  ```
  [search] pane=0 gen=5 done: 3 found, 4 folders walked, 0 skipped, in 5 ms
  [open] pane=0 C:\Users\jtkli\AppData\Local\Temp\twinfm-p3m2-search\root\sub\deep\hit-run.cmd: Ok(())
  [fileops] submit id=1 pane=0 hwnd=0xfb09ee Copy { sources: ["...\\root\\sub\\deep\\hit-run.cmd"], dest_dir: "...\\dest" }
  [fileops]   item 0x00000000 ...\root\sub\deep\hit-run.cmd -> ...\dest\hit-run.cmd
  [refresh] pane=0 gen=6 path=...\root
  [search] pane=0 gen=6 start "hit" in ...\root
  [menu] pane=0 row=2 1 target(s) at (436, 146) -> screen (705, 437)
  [menu] item menu for 1 item(s) in ...\root: Open in terminal | Copy path | --- | Open | Edit | ...
  [menu] closed after 429 ms (UI thread cpu 31 ms in the loop): id 0 = Nothing
  [dnd] pane=0 row=2: dragging 1 item(s)
  [dnd] drag image 96x96 px, 1 item(s), icon from the shell, composed in 22.5 ms: InitializeFromBitmap = 0x00000000
  [dnd] pane 0 tab 3 shows search results: no drop there (it would land in the search's root)
  [auto] dropfiles Row(0, -1) copy: 1 path(s), DragEnter effect none, Drop reported none
  [auto] dropfiles Tab(0, 1) copy: 1 path(s), DragEnter effect none, Drop reported none
  [fileops] pane=0 note: search results have no folder menu; right-click a result
  [refresh] pane=0 gen=9 path=...\root
  [search] pane=0 gen=9 done: 3 found, 4 folders walked, 0 skipped, in 5 ms
  [nav] pane=0 history push ...\root (1 back)
  [nav] pane=0 gen=10 path=...\root\sub\hit-folder
  [nav] pane=0 history back: ...\root\sub\hit-folder -> ...\root (0 back, 1 forward left)
  [sort] pane=0 3 of 3 entries by Name asc=true hidden=0 cursor=None selected=0 in 0.3 ms
  ```

  The script ran (`root\sub\deep\ran-marker.txt` appeared) and the copy landed in `dest`. Rows: 0
  `sub\hit-folder`, 1 `hit-top.txt`, 2 `sub\deep\hit-run.cmd`. Back returned to the root's own
  listing (3 entries), not to the search. Renames, in the same tree: `click 0 2; rename
  hit-renamed.cmd` gave `Rename { source: "...\\root\\sub\\deep\\hit-run.cmd", new_name:
  "hit-renamed.cmd" }`, and `rename sub\deep\hit-two.cmd` (the prefix kept) gave `Rename { source:
  "...\\root\\sub\\deep\\hit-renamed.cmd", new_name: "hit-two.cmd" }`; both `Done`, the file stayed
  in `sub\deep`.
- **Cancellation, on `C:\`.** `filter 0 kernel32.dll; search 0; sleep 1500; filter 0 ntdll.dll;
  search 0; sleep 1500; crumb 0 1; tab 1; key CtrlShiftF; sleep 1500; key Escape; tab 1; search 0;
  sleep 1500; tab close; tab 2; key CtrlShiftF; key n; key t; key d; key l; key l; key Return;
  sleep 1500; tab close`:

  ```
  [tab] pane=0 opened tab 2 of 2 (id 3) on C:\ for a search
  [search] pane=0 gen=2 start "kernel32.dll" in C:\
  [search] pane=0 gen=3 start "ntdll.dll" in C:\
  [search] pane=0 gen=2 cancelled after 13142 folders (new search)
  [auto] Crumb { pane: 0, index: 1 }
  [nav] pane=0 gen=4 path=C:\
  [search] pane=0 gen=3 cancelled after 18432 folders (navigated away)
  [auto] Key("CtrlShiftF")
  [tab] pane=0 opened tab 2 of 3 (id 4) on C:\ for a search
  [search] pane=0 gen=6 start "kernel32.dll" in C:\
  [auto] Key("Escape")
  [search] pane=0 Escape: stopping the search
  [search] pane=0 gen=6 cancelled after 20109 folders (Escape)
  [sort] pane=0 5 of 5 entries by Name asc=true hidden=0 cursor=None selected=0 in 0.2 ms
  [tab] pane=0 opened tab 2 of 4 (id 5) on C:\ for a search
  [search] pane=0 gen=8 start "kernel32.dll" in C:\
  [auto] Key("CtrlW")
  [tab] pane=0 closed tab 2 (id 5), 3 left
  [search] pane=0 gen=8 cancelled after 15677 folders (tab closed)
  [auto] Key("CtrlShiftF")
  [search] pane=0 no pattern yet: the filter box opens, Return searches
  [auto] Key("Return")
  [search] pane=0 gen=9 start "ntdll" in C:\
  [auto] Key("CtrlW")
  [search] pane=0 gen=9 cancelled after 19254 folders (tab closed)
  ```

  Every way out stopped the walk with its reason. After Escape the five results found so far were
  sorted and kept. Ctrl+Shift+F through the key path opened a search tab, and on an empty box opened
  it for the pattern: five typed keys, then Return, searched in place in that tab.

**Measurements** (this machine, RDP, from the lines above and `C:\traces\bigdir`, 40,000 one-byte
files `file_1.txt` to `file_40000.txt`; the pass is filter plus sort on the worker):

| Pass on bigdir | ms |
|---|---|
| no filter (seven passes: three sorts, four clears) | 18.3 to 25.0 |
| `*7*` glob, 13,756 kept (four passes, one after F5) | 17.2, 17.9, 19.0, 22.8 |
| `1234` substring, 14 kept | 6.4 |
| `file0*` glob, none kept | 12.3 |

On System32 (4,912 entries): 1.5 ms without a filter, 2.4 ms with `*.dll`. A filter that keeps few
rows makes the pass cheaper (less to sort); the per-entry lowercase costs about 6 ms per 40,000
names. The search of `C:\Windows` for `kernel32.dll`: 20 found, 156,836 folders walked, 45 skipped,
38.9 s, first result after 1.9 s.

### Gate

Pending: the lead builds `target-p3m2` from the PR head and Jeremy runs `tools/wpr-triplet.ps1
-Reference p3m1 -Candidate p3m2 -Tag ab11`. The gate scenario opens no filter and no search tab:
what it meets of this milestone is, per pane, one `if` (the closed box) and one width binding on
the path bar, and nothing in the rows.
