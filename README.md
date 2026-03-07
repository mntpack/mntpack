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
  - Generic (`mntpack.json` build command)
- GitHub release asset download with source-build fallback
- Package manifest support (`mntpack.json`)
- Global shim generation (`-g/--global`)
- PATH integration for global shims
- Config management from CLI

## Installation Layout

By default `mntpack` uses:

```text
~/.mntpack/
  config.json
  repos/
  packages/
  cache/
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
mntpack sync <repo> [-v <version_or_commit>] [-n <custom_name>] [-g]
mntpack run <package> [args...]
mntpack list
mntpack update
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
mntpack sync owner/repo --name custom-tool
mntpack run scalf
```

## Package Naming Rules

- Default package name: repository name
- If a different repo already uses that name:
  - `sync` prompts for a custom name
- You can always set an explicit name with `--name`

## Global Shims

- Shims are placed in `<MNTPACK_HOME>/bin` (or `~/.mntpack/bin`)
- Rust projects use the Rust executable name for shim name when globally installed
- Shim target paths resolve from `MNTPACK_HOME`

## Manifest (`mntpack.json`)

Supported fields include:

- `name`
- `version`
- `preinstall`
- `postinstall`
- `dependencies`
- `build`
- `bin`
- `release` (platform asset mapping)

## Development

```bash
cargo fmt
cargo check
cd mntpack-installer && cargo check
```

## Release Notes

See [RELEASE_NOTES.md](./RELEASE_NOTES.md).
