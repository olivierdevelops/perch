---
document_id: REF-2026-0003
title: Documentation standard source snapshot
document_type: reference
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
scope: Record the provisional documentation format used by the remediation proposal.
reason: The requested standard is absent from the repository; preserve the selected shared reference.
related_documents: [PROP-2026-0001, STD-2026-0001]
supersedes: null
superseded_by: null
tags: [documentation, traceability]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-10-26
---

# Documentation standard source snapshot

> **Status:** Draft · **Created / Updated:** 2026-09-26 · **Revision:** 1
> **Owner:** Perch maintainers (proposed) · **Affected versions:** Not applicable

## Provenance and applicability

Original: `/Users/oliverlaleau/Documents/projects/vibe-documentation/DOCUMENTATION.md`, REF-2026-0001 revision 3, updated 2026-08-27.

SHA-256 of original bytes: `5214f871da5ec7bd31609e9373a4f484d2a95fc80dae68e7eeb6ee3f1ba1500a`.

This is a verbatim reference snapshot, not an adoption or modification of project policy. The user was asked for the intended path; selection remains provisional. The complete source is quoted below to distinguish source content and metadata from this wrapper's identity. Relative links inside the quote are original-source references, not links claimed to resolve in this repository.

## Source text

`````markdown
---
document_id: REF-2026-0001
title: Documentation, Traceability and Release Standard
document_type: reference
status: active

created_date: 2026-08-11
last_updated: 2026-08-27
document_revision: 3

authors:
  - Documentation Owner

owner: Engineering Documentation Team
reviewers:
  - Architecture Team
  - Release Management

systems:
  - All

components:
  - docs

affected_versions: not-applicable

applicable_environments:
  - development
  - staging
  - production

audience:
  - engineers
  - architects
  - operators
  - technical-writers
  - release-managers

scope: Defines how project documentation is organized, named, written, reviewed, versioned, archived, and how a release is planned, implemented, tested, validated, documented and tagged.

reason: Provide a single authoritative standard so that every change in the project remains traceable from the original plan through implementation, incidents, tests, validation, demos, manuals, system documentation and the final tagged release.

related_documents:
  - REF-2026-0002

supersedes: null
superseded_by: null

tags:
  - documentation
  - traceability
  - release
  - governance

confidentiality: internal
review_cycle: 6-months
next_review_date: 2027-02-11
---

# Documentation, Traceability and Release Standard

> **Status:** Active
> **Created:** 2026-08-11
> **Last Updated:** 2026-08-27
> **Applies To:** All documentation under `docs/`
> **Owner:** Engineering Documentation Team

> **!!! IMPORTANT**
> This file is the single authoritative documentation standard. It lives at `docs/DOCUMENTATION.md`.
>
> * **Part I — Documentation Standard** (sections 1–21) defines how *all* documents are written.
> * **Part II — Release Process** (sections 22–35) defines how a release is produced.
>
> **When making a release you MUST follow Part II of this document.** Every document produced during a release MUST also satisfy Part I.

## Summary

Documentation is not a collection of files produced after the work is finished. It is a traceable chain of evidence created *during* the work: plans, incidents, troubleshooting notes, test records, validation results, demos, manuals and system documentation, terminating in a release document that is bound to an exact Git commit.

---

# Part I — Documentation Standard

## 1. Purpose

This standard defines how project documentation is organized, named, written, reviewed, versioned, maintained and archived.

Every document must clearly answer:

* What is this document?
* Why was it created?
* When was it created?
* Who owns it?
* What system, component or feature does it concern?
* Which software versions does it apply to?
* Is it still valid?
* What decisions or actions resulted from it?
* Which documents came before and after it?
* Where should related information be found?

Documentation should function as a traceable knowledge system that explains the history, design, operation and evolution of the project.

---

## 2. Core Principles

### 2.1 Every document must have context

A document without a date, owner, status, scope or affected version quickly becomes unreliable. Every document carries the metadata defined in section 5.

### 2.2 Documentation must reflect lifecycle

Documents may be proposed, reviewed, approved, implemented, deprecated, superseded or archived. The current lifecycle state must always be visible, both in the YAML front matter (section 6) and in the visible header (section 7).

### 2.3 Decisions and work must be traceable

A reader must be able to move forward and backward along the chain defined in section 3.2. Each document links to the documents that preceded and followed it.

### 2.4 Documentation must have ownership

Every active document has an owner, who is responsible for:

* keeping the document accurate;
* reviewing it when affected systems change;
* marking it deprecated when it is no longer valid;
* linking replacement documents;
* answering questions about its scope.

A document must not be marked `active` when nobody owns it.

### 2.5 Old documentation must not silently remain active

Outdated information is more dangerous than missing information. Documents that are no longer valid are marked `deprecated`, `superseded` or `archived` and link to their replacement.

### 2.6 Documentation is produced during the work, not after it

Incidents, troubleshooting findings, test records and validation results are written while the knowledge is fresh. Significant bugs must not be hidden inside implementation notes or commit messages.

### 2.7 Trace user intent through every change

For every proposed change, a reviewer must be able to follow one unbroken chain:

```text
user statement
  -> identified problem
  -> goal
  -> requirement
  -> user use case and surface interaction
  -> proposed implementation and file CRUD
  -> implementation-plan task
  -> automated or manual test
  -> released update
  -> demo and current manual instructions
```

Use stable IDs within the documents: `UQ-NN` for original user statements, `G-NN` for goals, `R1`, `R2`, … for
requirements, `UC-NN` for use cases, `C-NN` for proposed changes, `F-NN` for file changes, `T-NN` for tests
and `U-NN` for released updates. Each downstream row links back to the IDs it satisfies. A missing link is a
documentation defect: readers must not have to infer why code changed, which journey changed, or how a
requirement will be proved.

---

## 3. Directory Structure and Lifecycle

### 3.1 Canonical structure

All documentation lives under `docs/`.

```text
docs/
├── README.md                 # main entry point and index
├── DOCUMENTATION.md          # this standard (Parts I and II)
├── index/
│   ├── document-index.md
│   ├── component-index.md
│   ├── version-index.md
│   └── decision-index.md
│
├── architecture/
│   ├── system-overview/
│   ├── components/
│   ├── data-flow/
│   ├── deployment/
│   └── diagrams/
│
├── standards/
│   ├── index.md
│   ├── project-goals.md
│   ├── engineering-philosophy.md
│   ├── codebase-rules.md
│   ├── architecture-rules.md
│   ├── quality-expectations.md
│   └── exceptions.md
│
├── proposals/
├── discussions/
├── decisions/
├── plans/
├── techniques/
├── system/
├── api/
├── data/
├── security/
├── operations/
├── runbooks/
├── troubleshooting/
├── manuals/
├── onboarding/
├── demos/
├── testing/
├── reports/
├── incidents/
├── releases/
├── migrations/
├── research/
├── compliance/
├── references/
├── templates/
└── archive/
```

Not every project needs every folder immediately, but the structure must support these categories as the project grows. Folder names are always **plural** (`manuals/`, not `manual/`; `runbooks/`, not `runbook/`).

### 3.2 Canonical lifecycle chain

```text
Research
   ↓
Discussion
   ↓
Project Standards
   ↓
Proposal
   ↓
Decision
   ↓
Plan
   ↓
Implementation ──► Incident
   │           └──► Troubleshooting
   ↓
Testing
   ↓
Validation (report)
   ↓
Demo
   ↓
Manual
   ↓
System Documentation
   ↓
Version → Commit → Git Tag
   ↓
Release
   ↓
Operations
   ↓
Incident or Report
   ↓
Migration, Deprecation or Archive
```

Part II applies this chain to a concrete release.

---

## 4. Directory Definitions

Each subsection states what belongs in the folder, its document type and its ID prefix (section 6.1).

### 4.1 `architecture/` — type `architecture`, prefix `ARCH`

High-level and component-level architecture: system architecture, component boundaries, service relationships, deployment topology, data-flow and event-flow diagrams, dependency maps, infrastructure and network architecture.

Architecture documents explain how the system is *structured*. They are not used for temporary implementation discussions.

```text
architecture/
├── system-overview/
├── components/
├── data-flow/
├── deployment/
└── diagrams/
```

### 4.2 `proposals/` — type `proposal`, prefix `PROP`

Formal proposals for changes not yet approved or implemented: a new service, a database change, a plugin system, a replacement inference engine, a new authentication model, an API redesign, a new deployment strategy.

A proposal explains the problem, the proposed solution, alternatives, trade-offs, risks, affected systems, migration requirements and expected outcome. It identifies the active project standards that apply and records an initial validation result for each applicable rule (section 21). **A proposal is not a decision, and agent validation is not approval.**

Every proposal is also a reviewable simulation of the change. It records the request in the user's own
words, separates symptoms from root problems, and connects goals, requirements, user-visible use cases,
implementation methods, exact file CRUD and planned tests. The following coverage is mandatory:

1. **Original request (`UQ-NN`).** Preserve what was asked or said, with its source, date and relevant
   context. Quote only when the source permits it; otherwise provide a faithful paraphrase and link or cite
   the source inline. Later interpretation belongs in the problem and requirements sections, not in this
   record.
2. **Problem and goals (`G-NN`).** Describe the current journey, evidence, affected users and consequences.
   Define how each goal addresses one or more problems, the expected observable outcome and explicit
   non-goals. Include a before/after ASCII journey or state visual.
3. **Requirements (`R1`, `R2`, …).** State all functional, UX, API, CLI, data, security, compatibility,
   operational, documentation and release requirements with acceptance criteria and an external source.
   A requirement may cite a user statement, issue, active standard, current code, API contract, manual or
   other source, but never the proposal itself. Inline citations name the exact file/section, symbol, URL or
   command output and explain its relevance.
4. **Use cases (`UC-NN`).** Define the complete user-perspective catalogue. Each use case specifies actor,
   trigger, preconditions, environment, input, normal journey, output, visible result, negative/error paths,
   requirements, goals and planned tests. It lists every changed surface: exact CLI invocation and output;
   HTTP method, route, request, response, status and errors; or UI screen, control, event, state transition,
   feedback and recovery. Use case coverage becomes the basis of the test plan.
5. **Implementation design (`C-NN`).** For each use case, identify the method, affected environment,
   positive and negative paths, feasibility evidence, constraints and how it fulfils its requirements.
   Introduce every added, changed or removed command, route, event, screen, state, type, field, parameter,
   configuration key, environment variable, feature flag, storage item and error. Show material control,
   state, data and dependency changes with ASCII visuals.
6. **Exact source impact (`F-NN`).** Name every known path and CRUD operation (`CREATE`, `READ`, `UPDATE`,
   `DELETE`), the symbol or region, precise edit, reason, related change and requirements, plus dependent
   documentation and tests. `READ` means required discovery input and may coexist with another operation on
   the same path. If a path cannot be known before approval, state the discovery task and resolution rule;
   never invent a path or write "update relevant files."
7. **Complete inventory and alignment.** Summarize all CRUD items and prove both directions: every
   requirement maps to a goal, use case or internal constraint, change, file and test; every proposed change
   maps back to a requirement. Scope without a requirement is scope creep; a requirement without a change
   and validation method is unmet.

ASCII visuals are evidence, not decoration. At minimum, show current versus proposed user journeys, each
changed UI workflow, request/response or event sequence, and any changed component/data/state relationship.
Large proposals may split detailed diagrams or inventories into cited companion documents, but the proposal
retains the summary matrices and links; splitting must not break traceability.

### 4.3 `discussions/` — type `discussion`, prefix `DISC`

Structured technical or product discussions where no decision has been made yet: design explorations, meeting outcomes, competing approaches, unresolved questions.

A discussion identifies the topic, participants, options considered, open questions, agreements, disagreements and follow-up. Once a decision is made, the discussion links to the resulting decision record.

### 4.4 `decisions/` — type `decision`, prefix `ADR`

Architecture Decision Records and general Decision Records. This category is essential and must not be omitted.

Decision records are immutable historical records. Small corrections may be made, but the original decision is never rewritten to match later events. When a decision changes, create a new record and mark the old one `superseded`.

```text
adr-0001-use-postgresql-for-primary-storage.md
adr-0012-adopt-delayed-rag-indexing.md
```

### 4.5 `plans/` — type `plan`, prefix `PLAN`

    Intended work: implementation, migration, rollout, testing, release, recovery, deprecation and project plans.

    A plan defines objectives, scope, dependencies, milestones, owners, risks, rollback strategy and completion criteria. Plans describe future or ongoing work; reports describe completed or observed work.

    Your plan should be **complete and implementation-ready**, not a high-level outline.

    It must contain:

    1. **All goals and requirements**

      * Explicitly list **every goal, requirement, constraint, expected behavior, and acceptance criterion**.
      * Explain **why each requirement exists**.
      * Explain **how the proposed solution satisfies each requirement**.
      * Clearly state **what the implementation is expected to achieve** when complete.
      * Do not omit requirements because they appear obvious or are implied elsewhere.

    2. **Complete file-level change plan**

      * Identify **exactly every file that will be affected**.
      * For each file, specify the CRUD operation:

        * **CREATE** — new file
        * **READ** — existing file that must be inspected/used
        * **UPDATE** — existing file that must be modified
        * **DELETE** — file that must be removed
      * For **every file**, explain:

        * Why it needs to be read/created/updated/deleted.
        * What specifically will change.
        * Which requirement(s) the change satisfies.
        * Any dependencies or interactions with other files.
      * Do not use vague entries such as "update relevant files" or "modify tests." Name the **exact paths**.

    3. **Implementation approach**

      * Describe **how the solution will be implemented**, step by step.
      * Connect each implementation step to the requirements and files involved.
      * Explain important technical decisions and why the proposed approach can achieve the stated goals.
      * Identify assumptions, risks, edge cases, compatibility concerns, and anything that could prevent the proposal from succeeding.

    4. **Complete validation and test plan**

      * List **every test that must be performed**.
      * For each test, specify:

        * **What is being tested**
        * **Why the test is necessary**
        * **How the test will be performed**
        * **Expected result**
        * **Which goal/requirement it validates**
        * **Relevant files/components**
      * Include all applicable:

        * Unit tests
        * Integration tests
        * End-to-end tests
        * Regression tests
        * Error/failure-path tests
        * Edge-case tests
        * Compatibility tests
        * Build/compile checks
        * Static analysis/linting
        * Manual validation
        * Performance/resource tests where relevant

    5. **Requirement → Implementation → Test traceability**

      * Every requirement must map to:
        **Requirement → Implementation step → File change(s) → Validation test(s)**
      * There must be **no requirement without an implementation**, and **no implementation without a corresponding validation method**.

    The final plan should be detailed enough that another engineer or AI agent can execute it **without having to infer missing requirements, files, implementation steps, or tests**.

    A plan is also the **live execution ledger**, not a snapshot of the intended work. Its task tables are
    updated throughout implementation using only `NOT STARTED`, `IN PROGRESS`, `BLOCKED`, `DONE`, `FAILED`,
    `DEFERRED` or `NOT APPLICABLE`. Every row records its phase, owner, requirement/use-case/change IDs, exact
    files, test files or manual procedure, dependencies, status and evidence. A summary reports counts and
    the current blockers so opening the plan immediately answers what is happening and what remains.

    Group work into ordered phases when useful: discovery/contract approval, implementation, tests and
    validation, documentation and demos, release preparation, rollout and post-release verification. Each
    phase has entry and exit criteria. The plan separately inventories production files, test files,
    documentation files, generated artifacts, version sources and release artifacts so none disappear inside
    broad tasks.

    One proposal may have one plan or several. Use several plans when work has independently approvable or
    releasable increments, different owners or environments, or a size that makes one ledger difficult to
    review. In that case the proposal contains a plan map showing sequence, dependency, scope and the plan
    responsible for every requirement, `UC-NN` and `C-NN`; no item may be orphaned or ambiguously owned.


**Every release originates from a plan (section 24).**

### 4.6 `techniques/` — type `technique`, prefix `TECH`

Reusable engineering techniques and patterns: debounce strategies, delayed indexing, retries, caching, plugin loading, request tracing, concurrency control, distributed locking, prompt construction.

Technique documents capture reusable knowledge rather than a single project event.

### 4.7 `system/` — type `system`, prefix `SYS`

Detailed documentation of the **currently implemented** system: service responsibilities, internal modules, configuration, runtime behaviour, integrations, supported platforms, dependencies, background jobs, internal protocols.

```text
system/
├── components/
├── configuration/
├── integrations/
├── deployment/
└── runtime/
```

Historical designs stay in `decisions/`, `proposals/` or `archive/`.

### 4.8 `api/` — type `api`, prefix `API`

REST endpoints, WebSocket events, RPC services, plugin interfaces, webhook payloads, authentication requirements, error formats, API versioning, request/response examples.

```text
api/
├── public/
├── internal/
├── events/
├── webhooks/
└── examples/
```

Generated API references are stored separately from manually written API guides.

### 4.9 `data/` — type `data`, prefix `DATA`

Database schemas, entity relationships, data ownership, retention policies, data lifecycle, indexing strategies, backup policies, data migration rules, PII handling.

### 4.10 `security/` — type `security`, prefix `SEC`

Threat models, authentication design, authorization rules, secrets management, encryption requirements, vulnerability reports, review checklists, security incident procedures. Sensitive documents must set an appropriate `confidentiality` value.

### 4.11 `operations/` — type `operations`, prefix `OPS`

Environment configuration, deployment procedures, monitoring, logging, alerting, scaling, backup and restoration, service dependencies, production readiness requirements.

### 4.12 `runbooks/` — type `runbook`, prefix `RUN`

Step-by-step operational procedures: restart a failed service, rotate credentials, restore a database, clear a blocked queue, recover a node, perform a rollback, renew a certificate.

A runbook must be executable by someone other than its author and includes prerequisites, exact commands, expected output, verification, failure handling, rollback and escalation. It records a `last_validation_date`.

### 4.13 `troubleshooting/` — type `troubleshooting`, prefix `TRBL`

Known problems, symptoms, diagnostics and fixes, written so the knowledge is reusable (see section 26 for when a troubleshooting document is mandatory).

```text
troubleshooting/
├── installation/
├── runtime/
├── networking/
├── database/
├── ai/
└── platform/
```

### 4.14 `manuals/` — type `manual`, prefix `MAN`

User-facing and administrator-facing instructions: user manual, administrator manual, installation manual, configuration manual, operator guide, plugin development guide.

The folder is always named `manuals/`. Manuals explain; demos demonstrate.

The manual is the discoverable, current-state catalogue of everything a user can do and why they would use
it. Its index lists every supported feature and links to task-oriented instructions. Each feature documents
purpose, applicability, prerequisites, configuration, complete CLI/API/UI workflows, expected results,
errors and recovery, limitations, version applicability, and a verified demo. UI procedures name the screen,
control, action, visible state and next step; CLI and HTTP procedures are copy-pasteable. Use ASCII screen,
journey, sequence and state visuals wherever they make the interaction reviewable without running the app.

### 4.15 `onboarding/` — type `onboarding`, prefix `ONB`

Development environment setup, repository overview, first contribution guide, local testing, coding standards, access requirements, common workflows, glossary.

### 4.16 `demos/` — type `demo`, prefix `DEMO`

Demonstration scenarios and sample workflows: demo scripts, sample data, walkthroughs, screenshots, demo environments, expected results.

Every demo document states the version it was verified against and must contain reproducible commands, requests and expected output (section 29).

### 4.17 `testing/` — type `test`, prefix `TEST`

Reusable test definitions **and** their execution records: test plans, test cases, acceptance criteria, benchmark methodology, performance and compatibility testing, regression suites, evaluation datasets.

A `TEST` document contains both the definition of the test and its latest recorded result (`PASS`, `PARTIAL`, `FAIL`, `NOT APPLICABLE`), and links to the plan requirement it validates.

Broad analyses of test outcomes (for example a benchmark study or an evaluation write-up) belong in `reports/`.

### 4.18 `reports/` — type `report`, prefix `RPT`

Findings, results, assessments and completed analyses: performance reports, evaluation reports, investigation reports, security assessments, compatibility reports, **validation reports** (section 28), project completion reports, system health reports.

Reports clearly separate observations, evidence, interpretation, conclusions and recommendations.

### 4.19 `incidents/` — type `incident`, prefix `INC`

Production incidents *and* unexpected defects, regressions or abnormal behaviour discovered during implementation (section 25).

```text
incidents/
├── active/
├── resolved/
└── postmortems/
```

An incident document records start and end time, affected systems and versions, customer impact, detection, timeline, root cause, contributing factors, mitigation, corrective actions, verifying tests, owners and status.

### 4.20 `releases/` — type `release`, prefix `REL`

Release documents and release-specific material: release notes, compatibility changes, known issues, upgrade and rollback instructions, feature flags, removed features.

```text
releases/
├── v1/
├── v2/
└── unreleased/
```

Released documents are filed under the folder of their major version; work in progress lives in `unreleased/` until the tag exists.

### 4.21 `migrations/` — type `migration`, prefix `MIG`

Moving between technologies, architectures, schemas or versions: database, API, service, deployment and configuration migrations, language rewrites, dependency replacements. Every migration document includes a rollback plan.

### 4.22 `research/` — type `research`, prefix `RES`

Exploratory research that may not result in implementation: technology comparisons, model evaluations, framework investigations, feasibility studies, proof-of-concept findings, market research. Research distinguishes verified findings from assumptions and recommendations.

### 4.23 `compliance/` — type `compliance`, prefix `CMP`

Data retention rules, privacy requirements, audit evidence, licensing obligations, accessibility requirements, regulatory mappings.

### 4.24 `references/` — type `reference`, prefix `REF`

Stable reference material that does not belong to a workflow: terminology, glossaries, naming conventions, status definitions, supported platforms, port assignments, error code catalogs, environment definitions. **This standard is a reference document.**

### 4.25 `templates/` — type `template`, prefix `TMPL`

Approved templates, one per document type defined in section 12.

```text
templates/
├── proposal-template.md
├── discussion-template.md
├── decision-template.md
├── implementation-plan-template.md
├── technique-template.md
├── system-document-template.md
├── api-document-template.md
├── runbook-template.md
├── troubleshooting-template.md
├── test-template.md
├── validation-report-template.md
├── report-template.md
├── incident-template.md
├── demo-template.md
├── manual-template.md
├── release-template.md
└── project-standard-template.md
```

Section 12 defines the content of each of these templates. The two lists must always match.

### 4.26 `standards/` — type `standard`, prefix `STD`

Project-dependent goals, engineering philosophy, codebase rules, architecture invariants, quality expectations, required validation commands and approved exceptions.

`standards/index.md` is mandatory. It is the canonical entry point and current-state policy index, not merely a file listing. It records the directory owner, reading order, active standard documents and revisions, stable rule-ID ranges, recent changes, superseded rules, approved exceptions and unresolved conflicts.

```text
standards/
├── index.md
├── project-goals.md
├── engineering-philosophy.md
├── codebase-rules.md
├── architecture-rules.md
├── quality-expectations.md
└── exceptions.md
```

Projects may combine or split the subject files, but `index.md` is never optional. Rules use stable IDs: `GOAL-NNN`, `PHIL-NNN`, `CODE-NNN`, `ARCH-NNN`, `QUAL-NNN`, `GATE-NNN` and `EXC-NNN`. Each rule states its intent, rationale, scope, required and prohibited behaviour, examples, validation method, enforcement level, owner and exception process.

### 4.27 `archive/` — status `archived`

Documents that are no longer active but retained for history. Archived documents keep their original document type and ID, and preserve their original directory relationship:

```text
archive/
├── proposals/
├── plans/
├── system/
├── reports/
└── releases/
```

No document is moved to the archive without recording why (section 18).

---

## 5. Required Document Metadata

Every document begins with YAML front matter.

```yaml
---
document_id: TECH-2026-0021
title: Delayed RAG Indexing Strategy
document_type: technique
status: approved

created_date: 2026-07-15
last_updated: 2026-07-15
document_revision: 1

authors:
  - Oliver Luo

owner: AI Platform Team
reviewers:
  - Backend Team
  - Architecture Team

systems:
  - AI Manager

components:
  - ingestion-service
  - rag-indexer
  - primary-database

affected_versions:
  from: "2.4.0"
  to: null

applicable_environments:
  - development
  - staging
  - production

audience:
  - backend-engineers
  - ai-engineers
  - system-architects

scope: Defines how frequently updated content is stored immediately and indexed asynchronously.

reason: Reduce unnecessary indexing while keeping recent content immediately queryable.

related_documents:
  - ADR-0012
  - PLAN-2026-0018

supersedes: null
superseded_by: null

tags:
  - rag
  - indexing
  - debounce
  - database

confidentiality: internal
review_cycle: 6-months
next_review_date: 2027-01-15
---
```

Approved documents additionally carry the `approval` block defined in section 16. Archived documents additionally carry the `archive_date` and `archive_reason` fields defined in section 18.

---

## 6. Metadata Field Definitions

### 6.1 `document_id`

A permanent, never-reused unique identifier.

Format: `<PREFIX>-<YYYY>-<NNNN>`, with two exceptions noted in the table.

| Prefix | `document_type` | Directory | Format |
|---|---|---|---|
| `ARCH` | architecture | `architecture/` | `ARCH-2026-0004` |
| `PROP` | proposal | `proposals/` | `PROP-2026-0015` |
| `DISC` | discussion | `discussions/` | `DISC-2026-0008` |
| `ADR` | decision | `decisions/` | `ADR-0012` (global sequence, no year) |
| `PLAN` | plan | `plans/` | `PLAN-2026-0018` |
| `TECH` | technique | `techniques/` | `TECH-2026-0021` |
| `SYS` | system | `system/` | `SYS-2026-0030` |
| `API` | api | `api/` | `API-2026-0006` |
| `DATA` | data | `data/` | `DATA-2026-0003` |
| `SEC` | security | `security/` | `SEC-2026-0009` |
| `OPS` | operations | `operations/` | `OPS-2026-0011` |
| `RUN` | runbook | `runbooks/` | `RUN-2026-0004` |
| `TRBL` | troubleshooting | `troubleshooting/` | `TRBL-2026-0009` |
| `MAN` | manual | `manuals/` | `MAN-2026-0002` |
| `ONB` | onboarding | `onboarding/` | `ONB-2026-0001` |
| `DEMO` | demo | `demos/` | `DEMO-2026-0024` |
| `TEST` | test | `testing/` | `TEST-2026-0081` |
| `RPT` | report | `reports/` | `RPT-2026-0045` |
| `INC` | incident | `incidents/` | `INC-2026-0007` |
| `REL` | release | `releases/` | `REL-1.7.0` (version, not year) |
| `MIG` | migration | `migrations/` | `MIG-2026-0002` |
| `RES` | research | `research/` | `RES-2026-0013` |
| `CMP` | compliance | `compliance/` | `CMP-2026-0005` |
| `REF` | reference | `references/` | `REF-2026-0001` |
| `STD` | standard | `standards/` | `STD-2026-0001` |
| `TMPL` | template | `templates/` | `TMPL-2026-0007` |

The prefix, the `document_type` and the directory must always agree. Generic identifiers such as `DOC-…` are not permitted.

### 6.2 `document_type`

One of the values in the table above:

```text
architecture   proposal    discussion   decision    plan
technique      system      api          data        security
operations     runbook     troubleshooting          manual
onboarding     demo        test         report      incident
release        migration   research     compliance  reference
standard       template
```

### 6.3 `status`

The complete permitted set:

```text
draft
under-review
proposed
approved
active
implemented
mitigated
resolved
completed
rejected
cancelled
deprecated
superseded
archived
```

The permitted transitions depend on the document type:

| Document type | Lifecycle |
|---|---|
| Proposal | `draft → under-review → proposed → approved → implemented` (or `rejected` / `cancelled`) |
| Discussion | `draft → active → completed` (or `superseded`) |
| Decision | `proposed → approved → active → superseded` (or `rejected`) |
| Plan | `draft → under-review → approved → active → completed` (or `cancelled`) |
| Incident | `active → mitigated → resolved → completed` |
| Test | `draft → active → completed` (result is recorded separately, see section 6.4) |
| Report / Validation | `draft → under-review → completed` |
| Demo | `draft → active → deprecated → archived` |
| Manual / System / Runbook / API / Standard | `draft → active → deprecated → archived` |
| Release | `draft → under-review → completed` (never `active`) |

`status` describes the document, not a test outcome. Never use `PASS` or `FAIL` as a status.

### 6.4 Result values

Test results and validation results use exactly these values, everywhere in the documentation set:

```text
PASS
PARTIAL
FAIL
NOT APPLICABLE
```

### 6.5 `created_date`

The date the document was first created. It never changes. ISO format `YYYY-MM-DD`.

### 6.6 `last_updated`

The date of the latest meaningful content update. Purely cosmetic edits do not require a change.

### 6.7 `document_revision`

An integer incremented on every meaningful content change, matching the last row of the Change History table (section 10). It represents the revision of the *document*, never the version of the *software*.

### 6.8 `affected_versions`

The canonical form is a range:

```yaml
affected_versions:
  from: "2.4.0"
  to: "2.8.x"
```

An open-ended range uses `to: null`:

```yaml
affected_versions:
  from: "3.0.0"
  to: null
```

For documents that do not concern software versions:

```yaml
affected_versions: not-applicable
```

Discrete versions are expressed as a range whose bounds are equal or as multiple ranges:

```yaml
affected_versions:
  - from: "2.1.4"
    to: "2.1.5"
  - from: "2.3.0"
    to: "2.3.0"
```

Use semantic version ranges wherever practical.

### 6.9 `systems` and `components`

`systems` identifies the larger product or platform; `components` identifies the specific affected modules.

```yaml
systems:
  - Concio

components:
  - mobile-call-client
  - web-call-client
  - call-state-service
```

### 6.10 `reason`

Why the document exists. One of the most important fields.

Poor: `reason: Documentation`

Better: `reason: Document the call-state inconsistency observed when a callee returns a call immediately after an outgoing call completes.`

### 6.11 `supersedes` and `superseded_by`

```yaml
supersedes: SYS-2025-0018
superseded_by: null
```

The replaced document must be updated at the same time:

```yaml
status: superseded
superseded_by: SYS-2026-0031
```

### 6.12 `related_documents`

A list of document IDs in either direction of the chain in section 3.2. Release-related documents must at minimum link the plan and the release (`REL-<version>`).

### 6.13 `review_cycle`

```text
monthly   quarterly   6-months   annually   on-release   on-change   none
```

Critical operational documents use shorter cycles. Documents whose accuracy depends on the shipped version use `on-release`.

### 6.14 `confidentiality`

```text
public   internal   confidential   restricted
```

---

## 7. Standard Visible Header

After the YAML metadata, every document repeats the essentials in human-readable form.

```markdown
# Delayed RAG Indexing Strategy

> **Status:** Approved
> **Created:** 2026-07-15
> **Last Updated:** 2026-07-15
> **Affected Versions:** 2.4.0 and later
> **Owner:** AI Platform Team
> **Affected Components:** Ingestion Service, RAG Indexer, Primary Database

## Summary

This document defines how frequently changing content is stored immediately in the primary database while RAG indexing is delayed using debounce and batch-processing strategies.
```

The visible header must agree with the YAML front matter. Continuous integration checks this (section 19).

---

## 8. File Naming Convention

Lowercase kebab-case:

```text
<document-id-in-lowercase>-<short-description>.md
```

Examples:

```text
adr-0012-use-delayed-rag-indexing.md
prop-2026-0015-plugin-runtime-redesign.md
plan-2026-0018-postgresql-migration.md
test-2026-0081-call-state-cleanup.md
demo-2026-0024-plugin-cli.md
rpt-2026-0042-rag-retrieval-evaluation.md
inc-2026-0007-call-state-desynchronization.md
trbl-2026-0009-gpu-not-detected.md
rel-1.7.0-release-notes.md
```

Avoid: `new-document.md`, `notes.md`, `final.md`, `final-v2.md`, `updated-final.md`, `document-copy.md`, `report-latest.md`.

Version history is managed through Git, metadata and the Change History table — never through filenames.

---

## 9. Date Rules

Dates use ISO 8601:

```text
2026-07-15
```

Timestamps include the time zone:

```text
2026-07-15T09:30:00+08:00
```

Incident timelines, release records and test execution records must use full timestamps with time zones. Ambiguous formats such as `07/08/26`, `8/7/2026` or `July 8` are not permitted.

---

## 10. Document Versioning and Change History

Software version and document revision are distinct concepts (sections 6.7 and 6.8). Significant document changes are recorded in a Change History table whose last row matches `document_revision` and `last_updated`.

```markdown
## Change History

| Revision | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-07-15 | Oliver Luo | Initial document |
| 2 | 2026-07-18 | Backend Team | Added rollback procedure |
| 3 | 2026-08-01 | Platform Team | Updated affected versions |
```

---

## 11. Recommended Document Structure

Most technical documents follow this structure. Not every section applies to every type; the type-specific templates in section 12 take precedence.

```markdown
# Title

## Summary
## Background
## Problem
## Goals
## Non-Goals
## Scope
## Affected Systems
## Affected Versions
## Proposed or Current Design
## Alternatives
## Risks
## Security Considerations
## Operational Considerations
## Compatibility
## Migration or Rollout
## Validation
## Open Questions
## Decision or Outcome
## Related Documents
## Change History
```

---

## 12. Document-Type Templates

Each template below corresponds to exactly one file in `templates/` (section 4.25). All templates begin with the YAML front matter of section 5 and the visible header of section 7, and end with `## Related Documents` and `## Change History`.

### 12.1 Proposal — `proposal-template.md`

````markdown
# Proposal Title

## Decision Requested
State exactly what the reviewer is being asked to approve and what approval does not authorize.

## Original User Request
| ID | What Was Asked or Said | Source and Date | Interpretation Notes |
|---|---|---|---|
| UQ-01 | Faithful quote or paraphrase | Inline file/issue/conversation citation | Separate ambiguity from fact |

## Problem and Evidence
### Current User Journey
```text
[actor] -> [current action] -> [failure, gap or cost] -> [user consequence]
```
| Problem | Affected Users | Evidence and Inline Source | Consequence | UQ IDs |
|---|---|---|---|---|

## Goals and Non-Goals
| ID | Goal and Observable Outcome | Problems Solved | How It Solves Them | Success Signal |
|---|---|---|---|---|
| G-01 | | | | |

Explicit non-goals and boundaries:

## Proposed User Journey
```text
[actor] -> [entry point] -> [new interaction] -> [visible result]
             |                    |
             +-> [error] -> [feedback and recovery]
```

## Requirements
| ID | Requirement | Type | Source and Relevance | Acceptance Criteria | Goal IDs |
|---|---|---|---|---|---|
| R1 | | functional / UX / API / CLI / data / security / compatibility / operations / docs / release | | | G-01 |

## Use Cases
### Use-Case Catalogue
| ID | User Outcome | Actor | Surface and Trigger | Preconditions / Environment | Inputs | Outputs / Visible Result | Negative and Error Paths | Goal IDs | Requirement IDs | Test IDs |
|---|---|---|---|---|---|---|---|---|---|---|
| UC-01 | | | | | | | | G-01 | R1 | T-01 |

### UC-01 — Use-Case Name
Describe the numbered normal journey and every branch. Add all applicable surface contracts:

#### CLI Contract
```text
$ exact-command --all required-flags <input>
expected stdout
$ echo $?
0
```

#### HTTP Contract
```text
client                 API                  dependency
  | POST /path           |                       |
  |--------------------->| validate              |
  |                      |---------------------->|
  | 201 + response       |<----------------------|
  |<---------------------|                       |
```
Show a copy-pasteable `curl`, headers, realistic request body, response body, status, and each error shape.

#### UI Contract
```text
+---------------- Screen Name ----------------+
| Current state                                |
| [control] --event--> feedback/result         |
+----------------------------------------------+
      | success -> next state
      + error   -> message -> retry/recovery
```
Name the screen, entry path, controls, user events, focus/selection/loading/empty/error/success states,
navigation, accessibility feedback and recovery. Repeat for every added or changed interaction.

## Project Standards Baseline
| Standards Index | Revision | Validated At |
|---|---|---|
| docs/standards/index.md | | |
## Project Validation
| Rule | Applicability | Proposal Evidence | Initial Result | Exception or Follow-Up |
|---|---|---|---|---|
| ARCH-001 | Applies | | PASS / FAIL / EXCEPTION REQUESTED / NEEDS HUMAN REVIEW / NOT APPLICABLE | |

## Implementation Design
### Environment and Feasibility
| Environment / Version | Required Capability | Feasibility Evidence and Source | Constraint or Unknown | Resolution |
|---|---|---|---|---|

### Methods by Use Case
| Change ID | UC IDs | Method and Execution Order | Positive Path | Negative / Failure Path | Requirement Fulfilment | Feasibility |
|---|---|---|---|---|---|---|
| C-01 | UC-01 | | | | R1: explain how | proven / experiment needed / blocked |

### Added, Changed and Removed Contracts
| Item | CRUD | Kind | Name / Key / Route / Event | Type, Default or Schema | Scope / Lifetime | Consumers | Requirements |
|---|---|---|---|---|---|---|---|
| | CREATE / READ / UPDATE / DELETE | variable / env var / flag / command / route / event / screen / state / type / field / storage / error | | | | | |

### Architecture, Data, State and Interaction Visuals
Show before/after component boundaries, sequences, data transformations and state machines for every material
change. Label nodes with their source paths or contract IDs.

## Expected Code and Documentation Changes
| ID | Path | CRUD | Symbol / Region | Exact Planned Edit | Reason | Change IDs | Requirement IDs | Dependencies | Test / Documentation Impact |
|---|---|---|---|---|---|---|---|---|---|
| F-01 | exact/path | UPDATE | function or section | | | C-01 | R1 | | T-01, manual path |

For unresolved paths, record a bounded discovery action, evidence to inspect and the rule that selects the
path. Summarize counts and all created, read, updated and deleted features, contracts and files.
For every `CREATE`, `UPDATE` or `DELETE`, follow the inventory row with the proposed symbol signature,
schema, configuration fragment, pseudocode or focused before/after diff needed to review the edit. Label
illustrative code as illustrative; the proposal defines the approved behavior and does not authorize coding.

## Alternatives Considered
| Alternative | Advantages | Disadvantages | Why Selected or Rejected | Requirement Impact |
|---|---|---|---|---|

## Risks and Rollback
| Risk | Trigger / Detection | Impact | Mitigation | Rollback Action | Owner |
|---|---|---|---|---|---|

## Security Impact
## Operational Impact
## Compatibility Impact
## Migration Requirements

## Test and Validation Design
Derive tests from the use cases. Cover every normal journey, error branch, boundary, changed interface and
affected regression path.

| ID | Type | UC / Requirement IDs | Scenario and Purpose | Environment / Data | Exact Procedure or Command | Expected Result | Test File / Evidence Destination |
|---|---|---|---|---|---|---|---|
| T-01 | unit / integration / E2E / regression / failure / manual / performance | UC-01, R1 | | | | | |

### Measurement and Validation
Required when the proposal contains an optimization or any measurable claim.
| Measurement | Baseline Method | Test Command or Procedure | Controlled Environment | Acceptance Threshold | Report Destination |
|---|---|---|---|---|---|

## Documentation, Demo and Release Impact
| Artifact | Exact Path or Destination | CRUD | Required Content / Verification | Owner | Release Gate |
|---|---|---|---|---|---|
| Manual / CLI / API / system / architecture / demo / test / README / version source / release | | | | | |

## Requirements Alignment
| Requirement | User Request | Goal | Use Cases / Internal Constraint | Changes | Files | Tests | Manual / Demo / Release Evidence |
|---|---|---|---|---|---|---|---|
| R1 | UQ-01 | G-01 | UC-01 | C-01 | F-01 | T-01 | |

Reverse-check every `C-NN` and `F-NN` against at least one numbered requirement; explain any apparent orphan or remove it.

## Plan Strategy and Estimated Work
State whether one implementation plan is sufficient. If several are required, map every requirement,
use case and change to exactly one owning plan and show ordering and dependencies.

## Open Questions
## Approval
## Related Documents
## Change History
````

### 12.2 Discussion — `discussion-template.md`

```markdown
# Discussion Topic

## Context
## Discussion Date
## Participants
## Problem
## Options Discussed
### Option A
### Option B
### Option C
## Areas of Agreement
## Areas of Disagreement
## Open Questions
## Follow-Up Actions
## Resulting Decisions
## Related Documents
## Change History
```

### 12.3 Decision — `decision-template.md`

```markdown
# Decision Title

## Status
## Context
## Decision
## Rationale
## Alternatives Considered
## Consequences
### Positive Consequences
### Negative Consequences
### Risks
## Implementation Impact
## Superseded Decisions
## Related Documents
## Change History
```

### 12.4 Implementation Plan — `implementation-plan-template.md`

Requirements must be numbered (`R1`, `R2`, `R3`, …) because tests, validation and the release document reference them by number (sections 27, 28 and 32).

```markdown
# Implementation Plan

## Objective, Scope and Proposal Baseline
Link the approved proposal revision and list the owned `G-NN`, requirement, `UC-NN` and `C-NN` IDs. State what
this plan does not own and name the plan that does.

## Live Status Summary
| State | Count | Notes |
|---|---:|---|
| NOT STARTED | | |
| IN PROGRESS | | |
| BLOCKED | | |
| DONE | | |
| FAILED | | |
| DEFERRED | | |

Current phase, next action, blockers, last updated timestamp and release target:

## Requirements and Use Cases
| Requirement / UC | Planned Outcome | Acceptance Criteria | Owning Phase | Status |
|---|---|---|---|---|
| R1 / UC-01 | | | P1 | NOT STARTED |

## Applicable Project Standards
| Rule | Plan Impact | Validation |
|---|---|---|

## Measurable Claims
| Measurement | Baseline Method | Test Command or Procedure | Controlled Environment | Acceptance Threshold | Report Destination |
|---|---|---|---|---|---|

## Prerequisites

## Phases, Entry Gates and Exit Gates
| Phase | Purpose | Entry Criteria | Exit Criteria | Depends On | Status |
|---|---|---|---|---|---|
| P1 | Contract / discovery | | Human approval recorded | | NOT STARTED |
| P2 | Implementation | Approved contract | Buildable implementation | P1 | NOT STARTED |
| P3 | Tests / validation | P2 complete | All required gates pass | P2 | NOT STARTED |
| P4 | Docs / demos | Stable behavior | Current docs and verified demo | P3 | NOT STARTED |
| P5 | Release / rollout | Docs, demos and release candidate approved | Version shipped and verified | P4 | NOT STARTED |

## Live Work Checklist
| Task | Phase | Description / Method | Requirement / UC / Change IDs | Production Files | Test Files / Manual Procedure | Dependencies | Owner | Status | Evidence / Result |
|---|---|---|---|---|---|---|---|---|---|
| TASK-001 | P1 | | R1 / UC-01 / C-01 | exact/path | exact/test_path | | | NOT STARTED | |

Check boxes may accompany the table for quick scanning, but the table is authoritative and must retain
traceability and evidence when a task becomes `DONE`, `FAILED`, `BLOCKED` or `DEFERRED`.

## File and Artifact Checklist
| ID | Category | Exact Path | CRUD | Planned Edit | Requirements / Tasks | Status | Verification |
|---|---|---|---|---|---|---|---|
| F-01 | production / test / docs / generated / version / release | | | | | NOT STARTED | |

## Test and Validation Checklist
| Test ID | Type | Use Case / Requirement | Positive, Negative or Regression Scenario | Exact Test File or Manual Steps | Command / Environment | Expected Result | Status | Evidence |
|---|---|---|---|---|---|---|---|---|
| T-01 | | UC-01 / R1 | | | | | NOT STARTED | |

Include unit, integration, end-to-end, regression, error/failure, edge, compatibility, build, lint/static,
manual and performance/resource tests wherever applicable. Record `NOT APPLICABLE — <reason>` rather than
silently omitting a category.

## Documentation and Demo Checklist
| Artifact | Exact Path | Why It Changes | Required Addition / Removal | Related Interfaces | Status | Verification |
|---|---|---|---|---|---|---|
| README / manual / API / CLI / system / architecture / operations / troubleshooting / demo / test / index | | | | | NOT STARTED | |

The checklist explicitly considers new commands, flags, routes, schemas, events, screens, workflows,
configuration, environment variables, errors, migration instructions and when-to-use guidance. Demo rows
include executable CLI, HTTP and UI journeys with expected results.

## Version, Release and Rollout Checklist
| Item | Source / Target | Required Action | Dependency | Status | Evidence |
|---|---|---|---|---|---|
| Version | canonical version file | | | NOT STARTED | |
| Release notes | `releases/...` | | | NOT STARTED | |
| Release commit and tag | repository | | | NOT STARTED | |
| Rollout | environment | | | NOT STARTED | |
| Post-release verification | demo / monitoring | | | NOT STARTED | |

## Decisions, Findings, Deviations and Blockers
| Timestamp | Type | Task / Requirement | Finding or Decision | Impact | Owner / Follow-Up | Linked Incident / Research / ADR |
|---|---|---|---|---|---|---|

## Rollout Strategy
## Rollback Strategy
## Risks
## Completion Criteria and Final Traceability
| Requirement / UC | Implementation Tasks | File Changes | Tests | Docs / Demo | Release Update | Final Status |
|---|---|---|---|---|---|---|

## Post-Implementation Review
## Related Documents
## Change History
```

### 12.5 Technique — `technique-template.md`

```markdown
# Technique Title

## Summary
## Problem It Solves
## When To Use
## When Not To Use
## Mechanism
## Implementation Notes
## Source References
## Trade-offs
## Failure Modes
## Verification
## Related Documents
## Change History
```

### 12.6 System Document — `system-document-template.md`

```markdown
# Component or Service Name

## Summary
## Responsibilities
## Boundaries and Non-Responsibilities
## Architecture
## Interfaces
## Configuration
## Runtime Behaviour
## Data and Storage
## Dependencies
## Deployment
## Security Boundaries
## Observability
## Known Limitations
## Last Verified Version
## Related Documents
## Change History
```

### 12.7 API Document — `api-document-template.md`

```markdown
# API Name

## Summary
## Audience and Stability
## Authentication
## Endpoints or Events
## Request Format
## Response Format
## Error Format
## Rate Limits
## Versioning and Deprecation
## Examples
## Compatibility Notes
## Related Documents
## Change History
```

### 12.8 Runbook — `runbook-template.md`

```markdown
# Runbook Title

## Purpose
## When to Use This Runbook
## Preconditions
## Required Access
## Safety Warnings
## Procedure
## Expected Results
## Verification
## Rollback
## Failure Scenarios
## Escalation
## Related Monitoring
## Last Validation Date
## Related Documents
## Change History
```

### 12.9 Troubleshooting — `troubleshooting-template.md`

```markdown
# Troubleshooting Title

## Problem
## Symptoms
## Environment and Versions
## Investigation
## Possible Causes
## Experiments and Attempts
## Root Cause
## Solution or Workaround
## Verification
## Remaining Limitations
## Current Status
## Related Documents
## Change History
```

### 12.10 Test — `test-template.md`

```markdown
# Test Title

## Purpose
## Validated Requirements
| Plan Requirement | Description |
|---|---|
| PLAN-2026-0018 R3 | |
## Preconditions
## Test Environment
## Test Data
## Procedure
## Expected Results
## Actual Results
## Result
PASS | PARTIAL | FAIL | NOT APPLICABLE
## Evidence
## Evidence Sources
Commands, committed result files, CI runs, benchmark artifacts, logs and named datasets needed to reproduce the result.
## Executed By
## Executed At
## Defects Raised
## Related Documents
## Change History
```

### 12.11 Validation Report — `validation-report-template.md`

```markdown
# Validation of PLAN-XXXX-XXXX

## Summary
## Plan Under Validation
## Method
## Requirement Results
| Requirement | Expected | Observed | Test | Result |
|---|---|---|---|---|
| R1 | | | TEST-2026-0081 | PASS |
## Deviations From the Plan
## Unintended Behaviour
## Unresolved Incidents
## Remaining Limitations
## Required Follow-Up
## Conclusion
## Related Documents
## Change History
```

### 12.12 Report — `report-template.md`

```markdown
# Report Title

## Executive Summary
## Objective
## Scope
## Methodology
## Evidence
## Findings
## Analysis
## Limitations
## Conclusions
## Recommendations
## Follow-Up Actions
## Related Documents
## Change History
```

### 12.13 Incident — `incident-template.md`

```markdown
# Incident Title

## Incident Summary
## Severity
## Status
## Discovery Context
## Start Time
## End Time
## Affected Systems
## Affected Versions
## Affected Components
## Customer Impact
## Detection
## Reproduction Steps
## Timeline
## Logs and Evidence
## Source Files
## Root Cause
## Contributing Factors
## Resolution
## Verifying Tests
## Corrective Actions
## Preventive Actions
## Owners
## Remaining Risks
## Lessons Learned
## Related Documents
## Change History
```

### 12.14 Demo — `demo-template.md`

```markdown
# Demo Title

## Purpose
## Verified Against Version
## Prerequisites
## Setup
## Steps
### Command / Request
### Expected Output / Response
## Release Updates
| Update | Inciting User Requirement | What Changed | Do This | Expected Result | Evidence Source |
|---|---|---|---|---|---|
| U-01 | PROP-... R1 | | | | TEST-... |
## Cleanup
## Verification Record
| Step | Verified By | Verified At | Result |
|---|---|---|---|
## Known Caveats
## Related Documents
## Change History
```

### 12.15 Manual — `manual-template.md`

````markdown
# Manual Title

## Purpose
## Reading Order
## What's New in the Current Supported Release
List each user-visible addition or change, link its task instructions and verified demo, and point to the
release document for history. Keep the rest of the manual organized by user task rather than by release.
## Project Goals and Boundaries
## Concepts
## Architecture and Mental Model
## Installation and Setup

## Feature Catalogue
| Feature | Why / When to Use It | Supported Surfaces | Since Version | Instructions | Demo |
|---|---|---|---|---|---|

## Configuration and Environment Variables
| Name | Kind | Type / Allowed Values | Default | Required When | Scope | Effect | Security Notes | Example |
|---|---|---|---|---|---|---|---|---|

## Task-Oriented Workflows
### Feature / Task Name
Explain why and when to use it, prerequisites, environment, inputs and expected outcome.

#### Journey Overview
```text
[start] -> [action] -> [feedback/state] -> [result]
                       |
                       +-> [error] -> [recovery]
```

#### CLI Procedure
Show the exact copy-pasteable command, every parameter and default, stdout/stderr, exit code, side effects,
failure examples and recovery.

#### HTTP / Event Procedure
Show authentication, headers, exact method and route/event, `curl` or client request, request and response
schemas with realistic bodies, success statuses, every documented error/status and retry/recovery behavior.

#### UI Procedure
```text
+---------------- Screen X -----------------+
| 1. Locate [control]                       |
| 2. Enter/select [value]                   |
| 3. Activate [action]                      |
|    loading -> success result              |
|    error   -> message + recovery action   |
+-------------------------------------------+
```
Name how to reach the screen and give numbered interactions through the result. Describe what each loading,
empty, disabled, validation, confirmation, success and error state looks like, plus keyboard/accessibility
behavior where applicable.

#### Expected Result and Side Effects
#### Verified Demo
Link the executable `DEMO` document and its verified version, tag and commit.

## Complete CLI Reference
| Command / Flag | Purpose and When to Use | Syntax / Type / Default | Inputs | Output / Exit Codes | Errors | Example | Since |
|---|---|---|---|---|---|---|---|

## Complete API and Event Reference
| Method / Route / Event | Purpose | Auth | Request | Success Response | Errors / Status | Idempotency / Side Effects | Example | Since |
|---|---|---|---|---|---|---|---|---|

## UI Screen and Interaction Reference
| Screen | Entry Path | Controls / Events | States | Result / Navigation | Errors / Recovery | Related Features |
|---|---|---|---|---|---|---|

## Errors and Recovery Reference
| Error / Code / Message | Surface | Cause | User-Visible Result | Recovery | Retry Safe | Related Feature |
|---|---|---|---|---|---|---|

## Examples and Demos
## Operations, Observability and Maintenance
## Edge Cases
## Failure Modes, Recovery and Rollback
## Security and Compatibility
## Limitations
## Troubleshooting References
## Glossary
## Version Applicability
| Feature / Interface | Introduced | Changed | Deprecated / Removed | Applicable Environment |
|---|---|---|---|---|
## Related Features
## Related Documents
## Change History
````

### 12.16 Release — `release-template.md`

The full content of this template is defined in section 33.

### 12.17 Project Standard — `project-standard-template.md`

```markdown
# Project Standard Title

## Intent
## Rationale
## Scope
## Rules
| Rule ID | Required Behaviour | Prohibited Behaviour | Enforcement |
|---|---|---|---|
## Examples
## Validation Method
## Exceptions
## Owner and Review Triggers
## Related Documents
## Change History
```

---

## 13. README Requirements

Every major documentation folder contains a `README.md`, for example `docs/proposals/README.md`, `docs/testing/README.md`, `docs/incidents/README.md`.

Each README explains what belongs in the folder, what does not, required metadata, expected lifecycle, naming convention, the folder owner, links to the relevant template, active documents and deprecated documents.

The top-level `README.md` describes the current project entry points, headline capabilities, installation, configuration and primary workflows. It is reviewed after every release and updated whenever a significant user-visible capability, setup step, configuration contract, compatibility rule, primary workflow or project direction changes. A release that does not update it records `NOT APPLICABLE` and the reason.

---

## 14. Documentation Indexes

`docs/README.md` is the main entry point.

```markdown
# Project Documentation

## Start Here
- System Overview
- Developer Onboarding
- Installation Manual
- API Documentation
- Operations Guide
- Troubleshooting Guide

## Standards
- Documentation, Traceability and Release Standard (DOCUMENTATION.md)

## Architecture
- System Architecture
- Component Architecture
- Deployment Architecture

## Active Proposals
- PROP-2026-0015 Plugin Runtime Redesign

## Important Decisions
- ADR-0001 Primary Database Selection
- ADR-0012 Delayed RAG Indexing

## Current Releases
- REL-3.2.0
- REL-3.1.0

## Operations
- Production Deployment
- Database Recovery
- Incident Response
```

Generated indexes under `index/` allow lookup by type, component, version, date, owner and lifecycle status:

```text
index/document-index.md
index/component-index.md
index/version-index.md
index/decision-index.md
```

---

## 15. Linking Rules

Documents use relative links:

```markdown
See [ADR-0012: Delayed RAG Indexing](../decisions/adr-0012-use-delayed-rag-indexing.md).
```

* Every proposal links to related discussions, resulting decisions, plans and reports.
* Every plan links to its resulting implementation, tests, validation report and release.
* Every incident links to related troubleshooting documents, corrective plans, verifying tests, resulting decisions and the release that fixed it.
* Every test links to the plan requirement it validates.
* Every demo links to the manual section that explains the same functionality.
* Every deprecated or superseded document links to its replacement.

Broken links are detected automatically in continuous integration (section 19).

---

## 16. Review and Approval Rules

**Draft documents** may be incomplete but must already contain the document ID, title, author, creation date, status, scope and reason.

**Under-review documents** must identify reviewers, the review deadline, unresolved questions and the approvals requested.

**Approved documents** record approval in the front matter:

```yaml
approval:
  approved_by:
    - Architecture Team
    - Security Team
  approved_date: 2026-07-20
```

**Active documents** represent current behaviour, policy or guidance.

**Deprecated documents** must state why they are deprecated, when they became invalid, which document replaces them and which versions they still apply to.

---

## 17. Maintenance Rules

Documents must be reviewed when:

* the affected component changes;
* a new major release is created;
* an API contract changes;
* an incident reveals incorrect instructions;
* an architecture decision is replaced;
* a migration is completed;
* a security requirement changes;
* the `next_review_date` is reached.

Documents with `review_cycle: on-release` are reviewed as part of the release gate (section 34).

---

## 18. Archive Policy

Documents may be archived when they no longer apply to any supported version, the related project was cancelled, the system was removed, a proposal was rejected and is kept only for history, or a plan was completed and no longer needs active maintenance.

Before archiving:

1. Update the status to `archived`.
2. Record the archive reason.
3. Record the archive date.
4. Link to replacement documents.
5. Update the indexes.
6. Move the document under `docs/archive/`, preserving its original subdirectory.

```yaml
status: archived
archive_date: 2026-12-01
archive_reason: The legacy document parser was removed in version 4.0.0.
superseded_by: SYS-2026-0088
```

---

## 19. Automation and Validation

Documentation quality is checked automatically. Recommended rules:

* YAML front matter exists and parses;
* required fields for the document type are present (section 20);
* dates use ISO format;
* `document_id` is unique and never reused;
* the ID prefix, `document_type` and directory agree (section 6.1);
* `status` is valid for the document type (section 6.3);
* result values are one of `PASS`, `PARTIAL`, `FAIL`, `NOT APPLICABLE` (section 6.4);
* the file name matches the document ID (section 8);
* `affected_versions` is defined and well formed;
* an owner is defined for every non-archived document;
* the visible header matches the front matter (section 7);
* `document_revision` matches the last Change History row;
* internal links resolve;
* deprecated and superseded documents identify replacements;
* `next_review_date` is not overdue;
* the document appears in the appropriate index.
* every standards directory has an `index.md` whose active documents, rule IDs and revisions match the files;
* every proposal records its standards baseline and has no unresolved `FAIL` result before review;
* every proposal contains original-request, goal, requirement, use-case, change, file-CRUD and test IDs with no orphan in either direction;
* every changed CLI, HTTP/event and UI surface has a complete normal/error contract and at least one mapped test;
* every active plan reports live task status and every release-scope task is complete or explicitly resolved before release;
* every measurable claim has a predeclared test, threshold and reproducible report source;
* every released update traces from an inciting requirement to an executable verification row and evidence;
* every released feature appears in the manual feature catalogue with surface instructions, errors, version applicability and a verified demo;
* every release records demo, README, system, architecture, API/CLI and manual impact decisions.

```text
Commit or pull request
        ↓
Validate front matter
        ↓
Validate file name and ID
        ↓
Check document ID uniqueness
        ↓
Check status and result values
        ↓
Check links
        ↓
Check review dates
        ↓
Generate documentation indexes
        ↓
Publish documentation
```

---

## 20. Minimum Metadata by Document Type

In addition to the always-required fields (`document_id`, `title`, `document_type`, `status`, `created_date`, `last_updated`, `owner`, `reason`, `affected_versions`, `confidentiality`):

| Type | Additional required fields |
|---|---|
| Proposal | reviewers, systems, components |
| Decision | approval (decision date and approvers), systems, supersedes |
| Plan | start date, target date, dependencies, systems, components |
| System | component owner, last verified version, review cycle |
| Incident | severity, start and end times, systems, components, root-cause status |
| Troubleshooting | systems, components, current status |
| Test | validated plan requirements, environment, executed-by, executed-at, result |
| Demo | verified-against version, verification record |
| Manual | audience, applicable environments |
| Runbook | last validation date, applicable environments, escalation path |
| Report | report date, authors, scope, methodology, evidence sources |
| Release | version, Git tag, full commit SHA, release date, source branch, previous version and tag |
| Standard | rule IDs, owner, scope, enforcement level, validation method, review triggers |

---

## 21. Project Standards and Proposal Validation

Project standards are the project-dependent policy input that proposals must satisfy. They live under `docs/standards/`; `docs/standards/index.md` is the first file an author or agent reads.

### 21.1 Adding a philosophy, rule or expectation

When a user asks an agent to add the philosophy of the codebase, an architecture rule, a coding expectation, a project goal or a quality requirement, the agent follows this flow:

```text
User expectation
    ↓
Read standards/index.md and every potentially overlapping active standard
    ↓
Classify: goal · philosophy · codebase · architecture · quality · validation gate
    ↓
Check for duplication, overlap, contradiction and required exceptions
    ↓
Update the existing canonical rule, or draft a new standard when ownership or scope is distinct
    ↓
State rationale, scope, required/prohibited behaviour, examples and validation
    ↓
Obtain the required human approval
    ↓
Mark active and update index.md, revisions, change histories and affected proposals
```

A contradictory instruction is reported rather than silently replacing an active rule. Draft rules do not govern proposals until approval is recorded and the standards index marks them active.

### 21.2 Initial proposal validation

Before a proposal enters human review, the agent records the standards index revision and every applicable rule revision, then completes:

| Project Rule | Applicability | Proposal Evidence | Initial Result | Exception or Follow-Up |
|---|---|---|---|---|
| ARCH-003 | Applies | Design keeps domain code independent of adapters | PASS | none |
| QUAL-004 | Applies | Measurement environment is missing | FAIL | add environment before review |
| PHIL-002 | Unclear | Proposal introduces a second workflow | NEEDS HUMAN REVIEW | owner decision required |

Permitted results are `PASS`, `FAIL`, `EXCEPTION REQUESTED`, `NEEDS HUMAN REVIEW` and `NOT APPLICABLE`. `FAIL` blocks promotion to human review. An exception names its approver, reason, scope and expiry. Passing agent validation never approves the proposal.

### 21.3 Measurable proposals and plans

A proposal or plan triggers this rule when it claims or implies a change in latency, throughput, memory, storage, cost, quality, accuracy, reliability, load, size, duration, resource usage or any other comparable quantity.

Before implementation, it records the baseline method, exact test command or procedure, controlled environment, metric, acceptance threshold and test-report destination. The expected result is frozen before measurement. A claim without this information is not implementation-ready.

---

# Part II — Release Process

Every release must be fully documented, tested, validated and traceable from the original plan through to the final release document and Git tag.

## 22. Release Documentation Standard

**All documents produced during a release MUST follow Part I of this standard.**

Every document must preserve enough information to reconstruct why a change was made, how it was implemented and how it was verified. Where applicable, include:

* creation and last-updated dates;
* the software version concerned;
* related plan, release, incident, troubleshooting, test, validation, demo, manual and system documents;
* relevant source files and code locations;
* relevant code snippets;
* commands, APIs, configuration and environment details;
* external references and URLs;
* decisions made and their reasoning;
* known limitations and unresolved issues;
* verification evidence.

Documents must be cross-referenced so a reader can trace a release backwards:

`Release → Validation → Tests → Implementation → Plan`

and sideways into:

`Incidents / Troubleshooting / Demos / Manuals / System Documentation`

## 23. Release Workflow

```text
Project Standards
  │
  ▼
Proposal Validation
  │
  ▼
Plan
  │
  ▼
Implementation
  │
  ├──► Incident Documentation
  │       when unexpected bugs or regressions are discovered
  │
  ├──► Troubleshooting Documentation
  │       when difficult, unusual or unresolved technical
  │       problems are encountered
  ▼
Testing
  │
  ▼
Validation
  │
  ▼
Release Verification Guide and Demo
  │
  ▼
Manual Documentation
  │
  ▼
README · System · Architecture Assessment
  │
  ▼
Update Version
  │
  ▼
Final Release Commit
  │
  ▼
Record Commit Hash
  │
  ▼
Create Git Tag
  │
  ▼
Verify Tag → Commit
  │
  ▼
Release Documentation
```

This is the only release workflow; there is no separate "updated" variant. A release is not complete until every applicable stage has been completed and the gate in section 34 passes.

## 24. Implement an Approved Plan

Every release originates from a plan in `plans/` with `status: approved`.

Before implementation:

1. Identify the plan being implemented.
2. Confirm the proposal passed the project-standard validation in section 21 against the recorded standards index and rule revisions.
3. Confirm its numbered user requirements, affected components and expected results.
4. For every optimization or measurable claim, confirm the baseline, controlled environment, metric, exact test, acceptance threshold and report destination were defined before implementation.
5. Use the plan as the primary implementation reference.
6. Keep the plan linked to all resulting documentation via `related_documents`.

Implementation work must remain traceable to specific plan requirements (`R1`, `R2`, …).

If implementation differs materially from the plan, document what changed, why, what impact the deviation has, and whether the plan itself must be updated. Deviations are recorded in the validation report (section 28) and repeated in the release document.

## 25. Record Incidents During Implementation

If an unexpected bug, regression, failure or abnormal behaviour is discovered during implementation, create or update a document under `incidents/` using the template in section 12.13.

At minimum capture: what happened, when and how it was discovered, affected components and versions, reproduction steps, relevant logs and errors, relevant source files, root cause (if known), fix or mitigation, the tests that verify the fix, remaining risks and unresolved questions, and the related plan and release documents.

Do not hide significant bugs inside implementation notes or commit messages.

An incident discovered during implementation follows the incident lifecycle in section 6.3 and must reach `resolved` or be listed under Known Issues in the release document.

## 26. Record Troubleshooting Knowledge

Create a document under `troubleshooting/` (template 12.9) when a problem:

* was difficult to diagnose;
* required significant investigation;
* had a non-obvious solution;
* required experimentation;
* was caused by an unusual environment or configuration;
* could reasonably happen again;
* could not be fully solved;
* revealed useful technical knowledge.

Preserve reusable knowledge, not just the fact that a problem occurred:

```text
Problem → Symptoms → Investigation → Possible Causes →
Experiments/Attempts → Root Cause → Solution/Workaround →
Verification → Remaining Limitations
```

If the problem remains unresolved, mark its status clearly and reference it from the release document.

## 27. Test the Implementation

After implementation, all affected functionality must be tested. Testing covers, where applicable: new functionality, modified functionality, existing functionality affected by the change, regression scenarios, error handling, edge cases, API behaviour, CLI behaviour, configuration changes, integration behaviour, backward compatibility, and upgrade or migration behaviour.

The proposal use-case catalogue is the test-design baseline. For each `UC-NN`, execute at least its normal
journey, every declared negative/error branch, relevant boundaries and each changed surface. A use case with
no test is incomplete; a test that maps to no use case or internal requirement must explain why it exists.
UI tests reproduce the documented screen/event/state path, CLI tests assert output, stderr and exit status,
and API tests assert method, route, authentication, schema, status, error body and side effects.

Test definitions and their execution records live in `testing/` as `TEST` documents (section 4.17, template 12.10). Test evidence must be preserved and each test must link back to the plan requirement it validates:

```text
PLAN-2026-0018 R3
        │
        ▼
Implementation
        │
        ▼
TEST-2026-0081
        │
        ▼
PASS
```

A feature is not complete merely because the code compiles or the primary path works.

### 27.1 Measurable and optimization results

Every optimization or measurable claim identified by section 21.3 must execute its predeclared test. The `TEST` document records:

* the inciting proposal and user requirement;
* the unchanged baseline and how it was obtained;
* the controlled environment, configuration and dataset;
* the exact command or procedure;
* the acceptance threshold written before execution;
* actual values, variance and result;
* limitations and factors that may invalidate the comparison;
* reproducible evidence sources.

An evidence source is a command plus a committed result file, CI run, benchmark artifact, log or named dataset. An unsupported prose claim is not a source. The release cannot claim the optimization when the report is missing, the environment differs without explanation, or the result is `FAIL`.

## 28. Validate the Result Against the Plan

After testing, explicitly validate the implementation against the plan and record the outcome in a **validation report** under `reports/` (template 12.11).

For every requirement, record one of:

```text
PASS
PARTIAL
FAIL
NOT APPLICABLE
```

Validation must answer:

* Was the planned functionality actually implemented?
* Does it behave as expected?
* Were all expected files and components modified?
* Did implementation introduce unintended behaviour?
* Are there remaining limitations?
* Are there unresolved incidents?
* Are follow-up changes required?

Any `PARTIAL` or `FAIL` result must be documented and referenced from the release document.

## 29. Create the Release Verification Guide and Demo

Once implementation has passed testing and validation, create a release verification guide under `demos/` using the `DEMO` document type and template 12.14. This guide is mandatory for every release and is the release demo when it satisfies all demo requirements.

The guide begins by identifying the new release, its exact version, tag and commit. It then gives a reader who did not implement the change an executable verification procedure for every released update:

| Update | Inciting User Requirement | What Changed | Do This | Expected Result | Evidence Source |
|---|---|---|---|---|---|
| U-01 | `PROP-2026-0041 R2` | Expired sessions recover automatically | Expire a session, then request `/profile` | The request succeeds without a new login | `TEST-2026-0088` result and CI run |

Every `U-NN` row traces backward to the user requirement that incited the proposal and forward to test or demo evidence. Internal changes with no direct user action provide an operator or developer check, or state `NOT DIRECTLY USER-TESTABLE` with the reason and source evidence.

The guide includes prerequisites, environment and configuration, data setup, exact steps, cleanup, limitations, troubleshooting and actual verification status.

The demo shows how the new or modified functionality is actually used, and covers every newly introduced or changed interface: CLI commands, API endpoints, request/response formats, configuration, environment variables, UI workflows, SDK functions, scripts, operational commands, migration commands.

For a UI change, include an ASCII screen map and a numbered interaction from the entry screen through the
observable result, including loading, empty, disabled, validation, error and recovery states that changed.
For CLI and HTTP changes, include both successful and failing examples. The expected result must be specific
enough that a reader can decide pass or fail without interpreting the implementation.

Examples must be reproducible:

```bash
vhco example create --name demo
```

Expected result:

```text
Created example: demo
```

```http
POST /api/examples
Content-Type: application/json

{
  "name": "demo"
}
```

Expected response:

```json
{
  "id": "...",
  "name": "demo"
}
```

**Every documented demo must be executed or otherwise verified against the release version, and the verification recorded in the demo's verification record.** A demo that has not been executed cannot satisfy the release gate.

## 30. Update Manual Documentation

After the release verification guide and demo are verified, create or update documents under `manuals/` (template 12.15).

The manual is the canonical current-state book for the project, not a release diary or a collection of disconnected notes. A new reader must be able to follow it from the project's purpose and concepts through installation, real use, operation, troubleshooting and limitations without relying on undocumented knowledge.

The manual, or a root manual index that orders several volumes, covers where applicable:

1. project goals, boundaries and explicit non-goals;
2. core concepts, terminology, architecture and the reader's mental model;
3. prerequisites, installation, environments and configuration;
4. primary workflows from start to finish;
5. commands, APIs, interfaces, parameters and realistic examples;
6. expected behaviour, edge cases and failure modes;
7. operations, observability, maintenance, recovery and rollback;
8. security, compatibility, known constraints and limitations;
9. troubleshooting, glossary, references and version applicability.

The root manual index contains a feature catalogue answering **what can I do, why would I use it, and where
are the instructions?** Every added or changed feature has a task-oriented section with prerequisites,
environment, inputs, exact interactions, expected visible result, side effects, errors, recovery, limitations,
version applicability and a link to its verified demo. A reader testing a UI feature must be told which
screen to open, what to select or enter, what event to trigger, every intermediate state that matters and
what the final state looks like. CLI and HTTP instructions include copy-pasteable successful and failing
examples. Useful ASCII journey, screen, sequence and state diagrams are required for changed interactions.
A concise "What's New in the Current Supported Release" page or section links each addition to its permanent
task instructions and demo; detailed release history remains in `releases/`.

For a VHCO-annotated project, use the generated model to inventory interfaces before editing the manual:

```bash
vhco spec . --stdout              # structured model: features, actions, surfaces, flows and markers
vhco visualize .                  # every user action and the ports it needs
vhco flow .                       # how each action enters and proceeds layer by layer
vhco query . "actions"            # structured action inventory
vhco query . "effects"            # external effects to explain where relevant
vhco coverage .                   # annotation gaps that could make the inventory incomplete
vhco doc . --format html          # API & CLI, Errors, journeys and Documentation views
```

The API/CLI inventory comes from `vhco:surface`, `vhco:trigger`, `vhco:api`, `vhco:request` and
`vhco:response`; failure behavior comes from `vhco:error`; execution order comes from `vhco:step`. See
[`wiki/02-annotations.md`](wiki/02-annotations.md), [`wiki/05-traceability.md`](wiki/05-traceability.md),
[`wiki/06-markers.md`](wiki/06-markers.md) and [`wiki/07-docs-and-readme.md`](wiki/07-docs-and-readme.md).
The generated explorer is discovery evidence, not a substitute for the manual: compare it with the actual
CLI help, router, UI and tests, resolve coverage gaps, then write task-oriented prose and verified examples.
Generated files are never hand-edited.

Deep implementation detail may remain in `system/` or `architecture/`; the manual explains what the reader needs and links to those sources. Release history remains in `releases/`.

Update existing manuals rather than creating duplicates when the functionality belongs to an existing section. Integrate each release change into the correct chapter, remove or version-scope obsolete guidance and preserve a coherent reading order.

## 31. Update README, System and Architecture Documentation

After user-facing documentation is updated, make and record an explicit documentation-impact decision:

| Artifact | Update trigger | Required result |
|---|---|---|
| Release verification guide / demo | Every release | `UPDATED` and verified |
| Top-level `README.md` | Significant user-visible capability, setup, configuration, compatibility, primary workflow or headline project change | `UPDATED`, or `NOT APPLICABLE — <reason>` |
| `system/` | Implemented behaviour, dependencies, configuration, data, runtime or interfaces changed | `UPDATED`, or `NOT APPLICABLE — <reason>` |
| `architecture/` | Boundaries, components, ownership, data flow, deployment or major interactions changed | `UPDATED`, or `NOT APPLICABLE — <reason>` |
| `api/` and CLI reference | Routes, events, commands, flags, request/response schemas, errors or compatibility changed | `UPDATED`, or `NOT APPLICABLE — <reason>` |
| `manuals/` | A reader needs new or changed knowledge to install, configure, use, operate or troubleshoot the release | `UPDATED`, or `NOT APPLICABLE — <reason>` |

A blank decision fails the release gate. When applicable, synchronize `system/` (template 12.6), `architecture/`, `api/`, the top-level `README.md` and the affected manual chapters.

System documentation must describe the **current implemented system**, not the intended architecture. Update the affected areas: architecture, components, services, packages, modules, data flow, APIs, storage, configuration, runtime behaviour, dependencies, deployment, security boundaries, external integrations and internal interfaces.

A broader scan of the codebase may be used to detect documentation drift:

```bash
vhco doc
```

After the scan, verify:

```text
Actual Code  ≈  System Documentation
```

Any meaningful mismatch must be corrected or explicitly documented as a known limitation.

## 32. Release Versioning and Git Traceability

Every release is tied to an exact source state.

```text
Update Version → Commit Release State → Record Commit Hash →
Create Git Tag → Verify Tag → Commit → Finalize Release Document
```

### 32.1 Update the software version

Update the canonical version before creating the release. There must be exactly one authoritative version source, for example:

```text
VERSION
package.json
pyproject.toml
Cargo.toml
pubspec.yaml
go.mod / build metadata
application configuration
```

Example: `1.7.0`. All generated documentation and release information must use the same version. If several files carry version information, verify they are synchronized.

### 32.2 Commit the final release state

After implementation, testing, validation, demo verification, manual updates, system documentation updates and the version update, commit the complete release state:

```bash
git add .
git commit -m "release: v1.7.0"
```

Checking out that commit must reproduce the documented release state. Record the full commit SHA (not the abbreviated form):

```bash
git rev-parse HEAD
```

```text
8f1c42d73c5277c194e301ef7f03ea9de2639aad
```

### 32.3 Create and verify the Git tag

```bash
git tag -a v1.7.0 -m "Release v1.7.0"
git rev-list -n 1 v1.7.0
```

The returned commit hash must match the commit recorded in the release document. Tag format: `v<MAJOR>.<MINOR>.<PATCH>`.

Push the commit and tag when the repository has a remote:

```bash
git push origin <branch>
git push origin v1.7.0
```

### 32.4 Verify before finalizing

```bash
git status                          # working tree SHOULD be clean
git rev-parse HEAD                  # release commit
git describe --tags --exact-match HEAD
```

Expected:

```text
v1.7.0
```

```text
Release Document
      │ version = 1.7.0
      │ tag     = v1.7.0
      │ commit  = 8f1c42...
      ▼
   Git Tag v1.7.0
      ▼
Git Commit 8f1c42...
      ▼
Exact Source State
```

A release must never reference a tag and commit that point to different source states.

## 33. Create the Release Document

Only after implementation, incidents, troubleshooting, testing, validation, the release verification guide and demo, manual integration, README/system/architecture assessment, version update, commit and tag are complete is the release document created under `releases/v<MAJOR>/` with `document_id: REL-<version>`.

The release document is the central traceability record for the release and uses the following structure (this is the content of `templates/release-template.md`):

```markdown
# Release <version>

## Release Identity

Release Version: 1.7.0
Git Tag: v1.7.0
Git Commit: 8f1c42d73c5277c194e301ef7f03ea9de2639aad
Release Date: 2026-08-11
Source Branch: main
Previous Version: 1.6.2
Previous Tag: v1.6.2
Previous Release Commit: 3ab90f1...

Where applicable also record: repository, release branch, build identifier,
CI/CD run, artifact checksums, deployment version, container image digest.

## Plan
The plan that initiated the work (PLAN-YYYY-NNNN).

## Project Standards Baseline
| Standards Index | Revision | Applicable Rules | Proposal Validation |
|---|---|---|---|
| `docs/standards/index.md` | | | PASS |

## User Requirements
| Requirement | Source | Released Update | Result |
|---|---|---|---|
| PROP-... R1 | User request, incident or other inciting source | U-01 | PASS |

## Added
New functionality introduced.

## Changed
Existing functionality that was modified.

## Fixed
Bugs or defects resolved.

## Removed
Functionality removed or deprecated.

## Released Updates and Verification
| Update | Inciting User Requirement | What Changed | Do This | Expected Result | Evidence Source |
|---|---|---|---|---|---|
| U-01 | PROP-... R1 | | | | TEST-... / DEMO-... |

## Tests
| Test | Requirement | Result |
|---|---|---|
| TEST-2026-0081 | PLAN-2026-0018 R3 | PASS |

## Validation
Validation report reference and per-requirement status
(PASS / PARTIAL / FAIL / NOT APPLICABLE).

## Measured Results
Required when any proposal or plan contained an optimization or measurable claim.
| Update | Requirement | Metric | Baseline | Expected Threshold | Actual | Result | Test Report and Source |
|---|---|---|---|---|---|---|---|
| U-01 | PROP-... R1 | | | | | PASS | TEST-... / artifact or CI run |

## Incidents
Incidents discovered during implementation and their final status.

## Troubleshooting
Important technical investigations and reusable findings.

## Release Verification Guide and Demo
The `DEMO` document containing the executable per-update verification table and its recorded result.

## Manual
Manual documents created or updated.

## System
System, architecture and API documents created or updated.

## Documentation Impact
| Artifact | Decision | Updated Document or Reason |
|---|---|---|
| Release verification guide / demo | UPDATED | DEMO-... |
| README.md | UPDATED / NOT APPLICABLE | |
| System documentation | UPDATED / NOT APPLICABLE | |
| Architecture documentation | UPDATED / NOT APPLICABLE | |
| API and CLI reference | UPDATED / NOT APPLICABLE | |
| Manual | UPDATED / NOT APPLICABLE | |

## Source Changes
| Source | Change | Reason |
|---|---|---|
| internal/api/server.go | Modified | Add new endpoint |
| config/settings.go | Modified | Add configuration option |
| cmd/vhco/main.go | Modified | Add CLI command |

## Known Issues
Anything remaining unresolved.

## Limitations
Known functional or technical limitations.

## Follow-up Work
Work deferred to a later plan or release.

## Related Documents
## Change History
```

### 33.1 Release traceability

```text
REL-1.4.0
│
├── Plan
│   └── PLAN-2026-0024
│
├── Tests
│   ├── TEST-2026-0081
│   ├── TEST-2026-0082
│   └── TEST-2026-0083
│
├── Validation
│   └── RPT-2026-0055
│
├── Incidents
│   └── INC-2026-0017
│
├── Troubleshooting
│   └── TRBL-2026-0009
│
├── Demo
│   └── DEMO-2026-0024
│
├── Manual
│   ├── MAN-2026-0002 CLI Manual
│   └── MAN-2026-0003 API Manual
│
├── System
│   ├── SYS-2026-0030 API Architecture
│   ├── SYS-2026-0031 Runtime Architecture
│   └── SYS-2026-0032 Configuration System
│
├── Git Tag: v1.4.0
├── Commit: 8f1c42...
│
└── Source
    ├── src/...
    ├── internal/...
    └── config/...
```

The inverse lookup must always be possible: from a release, identify the exact source commit, understand why it exists, see what changed, reproduce its demos and tests, and trace every important decision back to its supporting documentation.

## 34. Release Completion Gate

A release must not be marked complete until every condition is satisfied. This is the single, authoritative gate.

```text
[ ] An approved plan exists in plans/
[ ] The proposal preserves the original user request and traces it through problems, goals and numbered requirements
[ ] Every user-facing requirement has a complete UC-NN use case with inputs, outputs, normal and error journeys, surfaces and planned tests
[ ] Every proposed change and file CRUD row maps to a requirement, and every requirement maps forward to implementation and validation
[ ] The proposal records the standards index revision and passes every applicable active project rule
[ ] Implementation corresponds to the plan, and deviations are documented
[ ] The live plan has no unexplained NOT STARTED, IN PROGRESS, BLOCKED, FAILED or DEFERRED item in release scope
[ ] Unexpected bugs are documented in incidents/
[ ] Significant or reusable technical problems are documented in troubleshooting/
[ ] Relevant tests were completed and recorded in testing/
[ ] Every test links to the plan requirement it validates
[ ] Every optimization or measurable claim has a predeclared baseline, environment, test and threshold
[ ] Every optimization or measurable test was executed and its reproducible report source is linked
[ ] The release summarizes measured results without claiming a failed or unsupported improvement
[ ] A validation report exists in reports/ with a result for every requirement
[ ] Every PARTIAL or FAIL result is documented and referenced
[ ] A release verification guide exists in demos/ with one U-NN row per released update
[ ] Every U-NN row links its inciting user requirement, exact test action, expected result and evidence source
[ ] Demo and release-verification commands, APIs and examples were actually executed and verified
[ ] Manual changes were integrated into the correct book chapters and the reading path remains coherent
[ ] The manual feature catalogue includes every supported addition with why/when-to-use guidance and a verified demo
[ ] Every changed CLI command, API route/event, UI workflow, configuration item and documented error appears in the manual/reference or has a justified NOT APPLICABLE result
[ ] README.md impact was assessed and recorded as UPDATED or NOT APPLICABLE with a reason
[ ] System documentation impact was assessed and recorded as UPDATED or NOT APPLICABLE with a reason
[ ] Architecture documentation impact was assessed and recorded as UPDATED or NOT APPLICABLE with a reason
[ ] API and CLI reference impact was assessed and recorded as UPDATED or NOT APPLICABLE with a reason
[ ] Manual impact was assessed and recorded as UPDATED or NOT APPLICABLE with a reason
[ ] Code and system documentation were checked for consistency
[ ] Documents with review_cycle: on-release were reviewed
[ ] Canonical project version was updated
[ ] All version references are synchronized
[ ] Final release state was committed
[ ] Working tree is clean
[ ] Full release commit SHA was recorded
[ ] Git release tag was created and matches the release version
[ ] Git tag resolves to the recorded release commit
[ ] Commit and tag were pushed to the remote, when applicable
[ ] Known issues and limitations are documented
[ ] Final release document was created in releases/
[ ] Release document contains version, tag and full commit hash
[ ] Release document includes user-requirement traceability, released-update verification and measured-result summaries
[ ] Release document records all six post-release documentation-impact decisions
[ ] Release document links all relevant artifacts
[ ] Release document accurately represents the tagged source state
[ ] Indexes were regenerated
```

## 35. Core Principle

The objective is not to produce documentation after coding. The objective is to maintain a traceable chain of evidence:

```text
Why was this needed?      → research, discussion, proposal, decision
What was planned?         → plan
What was implemented?     → implementation and source changes
What problems occurred?   → incidents
How were they solved?     → troubleshooting
How was it tested?        → tests
Did it satisfy the plan?  → validation report
How can someone use it?   → demo and manual
How does it affect the system? → system documentation
What exactly was released?     → release document, version, commit, tag
```

Someone reviewing the repository months or years later must be able to reconstruct this entire chain without relying on undocumented knowledge.

---

# Appendix A — Example Complete Document

An incident recorded during implementation, following template 12.13 and the metadata rules of sections 5 and 6.

```markdown
---
document_id: INC-2026-0007
title: Returned Call Leaves Previous Call Active
document_type: incident
status: resolved

created_date: 2026-07-13
last_updated: 2026-07-15
document_revision: 2

authors:
  - Oliver Luo

owner: Communications Platform Team

systems:
  - Concio

components:
  - mobile-call-client
  - web-call-client
  - call-state-service

affected_versions:
  from: "2.8.0"
  to: "2.8.3"

applicable_environments:
  - production

audience:
  - mobile-engineers
  - backend-engineers
  - qa-engineers

scope: Call-state synchronization between the mobile client, the web client and the call-state service.

reason: Document a call-state synchronization failure that occurs when a callee returns a call after a completed outgoing call.

related_documents:
  - PLAN-2026-0022
  - TEST-2026-0091
  - RPT-2026-0034
  - REL-2.8.4

supersedes: null
superseded_by: null

tags:
  - calling
  - mobile
  - synchronization
  - incident

confidentiality: internal
review_cycle: on-release
next_review_date: 2026-08-01
---

# Returned Call Leaves Previous Call Active

> **Status:** Resolved
> **Created:** 2026-07-13
> **Last Updated:** 2026-07-15
> **Affected Versions:** 2.8.0–2.8.3
> **Owner:** Communications Platform Team
> **Affected Components:** Mobile Call Client, Web Call Client, Call-State Service

## Incident Summary

After a completed outgoing call, the callee returned the call. The web client showed an incoming call while the mobile client continued to display the previous outgoing call as active.

## Severity

S2 — core calling functionality degraded for affected users.

## Status

Resolved in 2.8.4.

## Discovery Context

Discovered during implementation of PLAN-2026-0022 while exercising call cleanup paths.

## Start Time

2026-07-13T10:15:00+08:00

## End Time

2026-07-13T10:26:00+08:00

## Affected Systems

Concio.

## Affected Versions

2.8.0 – 2.8.3.

## Affected Components

- mobile-call-client
- web-call-client
- call-state-service

## Customer Impact

The user could not answer the returned call from the mobile client and had to clear the stale call state manually.

## Detection

Observed manually during implementation testing; confirmed in the call-state service event log.

## Reproduction Steps

1. Place an outgoing call from the mobile client and complete it.
2. Have the callee return the call within 60 seconds.
3. Observe the mobile client still displaying the previous call as active.

## Timeline

- 2026-07-13T10:15:00+08:00 — Outgoing call started.
- 2026-07-13T10:17:00+08:00 — Outgoing call ended.
- 2026-07-13T10:18:00+08:00 — Callee returned the call.
- 2026-07-13T10:18:00+08:00 — Web client displayed the incoming call.
- 2026-07-13T10:18:00+08:00 — Mobile client displayed the previous call as active.
- 2026-07-13T10:26:00+08:00 — User manually cleared the stale call state.

## Logs and Evidence

Call-state service event log excerpt attached to RPT-2026-0034.

## Source Files

- `mobile/lib/call/call_session_manager.dart`
- `services/call-state/internal/session/cleanup.go`

## Root Cause

The mobile client did not fully clear the previous call session before processing the new incoming call event.

## Contributing Factors

Session cleanup was asynchronous and unordered relative to inbound WebSocket call-state events.

## Resolution

The client now invalidates the previous session before accepting a new call-state event.

## Verifying Tests

| Test | Scenario | Result |
|---|---|---|
| TEST-2026-0091 | Returned call after completed outgoing call | PASS |
| TEST-2026-0092 | Returned call after rejected call | PASS |
| TEST-2026-0093 | Incoming call during cleanup | PASS |
| TEST-2026-0094 | Delayed WebSocket events | PASS |

## Corrective Actions

Session invalidation ordering fixed and covered by regression tests.

## Preventive Actions

Regression suite extended with delayed-event scenarios.

## Owners

Communications Platform Team.

## Remaining Risks

None known for the covered scenarios.

## Lessons Learned

Call-state transitions must be ordered explicitly rather than relying on event arrival order.

## Related Documents

- [Call-State Fix Plan](../plans/plan-2026-0022-call-state-fix.md)
- [Call-State Synchronization Report](../reports/rpt-2026-0034-call-state-synchronization.md)
- [Release 2.8.4](../releases/v2/rel-2.8.4-release-notes.md)

## Change History

| Revision | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-07-13 | Oliver Luo | Initial incident record |
| 2 | 2026-07-15 | Oliver Luo | Added root cause, resolution and verifying tests |
```

---

# Appendix B — Consistency Rules Summary

These are the rules most often broken; automation (section 19) should enforce them.

| # | Rule |
|---|---|
| 1 | All documentation lives under `docs/`; this standard is `docs/DOCUMENTATION.md`. |
| 2 | Folder names are plural: `manuals/`, `runbooks/`, `demos/`, `incidents/`, `releases/`. |
| 3 | ID prefix, `document_type` and directory always agree (section 6.1). |
| 4 | `ADR-NNNN` and `REL-<version>` are the only IDs without a year component. |
| 5 | `status` comes from section 6.3; test and validation outcomes use section 6.4 values only. |
| 6 | Test definitions and execution records live in `testing/`; validation reports and analyses live in `reports/`. |
| 7 | `affected_versions` uses the `from`/`to` range form or `not-applicable`. |
| 8 | Plan requirements are numbered and referenced by tests, validation and the release document. |
| 9 | Each template in section 12 has exactly one file in `templates/` (section 4.25). |
| 10 | There is one release workflow (section 23), one release document structure (section 33) and one completion gate (section 34). |
| 11 | Every document ends with `## Related Documents` and `## Change History`. |
| 12 | Timestamps in incidents, tests and releases include the time zone. |
| 13 | `docs/standards/index.md` is mandatory; proposals record the index and applicable rule revisions used for validation. |
| 14 | Every optimization or measurable claim defines its test and expected threshold before implementation and links its sourced result from the release. |
| 15 | Every release has a verified `DEMO` guide mapping each update to its inciting requirement, exact test action, expected result and evidence. |
| 16 | README, system, architecture and manual impact decisions are explicit; blank decisions fail the release gate. |
| 17 | Manuals explain the current project as a coherent book; release history remains in release documents. |
| 18 | User statements, problems, goals, requirements, use cases, changes, file CRUD, tests, released updates, demos and manuals form one ID-linked chain. |
| 19 | Proposal use cases enumerate every changed CLI, HTTP/event and UI interaction, including normal and error paths, and are the basis of testing. |
| 20 | Implementation plans are live ledgers with exact file/test/doc/release tasks, evidence and current status; open release-scope work blocks completion. |
| 21 | Manuals catalogue every supported feature with why/when-to-use guidance, exact surface workflows, errors, recovery, version applicability and verified demos. |

---

## Change History

| Revision | Date | Author | Change |
|---|---|---|---|
| 3 | 2026-08-27 | Documentation Owner | Added end-to-end user-intent traceability; complete proposal use-case, implementation and file-CRUD contracts; live phased plan ledgers; use-case-derived testing; comprehensive CLI/API/UI manuals; ASCII interaction requirements; and VHCO-assisted interface extraction guidance. |
| 2 | 2026-08-17 | Documentation Owner | Added indexed project standards and proposal validation, predeclared measurable-change tests, per-update release verification guides, explicit post-release documentation decisions and book-like manual requirements. |
| 1 | 2026-08-11 | Documentation Owner | Merged the documentation standard and the release process into a single internally consistent standard; unified IDs, statuses, directories, templates, workflows and completion gate. |

`````

## Related Documents

- [Remediation proposal](../proposals/prop-2026-0001-perch-correctness-security-and-completeness.md)
- [Standards baseline](../standards/index.md)

## Change History

| Revision | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-09-26 | Codex | Preserve provisional source text, identity and provenance. |
