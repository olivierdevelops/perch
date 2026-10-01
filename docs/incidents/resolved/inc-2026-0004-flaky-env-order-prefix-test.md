---
document_id: INC-2026-0004
title: An env-prefix test failed depending on test execution order
document_type: incident
status: resolved
created_date: 2026-10-01
last_updated: 2026-10-01
document_revision: 1
authors: [Claude]
owner: Perch maintainers (proposed)
reviewers: [Perch release maintainer]
systems: [Perch]
components: [infra/ops tests]
affected_versions:
  from: "0.1.1"
  to: null
applicable_environments: [development, CI]
audience: [engineers, maintainers]
scope: `t26_t27_t30_prefix_reaches_child_only` failed when it ran before the test that sets `PERCH_T28_DECLARED`.
reason: Record an unexpected defect or side effect found during implementation of PLAN-2026-0001, as required by documentation standard section 25.
related_documents: [PLAN-2026-0001, PROP-2026-0002]
supersedes: null
superseded_by: null
tags: [incident, flaky-test, env]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-11-01
---

# An env-prefix test failed depending on test execution order

> **Status:** resolved · **Created:** 2026-10-01 · **Revision:** 1
> **Owner:** Perch maintainers (proposed) · **Plan:** [PLAN-2026-0001](../../plans/plan-2026-0001-perch-safe-embeddable-layer.md)

## Incident Summary

The shared test header for the R05 prefix tests declared `env "PERCH_T28_DECLARED"` as a required variable. Only `t28` sets it with `std::env::set_var`. When `t26` ran first, preflight correctly refused with `requirement_unmet: required env var "PERCH_T28_DECLARED" is not set`, so the test saw empty output.

## Severity

Low (test-only; found by CI on macOS).

## Status

resolved

## Discovery Context

CI run 36794354349 failed on macOS; the same test passed on the next run. Reproduced locally 2 failures in 25 runs; the failure message was exposed by adding the error text to the assertion.

## Start Time

2026-10-01

## End Time

2026-10-01

## Affected Systems

Perch (Rust workspace on branch `rust-port`).

## Affected Versions

0.1.1 and the in-progress 0.2.0 work.

## Affected Components

[infra/ops tests]

## Customer Impact

None beyond the developer machine or CI run concerned; no release contained the defect.

## Detection

CI run 36794354349 failed on macOS; the same test passed on the next run. Reproduced locally 2 failures in 25 runs; the failure message was exposed by adding the error text to the assertion.

## Reproduction Steps

Run `cargo test -p perch-ops --test e2e` repeatedly; the order of test execution varies.

## Timeline

- CI macOS failure.
- Local reproduction (2/25).
- Error text added to the assertion; cause `requirement_unmet` identified.
- Header changed to `env "PERCH_T28_DECLARED" optional`; 60 consecutive green runs.

## Logs and Evidence

Panic message: `missing "first=hello" in out="" err=requirement_unmet: required env var "PERCH_T28_DECLARED" is not set`.

## Source Files

`infra/ops/tests/e2e.rs` (`PREFIX_SRC_HEAD`, the t26 assertion messages).

## Root Cause

A shared test fixture depended on process environment set by a different test.

## Contributing Factors

Rust runs tests in one process in parallel and in unspecified order; `set_var` is process-wide.

## Resolution

Declare the variable `optional` in the shared header; the tests that read it set it themselves. The assertion now prints the error text.

## Verifying Tests

`t26_t27_t30_prefix_reaches_child_only`, `t28_prefix_resolves_binding_and_declared_env`, `t29_*`: 60 consecutive green runs of the e2e target after the fix.

## Corrective Actions

Fix applied.

## Preventive Actions

Tests that need process environment set it themselves and declare it optional if another test also sets it; assertions print the error.

## Owners

Perch maintainers (proposed); fix authored in this session.

## Remaining Risks

Other tests use `set_var`; none are known to depend on order, but this was not audited.

## Lessons Learned

Print the underlying error in assertions; an empty output hides the cause.

## Related Documents

[PLAN-2026-0001](../../plans/plan-2026-0001-perch-safe-embeddable-layer.md) · [PROP-2026-0002](../../proposals/prop-2026-0002-perch-safe-service-layer-and-embeddable-runtime.md)

## Change History

| Rev | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-10-01 | Claude | Initial record. |
