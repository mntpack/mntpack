# mntpack 0.6.6 (2026-03-18)

## Fixed
- `run`-driven Rust packages no longer fail binary inference during prepare/update when `mntpack.json` defines `run` targets instead of `bin`.
- C/C++ command-launched packages now follow the same rule, so build drivers can succeed without forcing binary inference when launch commands are already defined.
- This fixes cases like `inscribe` where `mntpack update` followed by launch rebuilt successfully but then errored with `unable to infer rust binary`.
