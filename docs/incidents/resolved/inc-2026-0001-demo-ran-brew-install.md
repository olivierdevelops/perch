---
document_id: INC-2026-0001
title: A parity run executed a demo that installed packages on the developer host
document_type: incident
status: resolved
created_date: 2026-10-01
last_updated: 2026-10-01
document_revision: 1
authors: [Claude]
owner: Perch maintainers (proposed)
reviewers: [Perch release maintainer]
systems: [Perch]
components: [parity testing, demos]
affected_versions:
  from: "0.1.1"
  to: null
applicable_environments: [development, CI]
audience: [engineers, maintainers]
scope: A parity-testing agent ran `demos/02-cross-platform-setup` `setup` unrestricted on the developer's machine.
reason: Record an unexpected defect or side effect found during implementation of PLAN-2026-0001, as required by documentation standard section 25.
related_documents: [PLAN-2026-0001, PROP-2026-0002]
supersedes: null
superseded_by: null
tags: [incident, side-effect, testing]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-11-01
---

# A parity run executed a demo that installed packages on the developer host

> **Status:** resolved · **Created:** 2026-10-01 · **Revision:** 1
> **Owner:** Perch maintainers (proposed) · **Plan:** [PLAN-2026-0001](../../plans/plan-2026-0001-perch-safe-embeddable-layer.md)

## Incident Summary

While comparing the Rust build with the Go build, the orchestrator-porting agent ran every command of the demos with real execution in one of its matrices. `demos/02-cross-platform-setup` `setup` ran `brew install`, which upgraded `jq`, `ripgrep` and `watchexec` on the host.

## Severity

Medium (unrequested change to a developer machine; reversible by reinstalling versions).

## Status

resolved

## Discovery Context

Reported by the agent in its final hand-back; surfaced to the requester immediately.

## Start Time

2026-10-01

## End Time

2026-10-01

## Affected Systems

Perch (Rust workspace on branch `rust-port`).

## Affected Versions

0.1.1 and the in-progress 0.2.0 work.

## Affected Components

[parity testing, demos]

## Customer Impact

None beyond the developer machine or CI run concerned; no release contained the defect.

## Detection

Reported by the agent in its final hand-back; surfaced to the requester immediately.

## Reproduction Steps

Run the demo's `setup` command without `--dry-run`, `--no-shell` or `--no-subprocess` on a host with Homebrew.

## Timeline

- Parity matrix designed with a category of 'unrestricted demo runs in copied trees'.
- `setup` ran `brew install`.
- Agent reported it; requester informed in the next status message.

## Logs and Evidence

Agent report text: the unrestricted demo run executed `demos/02-cross-platform-setup` `setup`, which ran `brew install` and upgraded three packages.

## Source Files

None (a test procedure defect, not a source defect). Demo: `demos/02-cross-platform-setup/commands.perch`.

## Root Cause

The matrix author treated copying the demo tree as sufficient isolation. A copied tree does not isolate package managers, which write outside the tree.

## Contributing Factors

Demos contain real install commands; the harness had no allowlist of side-effect-free commands; the agent prompt asked for parity but did not forbid real installs.

## Resolution

No code change. The procedure is corrected: parity and validation runs use read-only invocations or `--no-shell --no-subprocess --no-network --no-write` (the later parity run for R04 did exactly that: 322 invocations, no side effects). PLAN-2026-0001 constraints §3.4 now forbids real side-effect demos outside `--dry-run`.

## Verifying Tests

T-24 (CLI parity) re-run with the restricted flag set after the incident.

## Corrective Actions

Plan constraint added; every later agent prompt states 'no real side-effect commands'.

## Preventive Actions

Agent prompts for any parity or demo execution must name the restriction flags; the release demo (DEMO-2026-0001) uses temp directories and `--dry-run` for install steps.

## Owners

Perch maintainers (proposed); fix authored in this session.

## Remaining Risks

The three packages may have moved to newer versions on the developer host; perch cannot undo that.

## Lessons Learned

Isolation by copy is not isolation. Restrict capabilities with the tool's own flags when executing unknown commands.

## Related Documents

[PLAN-2026-0001](../../plans/plan-2026-0001-perch-safe-embeddable-layer.md) · [PROP-2026-0002](../../proposals/prop-2026-0002-perch-safe-service-layer-and-embeddable-runtime.md)

## Change History

| Rev | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-10-01 | Claude | Initial record. |
