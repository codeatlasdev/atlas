import { expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { run } from "../lib/exec.ts";
import { STUDIO } from "../lib/paths.ts";

// Incident (site media regeneration, 1.5.0-alpha.7): the `mcp` capture timed out waiting for "atlas_status". Since the
// login-shell environment change (cfbf88e) a terminal started by the app gets the PATH of the user's login shell, and the
// scratch HOME has no rc file, so `python3` is the macOS system Python 3.9. mcp-demo.py annotated `params: dict | None`,
// which 3.9 evaluates at def time (TypeError), so the demo died before printing a single line.
// The demo must define itself on the oldest Python a login shell can hand it.
const SYSTEM_PYTHON = "/usr/bin/python3";
const demos = ["mcp-demo.py"]; // memhog.py is a script, not a module of defs: it is covered by the compile check below

test.skipIf(!existsSync(SYSTEM_PYTHON)).each(demos)("%s defines itself under the system Python (3.9), before it ever starts a server", async (file) => {
	const path = join(STUDIO, "src/capture/assets", file);
	// everything above the first rpc call: imports and every def, with the server spawn stubbed out (annotations are evaluated at def time on 3.9)
	const code = [
		"import re",
		`src = open(${JSON.stringify(path)}).read().split('rpc(1, "initialize"')[0]`,
		// the server spawn is a multi-line call: drop it, keep every def
		`src = re.sub(r"p = subprocess\\.Popen\\(.*?\\n\\)\\n", "p = None\\n", src, flags=re.S)`,
		`exec(compile(src, ${JSON.stringify(file)}, "exec"), {"__name__": "demo"})`,
		'print("defined")',
	].join("\n");
	const { code: exit, out } = await run([SYSTEM_PYTHON, "-c", code], { allowFail: true });
	expect(out.trim()).toBe("defined");
	expect(exit).toBe(0);
});

test.skipIf(!existsSync(SYSTEM_PYTHON)).each(["mcp-demo.py", "memhog.py"])("%s compiles on the system Python (3.9): no syntax newer than the oldest interpreter a login shell can yield", async (file) => {
	const path = join(STUDIO, "src/capture/assets", file);
	const { code: exit, out } = await run([SYSTEM_PYTHON, "-c", `compile(open(${JSON.stringify(path)}).read(), ${JSON.stringify(file)}, "exec"); print("ok")`], { allowFail: true });
	expect(out.trim()).toBe("ok");
	expect(exit).toBe(0);
});
