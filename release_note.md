# mntpack 0.6.8 (2026-04-02)

## Added
- First-class `.NET` / C# project detection for `*.csproj`, `*.sln`, `*.slnx`, `Directory.Build.*`, and `global.json`.
- Managed local NuGet feed support under `<MNTPACK_HOME>/nuget/source`.
- New `mntpack nuget ...` commands to ensure `NuGet.config`, add/remove/list package declarations, and apply/restore packages from `mntpack.json`.

## Changed
- Detected `.NET` repositories now ensure a project-local `NuGet.config` includes the managed `mntpack-local` feed.
- `mntpack.json` now supports a `nuget` field for declarative NuGet package requirements.
- `.NET` installs/builds prefer `dotnet build` on the detected solution and otherwise target the detected project in Release mode.
