// The docs in ../docs are written to read well on GitHub (relative `*.md` links).
// On the site those links must point at the rendered pages, and links to other
// repo files (skills, scripts, CHANGELOG, LICENSE…) must point at GitHub.
import { visit } from "unist-util-visit";

const REPO = "https://github.com/codeatlasdev/atlas";

export default function remarkDocsLinks() {
	return (tree, file) => {
		// A translation (docs/pt-BR/*.md) is authored for GitHub: `../x.md` is the English file (always there), `x.md` the translated
		// one. Both become the pt-BR page of that doc on the site, which exists for every doc (a translation, or the English body with
		// a banner), so no link is ever broken. Other relative paths are relative to docs/pt-BR/.
		const prefix = /[\\/]docs[\\/]pt-BR[\\/]/.test(file.path ?? "") ? "/pt-br" : "";
		const base = prefix ? ["docs", "pt-BR"] : ["docs"];
		visit(tree, "link", (node) => {
			const url = node.url;
			if (!url || /^([a-z][a-z0-9+.-]*:|#|\/)/i.test(url)) return;
			const [path, hash = ""] = url.split("#");
			const suffix = hash ? `#${hash}` : "";
			// A sibling doc: foo.md -> /docs/foo/
			const sibling = path.match(prefix ? /^(?:\.\.\/|\.\/)?([a-z0-9-]+)\.md$/i : /^(?:\.\/)?([a-z0-9-]+)\.md$/i);
			if (sibling) {
				node.url = sibling[1] === "README" ? `${prefix}/docs/` : `${prefix}/docs/${sibling[1]}/${suffix}`;
				return;
			}
			// Anything else is relative to docs/ inside the repo: ../skills/x -> repo/blob/main/skills/x
			const parts = path.split("/").filter((p) => p && p !== ".");
			const out = [];
			for (const p of [...base, ...parts]) {
				if (p === "..") out.pop();
				else out.push(p);
			}
			const repoPath = out.join("/");
			const isDir = path.endsWith("/") || !/\.[a-z0-9]+$/i.test(repoPath);
			node.url = `${REPO}/${isDir ? "tree" : "blob"}/main/${repoPath}${suffix}`;
		});
	};
}
