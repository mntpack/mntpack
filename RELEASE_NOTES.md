# Release Notes

## 0.6.0 - 2026-03-12

### Added
- New build recipe system with `mntpack build`:
  - `mntpack build` loads `./mntpack.yml` by default.
  - `mntpack build <file.yml>` runs an explicit recipe file.
  - recipe steps run sequentially with same-line progress output.
- Build recipe template generation:
  - `mntpack build --generate` creates `./mntpack.yml`.
  - `mntpack build --generate <file.yml>` writes a template to a custom path.
  - generated templates include starter steps inferred from common project files (`Cargo.toml`, `package.json`, Python project files).
- YAML recipe support via `serde_yaml` with per-recipe and per-step environment variables plus optional per-step working directory.

## 0.5.2 - 2026-03-11

### Added
- Same-line progress bars for long-running commands:
  - `sync`
  - `update`
  - `upgrade`
  - `prebuild`
- Unicode bar rendering (`█`/`░`) with ASCII fallback (`=`/`>`) when needed.

### Fixed
- `prebuild` binary-cache config validation now reports the real missing value:
  - if `binaryCache.enabled` is `false`, it asks to enable it,
  - if enabled but `binaryCache.repo` is missing, it now explicitly asks to set `binaryCache.repo`.

## 0.5.1 - 2026-03-11

### Fixed
- Worktree repair output and cleanup:
  - broken checkout recovery no longer leaks noisy `fatal: '... is not a working tree'` output during repair paths.
  - worktree cleanup now checks whether a checkout is registered before attempting `git worktree remove`.
  - stale/broken checkouts are still recreated automatically as before.

## 0.5.0 - 2026-03-11

### Added
- Deterministic lockfile support with `mntpack.lock`:
  - `sync` now respects pinned lock entries (`commit` + `binary_hash`) when present.
  - `lock regenerate` command added to rebuild `mntpack.lock` from installed package metadata.
- Content-addressed binary store:
  - binaries are stored at `store/sha256/<hash>/<binary>`.
  - package records now include `binaryHash` and `binaryName`.
- Remote binary cache system:
  - new config section: `binaryCache.enabled` and `binaryCache.repo`.
  - new command: `prebuild` (build/sync current repo and upload hashed binary to cache repo).
  - lockfile-based installs now attempt cache download before local build when hash is known.
- New dependency explanation command:
  - `why <package>` shows upward dependency paths from installed package metadata.

### Changed
- `update` now bypasses lock pinning while syncing and then regenerates `mntpack.lock`.
- `upgrade` now bypasses lock pinning and regenerates `mntpack.lock` after upgrade flow.
- `sync` now refreshes the full lockfile after install and warns when a package has no binary hash.
- Store compatibility for version workflows:
  - version aliases are now tracked under `store/versions/<repo>/<version-or-commit>/...`,
  - `use <package> <version>` and `exec <package>@<version>` now resolve through those aliases while keeping active records hash-backed.
- Remove flow now refreshes lockfile (if present in current directory) and cleans version-alias directories when a repo is fully removed.

## 0.4.5 - 2026-03-09

### Fixed
- Git worktree recovery for sync/upgrade:
  - when a repo checkout is a linked worktree but its `.git` metadata points to a missing path, mntpack now detects it and recreates the checkout automatically.
  - this prevents failures like `failed to resolve path .../cache/git/.../worktrees/...` during `sync`/`upgrade`.

## 0.4.4 - 2026-03-09

### Fixed
- `run` subcommand now disables clap's built-in `--help` flag, so package flags like `--help` are forwarded correctly even without explicit `--` separator.
- This resolves launcher cases like `wtree --help` previously showing `mntpack run` help instead of package help.

## 0.4.3 - 2026-03-09

### Fixed
- `run` CLI argument parsing now accepts hyphen-prefixed package args directly (`--help`, etc.) via `allow_hyphen_values` on trailing args.
- This prevents package flags from being interpreted as `mntpack run` help/options in launcher paths where separator forwarding may vary.

## 0.4.2 - 2026-03-09

### Fixed
- Fixed shim argument forwarding for command-driven packages:
  - generated shims now call `mntpack run <package> -- ...`, so package flags like `--help` are passed to the package instead of being parsed by `mntpack run`.
- Fixed run working-directory behavior for simple local executable run commands (for example `".\\wtree.exe"`):
  - these are now launched directly, preserving the caller's current directory.
- Fixed managed `mntpack` rebuild strategy in self-sync:
  - uses isolated `CARGO_TARGET_DIR=.mntpack-build-target` during managed rebuilds to reduce target-directory lock contention.
- Fixed repeated `os error 32` failures on unchanged managed commit sync:
  - store binaries are no longer force-overwritten for existing entries; copy now occurs only when the target file is missing.

## 0.4.1 - 2026-03-09

### Fixed
- Fixed managed self-sync behavior for `mntpack`:
  - `mntpack sync mntpack` now rebuilds from the synced repository first and installs that built binary.
  - previous behavior could restage the currently running old executable into the new store entry, causing `commit` to update while `mntpack -V` stayed on an older version.
- Added fallback behavior:
  - if rebuild fails, sync falls back to staging the current executable with a warning (so command availability is preserved).

## 0.4.0 - 2026-03-09

### Added
- New commands:
  - `use <package> <version>` to switch active installed version by retargeting package metadata/shims to an existing store version.
  - `upgrade [package]` to upgrade using GitHub release assets (`--release auto` flow) instead of commit-only update behavior.
  - `reinstall <package>` with alias `resync` to remove and reinstall packages in one step.
  - `search <query...>` to search GitHub repositories from the CLI.
  - `inspect <owner/repo>` to inspect repository installability before install (project type, build/run/binary hints).
- `sync` now has `install` alias (`mntpack install ...`).
- `doctor --fix` / `-f` added for automatic remediation.
- `list --global` / `-g` added to show shim mappings only (`shim -> package`).
- `exec <tool>@<version> [args...]` added for running a specific installed version from store without switching active version.

### Changed
- `--release auto` now performs platform/arch asset matching automatically (for example Windows -> `win`, x64 -> `amd64`).
- Explicit `-r/--release` now behaves as release-required: sync fails when no matching release asset is found.
- Generic/no-manifest install fallback now auto-discovers binaries in common output paths:
  - `target/release`, `bin`, `dist`, `build`, and repository root.
- Driver pipeline now applies binary auto-discovery fallback when language drivers return no explicit binary and no command-run launcher is configured.

### Fixed
- GitHub release cache path generation now sanitizes `owner/repo` keys for Windows-safe cache filenames/paths.

## 0.3.6 - 2026-03-08

### Fixed
- Fixed Windows shim exit-code propagation for batch shims (`*.cmd`) where `%errorlevel%` could be captured from earlier commands (for example `findstr`) and incorrectly return `1`.
- Windows shim templates now use delayed expansion (`!ERRORLEVEL!`) and `call` for batch-to-batch invocation to ensure correct return codes.
- This fixes cases like `php-asm` exiting with code `1` even when `mntpack run php-asm` succeeds.

## 0.3.5 - 2026-03-08

### Fixed
- Fixed Windows `os error 32` during `mntpack sync mntpack -g` / `mntpack update` when the running `mntpack` executable is already loaded from the managed store path.
- Managed `mntpack` overwrite behavior now only force-overwrites store binaries when running from a non-store executable (for example `cargo run --release`), avoiding attempts to overwrite the currently running locked binary.

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
