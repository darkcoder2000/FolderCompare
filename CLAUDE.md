# FolderCompare – notes for Claude

See README.md for features, shortcuts and layout. This file holds working knowledge only.

## Build & test
- .NET 10 SDK (all projects target `net10.0` / `net10.0-windows`).
- `dotnet build`, `dotnet test` (xUnit, all tests must pass before committing).
- Release exe: `dotnet publish src/FolderCompare.App -c Release -o publish` → single-file self-contained
  `publish\FolderCompare.exe` (~156 MB). Delete `publish\` first so no stale files are left.

## Architecture rules
- `FolderCompare.Core` has no WPF reference. All file access goes through `System.IO.Abstractions.IFileSystem`
  so it stays testable with the mock file system. New logic goes into Core with unit tests where possible.
- App is MVVM (CommunityToolkit.Mvvm, `[ObservableProperty]` / `[RelayCommand]`), services registered via DI in
  `App.xaml.cs`. After changing state that affects a command, update `RefreshCommandStates` in `MainViewModel`.
- Colors live only in `Themes/Colors.xaml` (prepared for a later dark theme), never hard-coded in XAML/C#.
- Settings: `AppSettings` → `%APPDATA%\FolderCompare\settings.json`; keep new properties backward compatible (defaults).

## Safety invariants (do not weaken)
- Scanning never writes. Every copy/delete target must be checked to be strictly inside its root after `GetFullPath`.
- Copies write to `<name>.tmpcopy` then rename; preserve and verify timestamp/size/attributes.
- Delete goes to the Recycle Bin by default; overwriting a newer file always asks.
- Log every file operation (Serilog).

## Text compare (src/FolderCompare.Core/TextDiff, src/FolderCompare.App/Views/TextCompare)
- Diff: own Myers implementation (`MyersDiff`, `LineDiffer`, `InlineDiffer`), AvalonEdit only for display/editing.
- Pane alignment relies on **every visual row being exactly one line height**: filler rows come from
  `FillerElementGenerator` (zero-length element with a custom height). Don't enable word wrap or variable line heights
  without reworking scroll sync, `DiffMargins` and `DiffBackgroundRenderer`.
- Block copies are computed by `BlockCopy` (tested on strings) and applied inside `BeginUpdate/EndUpdate` so each
  copy is one undo step.
- `TextFileCodec` must round-trip encoding, BOM and line endings unchanged.
- Name clashes seen before: `TextRange` (WPF vs. Core – use an alias), `Window.RestoreBounds`.

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
