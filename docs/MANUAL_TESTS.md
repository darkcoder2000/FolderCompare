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
```

## Comparison
- [ ] `TimeDiff.exe "%TEMP%\td\left" "%TEMP%\td\right" --compare` opens and compares immediately.
- [ ] Each status shows its color and glyph: newer side bold green, older side grey, one-sided items blue, conflict red with ⚠, identical in normal text.
- [ ] `src` shows `≠` and an orange dot on the folder icon; expanding it shows `main.cs` as newer left.
- [ ] The status bar shows the counts per category.
- [ ] Choosing the same folder on both sides (or a sub-folder) shows a red message and disables Compare.
- [ ] Typing a folder that doesn't exist shows an inline message under the path box.
- [ ] Dropping a folder from Explorer onto a path box fills it in. The dropdown lists recent paths.
- [ ] ⇄ swaps the sides and rescans.

## Filters and view
- [ ] Each Show toggle hides or shows its category right away.
- [ ] *Differences only* hides identical items and folders without differences.
- [ ] Typing in Search filters by name live, including matches inside collapsed folders.
- [ ] Exclude `*.cs` plus F5 removes `main.cs` (and `src` becomes one-sided or empty).
- [ ] Clicking column headers sorts (click again to reverse). Folders stay first unless *View → Folders before files* is off.
- [ ] ←/→ collapse and expand folders. Expand all / Collapse all work.

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

## Performance and robustness
- [ ] Compare two trees of 100,000 files each: the UI stays responsive, the scan can be cancelled with Esc, and memory stays under 500 MB.
- [ ] A folder without read permission shows as an error row, and the scan completes.
- [ ] Paths longer than 260 characters and Unicode names compare and copy correctly.
