// Spawning `gh` (and `git`) for pr-merge.ts and main-closes-check.ts, the same on every OS.
// `GH` replaces the gh binary (tests); a path ending in .ts, .js or .mjs runs through this Bun, so a fake gh is a
// script every OS can run, not a shell file Windows cannot exec.

/** The argv prefix that runs gh. */
export const ghCommand = (): string[] => {
  const gh = process.env.GH ?? "gh";
  return /\.(ts|m?js)$/.test(gh) ? [process.execPath, gh] : [gh];
};

/** Runs a command to completion. A binary that cannot be spawned is code 127 with the reason in `err`, never a throw. */
export async function run(cmd: string[]): Promise<{ code: number; out: string; err: string }> {
  try {
    const p = Bun.spawn(cmd, { stdout: "pipe", stderr: "pipe", stdin: "ignore" });
    const [out, err, code] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text(), p.exited]);
    return { code, out: out.trim(), err: err.trim() };
  } catch (e) {
    return { code: 127, out: "", err: `cannot run ${cmd[0]}: ${e instanceof Error ? e.message : String(e)}` };
  }
}

/** Runs gh and returns stdout; a failure throws with gh's own message. */
export async function ghOut(...args: string[]): Promise<string> {
  const r = await run([...ghCommand(), ...args]);
  if (r.code !== 0) throw new Error(`gh ${args.slice(0, 3).join(" ")} failed: ${r.err || r.out}`);
  return r.out;
}
