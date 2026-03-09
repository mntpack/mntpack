# Release Notes

## 0.3.4 - 2026-03-08

### Changed
- Installer now always performs a post-install managed self-sync:
  - runs `sync MINTILER-DEV/mntpack --name mntpack -g` using the installed `mntpack` binary.
- After post-install sync, installer refreshes resolved managed binary path from `packages/mntpack/install.json`.
- Post-install sync is best-effort; installer warns on sync failure instead of aborting installation.

## 0.3.3 - 2026-03-08

### Fixed
- Fixed Windows self-update failure for managed `mntpack` when running from `packages/mntpack/payload/mntpack`:
  - syncing `mntpack` no longer tries to relink/remove `packages/mntpack/payload` (which can be file-locked by the running process),
  - managed `mntpack` now keeps execution targeting the store binary path for self-update safety.

## 0.3.2 - 2026-03-08

### Fixed
- Fixed stale self-update behavior for managed `mntpack` package:
  - `sync/update` for `MINTILER-DEV/mntpack` now stages the currently running `mntpack` executable into the managed package/store, instead of leaving an old payload binary in place.
- This prevents reintroducing old shim behavior when users sync with a newer local build (`cargo run --release ...`) but still execute the old installed payload binary.

## 0.3.1 - 2026-03-08

### Fixed
- Fixed Windows shim recursion for managed `mntpack` installs where `bin/mntpack.cmd` could recursively invoke itself (`run mntpack`) and hang/lock the terminal.
- `mntpack` shim generation now uses a dedicated non-recursive launcher that directly executes the managed payload binary path.

## 0.3.0 - 2026-03-08

### Changed
- Repo checkout layout now uses nested path structure:
  - `repos/<owner>/<repo>`
- Legacy repo path migration added:
  - existing `repos/<owner>__<repo>` directories are moved to the nested layout on sync.
- Store layout now uses nested repo/version path structure:
  - `store/<repo>/<version-or-commit>/...`
- Installer now installs `mntpack` as a managed package (`packages/mntpack`) with payload in `store/mntpack/<id>` and creates `mntpack` shim in `bin`.
- Installer can bootstrap via an existing `mntpack` on PATH (`mntpack sync MINTILER-DEV/mntpack --name mntpack -g`) and falls back to embedded payload when unavailable.
- `mntpack` package protections:
  - package name `mntpack` is reserved for `MINTILER-DEV/mntpack`,
  - managed `mntpack` package cannot be removed.
- Shim runtime now falls back to `bin/mntpack.cmd` when `bin/mntpack.exe` is not present.

## 0.2.1 - 2026-03-08

### Changed
- Repository checkouts under `repos/` now use git linked worktrees backed by bare mirrors in `cache/git/*.git` instead of full local clones.
- Existing legacy full-clone repo folders are auto-migrated to mirror-backed worktrees during sync.
- Sync recovery now recreates broken repo checkouts as worktrees and prunes stale worktree metadata.

## 0.2.0 - 2026-03-08

### Added
- New commands:
  - `info <package>`
  - `outdated`
  - `clean [--repos]`
  - `exec <repo> [args...]`
  - `which <command>`
- New runtime paths:
  - `<MNTPACK_HOME>/store`
  - `<MNTPACK_HOME>/cache/git`
  - `<MNTPACK_HOME>/cache/exec`

### Changed
- Sync pipeline now uses bare git mirror cache under `cache/git` before updating working repos.
- Lazy package preparation/build flow:
  - `sync` now clone-syncs and records pending preparation when needed.
  - `run` prepares/builds on demand when artifacts are missing.
- Installed binaries are now persisted in shared `store` entries, and package payload paths link back to store content.
- Shims now inspect `autoUpdateOnRun`:
  - if enabled, route through `mntpack run <package>`
  - otherwise prefer direct binary execution.
- Remove/uninstall now cleans unused store entries for removed packages.
- Dependency syncs are executed in parallel task workers.

## 0.1.6 - 2026-03-08

### Added
- New uninstall command with aliases:
  - `remove`
  - `uninstall`
  - `rm`
  - `unsync`

### Changed
- All uninstall aliases now map to the same internal command pipeline.
- Uninstall now removes package files, cleans related global shim files, and prunes cloned repo directories when no installed packages still reference that repo.

## 0.1.5 - 2026-03-07

### Added
- `-r` / `--release <asset-file>` for `sync` / `add` to choose an explicit GitHub release asset file.

### Changed
- Release install flow now supports tag-specific release lookup when `-v <tag>` is used.
- Validation added:
  - `-r` cannot be used with `-v` set to a commit hash.
  - with `-r`, if `-v` is provided, it must resolve as a tag.

## 0.1.4 - 2026-03-07

### Added
- `add` command alias for `sync` (for example: `mntpack add MINTILER-DEV/php-asm`).
- `mntpack.json` command-map binary format support:
  - `"bin": { "phc": "php phc.php" }`

### Changed
- `bin` command maps now auto-set launcher/shim command names and run commands.
- Generic driver accepts command-map `bin` definitions and no longer requires a binary path when run-command launching is configured.

## 0.1.3 - 2026-03-07

### Added
- `mntpack.json` run-command support:
  - `run: "<command>"` for all targets
  - `run: { "<target>": "<command>" }` for per-target commands
- Generic packages can now install with `run` command only (no `bin` required).

### Changed
- Generic `build` is now optional.
- Generic driver install logs are no longer printed during sync.
- Shims now call `mntpack run <package>` first, then fall back to direct binary only if needed.
- `run` now supports command-driven packages via saved `run` command metadata.
- `sync`/`update` now reliably repull by syncing to origin default branch when not version-pinned.
- Package name conflict checks now only consider successful installs (record-based), not leftover directories.
- `update <package>` mirrors `sync` behavior and updates installed packages by package name.

## 0.1.2 - 2026-03-07

### Added
- `update [package]` support for targeted package updates.
- C/C++ project driver with `CMakeLists.txt` / `Makefile` detection.
- C/C++ build support via `cmake` or `make` with executable auto-detection.
- New config option: `autoUpdateOnRun` (default: `false`).
- New configurable tool paths: `paths.cmake` and `paths.make`.

### Changed
- Package names are now considered occupied only after a successful install (record-based conflict checks).
- `sync <package-name>` now updates an already-installed package when the name matches.
- Global shims now prefer invoking `mntpack run <package>` and fall back to direct binary execution.
- `run` now performs an automatic sync/update first when `autoUpdateOnRun` is enabled.
- Git sync behavior now fetches and hard-syncs to `origin` default branch to ensure repulls happen reliably.

## 0.1.1 - 2026-03-07

### Added
- `config` command support (`show`, `get`, `set`, `reset`) to manage `~/.mntpack/config.json` from the CLI.
- Global sync PATH integration to add `~/.mntpack/bin` (or custom install root bin) when needed.
- Separate `mntpack-installer` Cargo project with interactive install directory prompt and default to home directory.
- Embedded payload installer flow: `mntpack-installer` now bundles `mntpack` at build time.
- `sync --name` (`-n`) option for custom package naming.

### Changed
- Default package naming now uses repository name only.
- Name collisions now prompt for a custom package name when syncing without `--name`.
- Rust global shims now prefer Rust executable names for shim command names.
- Shim targets now resolve via `MNTPACK_HOME` (with fallback to detected install root), not hardcoded absolute install paths.

### Notes
- Version policy: for significant feature changes or bug fixes, bump version and add a release notes entry in this file.
