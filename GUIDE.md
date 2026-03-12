# mntpack Guide

This guide is for daily use of `mntpack`: installing tools from GitHub, updating them, running them, and managing configuration.

## 1. Install

If you are using the installer project:

```bash
cd mntpack-installer
cargo build --release
./target/release/mntpack-installer
```

The installer:

- asks for install base directory (default: home directory),
- creates `.mntpack` folders,
- installs `mntpack` as package `packages/mntpack`,
- stores mntpack payload in `store/sha256/<hash>/...`,
- creates `mntpack` shim in `.mntpack/bin`,
- runs a post-install managed self-sync for latest `mntpack/mntpack`,
- sets PATH and `MNTPACK_HOME`.

## 2. Core Commands

```bash
mntpack build [recipe.yml]
mntpack build --generate [recipe.yml]
mntpack sync <repo> [-v <tag_or_commit>] [-r <release_asset_file>] [-n <custom_name>] [-g]
mntpack install <repo> [-v <tag_or_commit>] [-r <release_asset_file|auto>] [-n <custom_name>] [-g]
mntpack add <repo> [-v <tag_or_commit>] [-r <release_asset_file>] [-n <custom_name>] [-g]
mntpack remove <repo_or_package>
mntpack uninstall <repo_or_package>
mntpack rm <repo_or_package>
mntpack unsync <repo_or_package>
mntpack reinstall <repo_or_package>
mntpack resync <repo_or_package>
mntpack use <package> <version>
mntpack info <package>
mntpack which <command>
mntpack outdated
mntpack clean [--repos]
mntpack exec <repo> [args...]
mntpack exec <package>@<version> [args...]
mntpack run <package> [args...]
mntpack list [--global]
mntpack update [package]
mntpack upgrade [package]
mntpack inspect <repo>
mntpack search <query...>
mntpack prebuild
mntpack why <package>
mntpack lock regenerate
mntpack doctor [--fix]
```

## 2.1 Build Recipes

`mntpack build` runs YAML build recipes:

- `mntpack build` reads `./mntpack.yml`
- `mntpack build path/to/recipe.yml` reads an explicit file
- `mntpack build --generate` creates `./mntpack.yml`
- `mntpack build --generate path/to/recipe.yml` creates a template at a custom path

Recipe example:

```yaml
version: 1
name: my-project
steps:
  - name: build
    run: cargo build --release
  - name: test
    run: cargo test
```

## 3. Repository Input Formats

You can sync with:

- `repo` (uses `defaultOwner` from config),
- `owner/repo`,
- `https://github.com/owner/repo.git`

Examples:

```bash
mntpack sync scalf
mntpack add MINTILER-DEV/php-asm
mntpack sync MINTILER-DEV/scalf
mntpack sync https://github.com/user/repo.git
mntpack sync scalf -v 1.2.0
mntpack sync scalf -v 8f3c2a1
mntpack add MINTILER-DEV/php-asm -v v1.0.0 -r php-asm-win64.zip
mntpack install MINTILER-DEV/php-asm -r auto
```

`-r/--release` behavior:

- chooses an exact release asset filename to download,
- `-r auto` chooses a matching release asset automatically for current OS/arch,
- cannot be used with `-v` set to a commit hash,
- if `-v` is provided with `-r`, it must be a tag.

## 4. Package Naming

Default package name is the repository name.

You can set a custom package name:

```bash
mntpack sync owner/repo --name mytool
```

Behavior:

- if a name is already used by a different installed package, `mntpack` asks for a custom name,
- names are treated as occupied only when install succeeds (record exists),
- if `sync` input matches an already-installed package name, `mntpack` updates that package.
- `mntpack` is a protected package name (reserved for `mntpack/mntpack` only).

## 5. Updating

Update all:

```bash
mntpack update
```

Update one package:

```bash
mntpack update mytool
```

`update <package>` uses the same sync pipeline for that package.
`update` bypasses lock pinning and regenerates `mntpack.lock` from installed package records.

Release upgrades (latest release assets, not commit pull flow):

```bash
mntpack upgrade
mntpack upgrade ripgrep
```

`upgrade` also bypasses lock pinning and regenerates `mntpack.lock` after upgrade.

## 5.1 Version Switching

Install multiple versions, then switch active version:

```bash
mntpack install ripgrep -v 13
mntpack install ripgrep -v 14
mntpack use ripgrep 14
```

Run a specific installed version without switching:

```bash
mntpack exec ripgrep@13 -- --version
```

## 6. Removing Packages

All of these map to the same internal command:

```bash
mntpack remove owner/repo
mntpack uninstall owner/repo
mntpack rm mytool
mntpack unsync mytool
```

Behavior:

- accepts package name (`mytool`) or repo form (`owner/repo`, URL, or default-owner shorthand),
- removes installed package files and global shim(s),
- removes cloned repo directory when no installed packages still use that repo.

## 7. Running Packages

Run directly:

```bash
mntpack run mytool
```

Pass args:

```bash
mntpack run mytool -- --flag value
```

If `autoUpdateOnRun` is enabled, `run` syncs before launching.
If build/install artifacts are pending, `run` prepares them on-demand (lazy build).

## 8. Package Introspection

Package details:

```bash
mntpack info mytool
```

Find which package provides a command:

```bash
mntpack which phc
```

Check for newer upstream commits:

```bash
mntpack outdated
```

Inspect a repository before install:

```bash
mntpack inspect owner/repo
```

Search GitHub for candidate tools:

```bash
mntpack search json parser
```

## 9. Cleaning Cache

Clear cache:

```bash
mntpack clean
```

Also remove repo clones not used by installed packages:

```bash
mntpack clean --repos
```

## 10. Ephemeral Exec

Run a repository without a global install:

```bash
mntpack exec MINTILER-DEV/php-asm compile test.php
```

## 11. Global Shims

Use `-g` to create a global shim:

```bash
mntpack sync owner/repo -g
```

Shims are created under:

- `<MNTPACK_HOME>/bin` (or `~/.mntpack/bin` if env var is unset).

Notes:

- Rust projects use their Rust executable name for global shim naming.
- Shims call `mntpack run <package>` when possible, so auto-update-on-run applies there too.
- List only global shim mappings with:

```bash
mntpack list --global
```

## 12. Config

Show full config:

```bash
mntpack config show
```

Get one key:

```bash
mntpack config get defaultOwner
mntpack config get autoUpdateOnRun
```

Set values:

```bash
mntpack config set defaultOwner mntpack
mntpack config set autoUpdateOnRun true
mntpack config set paths.cmake cmake
mntpack config set paths.make make
```

Reset:

```bash
mntpack config reset
```

Important config keys:

- `defaultOwner`
- `autoUpdateOnRun` (`true` / `false`)
- `binaryCache.enabled` (`true` / `false`)
- `binaryCache.repo` (for example `MINTILER-DEV/mntpack-binaries`)
- `syncDispatch.enabled` (`true` / `false`)
- `syncDispatch.repo` (default: `mntpack/mntpack-index`)
- `syncDispatch.tokenEnv` (default: `MNTPACK_SYNC_DISPATCH_TOKEN`)
- `syncDispatch.eventType` (default: `mntpack_sync`)
- `paths.git`
- `paths.python`
- `paths.pip`
- `paths.node`
- `paths.npm`
- `paths.cargo`
- `paths.cmake`
- `paths.make`

## 13. Project Type Detection

`mntpack` uses installer drivers:

- Rust: `Cargo.toml`
- Python: `requirements.txt` or `pyproject.toml`
- Node: `package.json`
- C/C++: `CMakeLists.txt` or `Makefile`/`makefile`
- Generic: fallback with `mntpack.json` run/bin

## 13.1 Lockfile and Binary Cache

`mntpack` uses `mntpack.lock` in your current working directory for deterministic installs.

Lock entries include:

- repository (`owner/repo`)
- commit
- binary hash (`sha256:...`)

Behavior:

- `sync` reads lock entries and pins to exact commit/hash when available.
- Hash mismatches during locked install abort the install.
- `lock regenerate` rebuilds the lockfile from installed package records.

Remote binary cache:

- configure `binaryCache.enabled` and `binaryCache.repo`,
- use `mntpack prebuild` inside a repository to upload hashed binaries,
- locked installs try local store first, then remote cache, then local build fallback.

## 14. `mntpack.json` Guide

`mntpack.json` is optional, but recommended for non-trivial packages.

Common fields:

- `name`
- `version`
- `preinstall` (shell command)
- `postinstall` (shell command)
- `dependencies` (other mntpack packages)
- `build` (optional shell command)
- `run` (command to launch the package)
- `release` (GitHub release asset map)

### `run` field (recommended)

`run` is command-based and supports:

- one command for all targets:

```json
{
  "run": "php asm.php"
}
```

- per-target commands:

```json
{
  "run": {
    "windows-x64": "php asm.php",
    "linux-x64": "php asm.php",
    "macos-arm64": "php asm.php"
  }
}
```

Supported target keys:

- `windows-x64`
- `windows-x86`
- `linux-x64`
- `linux-arm64`
- `macos-x64`
- `macos-arm64`

### `bin` command map (auto binary name)

You can define launcher names and commands directly:

```json
{
  "bin": {
    "phc": "php phc.php"
  }
}
```

In this mode:

- `phc` becomes the launcher/shim command name
- `"php phc.php"` is used as the run command

### `build` is optional

If your package does not need a build step, you can omit `build`.

Example generic package:

```json
{
  "name": "php-asm",
  "run": "php asm.php"
}
```

Example with optional build:

```json
{
  "name": "tool",
  "build": "npm run build",
  "run": "node dist/index.js"
}
```

### Legacy `bin`

`bin` path is still accepted for binary-style installs, but `run` is preferred for command-driven launchers.

## 15. Troubleshooting

Check tools:

```bash
mntpack doctor
```

Auto-repair common issues (PATH/shims/missing local artifacts):

```bash
mntpack doctor --fix
```

If a tool is missing, set the matching config path key to the right executable path.

If shims are not found in your shell, ensure `<MNTPACK_HOME>/bin` is on PATH and open a new terminal.

## 16. Files and Folders

Default root:

```text
~/.mntpack
```

Structure:

```text
config.json
repos/
packages/
store/
store/sha256/
store/versions/
cache/
cache/git/
cache/exec/
cache/binary-cache/
bin/
```

`repos/*` entries are mirror-backed git worktrees sourced from `cache/git/*.git`.
Current layout is `repos/<owner>/<repo>`.
