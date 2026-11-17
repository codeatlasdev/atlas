import { expect, test } from "bun:test";
import { closingRefs, prOfSubject, squashBody, wrongCloses } from "./pr-closes";

const R = "codeatlasdev/atlas";
const key = (n: number) => `${R}#${n}`;

// Incident (2026-11-16): f44c293d, the squash of PR #509, said `Closes #503`; #509 links #501 and #504.
test("a closing keyword for an issue the PR does not link is wrong; linked ones are not", () => {
  const msg = "fix(desktop): x (#509)\n\nCloses #503\nCloses #501";
  expect(wrongCloses(msg, [key(501), key(504)], R)).toEqual([key(503)]);
  expect(wrongCloses("fix: x (#509)\n\nCloses #501\nFixes #504", [key(501), key(504)], R)).toEqual([]);
});

test("every GitHub closing keyword in any case, with a colon, a url or a repo prefix", () => {
  const msg = [
    "CLOSES #1",
    "fixed: #2",
    "Resolves codeatlasdev/atlas#3",
    "resolved https://github.com/codeatlasdev/atlas/issues/4",
    "Fix other/repo#5",
    "closes #1", // duplicate
  ].join("\n");
  expect(closingRefs(msg, R)).toEqual([key(1), key(2), key(3), key(4), "other/repo#5"]);
});

test("mentions that do not close are ignored: refs, prefixes, unrelated words", () => {
  const msg = "see #12, related to #13\nSigned-off-by: A <a@b.dev>\nprefix #14\nthe closest #15 and a fixture #16";
  expect(closingRefs(msg, R)).toEqual([]);
});

test("repo names compare case-insensitively", () => {
  expect(wrongCloses("Closes CodeAtlasDev/Atlas#9", [key(9)], R)).toEqual([]);
});

test("the PR number is the last (#N) of the subject only", () => {
  expect(prOfSubject("fix: handle (#12) in titles (#509)")).toBe(509);
  expect(prOfSubject("fix: handle (#12) in titles")).toBeNull();
  expect(prOfSubject("chore(release): 1.5.0-alpha.26")).toBeNull();
  expect(prOfSubject('Revert "feat: x (#5)" (#6)')).toBe(6);
});

test("squash body: Closes lines from the linked issues in numeric order, then the sign-off", () => {
  const who = { name: "Matheus", email: "m@codeatlas.com.br" };
  expect(squashBody([key(504), key(9), key(501)], R, who)).toBe(
    `Closes #9\nCloses #501\nCloses #504\n\nSigned-off-by: Matheus <m@codeatlas.com.br>`,
  );
  // No linked issues: the body is the sign-off alone, never a guessed `Closes`.
  expect(squashBody([], R, who)).toBe("Signed-off-by: Matheus <m@codeatlas.com.br>");
  // A cross-repo issue keeps its repo.
  expect(squashBody(["other/repo#5", key(1)], R, who)).toContain("Closes #1\nCloses other/repo#5");
});

test("what pr:merge writes always passes the CI matcher", () => {
  const linked = [key(504), key(501), "other/repo#5"];
  const body = squashBody(linked, R, { name: "M", email: "m@codeatlas.com.br" });
  expect(wrongCloses(`fix: x (#509)\n\n${body}`, linked, R)).toEqual([]);
  expect(closingRefs(body, R).sort()).toEqual([...linked].sort());
});
