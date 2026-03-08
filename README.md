# mntpack

`mntpack` (Mintiler Package Manager) is a GitHub-aware package manager, version manager, and runtime launcher.

It can clone/pull repositories, install from releases or source, create global shims, and run installed tools.

## Projects In This Repository

- `mntpack` (main CLI)
- `mntpack-installer` (installer CLI that embeds `mntpack` at build time)

## Features

- Sync packages from GitHub using shorthand (`repo`), `owner/repo`, or GitHub URL
- Optional version/commit checkout (`-v/--version`)
- Optional custom package name (`-n/--name`)
- Conflict handling for package names (interactive prompt when needed)
- Driver-based installation architecture:
  - Rust
  - Python
  - Node
  - C/C++ (`cmake` / `make`)
  - Generic (`mntpack.json` build command)
- GitHub release asset download with source-build fallback
- Package manifest support (`mntpack.json`)
- Global shim generation (`-g/--global`)
- PATH integration for global shims
- Optional auto-update before run (`autoUpdateOnRun`)
- Config management from CLI

## Installation Layout

By default `mntpack` uses:

```text
~/.mntpack/
  config.json
  repos/
  packages/
  store/
  cache/
    git/
    exec/
  bin/
```

If `MNTPACK_HOME` is set, that directory is used as the root instead.

## Quick Start

### Build `mntpack`

```bash
cargo build --release
```

### Build installer

```bash
cd mntpack-installer
cargo build --release
```

The installer embeds `mntpack` during its build, so the resulting installer binary is self-contained.

### Run installer

```bash
./mntpack-installer --help
```

Installer behavior:

- Prompts for install base directory (default: home directory)
- Creates `<base>/.mntpack/{repos,packages,cache,bin}`
- Installs `mntpack` into `.mntpack/bin`
- Adds `.mntpack/bin` to PATH (if missing)
- Sets `MNTPACK_HOME` for custom install root support

## CLI Usage

```bash
mntpack sync <repo> [-v <tag_or_commit>] [-r <release_asset_file>] [-n <custom_name>] [-g]
mntpack add <repo> [-v <tag_or_commit>] [-r <release_asset_file>] [-n <custom_name>] [-g]
mntpack remove <repo_or_package>
mntpack uninstall <repo_or_package>
mntpack rm <repo_or_package>
mntpack unsync <repo_or_package>
mntpack info <package>
mntpack which <command>
mntpack outdated
mntpack clean [--repos]
mntpack exec <repo> [args...]
mntpack run <package> [args...]
mntpack list
mntpack update [package]
mntpack doctor
mntpack config show
mntpack config get <key>
mntpack config set <key> <value>
mntpack config reset
```

Examples:

```bash
mntpack sync scalf
mntpack sync MINTILER-DEV/scalf -g
mntpack sync https://github.com/user/repo.git -v 1.2.0
mntpack sync owner/repo -v v1.2.0 -r tool-win64.zip
mntpack sync owner/repo --name custom-tool
mntpack rm custom-tool
mntpack info custom-tool
mntpack which phc
mntpack outdated
mntpack clean --repos
mntpack exec MINTILER-DEV/php-asm compile test.php
mntpack run scalf
```

`-r/--release` notes:

- selects a specific GitHub release asset filename to download,
- when used with `-v`, `-v` must be a tag (commit hashes are rejected).

## Package Naming Rules

- Default package name: repository name
- If a different repo already uses that name:
  - `sync` prompts for a custom name
- You can always set an explicit name with `--name`

## Global Shims

- Shims are placed in `<MNTPACK_HOME>/bin` (or `~/.mntpack/bin`)
- Rust projects use the Rust executable name for shim name when globally installed
- Shim target paths resolve from `MNTPACK_HOME`
- Shims now check `autoUpdateOnRun` from `config.json`
- If `autoUpdateOnRun` is `true`, shims route through `mntpack run <package>`
- If `autoUpdateOnRun` is `false`, shims execute package binaries directly when available

## Store And Lazy Build

- Binaries are shared in `<MNTPACK_HOME>/store` and package folders link to them.
- `sync` is clone-first and marks packages for lazy preparation/build when needed.
- `run` prepares/builds packages on-demand when artifacts are missing.
- Git mirror cache is kept under `<MNTPACK_HOME>/cache/git`.

## Manifest (`mntpack.json`)

Supported fields include:

- `name`
- `version`
- `preinstall`
- `postinstall`
- `dependencies`
- `build`
- `run` (string command or target map)
- `bin` (legacy binary path or command map like `{ "tool": "php tool.php" }`)
- `release` (platform asset mapping)

## Development

```bash
cargo fmt
cargo check
cd mntpack-installer && cargo check
```

## Release Notes

See [RELEASE_NOTES.md](./RELEASE_NOTES.md).
