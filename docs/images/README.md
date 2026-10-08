# Screenshots

Images used by the main [README](../../README.md). Reference them with relative paths (`docs/images/<file>`).

## Checklist

| # | File | Shows | Status |
|---|---|---|---|
| 1 | `MainWindow.png` | Main window with mixed statuses (only left / only right / newer / identical / conflicts / errors, orange dot on folders) | done |
| 2 | `ContextMenu.png` | Right-click context menu on a selected file (copy / delete / rename) | to do |
| 3 | `FileDiff2.png` | Text compare window: red line highlights, filler rows, copy arrows in the strip between the panes | done |
| 3b | `FileDiff1.png` | Text compare window, small JSON file with word-level highlights | done (not used in README) |
| 4 | `demo.gif` | Optional, about 10 s: compare two folders, then copy one file left to right | to do |

Once a "to do" image exists, enable its reference in the main README (they are there as HTML comments).

## Capture tips

* Run `pwsh tools/New-ExampleDiff.ps1` to create `exampleDiff\folder1` and `folder2`, so the shots show every status.
* [ShareX](https://getsharex.com/) (free) works for both screenshots and GIFs.
* Capture the window only, at 100 % scaling if possible. Aim for about 1200–1600 px wide.
  The existing PNGs come from a high-DPI screen (2800–3800 px wide); consider scaling them down.
* PNG for screenshots, optimized (e.g. with `oxipng` or TinyPNG). Keep a GIF under about 5 MB.
* Use a neutral desktop background and no personal paths or file names.
