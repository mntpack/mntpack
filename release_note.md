# mntpack 0.6.9 (2026-04-02)

## Added
- Source-backed NuGet package workflows with `mntpack nuget source add/list/build/build-all/update/sync`.
- Managed local feed state and package inspection via `mntpack nuget feed path` and `mntpack nuget feed list`.
- `mntpack nuget init` and `mntpack nuget use` for smoother consumer-project setup and lazy local-package builds.
- `mntpack.json` support for `nugetSources` plus structured consumer package declarations under `nuget.packages`.

## Changed
- The managed local NuGet feed now lives at `<MNTPACK_HOME>/nuget/feed` with source-package state tracked in `<MNTPACK_HOME>/nuget/state`.
- Source package builds now clone/update GitHub repos, detect the target C# project, run `dotnet restore/build/pack`, and publish `.nupkg` output into the local feed.
- Consumer-side `nuget add/apply/use` commands now auto-build registered local source packages when the requested package is missing from the feed.
