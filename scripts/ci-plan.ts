// ci-plan.ts: the CI plan job. Prints one `area=true|false` line per area of scripts/checks/map.ts (the job's outputs,
// which gate the jobs of ci.yml) and, on stderr, why each area is on. The diff base comes from the event
// (scripts/lib/base.ts: a pull request's merge-ref first parent, a push's `before`); no base (workflow_dispatch, a new
// branch, no history) runs everything, a docs-only commit runs nothing heavy. The checkout needs fetch-depth: 0.
import { dirname } from "node:path";
import { resolveBase } from "./lib/base.ts";
import { classify } from "./checks/plan.ts";

const base = resolveBase(dirname(import.meta.dir));
// An empty diff is as unknowable as a missing base (an API hiccup, a no-op merge): run everything.
const c = classify(base && base.touched.length > 0 ? base.touched : undefined, "ci");
console.error(base ? `plan: ${base.touched.length} changed file(s) since ${base.ref} (${base.mode})` : "plan: no diff base");
if (c.everything) console.error(`plan: everything runs: ${c.everything}`);
for (const [area, a] of Object.entries(c.areas)) {
	console.log(`${area}=${a.on}`);
	console.error(`  ${area}=${a.on}${a.on ? `  ${a.because.join("; ")}` : ""}`);
}
