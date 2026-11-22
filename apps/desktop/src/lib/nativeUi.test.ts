// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterEach, describe, expect, test } from "bun:test";
import { blockFilePickers, openExternalLinks } from "./nativeUi.ts";

const click = (el: Element) => {
	const e = new MouseEvent("click", { bubbles: true, cancelable: true });
	el.dispatchEvent(e);
	return e;
};

afterEach(() => {
	document.body.innerHTML = "";
});

describe("external links", () => {
	const setup = () => {
		const opened: string[] = [];
		openExternalLinks(document, async (url) => void opened.push(url));
		return opened;
	};

	test("a target=_blank web or mail link goes to the browser through the host, once, and the page does not navigate", () => {
		document.body.innerHTML = `<a id="w" href="https://example.com/x" target="_blank"><span id="in">x</span></a>
			<a id="m" href="mailto:a@b.c" target="_blank">m</a>`;
		const opened = setup();
		const e = click(document.getElementById("in")); // a click on the link's content counts
		expect(e.defaultPrevented).toBe(true);
		click(document.getElementById("m"));
		expect(opened).toEqual(["https://example.com/x", "mailto:a@b.c"]);
	});

	test("other links are left alone: no target, a local file, a link a component already handled", () => {
		document.body.innerHTML = `<a id="a" href="https://example.com/">same window</a>
			<a id="f" href="file:///etc/passwd" target="_blank">file</a>
			<a id="h" href="https://example.com/h" target="_blank">handled</a>`;
		const opened = setup();
		document.getElementById("h").addEventListener("click", (e) => e.preventDefault());
		expect(click(document.getElementById("a")).defaultPrevented).toBe(false);
		click(document.getElementById("f"));
		click(document.getElementById("h"));
		expect(opened).toEqual([]);
	});
});

describe("file pickers in an agent instance", () => {
	test("a click that would open the OS panel is cancelled and reported; other inputs are untouched", () => {
		const reports: string[] = [];
		blockFilePickers(document, (m) => reports.push(m));
		document.body.innerHTML = `<label><input id="f" type="file"></label><input id="t" type="text">`;
		let reached = false;
		document.getElementById("f").addEventListener("click", () => (reached = true));
		expect(click(document.getElementById("f")).defaultPrevented).toBe(true);
		expect(reached).toBe(false);
		expect(reports).toHaveLength(1);
		expect(reports[0]).toContain("file picker suppressed");
		expect(click(document.getElementById("t")).defaultPrevented).toBe(false);
		expect(reports).toHaveLength(1);
	});
});
