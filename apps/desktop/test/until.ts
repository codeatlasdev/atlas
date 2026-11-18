// @ts-nocheck bun test runtime types are not part of the app tsconfig
/**
 * Resolve once `cond` holds: wait for the real event (a layer torn down, a row rendered, a dialog modal) instead of a
 * guessed delay or number of ticks, which is a race on a loaded machine. The 30 s is a hang detector, not a guard: what
 * the test asserts never depends on how long the condition takes.
 */
export async function until(cond: () => unknown, what: string): Promise<void> {
	const giveUp = Date.now() + 30_000;
	while (!cond()) {
		if (Date.now() > giveUp) throw new Error(`never happened: ${what}`);
		await new Promise((r) => setTimeout(r, 0));
	}
}
