# Project: TimeDiff – Timestamp-Based Directory Compare Tool (Windows / C# / WPF)

> **Instructions for Claude Code:** Build the application described below. Work in the phases listed in section 12, commit after each phase, and keep the solution building at all times. Ask me before deviating from the tech stack in section 2. Where a requirement says "should", use your judgment; where it says "must", it is mandatory.

---

## 1. Purpose

A lightweight, Beyond Compare-style folder comparison tool for Windows. It compares two directory trees **by modification timestamp only** (no content or hash comparison in v1), shows the result in a side-by-side UI, and lets the user copy or delete files via a right-click context menu.

Non-goals for v1: content/hash comparison, text diff, three-way compare, remote locations (SFTP/SMB URLs), cross-platform support.

---

## 2. Tech Stack (fixed)

| Item | Choice |
|---|---|
| Language | C# 12 |
| Runtime | .NET 8 (LTS), Windows only (`net8.0-windows`) |
| UI | WPF |
| Pattern | MVVM, with `CommunityToolkit.Mvvm` (source generators, `ObservableObject`, `RelayCommand`) |
| DI / hosting | `Microsoft.Extensions.DependencyInjection` (keep it light) |
| Logging | `Serilog` with a rolling file sink (`%LOCALAPPDATA%\TimeDiff\logs`) |
| Settings | JSON file in `%APPDATA%\TimeDiff\settings.json` (`System.Text.Json`) |
| Recycle bin delete | `Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile/DeleteDirectory` with `RecycleOption.SendToRecycleBin` |
| Tests | xUnit (+ `System.IO.Abstractions` for a mockable file system) |
| Packaging | `dotnet publish` as single-file, self-contained, `win-x64` (no installer required) |

Solution layout (suggested):

```
TimeDiff.sln
 ├─ src/TimeDiff.Core/        # comparison engine, models, file operations (no UI references)
 ├─ src/TimeDiff.App/         # WPF project (Views, ViewModels, Converters, Resources)
 └─ tests/TimeDiff.Core.Tests/
```

The Core project must have **no dependency on WPF** so the engine is fully unit-testable.

---

## 3. Core Comparison Engine

### 3.1 Inputs
- Left folder path, right folder path.
- Options: recursive (default on), timestamp tolerance, DST offset handling, symlink handling, include/exclude patterns, ignore hidden/system files.

### 3.2 Timestamp comparison rules
- Compare **last write time (UTC)** only.
- **Tolerance:** configurable, default **2 seconds** (covers FAT/exFAT rounding). Two files whose timestamps differ by ≤ tolerance are "identical".
- **DST option:** optional "ignore ±1 hour differences" setting (default off). When on, a difference of exactly 3600 s (± tolerance) counts as identical.
- File size is shown but is **not** used for the comparison result.

### 3.3 Item classification
Each relative path gets exactly one status:

| Status | Meaning |
|---|---|
| `OnlyLeft` | Exists only in left |
| `OnlyRight` | Exists only in right |
| `NewerLeft` | Exists in both, left is newer |
| `NewerRight` | Exists in both, right is newer |
| `Identical` | Exists in both, timestamps equal within tolerance |
| `TypeConflict` | File on one side, folder on the other |
| `Error` | Could not be read (access denied, locked, etc.) – store the error message |

Folders get a **rolled-up status**: `Identical` only if everything inside is identical; otherwise `Differs`. A folder that exists on one side only is `OnlyLeft`/`OnlyRight`.

### 3.4 Scanning behavior
- Use streaming enumeration (`Directory.EnumerateFileSystemEntries` / `FileSystemEnumerable`), not `GetFiles` into big arrays.
- Enumerate left and right in parallel (bounded parallelism).
- Path matching is **case-insensitive** (Windows semantics).
- Must support **long paths** (> 260 chars) and Unicode names.
- Symlinks/junctions: setting `Follow | Skip` (default `Skip`), with loop protection if following.
- Unreadable items must not abort the scan; mark them `Error` and continue.
- The scan runs on a background task with **progress reporting** (items scanned, current folder) and supports **cancellation** via `CancellationToken`.
- The scan **must never modify** anything.

### 3.5 Result model (suggested)
```csharp
record DiffNode(
    string RelativePath, string Name, bool IsDirectory,
    FileInfoSnapshot? Left, FileInfoSnapshot? Right,
    DiffStatus Status, string? ErrorMessage,
    List<DiffNode> Children);

record FileInfoSnapshot(long Size, DateTime LastWriteUtc, FileAttributes Attributes, string FullPath);
```

---

## 4. UI Requirements (WPF)

### 4.1 Main window layout
1. **Top bar:** Left folder path box + Browse button | Right folder path box + Browse button | Swap button (⇄) | Compare/Refresh button. Both path boxes accept typed paths, pasted paths, and **drag-and-drop of a folder**. Path boxes have a dropdown with recent paths (last 20).
2. **Toolbar:** filter toggles (see 4.5), search box, Copy →, ← Copy, Delete, Refresh, Options.
3. **Main area:** side-by-side comparison (see 4.2).
4. **Status bar:** counts per category (e.g. `12 only left · 5 newer right · 340 identical`), number of selected items, total size of selected items, and a progress bar while scanning or copying.

### 4.2 Side-by-side view
**Recommended implementation:** a single virtualized tree-grid where **each row represents one relative path** and contains both a left cell group and a right cell group. This guarantees perfect row alignment and synchronized scrolling without any scroll-sync code. (WPF has no built-in TreeListView. Either flatten the tree into an `ObservableCollection<RowViewModel>` with an indent level and expand/collapse toggles, or use a `TreeView` with a custom `ControlTemplate`. The flattened-list approach is preferred for virtualization and performance.)

Each row shows, for each side:
- Icon (file/folder), name, size, modified date/time.
- A missing item shows an empty placeholder cell.
- A center column shows a **status glyph** (e.g. `→` copy-to-right candidate, `←`, `=`, `≠`, `⚠`).

Requirements:
- Row virtualization (`VirtualizingStackPanel`, recycling mode) – must stay smooth with **100,000+ rows**.
- Expand/collapse folders; "Expand all" / "Collapse all"; keyboard navigation (arrows, ←/→ for collapse/expand, Space to toggle selection).
- Columns sortable (name, size, date, status) and resizable. Sorting keeps folders grouped before files by default (configurable).
- **Multi-select** with Ctrl/Shift click and rubber-band not required.
- Optional **flat list mode** (toggle) showing full relative paths instead of a tree.

### 4.3 Color coding (theme-aware, configurable in resources)
| State | Style |
|---|---|
| Newer side | Bold, green text |
| Older side | Grey text |
| Only-on-one-side | Blue text |
| Identical | Default text |
| Type conflict / Error | Red text with warning icon |
| Folder with differences | Folder icon with a small marker |

Provide **light and dark themes** (system-follow option). Keep brushes in `ResourceDictionary` files, not hard-coded.

### 4.4 Context menu (right-click)
Right-click acts on the **current multi-selection** (if the clicked row is not part of the selection, select it first). Items are enabled/disabled depending on the selection:

- **Copy to Right →**   (enabled if any selected item exists on the left)
- **← Copy to Left**   (enabled if any selected item exists on the right)
- **Delete Left**, **Delete Right**, **Delete Both** (to Recycle Bin by default)
- **Rename…**
- **Open** (default app) / **Open containing folder** / **Show in Explorer** (`explorer.exe /select,`)
- **Copy full path** (left / right / both) to clipboard
- **Select all newer on left**, **Select all newer on right**, **Select all only-left**, **Select all only-right**, **Invert selection**
- **Expand / Collapse subtree**

Copying a folder copies its whole contents. Copying an item creates missing parent directories on the target side.

### 4.5 Filtering and views
- Toggle buttons to show/hide: Only Left, Only Right, Newer Left, Newer Right, Identical, Errors/Conflicts. Default: show everything except... show everything.
- **"Show differences only"** quick toggle (hides identical items and folders with no differing content).
- **Include/exclude patterns** (glob, semicolon-separated, e.g. `*.tmp;.git;node_modules;bin;obj`), applied during the scan.
- **Quick search** box: live filter by file/folder name within the displayed results.
- Option: ignore hidden and system files.

---

## 5. File Operations

### 5.1 Copy
- Copy is performed in the **background** through an **operation queue** with per-file progress, overall progress, and a Cancel button.
- **Preserve the last-write timestamp** (and creation time where possible) on the copied file so it becomes `Identical` afterwards.
- Preserve file attributes (read-only etc.). If the destination is read-only and overwrite is confirmed, clear the attribute temporarily.
- After each copy, **verify**: destination exists, size matches, timestamp matches within tolerance. Report mismatches as failures.
- Copy to a temporary name (`.tmpcopy`) in the target folder and rename on success, so partial files never replace good ones.
- After a successful operation, **update the affected rows in place** (re-stat those items) instead of rescanning the entire tree.

### 5.2 Overwrite policy
Setting + per-operation dialog with: **Ask each time / Skip / Overwrite / Overwrite only if newer / Keep both (rename)**, plus an "Apply to all" checkbox. Overwriting a file that is **newer** than the source must trigger an explicit extra warning, since the direction might be a mistake.

### 5.3 Delete
- Default: send to Recycle Bin. Setting for permanent delete (with a stronger confirmation).
- Confirmation dialog listing the count and total size, with a "Don't ask again" option (only for recycle-bin deletes).

### 5.4 Dry run / preview
- Before executing copy/delete, show a **preview dialog** listing every action (source → destination, size, overwrite yes/no). Can be disabled in settings ("Skip preview for fewer than N items").

### 5.5 Result summary
- After every operation batch show a summary: succeeded / skipped / failed, with the failure reasons and an option to copy the report to the clipboard. All operations are logged (section 9).

### 5.6 Safety rules (must)
- Never modify, create, or delete anything during scan/compare.
- Never operate outside the two root folders. Validate that every target path resolves (after `Path.GetFullPath`) to inside the chosen root, to guard against path-traversal bugs.
- Refuse to run if left and right resolve to the same folder, or if one is inside the other (show a clear message).
- Handle locked/in-use files gracefully (report, do not crash).

---

## 6. Sessions, Settings, and Command Line

- **Command line:** `TimeDiff.exe "C:\A" "D:\B" [--compare] [--no-recursive]` pre-fills both folders and optionally starts the comparison immediately.
- **Single-instance** not required.
- **Remember** last-used folder pair, window size/position, column widths, and all options.
- **Favorites / profiles:** save a named pair of folders plus the options (filters, tolerance) and reopen it from a dropdown or menu.
- **Keyboard shortcuts:**

| Key | Action |
|---|---|
| F5 | Refresh / rescan |
| Ctrl+Right | Copy selection to right |
| Ctrl+Left | Copy selection to left |
| Delete | Delete (asks which side if both exist) |
| F2 | Rename |
| Ctrl+F | Focus search box |
| Ctrl+A | Select all visible |
| Ctrl+E | Export report |
| Esc | Cancel running scan/operation |

- **Refresh options:** full rescan, or rescan only the selected folder subtree.
- **Export report** of the current (filtered) differences to **CSV** and plain text.
- **Settings dialog** covering: tolerance, DST handling, symlink mode, hidden/system files, default exclude patterns, overwrite policy, delete mode, confirmations, theme, preview threshold.

---

## 7. Non-Functional Requirements

- **Performance targets** (local SSD): scanning 100,000 files in ≈ 10 s or less; UI remains responsive at all times; memory < 500 MB for 100k files per side.
- **Responsiveness:** all I/O off the UI thread; UI updates batched (e.g. every 100 ms) during scanning.
- **Reliability:** no unhandled exceptions reaching the user; global exception handler logs and shows a friendly dialog.
- **Accessibility:** keyboard-operable, readable contrast in both themes, sensible tab order.
- **Localization:** English only for v1, but keep strings in `.resx` for later translation.
- **Portability:** produce a single-file `TimeDiff.exe`; no admin rights required.

---

## 8. Error Handling

| Situation | Behavior |
|---|---|
| Folder does not exist / not accessible | Inline message next to the path box, comparison disabled |
| Access denied on a subfolder | Mark node as `Error`, continue scan, show count in status bar |
| Disk full / copy failure | Stop that item, record failure, continue with the rest unless the user cancels |
| Path too long / invalid characters | Report as failed item with reason |
| Cancel during copy | Finish or roll back the current file (no partial files remain), then stop |

---

## 9. Logging

- Serilog rolling daily file log, keep 14 days.
- Log every file operation: timestamp, action, source, destination, result, error.
- "Open log folder" menu entry.

---

## 10. Testing

Unit tests in `TimeDiff.Core.Tests` (use `System.IO.Abstractions.TestingHelpers` for a mock file system) must cover:

- Classification of all statuses, including tolerance edge cases (exactly at, just below, just above the threshold).
- DST ±1 hour handling on/off.
- Folder roll-up status.
- Case-insensitive path matching.
- Type conflicts (file vs. folder).
- Include/exclude pattern matching.
- Symlink loop protection.
- Copy: timestamp preserved, parent folders created, overwrite policies, verification failure path.
- Safety rules: path traversal rejection, nested/same-root rejection.
- Cancellation of scan and copy.

Also add a small manual test checklist in `docs/MANUAL_TESTS.md`.

---

## 11. Acceptance Criteria (definition of done for v1)

1. I can pick two folders, press Compare, and see aligned side-by-side results with correct statuses.
2. Right-clicking one or more rows offers Copy to Right/Left and Delete; after copying, the affected rows become `Identical` without a full rescan.
3. Timestamps on copied files match the source.
4. Scanning 100k files does not freeze the UI and can be cancelled.
5. Filters, search, and "differences only" work and update instantly.
6. Settings, last folders, and profiles persist across restarts.
7. Command-line invocation with two folders works.
8. Unit tests pass; `dotnet publish` produces a working single-file `TimeDiff.exe`.
9. No operation ever touches a path outside the two roots.

---

## 12. Suggested Build Order

1. **Phase 1 – Core engine:** solution setup, models, scanner, timestamp comparer, roll-up, unit tests.
2. **Phase 2 – Basic UI:** main window, folder pickers, scan with progress/cancel, flattened side-by-side tree-grid with status colors.
3. **Phase 3 – File operations:** context menu, copy/delete queue, overwrite policy, preview dialog, verification, in-place row refresh, summary dialog.
4. **Phase 4 – Filtering and productivity:** filter toggles, search, include/exclude patterns, selection helpers, sorting, flat mode, keyboard shortcuts.
5. **Phase 5 – Persistence and polish:** settings dialog, recent paths, profiles, command line, themes, report export, logging, global error handling.
6. **Phase 6 – Hardening:** performance test with 100k+ files, long paths, locked files, packaging, manual test checklist, README.

After each phase: build, run tests, and give me a short summary of what works and what is left.

---

## 13. Future Ideas (out of scope for v1, keep the architecture open for them)

- Optional comparison modes: size, content hash.
- Text diff viewer on double-click.
- Three-way compare.
- Sync rules (mirror / update / two-way).
- SFTP / SMB / cloud locations (introduce an `IFileSystemProvider` abstraction now to make this easier).
