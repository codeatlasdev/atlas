// Frozen messages (rule `frozen`, error): a message function evaluated once, at module scope, in a class field or in the
// root layout's script, keeps the language of that moment after a live language switch. See hardcoded/frozen.ts.
import { FROZEN_ROOTS, scanFrozen } from "../hardcoded/frozen.ts";
import type { Checker, Finding } from "../types.ts";
import { existsSync } from "node:fs";
import { join } from "node:path";

const FIX =
	"it runs once and keeps the language it was first called in; call it where it is read: a getter (`get label() { return m.x(); }`), a thunk (`label: () => m.x()`), `$derived`, or the template; English-only machine text: `// i18n-allow frozen: <why>`";

const checker: Checker = {
	name: "frozen",
	async run(ctx) {
		const out: Finding[] = [];
		for (const { surface, dir } of FROZEN_ROOTS) {
			if ((ctx.surface && ctx.surface !== surface) || !existsSync(join(ctx.root, dir))) continue;
			const { found, unparsable } = scanFrozen(ctx.root, [dir]);
			for (const u of unparsable) out.push({ rule: "frozen", severity: "error", file: u.file, message: `cannot parse for the frozen-message scan: ${u.message}` });
			for (const [file, violations] of found) {
				for (const v of violations) out.push({ rule: "frozen", severity: "error", file, line: v.line, message: `message called at load time (${v.text}): ${FIX}` });
			}
		}
		return out;
	},
};

export default checker;
