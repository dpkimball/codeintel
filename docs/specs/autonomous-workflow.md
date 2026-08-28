---
title: Autonomous engineering workflow
status: poc
github_issue: https://github.com/dpkimball/codeintel/issues/11
---

# Autonomous engineering workflow

The Architecture API can plan, evaluate doctrine, and compute impact. This spec is the **execution loop** an agent runs so bindb stays the primary substrate. Source files are opened only to implement or verify.

Git remains the source of truth. Re-index reads Git HEAD, not the dirty working tree.

## Workflow

```text
1. Open the bindb store
2. Resolve the request against the semantic graph (`plan`)
3. Produce an execution plan (inspection set, doctrine rules, likely affected)
4. Identify the minimal inspection set from graph impact
5. Open only those source files (new files in matched crates are allowed)
6. Implement
7. Validate (compile/tests — reported by the agent, not invented by the graph)
8. Commit, then re-index affected repositories
9. Re-evaluate doctrine
10. Completion report from the updated graph
```

Steps 1–4 and 8–10 do not walk the repository. Step 5 is the first source open. Step 7 is verification, not discovery.

## API

```text
workflow start  --store PATH --request "..." --bind name=repo --session FILE
workflow finish --session FILE --opened path --modified path --validation pass|fail
```

`start` writes a session: planning time, repositories consulted, files identified by the graph, all indexed file paths (the comparison baseline), doctrine before, impact before.

`finish` classifies opened paths:

| Class | Meaning |
|-------|---------|
| identified | path was in `smallest_file_set` |
| new | path was not in the graph at `start` (allowed) |
| outside | path was in the graph but not in the inspection set |

Then it re-indexes each `--bind`, re-runs doctrine and impact, and emits metrics.

## Where the graph is used vs source

| Step | Substrate |
|------|-----------|
| Resolve request, plan, inspection set | bindb |
| Doctrine before / after | bindb |
| Impact before / after | bindb |
| Re-index | Git tree → bindb (no agent `find`/`rg`) |
| Open files for the patch | source, inspection set only |
| `cargo test` / compile | source / toolchain |

## Plan matching

`Architecture::plan` prefers **file paths** named in the request (multi-component suffixes such as `src/lib.rs`, or distinctive filenames). If no file matches, it falls back to **crate names**. Crate-wide impact lists every file in the crate; file-level impact is the implicated set.

## Non-goals

New parsers, rustc, rust-analyzer, IDE, Cursor product hook, hosted service, LLM planner.
