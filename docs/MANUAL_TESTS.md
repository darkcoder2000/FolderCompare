# Manual test checklist

Use two scratch folders (never real data). The *Setup* script below creates a pair with every status.

## Setup

```powershell
$L = "$env:TEMP\td\left"; $R = "$env:TEMP\td\right"; $t0 = Get-Date "2024-05-01 12:00"
function F($p, $t, $c = "x") { New-Item -ItemType File -Force $p -Value $c | Out-Null; (Get-Item $p).LastWriteTime = $t }
F "$L\same.txt" $t0;               F "$R\same.txt" $t0.AddSeconds(1)
F "$L\newer-left.txt" $t0.AddHours(2); F "$R\newer-left.txt" $t0
F "$L\newer-right.txt" $t0;        F "$R\newer-right.txt" $t0.AddDays(1)
F "$L\only-left.txt" $t0;          F "$R\only-right.txt" $t0
F "$L\src\main.cs" $t0.AddMinutes(10); F "$R\src\main.cs" $t0
F "$L\onlyleftdir\x.txt" $t0
F "$L\conflict" $t0;               New-Item -ItemType Directory "$R\conflict" | Out-Null
F "$L\content\same-time-size.txt" $t0 "short"; F "$R\content\same-time-size.txt" $t0 "much longer"
F "$L\content\same-time-bytes.txt" $t0 "abc";  F "$R\content\same-time-bytes.txt" $t0 "xyz"
F "$L\content\time-only.txt" $t0.AddHours(3) "same"; F "$R\content\time-only.txt" $t0 "same"
```

## Comparison
- [ ] `FolderCompare.exe "%TEMP%\td\left" "%TEMP%\td\right" --compare` opens and compares immediately.
- [ ] Each status shows its color and glyph: newer side bold green, older side grey, one-sided items blue, conflict red with ⚠, identical in normal text.
- [ ] `src` shows `≠` and an orange dot on the folder icon; expanding it shows `main.cs` as newer left.
- [ ] The status bar shows the counts per category.
- [ ] Choosing the same folder on both sides (or a sub-folder) shows a red message and disables Compare.
- [ ] Typing a folder that doesn't exist shows an inline message under the path box.
- [ ] Dropping a folder from Explorer onto a path box fills it in. The dropdown lists recent paths.
- [ ] ⇄ swaps the sides and rescans.

## Size and content verification
- [ ] Default (*Verify file contents: Off (fast)*): `content\same-time-size.txt` shows ⚠ with both sides in bold orange and no arrow. Its tooltip says the side that is newer can't be determined. `content\same-time-bytes.txt` shows `=` (known limitation), and `content\time-only.txt` shows newer left.
- [ ] *Settings → Verify file contents → Verify files that look identical*, F5: `same-time-bytes.txt` becomes ⚠. `same.txt` stays `=`. The status bar shows "Verifying contents… n%" while it runs.
- [ ] *Verify all same-size files*, F5: `time-only.txt` shows `=` with the tooltip "Content identical, timestamps differ", and if it is the only remaining difference the `content` folder isn't marked `≠`. (`newer-left.txt` and `newer-right.txt` also become timestamp only because they have the same content.)
- [ ] The counts show "same time, different content" and "timestamp only". The *Same time, different* and *Time only* toggles hide and show those rows. *Differences only* hides timestamp-only rows.
- [ ] Copying `same-time-size.txt` (either direction) shows the preview row "ask - same time, different content!", then the overwrite dialog with the orange same-timestamp warning and Skip as the default. This also happens when the overwrite policy is *Overwrite* or *OverwriteIfNewer*.
- [ ] Copying `time-only.txt` shows "Nothing to copy." with the note that only the timestamps differ.
- [ ] The settings dialog shows the hints under Tolerance and the DST checkbox, and the verification mode survives a restart and is saved in profiles.
- [ ] `FolderCompare.exe "%TEMP%\td\left" "%TEMP%\td\right" --compare --content equal-size` verifies contents for that session only (the setting in the dialog is unchanged).
- [ ] With a large folder pair and verification on, Esc during "Verifying contents" cancels the scan.
- [ ] Ctrl+E report lists the new statuses by name.

## Filters and view
- [ ] Each Show toggle hides or shows its category right away.
- [ ] *Differences only* hides identical items and folders without differences.
- [ ] Typing in Search filters by name live, including matches inside collapsed folders.
- [ ] Exclude `*.cs` plus F5 removes `main.cs` (and `src` becomes one-sided or empty).
- [ ] Clicking column headers sorts (click again to reverse). Folders stay first unless *View → Folders before files* is off.
- [ ] ←/→ collapse and expand folders. Expand all / Collapse all work.
- [ ] The columns fill the list without a horizontal scrollbar, and the status column (≠, =, →, ...) sits centered
      under the ⇄ button. Both stay aligned while resizing/maximizing the window, when the vertical scrollbar
      appears or disappears, and after dragging a Size or Modified column (the name columns adjust).

## Copy
- [ ] Select `newer-left.txt` and press Ctrl+Right: the preview lists one copy with overwrite "yes". Confirm, choose Overwrite: the row turns `=` without a full rescan, and the summary shows 1 succeeded.
- [ ] Select `newer-right.txt` and press Ctrl+Right: the preview and overwrite dialogs both warn that the destination is NEWER, and Skip is the default button.
- [ ] Copying `onlyleftdir` to the right creates the folder with its contents. The right file has the same modified time as the left one.
- [ ] Keep both creates `name (1).ext`.
- [ ] Copy a large file (≥ 1 GB) and press Esc mid-copy: no partial file or `.tmpcopy` remains.
- [ ] Copying a file that another program has locked exclusively reports a failure, and the other items still copy.

## Delete / rename / shell
- [ ] Delete on an item that exists on both sides asks Left / Right / Both, then shows the confirmation with count and size. The item lands in the Recycle Bin.
- [ ] "Don't ask again" skips the confirmation for later Recycle Bin deletes (turn it back on in Settings).
- [ ] With *Delete to the Recycle Bin* off, deleting asks a second time and deletes permanently.
- [ ] F2 renames on the chosen sides, and the row updates.
- [ ] Open, Open containing folder, Show in Explorer and Copy full path work for both sides.
- [ ] The context menu's selection helpers (newer left/right, only left/right, invert) select the right rows.

## Persistence and misc
- [ ] Settings dialog changes survive a restart, as do the last folders, window size/position and column widths.
- [ ] Save a profile, change the folders, then pick the profile again: folders, filters and options are restored.
- [ ] Ctrl+E exports CSV and TXT reports of the currently filtered items.
- [ ] *File → Open log folder* shows the day's log, with one line per file operation.

## Explorer context menu
Restart Explorer after changing the shell extension (`Stop-Process -Name explorer`). The DLL is extracted to
`%LOCALAPPDATA%\FolderCompare\ShellExt`. The remembered item is `HKCU\Software\FolderCompare\LeftSelection`.
- [ ] *Tools → Explorer context menu integration* is unchecked on a fresh machine. Checking it adds the entries (Windows 11: *Show more options*). The checkmark survives a restart.
- [ ] Fresh state, right-click a folder: only *Select as left…*.
- [ ] Fresh state, right-click a file: only *Select as left…*.
- [ ] Folder1 as left, right-click folder2: `Compare to "folder1"…` and *Select as left…*.
- [ ] Same, right-click folder1 itself: only *Select as left…*.
- [ ] Folder1 as left, right-click a file: only *Select as left…*.
- [ ] Folder1 as left, rename or delete folder1, right-click folder2: only *Select as left…*, and the stored left is cleared.
- [ ] Select 2 folders, right-click: only *Compare Folders*. It opens one main window that compares at once.
- [ ] Select 2 files, right-click: only *Compare Files*. Only the text compare window opens; closing it exits the app.
- [ ] Select 1 file + 1 folder, right-click: no FolderCompare entries.
- [ ] Select 3 items, right-click: no FolderCompare entries.
- [ ] Folder1 as left → select folder2+folder3, open the menu, close it without executing → right-click folder4: `Compare to "folder1"…` and *Select as left…*.
- [ ] Folder1 as left → select folder2+folder3, execute *Compare Folders* → right-click folder4: only *Select as left…*.
- [ ] Folder1 as left, execute `Compare to "folder1"` on folder2 → right-click any folder: only *Select as left…*.
- [ ] Folder1 as left, *Select as left* on folder2 → right-click folder3: `Compare to "folder2"…`.
- [ ] The text *Compare to left with FolderCompare* never appears.
- [ ] Upgrading from 1.2.0 with the integration on: starting the new exe once replaces the old entries. This also works after a 1.2.0 exe registered its entries again (no *Compare to left with FolderCompare* left over).
- [ ] Move the exe and start it once: the entries point to the new location.
- [ ] Unchecking the menu item removes all entries from Explorer.

## Performance and robustness
- [ ] Compare two trees of 100,000 files each: the UI stays responsive, the scan can be cancelled with Esc, and memory stays under 500 MB.
- [ ] A folder without read permission shows as an error row, and the scan completes.
- [ ] Paths longer than 260 characters and Unicode names compare and copy correctly.

## Compare contents (text files)
- [ ] Double-click a file row, press Enter on it, and use *Tools → Compare contents...* and the context menu: each opens the text compare window. Double-clicking a folder still expands it.
- [ ] Changed, inserted and deleted lines are red, with hatched filler rows on the other side. Both panes stay aligned when scrolling, including far down a long file (3,000+ lines).
- [ ] The changed words inside a changed line are highlighted, and the line-details area shows both versions of the current line.
- [ ] Ctrl+N / Ctrl+P and the toolbar walk through the sections, and the status bar shows "Section x of y".
- [ ] Ctrl+R / Ctrl+L and the ▶ / ◀ arrows in the middle strip copy a section. Undo in the target pane reverts it in one step.
- [ ] Selecting several lines and pressing Ctrl+R copies every section in the selection.
- [ ] Typing in a pane recompares within a moment.
- [ ] *Ignore whitespace* and *Ignore case* turn those differences blue, and Ctrl+N skips them.
- [ ] Ctrl+S saves the focused side and keeps its encoding (try UTF-8 BOM, UTF-16, ANSI) and line endings. The main list then shows the new size and time.
- [ ] Saving after the file was changed by another program asks before overwriting.
- [ ] A file that exists on one side only opens against an empty pane, and saving that pane creates the file.
- [ ] A binary file shows "is not a text file". A file larger than 20 MB asks first.
- [ ] Closing the window, or the main window, with unsaved changes asks Save / Don't save / Cancel, and Cancel keeps it open.

## Rust app (branch `rust-port`)
The Rust/egui app (`target/debug/FolderCompare.exe`, or `publish/rust/<target>/` for release builds) must pass every
item above. For test runs set `FOLDERCOMPARE_SETTINGS_DIR` to a scratch folder: the real `settings.json` (shared
with the WPF app) stays untouched and an existing Explorer registration of another exe is left alone.
Additional items:
- [ ] Settings written by the WPF app are read (window bounds, column widths, filters, profiles, recent paths), and
      the WPF app still reads the file after the Rust app saved it (unknown fields survive).
- [ ] The status column stays centered under the ⇄ button while resizing the window and the size/date columns.
- [ ] Keyboard in the list: Up/Down/PgUp/PgDn/Home/End move the focus (Shift extends), Ctrl+Up/Down move only
      the focus, Space toggles, Right/Left expand/collapse/jump to the parent, Enter toggles or opens the text compare.
- [ ] Dropping a file or folder on a path box sets the folder (a file is replaced by its parent folder); the box
      is outlined while dragging over it.
- [ ] A worker or UI error shows "Something went wrong: ..." with the log folder and the app keeps running.
- [ ] Text compare opens as its own native window per request; several can be open; closing the main window asks
      each window with unsaved changes first (Cancel keeps everything open).
- [ ] Text compare: typing, Enter, Backspace/Delete (Ctrl = word), Tab, arrows, Home (smart)/End, Ctrl+Home/End,
      PgUp/PgDn, Shift selection, mouse selection (drag, double-click word), Ctrl+A/C/X/V, Ctrl+Z/Y/Shift+Z.
      Typing undoes word by word; a section copy is one undo step.
- [ ] Text compare round trip: edit and save files with CRLF, LF, mixed line endings, UTF-8 with/without BOM,
      UTF-16 LE/BE and ANSI; `git diff --no-index` against the original shows only the edited lines.
- [ ] Two files on the command line open only the text compare window; closing it exits the app.

### Windows
- [ ] The exe shows the app icon, version 2.0.0 in its properties, and no console window (release build).
- [ ] Tools > Explorer context menu integration registers/unregisters; the entries work with the Rust exe and the
      registry layout is the same as the WPF app's (switching between both apps re-registers on start).
- [ ] Delete goes to the Recycle Bin; long paths (> 260 chars) can be compared and copied.

### Linux (WSLg or a desktop)
- [ ] Case-only names (`a.txt` / `A.txt`) are separate rows; copying both works.
- [ ] Delete moves to the freedesktop trash (`~/.local/share/Trash`); a failed trash move offers a permanent delete.
- [ ] Dotfiles count as hidden; copied files keep their permission bits (e.g. an executable script).
- [ ] Folder pickers and the export dialog open (xdg-desktop-portal); Open / Show in file manager work.

### macOS (release build from CI)
- [ ] The `.app` starts after removing the quarantine flag (ad-hoc signed); Cmd replaces Ctrl in all shortcuts.
- [ ] Delete moves to the Trash; Show in Finder selects the item.
