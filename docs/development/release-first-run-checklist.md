# First Release Dry-Run Checklist

A one-time, end-to-end validation of the release-please pipeline (strategy §7, Phase 1→2).
Goal: confirm the **first Release PR** is correct — **especially `Cargo.lock`** — before any
real publish. Companion to [release-strategy.md](./release-strategy.md).

## 0. Preconditions
- [ ] GitHub App created and installed; `RELEASE_APP_ID` + `RELEASE_APP_PRIVATE_KEY` secrets set.
- [ ] release-please scaffolding merged to `main` (`release-please-config.json`,
      `.release-please-manifest.json`, `.github/workflows/release-please.yml`).
- [ ] `v0.13.26` tag exists on `main` and matches the manifest (baseline for the diff).
- [ ] Branch protection updated (or temporarily relaxed) for the new checks.

## 1. Trigger a Release PR
- [ ] Land one Conventional Commit on `main` (e.g. a `fix:`), or run
      *Actions → release-please → Run workflow*.
- [ ] Confirm release-please opens a **Release PR** labelled `autorelease: pending`.
- [ ] Confirm the proposed version is what you expect: pre-1.0, `fix:`→`0.13.27`,
      breaking→`0.14.0`.

## 2. Validate the Release PR diff (do NOT merge yet)
- [ ] `version.txt` → new version.
- [ ] `Cargo.toml` `[workspace.package] version` → new version.
- [ ] `deployment/helm/Chart.yaml` `version` **and** `appVersion` → new version.
- [ ] `CHANGELOG.md` → correct entries, grouped sections, only commits **since `v0.13.26`**
      (not the whole history).
- [ ] **`Cargo.lock` ⭐ — the 14 workspace crates (`agent`, `controller`, `akri-shared`, …)
      show the new version.** This is the known risk of the `simple` release type.

### If `Cargo.lock` was NOT updated
`simple` type doesn't touch the lockfile. Pick one and re-run:
- **Preferred:** switch to `rust` type + `cargo-workspace` plugin (strategy §5.1) so
  release-please maintains `Cargo.toml` + `Cargo.lock` natively; re-open the Release PR.
- **Fallback:** add a step in the release job (post-merge, pre-tag) that runs
  `cargo update --workspace` and commits `Cargo.lock`.

## 3. CI on the Release PR
- [ ] All checks green — **critically a build that uses `--locked`/`--frozen`** (proves the
      lockfile matches).
- [ ] `Version guard` does **not** block it (release-please branch is exempt).

## 4. Merge and verify publish
- [ ] Merge the Release PR (squash).
- [ ] release-please creates tag `vX.Y.Z` **and** a published GitHub Release (label flips to
      `autorelease: tagged`). If the Release is missing/draft, the App token isn't wired
      correctly (strategy §7.2).
- [ ] `release: published` fires the downstream pipelines:
  - [ ] `build-rust-containers` pushes `vX.Y.Z` + `vX.Y` images to `ghcr.io/project-akri/akri/*`.
  - [ ] `run-helm` publishes the `akri` (release) chart to the `gh-pages` repo.
- [ ] `docker/metadata-action` parsed the `vX.Y.Z` tag correctly (tags look right in GHCR).

## 5. Smoke test the artifacts
- [ ] `helm pull`/`install` the freshly published `akri` chart on a kind cluster; pods reach Ready.
- [ ] `docker pull` one released image and confirm the digest/tag.

## Abort / recover
- **Before merge:** just close the Release PR (nothing was published).
- **After a bad merge:** roll *forward* with a superseding patch (image tags and the
  `gh-pages` index can't be cleanly unpublished). See backlog **B4** for the release-please
  recovery steps (stuck `autorelease: pending`, `last-release-sha`).

## Sign-off
- [ ] Version, changelog, `Cargo.lock`, images, and chart all verified → the pipeline is
      trusted for routine releases. Remove any temporary branch-protection relaxations.
