# ADR 0001: Policy lives in files hostd owns, separate from crew.json

**Status:** Proposed (2026-11-20)

## Context

Authority today is a permission mode per crewmate (`ask`, `edits`, `bypass`), crew settings in `crew.json`, rails in code and task grants in memory (`crates/atlas-host/src/crew/approval.rs`). The four target repositories need rules such as "never deploy" and "never touch the production host" that the mode cannot express (research/dogfood-targets.md §6).

## Problem

Where do enforceable rules live so that (a) hostd reads them on every tool call, (b) a crewmate or a pull request cannot loosen them, and (c) a hostd rollback does not silently delete them?

## Decision drivers

- A rolled-back hostd rewrites `crew.json` and `crewmate.json` through its own structs and drops unknown fields (research/audit-runtime-authority.md §5).
- Crewmate users must not be able to write their own policy.
- No new database or policy language (RFC §27.3).

## Considered options

1. New fields in `crew.json` and `crewmate.json`.
2. A policy language with its own engine (Rego, Cedar).
3. Rules in the repository only (`.atlas/crew.yaml`).
4. JSON files under `<hostd dir>/crew/policy/` (crew, project, crewmate), owned by hostd, plus restrictions read from the repository that can only tighten.

## Decision

Option 4. `approval::classify` evaluates rails, deny, hold and allow rules, then the mode, then grants, then a person. No policy files means today's behaviour exactly.

## Consequences

- Downgrades keep policies intact on disk; an older hostd ignores them (and so enforces less: documented in the release notes of the version that introduces them).
- One more place to look when a call is held; every hold names its rule in the audit and in the agent's feedback.
- The archive format needs an entry for policy files or must leave them out explicitly (`crates/atlas-host/src/crew/store/archive.rs` `FORMAT_VERSION`).

## Security implications

Policy writes are owner commands only (refused for `crewmate:<id>` origins). An unreadable policy file holds every call it would have allowed. Repository rules can add `deny` and `hold`, never `allow`.

## Migration

None needed: absent files mean current behaviour. Hostd ships defaults for automatic origins (A1 in recommended-architecture.md).

## Open questions

Q7 in open-questions.md (do destructive defaults apply to the owner's own `bypass` tasks).
