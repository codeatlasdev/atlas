// The sidebar group titles of docs/_nav.json, as messages. A group that is not listed here fails the build (the same rule as a docs
// page missing from the nav), so a new group cannot reach /pt-br/docs/ in English by accident. References, not calls: the language is
// read when `docsGroupTitle` runs, per page.
import { m } from "./paraglide/messages.js";

const GROUP_TITLES: Record<string, () => string> = {
	"Start here": m.docs_chrome_group_start_here,
	Guides: m.docs_chrome_group_guides,
	Reference: m.docs_chrome_group_reference,
	Help: m.docs_chrome_group_help,
	Project: m.docs_chrome_group_project,
};

export function docsGroupTitle(title: string): string {
	const msg = GROUP_TITLES[title];
	if (!msg) throw new Error(`docs/_nav.json group "${title}" has no message: add docs_chrome_group_* and list it in site/src/lib/docs-chrome.ts`);
	return msg();
}
