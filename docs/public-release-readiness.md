# Public release readiness: codeintel, binder-llm, and bindb

Prepared: October 5, 2026  
Status: preparation in progress on `feature/public-release-readiness` in codeintel and binder-llm. No repository visibility changes performed. bindb was left on `feature/store-locked-111`.

This document tracks the work needed to make the three repositories public and usable by external engineers, including CTCT Platform Engineering. Findings are based on the local repositories and locally reachable Git history reviewed on the date above. Unchecked items are work remaining, not confirmed vulnerabilities.

## Release criteria

Each repository is ready when its applicable release blockers below are resolved, a clean external-style build succeeds, and the release owner has reviewed what will become public. Publishing to crates.io and accepting outside contributions are separate choices; neither is required to make the repositories public.

## 1. Required before changing repository visibility

### Isolate public CI — codeintel and binder-llm

**Finding:** Both PR workflows call `dpkimball/dpk-ci` using the `product-ci` runner and `secrets: inherit`. The locally inspected shared workflow uses a reused self-hosted workspace and executes the project's build. Public PR code must not gain access to trusted build infrastructure. Fork-secret restrictions alone do not isolate a runner.

References: `codeintel/.github/workflows/ci.yml`, `binder-llm/.github/workflows/ci.yml`, and the shared `dpk-ci/.github/workflows/rust-library-ci.yml`.

- [x] Run public PR checks on GitHub-hosted runners or disposable isolated runners without internal network access or retained credentials. (`ci.yml` in codeintel and binder-llm uses `ubuntu-latest` and `permissions: contents: read`.)
- [x] Make ordinary PR checks work without private `dpk-ci`, `dpk-build`, or registry credentials. Prefer direct Cargo commands for public validation.
- [x] Keep privileged release and private publishing jobs separate from untrusted PR execution. Release workflows stay tag-triggered. Security-audit and image workflows no longer run on `pull_request`.
- [x] Review security-audit and image workflows for the same private infrastructure assumptions.
- [ ] Review actual GitHub Actions permissions, fork approval settings, runner groups, environments, and secret access in all three repositories.
- [ ] Pin privileged third-party/shared workflow dependencies to reviewed immutable revisions.

**Acceptance:** A fork PR can run the intended public checks without private credentials, persistent internal runners, or privileged publishing access. Confirm this in GitHub settings and a controlled fork test, not solely by reading YAML.

### Align licenses — codeintel and binder-llm

**Finding:** Both manifests declare MIT, but neither repository has a root LICENSE. Their `packaging/LICENSE.txt` files explicitly say the binaries are not open source and the source is private. bindb already has MIT text in both locations.

- [x] Confirm the intended source and binary licensing policy. Aligned to the MIT text already declared in each `Cargo.toml` and already used by bindb. Release owner still signs the checklist below.
- [x] Add a root LICENSE to codeintel and binder-llm matching the chosen policy and correct copyright holder.
- [x] Align packaged licenses, README language, manifest metadata, and release archive contents.
- [x] Update or clearly mark historical `docs/specs/private-source-public-binary.md` documents as superseded.
- [ ] Confirm rights to publish contributed or imported code and bundled assets, including bindb's `image.png`.
- [ ] Review dependency notices required for distributed binaries and include the applicable notices.

**Acceptance:** A reader and a binary recipient can determine their permissions without encountering contradictory current licensing instructions. Ownership and asset provenance have been checked by the release owner.

### Complete disclosure checks — all three repositories

**Review result:** A custom pattern scan found no matching credential signatures, private keys, literal credential assignments matching its rules, or CTCT references in current tracked files and 935 historical file blobs across 163 locally reachable commits. The repositories were not shallow. This does not establish that every secret or private reference is absent.

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

## 3. Public documentation and maintenance policy

### bindb

- [ ] Confirm whether automatic closure of PRs is intentional. The current `.github/workflows/no-prs.yml` closes new PRs, and README declares that contributions are not accepted. Keep both if this is the desired maintenance policy.
- [ ] Revise `docs/benchmark-comparison.md` so measured bindb results are clearly distinguished from third-party figures collected on different hardware and workloads.
- [ ] Remove unsupported same-hardware or universal performance claims, or supply controlled reproducible comparisons with exact versions and settings.
- [ ] Make durability, supported scale, recovery behavior, and storage-format compatibility expectations clear to adopters.

### All repositories

- [ ] Provide a concise purpose, supported use cases, installation/build steps, and a small example in each README.
- [x] Replace private issue-doctrine links in `.github/ISSUE_TEMPLATE` with accessible instructions. Done in codeintel and binder-llm. bindb still links at the private doctrine doc; that repo was not edited on this pass.
- [ ] State contribution and support expectations; accepting PRs is optional.
- [x] Add a SECURITY.md with a private vulnerability reporting route that actually works. GitHub Security Advisories. Done in codeintel and binder-llm.
- [ ] State current stability and compatibility expectations without overstating production readiness.
- [ ] Verify links and the source-to-binary release path, including separate `*-releases` repositories if retained.

## 4. Validation evidence to collect

Run in clean disposable checkouts with the selected dependency versions. Use test credentials only; provider integration tests should use mock servers.

| Repository | Required checks |
| --- | --- |
| codeintel | `cargo fmt -- --check`; `cargo clippy --locked --all-targets -- -D warnings`; `cargo test --locked`; index and query a small fixture repo; container smoke test if shipped |
| binder-llm | `cargo fmt -- --check`; `cargo clippy --locked --all-targets --all-features -- -D warnings`; `cargo test --locked --all-features`; external library consumer example; redaction regressions |
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
4. Make bindb available before or alongside codeintel so codeintel's documented dependency can be obtained. binder-llm can be released independently.
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
