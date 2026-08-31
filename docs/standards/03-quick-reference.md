---
title: Documentation Standards - Quick Reference
description: Fast lookup card for DPK documentation standards — naming, categories, frontmatter, structure
category: architecture
use_cases: [understanding-architecture, development-workflow]
---

# Documentation Standards — Quick Reference

## File Naming

✅ `01-deployment-overview.md`  
❌ `Deployment Overview.md`, `deployment_overview.md`

**Format**: `NN-description.md` (numbered, kebab-case)  
**Exempt**: `README.md`, `CLAUDE.md`, `AGENTS.md`, `INDEX.md`, `LICENSE.md`

---

## Categories

`deployment` · `development` · `architecture` · `configuration` · `troubleshooting` · `tasks` · `vision` · `specs` · `plans` · `reviews` · `api` · `setup` · `usage` · `examples`

---

## Frontmatter (Required)

```yaml
---
title: Document Title
description: Brief description
category: deployment
---
```

---

## Document Structure

```text
# Title

Introduction

## Prerequisites

## Overview

## Main Sections

## Examples

## Troubleshooting

## Related Documentation
```

---

## CLAUDE.md (5 sections, ~80 lines)

1. What this is
2. What it is NOT
3. Layout
4. Build
5. Key constraints

---

## Heading Rules

- One H1 per file
- No skipped levels (H1 → H2 → H3, not H1 → H3)

---

## Code Blocks

Always name the language:

```bash
kubectl get pods -n dev
```

---

## Validation

```bash
codeintel architecture doc-compliance --store <path>
```

---

**Full guide**: [01-documentation-standards.md](01-documentation-standards.md)  
**Minimum criteria**: [02-minimum-criteria.md](02-minimum-criteria.md)
