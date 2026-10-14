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
  tab. A folder result opens in the same tab, and the search goes on the back stack (next bullet
  but one). Two places needed the relative name
  split: `shellmenu::parse_items` (the context menu and the drag's data object) now resolves the
  names to their own folder with `common_parent`, since a child PIDL must be one level deep, and
  refuses items from different folders ("the items are in different folders; select items from one
  folder"); the rename editor shows the relative name and a rename keeps the item in its folder
  (`edit_commit` strips the folder prefix when the typed name keeps it).
- **Searches are in the tab's history** (`tabs.rs`, `main.rs`; changed after Jeremy's console
  check, see Decisions). `tabs::History` holds `tabs::Place` entries, a folder or a search (`path`
  the root, `search` the pattern), instead of bare folders; `push`, `back` and `forward` take and
  give places, and two places are the same when the folder matches ignoring ASCII case and the
  pattern matches exactly. `Tab::place()` is where a tab is. Opening a folder result pushes the
  search; Back (Alt+Left, the mouse's Back button) to a search runs it again in that tab under
  the same root (`start_search(.., Some(root))`, nothing pushed) and Forward from it returns to the
  folder. A new search tab starts with its root folder on its back stack (it was searched from
  there), so Back from a search goes to the root folder; a new search in a search tab pushes the
  old one. The `[nav]` history lines name a search as `search "<pattern>" in <root>`.
- **Focus and keys around the box** (changed after the console check). Starting a search gives the
  results list the focus (`focus-pane` after the state borrow), and whenever Rust closes an open
  box in the active pane (a navigation, a tab switch, a new tab, a search) it gives that pane's
  list the focus (`set_filter_box`): a box destroyed while it holds the focus leaves the focus
  nowhere, and then no key reached the window at all. The rule for keys while the box has the
  focus: the window's keys pass through it (Ctrl+W, Ctrl+T, Ctrl+Tab, Ctrl+1 to 9, F3, F5, F9,
  Ctrl+H, Ctrl+L, Ctrl+F, Ctrl+Shift+F: the box does not take them); F7 does what it does in the
  list (a new folder, or the refusal in a search tab); text keys stay the box's (Ctrl+V, Ctrl+A,
  Ctrl+C, Ctrl+X, Ctrl+Z, Delete, Backspace, the arrows but Down, Home, End). The pane's
  `filter-focused` property and its `changed filter-open` handler are gone (the handler did not
  restore the focus after a tab switch).
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
- **Tests**, 99 to 104: `history_holds_searches_like_folders` (a search and a folder result in
  the history, back and forward through both, a search is not its root, the log form);
  `name_match_substring_glob_and_case` (empty matches all; substring either
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
- **A search is a place in the tab's history, like a page in a browser** (Jeremy, after his console
  check, 2026-10-13, reversing the first version, where Back from a folder result went to the root
  folder and the search was lost). A folder result opens in the same tab; Back runs the search
  again (same root, same pattern), Forward returns to the folder; Back from the search goes to the
  root folder. Middle-click opens a folder result in a new tab, as before.
- **Starting a search moves the focus to the results list.** In Jeremy's check Ctrl+F, typing,
  Ctrl+Shift+F, then Ctrl+W did nothing: the new tab's empty filter closed the box that held the
  focus, the focus went nowhere and no key reached the window (Ctrl+W, Ctrl+V and F7 alike: the
  paste and new-folder refusals never ran). The scripted check had used the `filter` step, which
  opens the box without the focus.
- **Window keys pass the box, text keys stay in it, F7 acts as in the list.** Checked with real
  keys: with the box focused, Ctrl+W, Ctrl+T, Ctrl+Tab, F3, F5 and F9 already reached the window
  (the LineEdit does not take them); only F7, a list key, needed forwarding.
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
- **Result names are cut short** in a search tab: the Name column shows the relative path and
  elides it from the right (`Windows\WinSxS\a...` in Jeremy's console check, 2026-10-13); a wider
  Name column for search tabs, or the folder in its own column, is a follow-up.
- **Refusal notes stay in the status bar** (paste, new folder, copy and move into a search tab);
  Jeremy is fine with that for now (2026-10-13); a louder message would be a product decision.

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

- **After Jeremy's console check: the focus, the keys and the history** (same rules; scratch
  trees in `%TEMP%\twinfm-p3m2-fix\` and `%TEMP%\twinfm-p3m2-search\`, removed afterwards). Before
  the fix, `key CtrlF; key n; key t; key d; key l; key l; key CtrlShiftF; sleep 500; key CtrlW` on
  `C:\` logged `[search] ... start "ntdll" in C:\` and, after `Key("CtrlW")`, only more `batch`
  lines: the tab stayed open. A longer run showed that every key after Ctrl+Shift+F (Down, Escape,
  Ctrl+W, Ctrl+F, Ctrl+T) logged nothing. After the fix the same script gives:

  ```
  [search] pane=0 gen=2 start "ntdll" in C:\
  [auto] Key("CtrlW")
  [tab] pane=0 closed tab 2 (id 3), 1 left
  [search] pane=0 gen=2 cancelled after 5217 folders (tab closed)
  ```

  Keys with the box focused (`key CtrlF` before each): `CtrlT` opened a tab, `CtrlTab` switched,
  `F5` refreshed, `F9` hid and showed the sidebar, `F3` opened the split, `CtrlW` reached the
  pane (`the last tab stays` on the new pane), and `Down` after a search moved the list's cursor
  (`[select] pane=0 Only row=0 selected=1`). Refusals with real keys in a search tab, after `cc`
  on a file in the other pane, `click 0 1` on a result:

  ```
  [auto] Key("CtrlV")
  [fileops] pane=0 note: search results: paste into a folder
  [auto] Key("F7")
  [fileops] pane=0 note: search results: open a folder to make a folder in it
  [auto] Key("CtrlF")
  [auto] Key("F7")
  [fileops] pane=0 note: search results: open a folder to make a folder in it
  ```

  The root held `hit-top.txt` and `sub\hit-folder` before and after (nothing pasted or created); a
  screenshot shows the status bar "search results: paste into a folder | 2 found in 0.0 s". History,
  `nav 0 <root>; filter 0 hit; search 0; waitsearch 5000; click 0 0; key Return; backclick 0;
  waitsearch 5000; forwardclick 0; key AltLeft; waitsearch 5000; key AltRight; key AltLeft;
  waitsearch 5000; key AltLeft; key AltLeft; key AltRight; waitsearch 5000`:

  ```
  [nav] pane=0 history push ...\root (1 back)
  [tab] pane=0 opened tab 2 of 2 (id 3) on ...\root for a search
  [search] pane=0 gen=3 done: 2 found, 3 folders walked, 0 skipped, in 4 ms
  [nav] pane=0 history push search "hit" in ...\root (2 back)
  [nav] pane=0 gen=4 path=...\root\sub\hit-folder
  [nav] pane=0 Back button (active pane was 0, now 0)
  [nav] pane=0 history back: ...\root\sub\hit-folder -> search "hit" in ...\root (1 back, 1 forward left)
  [search] pane=0 gen=5 start "hit" in ...\root
  [search] pane=0 gen=5 done: 2 found, 3 folders walked, 0 skipped, in 4 ms
  [nav] pane=0 Forward button (active pane was 0, now 0)
  [nav] pane=0 history forward: search "hit" in ...\root -> ...\root\sub\hit-folder (2 back, 0 forward left)
  [auto] Key("AltLeft")
  [nav] pane=0 history back: ...\root\sub\hit-folder -> search "hit" in ...\root (1 back, 1 forward left)
  [search] pane=0 gen=7 start "hit" in ...\root
  [auto] Key("AltRight")
  [nav] pane=0 history forward: search "hit" in ...\root -> ...\root\sub\hit-folder (2 back, 0 forward left)
  [auto] Key("AltLeft")      (the search again, gen 9)
  [auto] Key("AltLeft")
  [nav] pane=0 history back: search "hit" in ...\root -> ...\root (0 back, 2 forward left)
  [auto] Key("AltLeft")
  [nav] pane=0 history back: nothing to go back to
  [auto] Key("AltRight")
  [nav] pane=0 history forward: ...\root -> search "hit" in ...\root (1 back, 1 forward left)
  [search] pane=0 gen=11 start "hit" in ...\root
  ```

  The filter, results and cancellation checks above were re-run on this build with the same
  lines (`3638 of 4904`, `51`, `3642`; `[open]` with the full path, copy, the item menu, the drag,
  both drops refused; every cancel reason). The results check's `history back` now runs the
  search again (`history back: ...\hit-folder -> search "hit" in ...\root`, `[search] ... gen=11
  start`).

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

**Passed (2026-10-13), one triplet.** Console WPR, Jeremy elevated, `tools/wpr-triplet.ps1
-Reference p3m1 -Candidate p3m2 -Tag ab11`, accessibility on, Narrator off, no other TwinFM
window: `target-p3m1` (537f606) against `target-p3m2` (e2b6343, the PR head, built from a
detached checkout). Analysed with `tools/analyze-trace.py --symbols --exe`.

| Run | P3-M1 (`target-p3m1`) | P3-M2 e2b6343 |
|---|---|---|
| 1 | 21.5% (2152 ms) | 26.4% (2636 ms) |
| 2 | 26.2% (2622 ms) | 27.1% (2706 ms) |
| 3 | 26.9% (2693 ms) | 26.9% (2685 ms) |
| **median** | **26.2%** | **26.9%** |

The medians differ by 0.7, and the reference's run 3 is not below the candidate's run 1, so
neither half of the rule calls a gap. The machine rose by five points during the candidate's first
run and stayed there (Defender's `MsMpEng` at 460 MB working set during the analysis, most likely
scanning after the console checks' search had walked `C:\`), which the pairwise reading tolerates:
runs 2 and 3 pair at +0.9 and 0.0 with every category in proportion (accessibility 261 / 262,
247 / 250; render 1528 / 1581, 1599 / 1552; draw_text 861 / 886, 907 / 879; rows 514 / 536,
545 / 502; timers 909 / 940, 915 / 905; heap 263 / 253, 236 / 255). No filter, matcher, search or
walk frame of ours appears in any candidate window (the `filter_item` and `WalkFrameChain` frames
are Slint's renderer and the kernel's stack walker, in both binaries). Traces
`C:/traces/twinfm-ab11-{p3m1,p3m2}-{1,2,3}.etl`, reports under `C:/traces/export/`. Lesson for the
protocol: run the console checks AFTER the triplet, or wait for Defender to settle; a recursive
search of `C:\` before the gate costs the session five points of noise.

**After the gate** (fixes from Jeremy's console checks, see Decisions): searches in the tab's
history (`tabs::Place`), the focus given to the results list when a search starts and to the list
when Rust closes an open filter box, F7 forwarded from the box, and the pane's `filter-focused`
property and `changed filter-open` handler removed. None of it touches the rows. In the gate
scenario (no box, no search) what runs differently is a `Place` cloned per navigation instead of a
`PathBuf` (per navigation, not per frame) and one change handler fewer per pane.

## P3-M3: folder sizes

The backlog was two deferrals: M6a's "the status bar sums file sizes only; folder sizes are not
computed" (`phase-1-plan.md` § M6a Deferred) and P2-M1's "subtree watches (sizes or counts of
subfolders), not needed until folder sizes exist" (`phase-2-plan.md` § P2-M1 Deferred). TwinFM now
adds up the folders the user selects (or, with Alt+Return, every folder of the listing) on one
worker, shows the totals in the status bar while they grow and in the Size column once they are
known, and watches the whole tree below the folder while any size is cached or being computed, so
a change inside a sized folder drops its size. The gate scenario never meets any of it: nothing is
sized without a selection or Alt+Return, and the row is unchanged.

### What landed (2026-10-13)

- **The size worker** (`src/fs/sizes.rs`). `measure(root, max_depth, cancel, each_dir)` walks
  `root` depth first from an explicit stack with `enumerate::list_dir` (now `pub(crate)`; no second
  find loop) and adds up every file's size, hidden and system files included (disk usage, not what
  the list shows). Only `Kind::Directory` is entered; a junction, symbolic link, cloud placeholder
  or app alias is counted (as a folder or a file by its attributes) at 0 bytes and never entered.
  Folders deeper than 64 (`SIZE_MAX_DEPTH`) are counted and not entered (`truncated`). A folder
  that cannot be listed counts as skipped. The cancel flag (`AtomicBool`) is read before each
  folder; `each_dir` gets the totals after each folder. Result: `Totals { bytes, files, folders,
  truncated, skipped }`, None when cancelled. One thread ("sizes") serves a shared
  `Mutex<Queue>` + `Condvar`, newest first (`Queue::pop` takes the last request and drops cancelled
  ones it meets), like the icon pool's queue; the UI pushes into it directly (one worker, so no
  feeder thread). A request is (pane, tab id, the tab's size generation, folder name, path, cancel
  flag); replies (partial every 250 ms, `PARTIAL_EVERY`, and the final one) go over a channel and
  wake the UI with `invoke_from_event_loop(drain_sizes)`. A cancelled walk sends nothing.
- **Per-tab bookkeeping** (`fs::sizes::TabSizes`, pure, on `Tab::sizes`): `cache` (folder name to
  totals), `pending` (name to cancel flag, latest partial totals, and whether Alt+Return asked for
  it), `generation` (bumped whenever the cache is cleared), `overflows` (subtree overflows in a
  row). `plan(selected, 100)` says what a selection change queues (the first 100 selected folders in
  display order that are neither cached nor in flight) and cancels (requests a selection made for
  folders no longer selected); `summary(selected)` gives the status bar's folder part (known bytes:
  finished sizes plus partial totals; how many are pending; how many are not sized);
  `invalidate(child, now)` marks a cached size stale by the first component of a changed path,
  ignoring ASCII case, with its re-walk time (`stale`), and returns nothing when it is stale already;
  `due(now)` and `next_due()` hand out the re-walks; `walk_ms` holds each folder's last walk time and
  `from_all` the folders Alt+Return sized; `forget(name)` drops one size; `clear()` cancels
  everything and empties the cache. `drain_sizes` drops a reply whose tab is
  gone, whose generation is old, or whose cancel flag is not the pending request's own.
- **What the user sees.** Selecting folders (click, Ctrl+click, Shift+click, Ctrl+A, Space,
  Shift+arrows, a plain arrow onto a folder) queues the selected plain folders that have no size
  yet; a selection change cancels the walks for folders no longer selected. The status bar's middle
  text (`statusbar::mid_text`, now with a `DirTotals` argument) reads "N selected, X" once every
  selected folder is sized (files and folders together), "N selected, X so far..." while some are
  walking (X grows with the partial totals four times a second), and "N selected, X so far (K
  folders not sized)" past the 100-folder cap, "N selected, X (updating...)" when every selected
  folder has a size and some are out of date; with only files selected it reads as before. The Size
  column shows a folder's size when its walk ends: `PaneModel::set_size` writes the entry in the
  listing (`all`) and in the shown rows and notifies that one row; the row and its Slint side are
  unchanged (the cell is the same `row.size` text files use). A folder with no computed size shows an
  empty cell as before. A cached folder shows its size at once when selected again. Alt+Return in
  the list (`pane.slint`'s key function, `command("sizeall")`) sizes every folder of the listing
  that has no size, at most 500, the ones nearest the cursor first. Search tabs and This PC get no
  sizes (Alt+Return there leaves a note in the status bar).
- **When sizes are forgotten.** Navigation, F5 / Ctrl+R, a tab leaving the foreground (its watch
  goes with it, as P2-M1 made it stale) and closing a tab clear the tab's cache and cancel its walks
  (`Tab::drop_sizes`). A refresh started by the directory watch or after a file operation keeps
  them: `Msg::Done` puts the cached sizes back into the new listing (`PaneModel::apply_sizes`, one
  pass over it, only when the cache is not empty), so the pass that follows shows and sorts them.
- **Subtree watches** (`src/fs/watch.rs`). `Watcher::watch` takes `subtree`; `watch_active` asks
  for it while the tab has sizes cached or in flight (`TabSizes::any`) and has not overflowed twice
  in a row, so the watch is re-armed with `bWatchSubtree = TRUE` when the first folder is queued and
  goes back to shallow when the cache is cleared or the last request is cancelled with nothing
  cached (`[watch] subtree on|off <path>`). Only a subtree watch parses its records
  (`parse_records`: `NextEntryOffset`, `Action`, `FileNameLength`, the UTF-16 name, bounds-checked)
  and delivers `Event::Subtree { top, changes, overflow }` instead of `Changed`: `top` when a record
  names an entry of the folder itself, `changes` one (child, first record) pair per child of the
  folder that a record lay below or added, removed or renamed (a record that only modified a
  subfolder's own entry, `modified a`, does not count: see Decisions), `overflow` for
  `ERROR_NOTIFY_ENUM_DIR` or an empty completion. The 250 ms coalescing is unchanged. On the UI side
  (`watch_event`): a change below a sized subfolder marks its size stale (next bullet); only `top`
  or an overflow refreshes the listing, otherwise one line says
  `changed below the folder: no refresh`. An overflow drops every cached size, re-queues the
  selected ones and refreshes; after two in a row (no parsed event between) the tab's watch stays
  shallow (`subtree disabled for this tab after two overflows`); navigation resets the count.
- **Re-walks after a change, with a back-off** (added after the lead's review, c5f29c4). A change
  below a sized folder does not re-queue it at once: the size is marked stale (`[size] invalidated
  <path> (<change>)` and `[size] stale <path>: re-walk in N s`, once per window) and walked again
  after max(2 s, twice its last walk's duration) (`REWALK_MIN`, `rewalk_delay`); further changes in
  that window join the one re-walk and log nothing. One single-shot `slint::Timer` per pane
  (`State::size_timer`) is started for the earliest stale size of the active tab and restarted when
  a new one is marked; when it fires (`rewalk_due`), each due folder is walked again if it is still
  selected or was sized by Alt+Return (the tab is still on its folder: navigation clears it all), and
  otherwise forgotten (`[size] stale <path>: not selected any more, size dropped`, its cell
  emptied). Meanwhile the old size stays in the cell and counts in the status bar, which reads "N
  selected, X (updating...)" (see Decisions); deselecting a folder whose re-walk is running cancels
  it and drops the out-of-date size. A change during a re-walk marks the size stale again, for the
  next window.
- **Logging.** `[size] queued <path> (K in queue)`, `partial <path>: X bytes, F files, D folders`
  (from the worker, every 250 ms of a walk), `done <path>: X bytes, F files, D folders in T ms (S
  skipped)` (`, truncated at depth 64` when it was), `cancelled <path> (<why>)` (deselected,
  navigation, F5, tab in the background, tab closed), `cache cleared pane=N (<why>)`, `invalidated
  <path> (<change>)`, `stale <path>: re-walk in N s`, `stale <path>: not selected any more, size
  dropped`, `capped: K of M folders sized`, `pane=N Alt+Return: K of M folder(s) to size`;
  `[watch] subtree on <path>`, `subtree off <path>`, `changed below the folder: no refresh`,
  `subtree overflow <path>: sizes dropped`, `subtree disabled for this tab after two overflows`.
- **Driver** (`auto.rs`): `sizeall <pane>` (focuses the pane's list and presses Alt+Return, key name
  `AltReturn`), `waitsize <ms>` (until no tab has a size queued or walking, so the last reply has
  landed, then 300 ms; `main::sizes_busy`).
- **Tests**, 104 to 111: `walker_counts_hidden_files_and_skips_links` on a tree in
  `%TEMP%\twinfm-p3m3-test-<pid>\` (hidden and system files counted, 1,192 bytes in 6 files; a
  junction to the root counted as a folder and not entered; a folder whose listing is denied to
  Everyone with `icacls /deny *S-1-1-0:(RD)` counted as skipped, the deny removed before the tree is
  deleted; a depth limit of 2 truncates and leaves the deepest file out; a flag set after the first
  folder stops the walk there; a missing root gives one skipped);
  `queue_serves_newest_first_and_drops_cancelled`; `selection_plan_caps_and_cancels` (150 selected
  folders: 100 queued from the top, `capped (100, 150)`; the summary with two sized, one walking and
  the rest not sized; a new selection cancels the selection's request and keeps Alt+Return's);
  `cache_invalidates_by_first_component` (`sub\deep\x.txt` marks `Sub` stale, not `sub2`, with a
  30 s re-walk after a 15 s walk and 2 s after a short one; a second change in the window returns
  nothing; the old size stays cached; `due` hands them out at their times; `clear` cancels, empties
  and bumps the generation); `folder_sizes_in_the_middle_text` (also "(updating...)", and "so far..."
  winning when one folder has no size); the summary counts a stale size as updating; `records_parse` (a two-record buffer, a cut record dropped);
  `subtree_watch_tells_deep_changes_from_top_ones` (a file two levels down gives `Subtree` with
  `top` false and the child `sub`; a top-level file gives `top` true; back to shallow, a deep file
  gives nothing and a top-level one a plain `Changed`).
- **Docs**: `CLAUDE.md` (test count, `sizeall`, `waitsize`, `AltReturn`, the `[size]` lines and the
  `[watch]` additions, the new scratch folder); the stray BEL in the P3-M2 Deferred bullet above is
  now `Windows\WinSxS\a...` (`grep -c $'\x07' docs/phase-3-plan.md` gives 0).

### Decisions

- **Records are parsed**, not "invalidate every size on any event": the parser is twenty lines and
  the first component of a record's path is the sized child it lies under.
- **A record that only modifies a subfolder's own entry (`modified a`) does not drop `a`'s size.**
  NTFS updates a folder's entry in its parent lazily, after something has opened the folder, so the
  first walk of a freshly changed folder produces one; dropping the size for it re-walked every
  newly sized folder once more (seen in the first scripted run: `invalidated ...\c (modified c)`
  right after `c`'s walk). A content change always comes with a record below the child. The record
  still sets `top`, so the listing refreshes as a shallow watch would have.
- **A refresh by the watch or after a file operation keeps the sizes; F5 drops them.** The brief
  said the cache is cleared on refresh. Clearing on the watch's refreshes would restart every walk
  whenever anything at the top of the folder changes (`NTUSER.DAT`'s logs in `C:\Users\jtkli`), and
  the subtree watch already drops what an operation changed.
- **A tab leaving the foreground forgets its sizes** (and cancels its walks): its watch moves to the
  tab that takes its place, so its sizes could go stale unseen, as its listing does (P2-M1).
- **The Size column shows finished sizes only**; partial totals are in the status bar ("so far...").
  A cell never shows a number that is still growing or that a cancel would leave behind.
- **No automatic re-sort on a reply.** The size goes into the listing's entry, so it takes part the
  next time the user sorts (by Size or anything else); rows do not jump while sizes arrive.
- **Alt+Return, no menu item.** One key in the list, beside Return. Its requests are not cancelled
  by selection changes (the user asked for all of them); navigation, F5, a tab switch or close do.
- **Only plain folders are sized.** A selected junction, directory link, cloud folder or drive counts
  no bytes, as before, and is never queued: sizing it would walk what it points to.
- **Hard links count once per link** (no file IDs are read): `C:\Windows` reads 43.8 GiB, most of it
  `WinSxS` links to files that also sit in `System32`.
- **The request carries the tab's size generation, not the listing's.** The listing generation
  changes on every refresh, which would drop a walk's result whenever the watch refreshes the folder;
  the size generation changes only when the cache is cleared. The cancel flag's identity is checked
  too, so a reply for a request cancelled and queued again is dropped.
- **Newest first, cursor first.** A selection pushes its folders last first, so the topmost selected
  folder is served first; Alt+Return pushes the farthest from the cursor first.
- **The worker logs `partial` and `done`** (it has the times and counts), as the search worker logs
  its own lines; the UI logs queue, cancel, cache and invalidation lines.
- **One worker, no feeder thread.** The UI pushes into the mutex-guarded queue and signals the
  condition variable itself; the icon pool's feeder exists to wake one of two workers.
- **A changed folder is walked again after max(2 s, twice its last walk)**, once per window (the
  lead's rule after review): `C:\Windows`, invalidated by Prefetch within a second of each 15.6 s
  walk, is walked at most every 31 s instead of back to back. One timer per pane rather than per
  folder: only the active tab has sizes, and one single-shot timer aimed at the earliest due time
  covers any number of stale folders.
- **The status texts with out-of-date sizes**: the old size counts and the text is "N selected, X
  (updating...)" while every selected folder has some size; as soon as one has none at all (walking
  for the first time, or past the cap) the "so far" forms apply as before ("X so far...", "X so far
  (K folders not sized)") and "(updating...)" is not added. The Size cell keeps the last finished
  size until the new one lands.
- **A stale size that is no longer wanted is dropped, not kept**: when its re-walk is due and the
  folder is neither selected nor Alt+Return's, or when a running re-walk is cancelled by a
  deselection, its cell is emptied, since nothing will bring it up to date.

### Deviations

- **The subtree watch is on while sizes are cached or in flight**, not only while cached. With
  "cached" alone, a selected folder whose size was dropped emptied the cache, the watch went shallow
  (closing the handle), the re-queued walk ended, and the watch went subtree again (a new handle): in
  a scripted burst below a sized folder the log showed `subtree off` / `subtree on` around every one
  of the re-walks, with the changes in between unseen. Now the handle stays while anything is sized
  or being sized.
- **One row is updated through the model's own `row_changed`** (`PaneModel::set_size`), not
  `set_row_data`: `PaneModel` is our own `Model`, and a rename re-lists the folder rather than
  updating a row.
- **The cache is not cleared on every refresh, and is cleared when a tab leaves the foreground**
  (see Decisions).
- **Extra lines**: `[size] pane=N Alt+Return: K of M folder(s) to size`, `[watch] ... changed below
  the folder: no refresh`, and `, truncated at depth 64` after a `done` line.
- **"100 folders in projects"**: `C:\Users\jtkli\projects` holds 20 folders, so Ctrl+A there sized
  20; the cap of 100 was shown in `C:\Windows\WinSxS` instead (Scripted checks).

### Deferred

- **`waitsize` does not wait for a stale size's re-walk** (it waits for queued and walking sizes
  only); a script that needs the re-walk sleeps past the delay.
- **The first walk of a freshly changed folder costs one more walk** (`invalidated ...\a (modified
  a\sub)` after it, the same lazy NTFS entry update one level down) and one listing refresh (the
  top-level `modified a`).
- **The overflow path was not triggered.** Four threads creating 32,000 files with long names below
  a sized folder gave 66 parsed events and no overflow: the watcher thread re-arms its read faster
  than that. "Disabled after two overflows" is therefore unexercised.
- **The walk pays for what the listing needs**: `list_dir` builds each entry's type name and icon key
  (cached per extension) and formats its time. Measured as it is (`C:\Windows` in 15.6 s); a leaner
  callback for the size worker would need a second shape of the loop.
- **A huge selection rebuilds its folder list on every status update** (`selected_folders` and
  `summary`): with Ctrl+A in `WinSxS` (26,867 folders) that is once per reply and per partial total,
  on the UI thread. Not measured; per selection change and per reply, never per frame.
- **A subtree watch on a drive root sees the whole volume.** On `C:\` the watcher parsed every change
  on the drive while `C:\Windows` walked (47 coalesced events in the run below, none refreshed).
- **A walk cancelled during its last folder still logs `done`** (the flag is read before each folder);
  its reply is dropped. Seen once in the WinSxS run (548 done and 53 cancelled for 600 queued).
- **No "size on disk"** (allocation size, compression), no sizes for search results, background
  tabs, or the other drives on This PC (which show their free space).

### Scripted checks

RDP session, this worktree's `target\release\twinfm.exe`: the final build, c5f29c4 (with the
re-walk back-off), for the gate scenario, the scratch-tree check, the back-off check and the
`C:\Users\jtkli` run; 7aacc01 for the burst; 1ed54d3, the build before the Deviation, for the
others (the worker, the caps and the cache are the same in all three), every run
with `TWINFM_SCRIPT` inline and without `TWINFM_SOAK`; no run read or wrote
`%APPDATA%\twinfm\config.toml` (`[settings] scripted run: defaults, nothing saved`). Scratch trees
under `%TEMP%`, removed afterwards. All exit code 0, no `panicked` line.

- **The gate scenario meets no size code.** The gate script's steps (`sleep 2000; split; sleep 500;
  wheel 1 -120 16; wheel 0 -120 16; drag 1 400 1000; drag 0 400 1000; sleep 6000; sort 0 1; sleep
  1000; sort 1 2; sleep 3000; quit` on `C:\traces\bigdir`) select nothing and press no Alt+Return.
  A run of it on the final build: `size lines: 0, subtree lines: 0, panicked: 0`; the four `[sort]`
  lines as before (`40000 of 40000 entries ... selected=0`).
- **Selection, cache, invalidation, Alt+Return, sort, F5** (`%TEMP%\twinfm-p3m3-check\root`: `a`
  with `a1.bin` 1,000 bytes and `sub\x0.bin` 24, `b` with 5,000 and a hidden 120, `c` with 300,
  `f.txt`), script `sleep 1000; click 0 0; waitsize 5000; ctrlclick 0 1; waitsize 5000; click 0 3;
  sleep 300; click 0 0; sleep 300; touch 0 a\sub\new.txt; sleep 1500; waitsize 5000; touch 0 top.txt;
  waitwatch 3000; sizeall 0; waitsize 5000; sort 0 1; sleep 800; click 0 0; ctrlclick 0 1; ctrlclick
  0 2; sleep 4000; key F5; sleep 1500; quit` (the driver refuses `touch` with a path, so the deep
  change came from the shell: `a\sub\new.bin`, 2,000 bytes, written 5 s after the start):

  ```
  [size] queued ...\root\a (1 in queue)
  [watch] pane=0 subtree on ...\root
  [size] done ...\root\a: 1024 bytes, 2 files, 1 folders in 3 ms (0 skipped)
  [size] invalidated ...\root\a (modified a\sub)            (the lazy entry update, Deferred)
  [size] stale ...\root\a: re-walk in 2.0 s
  [select] pane=0 Toggle row=1 selected=2
  [size] queued ...\root\b (1 in queue)
  [size] done ...\root\b: 5120 bytes, 2 files, 0 folders in 1 ms (0 skipped)
  [select] pane=0 Only row=3 selected=1
  [size] stale ...\root\a: not selected any more, size dropped
  [select] pane=0 Only row=0 selected=1
  [size] queued ...\root\a (1 in queue)
  [size] done ...\root\a: 1024 bytes, 2 files, 1 folders in 2 ms (0 skipped)
  [size] invalidated ...\root\a (added a\sub\new.bin)
  [size] stale ...\root\a: re-walk in 2.0 s
  [watch] pane=0 tab=1 changed below the folder: no refresh
  [size] queued ...\root\a (1 in queue)                     (2 s later)
  [size] done ...\root\a: 3024 bytes, 3 files, 1 folders in 2 ms (0 skipped)
  [size] invalidated ...\root\a (modified a\sub)
  [size] stale ...\root\a: re-walk in 2.0 s
  [watch] pane=0 tab=1 changed: refresh                      (top.txt; sizes kept)
  [size] pane=0 Alt+Return: 1 of 1 folder(s) to size
  [size] queued ...\root\c (1 in queue)
  [size] done ...\root\c: 300 bytes, 1 files, 0 folders in 2 ms (0 skipped)
  [auto] Sort { pane: 0, col: 1 }
  [select] pane=0 Only row=0 selected=1
  [size] stale ...\root\a: not selected any more, size dropped
  [select] pane=0 Toggle row=1 selected=2
  [size] queued ...\root\a (1 in queue)
  [size] done ...\root\a: 3024 bytes, 3 files, 1 folders in 2 ms (0 skipped)
  [select] pane=0 Toggle row=2 selected=3
  [auto] Key("F5")
  [size] cache cleared pane=0 (F5)
  [watch] pane=0 subtree off ...\root
  [size] queued ...\root\c (1 in queue)
  [size] queued ...\root\b (1 in queue)
  [size] queued ...\root\a (2 in queue)
  [watch] pane=0 subtree on ...\root
  [size] done ...\root\c: 300 bytes, 1 files, 0 folders in 1 ms (0 skipped)
  [size] done ...\root\a: 3024 bytes, 3 files, 1 folders in 1 ms (0 skipped)
  [size] done ...\root\b: 5120 bytes, 2 files, 0 folders in 1 ms (0 skipped)
  ```

  A screenshot (`tools/shot.ps1`) after the sort and the three clicks shows the folders sorted by
  their computed sizes, `c` 300 B, `a` 3.0 KB, `b` 5.0 KB, in the Size column, and the status bar
  "3 selected, 8.2 KB" (300 + 3,024 + 5,120 bytes). Hidden files counted: `b` is 5,120 bytes with
  its hidden file.
- **Cancellation, partial totals and `C:\Windows`** (TwinFM on `C:\`, rows 3 `home`, 16 `Users`,
  17 `Windows`): `sleep 1500; click 0 17; sleep 2000; click 0 16; sleep 1500; click 0 3; waitsize
  20000; click 0 17; waitsize 600000; click 0 3; sleep 300; click 0 17; sleep 7000; nav 0
  C:\Users\jtkli\projects; sleep 1500; key CtrlA; waitsize 600000; sleep 7000; quit`:

  ```
  [size] queued C:\Windows (1 in queue)
  [size] partial C:\Windows: 508155492 bytes, 2091 files, 26991 folders
  ...
  [size] cancelled C:\Windows (deselected)
  [size] queued C:\Users (1 in queue)
  [size] cancelled C:\Users (deselected)
  [size] queued C:\home (1 in queue)
  [size] done C:\home: 4475 bytes, 1 files, 4 folders in 3 ms (0 skipped)
  [watch] pane=0 subtree on C:\
  [size] queued C:\Windows (1 in queue)
  [size] done C:\Windows: 47062949257 bytes, 184374 files, 157383 folders in 15581 ms (50 skipped)
  [select] pane=0 Only row=3 selected=1
  [select] pane=0 Only row=17 selected=1                     (cached: nothing queued)
  [size] invalidated C:\Windows (modified Windows\Prefetch\CVTRES.EXE-BBD3ED93.pf)
  [size] queued C:\Windows (1 in queue)
  [size] cancelled C:\Windows (navigation)
  [size] cache cleared pane=0 (navigation)
  [select] pane=0 All row=-1 selected=27
  [size] queued C:\Users\jtkli\projects\twinfm (1 in queue)
  ... 20 folders queued, 20 done
  ```

  86 `partial` lines over the three walks of `C:\Windows` (at most four a second). While `C:\`
  carried the subtree watch, 47 `changed below the folder: no refresh` lines and no `[refresh]`
  line at all. A screenshot 1 s into the last 7 s sleep on `C:\` shows `Windows` already dropped by
  the Prefetch write: an empty cell and "1 selected, 0 B so far..." (that build; the back-off below
  keeps the old size and re-walks `C:\Windows` no sooner than 31 s later).
- **Cached re-selection** (TwinFM on `C:\Users\jtkli\projects`, row 11 `Homelab`, row 2 `applyctl`):
  `sleep 1500; click 0 11; waitsize 20000; click 0 2; waitsize 20000; click 0 11; sleep 4000; quit`:

  ```
  [size] queued C:\Users\jtkli\projects\Homelab (1 in queue)
  [size] done C:\Users\jtkli\projects\Homelab: 20294221 bytes, 1605 files, 479 folders in 130 ms (0 skipped)
  [size] queued C:\Users\jtkli\projects\applyctl (1 in queue)
  [size] done C:\Users\jtkli\projects\applyctl: 172588058 bytes, 10243 files, 1465 folders in 220 ms (0 skipped)
  [auto] Click { pane: 0, row: 11, modifier: None }
  [select] pane=0 Only row=11 selected=1
  ```

  No `queued` line after the third click; the screenshot taken as soon as its `Click` line appeared
  shows `Homelab` selected, 19.4 MB in its Size cell (and 164.6 MB in `applyctl`'s), and the status
  bar "1 selected, 19.4 MB": the status bar is composed in the click's own handler from the cache.
- **The caps** (TwinFM on `C:\Windows\WinSxS`, 26,869 entries): `sleep 2500; key CtrlA; waitsize
  120000; sleep 5000; click 0 0; sizeall 0; sleep 1500; nav 0 C:\; sleep 1000; quit`:

  ```
  [select] pane=0 All row=-1 selected=26869
  [size] capped: 100 of 26867 folders sized
  [select] pane=0 Only row=0 selected=1
  [size] pane=0 Alt+Return: 500 of 26767 folder(s) to size
  [size] capped: 500 of 26767 folders sized
  [size] cache cleared pane=0 (navigation)
  ```

  600 `queued`, 548 `done`, 53 `cancelled ... (navigation)`. The screenshot during the 5 s sleep shows
  the first rows' sizes (1.1 KB, 3.8 KB, 708 B, ...) and "26869 selected, 2.1 MB so far (26767 folders
  not sized)".
- **The subtree watch on `C:\Users\jtkli` while the worker walks it** (hidden files shown, so
  `AppData` too): `sleep 1500; key CtrlH; sleep 1000; sizeall 0; waitsize 900000; sleep 15000; quit`
  on the final build:

  ```
  [size] pane=0 Alt+Return: 52 of 52 folder(s) to size
  [watch] pane=0 subtree on C:\Users\jtkli
  [size] done C:\Users\jtkli\AppData: 115565267969 bytes, 492243 files, 95359 folders in 17472 ms (1 skipped)
  [size] invalidated C:\Users\jtkli\AppData (added AppData\Local\BraveSoftware\...\Network Persistent State...tmp)
  [size] stale C:\Users\jtkli\AppData: re-walk in 34.9 s
  [size] invalidated C:\Users\jtkli\projects (modified projects\.repowise\sessions)
  [size] stale C:\Users\jtkli\projects: re-walk in 3.7 s
  ```

  Counted over the whole run (final build): 53 `queued`, 53 `done` (the 52 folders and one re-walk
  of `projects`, which Alt+Return sized, 3.7 s after its change; `AppData`'s re-walk was due 34.9 s
  later, after the script had quit), 88 `partial`, 2 `invalidated` with 2 `stale ... re-walk in`;
  23 `[watch]` lines: 2 `watching`, 1 `subtree on`, 20 `changed below the folder: no refresh`, no
  `changed: refresh`, no overflow; **0 `[refresh]` lines**. Every change the profile saw during the
  walks (browser and Claude caches, `.repowise`) lay below a subfolder, so none re-listed
  `C:\Users\jtkli`. Earlier runs on 7aacc01 (23 `[watch]`, 0 `[refresh]`, 52 `done`) and on 1ed54d3
  (24 `[watch]`, 1 `[refresh]` from a top-level change, 3 `invalidated`) gave the same picture.

- **A burst below a sized, selected folder** (`%TEMP%\twinfm-p3m3-overflow\root\a`, four Python
  threads creating 32,000 one-byte files with long names in `a\sub`): `sleep 1000; click 0 0;
  waitsize 5000; sleep 20000; waitsize 60000; sleep 1000; quit`. One `subtree on`, no `subtree off`,
  66 `invalidated` and 66 `changed below the folder: no refresh` lines, each followed by a re-walk
  (`done ...\a: 884 bytes, 786 files` up to `32100 bytes, 32001 files`), one `[refresh]` (the lazy
  `modified a` after the first walk), no overflow. The earlier build (subtree only while cached)
  logged `subtree off` / `subtree on` around every re-walk (Deviations). This run predates the
  back-off; the back-off check below is its replacement.
- **The re-walk back-off** (final build; `%TEMP%\twinfm-p3m3-backoff\root`, folder `a` holding a
  1 MiB file): `sleep 1000; click 0 0; waitsize 5000; sleep 10000; waitsize 10000; sleep 500;
  quit`, and once `a` was sized a shell loop wrote a 100,000-byte file into `a` every 300 ms, 17
  times (5.1 s):

  ```
  [size] done ...\root\a: 1048576 bytes, 1 files, 0 folders in 3 ms (0 skipped)
  [size] invalidated ...\root\a (added a\w1.bin)
  [size] stale ...\root\a: re-walk in 2.0 s
  [size] queued ...\root\a (1 in queue)
  [size] done ...\root\a: 1748576 bytes, 8 files, 0 folders in 2 ms (0 skipped)
  [size] invalidated ...\root\a (added a\w7.bin)
  [size] stale ...\root\a: re-walk in 2.0 s
  [size] queued ...\root\a (1 in queue)
  [size] done ...\root\a: 2348576 bytes, 14 files, 0 folders in 2 ms (0 skipped)
  [size] invalidated ...\root\a (added a\w13.bin)
  [size] stale ...\root\a: re-walk in 2.0 s
  [size] queued ...\root\a (1 in queue)
  [size] done ...\root\a: 2748576 bytes, 18 files, 0 folders in 2 ms (0 skipped)
  ```

  3 `stale ... re-walk in` lines (one per 2 s window; the writes between them logged nothing) and 4
  `done` lines (the first walk and 3 re-walks; the bound is ceil(5 s / 2 s) + 1 = 4); the second run
  of the same script gave the same counts. The last size is the 1 MiB file plus 17 x 100,000 bytes.
  The writes into `a` itself also change `a`'s own entry, so the listing refreshed with each 250 ms
  batch (16 `[refresh]` lines), as a shallow watch would. Four screenshots taken one after another
  while the writes ran show, in the status bar and `a`'s Size cell: "1 selected, 1.0 MB
  (updating...)" with 1.0 MB in the cell, "1 selected, 1.6 MB" (just after a re-walk), "1 selected,
  1.6 MB (updating...)" with 1.6 MB, "1 selected, 2.2 MB (updating...)" with 2.2 MB: the cell never
  empties and the old size counts while the folder waits for its re-walk.

### Measurements

This machine, RDP, builds as in Scripted checks:

| What | Result |
|---|---|
| `C:\Windows` | 47,062,949,257 bytes (43.8 GiB, hard links counted per link), 184,374 files, 157,383 folders, 15.6 s, 50 folders skipped |
| `C:\Users\jtkli\projects`, Ctrl+A (20 folders, 27 items) | 10.78 GB, 168,837 files, 17,421 folders; 2.54 s of worker time summed over the 20 walks (largest `twinfm`, 9.2 GB in 454 ms) |
| Cached re-selection | no walk, no queue; the status bar is composed in the click handler from the cache |
| `C:\Users\jtkli\AppData` (in the Alt+Return of the profile) | 115,561,672,643 bytes (107.6 GiB), 492,192 files, 95,350 folders, 21.8 s, 1 folder skipped |
| Partial totals | 86 lines over three walks of `C:\Windows` (about 4 a second) |

### Gate

**Passed (2026-10-13 evening), one triplet.** Console WPR, Jeremy elevated, `tools/wpr-triplet.ps1
-Reference p3m2 -Candidate p3m3 -Tag ab12`, accessibility on, Narrator off, no other TwinFM
window, console checks run AFTER the triplet this time: `target-p3m2` (05c52a3) against
`target-p3m3` (1dd2820, the PR head, built from a detached checkout). Analysed with
`tools/analyze-trace.py --symbols --exe`.

| Run | P3-M2 (`target-p3m2`) | P3-M3 1dd2820 |
|---|---|---|
| 1 | 20.5% (2052 ms) | 20.6% (2063 ms) |
| 2 | 20.9% (2087 ms) | 21.7% (2170 ms) |
| 3 | 20.7% (2072 ms) | 20.5% (2048 ms) |
| **median** | **20.7%** | **20.6%** |

The medians are 0.1 apart in the candidate's favour and the candidate's run 3 sits below every
reference run: no gap by either half of the rule. The session was quiet (every run within 1.2
points), the first since the triplet protocol began with no disturbed run. Frame shares match
within a few samples in every pair (accessibility 219 / 209, 232 / 231, 218 / 229; render 1194 /
1205, 1212 / 1249, 1198 / 1164; draw_text 696 / 675, 699 / 702, 699 / 665; rows 370 / 370, 373 /
391, 378 / 366; timers 703 / 696, 712 / 739, 716 / 700; heap 218 / 212, 231 / 214, 228 / 184),
and no size, subtree or walk frame of ours appears in any candidate window, as expected from the
0 `[size]` lines the gate script logs. Jeremy's console checks (folder selection, Ctrl+A, Alt+Return
in System32, a change inside a sized folder) passed, "very fast". Traces
`C:/traces/twinfm-ab12-{p3m2,p3m3}-{1,2,3}.etl`, reports under `C:/traces/export/`.

## P3-M4: archives

The backlog was two items: the Phase 2 close's archives candidate, "zip folders as navigable,
which also needs the virtual-file drop path" (`phase-2-plan.md` § Phase 2 close), and P2-M4's
deferral of virtual files, "`CFSTR_FILEDESCRIPTOR` / `CFSTR_FILECONTENTS`: a zip folder's items, an
Outlook attachment ... still only `CF_HDROP`" (`phase-2-plan.md` § P2-M4 Deferred). TwinFM now opens
a `.zip` as a folder (read with a Rust reader, cached per archive), copies out of it through the
shell's own extraction (the same `IFileOperation` path as every copy, so the progress dialog,
collisions and undo work), drags out of it with the shell zip folder's data object, and accepts
virtual files dropped from anywhere (Explorer's zip view, a mail attachment, its own zip drags) by
writing them itself. Nothing is written into an archive yet. The gate scenario never meets an
archive: 0 `[archive]` lines.

### What landed (2026-10-14)

- **Archive paths** (`src/fs/archive.rs`, new). `split(path)` says whether a path lies inside a
  zip: walking its components, the first one named `*.zip` (any case) that is a file on disk
  (`GetFileAttributesW`: exists and is not a directory) is the archive, and the rest is the path
  below its root (`C:\dir\x.zip\sub` gives `C:\dir\x.zip` and `sub`; `C:\dir\x.zip` gives the root,
  `""`). A folder named `x.zip` is a folder, `C:\a.zipx` is not an archive, and a path with no
  `.zip` in it costs one lowercase scan and no file system call. `inside(path)` is `split` as a
  yes or no; `is_zip_name(name)` is the name test a row uses to open as a folder.
- **The reader and the cache.** The `zip` crate (9.0.3, MIT, `default-features = false`: no
  decompression code, the shell extracts) reads the central directory once (`ZipArchive::new` and
  `by_index_data`, which touch no local headers) into levels: a map from each folder's lowercase
  path below the root to its entries (name, folder or file, uncompressed size, modified time). A
  folder implied only by its files' paths gets a level and an entry with no date; a stored folder
  entry gives its folder its own date. Names are split on `/` and `\`, empty, `.` and `..` parts
  dropped, decoded by the crate (UTF-8 when the flag says so, CP437 otherwise). The MS-DOS time is
  local time: `LocalZone::from_local` (new, in `model.rs`) turns it into Unix seconds with the
  zone's daylight rules. The levels are cached by archive path, size and modified time, the 8 most
  recently used (`CACHE_MAX`; a `Mutex<Vec<_>>`, least recent first): a changed archive is read
  again, a ninth evicts the oldest (`[archive] cache evicted <zip>`). A damaged central directory is
  an error ("not a readable zip archive: ..."), never a panic. Only workers call it.
- **Listing** (`fs/enumerate.rs`). `run` checks `split` first; an archive level is listed from the
  cache (`archive::list`) and streamed as `Msg::Batch`es of 2,000 and one `Msg::Done`, exactly as a
  folder is, with no free space and `Msg::Done::archive` carrying the central directory's entry
  count, its read time and whether it came from the cache, for the `[enum]` line. Entries are
  `Kind::Directory` or `Kind::File` with no attributes (no hidden flags, no reparse kinds), the Type
  from the extension through `TypeNames`, icon keys `dir` and `ext:.<ext>` only (an `.exe` or `.lnk`
  inside a zip has no file to take its own icon from; the icon worker renders the first of each
  through the zip folder's shell item). The search walker (`walk`) inside an archive is a pass over
  the cached levels (`archive::walk`: every entry below the level whose own name matches, named
  `sub\file.txt`); from a folder it still lists only folders, so an archive is a file to it. The
  breadcrumb dropdown of a zip crumb lists the archive's folders (`pathbar::subfolders`).
- **Navigation** (`main.rs`). Return or a double-click on a `*.zip` file row (not inside an
  archive) navigates into it, as does middle-click into a new tab; a folder row inside an archive
  navigates as any folder; Backspace and Alt+Up go up through the archive's folders to its root and
  out to the folder holding it (by path); crumbs, the path entry (`navigate`), the history and
  bookmarks take archive paths like any other. The window and tab titles are the level's name.
- **What an archive tab is not.** No directory watch: the watcher refuses archive paths
  (`[watch] pane=N no watch for <path>: inside an archive`, once per pane and path, from
  `unwatchable`). No folder sizes: `Tab::sizable` is false there, a selection with folders logs
  `[archive] refused: folder sizes for N selected folder(s) in <path> (inside an archive)` and
  Alt+Return leaves the note "folders inside an archive are not sized". No free-space figure. The
  filter and the recursive search work on its listing like any other.
- **Copying out is extraction.** `fileops::shell_item` already made sources with
  `SHCreateItemFromParsingName`, which the zip folder parses for paths inside an archive (it now
  logs `[archive] parse "<path>" -> item ok|failed 0x...` for those), so Shift+F5 / F6 to the other
  pane, Ctrl+C then Ctrl+V into a folder, and the context menu's verbs reach the shell's own
  extraction with its progress dialog and collision prompts. The progress sink's `item_path` falls
  back to `SIGDN_DESKTOPABSOLUTEPARSING` when an item has no file system path, so an extraction's
  results name their in-archive sources and the undo record is built: Ctrl+Z after an extraction
  recycles the extracted copies (`Delete { permanent: false }` of the destination items).
- **Open.** Return on a file inside an archive goes to `shellmenu::open_in_archive`: the item's PIDL
  (`SHParseDisplayName`) to `ShellExecuteExW` with `SEE_MASK_IDLIST`, so the zip folder extracts it
  to a temporary folder and opens it there, as Explorer does. Files elsewhere keep the path route.
- **Refusals** (nothing is written into an archive in this milestone): paste into an archive tab,
  a new folder in one, copy or move into one from the other pane, and drops into one (its list, its
  tab in the strip, or a sidebar bookmark of an archive path: DragEnter effect none). Each says why:
  `[archive] refused: <what> (writing into a zip archive is not supported)` and the same in the
  status bar (drops log once per hover). Editing inside an archive is refused the same way, before
  any shell call (the lead's decision after the first report, since the shell's zip folder hangs on
  these through `IFileOperation`, Decision 2): rename (F2, and the shell menu's "Rename", which
  comes to TwinFM's editor), delete to the Recycle Bin, permanent delete, move out (Shift+F6), cut
  (Ctrl+X, whose paste would be a move out) and new folder, each with `[archive] refused: <what>
  (editing a zip archive is not supported)`. Copying out and opening stay. The shell menu's own
  "Delete" and "Cut" verbs inside an archive go to the shell and are not intercepted.
- **Context menus.** `parse_items` with the archive folder as the parent works as is
  (`SHParseDisplayName` of `C:\dir\x.zip\sub`, the zip folder's `ParseDisplayName` for the child
  names), so a zip entry gets the shell's item menu (Open, Explore, Cut, Copy, Delete, Properties)
  under TwinFM's own items, and empty space the folder menu ("Extract All..."). The zip folder's
  menus crash inside their own `QueryInterface` for `IContextMenu2` and `IContextMenu3` (an access
  violation in the first scripted run, reproduced in a throwaway test on both item and background
  menus), so inside an archive `shellmenu::show` skips the IContextMenu2/3 routing (`[menu] inside
  an archive: no IContextMenu2/3 routing`); those menus have no owner-drawn submenus to route.
- **Drags out.** `dnd::drag` from a folder inside an archive asks the zip folder for its own data
  object (`shellmenu::ui_data_object`: `GetUIObjectOf(IID_IDataObject)` with the children's PIDLs)
  instead of `SHCreateDataObject`; it offers `FileGroupDescriptorW` and `FileContents` (HGLOBAL,
  IStream, IStorage) and no `CF_HDROP`, and folders expand into descriptors for their contents.
  Before the drag TwinFM calls its `SetAsyncMode(TRUE)` (`[dnd] the zip folder's data object:
  SetAsyncMode(TRUE) = 0x00000000`); the drag image is composed as for any drag. While the drag
  runs, `dnd` keeps the in-archive paths it was made from (`OWN_ARCHIVE_SOURCES`); a drop of it on
  our own other pane (`target_offer`: `Offer::Archive`) becomes the extraction job, a copy queued
  through `invoke_from_event_loop` exactly as a files drop is (the shell's progress dialog, the
  undo record, the UI thread free), and the virtual-file reader is not used (the lead's decision
  after the first report: that reader held the UI thread for 7 to 10 s per 1,000 files).
- **Virtual-file drops** (`dnd.rs`). The drop target reads what a data object offers once per
  `DragEnter` and `Drop` (`offer`): `Offer::Files` (`CF_HDROP`, which wins when both are there),
  `Offer::Virtual` (the parsed `FileGroupDescriptorW`) or nothing. A virtual drop is a copy whatever
  the source allows (`virtual_effect`; a right-button drop's menu greys "Move here"), into the same
  spots as any drop and refused where they are (search tabs, archives, This PC). On Drop,
  `virtual_drop` asks the source for `IDataObjectAsyncCapability`: when `GetAsyncMode` says yes and
  `StartOperation` succeeds, the data object is marshalled (`CoMarshalInterThreadInterfaceInStream`)
  to a `virtual-drop` worker (MTA) that writes the files, posts "Extracting K of N..." to the pane's
  status bar through `invoke_from_event_loop` after each file, and ends with `EndOperation(S_OK,
  DROPEFFECT_COPY)` (or `E_FAIL`, none); otherwise the files are written inside Drop (the source's
  requirement), refused before anything is written when the descriptors' sizes add up to more than
  256 MB (`SYNC_CAP`). `extract` writes the items in descriptor order: folders made as needed (a
  folder the descriptors only imply goes below the nearest one they list), a name already taken
  gets " (2)", " (3)"... before a file's extension (a folder so renamed takes its contents with
  it), each file's contents read from `FileContents` with its `lindex` as an IStream (64 KB reads)
  or an HGLOBAL (cut to the descriptor's size). Names that are not file names once `.` and `..`
  parts are dropped (a drive, a stream's `:`) are left out with a log line. The note ends "Extracted
  N files" or "Extracting failed: <why>"; the refresh is the watch's.
- **Drags from Explorer's zip view (fix after the console check).** The drop target refused one at
  DragEnter (the deny cursor). In process, the shell's generic data object over a zip folder's
  items (`SHCreateDataObject`) offers only `Shell IDList Array` (`formats [Shell IDList Array
  tymed=0x1 lindex=-1 aspect=1]`, `QueryGetData` S_FALSE for both descriptors, `FileContents` and
  `CF_HDROP`), which `offer` turned into nothing; the zip folder's own object (`GetUIObjectOf`, and
  `SHCreateShellItemArray` + `BindToHandler(BHID_DataObject)`) offers `Shell IDList Array`,
  `FileGroupDescriptorW` and `FileContents` and was accepted. So `offer` now falls back, after
  `CF_HDROP` and the descriptors, to the `Shell IDList Array`: when every item's parsing path
  (`ILCombine` with the folder's PIDL, `SHGetNameFromIDList(SIGDN_DESKTOPABSOLUTEPARSING)`) lies
  inside a zip archive, the drop is `Offer::Archive` with those paths, the extraction job as for
  our own drags. The descriptors are also read from the ANSI `FileGroupDescriptor` when
  `FileGroupDescriptorW` is missing (332-byte records, names through `MultiByteToWideChar(CP_ACP)`).
  A DragEnter that ends with effect none for a data object without files now logs one `[dnd]
  DragEnter refused: allowed=0x.. keys=0x..; formats [...]; QueryGetData ...` line (every format
  `EnumFormatEtc` lists with its tymed, lindex and aspect, and `QueryGetData` for
  `FileGroupDescriptorW`, `FileGroupDescriptor`, `FileContents` at lindex 0, `CF_HDROP` and `Shell
  IDList Array`), only when the object offers nothing TwinFM takes (not when the spot under the
  pointer refuses an acceptable one). Explorer's zip view did not need the fallback (confirmed at
  the console, see Gate: it offers the W descriptors and takes the async virtual path); the
  fallback covers `SHCreateDataObject`-style sources. Driver: `dropvirtual <pane>
  <row> idlist <path|...>` drops the `SHCreateDataObject` object.
- **The minimal data object** (`dnd.rs`, the driver's and the tests') now serves `CF_HDROP`,
  virtual files (descriptors built by `descriptor_block`, contents as HGLOBALs or as IStreams from
  `SHCreateMemStream`), or both, and with `can_async` an `IDataObjectAsyncCapability` face (a
  second vtable pointer in the same object) that asks for asynchronous extraction and records
  `EndOperation`.
- **Driver** (`auto.rs`): `dropvirtual <pane> <row> [async] <name=size|...>` (generated contents,
  `name\` a folder), `dropvirtual <pane> <row> zip <path|...>` (the zip folder's own data object for
  paths in one folder of an archive, what a drag out of a zip pane carries; `ownzip` in place of
  `zip` marks it as our own drag, with those paths as its sources); `waitops` also waits
  for an async virtual drop (`dnd::virtual_drops_running`). `nav` takes archive paths as it is.
- **Logging.** `[enum] pane=N done: N entries (archive level, central directory C entries, read in
  R ms, cached|fresh), worker ...`; `[archive] open <zip>: C entries, central directory R ms`,
  `[archive] cache evicted <zip>`, `[archive] refused: <what> (<why>)`, `[archive] parse "<path>" ->
  item ok|failed 0x...`; `[dnd] DragEnter: N virtual item(s) (FileGroupDescriptorW, no CF_HDROP)`,
  `[dnd] virtual drop: N file(s), M bytes, sync|async`, `[dnd] virtual drop wrote <path> (K of N)`,
  `[dnd] virtual drop done in T ms: ...`, `[dnd] virtual drop refused: <why>`, `[dnd] virtual drop:
  GetAsyncMode = ..., StartOperation = ...`, `[dnd] virtual drop: EndOperation(...)`; `[watch] ...
  no watch for <path>: inside an archive`; `[menu] inside an archive: no IContextMenu2/3 routing`.
- **Tests**, 111 to 118: `central_directory_levels` (zips the test writes in
  `%TEMP%\twinfm-p3m4-test-<pid>\`: nested folders with a stored folder entry and implied ones, a
  file at depth 3, a level asked for in another case, the MS-DOS time shown as written, `dir` and
  `ext:.txt` keys, a missing level and a file as a level are errors, the second listing cached, the
  crumb dropdown's folders, the archive walk's 3 matches in 5 folders; a zip with a top-level folder
  only; an empty zip; non-ASCII names with the UTF-8 flag; a zip64 entry; a damaged end of central
  directory and a file that is no zip at all as errors; a rewritten archive read fresh; eviction
  after 8 newer archives); `archive_paths_are_decided_by_the_file_system` (inside, the root, any
  case, `a.zipx`, a folder named `folder.zip`, a missing zip); the desktop test
  `shell_items_inside_a_zip_extract` (the zip folder parses `t.zip\sub\file.txt`, which has no file
  system path and a parsing path equal to it, `item_path` gives it; an `IFileOperation` copy of a
  file and a folder out of the zip extracts both, one result per source, and the undo record
  recycles the two copies); `virtual_files_round_trip` (HGLOBAL and IStream: descriptors with their
  indexes, `..` dropped, a drive-qualified name left out, a nested path, a 70,000-byte file and an
  empty one; the same drop twice gives `top (2).txt` and `dir (2)\nested\deep.bin`; a 100-byte cap
  stops it; `over_cap` at and past 256 MB; a cut block); `hdrop_wins_over_virtual_files` (also:
  during our own archive drag virtual files become its in-archive paths, files stay files, and
  without it they stay virtual);
  `zip_folder_data_object_offers_virtual_files` (the zip folder's data object has no `CF_HDROP`,
  offers `a.txt`, `inner` and `inner\b.txt`, has `IDataObjectAsyncCapability`, and extracts both
  files through `extract`); `async_virtual_drop_on_a_worker` (a source asking for async: Drop starts
  the operation, the worker reads through the marshalled object while the test's STA pumps, 20 files
  land and `EndOperation` reports `S_OK` and a copy).
- **Docs**: `CLAUDE.md` (test count, the P3-M4 driver steps and log lines, `C:\traces\bigdir.zip` as
  a fixture, the new scratch folders), the `src/auto.rs` header.

### Decisions

- **Decision 1, listing: the Rust reader (b).** Measured on a worker before anything else was built
  (Measurements, first table): the shell's zip folder (a) took 4.4 s cold and 0.55 s warm for the
  40,000-entry `bigdir.zip`, 0.3 s cold for the 2,000-entry archive's top level and 0.27 s cold for a
  subfolder; the `zip` crate took 0.10 s, 0.026 s, 0.004 s and 0.008 s. (b) is faster on every row,
  by 7x to 45x, and needs no COM at all; (a) is not within 2x anywhere. The measuring code (an STA
  worker running `SHParseDisplayName`, `SHBindToObject(IShellFolder2)`, `EnumObjects`,
  `GetDisplayNameOf` and two `GetDetailsEx` per entry, beside `archive::list`) was a throwaway test,
  not committed.
- **Decision 2, operations: the shell.** Extraction is the existing copy job with in-archive sources
  (above); the sink's parsing-path fallback is what makes its undo record. What the zip folder does
  not do through `IFileOperation` shows the shell's own error: a permanent delete inside a zip
  stopped in the shell's "1 Interrupted Action" window with "An unexpected error is keeping you from
  deleting the file ... Error 0x80004005: Unspecified error" (read through UI Automation in a
  throwaway test with no owner window); a rename, a delete to the Recycle Bin, a new folder and a
  move out of a zip did not finish within 15 s in the same test either (not inspected; the move had
  extracted its file first). So TwinFM refuses them up front (the lead's decision, revising the
  brief's "the shell's own error"): rename, delete, permanent delete, move out, cut and new folder
  inside an archive, before any shell call.
- **Decision 3, drags: the zip folder's data object; our own drag lands as the extraction job.** The
  zip folder's object offers no `CF_HDROP`. It reports `IDataObjectAsyncCapability` but answers
  `GetAsyncMode` with FALSE even after our `SetAsyncMode(TRUE)` returned S_OK, so a target reads it
  synchronously inside Drop. Foreign sources (Explorer's zip view, mail programs, the driver's
  `dropvirtual`) take the virtual-file path; our own drag out of a zip onto our other pane takes the
  extraction job from the in-archive paths it was made from (the lead's decision). The async path
  is exercised by the minimal data object (test and `dropvirtual ... async`).
- **Collisions in a virtual drop get " (2)"** (`free_name`), files and folders alike, with no shell
  dialog; a renamed folder carries the items below it. Explorer would ask.
- **A virtual drop is always a copy**, and is reported to the source as one (`Performed DropEffect`
  and the return value `DROPEFFECT_COPY`, or `EndOperation`'s effect).
- **Icon keys inside an archive are by extension only**, `.exe`, `.lnk` and `.ico` included: their
  own icons would need the file.
- **The path test asks the file system only for `*.zip` components**, on whichever thread needs it
  (the enumeration and search workers, the watcher, and on the UI thread per navigation, operation
  and drop spot, never per row or frame).
- **Shell context menus inside archives run without IContextMenu2/3 routing** rather than TwinFM's
  own items only: everything but the routing works.

### Deviations

- **Search inside an archive walks its cached levels.** The brief said the walker does not descend
  into zips (kept: from a folder, an archive is a file); a search started in an archive tab would
  otherwise fail to list its root, so `walk` answers it from the cache.
- **`SetAsyncMode(TRUE)` on the zip folder's data object before our drags**: not in the brief; it
  costs nothing and lets a target that honours it extract asynchronously. The zip folder itself
  still reports synchronous (Decision 3).
- **An async-capable face on the minimal data object** and the `async` form of `dropvirtual`, since
  no real source here reads asynchronously; the brief allowed for none existing.
- **A `zip` form of `dropvirtual`** (the zip folder's own data object, dropped on our target
  directly): the only scripted way to drop what our own zip drags carry, since a scripted drag can
  only be cancelled.
- **No IContextMenu2/3 routing inside archives** (the crash, above): a change to `shellmenu::show`.
- **Extra lines**: `[archive] refused: folder sizes ...`, `[menu] inside an archive: ...`, the
  `GetAsyncMode` / `EndOperation` / `SetAsyncMode` lines, `virtual drop done in T ms`.
- **New `windows-sys` features**, `Win32_System_Com_Marshal` (for
  `CoMarshalInterThreadInterfaceInStream`) and, with the console fix, `Win32_Globalization` (for
  `MultiByteToWideChar`, the ANSI descriptors).
- **After the console check**: a `Shell IDList Array` of items inside an archive is accepted as an
  extraction job, the ANSI descriptor is read, refused drags log their formats, and `dropvirtual`
  gained the `idlist` form.

### Deferred

- **Other formats** (7z, rar, tar, gz), **writing into archives** (paste, drop, new folder),
  **editing inside archives** (rename, delete, move out, cut: refused; the shell menu's own Delete
  and Cut verbs are not intercepted), **password-protected zips** (not tried; extraction is the
  shell's), **nested archives** (a zip inside a zip is a file there: Return goes to the shell's
  open; not tried).
- **A foreign zip source still blocks the UI thread inside Drop** (Explorer's zip view dropped on
  TwinFM reads synchronously: about 7 to 10 s per 1,000 small files, Measurements); only our own
  drags take the extraction job.
- **The clipboard from an archive**: Ctrl+C writes `CF_HDROP` with in-archive paths, which TwinFM's
  own paste extracts (checked); a paste of them into Explorer, and TwinFM's paste after the zip
  menu's own Copy (which puts the zip folder's data object on the clipboard, while TwinFM's paste
  reads `CF_HDROP` only), were not tried. A virtual-file paste would be the follow-up.
- **`IStorage` contents** (an Outlook message dragged as a file) are not read: "unsupported medium".
- **An archive tab does not follow changes to the archive** (no watch); F5 re-reads it when its size
  or time changed.
- **Not checked by a person**: a drop from Explorer's zip view and from Outlook, a real pane-to-pane
  drag out of a zip, the shell's extraction progress and collision dialogs, Narrator in an archive
  tab.

### Scripted checks

RDP session, this worktree's `target\release\twinfm.exe` built from 2ccc65d plus the comment-only
`auto.rs` header change (the bookmark and `deep` search checks, and the screenshot, on earlier
builds of the same listing and navigation code), every run with `TWINFM_SCRIPT` (inline or
`@file` in the session scratchpad) and without `TWINFM_SOAK`; no run read or wrote
`%APPDATA%\twinfm\config.toml`. Scratch tree `%TEMP%\twinfm-p3m4-check\` (`nested.zip`, a copy of the 2,000-entry archive: 50 folders `d01`
to `d50` of 40 files, 1 to 4 KB each, 5,375,176 bytes; `small.zip` with `inner\deep.txt`,
`note.txt` and `run-me.cmd`; a folder named `folder.zip`; `dest`), removed afterwards. All exit
code 0, no `panicked` line. One earlier run, before the routing change, ended in an access violation
inside the zip folder's background menu (`[menu] background menu for the folder in ...nested.zip:
... | Extract All...` was its last line); that run is what found the crash.

- **The gate scenario meets no archive code.** The gate script on `C:\traces\bigdir`: `archive
  lines 0, virtual 0, panicked 0`; the four `[sort]` lines as before (`40000 of 40000 entries`).
- **Navigation, crumbs, history, cache** (TwinFM on the scratch tree): `nav 0 C:\traces\bigdir.zip;
  crumb 0 2; history back; nav 0 ...\nested.zip\d10; crumbmenu 0 8; crumbpick d20; key Backspace; key
  Backspace; history back; history forward; nav 0 ...\folder.zip; nav 0 ...\small.zip\inner`:

  ```
  [watch] pane=0 no watch for C:\traces\bigdir.zip: inside an archive
  [archive] open C:\traces\bigdir.zip: 40000 entries, central directory 86.6 ms
  [enum] pane=0 done: 40000 entries (archive level, central directory 40000 entries, read in 86.6 ms, fresh), worker 133.7 ms, request-to-done 144.2 ms
  [crumb] pane=0 index 2: C:\traces
  [nav] pane=0 history back: C:\traces -> C:\traces\bigdir.zip (1 back, 1 forward left)
  [enum] pane=0 done: 40000 entries (archive level, central directory 40000 entries, read in 86.6 ms, cached), worker 38.4 ms, request-to-done 57.0 ms
  [archive] open ...\nested.zip: 2000 entries, central directory 6.0 ms
  [enum] pane=0 done: 40 entries (archive level, central directory 2000 entries, read in 6.0 ms, fresh), worker 8.4 ms, request-to-done 9.5 ms
  [crumb] pane=0 menu: 50 folder(s) in ...\nested.zip, listed in 0.6 ms
  [crumb] pane=0 picked ...\nested.zip\d20
  [nav] pane=0 gen=7 path=...\nested.zip                      (Backspace)
  [nav] pane=0 gen=8 path=...\twinfm-p3m4-check               (Backspace: out of the archive)
  [nav] pane=0 history back: ...\twinfm-p3m4-check -> ...\nested.zip (4 back, 1 forward left)
  [nav] pane=0 history forward: ...\nested.zip -> ...\twinfm-p3m4-check (5 back, 0 forward left)
  [watch] pane=0 tab=1 watching ...\folder.zip                (a folder named .zip is a folder)
  [enum] pane=0 done: 1 entries (archive level, central directory 3 entries, read in 0.2 ms, fresh), ...   (small.zip\inner)
  ```

  A screenshot (`tools/shot.ps1`) of `nested.zip\d25` shows the crumbs `This PC > C: > ... >
  twinfm-p3m4-nested > nested.zip > d25`, the title `TwinFM - d25`, 40 rows `f0001.bin`... with
  Type "BIN File", sizes from 1.1 KB to 3.9 KB, Modified `2026-10-14 08:12` (when the files were
  written), file icons, and "40 items" with no free-space figure.
- **Open, Return on a zip, clipboard extraction**: `click 0 4; key Return` (small.zip) `; click 0 2;
  key Return` (run-me.cmd, which writes a marker file) `; click 0 1; cc; nav 0 ...\dest; click 0 0;
  cv; waitops 20000`:

  ```
  [nav] pane=0 gen=2 path=...\small.zip
  [archive] parse "...\small.zip\run-me.cmd" -> item ok
  [open] pane=0 ...\small.zip\run-me.cmd: Ok(())
  [fileops] pane=0 note: Copied 1 item to the clipboard
  [fileops] submit id=1 pane=0 hwnd=0xff083e Copy { sources: ["...\\small.zip\\note.txt"], dest_dir: "...\\dest" }
  [archive] parse "...\small.zip\note.txt" -> item ok
  [undo] recorded "Copied 1 item": Some(Delete { sources: ["...\\dest\\note.txt"], permanent: false })
  [fileops] done id=1 pane=0 tab=1 Done: Copied 1 item
  [fileops]   item 0x00000000 ...\small.zip\note.txt -> ...\dest\note.txt
  ```

  The marker file read "opened from the zip"; `dest` then held `keep.txt` and `note.txt`. The run
  overwrote the clipboard.
- **Refusals, sizes, filter, search, menus, drag** (`split; nav 1 ...\dest; nav 0 ...\nested.zip;
  click 0 0; sizeall 0; key F7; key CtrlV; click 1 0; copy; dropfiles 0 -1 ...; dropfiles 0 3 ...;
  dropvirtual 0 -1 into-zip.txt=5; filter 0 d1; filter 0 ; filter 0 f0007; search 0; waitsearch
  5000; tab close; filter 0 ; rclick 0 2; menuclose; rclickempty 0; menuclose; nav 0 ...\d10; click 0
  0; shiftclick 0 2; rclick 0 1; menuclose; dragrow 0 1 0 40; dragcancel`):

  ```
  [archive] refused: folder sizes for 1 selected folder(s) in ...\nested.zip (inside an archive)
  [archive] refused: folder sizes (inside an archive)
  [fileops] pane=0 note: folders inside an archive are not sized
  [archive] refused: new folder (writing into a zip archive is not supported)
  [archive] refused: paste (writing into a zip archive is not supported)
  [archive] refused: copy into ...\nested.zip (writing into a zip archive is not supported)
  [archive] refused: drop into ...\nested.zip (writing into a zip archive is not supported)
  [auto] dropfiles Row(0, -1) copy: 1 path(s), DragEnter effect none, Drop reported none
  [auto] dropfiles Row(0, 3) copy: 1 path(s), DragEnter effect none, Drop reported none
  [dnd] DragEnter: 1 virtual item(s) (FileGroupDescriptorW, no CF_HDROP), first into-zip.txt
  [auto] dropvirtual pane 0 row -1: 1 item(s), DragEnter effect none, Drop reported none
  [filter] pane=0 "d1" (substring): 10 of 50 shown, pass 0.3 ms
  [search] pane=0 gen=6 start "f0007" in ...\nested.zip
  [search] pane=0 gen=6 done: 50 found, 51 folders walked, 0 skipped, in 5 ms
  [menu] item menu for 1 item(s) in ...\nested.zip: Open in other pane | Open in terminal | Copy path | --- | Open | Explore | --- | Cut | Copy | --- | --- | Delete | --- | Properties
  [menu] inside an archive: no IContextMenu2/3 routing
  [menu] closed after 336 ms (UI thread cpu 16 ms in the loop): id 0 = Nothing
  [menu] background menu for the folder in ...\nested.zip: Open in other pane | Open in terminal | Copy path | --- | --- | Extract All...
  [menu] item menu for 3 item(s) in ...\nested.zip\d10: Open in terminal | Copy path | --- | Open | --- | Cut | Copy | --- | --- | Delete | --- | Properties
  [dnd] pane=0 row=1: dragging 3 item(s)
  [dnd] the zip folder's data object: SetAsyncMode(TRUE) = 0x00000000
  [dnd] drag image 96x96 px, 3 item(s), icon from the shell, composed in 32.6 ms: InitializeFromBitmap = 0x00000000
  [dnd] DoDragDrop starts (left button): 3 item(s) from ...\nested.zip\d10
  [dnd] DoDragDrop returned 0x00040101 (cancelled) after 478 ms: effect none, logical performed effect none
  ```

  A bookmark of `nested.zip\d07` (`bookmark add`) opened from the sidebar (`[sidebar] row 12 click:
  ...\nested.zip\d07`, listed from the cache) and refused a drop (`dropfiles side -1 ...`: `[archive]
  refused: drop into ...\nested.zip\d07`, DragEnter effect none). A search for `deep` from the
  scratch folder found only `src\inner\deep.txt` (`1 found, 5 folders walked`: the two zips were not
  entered, `folder.zip` was).
- **Extraction of 1,000 files and its undo** (`nav 1 ...\dest\x; nav 0 ...\nested.zip; click 0 0;
  shiftclick 0 24; copy; waitops 120000; click 1 0; undo; waitops 120000`; stderr timestamped by a
  reader process):

  ```
  904.220 [fileops] submit id=1 pane=0 hwnd=0xa20acc Copy { sources: ["...\\nested.zip\\d01", ... 25 folders
  917.467 [undo] recorded "Copied 25 items": Some(Delete { sources: ["...\\dest\\x\\d01", ... permanent: false })
  917.468 [fileops] done id=1 pane=0 tab=1 Done: Copied 25 items
  918.906 [fileops] submit id=2 pane=1 hwnd=0xa20acc Delete { sources: ["...\\dest\\x\\d01", ...
  932.751 [undo] done id=2 Done: Undid: Copied 25 items
  ```

  25 `[archive] parse ... -> item ok` lines, 25 `item` lines per job; after the undo `dest\x` held
  only its `keep.txt`. Each undo run (three in all) put its 25 extracted folders (1,000 small
  files) in the Recycle Bin.
- **Editing refused, and our own zip drag as the extraction job** (after the lead's two fixes, the
  final build; a fresh `nested.zip` in the scratch tree): `split; nav 1 ...\dest; nav 0
  ...\nested.zip; click 0 0; key F2; key Delete; key ShiftDelete; key ShiftF6; key F7; key CtrlX;
  dropvirtual 1 -1 ownzip ...\nested.zip\d26|...|...\nested.zip\d50; waitops 120000` (stderr
  timestamped by a reader process):

  ```
  673.904 [archive] refused: rename (editing a zip archive is not supported)
  674.106 [archive] refused: delete (editing a zip archive is not supported)
  674.308 [archive] refused: permanent delete (editing a zip archive is not supported)
  674.510 [archive] refused: move out of an archive (editing a zip archive is not supported)
  674.712 [archive] refused: new folder (editing a zip archive is not supported)
  674.915 [archive] refused: cut (editing a zip archive is not supported)
  675.339 [dnd] DragEnter: our own drag of 25 item(s) out of an archive, first ...\nested.zip\d26
  675.340 [dnd] Drop at pane 1 (...\dest) keys=0x0 allowed=0x3: 25 item(s) of our own archive drag, effect copy (reported to the source: copy)
  675.341 [dnd] drop into pane 1: copy 25 item(s) to ...\dest
  675.345 [fileops] submit id=1 pane=1 hwnd=0x4250852 Copy { sources: ["...\\nested.zip\\d26", ...
  678.993 [perf] frames_total=68 render[n=39 p50=1.79ms ...] ui_cpu=391ms over 5s = 10.02ms/frame
  680.994 [busy] 10 s after split: UI thread 5.9% busy over 10.0 s (cpu 0.594 s)
  683.993 [perf] frames_total=102 render[n=34 p50=2.69ms ...] ui_cpu=188ms over 5s = 5.51ms/frame
  686.242 [undo] recorded "Copied 25 items": Some(Delete { sources: ["...\\dest\\d26", ...
  686.242 [fileops] done id=1 pane=1 tab=2 Done: Copied 25 items
  ```

  One `[fileops] submit` in the whole run (the drop's), no `virtual drop` line; the UI thread kept
  rendering (34 to 39 frames per 5 s) and was 5.9% busy while the job ran 10.9 s; `dest` then held
  1,000 extracted files and `keep.txt`. The gate script on this build: `gate: archive lines 0,
  virtual 0, panicked 0`, the four `[sort]` lines as before.
- **Virtual drops** (`nav 1 ...\dest\v100; dropvirtual 1 -1 sub\|sub\inner.txt=10|v001.bin=524288|...
  v100.bin=524288; waitops; nav 1 ...\dest\vasync; dropvirtual 1 -1 async v001.bin=524288|...; waitops;
  nav 1 ...\dest\vzip; dropvirtual 1 -1 zip ...\nested.zip\d26|...|...\nested.zip\d50; waitops;
  dropvirtual 1 -1 huge.bin=270000000`):

  ```
  [dnd] virtual drop: 101 file(s), 52428810 bytes, sync
  [dnd] virtual drop done in 260 ms: Ok((101, 52428810))
  [dnd] virtual drop: GetAsyncMode = 0x00000000 (1), StartOperation = 0x00000000
  [dnd] virtual drop: 100 file(s), 52428800 bytes, async
  [auto] dropvirtual pane 1 row -1 (async source): 100 item(s), DragEnter effect copy, Drop reported copy
  [dnd] virtual drop done in 1104 ms: Ok((100, 52428800))
  [dnd] virtual drop: EndOperation(0x00000000, copy) = 0x00000000
  [dnd] the zip folder's data object: SetAsyncMode(TRUE) = 0x00000000
  [dnd] DragEnter: 1025 virtual item(s) (FileGroupDescriptorW, no CF_HDROP), first d26
  [dnd] virtual drop: GetAsyncMode = 0x00000000 (0), StartOperation = 0xFFFFFFFF
  [dnd] virtual drop: 1000 file(s), 2575335 bytes, sync
  [dnd] virtual drop done in 9779 ms: Ok((1000, 2575335))
  [dnd] virtual drop: 1 file(s), 270000000 bytes, sync
  [dnd] virtual drop refused: 270000000 bytes is over the 268435456-byte limit for a synchronous source
  ```

  1,201 `virtual drop wrote` lines; 100 notes "Extracting K of 100..." (the async drop only), then
  "Extracted 100 files"; the folders held 101 files of 52,428,810 bytes, 100 of 52,428,800, and
  1,000 of 2,575,335 (the 25 folders' files, byte counts as in the archive); the refused drop wrote
  nothing. The async drop returned from Drop at once (the `[auto]` line comes before the worker's
  `done`).

### Measurements

This machine, RDP. Decision 1, on a worker thread, three runs each (a throwaway test; "cold" is the
first listing of the archive in the process, the file itself in the OS cache; the subfolder's cold
row from runs that listed only it):

| Listing | (a) shell zip folder | (b) `zip` crate (central directory read) |
|---|---|---|
| 40k flat, top level, cold | 4,432 / 4,482 / 4,442 ms | 99.4 / 100.9 / 99.8 ms (62.2 / 64.3 / 61.6 ms) |
| 40k flat, top level, warm | 570 / 548 / 559 ms | 26.6 / 25.4 / 26.0 ms |
| 2k nested, top level (50 folders), cold | 302 / 312 / 314 ms | 4.1 / 3.9 / 4.1 ms (3.7 / 3.5 / 3.7 ms) |
| 2k nested, top level, warm | 7.0 / 7.2 / 5.2 ms | 0.2 / 0.1 / 0.2 ms |
| 2k nested, subfolder `d25` (40 files), cold | 268 / 273 / 266 ms | 8.4 / 8.4 / 8.5 ms (3.4 / 3.5 / 3.4 ms) |
| 2k nested, subfolder `d25`, warm | 7.2 / 7.3 / 6.9 ms | 0.8 / 0.9 / 0.9 ms |

In the app (`[enum]` lines above): `bigdir.zip` fresh worker 96.1 and 133.7 ms in two runs
(central directory 65.0 and 86.6 ms), cached 29.5 and 38.4 ms; a `nested.zip` subfolder fresh
8.4 to 14.8 ms, cached 1.7 to 3.7 ms, its top level cached 0.4 to 0.7 ms. The filter `*7*` over
`bigdir.zip` kept 13,756 of 40,000 in a 24.9 ms pass (the same count as over `C:\traces\bigdir` in P3-M2).

| Operation | Result |
|---|---|
| Extraction, 25 folders / 1,000 files / 2.5 MB from `nested.zip`, `IFileOperation` | 13.2 s and 11.1 s (two runs); undo record: recycle the 25 extracted folders |
| Its undo (recycle) | 13.8 s and 11.0 s |
| Virtual drop, 101 files / 50 MB, sync (inside Drop) | 260 ms and 180 ms |
| Virtual drop, 100 files / 50 MB, async source (worker) | 1,104 ms and 1,109 ms |
| Virtual drop of the zip folder's object, 25 folders / 1,000 files, sync | 9.8 s, 7.2 s and 6.7 s (UI thread inside Drop throughout; now only for foreign sources) |
| Our own zip drag, 25 folders / 1,000 files, as the extraction job | 10.9 s, UI thread 5.9% busy |
| Gate script | 0 `[archive]` lines |

### Gate

**Passed (2026-10-14), one triplet.** Console WPR, Jeremy elevated, `tools/wpr-triplet.ps1
-Reference p3m3 -Candidate p3m4 -Tag ab13`, accessibility on, Narrator off, no other TwinFM
window, console checks after the triplet: `target-p3m3` (1dd2820) against `target-p3m4`
(b6bfd73, the PR head, built from a detached checkout with `--locked`). Analysed with
`tools/analyze-trace.py --symbols --exe`.

| Run | P3-M3 (`target-p3m3`) | P3-M4 b6bfd73 |
|---|---|---|
| 1 | 20.3% (2030 ms) | 20.6% (2061 ms) |
| 2 | 20.9% (2086 ms) | 20.7% (2074 ms) |
| 3 | 20.8% (2082 ms) | 21.3% (2132 ms) |
| **median** | **20.8%** | **20.7%** |

The medians are 0.1 apart in the candidate's favour and the candidate's run 2 sits below the
reference's, so neither half of the rule calls a gap; a quiet session (every run within one
point). Frame shares match within a few samples in every pair (accessibility 223 / 227, 239 /
219, 233 / 220; render 1172 / 1198, 1204 / 1199, 1198 / 1226; draw_text 678 / 687, 680 / 683,
666 / 680; rows 346 / 373, 383 / 356, 388 / 405; timers 686 / 706, 734 / 689, 722 / 745; heap
206 / 182, 207 / 230, 213 / 210); no archive, zip-reader or virtual-drop frame appears in any
candidate window, as expected from the 0 `[archive]` lines the gate script logs. Traces
`C:/traces/twinfm-ab13-{p3m3,p3m4}-{1,2,3}.etl`, reports under `C:/traces/export/`.

Jeremy's console checks (2026-10-14) on `target-p3m4`: entering the 40k zip, browsing a nested
one, opening a file inside, Ctrl+C/Ctrl+V extraction with the shell's progress and Ctrl+Z, a real
pane-to-pane drag out of a zip, and the in-zip delete and rename refusals all passed; a drag from
Explorer's zip view onto a TwinFM pane showed the deny cursor (see the fix below).

The fix (after the triplet, not re-gated: it touches only the drop target's DragEnter and Drop): a
data object offering only a `Shell IDList Array` of items inside an archive, which the shell's
generic data object over zip items is (`SHCreateDataObject`, measured in process), is now accepted
and extracted by the copy job; the ANSI `FileGroupDescriptor` is read; and a refused drag logs its
formats (`[dnd] DragEnter refused: ...`), so Jeremy's next drag from Explorer's zip view either
shows `[dnd] DragEnter: N item(s) inside an archive (extraction job)` or `[dnd] DragEnter: N
virtual item(s) ...`, or the refusal line with what Explorer offered. The gate script on the fix's
build: `gate: archive lines 0, virtual 0, refused 0, panicked 0`.
Confirmed at the console 2026-10-14 (07ca56c, `C:\traces\p3m4-drop.stderr.txt`): Explorer's zip
view offers `FileGroupDescriptorW` + `FileContents` (ISTREAM) + `Shell IDList Array` with
`IDataObjectAsyncCapability`; a drag of `README-original.txt` out of `acidwarp-win64-static.zip`
onto a pane on `Downloads` was accepted on the async virtual path (`GetAsyncMode = 0x00000000 (1),
StartOperation = 0x00000000`, `virtual drop: 1 file(s), 3249 bytes, async`, `done in 16 ms`,
`EndOperation(0x00000000, copy) = 0x00000000`), 16 ms for one file. That object was already
accepted before the fix, so the deny cursor first reported was probably the spot (the capture shows
`over nowhere ... -> effect none` on entry, then `effect copy` over the pane), not the offer; not
confirmed. Since then the `DragEnter refused` line is logged only when the data object offers
nothing TwinFM takes.

## P3-M5: thumbnails and the large-icon view

The last agreed milestone of the phase, "trace first" (2026-10-13): the first per-row rendering
since M1, so it starts with a trace of per-row layouts against the reference before any thumbnail
UI is built, and the strip pattern (`phase-1-plan.md` § M2 Rework: rows carry no image, one
composed image per pane) must carry it or the milestone stops at the trace with the numbers
recorded. The large-icon view, declined for Phase 2, is in this milestone only because thumbnails
share its work; whether it is wanted at all is Jeremy's call once its trace is read.

The open questions carried from the kickoff handoff (`docs/handoffs/2026-10-14-p3-m5-kickoff.md`,
deleted in this PR):
- **Thumbnails default on or off** once they exist: the gate decides on the defaults, so if the
  thumbnail strip costs anything the default is off and the setting carries it.
- **Whether the large-icon view is wanted at all** if its trace holds: confirm before building the
  real grid (view switch, keyboard by tile, selection and drops).
- **The Slint whole-row snap patch** (P3-M1 Deferred): the fork is touched next when upstreaming or
  when P3-M5 needs a Slint change; Jeremy decides. A view switch rebuilds the list: expect the snap.
- **Upstreaming the two accessibility patches**: Jeremy decides when.

### Trace (2026-10-14): two throwaway variants

Two throwaway builds, each from `origin/main` at 085be4d, each the ONLY list in its build (so
"chosen once per pane" holds trivially: no per-row conditional, no per-row binding on a runtime
property, no view switch, no setting), each built into its own gate dir with `--locked` from a
clean committed tree, branches pushed and never merged:

- **(a) `target-p3m5a`, the details view at 32 px rows with a 32 px icon strip.** Branch
  `trace/p3-m5-a-32px`, commit 9598f79, 72 lines changed each way in five files. `ROW_PX` 22 to 32
  and `ICON_LOGICAL_PX` 16 to 32 in `icons.rs` (the workers ask the shell for 32 x scale px through
  `geom.icon_px`; the overlay path takes `SHIL_LARGE`, `overlay index 2 fetched at 32 px`); every
  row-height `22px` in `pane.slint` to `32px` (both row components, `content-height`, bring-into-view,
  the strip's `y`, the drop highlight, `rows-end`, the rename editor, whose `y` offset moved from
  -3 px to +2 px to stay centred); the rows' `padding-left` 28 to 38 px; the driver's and the
  keyboard menu's row arithmetic read `ROW_PX`; five test expectations moved to 32 px rows. The row
  structure is the reference's. 118 tests pass.
- **(b) `target-p3m5b`, a large-icon grid under the strip pattern.** Branch `trace/p3-m5-b-grid`,
  commits 0528066 and 075f85a, 502 lines in, 170 out. The `ListView` keeps its single `for`, now over
  GRID ROWS: `GridModel(Rc<PaneModel>)` in `model.rs` adapts the pane's shown entries into
  `GridRow { first, tiles: [Tile] }` with `Tile { name, selected, cut, current }`, N = max(1,
  floor(list width / 96 px)) set once per pane from a `changed cols` on the host (startup, split,
  resize only). Each grid row holds a NESTED `for t[c] in tiles`: a `Rectangle` at `x: c * 96px`
  with the selection background, a 1 px cursor border from the tile's own `current` field (no tile
  binds to a root property), one elided `Text` below the icon area, and a `TouchArea` reporting entry
  `first + c` through the existing `row-pointer`. Tiles 96 x 120 px, no tile has an `Image`. The
  strip is one image per pane of N x 96 x scale by rows x 120 x scale px (`GridGeom`,
  `compose_grid`, `GRID_MARGIN_ROWS = 1`: one grid row of slack each side, close to the details
  strip's 88 px, about 2 MB per compose), at `x: 0`, `y: content-y + strip-first * 120px`; icons are
  requested at 96 x scale px and `overlay_pixels` steps SMALL, LARGE, EXTRALARGE, JUMBO until the
  list's icons are at least that size. Accessibility: `accessible-item-count` is the grid-row count,
  each grid row one `list-item`, tiles have no role of their own. Keys: Left/Right one entry, Up/Down
  N. Left unmapped (no panic in any run): the drop highlight and rename editor `y`, `dnd::hit`, the
  shell menu point and the driver's `click` still use 22 px rows (the gate scenario uses none of
  them); the sort arrow shows only for Name; a 24 px Name header was kept so `dnd::LIST_TOP` and the
  driver's thumb point stay valid. 120 tests pass (two added: grid compose placement at 1.0, 1.25,
  1.5 and 2.0 scale; entry to row/column mapping with the last partial row, N = 1 and the N = 0
  guard).

**In-app, relative only** (harness session, 1216 x 839 window, scale 1.0, accessibility on, the
gate scenario on `C:\traces\bigdir`, `[busy]` over the 10 s after the split, each variant run
interleaved with `target-p3m4` by its own implementer; the two implementers' builds and runs
overlapped in time, so the session was noisier than a gate session and the reference sat several
points above its usual in-app 17):

| Run | p3m4 (ref, with a) | (a) 32 px details | p3m4 (ref, with b) | (b) grid |
|---|---|---|---|---|
| 1 | 32.8 (cold) | 15.9 | 19.5 | 17.8 |
| 2 | 22.0 | 17.8 | 23.9 | 15.8 |
| 3 | 24.1 | 17.5 | 23.3 | 16.1 |
| 4 | 22.0 | 17.7 | 24.5 | 16.6 |
| **median** | **23.05** | **17.6** | **23.6** | **16.35** |

Every candidate run sat below every reference run for both variants, 0 panics in every run. The
reading is NOT "32 px rows are cheaper": the scenario scrolls by pixels, and a 32 px row shows
about 31% fewer rows per screen and passes fewer rows per wheel tick, so each variant does less
per-row work in the same 10 s than the reference does. What the in-app numbers do say is that
neither variant's new per-frame cost shows above that saving:

- (a): 251 strips composed per run (reference 371) at 48.5 to 55.2 us each (reference 25 to 40):
  each strip 1.7x the reference's, fewer of them. `[perf]` agrees: about 5.0 to 5.2 ms of UI CPU
  per frame and render p50 about 2.6 ms, against 6.3 to 6.6 and 3.4 for the reference.
- (b): 252 strips per run at 1,312 to 1,390 us each: the composed strip is the grid's one real new
  cost, about 0.34 s of the 10 s, roughly 3.4 of its 16.35 points by the counters (bigdir has one
  icon key, so these composes are pure copies plus allocation and zeroing of a 480 x 1080 px
  buffer). Resolve calls stay at about 5 us. No elimination pass was run, since the grid was not
  the more expensive build; if the console triplet disagrees, the first elimination is the strip
  hidden, the second the labels removed.

**Icons at the larger sizes** (`C:\Windows\System32`, scroll scenario, worker ms per icon): (a)
4.83 ms over 53 icons at 32 px, the first batch 65.5 ms with two 149 ms renders of image files
(`@facial-recognition-windows-hello-rejuv.gif`, `@AdvancedKeySettingsNotification.png`: the shell
renders a thumbnail for those even with `SIIGBF_ICONONLY` at this size, worth knowing for the
thumbnail step); (b) 542 icons at 96 px at 10.05 and 11.12 ms per icon (reference 555 at 16 px, 6.07
and 11.73), slow renders 0 in the measured runs (exploratory runs: the first `.gif` 141 ms,
`ieuinit.inf` 106 ms, `OneDrive` 104 ms), composes on System32 about 300 per run at 1.54 ms (the
fit of each new icon is on the UI thread), in-app busy there 21.2 and 21.4 against the reference's
26.1 (and one disturbed 39.4). `C:\Users\jtkli` with hidden files: `OneDrive` 103 and 153 ms,
`netbird-0.80.0-ipv6fix.exe` 1,302 ms at 32 px (1,057 ms at 16 px in P2-M5).

**Screenshots** (software renderer, `tools/shot.ps1`, session scratchpad, not in the repo): (a)
the 32 px icons sit centred and aligned with their rows in both panes, at the top and scrolled; the
icon fills the row height; sharp, no blurry upscale except the first frame's stretched 16 px
placeholders; the junction arrow on `Application Data` and `Cookies` at a sensible size;
`Downloads` shows its custom icon; `Contacts`, `Documents`, `Favorites` and `Links` show plain
folders in this build AND in the reference (pre-existing, not this change). (b) icons centred in
their tiles and aligned with their labels in both panes, long names elided, custom folder icons
(Contacts, Favorites, Links, Music, Searches, Saved Games, Downloads, OneDrive) and the junction
arrow at 96 px on the nine profile junctions; bigdir after the scenario near entry 24,606 in both
panes. Not investigated: a thin green fringe along the top edge of the shell's 96 px folder icon in
every grid screenshot (possibly non-premultiplied edge pixels from `GetImage` at that size); the
16 x 16 placeholders stretched to 96 px before the first icon lands.

**Gate (2026-10-14, Jeremy at the console): both variants hold the bar.** One command, three
builds, nine runs from 11:52 to 12:00, reference first in each round (`tools/wpr-triplet.ps1
-Reference p3m4 -Candidate p3m5a,p3m5b -Tag ab14`; the script now takes a comma-separated candidate
list), accessibility on, Narrator off, no other TwinFM window, console checks after. `target-p3m4`
(07ca56c) against `target-p3m5a` (9598f79) and `target-p3m5b` (075f85a). Analysed with
`tools/analyze-trace.py --symbols --exe`.

| Run | P3-M4 (`target-p3m4`) | (a) 32 px details, `target-p3m5a` | (b) grid, `target-p3m5b` |
|---|---|---|---|
| 1 | 19.0% (1896 ms) | 13.6% (1359 ms) | 13.3% (1330 ms) |
| 2 | 17.4% (1738 ms) | 13.7% (1371 ms) | 13.2% (1325 ms) |
| 3 | 16.7% (1672 ms) | 13.8% (1381 ms) | 13.2% (1316 ms) |
| **median** | **17.4%** | **13.7%** | **13.2%** |

Every candidate run sits below every reference run, both medians are more than three points under
the reference's and six under the bar, and the candidates' spreads are the tightest ever recorded
(0.2 and 0.1); the in-app lines agree (reference 19.8, 20.2, 18.0; (a) 14.8, 14.4, 15.5; (b) 15.6,
14.5, 12.5), 0 `panicked` in every log. The session itself sat a quiet two to three points below
the P3-M4 gate's (the reference read 20.8 there), which the pairwise reading tolerates. Traces
`C:/traces/twinfm-ab14-{p3m4,p3m5a,p3m5b}-{1,2,3}.etl`, reports under `C:/traces/export/`.

What the symbolized windows say (inclusive samples, run 1; runs 2 and 3 match within a few):

- **(a) costs nothing new.** No `twinfm` frame reaches the table: `icons::compose` 1 sample,
  `resolve_view` 1, `Texture::new_from_image` absent (the reference's 19). The whole difference is
  fewer rows on screen: `ensure_updated_listview` 225 against 364, `draw_text` 486 against 637,
  accessibility 148 against 195, heap 118 against 195. The 32 px strip (251 composes at 47 us) is
  invisible at this resolution. A 32 px details view holds the bar with the same margin the
  reference has, plus what the taller rows save.
- **(b) spends about a third of its busy on the strip and still holds.** `compose_grid` 254 samples
  (19.1% of the window) and the femtovg texture path for the composed image `Texture::new_from_image`
  227 (17.1%, of it `ImageStore::alloc` 169): the M2 mechanism again (a pixel-buffer `Image` has no
  cache key, so every compose uploads a fresh 480 x 1080 px texture), about 480 ms of the 1,330,
  roughly 4.8 points, matching the in-app 252 composes at 1.2 ms. It is paid for by the grid drawing
  far less: `ensure_updated_listview` 42 against 364, `draw_text` 304 against 637, accessibility 65
  against 195 (6 grid rows of 5 tiles against about 33 rows of 4 texts; the console window is 1920
  px wide, so more tiles per row than the harness's 5). `row_data` 2 samples. If the grid is built
  for real, the strip's recompose rate (one grid row of margin, `GRID_MARGIN_ROWS = 1`) and the
  texture upload per compose are the first things to trim; neither is needed to pass.

By the kickoff's decision tree both hold, so P3-M5 is thumbnails first (step 3 of the handoff) and
the grid only if Jeremy wants it. Two decisions for Jeremy before the thumbnail brief: whether
the large-icon view is wanted at all, and how the details view's row height is chosen (32 px for
everyone, or 22 px as today with 32 px rows only when thumbnails are on, a per-pane list variant
like the Type column's, so the default stays the reference row).
