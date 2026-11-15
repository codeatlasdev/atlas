// @ts-nocheck
import { describe, test, expect } from "bun:test";
import { signinPtyRequest, stripAnsi } from "./signinPty.ts";

describe("stripAnsi", () => {
	test("removes color and cursor escapes", () => {
		expect(stripAnsi("\u001b[32mLogged in\u001b[0m")).toBe("Logged in");
		expect(stripAnsi("\u001b[2K\u001b[1GPrompt> ")).toBe("Prompt> ");
	});

	test("removes OSC sequences ended by BEL or ST", () => {
		expect(stripAnsi("\u001b]0;title\u0007hello")).toBe("hello");
		expect(stripAnsi("\u001b]8;;http://x\u001b\\link")).toBe("link");
	});

	test("keeps only what a carriage return left on screen", () => {
		expect(stripAnsi("progress 10%\rprogress 90%")).toBe("progress 90%");
		expect(stripAnsi("line one\nline two")).toBe("line one\nline two");
	});

	test("plain text is untouched", () => {
		expect(stripAnsi("Open https://example.com/device and enter the code")).toBe(
			"Open https://example.com/device and enter the code",
		);
	});
});

describe("signinPtyRequest", () => {
	// Issue #355: the terminal runs the argv Rust built for the server (its port, the agents' PATH), untouched.
	test("opens the pty with exactly the argv it was given, at the root", () => {
		const argv = ["ssh", "-p", "2222", "-t", "atlas-crew@srv", "--", "bash -c 'claude'"];
		expect(signinPtyRequest(argv)).toEqual({ argv, cwd: "/" });
	});
});
