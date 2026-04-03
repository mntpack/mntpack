# mntpack 0.6.11 (2026-04-02)

## Fixed
- Windows argument forwarding for command-driven packages no longer injects literal quotes into forwarded args like `build` and `--project`.
- Simple manifest `run`/`bin` commands are now executed directly when possible, so tools like `cs2luau` receive the expected argv on launch.
