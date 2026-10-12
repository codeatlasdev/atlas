// The layout renders the page title itself, so the first `# Heading` of each doc
// is removed from the body and exposed as `frontmatter.title` when the file has
// no `title:` of its own. Docs stay valid, self-titled markdown on GitHub.
import { toString } from "mdast-util-to-string";

export default function remarkDocsTitle() {
	return (tree, file) => {
		const i = tree.children.findIndex((n) => n.type === "heading" && n.depth === 1);
		if (i === -1) return;
		const title = toString(tree.children[i]).trim();
		const fm = file.data.astro?.frontmatter;
		if (fm && !fm.title) fm.title = title;
		tree.children.splice(i, 1);
	};
}
