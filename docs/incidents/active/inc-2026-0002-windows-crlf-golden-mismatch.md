---
document_id: INC-2026-0002
title: Windows checkouts used CRLF and broke byte-for-byte golden comparisons
document_type: incident
status: mitigated
created_date: 2026-10-01
last_updated: 2026-10-01
document_revision: 1
authors: [Claude]
owner: Perch maintainers (proposed)
reviewers: [Perch release maintainer]
systems: [Perch]
components: [httpserver, CI, Windows]
affected_versions:
  from: "0.1.1"
  to: null
applicable_environments: [development, CI]
audience: [engineers, maintainers]
scope: Windows CI failed `template_matches_go_html_template` because text files were checked out with CRLF line endings.
reason: Record an unexpected defect or side effect found during implementation of PLAN-2026-0001, as required by documentation standard section 25.
related_documents: [PLAN-2026-0001, PROP-2026-0002]
supersedes: null
superseded_by: null
tags: [incident, windows, ci, line-endings]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-11-01
---

# Windows checkouts used CRLF and broke byte-for-byte golden comparisons

> **Status:** mitigated · **Created:** 2026-10-01 · **Revision:** 1
> **Owner:** Perch maintainers (proposed) · **Plan:** [PLAN-2026-0001](../../plans/plan-2026-0001-perch-safe-embeddable-layer.md)

## Incident Summary

The first Windows CI run failed one test: the embedded `index.html` and its golden files carried `\r\n` on a Windows checkout, while the expected output used `\n`. A `.gitattributes` forcing LF was added. Later Windows runs revealed further, separate Windows failures (path normalisation, unix-only tests); by the requester's decision of 2026-10-01 Windows CI is deferred.

## Severity

Low for developers; Windows release binaries were untested.

## Status

mitigated

## Discovery Context

First CI run on the `rust-port` branch (run 36756795589), read through public check-run annotations.

## Start Time

2026-10-01

## End Time

2026-10-01

## Affected Systems

Perch (Rust workspace on branch `rust-port`).

## Affected Versions

0.1.1 and the in-progress 0.2.0 work.

## Affected Components

[httpserver, CI, Windows]

## Customer Impact

Windows builds of perch are untested and may mis-handle paths; see Remaining Risks.

## Detection

First CI run on the `rust-port` branch (run 36756795589), read through public check-run annotations.

## Reproduction Steps

1. Check out the repository on Windows with `core.autocrlf=true` (the runner default).
2. Run `cargo test -p perch-httpserver`.

## Timeline

- First Windows run: build failed on unix-only APIs (fixed by `fsx` shims).
- Second run: the golden comparison failed with CRLF in the left value.
- `.gitattributes` added: `* text=auto eol=lf`.
- Third and later runs: other Windows failures; Windows work deferred.

## Logs and Evidence

CI annotation text for run 36756795589: left value of `template_matches_go_html_template` contained `\r\n` throughout.

## Source Files

`.gitattributes`; `infra/httpserver/index.html`; `infra/httpserver/testdata/*.golden.html`; `infra/httpserver/src/tests.rs`.

## Root Cause

Git on Windows converts line endings on checkout; the embedded template and goldens were compared byte-for-byte.

## Contributing Factors

No `.gitattributes` existed; the golden files were generated on macOS.

## Resolution

`.gitattributes` forces LF for all text files. Windows path-normalisation fixes landed in `3b0471c`, `bdfbece`, `4718779`. The full Windows test suite was not driven to green: deferred by requester decision.

## Verifying Tests

`template_renders_complete_page_for_arg_commands` and the golden tests in `infra/httpserver/src/tests.rs` (green on macOS and Linux; Windows not confirmed).

## Corrective Actions

Finish the Windows test run and remove `continue-on-error` from `.github/workflows/ci.yml` (plan row L-19, deferred).

## Preventive Actions

Keep `.gitattributes`; any new golden file must be added with LF endings.

## Owners

Perch maintainers (proposed); fix authored in this session.

## Remaining Risks

Windows binaries are built and released but their behaviour is untested; the release notes list this as a known issue.

## Lessons Learned

Embedding files with `include_str!` makes line endings part of the program's behaviour; pin them in `.gitattributes`.

## Related Documents

[PLAN-2026-0001](../../plans/plan-2026-0001-perch-safe-embeddable-layer.md) · [PROP-2026-0002](../../proposals/prop-2026-0002-perch-safe-service-layer-and-embeddable-runtime.md)

## Change History

| Rev | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-10-01 | Claude | Initial record. |
