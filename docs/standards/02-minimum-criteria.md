---
title: Minimum Criteria for Canonical Documentation
description: What a document must satisfy to be canonical documentation rather than a workflow artifact
category: architecture
use_cases: [understanding-architecture]
---

# Minimum Criteria for Canonical Documentation

## Core Principle

**Canonical documentation** describes "how to" or "what is."  
**Workflow artifacts** describe "what needs to be done."

## Minimum Criteria

A document must meet **ALL** of these to be canonical:

### 1. Purpose: Descriptive, Not Prescriptive

✅ **Legitimate**: Describes how something works, how to use it, what it is
- "How to deploy the service"
- "Architecture overview"
- "API reference"

❌ **Workflow artifact**: Describes what needs to be done
- "TODO: implement feature X"
- "Status: 50% complete"
- "Fix needed for bug Y"

### 2. Frontmatter

Required fields: `title`, `description`, `category`

```yaml
---
title: Deployment Guide
description: Step-by-step guide for deploying the service
category: deployment
---
```

### 3. Naming

- ✅ `01-deployment-guide.md` (kebab-case, numbered prefix)
- ❌ `todo.md`, `status.md`, `notes.md` (workflow-suggesting names)
- ❌ `deployment_guide.md` (underscores)

Files with workflow-suggesting names (todo, status, summary, notes) should be reviewed even if content is legitimate — rename them.

### 4. Structure

- H1 title at the beginning
- Proper heading hierarchy (no skipped levels)
- Substantive content (not just checkboxes or status markers)

### 5. Content Quality

**Legitimate documentation**:
- Explains concepts, procedures, or systems
- Uses declarative language ("The system does X")

**Workflow artifacts**:
- Lists tasks to complete, tracks progress or status
- Uses imperative language ("Need to do X", "Should implement Y")
- High ratio of unchecked checkboxes

## Detection Signals

| Signal | Strength |
|--------|---------|
| Workflow-suggesting filename | Strong |
| High ratio of unchecked checkboxes (>5, mostly unchecked) | Strong |
| Status/progress markers (status: pending, progress: X%) | Medium |
| Temporal/event language (Last updated, Fixed, Resolved) | Medium |
| Task language without how-to context | Weak |

## Enforcement

codeintel runs structural checks at index time. Use `codeintel architecture doc-compliance --store <path>` to view violations across all indexed repos.

Violations detected:
- Missing frontmatter block
- Missing title, description, or category
- Category not in standard set
- No H1 heading
- Heading level jump (e.g. H1 → H3)
- Code blocks without language specifier

## Examples

### ✅ Legitimate Documentation

```markdown
---
title: Deployment Guide
description: Step-by-step guide for deploying the service
category: deployment
---
# Deployment Guide

This guide explains how to deploy the service to production.

## Prerequisites

- Kubernetes cluster
- Helm installed
```

### ❌ Workflow Artifact

```markdown
# TODO: Deployment Improvements

- [ ] Add health checks
- [ ] Configure auto-scaling

Status: In progress
Last updated: 2026-01-26
```

### ⚠️ Ambiguous Case

A file with a workflow-suggesting name (`status.md`) that contains a legitimate architecture diagram — **rename it**, don't move it:

`status.md` → `01-chat-flow-sequence.md`
