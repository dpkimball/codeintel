---
title: Agent-first architectural API
description: Stable agent-facing Architecture API over bindb (plan, doctrine, impact, drift, federation).
status: poc
github_issue: https://github.com/dpkimball/codeintel/issues/9
category: specs
---
# Agent-first architectural reasoning

Indexing, doctrine, impact, drift, and federation already write and read bindb. This spec is the **stable agent surface**. Agents call semantic operations. They do not walk the repository and they do not depend on bindb object-id layout.

Git remains the source of truth. bindb is architectural knowledge. Source files are opened only from inspection paths the graph returns.

## API

```text
Architecture::open(store_path)
architecture.summary()
architecture.doctrine()
architecture.impact(target)
architecture.dependencies(target)
architecture.cycles()
architecture.layering()
architecture.drift(from_sha, to_sha)
architecture.federation()
architecture.plan(request)
architecture.close()
```

`open` takes a filesystem path to an existing store. That is the only storage detail. Return types are JSON-serializable views with `primary_source: "bindb"` and `source_files_opened: 0`.

CLI (agent JSON, not a human dashboard):

```bash
codeintel architecture --store ./store summary
codeintel architecture --store ./store plan --request "change binder-storage"
```

## Execution model

1. Index each Git repo into the same store (`codeintel index`).
2. Agent calls `Architecture::open`.
3. Agent calls `plan` / `impact` / `doctrine` / `federation` as needed.
4. Agent opens source **only** at `inspect_before_change` / `smallest_file_set` paths.
5. After edits, `workflow finish` (re-index if HEAD moved, then doctrine).

File paths in the request win, then config keys/tokens and docker-shipping intent, then crate names as whole tokens. Inspect paths are `repo:path`. Nested git subdirectories index as their own `--name`. See [plan-inspect-sets](plan-inspect-sets.md), [plan-intent-nested](plan-intent-nested.md), and [autonomous-workflow](autonomous-workflow.md).

## Non-goals

rustc, rust-analyzer, IDE, Cursor product hook, hosted service, UI.
