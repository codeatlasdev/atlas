/**
 * happy-dom does no layout: every element is 0px tall, so a windowed list (VirtualList) would show a single screenful
 * of nothing, and another suite that stubs `offsetHeight` for the whole process (Terminal.test.ts: 400) would make every
 * row 400px tall. `stubLayout` gives the list a viewport of `viewport` px and its rows `row` px until the returned
 * function puts the real properties back (call it in `afterAll`: the DOM is shared by every test file in the process).
 */
export function stubLayout(viewport: number, row: number): () => void {
	const proto = HTMLElement.prototype;
	const saved = (["clientHeight", "offsetHeight"] as const).map(
		(k) => [k, Object.getOwnPropertyDescriptor(proto, k)] as const,
	);
	Object.defineProperty(proto, "clientHeight", { get: () => viewport, configurable: true });
	Object.defineProperty(proto, "offsetHeight", {
		get(this: HTMLElement) {
			return this.classList.contains("vl-row") ? row : 0;
		},
		configurable: true,
	});
	return () => {
		for (const [k, d] of saved) {
			if (d) Object.defineProperty(proto, k, d);
			else delete (proto as unknown as Record<string, unknown>)[k];
		}
	};
}
