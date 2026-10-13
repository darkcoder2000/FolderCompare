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
- CI (`.github/workflows/ci.yml`) builds and tests on Windows, Linux and macOS for every push; on Linux/macOS only
  Core tests and the Avalonia project are built (the WPF project needs Windows).
- WPF release exe: `dotnet publish src/FolderCompare.App -c Release -o publish` → single-file self-contained
  `publish\FolderCompare.exe` (~156 MB). Delete `publish\` first so no stale files are left.
- Avalonia release builds take the RID on the command line (nothing is fixed in the csproj):
  `dotnet publish src/FolderCompare.Avalonia -c Release -r <win-x64|linux-x64|osx-arm64|osx-x64> -o publish/<rid>`
  → single file of ~100–115 MB. Linux/macOS can be cross-published from Windows. The macOS `.app` bundle is made by
  `build/macos/make-app.sh` (needs macOS tools, runs in the release workflow).

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
- Delete goes to the Recycle Bin / Trash by default (Windows: VisualBasic `FileSystem`, macOS: `NSFileManager
  trashItemAtURL` via the ObjC runtime, Linux: `FreedesktopTrash` in Core – home trash or `$topdir/.Trash-$uid`, with
  `.trashinfo`). When `IRecycleBin.CanRecycle` is false the VM asks before deleting permanently; never fall back
  silently. Overwriting a newer file always asks.
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
  incremental target in `FolderCompare.App.csproj` / `FolderCompare.Avalonia.csproj`, then embedded in the exe.
  Building either app on Windows needs MSVC. In the Avalonia project the target and the embedded DLL are conditional
  (`WithShellExt`: Windows host and RID empty or `win-x64`), so Linux/macOS builds don't need MSVC. The Tools menu entry
  is hidden where `IExplorerIntegrationService.IsSupported` is false (`NoExplorerIntegration` off Windows).
- Registered per user via `ExplorerCommandHandler` verbs (classic menu, no package). The DLL decides visibility and
  titles (`GetState`/`GetTitle`), stores the left item in `HKCU\Software\FolderCompare\LeftSelection` and starts
  `FolderCompare.exe "left" "right" --compare`. CLSIDs and registry names in `ExplorerMenuLayout` must match `ShellExt.cpp`.
- The DLL is extracted as `%LOCALAPPDATA%\FolderCompare\ShellExt\FolderCompare.ShellExt.<hash>.dll` because Explorer
  keeps it loaded; `EnsureCurrentPath` re-registers on start when exe or DLL hash changed. Restart Explorer to test changes.
- `Shell.Application` → `Namespace(dir).ParseName(name).Verbs()` lists the visible entries for single-item checks.

## Releasing
Until the Avalonia port is merged, releases from `main` are still the WPF app (steps 1–4). The Avalonia app (2.0.0)
uses the release workflow instead (steps A–C).
1. Bump `<Version>` in `src/FolderCompare.App/FolderCompare.App.csproj` (semver; new feature → minor).
2. Test, clean Release publish, check the exe's ProductVersion.
3. Commit, push `main`, create annotated tag `vX.Y.Z`, push tag.
4. `gh release create vX.Y.Z publish/FolderCompare.exe --title "FolderCompare X.Y.Z" --notes ...`
   Release notes describe user-visible changes; no mention of Claude.

Avalonia app (`.github/workflows/release.yml`, runs on every pushed `v*` tag once it is on the default branch):
A. Bump `<Version>` in `src/FolderCompare.Avalonia/FolderCompare.Avalonia.csproj`; the workflow fails if it differs
   from the tag.
B. Commit, push, create an annotated tag `vX.Y.Z` whose message is the release notes (or add
   `docs/release-notes/X.Y.Z.md`), push the tag.
C. The workflow tests, publishes win-x64 / linux-x64 / osx-arm64 / osx-x64, packages (`FolderCompare.exe`, zip,
   tar.gz, ad-hoc signed `.app` zips), adds `SHA256SUMS.txt` and creates a **draft** release. Check it and publish it
   on GitHub. Release notes: user-visible changes only, no mention of Claude.

## Verifying UI changes
There are no automated UI tests. Run the Debug build, check the relevant items in `docs/MANUAL_TESTS.md`,
and add new checklist items for new features. When automating with SendKeys use the grouped form `^(n)`.
UI changes during the port go into both apps (or only the Avalonia one if the WPF app doesn't need them); the
Avalonia app also has OS-specific checklist sections. Linux can be checked locally through WSLg (see
`CLAUDE.local.md`); macOS only through CI unless a Mac is available.
