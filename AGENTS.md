# AGENTS.md - Project Schema

This is **frontbox**: an offline-first cache and mutation outbox for Rust frontends.

The name is the backend "outbox pattern" moved to the client. The library is a durable local
queue of writes that survives restarts, replays when connectivity returns, and routes server
refusals into dead letters instead of losing them — plus local read models and version-based
cache invalidation.

It is being extracted from RepForge's Dioxus application into a framework-neutral core with
framework and storage adapters. The core deliberately carries no Dioxus dependency.

**Current stage: D1 through D5 and D4d are built.** D1 2026-08-26, D2 2026-08-27, D3a/D3b/D4a
2026-08-29, D4b/D4c/D5 2026-08-30, **D4d 2026-08-31** — and D5's last owed proof line was met the
same day: the two-realm drain is observed by a browser fixture rather than argued from a
single-realm test. Every *deliverable* with a plan is built and none is carrying an unwitnessed
promise. **D6** has no plan. `wiki/plans/queued-write-coalescing.plan.md` sits outside the D0-D6
sequence — it answers an external request rather than a roadmap step — and was built 2026-09-05
(decision 044), then reviewed twice the same day. The first review found a real safety bypass, now
conformance case 80. The second found no defect and a public contract that had not caught up with
the first fix; the eligibility rule is the durable `transport_started` mark with both of its writers
named, and conformance covers cases 70-82.

The repository is a Cargo workspace: the root package is `frontbox`, plus `crates/frontbox-dioxus`,
`crates/frontbox-sqlite`, `crates/frontbox-indexeddb`, and the four trial crates
`examples/todo-core`, `examples/todo-server`, `examples/user-server`, `examples/todo-app`. `src/`
deliberately did not move into `crates/`, because the wiki cites `src/<file>.rs:<line>` in hundreds
of places. Design lives in `wiki/` as specs, proposals, and decisions.

Do not begin work on a later deliverable — D6, or anything else the roadmap has not marked built —
without an explicit go-ahead from the user.

Maintenance of what is already built does not need one. **Keep this paragraph and the roadmap's
Sequencing Principle in step with the roadmap's per-deliverable Status lines**: this one named D4b
and D5 as unauthorized for two days after both shipped, which meant the gate was blocking work
already done and the schema was misdescribing its own project.

**`examples/todo-server` must not depend on `frontbox`.** It hand-writes the wire shapes from the
spec, which is what makes the trial's observation and regression tests a wire-format oracle rather
than a serde round-trip. Importing the client's types would be one line and would delete the
evidence.

**`examples/todo-core` must not depend on Dioxus or `frontbox-dioxus`.** It is the
framework-neutral application half of the trial. If the application logic needs the adapter, the
adapter is leaking and the trial stops proving the boundary.

**Maintenance** — go ahead:

- fixing a defect;
- tightening a doc comment;
- adding a test, including a new conformance case;
- splitting a file that has grown too long, with no change in behaviour;
- correcting something a review found wrong.

**Design change** — needs the same go-ahead as a new deliverable:

- adding a public type, trait, method, field, enum variant, or exported macro;
- adding or removing a feature flag;
- changing what an existing public item means, even where the signature is unchanged.

The public surface is what decisions 001-010 exist to pin down, which is why the second list is
gated and the first is not. If a fix seems to require an API change, say so and stop rather than
making it.

## Code Shape

- **Keep files under ~400 lines.** A soft limit, applied by splitting a module into a directory with
  a facade `mod.rs` rather than by deleting content. The public paths must not move: a reader and a
  `use` statement should not be able to tell that `cases::case_01_applied_is_deleted` lives in
  `cases/status.rs`.
- **Keep total coverage at or above 80%.** `scripts/verify.sh` gates it with `cargo llvm-cov`. It is
  a floor on the crate, not a per-file rule — a module of trait declarations has nothing to execute,
  and a test written to move that number would be asserting the compiler works.

`./scripts/verify.sh` runs every gate the roadmap requires. Run it before claiming anything works.

**Every gate names the package it means.** A bare `cargo clippy`, `cargo test`, or `cargo llvm-cov`
in a workspace silently changes which crates it covers, so a new gate must say `-p frontbox` or
`-p frontbox-dioxus`. The coverage floor is on `-p frontbox` alone: the conformance suite cannot
reach the adapter, and averaging one number across both would let a regression in either hide
behind the other.

## Agent Role

You own `wiki/`. You write, update, cross-link, and maintain all wiki content.
Humans curate `raw/` and make judgment calls. You handle the bookkeeping.

## How To Orient

When the host exposes the LLM Wiki MCP server, it is your primary surface for
wiki operations — prefer it over shell-family file reads and search:

- Read `wiki/` and `raw/` pages with the `llm_wiki_read` MCP tool (not `cat`,
  `sed`, or a shell read on those paths).
- Find pages with the `llm_wiki_search` MCP tool, and `llm_wiki_search_all` for
  cross-project search, instead of grepping the filesystem.

(On a developer test instance these tools carry a `_test` suffix, e.g.
`llm_wiki_search_test`.) If the MCP server is not configured, fall back to the
index-and-read steps below.

1. Read `project_guidelines.md` for the documentation model and rules.
2. Read `wiki/index.md` for the catalog of all project knowledge.
3. Read specific wiki pages identified from the index (via `llm_wiki_read` when
   the MCP server is available).
4. Read `raw/` sources only when wiki content is insufficient.

Never browse the filesystem to find information. `wiki/index.md` is your
entry point.

## Operations

### Ingest

When new material appears in `raw/`:

1. Read the raw source fully.
2. Identify facts, entities, relationships, decisions.
3. Write or update wiki pages using the correct document type.
4. Check for contradictions with existing wiki content.
5. Update `wiki/index.md`.
6. Append to `wiki/log.md`.

### Query

When answering questions:

1. Search with the `llm_wiki_search` MCP tool first — it ranks across the whole
   project. Read `wiki/index.md` for orientation when the MCP server is
   unavailable.
2. Read the matching pages with `llm_wiki_read` (or directly when no MCP server
   is configured).
3. Synthesize an answer with citations.
4. If the answer is durable new knowledge, file it as a wiki page.

### Lint

Periodically or on request:

1. Scan for contradictions between pages.
2. Find stale statuses or outdated claims.
3. Identify orphan pages not linked from index.
4. Check for missing cross-references.
5. Fix issues directly.
6. Log all changes in `wiki/log.md`.

## Conventions

- Document types: spec, decision, proposal, roadmap, plan, checklist,
  reference.
- Pack-specific document types are listed below when active packs add them.
- Use the type by role, not convenience. See `project_guidelines.md`.
- Every wiki page has a metadata block: Document Class, Status, Date,
  Category, Scope, Sources, and Related when useful.
- Filenames: `[slug].type.md` or `[index]-[slug].type.md`.
- Archived documents go to `wiki/archive/`.
- `wiki/log.md` uses format: `## [YYYY-MM-DD] operation | subject`.

## Pack Document Types

| Document type | Filename suffix | Folder |
| --- | --- | --- |
| API Spec | `api.md` | `wiki/apis` |
| Compatibility Note | `compat.md` | `wiki/compatibility` |

## API Pack

- Track API contracts in `wiki/apis/` using `*.api.md` files.
- Store source schemas, OpenAPI files, and vendor references under `raw/api/`.
- Record stability, compatibility, and deprecation notes when API behavior changes.

## Library Pack

- Track public API compatibility in `wiki/compatibility/`.
- Keep runnable examples under `examples/`.
- Record changelog, migration, and semver-impact notes when public behavior changes.

## Code Pack

- Keep application or tool source under `src/`.
- Keep automated tests under `tests/`.
- Keep project utilities and automation under `scripts/`.
- Keep infrastructure definitions under `infra/`.
