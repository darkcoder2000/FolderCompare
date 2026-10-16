# FolderCompare – notes for Claude

See README.md for features, shortcuts and layout. This file holds working knowledge only.

## Build & test
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

## Rust port (branch `rust-port`)
Full reimplementation in Rust + egui/eframe for Windows, Linux and macOS (plan: `PLAN-rust-port.md`, untracked).
The C# projects stay untouched on this branch until the Rust app reaches parity.
- Cargo workspace at the repo root (edition 2024, shared `[workspace.dependencies]`):
  - `crates/fc-core` – library without UI dependencies (counterpart of `FolderCompare.Core`).
  - `crates/foldercompare` – the egui app, binary `FolderCompare`.
- `cargo build`, `cargo test --workspace`, `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings` – all must pass before committing, on Windows and Linux.
- CI: `.github/workflows/rust.yml` (Windows, Linux, macOS: fmt, clippy, test).
  Its `publish` job (after `build`) uploads release builds per push as run artifacts: win-x64 exe,
  linux-x64 tar.gz, macos-arm64/x64 `.app` zips, named `FolderCompare-<version>-<sha>-<os>` (30 days).
- fc-core layout: `model.rs` (options, `DiffStatus` – variant order is the sort order –, `FileSnapshot`, arena
  `DiffTree` with `NodeId`; refresh grafts new nodes and marks replaced ones `alive = false`), `compare/`
  (`scanner`, `engine` = `DiffEngine::compare`/`prepare_refresh` + `DiffResult::apply_refresh`, `content`,
  `glob`, `timestamp`, `path_comparison`), `ops/` (safety, planner, executor).
- Same cross-platform rules as the C# Core: relative paths use the OS separator (`model::join_relative`), names are
  matched through `PathComparison` (`DiffResult::paths()`), dotfiles count as hidden on Unix. Cancellation is an
  `AtomicBool`; scans run on two scoped threads with a large stack (recursion per folder level, max 512).
- Tests in `crates/fc-core/tests/` run on the real file system (`tempfile`, `filetime`); test paths are written with
  `\` and mapped by `common::p`. OS-specific cases use `#[cfg(...)]`.
- Rust safety invariants (same as above): `ops::ensure_inside_root` (lexical `..` resolution, ordinal, strictly
  inside) guards every copy/delete/rename target; `ops::validate_roots` ignores case. `FileOperationExecutor`
  copies to `<name>.tmpcopy` (1 MiB buffer), sets mtime (+ creation time on Windows/macOS) through the handle,
  renames over the target, copies attributes (Windows) or the mode (Unix), then verifies size/mtime/mode. Deletes go
  through the `ops::Trash` trait (the app wraps the `trash` crate). Every outcome is logged with `tracing`.
  Raw Win32 calls need the `\?\` prefix for long paths (std adds it itself).
- `report.rs` (CSV/TXT, the app adds the BOM) and `shell_layout.rs` (Explorer registry layout, `classify`) are pure
  and tested on every OS.
- `textdiff/`: own Myers (`myers.rs`, explicit stack instead of recursion, `MAX_WORK_PER_SNAKE` fallback),
  `compare_lines`, `compare_inline`, `TextComparison` (fillers, `block_at`, `partner_line`), `compute_block_copy`
  over the `LineSource` trait, `codec` (BOM / strict UTF-8 / Windows-1252, byte-exact round trip; a text without
  line breaks gets the OS newline). All offsets inside lines are **chars**, not bytes or UTF-16 units.
- App crate (`crates/foldercompare`, eframe/egui **0.36** – egui-phosphor 0.14 needs it): `main.rs` (logging via
  `tracing-appender`, panic hook), `cli.rs`, `settings.rs` (same `settings.json` as WPF: PascalCase, enums as
  strings, unknown fields kept in `extra`, `FOLDERCOMPARE_SETTINGS_DIR` overrides the folder – use it for test runs
  so the user's settings stay untouched), `state.rs` (egui-free view model: filters, rows, sort, expansion,
  selection, counts, `can_*` rules; unit-tested), `app.rs` (workers send `Msg` over an mpsc channel and call
  `request_repaint`), `main_view.rs`, `result_list.rs` (hand-drawn virtualized list, status column fitted under the
  swap button by `fit_columns`), `theme.rs` (**only place with colors**; fonts: Segoe UI / Noto / DejaVu / Helvetica
  when present, Phosphor icons, Hack for monospace), `dialogs.rs` (`egui::Modal` stack).
- Run the Debug build: `target/debug/FolderCompare.exe "left" "right" --compare`. Screenshots: `shot.ps1` in the
  scratchpad (PrintWindow by process id).
- Operations (`actions.rs`): copy/delete/rename/rescan/export/profiles/settings follow `MainViewModel` step by step.
  The executor runs on a worker; an overwrite question is `Msg::Overwrite` with a reply channel the dialog answers
  (closing it = Cancel all). Afterwards the affected rows are refreshed (`RefreshRequest`, plus `with_name` for "keep
  both" copies) and applied only if `MainState::generation` is unchanged. Items the trash refused are offered for a
  permanent delete (never silently). Context-menu enable states are captured in `Enables` before the list borrows
  the state.
- Text compare (`crates/foldercompare/src/text_compare/`): `buffer.rs` (lines keep their own endings → mixed
  endings save byte-exact; char positions; undo groups – a block copy is one `begin_group`/`end_group` step; dirty =
  not at the save point), `layout.rs` (`RowMap`: **every visual row is exactly one line height**, text or filler),
  `pane.rs` (hand-drawn editor: gutter, hatched fillers, diff backgrounds, inline ranges, section outline,
  selection, caret; tabs expand to 4 columns), `mod.rs` (window: recompare 250 ms after edits on a thread with a job
  id, copy recompares synchronously first, shared scroll offset for both panes, overview bar, center strip, details,
  save/reload/close prompts). Each window is its own native window (`show_viewport_deferred`, state in
  `Arc<Mutex<…>>`); with two files on the command line `TextOnlyApp` runs it as the root window.
- Explorer menu (Rust, `explorer.rs`, Windows only): `build.rs` runs `src/FolderCompare.ShellExt/build.ps1` (MSVC;
  a mutex serializes it with the WPF build) for `x86_64-pc-windows-msvc` and embeds the DLL with `include_bytes!`
  (`cfg(shellext)`). Without MSVC (or with `FOLDERCOMPARE_NO_SHELLEXT`) the app builds without the integration and
  the Tools entry is hidden. Same registry layout as the WPF app (`fc_core::shell_layout`); test runs with
  `FOLDERCOMPARE_SETTINGS_DIR` skip the start-up re-registration. `build.rs` also compiles a generated `.rc`
  (app.ico, `windows/app.manifest`: asInvoker, longPathAware, PerMonitorV2, version info from Cargo).
- Rust release (`.github/workflows/rust-release.yml`, tags `v2.*`): bump `version` in the root `Cargo.toml`
  (the workflow fails if it differs from the tag), push an annotated tag whose message is the release notes (or add
  `docs/release-notes/X.Y.Z.md`). It tests, builds win-x64 / linux-x64 / macos-arm64 / macos-x64, packages (exe,
  zip, tar.gz, ad-hoc signed `.app` zips via `build/macos/make-app.sh`), adds `SHA256SUMS.txt` and creates a draft
  release. Local release builds: `cargo build --release` → copy to `publish/rust/<target>/`.
- Verifying the Rust app: `docs/MANUAL_TESTS.md` ("Rust app" section, per OS). Linux via WSLg (`CLAUDE.local.md`),
  macOS through CI only.
