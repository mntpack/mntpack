# mntpack 0.6.10 (2026-04-02)

## Fixed
- Managed `.NET` packages that define their own `build` command no longer fail lazy prepare on multi-project repositories by forcing an implicit `dotnet add/remove` project selection first.
- This restores flows like `mntpack sync cs2luau -g` followed by `cs2luau` for repositories that build through a solution-level manifest command.
