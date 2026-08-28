# Agent workflows (bindb first)

These sequences are what an autonomous agent should run. None of them start with `find` / `rg` over the application tree.

## Implement a change in a crate

1. `architecture.plan("change binder-storage")`
2. Read `rules_must_remain_satisfied` and `smallest_file_set`.
3. Open only those files.
4. After the patch: `index` then `architecture.doctrine()`.

## Review a PR for layering

1. `architecture.layering()`
2. `architecture.doctrine()`
3. Open files only for findings that need a patch.

## Impact of touching a shared library

1. Index the library repo and each consumer into one store.
2. `architecture.federation()`
3. `architecture.impact("bindb")` — `repositories` and `cross_repo_dependents`.

## Historical regression

1. Index SHA A, then SHA B (or ensure both catalogs exist).
2. `architecture.drift(A, B)` — deps, symbols, doctrine introduced/resolved.
3. Do not check out A to compare architecture.
