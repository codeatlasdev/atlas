// The docs in ../docs are written to read well on GitHub (relative `*.md` links).
// On the site those links must point at the rendered pages, and links to other
// repo files (skills, scripts, CHANGELOG, LICENSE…) must point at GitHub.
import { visit } from "unist-util-visit";

const REPO = "https://github.com/codeatlasdev/atlas";

export default function remarkDocsLinks() {
	return (tree) => {
		visit(tree, "link", (node) => {
			const url = node.url;
			if (!url || /^([a-z][a-z0-9+.-]*:|#|\/)/i.test(url)) return;
			const [path, hash = ""] = url.split("#");
			const suffix = hash ? `#${hash}` : "";
			// A sibling doc: foo.md -> /docs/foo/
			const sibling = path.match(/^(?:\.\/)?([a-z0-9-]+)\.md$/i);
			if (sibling) {
				node.url = sibling[1] === "README" ? "/docs/" : `/docs/${sibling[1]}/${suffix}`;
				return;
			}
			// Anything else is relative to docs/ inside the repo: ../skills/x -> repo/blob/main/skills/x
			const parts = path.split("/").filter((p) => p && p !== ".");
			const out = [];
			for (const p of ["docs", ...parts]) {
				if (p === "..") out.pop();
				else out.push(p);
			}
			const repoPath = out.join("/");
			const isDir = path.endsWith("/") || !/\.[a-z0-9]+$/i.test(repoPath);
			node.url = `${REPO}/${isDir ? "tree" : "blob"}/main/${repoPath}${suffix}`;
		});
	};
}
