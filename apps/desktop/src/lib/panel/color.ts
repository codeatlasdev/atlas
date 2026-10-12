/**
 * Canvas (ECharts) rejects `oklch()`, `color-mix()` and `var()`: resolve any CSS color the
 * browser understands to `#rrggbb` by painting it on a 1×1 canvas. `scope` is the element the
 * custom properties (e.g. `--space-accent`) are inherited from; `key` changes with the theme
 * or space so cached results are dropped.
 */
let ctx: CanvasRenderingContext2D | null = null;
const cache = new Map<string, string>();
let cacheKey = "";

export function hex(css: string, key = "", scope: Element = document.body): string {
	if (key !== cacheKey) {
		cache.clear();
		cacheKey = key;
	}
	const hit = cache.get(css);
	if (hit) return hit;
	ctx ??= document.createElement("canvas").getContext("2d", { willReadFrequently: true });
	if (!ctx) return "#888888";
	const probe = document.createElement("span");
	probe.style.color = css;
	scope.append(probe);
	const resolved = getComputedStyle(probe).color;
	probe.remove();
	ctx.clearRect(0, 0, 1, 1);
	ctx.fillStyle = "#000";
	ctx.fillStyle = resolved;
	ctx.fillRect(0, 0, 1, 1);
	const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
	const out = `#${[r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("")}`;
	cache.set(css, out);
	return out;
}
