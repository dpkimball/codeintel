---
title: Nightly codeintel store refresh + doc-compliance (QA)
status: spec
github_issue: https://github.com/dpkimball/dpk-infra/issues/703
parents:
  - https://github.com/dpkimball/binder-workspace/issues/154
  - https://github.com/dpkimball/binder-ai/issues/33
---

# Nightly store refresh (replaces dpk-docs nightly)

## Agent briefing

Implement GitOps refresh of the QA bindb store so Cursor/`POST /plan` stay current without bumping `seed.generation` every night. Do not revive archived `dpk-docs` workflows. Do not `kubectl`/`helm` on QA.

| Rule | Do |
|---|---|
| Inventory | One list: `charts/binder-ai-serve` `seed.repos` + `seed.prefixIndexes` |
| RWO PVC | Refresh **sidecar** on the Deployment (shares `/data`). No CronJob mount while serve runs |
| Shell | POSIX `set -eu` (dash). No `pipefail` |
| Report | `doc-compliance` JSON only — no autofix PRs this issue |
| PRs | `dpkimball-cursor`; `Implements #703` until AC met |

## Problem

| Old (dpk-docs, archived) | Current QA seed | Gap |
|---|---|---|
| Nightly GHA: checkout matrix → `standards_validator.py` | Init clones ~41 repos once → `.seeded-<N>` | No daily git update / re-index / compliance report |
| Hand matrix (~7 stale names) | `seed.repos` (~41) | Inventory already better — keep it |
| Docs path only | Full tree + DocRecords | Use `codeintel architecture doc-compliance` |

## Desired behavior

```text
seed.repos / prefixIndexes
        │
        ├─ init (once per generation) → clone + index + .seeded-N
        │
        └─ sidecar (daily ~03:00 UTC)
              git fetch + reset --hard origin/HEAD (or default branch)
              codeintel index (incremental by SHA)
              optional: index-issues / index-prs / link-issues
              codeintel architecture doc-compliance → /data/doc-compliance.json
```

### Sidecar (normative for this issue)

- Container `store-refresh` in the same pod as `binder-ai-serve`
- Same image, same PVC mount, same github-token mount when enabled
- Loop: sleep until next `refresh.hourUtc` (default **3**), run refresh once, sleep ≥1h to avoid double-run
- Missing workspace dir → `git clone --depth 1` (recover partial seed); existing → fetch/reset
- Failures on one repo log and continue (`set +e` per repo or `|| true` on non-fatal steps)
- After all indexes: write compliance report; non-zero violation count is **not** a pod crash (report-only)

### Non-goals

- Bumping `seed.generation` on a schedule
- Autofix PRs into product repos
- Laptop book SSD roots in QA
- HTTP admin API / CronJob without PVC (follow-up)
- Closing binder-ai#33 or #154 from this work alone

## Acceptance criteria

Mirror [dpk-infra#703](https://github.com/dpkimball/dpk-infra/issues/703).

## Files

| Path | Change |
|---|---|
| `dpk-infra/charts/binder-ai-serve/values.yaml` | `refresh.enabled`, `hourUtc`, resources |
| `dpk-infra/charts/binder-ai-serve/values-qa.yaml` | `refresh.enabled: true` |
| `dpk-infra/charts/binder-ai-serve/templates/deployment.yaml` | sidecar + script |
| `dpk-infra/charts/binder-ai-serve/README.md` | seed vs refresh |
| `codeintel/docs/specs/nightly-store-refresh.md` | this spec |

## Verify

```bash
helm template binder-ai-serve charts/binder-ai-serve \
  -f charts/binder-ai-serve/values.yaml \
  -f charts/binder-ai-serve/values-qa.yaml | rg store-refresh|hourUtc|doc-compliance
```

After Argo sync (human merge): read-only `kubectl logs -n services -c store-refresh …`; `/data/doc-compliance.json` via describe/logs only — no exec required if script echoes summary.
