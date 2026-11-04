# Akri Release Management — Open Questions & Backlog

Companion to [release-strategy.md](./release-strategy.md). The strategy describes the target
*mechanics* (release-please + GitHub App token + Conventional Commits). This doc tracks the
**decisions still needed** and the **release-management policy work** that must land around
those mechanics before/after cutover.

---

## 1. Open decisions (need maintainer input)

| # | Decision | Notes / recommendation |
|---|---|---|
| 1 | **CRD API version coupling** — decouple `vMAJOR` from the product version, or keep it coupled via `x-release-please-major` annotations? | **Highest impact.** Decoupling removes the hardest propagation and lets release-please own everything. Recommended: decouple. |
| 2 | **Cut `1.0.0`, or stay `0.x`?** | Determines the bump mapping (see strategy §6). At `0.x`: `feat`→patch, breaking→minor. This surprises contributors — either document it clearly or go `1.0`. |
| 3 | **Squash-merge mandatory?** | Required for clean Conventional history and release-please override features. Recommended: yes. |
| 4 | **`opt:` prefix** — keep as an alias mapped to "Performance & Optimizations", or migrate history to `perf:`/`refactor:`? | `opt` is non-standard and will be rejected by a Conventional-Commit title linter unless allow-listed. Recommended: migrate to `perf`. |
| 5 | **Workspace version handling** — does `cargo-workspace` bump the virtual `workspace.package.version`, or do we use the `extra-files` TOML fallback? | Answered by the Phase-1 spike. |
| 6 | **Docs versioning** — cut an `akri-docs` version on every release, or only minor/major? | — |
| 7 | **Announcement channels** — which to automate? | Slack likely automated; blog/social likely manual. |

---

## 2. Risks & validation

| Risk | Mitigation |
|---|---|
| `cargo-workspace` mishandles the virtual `workspace.package.version` | Phase-1 spike proves it before cutover; fallback = `extra-files` TOML updater + `cargo update --workspace` in the release job. |
| `extra-files` / `x-release-please-major` don't rewrite a file `version.sh` used to | Phase-1 spike enumerates all 8 files; coupled fallback keeps a CRD-only `version.sh` step; decoupling removes the problem. |
| Wrong bump level from a mis-typed commit | Enforced Conventional PR titles + squash-merge; Release PR is reviewed; `Release-As:` override available. |
| Release doesn't trigger downstream pipelines | Root cause is `GITHUB_TOKEN`; use the GitHub App token so `release: published` fires. Verify at cutover. |
| GitHub App key compromise | Org secret; App scoped to `contents`/`PRs`/`issues` on one repo; rotate periodically; `if: github.repository == …` keeps forks safe. |
| Broken/partial release | Publish workflows idempotent on tag; add a post-publish status check (see backlog item B8). |

---

## 3. Backlog — release-management policy (from maintainer review)

These are process gaps the tooling does **not** solve on its own. Track as issues.

### B1 — Branching & maintenance releases
- Define the release-line model (trunk-based `main` + maintenance branches, e.g. `release-0.13`).
- Backport policy: how many minors are supported, cherry-pick flow, and per-branch
  release-please config via `target-branch` + a separate manifest.

### B2 — Security / embargoed release path
- Document how an embargoed CVE fix flows *outside* the public Release-PR automation
  (private fork/branch → GHSA advisory → coordinated fast-follow public release).
- Tie into `SECURITY.md`.

### B3 — Pre-releases / release candidates
- Decide whether GA is preceded by an RC (`-rc.N`) via release-please `prerelease` versioning.
- Map the existing "bug bash" onto an RC → GA promotion step.

### B4 — Rollback / bad-release runbook
- "Roll forward" procedure (superseding patch) since image tags and the `gh-pages` chart
  index can't be cleanly unpublished.
- release-please recovery steps: stuck `autorelease: pending` label, `last-release-sha`.

### B5 — Governance & cadence
- Who is allowed to merge the Release PR (maintainers/CODEOWNERS)?
- Is cadence time-based (e.g. monthly) or feature-based? Document it.
- Link to `GOVERNANCE.md` / `owners.txt`.

### B6 — Supported-versions policy
- Formalize the Kubernetes support window / skew and how long a minor is maintained.
- Automate/curate the "Validated With" table (old checklist steps 1 & 6a).

### B7 — CHANGELOG migration
- Freeze the existing bespoke CHANGELOG format below a marker; let release-please manage
  above it (avoids a visible format seam).
- Use `bootstrap-sha` and confirm existing `vX.Y.Z` tags are discoverable so the first run
  doesn't re-parse all history.

### B8 — Release verification gate
- Add an install-the-published-chart / pull-the-image smoke test as a gate **before** the
  announcement and docs fan-out (reuse existing smoke-test pipelines).

### B9 — Housekeeping
- **DCO**: ensure squash-merge and bot commits preserve `Signed-off-by` so the DCO check
  doesn't block release commits.
- **Concurrency**: add a `concurrency:` guard to the release-please workflow.
- **Chart signing**: extend cosign signing to the Helm chart, not just images.
- **Chart vs appVersion**: they are coupled 1:1 today; decide whether to allow divergence.

---

## 4. Scope note

This strategy governs the **public GitHub release path** (`ghcr.io` images + `gh-pages`
Helm repo). It does **not** cover any internal build/promotion pipeline (e.g. OneBranch /
MCR promotion); that remains a separate concern and should be reconciled explicitly if both
paths ship the same artifacts.
