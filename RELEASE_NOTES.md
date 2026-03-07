# Release Notes

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
