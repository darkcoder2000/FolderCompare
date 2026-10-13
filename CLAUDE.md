# FolderCompare – notes for Claude

See README.md for features, shortcuts and layout. This file holds working knowledge only.

## Build & test
- .NET 10 SDK (all projects target `net10.0` / `net10.0-windows`).
- Two UI projects during the Avalonia port (branch `avalonia-port`, see `PLAN-avalonia-cross-platform-port.md`):
  - `src/FolderCompare.App` – WPF, Windows only, the released app until the port reaches parity.
  - `src/FolderCompare.Avalonia` – Avalonia 12 (Windows, Linux, macOS), will replace it and then be renamed.
    Run it with `dotnet run --project src/FolderCompare.Avalonia [-- "left" "right" --compare]`.
- `dotnet build`, `dotnet test` (xUnit, all tests must pass before committing). Core tests must also pass on Linux
  (see `CLAUDE.local.md` for the local WSL setup).
- Release exe: `dotnet publish src/FolderCompare.App -c Release -o publish` → single-file self-contained
  `publish\FolderCompare.exe` (~156 MB). Delete `publish\` first so no stale files are left.

## Architecture rules
- `FolderCompare.Core` has no WPF reference. All file access goes through `System.IO.Abstractions.IFileSystem`
  so it stays testable with the mock file system. New logic goes into Core with unit tests where possible.
- Core is cross-platform (tests run on Windows, Linux, macOS). Never hard-code `\` or `OrdinalIgnoreCase` for paths:
  - `DiffNode.RelativePath` uses the OS separator (`DiffNode.JoinRelative`); glob patterns accept `/` and `\`.
  - Name matching goes through `PathComparison` (`CompareOptions.PathComparison`; null = `ForRoots`: case-insensitive
    on Windows/macOS unless a root probes case-sensitive, case-sensitive on Linux). `DiffResult.Options.Paths` holds
    the resolved value – use it for name/path comparisons in the App too.
  - Tests are written with Windows paths wrapped in `TestFs.P(...)`, which maps them to `/...` on Unix.
  - "Hidden & system" on Unix = dotfiles (that's how .NET maps attributes there).
- App is MVVM (CommunityToolkit.Mvvm, `[ObservableProperty]` / `[RelayCommand]`), services registered via DI in
  `App.xaml.cs` / `App.axaml.cs`. After changing state that affects a command, update `RefreshCommandStates` in `MainViewModel`.
- Shared code: `MainViewModel`, `RowViewModel`, `AppSettings`, `SettingsService`, `CommandLineOptions`, the service
  interfaces (`IDialogService`, `IShellService`, `ITextCompareLauncher`), `ExplorerIntegrationService` and `UiThread`
  live in `src/FolderCompare.Avalonia` and are **linked** (not copied) into the WPF project (see its csproj). They must
  compile for both; framework-specific bits use `#if AVALONIA` (defined only in the Avalonia project). Views,
  dialogs, `DialogService`, `ShellService`, `TextCompareLauncher` and the text compare window exist once per UI.
- `IDialogService` is async (Avalonia dialogs are): always `await` it before acting on the answer. The WPF
  implementation wraps its synchronous dialogs in completed tasks.
- Avalonia UI: colors only in `Themes/Colors.axaml` (`ThemeDictionaries`, Light only so far), never hard-coded in
  AXAML/C# – converters and `DiffColors` look keys up there. Icons are geometries in `Themes/Icons.axaml` (Fluent UI
  System Icons, MIT; no Segoe MDL2). Monospace text uses the `MonoFont` resource. Ctrl shortcuts are Cmd on macOS
  (`_command` in the windows); show them via code-set `InputGesture`/tooltips. The result list is a `DataGrid`
  whose `Sorting` is cancelled and forwarded to `MainViewModel.SortBy` (the tree sorts, not the grid).
- WPF UI: colors only in `Themes/Colors.xaml`, never hard-coded in XAML/C#.
- Settings: `AppSettings` → `settings.json` in `%APPDATA%\FolderCompare` (Windows), `~/.config/FolderCompare` (Linux),
  `~/Library/Application Support/FolderCompare` (macOS); both UIs share the file. Window bounds are stored in
  device-independent pixels. Keep new properties backward compatible (defaults).

## Safety invariants (do not weaken)
- Scanning never writes. Every copy/delete target must be checked to be strictly inside its root after `GetFullPath`.
- Containment checks (`SafetyValidator.IsInside`/`EnsureInsideRoot`) are ordinal (case-sensitive) and use
  `Path.DirectorySeparatorChar`; `ValidateRoots` ignores case for "same/nested folder" because that rejects more.
- Copies write to `<name>.tmpcopy` then rename; preserve and verify timestamp/size/attributes, on Unix also the
  file mode (read it with `IFile.GetUnixFileMode` – `IFileInfo.UnixFileMode` is never filled by the wrapper).
  Creation time is only set off Linux.
- Delete goes to the Recycle Bin by default; overwriting a newer file always asks.
- Log every file operation (Serilog).

## Text compare (src/FolderCompare.Core/TextDiff, src/FolderCompare.*/Views/TextCompare)
- Diff: own Myers implementation (`MyersDiff`, `LineDiffer`, `InlineDiffer`), AvalonEdit (WPF) / AvaloniaEdit
  (Avalonia, same class structure: `VisualLineElementGenerator`, `IBackgroundRenderer`, `TextView`) only for
  display/editing.
- Pane alignment relies on **every visual row being exactly one line height**: filler rows come from
  `FillerElementGenerator` (zero-length element with a custom height). Don't enable word wrap or variable line heights
  without reworking scroll sync, `DiffMargins` and `DiffBackgroundRenderer`.
- Block copies are computed by `BlockCopy` (tested on strings) and applied inside `BeginUpdate/EndUpdate` so each
  copy is one undo step.
- `TextFileCodec` must round-trip encoding, BOM and line endings unchanged.
- AvaloniaEdit filler rows are a zero-width `DrawableTextRun` (`Size` = rows × `DefaultLineHeight`, `Baseline` shifted by
  the rows above). Scroll sync uses `TextView.ScrollOffsetChanged` with a reentrancy flag (`_syncingScroll`). Unsaved-change
  prompts are async, so `Closing` cancels first and closes again after the answer.
- Name clashes seen before: `TextRange` (WPF/Avalonia vs. Core – use an alias), `Window.RestoreBounds`,
  `LogicalDirection` (AvaloniaEdit vs. Avalonia.Media.TextFormatting).

## Explorer context menu (src/FolderCompare.ShellExt)
- Native C++ `IExplorerCommand` DLL (WRL, x64, static CRT), built by `build.ps1` with the MSVC Build Tools from an
  incremental target in `FolderCompare.App.csproj`, then embedded in the exe. `dotnet build` of the App needs MSVC.
- Registered per user via `ExplorerCommandHandler` verbs (classic menu, no package). The DLL decides visibility and
  titles (`GetState`/`GetTitle`), stores the left item in `HKCU\Software\FolderCompare\LeftSelection` and starts
  `FolderCompare.exe "left" "right" --compare`. CLSIDs and registry names in `ExplorerMenuLayout` must match `ShellExt.cpp`.
- The DLL is extracted as `%LOCALAPPDATA%\FolderCompare\ShellExt\FolderCompare.ShellExt.<hash>.dll` because Explorer
  keeps it loaded; `EnsureCurrentPath` re-registers on start when exe or DLL hash changed. Restart Explorer to test changes.
- `Shell.Application` → `Namespace(dir).ParseName(name).Verbs()` lists the visible entries for single-item checks.

## Releasing
1. Bump `<Version>` in `src/FolderCompare.App/FolderCompare.App.csproj` (semver; new feature → minor).
2. Test, clean Release publish, check the exe's ProductVersion.
3. Commit, push `main`, create annotated tag `vX.Y.Z`, push tag.
4. `gh release create vX.Y.Z publish/FolderCompare.exe --title "FolderCompare X.Y.Z" --notes ...`
   Release notes describe user-visible changes; no mention of Claude.

## Verifying UI changes
There are no automated UI tests. Run the Debug build, check the relevant items in `docs/MANUAL_TESTS.md`,
and add new checklist items for new features. When automating with SendKeys use the grouped form `^(n)`.
