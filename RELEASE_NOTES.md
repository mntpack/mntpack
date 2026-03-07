# Release Notes

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
