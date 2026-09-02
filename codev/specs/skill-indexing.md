---
title: Skill Indexing and Discovery via plan()
description: Add index-skills command to codeintel and linked_skills to PlanReport so agents discover relevant skills from plan() output rather than needing to know skill names in advance.
category: specs
---

# Skill Indexing and Discovery via plan()

## Problem

Skills (`.cursor/skills/*/SKILL.md`, `.claude/skills/*/SKILL.md`) are invisible to the codeintel graph. `plan()` cannot surface them. Agents must know skill names in advance; if they don't, they implement from scratch what a skill already covers.

## Solution

1. `codeintel index-skills --dir <dir> --store <store> --name <namespace>` — walk one level of a skills directory, parse SKILL.md frontmatter, index `SkillRecord` objects into bindb under `code://skill/{namespace}/{name}`.
2. `PlanReport.linked_skills: Vec<SkillRef>` — top-5 skills by token overlap with the query, emitted alongside `linked_docs` and `guidance`.
3. `scripts/codeintel-index-workspace` calls `index-skills` on the workspace `.cursor/skills/` directory as part of the full index.
4. binder-ai propagates `linked_skills` through `KnowledgePlan → PlanResponse → AgentPlan` (separate PR).

## Acceptance criteria

- [ ] `codeintel index-skills --dir /Volumes/SSD/dpk-workspace/.cursor/skills --store codeintel/store --name dpk-workspace` indexes all SKILL.md files with a valid `name:` frontmatter field
- [ ] `scripts/binder-plan "run the issue pipeline"` returns `linked_skills: [{name: "issue-agent-pipeline", ...}]`
- [ ] `scripts/binder-plan "query codeintel store"` returns `linked_skills: [{name: "codeintel-query", ...}]`
- [ ] `scripts/binder-plan "deploy and fix"` returns `linked_skills: [{name: "fix-deploy-test-verify", ...}]`
- [ ] Skills with no token overlap do not appear
- [ ] `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check` all pass

## Key constraints

- No network calls in `index-skills` — reads filesystem only
- Skills are stored under `code://skill/{namespace}/{name}` — namespace avoids collisions between workspace-level and repo-level skill sets
- Token threshold for `linked_skills` is ≥ 1 overlap (skills are specific enough that any overlap is meaningful)
- `linked_skills` is omitted from JSON when empty (`skip_serializing_if = "Vec::is_empty"`)
