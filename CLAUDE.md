# FolderCompare – notes for Claude

See README.md for features, shortcuts and layout. This file holds working knowledge only.
FolderCompare is a Rust + egui/eframe app for Windows, Linux and macOS (2.x). It replaced the Windows-only .NET/WPF
app (1.x, removed from the repo; still in the git history up to tag `v1.5.0`).

## Safety invariants (do not weaken)
- Scanning never writes. Every copy/delete/rename target must be checked to be strictly inside its root.
- Copies write to `<name>.tmpcopy` then rename; preserve and verify timestamp/size/attributes.
- Delete goes to the Recycle Bin / Trash by default; overwriting a newer file always asks.
- Log every file operation.
- `settings.json` stays compatible with 1.x: new properties need defaults, unknown fields are kept.

## Explorer context menu (Windows, `crates/foldercompare/shellext`)
- Native C++ `IExplorerCommand` DLL (WRL, x64, static CRT), built by `build.ps1` with the MSVC Build Tools and
  embedded in the exe.
- Registered per user via `ExplorerCommandHandler` verbs (classic menu, no package). The DLL decides visibility and
  titles (`GetState`/`GetTitle`), stores the left item in `HKCU\Software\FolderCompare\LeftSelection` and starts
  `FolderCompare.exe "left" "right" --compare`. CLSIDs and registry names in `fc_core::shell_layout` must match `ShellExt.cpp`.
- The DLL is extracted as `%LOCALAPPDATA%\FolderCompare\ShellExt\FolderCompare.ShellExt.<hash>.dll` because Explorer
  keeps it loaded; on start the app re-registers when the exe path or DLL hash changed. Restart Explorer to test changes.
- `Shell.Application` → `Namespace(dir).ParseName(name).Verbs()` lists the visible entries for single-item checks.

## Releasing
1. Bump `version` in the root `Cargo.toml` (semver; new feature → minor). The release workflow fails if it differs
   from the tag.
2. Tests, fmt, clippy pass; CI on `main` is green.
3. Push `main`, create an annotated tag `vX.Y.Z` with the message `FolderCompare X.Y.Z`, push the tag.
4. `.github/workflows/rust-release.yml` builds all platforms and creates a **draft** release titled
   `FolderCompare X.Y.Z` (notes = tag message, or `docs/release-notes/X.Y.Z.md` if present).
5. Set the notes (`gh release edit vX.Y.Z --notes-file …`) in the style of earlier releases (short intro, bullet
   list of user-visible changes, a *Downloads* list) and publish with `--draft=false --latest`.
   Release notes and commit messages never mention Claude.

## Verifying UI changes
There are no automated UI tests. Run the Debug build, check the relevant items in `docs/MANUAL_TESTS.md`,
and add new checklist items for new features.

## Code notes
- Cargo workspace at the repo root (edition 2024, shared `[workspace.dependencies]`):
  - `crates/fc-core` – library without UI dependencies.
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
- Cross-platform rules: relative paths use the OS separator (`model::join_relative`), names are
  matched through `PathComparison` (`DiffResult::paths()`), dotfiles count as hidden on Unix. Cancellation is an
  `AtomicBool`; scans run on two scoped threads with a large stack (recursion per folder level, max 512).
- Tests in `crates/fc-core/tests/` run on the real file system (`tempfile`, `filetime`); test paths are written with
  `\` and mapped by `common::p`. OS-specific cases use `#[cfg(...)]`.
- Safety invariants in code: `ops::ensure_inside_root` (lexical `..` resolution, ordinal, strictly
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
  `tracing-appender`, panic hook), `cli.rs`, `settings.rs` (`settings.json` compatible with 1.x: PascalCase, enums as
  strings, unknown fields kept in `extra`, `FOLDERCOMPARE_SETTINGS_DIR` overrides the folder – use it for test runs
  so the user's settings stay untouched), `state.rs` (egui-free view model: filters, rows, sort, expansion,
  selection, counts, `can_*` rules; unit-tested), `app.rs` (workers send `Msg` over an mpsc channel and call
  `request_repaint`), `main_view.rs`, `result_list.rs` (hand-drawn virtualized list, status column fitted under the
  swap button by `fit_columns`), `theme.rs` (**only place with colors**; fonts: Segoe UI / Noto / DejaVu / Helvetica
  when present, Phosphor icons, Hack for monospace), `dialogs.rs` (`egui::Modal` stack).
- Run the Debug build: `target/debug/FolderCompare.exe "left" "right" --compare`. Screenshots: `shot.ps1` in the
  scratchpad (PrintWindow by process id).
- Operations (`actions.rs`): copy/delete/rename/rescan/export/profiles/settings follow the former WPF `MainViewModel` (1.x) step by step.
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
- Explorer menu (`explorer.rs`, Windows only): `build.rs` runs `shellext/build.ps1` (MSVC;
  a mutex serializes parallel cargo runs) for `x86_64-pc-windows-msvc` and embeds the DLL with `include_bytes!`
  (`cfg(shellext)`). Without MSVC (or with `FOLDERCOMPARE_NO_SHELLEXT`) the app builds without the integration and
  the Tools entry is hidden. Registry layout in `fc_core::shell_layout`; test runs with
  `FOLDERCOMPARE_SETTINGS_DIR` skip the start-up re-registration. `build.rs` also compiles a generated `.rc`
  (app.ico, `windows/app.manifest`: asInvoker, longPathAware, PerMonitorV2, version info from Cargo).
- Release workflow (`.github/workflows/rust-release.yml`, only tags `v2.*` – widen the filter for 3.x): bump `version` in the root `Cargo.toml`
  (the workflow fails if it differs from the tag), push an annotated tag whose message is the release notes (or add
  `docs/release-notes/X.Y.Z.md`). It tests, builds win-x64 / linux-x64 / macos-arm64 / macos-x64, packages (exe,
  zip, tar.gz, ad-hoc signed `.app` zips via `build/macos/make-app.sh`), adds `SHA256SUMS.txt` and creates a draft
  release. Local release builds: `cargo build --release` → copy to `publish/rust/<target>/`.
- Verifying the app: `docs/MANUAL_TESTS.md` ("App specifics and per-OS checks"). Linux via WSLg (`CLAUDE.local.md`),
  macOS through CI only.
