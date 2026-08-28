---
title: Plan intent, Helm env, nested index
status: active
github_issue: https://github.com/dpkimball/codeintel/issues/15
---

# Plan intent, Helm env, nested index

Issue #13 shipped `repo:path` inspect sets and `dpk.toml` keys. Agents still got an empty set for Docker-shipping English, a crate dump when a crate name appeared in a question, no `OLLAMA_HOST`, and no way to index a monorepo subdirectory such as `repo-dashboard`.

Git remains the source of truth. bindb stores keys and identifier tokens, not blobs.

## Matching order

1. **File** — request names an indexed path.
2. **Config** — `dpk.toml` keys/tokens, Helm/env names (`env.OLLAMA_HOST`), **or** docker-shipping intent on `docker.extra_args`.
3. **Crate** — whole-token crate name, only when config did not match.

Docker-shipping intent is true when the request contains `docker` / `dockerfile` / `extra_args` / `build-context` / `build-arg` / `kellnr`, **or** (`ship`/`shipping` and `binary`/`rust`). Intent matching does not use sibling crate names inside `extra_args` (`binder-core=../binder-core`), so `change binder-core` stays crate-scoped.

When intent matches `docker.extra_args`, the inspect set is those `dpk.toml` files plus a sibling `Dockerfile` when that file is in the graph.

## Nested git trees

`codeintel index ./repo-dashboard --name repo-dashboard` may be a subdirectory of a larger git repository. The indexer uses `git rev-parse --show-toplevel`, then **only** blobs under that subdirectory, with stored paths relative to it. Sibling directories are not indexed.

## Helm / env

`values.yaml`, `values-*.yaml`, `templates/*`, and Dockerfiles contribute `env.{NAME}` objects for uppercase identifier tokens (length ≥ 8), including `OLLAMA_HOST`.

## Re-index

Existing stores need a new index (and a nested index of workspace apps) before these objects exist.

## Non-goals

Python/TS symbol parsers, `query patterns` catalog rewrite, LLM planner.
