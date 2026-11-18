// @ts-nocheck bun test runtime types are not part of the app tsconfig
/**
 * Complexity checks that hold on a loaded machine. A wall-clock budget ("under 500 ms") measures the machine as much
 * as the code: it failed three times in a row under a load average of 181 with nothing regressed. What a regression
 * changes is how the cost GROWS with the input, so these helpers compare the cost at two sizes in the same process:
 *
 * - the clock is CPU time (`process.cpuUsage`, user + system), which does not count the time the scheduler kept the
 *   process off a core;
 * - a sample is the minimum of several runs (noise only ever adds time);
 * - the result is a ratio, so the speed of the machine cancels out.
 *
 * Growing the input `factor` times costs about `factor` times as much for a linear pass and `factor ** 2` for a
 * quadratic one (a regex that backtracks, a string rebuilt per character, a rescan per item): the check bounds the ratio
 * between the two.
 */

const cpuMs = () => {
	const u = process.cpuUsage();
	return (u.user + u.system) / 1000;
};

/** Minimum CPU ms over `runs` samples of `reps` calls of `fn`. */
function sample(fn: () => unknown, reps: number, runs: number): number {
	let best = Infinity;
	for (let r = 0; r < runs; r++) {
		const t0 = cpuMs();
		for (let k = 0; k < reps; k++) fn();
		best = Math.min(best, cpuMs() - t0);
	}
	return best;
}

/**
 * How many times more CPU `prepare(n * factor)()` takes than `prepare(n)()`. `prepare` builds the input (not timed)
 * and returns the operation. The operation is repeated enough times that the small size costs at least `minMs` of CPU,
 * so the clock's resolution does not matter.
 */
export function growth(
	prepare: (n: number) => () => unknown,
	n: number,
	{ factor = 8, runs = 7, minMs = 4 } = {},
): number {
	const small = prepare(n);
	const big = prepare(n * factor);
	small();
	big(); // warm up: JIT, caches
	let reps = 1;
	while (sample(small, reps, 1) < minMs && reps < 1 << 22) reps *= 2;
	return sample(big, reps, runs) / sample(small, reps, runs);
}

/**
 * The bound for "linear": a linear pass grows `factor` times (cache effects and allocation add some), a quadratic one
 * `factor ** 2`. Three times the linear cost is far from both for the default factor of 8.
 */
export const linearBound = (factor = 8) => factor * 3;
