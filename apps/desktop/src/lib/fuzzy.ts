/**
 * Subsequence match score (0 = no match). Matches in `name` outrank matches in
 * `dir`; consecutive and word-start hits score higher, shorter names win ties.
 */
export function fuzzy(query: string, name: string, dir = ""): number {
	const q = query.toLowerCase().replace(/\s+/g, "");
	if (!q) return 1;
	const inName = score(q, name.toLowerCase());
	if (inName) return 1000 + inName - name.length / 100;
	const inAll = score(q, `${dir}/${name}`.toLowerCase());
	return inAll ? inAll - (dir.length + name.length) / 100 : 0;
}

function score(q: string, s: string): number {
	let total = 0;
	let run = 0;
	let j = 0;
	for (let i = 0; i < s.length && j < q.length; i++) {
		if (s[i] !== q[j]) {
			run = 0;
			continue;
		}
		run++;
		total += 1 + run * 2 + (i === 0 || /[\s/\-_.]/.test(s[i - 1]) ? 5 : 0);
		j++;
	}
	return j === q.length ? total : 0;
}

/**
 * Split `text` into runs, flagging the characters that the subsequence match of
 * `query` consumes (the same greedy left-to-right walk `fuzzy` scores). Returns one
 * unmatched run when `query` does not match `text`.
 */
export function highlight(query: string, text: string): { text: string; hit: boolean }[] {
	const q = query.toLowerCase().replace(/\s+/g, "");
	const lower = text.toLowerCase();
	const hits = new Set<number>();
	let j = 0;
	for (let i = 0; i < lower.length && j < q.length; i++) if (lower[i] === q[j]) (hits.add(i), j++);
	if (!q || j < q.length) return [{ text, hit: false }];
	const runs: { text: string; hit: boolean }[] = [];
	for (let i = 0; i < text.length; i++) {
		const hit = hits.has(i);
		const last = runs[runs.length - 1];
		if (last && last.hit === hit) last.text += text[i];
		else runs.push({ text: text[i], hit });
	}
	return runs;
}
