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
- installs `mntpack` into `.mntpack/bin`,
- sets PATH and `MNTPACK_HOME`.

## 2. Core Commands

```bash
mntpack sync <repo> [-v <tag_or_commit>] [-r <release_asset_file>] [-n <custom_name>] [-g]
mntpack add <repo> [-v <tag_or_commit>] [-r <release_asset_file>] [-n <custom_name>] [-g]
mntpack run <package> [args...]
mntpack list
mntpack update [package]
mntpack doctor
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
```

`-r/--release` behavior:

- chooses an exact release asset filename to download,
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

## 6. Running Packages

Run directly:

```bash
mntpack run mytool
```

Pass args:

```bash
mntpack run mytool -- --flag value
```

If `autoUpdateOnRun` is enabled, `run` syncs before launching.

## 7. Global Shims

Use `-g` to create a global shim:

```bash
mntpack sync owner/repo -g
```

Shims are created under:

- `<MNTPACK_HOME>/bin` (or `~/.mntpack/bin` if env var is unset).

Notes:

- Rust projects use their Rust executable name for global shim naming.
- Shims call `mntpack run <package>` when possible, so auto-update-on-run applies there too.

## 8. Config

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
mntpack config set defaultOwner MINTILER-DEV
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
- `paths.git`
- `paths.python`
- `paths.pip`
- `paths.node`
- `paths.npm`
- `paths.cargo`
- `paths.cmake`
- `paths.make`

## 9. Project Type Detection

`mntpack` uses installer drivers:

- Rust: `Cargo.toml`
- Python: `requirements.txt` or `pyproject.toml`
- Node: `package.json`
- C/C++: `CMakeLists.txt` or `Makefile`/`makefile`
- Generic: fallback with `mntpack.json` run/bin

## 10. `mntpack.json` Guide

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

## 11. Troubleshooting

Check tools:

```bash
mntpack doctor
```

If a tool is missing, set the matching config path key to the right executable path.

If shims are not found in your shell, ensure `<MNTPACK_HOME>/bin` is on PATH and open a new terminal.

## 12. Files and Folders

Default root:

```text
~/.mntpack
```

Structure:

```text
config.json
repos/
packages/
cache/
bin/
```
