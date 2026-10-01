---
document_id: INC-2026-0003
title: A mock HTTP test server closed sockets with unread request data and made a test flaky
document_type: incident
status: resolved
created_date: 2026-10-01
last_updated: 2026-10-01
document_revision: 1
authors: [Claude]
owner: Perch maintainers (proposed)
reviewers: [Perch release maintainer]
systems: [Perch]
components: [infra/ops group_b tests]
affected_versions:
  from: "0.1.1"
  to: null
applicable_environments: [development, CI]
audience: [engineers, maintainers]
scope: `http_local_get_post_and_redirects` failed intermittently when the mock server closed its socket before reading the whole POST.
reason: Record an unexpected defect or side effect found during implementation of PLAN-2026-0001, as required by documentation standard section 25.
related_documents: [PLAN-2026-0001, PROP-2026-0002]
supersedes: null
superseded_by: null
tags: [incident, flaky-test, networking]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-11-01
---

# A mock HTTP test server closed sockets with unread request data and made a test flaky

> **Status:** resolved · **Created:** 2026-10-01 · **Revision:** 1
> **Owner:** Perch maintainers (proposed) · **Plan:** [PLAN-2026-0001](../../plans/plan-2026-0001-perch-safe-embeddable-layer.md)

## Incident Summary

The group B test server read once (`read(&mut buf)`), wrote a canned reply and dropped the socket. For POST requests the body can still be in flight; closing a socket with unread data makes the OS send a reset, which the client sometimes saw as a connection error. The test failed about 1 run in 3 under load.

## Severity

Low (test flake).

## Status

resolved

## Discovery Context

Observed locally while re-running `cargo test -p perch-ops` to verify a build: 1 failure in 3 runs, none in 12 isolated runs.

## Start Time

2026-10-01

## End Time

2026-10-01

## Affected Systems

Perch (Rust workspace on branch `rust-port`).

## Affected Versions

0.1.1 and the in-progress 0.2.0 work.

## Affected Components

[infra/ops group_b tests]

## Customer Impact

None beyond the developer machine or CI run concerned; no release contained the defect.

## Detection

Observed locally while re-running `cargo test -p perch-ops` to verify a build: 1 failure in 3 runs, none in 12 isolated runs.

## Reproduction Steps

Run `cargo test -p perch-ops` repeatedly under load (the full suite in parallel).

## Timeline

- Failure seen once in three full runs.
- Not reproducible in 12 isolated runs.
- Cause identified by reading the mock server.
- Fixed with a `read_request` helper that drains headers and `Content-Length` bytes first; six clean runs follow.

## Logs and Evidence

Local runs: `http_local_get_post_and_redirects ... FAILED` once, then 6 consecutive passes after the fix.

## Source Files

`infra/ops/src/group_b/mod.rs` (test module).

## Root Cause

The test server did not drain the request before closing the connection.

## Contributing Factors

Test ran in parallel with CPU-heavy tests, widening the race.

## Resolution

Added `read_request` to the test module and used it in `serve` and the inline redirect server.

## Verifying Tests

`http_local_get_post_and_redirects` (6 consecutive green runs after the fix; also green in CI on macOS, Linux).

## Corrective Actions

Fix applied; no further action.

## Preventive Actions

Mock servers in tests drain requests before replying.

## Owners

Perch maintainers (proposed); fix authored in this session.

## Remaining Risks

A similar pattern exists in `infra/ops/tests/wasm.rs` (single `read`, small request bodies); not changed because requests there are GETs.

## Lessons Learned

Flakes caused by test fixtures are cheaper to fix than to retry; always identify the race.

## Related Documents

[PLAN-2026-0001](../../plans/plan-2026-0001-perch-safe-embeddable-layer.md) · [PROP-2026-0002](../../proposals/prop-2026-0002-perch-safe-service-layer-and-embeddable-runtime.md)

## Change History

| Rev | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-10-01 | Claude | Initial record. |
