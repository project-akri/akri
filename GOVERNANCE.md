# Akri Project Governance

This document outlines how the Akri project governs itself. The Akri project
consists of all of the repositories in the https://github.com/project-akri/
organization. 

Everyone who interacts with the project must abide by our [Code of Conduct](./CODE_OF_CONDUCT.md). 

## Legal

The Akri project is in the [CNCF Sandbox](https://www.cncf.io/sandbox-projects/). ⛱

## Roles

Akri has defined three roles that community members can fill. This section
defines the expectations and responsibilities of each role and describes how to
graduate from one role to another.
* Community member
* Maintainer
* Admin

### Community member
Anyone can be a member of the Akri community! :two_hearts: Here are some ways
you can actively engage
- Add context to an issue 
- Submit a pull request to fix an issue
- Submit a pull request to add a new feature to Akri such as a new Discovery
  Handler. See our [contributor
  guide](https://docs.akri.sh/community/contributing) to get started.
- Report a bug
- Create a pull request with a
  [proposal](https://github.com/project-akri/akri-docs/tree/main/proposals) of a way
  you'd like to extend, modify, or enhance Akri
- Join our [monthly community call](https://hackmd.io/@akri/S1GKJidJd)
- Join the conversation in our
  [Slack](https://kubernetes.slack.com/messages/akri)

### Maintainer
Maintainers can review and merge pull requests. The [CODEOWNERS](./CODEOWNERS)
file defines the current maintainers of the project.

Beyond maintaining Akri's code, maintainers also:
- Help drive the [road map](https://docs.akri.sh/community/roadmap) for Akri
- Organize and lead Akri's monthly [community
  meetings](https://hackmd.io/@akri/S1GKJidJd)
- Help foster a safe and inclusive environment for all community members and
  enforce Akri's [Code of Conduct](CODE_OF_CONDUCT.md).
- Triage issues (add labels, promote discussions, close issues)

If a **maintainer is no longer interested** or cannot perform the maintainer duties
listed above, they should volunteer to be moved to emeritus status.

Regular contributors can **become a maintainer of Akri**. If you frequently find
yourself doing any combination of commenting on issues, adding thoughts to PRs,
contributing PRs, writing proposals, attending community meetings, promoting
discussion on Slack, and so on, you may be a great candidate to become a
maintainer! Please reach out to one or more [maintainers](./CODEOWNERS) -- we
may even ask you first!

### Admin
Admins are maintainers with the added ability manage and create new repositories
in the `project-akri` organization.

## Release Process

Maintainers create the next release when a set of new features and fixes have
been completed and the main branch is stable. Akri does not have a fixed release
cadence; however, we prefer to release smaller batches of work more often.
Akri's [documentation
repository](https://github.com/project-akri/akri-docs/tree/main/proposals) creates
releases to match Akri's main repository (starting at `v0.6.19`).