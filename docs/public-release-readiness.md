# Public release readiness: bindb, codeintel, binder-llm, and binder-ai

Prepared: October 5, 2026  
Status: preparation is on feature branches. No repository visibility changes performed. Pull-request CI pins are recorded below. The private-token checkout of bindb and binder-llm stays until those repositories are public.

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

binder-ai pull-request CI uses `ubuntu-latest`, Cargo, `permissions: contents: read`, and no `secrets: inherit`. It checks out `binder-llm` as a sibling because `Cargo.toml` uses `path = "../binder-llm"`. The checkout ref is commit `a27c989c36c19c9ae7c40a49b721689c638a1c9b`. `secrets.PAT_WORKFLOWGITACTION || github.token` remains until binder-llm is public, because a fork pull request cannot read a private sibling. codeintel's checkout of bindb is pinned to `c07efbc31ff02f8cf87d8b1dc4a6f5f7447abd68` with the same token constraint. The image build stays on `product-ci` and does not run on pull requests.

**Finding:** Both PR workflows call `dpkimball/dpk-ci` using the `product-ci` runner and `secrets: inherit`. The locally inspected shared workflow uses a reused self-hosted workspace and executes the project's build. Public PR code must not gain access to trusted build infrastructure. Fork-secret restrictions alone do not isolate a runner.

References: `codeintel/.github/workflows/ci.yml`, `binder-llm/.github/workflows/ci.yml`, and the shared `dpk-ci/.github/workflows/rust-library-ci.yml`.

- [x] Run public PR checks on GitHub-hosted runners or disposable isolated runners without internal network access or retained credentials. (`ci.yml` in codeintel and binder-llm uses `ubuntu-latest` and `permissions: contents: read`.)
- [x] Make ordinary PR checks work without private `dpk-ci`, `dpk-build`, or registry credentials. Prefer direct Cargo commands for public validation.
- [x] Keep privileged release and private publishing jobs separate from untrusted PR execution. Release workflows stay tag-triggered. Security-audit and image workflows no longer run on `pull_request`.
- [x] Review security-audit and image workflows for the same private infrastructure assumptions.
- [ ] Review actual GitHub Actions permissions, fork approval settings, runner groups, environments, and secret access in all four repositories.
- [ ] Pin privileged third-party/shared workflow dependencies to reviewed immutable revisions. Pull-request `ci.yml` in all four repositories pins `actions/checkout` to `11d5960a326750d5838078e36cf38b85af677262` (v4.4.0) and `actions/cache` to `0057852bfaa89a56745cba8c7296529d2fc39830` (v4.3.0). `dtolnay/rust-toolchain@stable`, bindb release/bench/security-audit workflows, and any shared workflow still move with a tag.

**Acceptance:** A fork PR can run the intended public checks without private credentials, persistent internal runners, or privileged publishing access. Confirm this in GitHub settings and a controlled fork test, not solely by reading YAML.

### Align licenses — codeintel, binder-llm, and binder-ai

**Finding:** The manifests declare MIT. bindb already had a root LICENSE. codeintel, binder-llm, and binder-ai now have a root LICENSE and MIT text in `packaging/LICENSE.txt`.

- [x] Confirm the intended source and binary licensing policy. Aligned to the MIT text already declared in each `Cargo.toml` and already used by bindb. Release owner still signs the checklist below.
- [x] Add a root LICENSE to codeintel, binder-llm, and binder-ai matching the chosen policy and correct copyright holder.
- [x] Align packaged licenses, README language, manifest metadata, and release archive contents.
- [x] Update or clearly mark historical `docs/specs/private-source-public-binary.md` documents as superseded.
- [ ] Confirm rights to publish contributed or imported code and bundled assets, including bindb's `image.png`.
- [ ] Review dependency notices required for distributed binaries and include the applicable notices.

**Acceptance:** A reader and a binary recipient can determine their permissions without encountering contradictory current licensing instructions. Ownership and asset provenance have been checked by the release owner.

### Complete disclosure checks — all four repositories

**Review result (October 6, 2026, `origin/master` of all four repositories):** A pattern scan of current trees and of `git log -G` history covered AWS access-key IDs, `github_pat_`, `ghp_`, `gho_`, Slack token prefixes, Slack webhook URLs, and private-key headers. bindb (102 commits), codeintel (47), binder-llm (10), and binder-ai (64) were included. The only hit is the test constant `FAKE_TOKEN` in `codeintel/src/github_issues.rs`. No credential was copied into this document, and nothing was rotated. This is not a gitleaks/trufflehog run, and it does not cover GitHub issues, pull-request discussions, Actions logs, wikis, or release attachments.

- [ ] Fetch and inventory the branches and tags intended to become public, then run a dedicated secret scanner across that history and the release tree.
- [ ] Review scanner findings without copying real secrets into issues, reports, or logs.
- [ ] If a credential is found, revoke or rotate it before deciding whether history cleanup is needed.
- [ ] Review commit messages, author metadata, tags, Git LFS objects, submodules, and other material outside the earlier file-content scan.
- [ ] Review existing GitHub issues, PR discussions, releases, attachments, Actions logs/artifacts, and wiki content that may become visible.
- [ ] Review personal paths, internal service names, private repository references, and architecture details for intentional disclosure. Examples currently include Binder Desktop paths and internal registry/service names in fixtures and docs.
- [ ] Exclude populated private codeintel/bindb stores and real provider credentials from examples, release archives, and container build contexts.
- [ ] Add suitable ignore rules for local credentials and generated stores; do not rely on ignore rules to remove anything already tracked.

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
- [ ] Verify container build and runtime prerequisites separately; the current final Docker stage copies only the binary, while repository indexing invokes Git.

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

**Initial inspection:** Root and packaged LICENSE files match. Hosted Cargo PR CI and public development instructions are present. This is a targeted source/configuration inspection, not a completed runtime security review or secret scan.

- [ ] Include binder-ai in the GitHub-content, dependency-notice, and vulnerability scans. The October 6 pattern scan of `origin/master` history did include binder-ai.
- [ ] Remove the private PAT fallback from public dependency checkout after binder-llm is public, then verify a fork pull request. The binder-llm revision is pinned to `a27c989c36c19c9ae7c40a49b721689c638a1c9b`. Anonymous checkout cannot work while that repository is private.
- [ ] Record compatible revisions of all four repositories and test the locked build in a clean sibling workspace.
- [ ] Provide a complete example: index a synthetic repo with codeintel, point binder-ai at that store and binary, request a plan, and demonstrate a mocked or explicitly configured model workflow.
- [ ] Verify by running it that dry-run implementation leaves files unchanged. The README now states the binary default: `--dry-run` does not call a model and does not write; `--execute` may change files in the workspace; `--publish` is rejected.
- [ ] Review `LocalRuntime` file boundaries, symlinks, command execution, environment inheritance, and handling of untrusted repository/model content. The README now states that `LocalRuntime` runs commands on the host with the process environment and is not a sandbox.
- [ ] Verify LocalOnly/Private policy and fallback behavior using tests that prove no cloud provider receives request content under those policies.
- [x] Default listen when `--listen` and `BINDER_AI_LISTEN` are unset is `127.0.0.1:7841`. The container image still sets `0.0.0.0:7841`. The process still has no authentication. `POST /plan` does not call a model. `POST /ask` and `POST /explain` can. Question routes can change the bank.
- [ ] Test unauthorized access and workspace restrictions before recommending a network deployment.
- [x] README distinguishes `/plan` from `/ask`, `/explain`, and the question routes.
- [x] README no longer shows hygiene examples or links the private dashboard repository. `CLAUDE.md` no longer links it either.
- [ ] Verify the public container build includes compatible dependencies without relying on the private registry or internal image workflow.

**Acceptance:** An external engineer can run the documented four-repository workflow. Server and execution boundaries are documented and tested; source publication does not imply that the HTTP service is safe to expose unauthenticated.

## 3. Public documentation and maintenance policy

### bindb

- [x] Automatic closure of pull requests stays. `.github/workflows/no-prs.yml` matches the README: this repository does not accept contributions. Open an issue instead.
- [x] `docs/benchmark-comparison.md` separates measurements from the development machine and published figures from other systems.
- [x] Same-hardware and universal "N times faster" claims were removed from the takeaways and the summary. No replacement same-hardware benchmark was added.
- [x] README states WAL-fsync durability, replay on open, and that the on-disk format is not a compatibility promise between releases.

### All repositories

- [ ] Provide a concise purpose, supported use cases, installation/build steps, and a small example in each README.
- [x] Replace private issue-doctrine links in `.github/ISSUE_TEMPLATE` with accessible instructions. The contact link to the private workspace doctrine doc is removed in all four repositories. bindb's issue form no longer links it. `SECURITY.md` is present in bindb as well as codeintel and binder-llm.
- [ ] State contribution and support expectations; accepting PRs is optional.
- [x] Add a SECURITY.md with a private vulnerability reporting route that actually works. GitHub Security Advisories. Present in codeintel, binder-llm, binder-ai, and bindb.
- [ ] State current stability and compatibility expectations without overstating production readiness.
- [ ] Verify links and the source-to-binary release path, including separate `*-releases` repositories if retained.

## 4. Validation evidence to collect

Run in clean disposable checkouts with the selected dependency versions. Use test credentials only; provider integration tests should use mock servers.

| Repository | Required checks |
| --- | --- |
| codeintel | `cargo fmt -- --check`; `cargo clippy --locked --all-targets -- -D warnings`; `cargo test --locked`; index and query a small fixture repo; container smoke test if shipped |
| binder-llm | `cargo fmt -- --check`; `cargo clippy --locked --all-targets --all-features -- -D warnings`; `cargo test --locked --all-features`; external library consumer example; redaction regressions |
| binder-ai | `cargo fmt -- --check`; `cargo clippy --locked --all-targets --all-features -- -D warnings`; `cargo test --locked --all-features`; four-repository smoke test; dry-run, routing privacy, and server access-control checks |
| bindb | `cargo fmt -- --check`; `cargo clippy --locked --all-targets -- -D warnings`; `cargo test --locked --all`; verify recovery/locking tests and an external embedding example |

- [ ] Run a current dependency vulnerability audit for each lockfile and resolve or document findings.
- [ ] Verify the declared Rust 1.82 minimum against supported dependency configurations, or update that declaration.
- [ ] Build the intended release artifacts and inspect archive contents, license files, notices, and installation instructions.
- [ ] Record commit SHAs, toolchain versions, check results, and any accepted limitations in the release PR.

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

- [ ] CI is safe for public submissions and public checks do not depend on internal access.
- [ ] Source, packaging, and documentation licensing are consistent.
- [ ] Disclosure review covers the public refs and relevant GitHub content.
- [ ] Confirmed sanitizer defects and credential argument exposure are fixed.
- [ ] Each repository's supported build/use procedure succeeds externally.
- [ ] Validation evidence and any remaining limitations are documented.
- [ ] Release owner approves the exact repositories and refs for publication.

Release owner: ____________________  
Reviewed commit SHAs: ____________________  
Approval date: ____________________
