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
- Release asset auto-detection (`-r auto` / `--release auto`)
- Content-addressed store (`store/sha256/<hash>/<binary>`)
- Remote binary cache support (`binaryCache` config + `mntpack prebuild`)
- Conflict handling for package names (interactive prompt when needed)
- Managed local NuGet feed and source-package state under `<MNTPACK_HOME>/nuget/{feed,state}`
- `mntpack nuget ...` commands for source-backed and consumer-side NuGet workflows
- Driver-based installation architecture:
  - Rust
  - Python
  - Node
  - .NET / C#
  - C/C++ (`cmake` / `make`)
  - Generic (`mntpack.json` build command)
- GitHub release asset download with source-build fallback
- Version switching for installed packages (`mntpack use <package> <version>`)
- Release-first upgrades (`mntpack upgrade`)
- Repository search (`mntpack search ...`)
- Install inspection (`mntpack inspect owner/repo`)
- Dependency explanation (`mntpack why <package>`)
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
    binary-cache/
  nuget/
    feed/
    state/
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
- Creates `<base>/.mntpack/{repos,packages,store,cache,nuget,bin}`
- Installs `mntpack` as managed package `packages/mntpack`
- Places payload in hash-backed store entries under `store/sha256/<hash>/...`
- Creates `mntpack` shim in `.mntpack/bin`
- Runs a post-install managed self-sync (`sync mntpack/mntpack --name mntpack -g`)
- Adds `.mntpack/bin` to PATH (if missing)
- Sets `MNTPACK_HOME` for custom install root support

## CLI Usage

```bash
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
mntpack doctor [--fix]
mntpack config show
mntpack config get <key>
mntpack config set <key> <value>
mntpack config reset
mntpack nuget init [--path <dir>] [--project <file>]
mntpack nuget use <package> [version] [--path <dir>] [--project <file>] [--build]
mntpack nuget add <package> [version] [--source <source>] [--path <dir>] [--project <file>] [--no-restore] [--build]
mntpack nuget remove <package> [--path <dir>] [--project <file>] [--no-restore] [--build]
mntpack nuget list [--path <dir>] [--project <file>]
mntpack nuget apply [--path <dir>] [--project <file>] [--build]
mntpack nuget restore [--path <dir>] [--project <file>] [--build]
mntpack nuget feed path
mntpack nuget feed list
mntpack nuget source add <name> --repo <owner/repo|url> [--project <file.csproj>] [--package-id <id>] [--version <ver>] [--path <dir>]
mntpack nuget source list [--path <dir>]
mntpack nuget source build <name> [--path <dir>] [--force]
mntpack nuget source build-all [--path <dir>] [--force]
mntpack nuget source update <name> [--path <dir>]
mntpack nuget source sync [--path <dir>] [--force]
```

Examples:

```bash
mntpack sync scalf
mntpack sync MINTILER-DEV/scalf -g
mntpack sync https://github.com/user/repo.git -v 1.2.0
mntpack sync owner/repo -v v1.2.0 -r tool-win64.zip
mntpack sync owner/repo -r auto
mntpack sync owner/repo --name custom-tool
mntpack use ripgrep 14
mntpack exec ripgrep@13 -- --version
mntpack upgrade
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
- accepts `auto` for automatic platform/arch asset matching,
- when used with `-v`, `-v` must be a tag (commit hashes are rejected).

## Package Naming Rules

- Default package name: repository name
- If a different repo already uses that name:
  - `sync` prompts for a custom name
- You can always set an explicit name with `--name`
- `mntpack` is a protected reserved package name:
  - only `mntpack/mntpack` can use package name `mntpack`
  - the `mntpack` package cannot be removed via `remove`/`uninstall`/`rm`/`unsync`

## Global Shims

- Shims are placed in `<MNTPACK_HOME>/bin` (or `~/.mntpack/bin`)
- Rust projects use the Rust executable name for shim name when globally installed
- Shim target paths resolve from `MNTPACK_HOME`
- Shims now check `autoUpdateOnRun` from `config.json`
- If `autoUpdateOnRun` is `true`, shims route through `mntpack run <package>`
- If `autoUpdateOnRun` is `false`, shims execute package binaries directly when available

## Store And Lazy Build

- Binaries are shared in `<MNTPACK_HOME>/store` and package folders link to them.
- Primary store layout is `<MNTPACK_HOME>/store/sha256/<hash>/<binary>`.
- Version aliases for `use` / `exec <package>@<version>` are tracked at:
  - `<MNTPACK_HOME>/store/versions/<repo>/<version-or-commit>/...`.
- `sync` is clone-first and marks packages for lazy preparation/build when needed.
- `run` prepares/builds packages on-demand when artifacts are missing.
- Git mirror cache is kept under `<MNTPACK_HOME>/cache/git`.
- `repos/<owner>/<repo>` checkouts are linked git worktrees backed by those mirrors.

## Binary Cache Config

Configure remote binary cache with:

- `binaryCache.enabled` (`true` / `false`)
- `binaryCache.repo` (optional; falls back to `syncDispatch.repo`, default `mntpack/mntpack-index`)

Example:

```json
{
  "binaryCache": {
    "enabled": true,
    "repo": "mntpack/mntpack-index"
  }
}
```

## Sync Dispatch Config

You can trigger an external GitHub workflow after each successful `mntpack sync` by configuring:

- `syncDispatch.enabled`
- `syncDispatch.repo` (default: `mntpack/mntpack-index`)
- `syncDispatch.tokenEnv` (default: `MNTPACK_SYNC_DISPATCH_TOKEN`)
- `syncDispatch.eventType` (default: `mntpack_sync`)

`sync` sends a `repository_dispatch` event to the configured repo using the token from the env var defined by `syncDispatch.tokenEnv`.

When binary cache is enabled, `sync` also tries to consume prebuilt binaries from cache/index releases (including `.tar.xz`) before local build fallback.

## Manifest (`mntpack.json`)

Supported fields include:

- `name`
- `version`
- `preinstall`
- `postinstall`
- `dependencies`
- `nugetSources` (source-backed C# / NuGet package recipes)
- `nuget` (consumer package declarations)
- `build`
- `run` (string command or target map)
- `bin` (legacy binary path or command map like `{ "tool": "php tool.php" }`)
- `release` (platform asset mapping)

Source-backed NuGet example:

```json
{
  "nugetSources": {
    "CS2Luau.Roblox": {
      "type": "github",
      "repo": "MINTILER-DEV/CS2Luau",
      "project": "src/CS2Luau.Roblox/CS2Luau.Roblox.csproj",
      "packageId": "CS2Luau.Roblox",
      "version": "1.0.0-local.1",
      "outputMode": "feed"
    }
  }
}
```

Consumer package example:

```json
{
  "nuget": {
    "packages": [
      {
        "name": "CS2Luau.Roblox",
        "version": "1.0.0-local.1",
        "source": "mntpack-local"
      }
    ]
  }
}
```

Typical workflow:

```bash
mntpack nuget source add CS2Luau.Roblox --repo MINTILER-DEV/CS2Luau --project src/CS2Luau.Roblox/CS2Luau.Roblox.csproj --package-id CS2Luau.Roblox --version 1.0.0-local.1
mntpack nuget source build CS2Luau.Roblox
mntpack nuget init
mntpack nuget use CS2Luau.Roblox 1.0.0-local.1
```

`nuget use` is the smooth path: it ensures the local feed exists, builds a registered source package if it is missing, writes `NuGet.config`, adds the package to the selected project, and restores it.

## Development

```bash
cargo fmt
cargo check
cd mntpack-installer && cargo check
```

## Release Notes

See [RELEASE_NOTES.md](./RELEASE_NOTES.md).
