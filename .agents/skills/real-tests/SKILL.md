---
name: real-tests
description: Writing, fixing, or reviewing tests; any bug fix (test first). Real behavior tests, not filler.
---

# Real tests, not filler

## When to use
- Every bug, whether found or reported: **test first**.
- New feature or contract change, especially across a boundary (Rust↔TS, API↔client, file format).
- Reviewing a PR that adds tests or claims coverage.
- A flaky or slow suite, or a refactor of legacy code that has no tests.

## 1. Bug workflow (mandatory order)
1. **Reproduce it in a failing test.** Run the test and watch it go RED for the *same reason* as the incident. A test that never failed proves nothing (Beck, *TDD by Example*).
2. **Find the root cause and its class.** Ask: "where else is the same mistake possible?" Add edge-case tests for that class, such as the sibling fields, the other serializer, the empty list and the duplicate keys.
3. **Fix** at the shared root (one guard in the shared function, not one per caller).
4. **Prove it.** The new tests go GREEN and the whole suite stays green.
5. **Name the test after the behavior**, not the ticket. Add a comment that links the incident:
```rust
// Regression: 1.3.0 crash — Settings>Languages read languageLabels[i] undefined (incident link)
#[test]
fn language_labels_serialize_camel_case_and_align_with_languages() { … }
```
A regression test is a test that pins behavior that used to work or that once broke, so the bug cannot come back silently. The **Beyoncé rule** (Google, *Software Engineering at Google*, ch. 11) says: "if you liked it, you should have put a test on it". Behavior without a test is fair game for someone else to break.

## 2. REAL vs FILLER
**A test is REAL when it:**
- asserts **consumer-visible behavior or a contract**: outputs, persisted state, emitted JSON, errors returned to the caller;
- **would fail on a plausible bug**, such as an off-by-one, a wrong case, a swapped arguments, a missing branch, or wrong precedence;
- is deterministic, fast, independent (no order or shared state), and fails with a clear message;
- targets the risky places: **boundaries** (0, 1, max, empty, unicode), **error paths**, **state transitions** (including illegal ones), **invariants** (len(a)==len(b), round-trip), and **precedence** (local overrides global, flag overrides env).

**A test is FILLER when it:**
- tests wiring or forwarding (`foo()` calls `bar()`) or "mock returns X, so the code returns X";
- uses mocks that echo mocks. Mock only what you own *and* what is slow or nondeterministic. Prefer fakes or real objects (Fowler, *Mocks Aren't Stubs*);
- checks implementation details: private fields, call counts, internal state, CSS classes. These are **change-detector tests**. They break on every refactor and catch no bugs (Google Testing Blog; Dodds, *Testing Implementation Details*);
- is a tautology: it recomputes the expected value with the same code under test;
- has `expect(() => f()).not.toThrow()` as its only assertion;
- is a snapshot of everything. Huge snapshots get rubber-stamped on update (`-u`). Snapshot only small, reviewed, stable outputs;
- exists to hit a coverage number. Coverage shows what is *unexecuted*, not what is *verified*.

Example in TS (bun:test):
```ts
// FILLER
test("saves", () => { const db = { put: mock() }; save(db, x); expect(db.put).toHaveBeenCalled(); });
// REAL
test("save then load round-trips and rejects duplicate ids", async () => {
  const db = new MemoryDb();
  await save(db, { id: "a", n: 1 });
  expect(await load(db, "a")).toEqual({ id: "a", n: 1 });
  await expect(save(db, { id: "a", n: 2 })).rejects.toThrow(/duplicate/);
});
```

## 3. Prove the test is real: make it go red
- **Break the code on purpose.** Flip a `<` to `<=`, drop a branch, rename a serde field. The test MUST fail. If it stays green, it is filler, so fix the test.
- **Run mutation testing** to do this automatically: `cargo mutants -f src/foo.rs` (Rust) and Stryker (`npx stryker run`, TS/JS). Surviving mutants show assertions that are missing.
- When fixing a bug: revert the fix → red, reapply → green. Note this in the PR.

## 4. Cross-boundary contract tests (Rust ↔ TS and similar)
**Rule:** serialize the **real producer output** and validate it against the **consumer's real types or parser**. NEVER hand-write a mock of the other side, because the mock encodes your assumption rather than the producer's actual behavior.

**Atlas incident (1.3.0):**
- Rust serde emitted snake_case `language_labels`, and TS read `languageLabels`.
- The frontend tests used camelCase mocks, so they passed while the app crashed (`t.entry.languageLabels[i]` was undefined).
- A second bug: dedup ran on `languages` but not on `labels`, which broke index pairing (`labels[i]` no longer matched `languages[i]`).

Pattern: the producer writes a fixture, and the consumer consumes that fixture.
```rust
#[test]
fn settings_entry_json_contract() {
    let e = SettingsEntry::new(vec!["rust".into(), "rust".into(), "ts".into()]);
    let v = serde_json::to_value(&e).unwrap();
    assert!(v.get("languageLabels").is_some(), "camelCase expected by apps/desktop");
    let (l, lab) = (v["languages"].as_array().unwrap(), v["languageLabels"].as_array().unwrap());
    assert_eq!(l.len(), lab.len(), "index pairing invariant after dedup");
    std::fs::write("../../apps/desktop/src/lib/__fixtures__/settings_entry.json", v.to_string()).unwrap();
}
```
```ts
import fixture from "#lib/__fixtures__/settings_entry.json";
test("renders real Rust payload", () => {
  const e = parseSettingsEntry(fixture); // the same parser/type guard the app uses
  e.languages.forEach((_, i) => expect(e.languageLabels[i]).toBeString());
});
```
- Commit the fixture. CI fails if regenerating it changes the diff.
- For HTTP or services between teams, use consumer-driven contracts (Pact; Fowler, *Consumer-Driven Contracts*). The consumer publishes its expectations, and the provider verifies them against the real provider.

## 5. Choose the level by risk
| Risk | Level |
|---|---|
| Pure logic, parsers, math, state machines | unit (small: no I/O, ms) |
| Serialization, DB, FS, process, IPC, the boundary between two modules | integration (medium: localhost, real deps) |
| Critical user journey, wiring that only exists when fully assembled | a few e2e |

- Most tests are fast and focused. Integration is the sweet spot for app code (Fowler, *TestPyramid*; Vocke, *The Practical Test Pyramid*; Dodds, *Testing Trophy*).
- Keep e2e tests few: they are slow and flaky, and their failures do not locate the bug (Google, *Just Say No to More End-to-End Tests*).
- Classify tests by **size** (small/medium/large: what resources they may touch), not by name (Google, *Test Sizes*).
- Test the lowest level that can catch the bug. A bug that escaped a level means a test belongs at that level.

## 6. Property-based tests for parsers, encoders, normalizers
Assert invariants over generated inputs (proptest, fast-check, Hypothesis): round-trip, idempotence, never panic, length preservation.
```rust
proptest! { #[test] fn roundtrip(s in ".*") { prop_assert_eq!(decode(&encode(&s)).unwrap(), s); } }
```
```ts
import fc from "fast-check";
test("dedup keeps pairs aligned", () => fc.assert(fc.property(fc.array(fc.string()), xs => {
  const { languages, labels } = dedupPairs(xs, xs.map(x => x.toUpperCase()));
  return languages.length === labels.length && languages.every((l, i) => labels[i] === l.toUpperCase());
})));
```
When a property finds a counterexample, save the shrunk case as an explicit regression test.

## 7. Legacy code: characterization tests
Before refactoring untested code, write tests that pin down **what it does now**, even if that behavior looks wrong (Feathers, *Working Effectively with Legacy Code*). Then refactor. Change behavior only in a separate, intentional step that comes with its own test.

## 8. Flaky-test policy
Common causes (Google, *Flaky Tests at Google and How We Mitigate Them*):
- async waits, sleeps and timeouts
- test order and shared state
- time, timezone, randomness without a seed
- network and ports
- resource leaks and concurrency

The policy:
1. A flaky test is a bug. **Quarantine it immediately**: skip it with an issue link and an owner. Never "just retry".
2. Fix the root cause:
   - inject a clock;
   - seed the RNG;
   - use port 0 or a temp dir per test;
   - await a condition instead of sleeping.
3. Don't leave the test quarantined for long: fix it or delete it, because it has stopped protecting anything.
4. Never make a test pass by loosening an assertion or adding a long sleep.
5. Performance guards never assert a wall-clock budget (`< 500 ms` fails on a loaded machine with nothing regressed). Count operations (spies, proxies, matcher calls, rows rendered), use a virtual clock, await the real event instead of a timer, or bound how the cost grows with the input in the same process (apps/desktop `test/scaling.ts`: CPU time at 8x the size over the cost at 1x). A wall-clock ceiling is only a hang detector (60 s).

## 9. Reviewer checklist
- [ ] Bug fix: a test was shown RED before the fix and GREEN after.
- [ ] Edge cases cover the bug's *class*, not just the reported instance.
- [ ] Assertions target observable behavior or a contract, not mocks or private state.
- [ ] Breaking the code on purpose (or running mutants) makes the test fail.
- [ ] Cross-boundary data uses real producer output, with no hand-written mocks of the other side.
- [ ] Deterministic: no sleep, real clock, unseeded RNG, fixed ports or order dependence.
- [ ] Right level for the risk, with no e2e test where a unit or integration test would do.
- [ ] Name states the behavior, and the comment links the incident.
- [ ] No `not.toThrow`-only tests, mega-snapshots or coverage padding.

## Sources
- Fowler — [TestPyramid](https://martinfowler.com/bliki/TestPyramid.html), [Mocks Aren't Stubs](https://martinfowler.com/articles/mocksArentStubs.html), [UnitTest](https://martinfowler.com/bliki/UnitTest.html), [Consumer-Driven Contracts](https://martinfowler.com/articles/consumerDrivenContracts.html)
- Ham Vocke — [The Practical Test Pyramid](https://martinfowler.com/articles/practical-test-pyramid.html)
- Kent Beck — *Test-Driven Development: By Example* (2002)
- Google Testing Blog — [Test Sizes](https://testing.googleblog.com/2010/12/test-sizes.html), [Just Say No to More End-to-End Tests](https://testing.googleblog.com/2015/04/just-say-no-to-more-end-to-end-tests.html), [Change-Detector Tests Considered Harmful](https://testing.googleblog.com/2015/01/testing-on-toilet-change-detector-tests.html), [Test Behavior, Not Implementation](https://testing.googleblog.com/2013/08/testing-on-toilet-test-behavior-not.html), [Flaky Tests at Google](https://testing.googleblog.com/2016/05/flaky-tests-at-google-and-how-we.html)
- *Software Engineering at Google*, ch. 11 — [Testing Overview (Beyoncé rule)](https://abseil.io/resources/swe-book/html/ch11.html)
- Kent C. Dodds — [Testing Trophy](https://kentcdodds.com/blog/the-testing-trophy-and-testing-classifications), [Testing Implementation Details](https://kentcdodds.com/blog/testing-implementation-details), [Effective Snapshot Testing](https://kentcdodds.com/blog/effective-snapshot-testing)
- Mutation testing — [Stryker](https://stryker-mutator.io/), [cargo-mutants](https://mutants.rs/)
- Property-based testing — [proptest](https://proptest-rs.github.io/proptest/), [fast-check](https://fast-check.dev/), [Hypothesis](https://hypothesis.readthedocs.io/)
- Contracts — [Pact](https://docs.pact.io/)
- Michael Feathers — *Working Effectively with Legacy Code* (characterization tests)
