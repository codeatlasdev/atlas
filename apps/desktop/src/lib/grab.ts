/** Element captured by Design Mode (see crates/atlas-desktop/src/browser_grab.js). */
export interface Grab {
	url: string;
	title: string;
	viewport: { width: number; height: number; dpr: number };
	target: {
		tagName: string;
		selector: string;
		htmlSnippet: string;
		textSnippet: string;
		attributes: Record<string, string>;
		computedStyles: Record<string, string>;
		rectViewport: { x: number; y: number; width: number; height: number };
		rectPage: { x: number; y: number; width: number; height: number };
	};
	ancestorPath: string[];
	nearbyText: string[];
}

/**
 * Prompt for the agent. The screenshot is a bare path on its own line: agents
 * read it as an image, and an `@` mention would open omp's file picker and
 * swallow the Enter that submits the prompt.
 */
export function grabPrompt(g: Grab, shot: string | null): string {
	const styles = Object.entries(g.target.computedStyles)
		.filter(([, v]) => v && v !== "none" && v !== "normal" && v !== "auto")
		.map(([k, v]) => `${k}: ${v}`)
		.join("; ");
	return [
		`UI element from ${g.url}:`,
		g.target.selector,
		"```html",
		g.target.htmlSnippet,
		"```",
		`Computed: ${styles}`,
		g.ancestorPath.length ? `Inside: ${g.ancestorPath.join(" < ")}` : "",
		shot ? `Screenshot of the element (read this image file): ${shot}` : "",
	]
		.filter(Boolean)
		.join("\n");
}
