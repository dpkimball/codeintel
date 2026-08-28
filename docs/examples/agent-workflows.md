# Agent workflows (bindb first)

These sequences are what an autonomous agent should run. None of them start with `find` / `rg` over the application tree.

## Implement a change in a crate

1. `codeintel workflow start --store ./store --request "change src/lib.rs" --bind repo=./repo --session /tmp/wf.json`
2. Open only `files_identified` (plus new files that did not exist in the graph). Paths are `repo:path`; opening the unprefixed path still counts as identified.
3. Implement and validate.
4. Commit (re-index follows Git HEAD).
5. `codeintel workflow finish --session /tmp/wf.json --opened src/lib.rs --modified src/lib.rs --validation pass`

Planning, doctrine, and impact never walk the tree. `opened_outside_inspection_set` is non-empty if the agent read a graph file the plan did not name.

## Review a PR for layering

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
