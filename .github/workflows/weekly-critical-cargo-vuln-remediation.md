---
emoji: 🔐
description: Weekly critical Cargo vulnerability remediation for Akri dependencies
name: Weekly Critical Cargo Vulnerability Remediation
on:
  schedule: weekly
  workflow_dispatch:
permissions:
  contents: read
  issues: read
  pull-requests: read
  actions: read
strict: true
network:
  allowed: [defaults, rust, github]
tools:
  github:
    mode: gh-proxy
    toolsets: [default]
safe-outputs:
  create-pull-request:
    title-prefix: "[security] "
    labels: [security, dependencies]
    draft: false
    if-no-changes: warn
    allowed-files:
      - "Cargo.lock"
      - "Cargo.toml"
      - "**/Cargo.toml"
      - "**/*.rs"
---

# Weekly Critical Cargo Vulnerability Remediation

## Task

Run a weekly security sweep for Cargo dependencies and remediate critical vulnerabilities without removing functionality.

1. Detect Rust dependency vulnerabilities and focus only on **critical** findings.
2. If there are no critical vulnerabilities, use `noop` with a short summary.
3. For each critical vulnerability, determine the minimal safe dependency upgrade needed.
4. Apply dependency updates in the relevant Cargo manifests and lockfiles.
5. Make only the minimal required Rust source changes if API updates are needed to keep existing behavior.
6. Verify the repository still builds after changes.
7. Create a pull request summarizing:
   - critical advisories addressed
   - dependency versions updated
   - any code changes required for compatibility
   - build verification results

## Safe Outputs

- Use `create-pull-request` to submit the remediation changes.
- Use `noop` when no critical vulnerability requires changes.
