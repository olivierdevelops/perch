---
document_id: STD-2026-0001
title: Perch proposal standards baseline
document_type: standard
status: draft
created_date: 2026-09-26
last_updated: 2026-09-26
document_revision: 1
authors: [Codex]
owner: Perch maintainers (proposed)
reviewers: [Perch maintainers]
systems: [Perch]
components: [documentation]
affected_versions: not-applicable
applicable_environments: [development]
audience: [engineers, reviewers]
scope: Index the authority used for the remediation proposal without adopting new project rules.
reason: Make architecture instructions and provisional documentation guidance explicit.
related_documents: [PROP-2026-0001, REF-2026-0003]
supersedes: null
superseded_by: null
tags: [documentation, traceability]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-10-26
rule_ids: [ARCH-001, CODE-001, QUAL-001]
enforcement_level: user-instruction-for-VHCO-provisional-for-documentation
validation_method: proposal-source-and-traceability-review
review_triggers: [user-clarification, architecture-change, release]
---

# Perch proposal standards baseline

> **Status:** Draft · **Created / Updated:** 2026-09-26 · **Revision:** 1
> **Owner:** Perch maintainers (proposed) · **Affected versions:** Not applicable

## Authority and reading order

1. [AGENTS.md](../../AGENTS.md) requires [vhco-architecture.md](../../vhco-architecture.md). These instructions apply by the user's explicit request; this draft index does not grant their authority.
2. [Documentation source snapshot](../references/ref-2026-0003-documentation-standard-source.md), original REF-2026-0001 revision 3, is the provisional format reference pending confirmation of the requested path.
3. [Remediation proposal](../proposals/prop-2026-0001-perch-correctness-security-and-completeness.md) revision 1 records application, evidence and remaining conflicts.

## Indexed rules

These IDs label existing instructions for traceability; they do not introduce new approved policy.

| ID | Source / revision | Intent and scope | Required / prohibited behavior | Example / validation | Authority / owner |
|---|---|---|---|---|---|
| ARCH-001 | VHCO Golden Rules 1–8, repository source at c60e402 (no document revision declared) | Closed-world boundaries for touched implementation | Consumer-owned protocols, explicit inputs, raw infra, all new composition in orchestrator; no new top-level folders | Inject HTTP adapter into orchestrator capability; inspect changed imports and signatures | User instruction; Perch maintainer role (assignment pending) |
| CODE-001 | VHCO Golden Rules 3–4, same source | Rendering and interaction separation | UI renders state; validation/business decisions outside rendering; no new display-text-producing business model | Enum selector renders schema, invocation validates; inspect UI/handler contract | User instruction; Perch maintainer role (assignment pending) |
| QUAL-001 | Documentation §§4.2, 12.1, 21, 34; provisional revision 3 | Reviewable proposal and eventual release evidence | Trace requests to requirements/use cases/files/tests; do not claim unexecuted tests or approved decisions | R1 -> UC-01 -> C-03 -> F05 -> T-01; verify complete inventory and links | Provisional reference; owner confirmation pending |

## Unresolved conflicts and exceptions

- Existing repository directories and imports do not fully match VHCO. Do not claim whole-repository compliance; migrate touched responsibilities and record remaining drift.
- VHCO forbids new top-level folders while its testing FAQ suggests a root tests folder. Prefer the explicit Golden Rule and existing colocated Go tests.
- The documentation standard asks for policy indexes and approval; this index is draft, not a new active rule set. The user's architecture instruction remains applicable independently.
- No exception is approved. A proposed exception must record exact scope, rationale, owner/approver and expiry before the affected implementation relies on it.
- No rules are superseded by this document. No new ownership assignment is asserted.

## Related Documents

- [Document index](../index/document-index.md)
- [Remediation proposal](../proposals/prop-2026-0001-perch-correctness-security-and-completeness.md)
- [Documentation source](../references/ref-2026-0003-documentation-standard-source.md)

## Change History

| Revision | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-09-26 | Codex | Index actual user authority, provisional reference and unresolved conflicts. |
