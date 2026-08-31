---
title: Validation — issue #11
description: Validation evidence for codeintel issue #11.
category: reviews
---

# Validation — issue #11

Throwaway store `/tmp/codeintel-wf-poc4`. Indexed Git HEAD **before** the workflow commit (`990812c`, 49 files), then `workflow start` against that graph. Implementation used the inspection set plus new files. After commit `5bf302c`, `workflow finish` re-indexed the same store.

Request:

```text
wire workflow start and finish into src/lib.rs and src/main.rs using Architecture
in src/architecture.rs; update tests/architecture.rs README.md CLAUDE.md
docs/examples/agent-workflows.md docs/specs/agent-architecture-api.md
```

## Execution metrics

| Metric | Value |
|--------|--------|
| Planning time | **4 ms** |
| Repositories consulted | **1** (`codeintel`) |
| Files in the indexed graph (start) | **49** |
| Files identified by the graph | **8** |
| Files actually opened (implementation set) | **11** (8 identified + 3 new) |
| Files modified | **11** |
| Opened outside inspection set | **0** |
| Source files opened during planning / doctrine / impact | **0** |
| Validation | **pass** (`make b` lint+test+release; 19 tests) |
| Doctrine before | **satisfied** (workspace, 0 findings, 1 rule) |
| Doctrine after | **satisfied** (workspace, 0 findings, 1 rule) |
| Re-index duration | **169 ms** (11 files indexed, 52 seen, 341 symbols) |
| Updated graph | SHA `5bf302c`; files 49 → **52**; symbols 316 → **341** |

Identified (graph): `src/lib.rs`, `src/main.rs`, `src/architecture.rs`, `tests/architecture.rs`, `README.md`, `CLAUDE.md`, `docs/examples/agent-workflows.md`, `docs/specs/agent-architecture-api.md`.

New (not in the graph at start): `src/workflow.rs`, `tests/workflow.rs`, `docs/specs/autonomous-workflow.md`.

## Impact (bindb only)

| | Before | After |
|--|--------|--------|
| Target | `src/architecture.rs` (file) | same |
| Inspection files | `src/architecture.rs` | `src/architecture.rs` |
| `source_files_opened` | 0 | 0 |

After re-index, `plan("change src/workflow.rs")` returns `src/workflow.rs` and `tests/workflow.rs` with `source_files_opened: 0`. The new module is queryable without a source walk.

## Context reduction

Planning named **8 of 49** indexed files (16%). Crate-wide `plan("change codeintel")` would have listed all 49. File-path matching is what keeps the inspection set small. No `find` / `rg` over the tree was used to decide what to open.

## Tests

`tests/workflow.rs`:

- `start` after both Git trees are deleted (graph-only planning).
- Full loop: inspect `src/lib.rs` only, add a new file, commit, `finish`; doctrine stays satisfied; `files_new` records the new path.
- Opening `src/extra.rs` when the plan named `src/lib.rs` sets `opened_outside_inspection_set`.

`make b` / clippy `-D warnings` clean. Image/deploy skipped (`dpk.toml` / `env.sh`).

## Not in this milestone

rustc, rust-analyzer, IDE, Cursor product hook, hosted service, LLM planner.
