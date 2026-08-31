---
title: Plan inspect-set fidelity
description: Plan() inspect-set fidelity: repo:path, keys, and crate matching.
status: active
github_issue: https://github.com/dpkimball/codeintel/issues/13
category: specs
---
# Plan inspect-set fidelity

`Architecture::plan` is the agent inspection set. Paths must be unambiguous across repositories, crate-wide matches must not dump repo clutter, and `dpk.toml` keys must be in the graph so Docker/`extra_args` questions resolve without a source walk.

Git remains the source of truth. bindb stores dotted keys and identifier tokens (env names, Docker flags), not config blobs.

## Matching order

1. **File** — request names an indexed path (multi-component suffix or distinctive filename).
2. **Config** — `dpk.toml` keys/tokens, Helm/env names, or docker-shipping intent (`docker.extra_args`). See [plan-intent-nested](plan-intent-nested.md) (#15).
3. **Crate** — request contains the crate name as a whole token (`change` / non-intent).

`matched_kind` is `file`, `config`, `crate`, or `none`. File wins over config; config wins over crate.

## Inspect paths

`inspect_before_change` and `smallest_file_set` entries are `repo:path` (example: `binder-server:dpk.toml`). Agents may still open the unprefixed path; `workflow finish` treats `src/lib.rs` as matching `libs:src/lib.rs`.

Crate-wide inspection includes only `*.rs`, `Cargo.toml`, `dpk.toml`, `Dockerfile`, `Makefile`, and `.cargo/config.toml`. It excludes `.github/`.

## Re-index

Existing stores do not gain config objects until each repo is indexed again.

## Non-goals

Python/TS parsers, `query patterns` catalog rewrite. Helm env and nested-tree index are #15.
