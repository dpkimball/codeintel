---
title: DPK Documentation Standards
description: Rules for all documentation across DPK platform repos — structure, frontmatter, categories, naming, formatting
category: architecture
use_cases: [understanding-architecture, development-workflow]
---

# DPK Documentation Standards

**Applies To**: All DPK platform repositories

## Table of Contents

1. [File Structure](#file-structure)
2. [Categories](#categories)
3. [Frontmatter](#frontmatter)
4. [Content Organization](#content-organization)
5. [Naming Conventions](#naming-conventions)
6. [Formatting Standards](#formatting-standards)
7. [CLAUDE.md Standard](#claudemd-standard)
8. [Templates](#templates)

---

## File Structure

### Directory Organization

```text
repo-name/
├── README.md                    # Main repository README
├── CLAUDE.md                    # Claude-specific instructions
├── AGENTS.md                    # AI agent instructions (if applicable)
├── docs/                        # Documentation directory
│   ├── setup/
│   ├── deployment/
│   ├── development/
│   ├── architecture/
│   ├── api/
│   └── troubleshooting/
└── [code files]
```

### File Naming

**Format**: `NN-description.md` (numbered prefix for ordering)

- ✅ `01-deployment-overview.md`
- ✅ `02-development-setup.md`
- ❌ `Deployment Overview.md`
- ❌ `deployment_overview.md`

**Exempt root-level files** (no numbered prefix required):
- `README.md`, `CLAUDE.md`, `AGENTS.md`, `INDEX.md`, `LICENSE.md`

---

## Categories

All documentation must use a **standard category**:

| Category | Use for |
|----------|---------|
| `deployment` | Deployment procedures and guides |
| `development` | Development setup and workflows |
| `architecture` | System architecture and design |
| `configuration` | Configuration guides |
| `troubleshooting` | Problem-solving guides |
| `tasks` | Task-specific documentation |
| `vision` | Vision and planning |
| `specs` | Specifications |
| `plans` | Implementation plans |
| `reviews` | Reviews and retrospectives |
| `api` | API documentation |
| `setup` | Setup guides |
| `usage` | Usage guides |
| `examples` | Code examples |

codeintel validates categories at index time; unlisted values appear in `doc_compliance()` output.

---

## Frontmatter

### Required Fields

```yaml
---
title: Document Title
description: One-sentence description of what this document covers
category: deployment
---
```

### Recommended Fields

```yaml
---
title: Document Title
description: One-sentence description
category: deployment
tags: [kubernetes, helm]
use_cases: [deploying-to-qa, deploying-to-production]
related:
  - ../development/02-development-setup.md
last_updated: 2026-01-25
---
```

### Standard Use Cases

- `setting-up-dev`
- `deploying-to-qa`
- `deploying-to-production`
- `troubleshooting`
- `configuring-services`
- `understanding-architecture`
- `api-integration`
- `development-workflow`

---

## Content Organization

### Document Structure

```text
# Title

Brief introduction (1–2 sentences)

## Prerequisites

- Requirement 1

## Overview

Main content introduction

## Main Sections

Content...

## Examples

Code examples

## Troubleshooting

Common issues

## Related Documentation

- [Link](path/to/doc.md)
```

### Heading Hierarchy

- **H1** (`#`) — Document title (one per file)
- **H2** (`##`) — Major sections
- **H3** (`###`) — Subsections
- Do not skip levels (H1 → H3 is a violation)

---

## Naming Conventions

### Document Titles

- ✅ "Deployment Overview"
- ✅ "Setting Up Development Environment"
- ❌ "Docs", "Stuff", "README"

### Section Headings

- ✅ "Prerequisites", "Installation Steps", "Configuration Options"
- ❌ "Stuff You Need", "How To", "Problems"

---

## Formatting Standards

### Code Blocks

Always specify a language:

```bash
kubectl get pods -n dev
```

```python
def example():
    return "example"
```

```yaml
key: value
```

### Warnings and Notes

```markdown
⚠️ **Warning**: Important warning text
💡 **Note**: Helpful note text
✅ **Tip**: Useful tip text
❌ **Don't**: What not to do
```

---

## CLAUDE.md Standard

Every repo must have a `CLAUDE.md` with exactly five sections:

1. **What this is** — one-paragraph summary of purpose
2. **What it is NOT** — explicit routing boundaries for codeintel
3. **Layout** — directory tree with annotations
4. **Build** — exact commands to build, test, lint
5. **Key constraints** — invariants agents must not violate

Target: 80 lines. No setup tutorials or prose. The "What it is NOT" section is the most valuable — it tells codeintel where NOT to route changes.

---

## Templates

### Setup Guide

```yaml
---
title: Setting Up [Service Name]
description: Complete guide for setting up [service] in [environment]
category: setup
tags: [service-name, setup]
use_cases: [setting-up-dev]
last_updated: YYYY-MM-DD
---
```

```markdown
# Setting Up [Service Name]

Brief introduction.

## Prerequisites

- Requirement 1

## Installation

### Step 1: [Action]

Description and commands.

## Verification

    # Verification command

## Troubleshooting

### Common Issue 1

**Problem**: Description  
**Solution**: Solution steps
```

### API Documentation

```yaml
---
title: [API Name] API Reference
description: Complete API reference for [service]
category: api
tags: [api, service-name]
use_cases: [api-integration]
last_updated: YYYY-MM-DD
---
```

### Troubleshooting Guide

```yaml
---
title: Troubleshooting [Topic]
description: Common issues and solutions for [topic]
category: troubleshooting
tags: [troubleshooting, topic]
use_cases: [troubleshooting]
last_updated: YYYY-MM-DD
---
```

---

## Quality Checklist

Before committing documentation:

- [ ] Follows standard document structure
- [ ] Has clear H1 title with no skipped heading levels
- [ ] Frontmatter: title, description, category present
- [ ] Category is from the standard list
- [ ] Code blocks specify language
- [ ] Links use relative paths and are valid
- [ ] Examples are tested and work

Run `codeintel architecture doc-compliance --store <path>` to check all indexed docs.

---

**Status**: Active Standard
