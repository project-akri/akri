# Releasing Akri

The canonical runbook for cutting an Akri release. It covers `project-akri/akri` and the
repositories a release depends on: `project-akri/examples`, the chart index published from the
`gh-pages` branch, and `project-akri/akri-docs`.

Replace `X.Y.Z` with the release version throughout. Tags are `vX.Y.Z`; file contents are `X.Y.Z`.

---

## 0. Decide the version

Bump the **minor** version when any of the following changed since the last release:

- the supported Kubernetes baseline or the CI test matrix,
- the public API surface (`kube-rs`, `tonic`, `prost`, `hyper`, `rustls`) — this breaks downstream embedders,
- the MSRV,
- image paths or Helm value names.

Otherwise a patch bump is enough.

## 1. Release order

The repositories are coupled, so release in this order — each step publishes artifacts the next one references:

1. **`project-akri/examples`** — broker and sample-app images
2. **`project-akri/akri`** — core images and the Helm chart
3. **`project-akri/akri-docs`** — documentation referencing the published image tags

Documentation must land **after** the examples release. It pins
`ghcr.io/project-akri/examples/*:vX.Y.Z`, which does not exist until that release is published.

---

## 2. `project-akri/examples`

- [ ] Bump `version.txt` to `X.Y.Z` and merge.
- [ ] Confirm the four build workflows publish `vX.Y.Z-dev` images.
- [ ] Create a GitHub Release tagged `vX.Y.Z`.
- [ ] Confirm each of the five images — `udev-video-broker`, `onvif-video-broker`,
      `opcua-monitoring-broker`, `video-streaming-app`, `anomaly-detection-app` — published
      `vX.Y.Z`, `vX.Y` and `latest`. The package pages under the repository's _Packages_ tab
      list the tags.

> Pull-request CI in `examples` is build-only; it never pushes. The publish and manifest steps
> run on `push` to `main` and on `release`. A green PR check does **not** prove the publish path
> works, so verify the tags after the release rather than trusting CI.

---

## 3. `project-akri/akri` — pre-release

- [ ] Confirm `main` is green: `check-rust`, `check-versioning`, `run-helm`,
      `build-rust-containers`, and the full nine-job `run-test-cases` matrix.
- [ ] Review `cargo audit`. Any advisory not fixed in this release must be listed under
      **Known Issues** in `CHANGELOG.md`.
- [ ] Confirm the core image repositories in `deployment/helm/values.yaml` still point at
      `ghcr.io/project-akri/akri/`.

> `run-test-cases` sleeps for 80 minutes before running the suite on **every** `push` to `main`,
> not only on a release. `main` therefore goes green roughly 85 minutes after any merge. Budget
> for that wait here, and again after the version bump lands.

### Version bump

Write the new version into `version.txt`, then run `./version.sh -u -s` to propagate it to
`Cargo.toml`, `deployment/helm/Chart.yaml`, the CRDs and `values.yaml`. Refresh the workspace
crate versions in `Cargo.lock` with `cargo update --workspace`. Finally run `./version.sh -c -s`,
which must exit zero — this is the same check the `check-versioning` workflow runs.

Confirm before opening the PR:

- `version.txt`, the `[workspace.package]` version in `Cargo.toml`, and both `version` and
  `appVersion` in `deployment/helm/Chart.yaml` are all `X.Y.Z`.
- The `Cargo.lock` diff touches **only** version lines. Third-party dependency churn is
  unrelated to the release and should be dropped.
- The CRD `API_VERSION` in `shared/src/akri/mod.rs` is **unchanged**. It tracks the CRD API
  version, not the release version.
- No per-crate edits were needed — every workspace member inherits the version from the
  workspace, so the root `Cargo.toml` is the only declaration point.

### Changelog

Add a `# vX.Y.Z` section at the top of `CHANGELOG.md` containing: the announcement, new features
(grouped by area), breaking changes, known issues, the validated distributions, what's next, and
contributors.

Verify it against the repository rather than by eye:

- Every referenced pull request exists and is merged.
- Every pull request merged since the previous release is either represented or deliberately
  omitted — compare against the commit log for that range.
- The contributor list equals the set of authors of those merged pull requests.
- **Known Issues** matches the current `cargo audit` output.
- **Validated With** matches the matrix in `.github/workflows/run-test-cases.yml`.

> Pull requests that do not change `version.txt` need the **`same version`** label, or
> `check-versioning` will fail.

---

## 4. `project-akri/akri` — release execution

- [ ] Merge the version bump pull request.
- [ ] Tag `vX.Y.Z` on `main` and publish the GitHub Release, using the changelog section as the
      release notes.

Publishing the release triggers `build-rust-containers`, `run-helm`, `run-test-cases` and
`check-versioning`. Images are tagged `vX.Y.Z`, `vX.Y` and `latest`.

### Release notes and flags

- The body comes from the `# vX.Y.Z` section of `CHANGELOG.md`. If this release is stable, check
  that the announcement sentence does not still describe it as a pre-release — the changelog is
  the source for the body, so a stale sentence ships with it.
- Set the **pre-release** and **latest release** flags deliberately, and make them agree with that
  sentence. Confirm afterwards that `releases/latest` resolves to the new tag.
- Delete stale draft releases before publishing, so the wrong one cannot be picked by mistake.
- _Generate release notes_ compares from the nearest **tag**, not the last **release**. A stray tag
  with no release attached silently shortens the diff, so check the tag list for orphans first and
  set the previous tag explicitly when generating. Remove any orphan tags afterwards.
- The container `latest` tag is independent of the pre-release flag. `docker/metadata-action` runs
  with `latest=auto` and no `flavor:` override, so `latest` moves on any non-prerelease semver tag
  regardless of how the GitHub release is marked.

- [ ] Confirm all eight core images published `vX.Y.Z`: `agent`, `agent-full`, `controller`,
      `webhook-configuration`, `debug-echo-discovery`, `udev-discovery`, `opcua-discovery` and
      `onvif-discovery`. The published name is the workflow's component label with `-handler`
      stripped, so the discovery handlers do **not** appear under their label.

> Verifying tags from the command line needs `read:packages` on the token
> (`gh auth refresh -s read:packages`). The registry's tag listing is paginated and ordered by
> creation, so a first-page check will not show freshly pushed tags — follow the `Link` header to
> the last page before concluding a tag is missing.

- [ ] Confirm `run-helm` published the chart and merged it into the chart index on `gh-pages`.
- [ ] Smoke-test a clean install from the published chart at `--version X.Y.Z` on a fresh
      cluster, using the debug-echo discovery handler, and confirm Configurations, Instances and
      broker Pods appear.

> `run-test-cases` deliberately waits 80 minutes before running the suite, on both `push` and
> `release` events, so the chart and containers have time to publish first. Each run occupies nine
> runners for roughly 85 minutes — avoid scheduling other repositories' CI against it, and expect
> the same wait after merging the version bump above.

---

## 5. `project-akri/akri-docs`

- [ ] Repoint every `ghcr.io/project-akri/examples/` reference to `vX.Y.Z`.
- [ ] Wherever a Helm command sets a broker `image.repository`, make sure it also sets
      `image.tag`. The chart defaults broker tags to `latest`, so a repository-only override
      silently resolves to a tag that may not exist.
- [ ] Refresh the supported Kubernetes versions, the Rust toolchain version, and any sample
      command output that shows a version.

---

## 6. Post-release

- [ ] Confirm the monthly dependency-update workflow is healthy. It runs `./version.sh -u -p`
      whenever it finds updates, so the next patch bump arrives on its own — there is nothing
      to reset by hand. `version.txt` never carries a `-dev` suffix; the build appends that to
      non-release image tags.
- [ ] Move any still-open advisories into a follow-up tracking issue.
- [ ] Close the release tracking issue.

---

## Troubleshooting

**A workflow never gets a runner.** Check whether another repository in the organisation is
holding them — a release `run-test-cases` occupies nine runners for about 85 minutes. Then check
the organisation's Actions spending limit and its fork pull-request approval policy.

**An image is missing from the registry.** If the package has untagged versions but no tagged
ones, the per-architecture images were pushed by digest and the manifest step then failed. Look
at the tagging step, not the build.

**`check-versioning` fails on a pull request.** Either bump `version.txt` or add the
`same version` label.
