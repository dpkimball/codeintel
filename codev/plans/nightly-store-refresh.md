---
spec: ../specs/nightly-store-refresh.md
github_issue: https://github.com/dpkimball/dpk-infra/issues/703
created: 2026-08-31
---

# Plan — Nightly store refresh (#703)

## Approach

Chart-only Phase 1: `store-refresh` sidecar on `binder-ai-serve` Deployment. Shares RWO PVC. Schedule ≈ 03:00 UTC via hour poll (no supercronic in image).

## File changes (dpk-infra)

| File | Change |
|---|---|
| `charts/binder-ai-serve/values.yaml` | `refresh.enabled: false`, `hourUtc: 3`, `pollSeconds: 60`, resources |
| `charts/binder-ai-serve/values-qa.yaml` | `refresh.enabled: true` |
| `charts/binder-ai-serve/templates/deployment.yaml` | sidecar container; embed POSIX refresh script from `seed.repos` / `prefixIndexes` |
| `charts/binder-ai-serve/README.md` | Seed vs refresh section |
| `charts/binder-ai-serve/Chart.yaml` | bump patch version |

## Refresh script behavior

1. Wait until `date -u +%H` equals `hourUtc`, then run.
2. For each `seed.repos`: ensure clone; `git fetch --depth 1 origin` + `git reset --hard FETCH_HEAD` (or `origin/master`); `codeintel index`.
3. If token: `index-issues` / `index-prs` / `link-issues` with `|| true`.
4. For each `prefixIndexes`: update umbrella; re-index prefixes; refresh symlinks.
5. `codeintel architecture --store "$STORE" doc-compliance > "$STORE/doc-compliance.json"`; echo violation count.
6. Sleep 3600s before polling again.

## Out of this PR

binder-ai HTTP trigger, CronJob, autofix PRs, question-bank guidance text fix (optional small follow-up).
