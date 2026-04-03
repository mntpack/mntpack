# mntpack 0.6.12 (2026-04-03)

## Fixed
- Added `mntpack nuget cache clear <package> [version]` plus `--refresh` support on `mntpack nuget add/use/apply/restore` for clearing stale global NuGet cache entries when local feed packages are rebuilt with the same version.
- This makes same-version local feed updates usable without manually deleting entries under `%USERPROFILE%\\.nuget\\packages`.
