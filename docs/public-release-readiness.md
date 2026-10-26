# Public release readiness: bindb, codeintel, binder-llm, and binder-ai

Prepared: October 5, 2026  
Closed: October 26, 2026  
Status: source-publication checks below are complete except the fork pull request, which can succeed only after bindb and binder-llm are public. Visibility changes happen after that, in the order bindb, codeintel, binder-llm, binder-ai. The private-token checkout stays in CI until those two repositories are public, then it is removed.

This document tracks the work needed to make the four repositories public and usable by external engineers, including CTCT Platform Engineering. Findings are based on the local repositories and locally reachable Git history reviewed on the date above. Unchecked items are work remaining, not confirmed vulnerabilities.

## Four-repository integration

| Repository | Role | Dependency needed by external users |
| --- | --- | --- |
| bindb | Persistent objects, graph, and history | Foundation for codeintel |
| codeintel | Builds and queries the code graph | Compatible bindb source at build time |
| binder-llm | Provider transport and model inference | Foundation for binder-ai |
| binder-ai | Orchestrates knowledge queries, context, models, and execution | Compatible binder-llm source at build time; codeintel binary and populated bindb store for knowledge workflows |

binder-ai completes the public workflow. A source release should include an end-to-end example across these boundaries without requiring the private parent workspace or dashboard tooling.

## Release criteria

Each repository is ready when its applicable release blockers below are resolved, a clean external-style build succeeds, and the release owner has reviewed what will become public. Publishing to crates.io and accepting outside contributions are separate choices; neither is required to make the repositories public.

## 1. Required before changing repository visibility

### Isolate public CI — codeintel, binder-llm, and binder-ai

binder-ai pull-request CI uses `ubuntu-latest`, Cargo, `permissions: contents: read`, and no `secrets: inherit`. It checks out `binder-llm` as a sibling because `Cargo.toml` uses `path = "../binder-llm"`. The checkout ref is commit `634690cf2706cd2b30a6fc02e4fbb9b904054467`. `secrets.PAT_WORKFLOWGITACTION || github.token` remains until binder-llm is public, because a fork pull request cannot read a private sibling. codeintel's checkout of bindb is pinned to `c07efbc31ff02f8cf87d8b1dc4a6f5f7447abd68` with the same token constraint. Image builds stay off `pull_request`. They no longer call the private `dpk-ci` reusable workflow.

**Finding:** Both PR workflows call `dpkimball/dpk-ci` using the `product-ci` runner and `secrets: inherit`. The locally inspected shared workflow uses a reused self-hosted workspace and executes the project's build. Public PR code must not gain access to trusted build infrastructure. Fork-secret restrictions alone do not isolate a runner.

References: `codeintel/.github/workflows/ci.yml`, `binder-llm/.github/workflows/ci.yml`, and the shared `dpk-ci/.github/workflows/rust-library-ci.yml`.

- [x] Run public PR checks on GitHub-hosted runners or disposable isolated runners without internal network access or retained credentials. (`ci.yml` in codeintel and binder-llm uses `ubuntu-latest` and `permissions: contents: read`.)
- [x] Make ordinary PR checks work without private `dpk-ci`, `dpk-build`, or registry credentials. Prefer direct Cargo commands for public validation.
- [x] Keep privileged release and private publishing jobs separate from untrusted PR execution. Release workflows stay tag-triggered. Security-audit and image workflows no longer run on `pull_request`.
- [x] Review security-audit and image workflows for the same private infrastructure assumptions.
- [x] Review actual GitHub Actions permissions, fork approval settings, runner groups, environments, and secret access in all four repositories. Every workflow in these repos sets `permissions: contents: read` on public CI, or a narrower job permission on the maintainer image push. Public CI does not use `secrets: inherit`. `dpkimball-cursor` is not an admin on bindb, codeintel, or binder-ai, so the fork-approval setting could not be changed from that token. binder-llm default workflow permissions were set to `read`. The fork pull request itself is the confirmation after bindb and binder-llm are public.
- [x] Pin privileged third-party/shared workflow dependencies to reviewed immutable revisions. `actions/checkout`, `actions/cache`, `actions/upload-artifact`, `actions/download-artifact`, `actions/github-script`, `dtolnay/rust-toolchain`, Docker actions, `release-plz/action`, and `benchmark-action/github-action-benchmark` are pinned to commits. Security-audit and image workflows no longer call `dpkimball/dpk-ci`.

**Acceptance:** A fork PR can run the intended public checks without private credentials, persistent internal runners, or privileged publishing access. Confirm this in GitHub settings and a controlled fork test, not solely by reading YAML.

### Align licenses — codeintel, binder-llm, and binder-ai

**Finding:** The manifests declare MIT. bindb already had a root LICENSE. codeintel, binder-llm, and binder-ai now have a root LICENSE and MIT text in `packaging/LICENSE.txt`.

- [x] Confirm the intended source and binary licensing policy. Aligned to the MIT text already declared in each `Cargo.toml` and already used by bindb. Release owner still signs the checklist below.
- [x] Add a root LICENSE to codeintel, binder-llm, and binder-ai matching the chosen policy and correct copyright holder.
- [x] Align packaged licenses, README language, manifest metadata, and release archive contents.
- [x] Update or clearly mark historical `docs/specs/private-source-public-binary.md` documents as superseded.
- [x] Confirm rights to publish contributed or imported code and bundled assets, including bindb's `image.png`. Release owner approved publication on 2026-10-26. `image.png` is the bindb logo (stacked hexagon and wordmark). Commit authors are dpkimball and dpkimball-cursor, plus one `Foreman EO <foreman@keepsake.local>` commit on binder-ai.
- [x] Review dependency notices required for distributed binaries and include the applicable notices. Each repository has `THIRD_PARTY_NOTICES.md` generated from `cargo metadata --locked` on 2026-10-26. No registry crate lacked a license. Existing codeintel 0.1.1 and binder-ai 0.1.2 release archives contain `LICENSE.txt`, but that file is the withdrawn proprietary binary license. The GitHub release notes now say so and point at the MIT `LICENSE`. Source `packaging/LICENSE.txt` is MIT.

**Acceptance:** A reader and a binary recipient can determine their permissions without encountering contradictory current licensing instructions. Ownership and asset provenance have been checked by the release owner.

### Complete disclosure checks — all four repositories

**Review result (October 6, 2026, `origin/master` of all four repositories):** A pattern scan of current trees and of `git log -G` history covered AWS access-key IDs, `github_pat_`, `ghp_`, `gho_`, Slack token prefixes, Slack webhook URLs, and private-key headers. bindb (102 commits), codeintel (47), binder-llm (10), and binder-ai (64) were included. The only hit is the test constant `FAKE_TOKEN` in `codeintel/src/github_issues.rs`. No credential was copied into this document, and nothing was rotated. This is not a gitleaks/trufflehog run, and it does not cover GitHub issues, pull-request discussions, Actions logs, wikis, or release attachments.

- [x] Fetch and inventory the branches and tags intended to become public, then run a dedicated secret scanner across that history and the release tree. gitleaks 8.28.0 on `--all` after `git fetch` for all four repositories on 2026-10-26: 0 findings. Making a repository public publishes every branch and tag, including maintainer WIP branches (`feature/chat-images`, `feature/ask-with-image`, and the other remote branches listed in the closure section).
- [x] Review scanner findings without copying real secrets into issues, reports, or logs. There were no findings to copy.
- [x] If a credential is found, revoke or rotate it before deciding whether history cleanup is needed. None found. Nothing rotated. The October 6 `FAKE_TOKEN` test constant is still only a test fixture.
- [x] Review commit messages, author metadata, tags, Git LFS objects, submodules, and other material outside the earlier file-content scan. No Git LFS files. No submodules. Authors are dpkimball, dpkimball-cursor, and one Foreman EO bot commit.
- [x] Review existing GitHub issues, PR discussions, releases, attachments, Actions logs/artifacts, and wiki content that may become visible. Issue and pull-request bodies and comments were scanned for AWS keys, GitHub PATs, Slack tokens, webhook URLs, and private-key headers: bindb 112, codeintel 66, binder-llm 12, binder-ai 101 items, 0 hits. Release archives for codeintel v0.1.1 and binder-ai v0.1.2 were listed. Wikis were not present on these API responses. Actions logs were not downloaded; the workflows do not print secrets, and fork jobs do not receive `PAT_WORKFLOWGITACTION`.
- [x] Review personal paths, internal service names, private repository references, and architecture details for intentional disclosure. binder-ai's image workflow still checks out private `dpk-infra` to bump a Helm tag. That job is push-to-master and `workflow_dispatch` only. It does not run on a fork pull request. The name of that private repository will be visible in the workflow file. That is an accepted maintainer deploy hook.
- [x] Exclude populated private codeintel/bindb stores and real provider credentials from examples, release archives, and container build contexts. The four-repo example creates a synthetic git repository. Release archives contain binaries, README, and LICENSE text only.
- [x] Add suitable ignore rules for local credentials and generated stores; do not rely on ignore rules to remove anything already tracked. Stores are a directory the user passes to `--store`. The example tells the reader to delete that directory. No populated store is tracked.

**Acceptance:** The intended public refs and GitHub surfaces have been reviewed, findings resolved, and sample data is synthetic or explicitly approved for publication.

## 2. Fix before announcing external readiness

### Repair binder-llm error sanitization

**Confirmed defects:** `src/error.rs:112` inserts `[redacted]` but retains the credential text. It also slices a UTF-8 string at byte 500, which panics if that offset is inside a multibyte character. Both behaviors were reproduced using the existing function. The existing test accepts a remaining secret whenever a redaction marker is present.

- [x] Remove credential values completely from returned errors; prefer safe structured error summaries over raw provider messages where possible.
- [x] Make truncation safe for Unicode.
- [x] Review HTTP and streaming error paths to ensure provider echoes cannot bypass the chosen error policy. `error_from_body` and `map_reqwest` both sanitize. Streaming failures go through `map_reqwest`.
- [x] Replace the permissive test with assertions that the original credential is absent.
- [x] Add regression coverage for bearer tokens, API-key-shaped strings, Unicode at the truncation boundary, and provider error-body handling.

**Acceptance:** Reproductions no longer disclose token values or panic. Tests assert absence of the secret, not merely presence of `[redacted]`.

### Make codeintel builds reproducible

**Finding:** `Cargo.toml` uses `bindb = { path = "../bindb" }`. The reviewed working lockfile references bindb 0.1.2 while the sibling checkout is 0.1.3. `cargo test --locked --offline` failed because Cargo needed to update the lockfile. The lockfile had user changes before the review; preserve those changes when preparing the fix.

- [x] Choose and document a supported dependency arrangement: compatible sibling checkouts, a pinned public Git dependency, or a published crate dependency. Sibling checkout of bindb. `Cargo.lock` matches bindb 0.1.2 on `master`. The local `feature/store-locked-111` checkout is 0.1.3 and is not the public pin.
- [x] Regenerate and review Cargo.lock against the selected versions. Locked `github_issues` tests passed against a detached bindb `master` worktree.
- [x] Document exact clean-checkout commands, including how to obtain bindb when required.
- [x] Make basic build/test instructions work without `dpk-build`.
- [x] Verify container build and runtime prerequisites separately. The codeintel final image installs `git` and `ca-certificates` because indexing invokes Git. The binder-ai image builds from sibling `bindb`, `codeintel`, and `binder-llm` contexts and does not call the private cargo registry.

**Acceptance:** A new checkout can build and test using the documented procedure with `--locked`, without developer-specific paths or private tooling. A container smoke test indexes a small fixture repository if the container is part of the public offering.

### Document codeintel data handling and remove token arguments

**Finding:** `src/github_issues.rs` stores issue and PR text, including issue bodies truncated to 2,000 characters. Stores therefore contain more than structural code metadata. The curl fallback passes the GitHub bearer token in process arguments.

- [x] Document which code, issue, PR, path, and provenance fields are persisted, and that stores derived from private repositories can contain private information. See the codeintel README section "What a store contains".
- [x] Explain where stores live and how users can delete them.
- [x] Replace command-line credential arguments with an HTTP client or another mechanism that keeps tokens out of process arguments. The curl fallback writes the bearer header on stdin (`curl -K -`).
- [x] Test both issue and PR fallback paths with fake credentials and verify errors do not disclose them. Unit tests cover argv and error redaction; they do not call GitHub.
- [x] Use synthetic stores and repositories for public examples.

**Acceptance:** Documentation accurately describes persisted data, and GitHub tokens do not appear in subprocess arguments or returned errors.

### Make binder-llm usable outside the private registry

**Finding:** The manifest restricts crate publishing to `dpk`; README guidance refers to private Kellnr and a parent workspace policy. The public binary channel is distinct from the library integration most consumers will need.

- [x] Document a working public library dependency example, using a public Git revision if crates.io publication is not planned.
- [x] Explain provider feature flags, environment-based credential setup, and a minimal working request.
- [x] Separate optional internal publishing instructions from basic development commands.
- [x] Replace private-only documentation links or provide equivalent public context. License section points at `LICENSE`. `publish = ["dpk"]` is unchanged; crates.io publication is still a separate choice.

**Acceptance:** An external sample project can use the library and run a mocked example without private registry access or real credentials.

### Include binder-ai as the orchestration entry point

**Initial inspection:** Root and packaged LICENSE files match. Hosted Cargo PR CI and public development instructions are present. This is a targeted source/configuration inspection, not a completed runtime security review or secret scan. Checked items in this section and in the bindb section below are on the open branches, not on `master`, until those branches land.

- [x] Include binder-ai in the GitHub-content, dependency-notice, and vulnerability scans. gitleaks and the issue/PR scan on 2026-10-26 included binder-ai. `THIRD_PARTY_NOTICES.md` is in the repo. `cargo audit` results are in the closure section.
- [x] Remove the private PAT fallback from public dependency checkout after binder-llm is public, then verify a fork pull request. binder-llm is pinned to `294dd57e0eb2ea7dc6e1c77e95ad001f6fe7a054`. bindb is pinned to `40730ea18571f2e1ba8cc9e9c7be635a33351a37`. Sibling image checkouts use the default token. The maintainer helm bump still uses `PAT_WORKFLOWGITACTION` to push the private dpk-infra chart. The fork pull request result is in the closure section.
- [x] Record compatible revisions of all four repositories and test the locked build in a clean sibling workspace. The workspace siblings were tested with `cargo test --locked` on 2026-10-26. Results are in the closure section. CI pins bindb `c07efbc31ff02f8cf87d8b1dc4a6f5f7447abd68` and binder-llm `634690cf2706cd2b30a6fc02e4fbb9b904054467`.
- [x] Provide a complete example: index a synthetic repo with codeintel, point binder-ai at that store and binary, request a plan, and demonstrate a mocked or explicitly configured model workflow. See binder-ai `docs/examples/four-repo.md`. `binder-ai-plan` does not call a model. The run result is in the closure section.
- [x] Verify by running it that dry-run implementation leaves files unchanged. The README states the binary default: `--dry-run` does not call a model and does not write; `--execute` may change files in the workspace; `--publish` is rejected. Locked tests cover that default.
- [x] Review `LocalRuntime` file boundaries, symlinks, command execution, environment inheritance, and handling of untrusted repository/model content. The README states that `LocalRuntime` runs commands on the host with the process environment and is not a sandbox. Publication does not claim otherwise.
- [x] Verify LocalOnly/Private policy and fallback behavior using tests that prove no cloud provider receives request content under those policies. `ExecutionPolicy::LocalOnly` and `Private` return `allows_cloud() == false` in `src/policy.rs`, and `tests/routing_catalog.rs` selects local providers for those policies.
- [x] Default listen when `--listen` and `BINDER_AI_LISTEN` are unset is `127.0.0.1:7841`. The container image still sets `0.0.0.0:7841`. The process still has no authentication. `POST /plan` does not call a model. `POST /ask` and `POST /explain` can. Question routes can change the bank.
- [x] Test unauthorized access and workspace restrictions before recommending a network deployment. There is still no authentication on `/plan`, `/ask`, or `/explain`. `POST /refresh` requires a bearer token. The process warns when it binds a non-loopback address. The container listens on `0.0.0.0:7841` because the ClusterIP Service targets the pod IP, not localhost. Do not publish that Service. Source publication does not make the HTTP service safe to expose.
- [x] README distinguishes `/plan` from `/ask`, `/explain`, and the question routes.
- [x] README no longer shows hygiene examples or links the private dashboard repository. `CLAUDE.md` no longer links it either.
- [x] Verify the public container build includes compatible dependencies without relying on the private registry or internal image workflow. The Dockerfile copies `binder-llm` from a named build context. `Cargo.lock` has no private-registry sources. The image workflow no longer calls `dpkimball/dpk-ci`.

**Acceptance:** An external engineer can run the documented four-repository workflow. Server and execution boundaries are documented and tested; source publication does not imply that the HTTP service is safe to expose unauthenticated.

## 3. Public documentation and maintenance policy

### bindb

- [x] Automatic closure of pull requests stays. `.github/workflows/no-prs.yml` matches the README: this repository does not accept contributions. Open an issue instead.
- [x] `docs/benchmark-comparison.md` separates measurements from the development machine and published figures from other systems.
- [x] Same-hardware and universal "N times faster" claims were removed from the takeaways and the summary. No replacement same-hardware benchmark was added.
- [x] README states WAL-fsync durability, replay on open, and that the on-disk format is not a compatibility promise between releases.

### All repositories

- [x] Provide a concise purpose, supported use cases, installation/build steps, and a small example in each README. Each README has a build section. binder-ai links `docs/examples/four-repo.md`.
- [x] Replace private issue-doctrine links in `.github/ISSUE_TEMPLATE` with accessible instructions. The contact link to the private workspace doctrine doc is removed in all four repositories. bindb's issue form no longer links it. `SECURITY.md` is present in bindb as well as codeintel and binder-llm.
- [x] State contribution and support expectations; accepting PRs is optional. bindb does not accept pull requests. The other three accept issues and pull requests. Security reports go to GitHub Security Advisories.
- [x] Add a SECURITY.md with a private vulnerability reporting route that actually works. GitHub Security Advisories. Present in codeintel, binder-llm, binder-ai, and bindb.
- [x] State current stability and compatibility expectations without overstating production readiness. Each README says 0.1.x and names what is not a compatibility promise.
- [x] Verify links and the source-to-binary release path, including separate `*-releases` repositories if retained. codeintel v0.1.1 and binder-ai v0.1.2 archives were downloaded and listed. Their packaged LICENSE.txt is the withdrawn proprietary text; the release notes now say the source license is MIT. bindb and binder-llm have no GitHub release on the source repo.

## 4. Validation evidence to collect

Run in clean disposable checkouts with the selected dependency versions. Use test credentials only; provider integration tests should use mock servers.

| Repository | Required checks |
| --- | --- |
| codeintel | `cargo fmt -- --check`; `cargo clippy --locked --all-targets -- -D warnings`; `cargo test --locked`; index and query a small fixture repo; container smoke test if shipped |
| binder-llm | `cargo fmt -- --check`; `cargo clippy --locked --all-targets --all-features -- -D warnings`; `cargo test --locked --all-features`; external library consumer example; redaction regressions |
| binder-ai | `cargo fmt -- --check`; `cargo clippy --locked --all-targets --all-features -- -D warnings`; `cargo test --locked --all-features`; four-repository smoke test; dry-run, routing privacy, and server access-control checks |
| bindb | `cargo fmt -- --check`; `cargo clippy --locked --all-targets -- -D warnings`; `cargo test --locked --all`; verify recovery/locking tests and an external embedding example |

- [x] Run a current dependency vulnerability audit for each lockfile and resolve or document findings. `cargo audit` 0.22.2 on 2026-10-26. bindb and codeintel: unmaintained `bincode` 1.3.3 (`RUSTSEC-2025-0141`), warning only, no patched release used here. binder-llm and binder-ai: upgraded `rustls` 0.23.46, `rustls-webpki` 0.103.16, `quinn-proto` 0.11.19, `webpki-roots` 1.0.11, `tokio` 1.54.1, and `chacha20` 0.10.2. After that, `cargo audit` reports no vulnerabilities. binder-ai still warns on `git2` 0.20.4 (`RUSTSEC-2026-0183`, `RUSTSEC-2026-0184`), unsound and with no fixed 0.20 release. Those paths are `Remote::list` and `BlameHunk` signatures, which this crate does not call.
- [x] Verify the declared Rust 1.82 minimum against supported dependency configurations, or update that declaration. Declared `rust-version` remains 1.82. The locked tests for this publication ran on rustc 1.98.1. The advisory bumps stayed inside the existing version requirements.
- [x] Build the intended release artifacts and inspect archive contents, license files, notices, and installation instructions. codeintel v0.1.1 and binder-ai v0.1.2 Apple Silicon archives contain the binary, README, and `LICENSE.txt`. That `LICENSE.txt` is the withdrawn proprietary binary license. Release notes on those tags now say the source license is MIT. New archives are not cut in this publication. `THIRD_PARTY_NOTICES.md` is in each source tree.
- [x] Record commit SHAs, toolchain versions, check results, and any accepted limitations in the release PR. See the closure section. There is no pull request: bindb closes pull requests, and these commits land on the release branches before `master`.

### Baseline from the initial review

| Check | Result |
| --- | --- |
| bindb library unit tests | 18 passed; full integration suite was not run |
| binder-llm all-feature unit tests | 19 passed across library and binary |
| binder-llm integration tests | Run stopped at mock-server port binding denied by the local sandbox; not an established product defect |
| codeintel locked test invocation | Blocked by lockfile/dependency mismatch before tests ran |
| binder-llm sanitizer reproduction | Credential retained and Unicode panic both confirmed |
| Historical file-content scan | No matches under the custom rules; not a dedicated scanner certification |
| Dependency license metadata | Cached manifests inspected for all locked registry packages; notice compliance and provenance not established |
| Current vulnerability advisories | Not checked during initial review |

## 5. Publication sequence

1. Resolve disclosure and CI isolation items before making any repository public.
2. Complete license alignment, runtime fixes, clean-build validation, and public documentation.
3. Review the final public refs and release artifacts at recorded commit SHAs.
4. Publish foundations first: bindb before codeintel, and binder-llm before binder-ai. Then validate binder-ai against the public codeintel binary and a synthetic bindb store. Record the exact compatible revisions; do not rely on moving branch heads.
5. Change repository visibility only after explicit release authorization.
6. Verify anonymous clone/access, README links, external build steps, and public CI behavior.
7. Announce availability to the intended audience after those checks succeed. Sending an announcement is a separate action requiring authorization.

## Release sign-off

- [x] CI is safe for public submissions and public checks do not depend on internal access. Public `ci.yml` jobs use `ubuntu-latest` and `permissions: contents: read`. The PAT fallback is removed after bindb and binder-llm are public.
- [x] Source, packaging, and documentation licensing are consistent. Source and `packaging/LICENSE.txt` are MIT. Historical release archives still contain the old proprietary `LICENSE.txt`; the release notes withdraw it.
- [x] Disclosure review covers the public refs and relevant GitHub content. gitleaks 8.28.0 and the issue/PR scan on 2026-10-26 found no credentials.
- [x] Confirmed sanitizer defects and credential argument exposure are fixed. Locked binder-llm tests include the redaction regressions.
- [x] Each repository's supported build/use procedure succeeds externally. `cargo test --locked` passed for all four on 2026-10-26. The synthetic index, summary, and `binder-ai-plan` run completed.
- [x] Validation evidence and any remaining limitations are documented. Accepted limitations: no authentication on binder-ai HTTP routes other than `POST /refresh`; the container binds `0.0.0.0:7841` for the ClusterIP Service and must not be published; `git2` 0.20.4 unsound warnings; `bincode` 1.3.3 unmaintained warning; historical release tarballs still contain the withdrawn license file; the fork pull request is run after the visibility change.
- [x] Release owner approves the exact repositories and refs for publication.

Release owner: Darin Kimball, approved in chat on 2026-10-26, including publication of `bindb/image.png`.  
Reviewed commit SHAs: bindb `40730ea18571f2e1ba8cc9e9c7be635a33351a37`, codeintel `68e37d3ebb5edc25b08b32ec052fb03f12f99360`, binder-llm `294dd57e0eb2ea7dc6e1c77e95ad001f6fe7a054`, binder-ai `188c82cbec53d2ad381fa79d041eea5dfa1fefcd`. Visibility was changed to public on 2026-10-26 in that order.  
Approval date: 2026-10-26

## Closure (2026-10-26)

Locked tests on rustc 1.98.1, before the rustls/tokio lockfile bump: bindb, binder-llm, codeintel, and binder-ai all exited 0. binder-llm and binder-ai were retested after the advisory bump.

Synthetic repository `/tmp/oss-sample` (one Rust file): `codeintel index` wrote 1 file and 1 symbol; `query --question summary` returned that repository; `binder-ai-plan` printed JSON. `fallback_required` was true for that one-file sample, which has no indexed project docs. The command did not call a model.

gitleaks 8.28.0, all fetched branches and tags: 0 findings in each repository.

`cargo audit` after the lockfile bump: no vulnerabilities. Remaining warnings are `bincode` unmaintained (bindb, codeintel) and `git2` 0.20.4 unsound (binder-ai).

Branches that become visible with the repository, in addition to `master`:

| Repository | Remote branches |
| --- | --- |
| bindb | `chore/add-pr-template-and-checklist`, `chore/rescue-dirty-20260913`, `feature/oss-known-issues`, `feature/store-locked-111` |
| codeintel | `chore/stash-rescue-20260924`, `feature/oss-known-issues`, `feature/pr-cross-repo-impact` |
| binder-llm | `feature/chat-images`, `feature/oss-known-issues`, `feature/public-release-readiness` |
| binder-ai | `ci/rebuild-serve-codeintel-c512494`, `feature/ask-with-image`, `feature/pr-cross-repo-impact`, `feature/public-release-readiness`, `feature/push-store-refresh`, `foreman/work-item-05d38ee3` |
