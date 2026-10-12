/** Line diff (LCS) as unified-ish text for shiki's `diff` grammar, with 3 lines of context. */
export function diff(a: string, b: string): string {
	const x = a.split("\n");
	const y = b.split("\n");
	const n = x.length;
	const m = y.length;
	const dp = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
	for (let i = n - 1; i >= 0; i--)
		for (let j = m - 1; j >= 0; j--)
			dp[i][j] = x[i] === y[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
	const out: string[] = [];
	let i = 0;
	let j = 0;
	while (i < n || j < m) {
		if (i < n && j < m && x[i] === y[j]) {
			out.push(`  ${x[i]}`);
			i++;
			j++;
		} else if (j < m && (i >= n || dp[i][j + 1] >= dp[i + 1][j])) out.push(`+ ${y[j++]}`);
		else out.push(`- ${x[i++]}`);
	}
	const keep = out.map((_, k) => out.slice(Math.max(0, k - 3), k + 4).some((c) => c[0] !== " "));
	return out
		.map((l, k) => (keep[k] ? l : keep[k - 1] ? "  …" : null))
		.filter((l) => l !== null)
		.join("\n");
}
