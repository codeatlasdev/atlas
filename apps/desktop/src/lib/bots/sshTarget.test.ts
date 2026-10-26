// @ts-nocheck
import { describe, test, expect } from "bun:test";
import { botTarget, parseSSHTarget } from "./sshTarget.ts";

describe("parseSSHTarget", () => {
	test("standard target — no port", () => {
		const r = parseSSHTarget("ubuntu@my-server.com");
		expect(r.ok).toBe(true);
		if (r.ok) {
			expect(r.target).toBe("ubuntu@my-server.com");
			expect(r.port).toBe(22);
		}
	});

	test("custom port", () => {
		const r = parseSSHTarget("ubuntu@my-server.com:2222");
		expect(r.ok).toBe(true);
		if (r.ok) {
			expect(r.target).toBe("ubuntu@my-server.com");
			expect(r.port).toBe(2222);
		}
	});

	test("IPv6 in brackets, custom port", () => {
		const r = parseSSHTarget("root@[::1]:2222");
		expect(r.ok).toBe(true);
		if (r.ok) {
			expect(r.target).toBe("root@[::1]");
			expect(r.port).toBe(2222);
		}
	});

	test("IPv6 in brackets, no port", () => {
		const r = parseSSHTarget("root@[::1]");
		expect(r.ok).toBe(true);
		if (r.ok) {
			expect(r.target).toBe("root@[::1]");
			expect(r.port).toBe(22);
		}
	});

	test("missing user → error", () => {
		expect(parseSSHTarget("my-server.com").ok).toBe(false);
		expect(parseSSHTarget("my-server.com").ok).toBe(false);
		const r = parseSSHTarget("my-server.com");
		if (!r.ok) expect(r.error).toMatch(/user/i);
	});

	test("invalid port string → error", () => {
		const r = parseSSHTarget("ubuntu@host:abc");
		expect(r.ok).toBe(false);
		if (!r.ok) expect(r.error).toMatch(/valid port/i);
	});

	test("port 0 → error", () => {
		expect(parseSSHTarget("ubuntu@host:0").ok).toBe(false);
	});

	test("port 65536 → error", () => {
		expect(parseSSHTarget("ubuntu@host:65536").ok).toBe(false);
	});

	test("port 65535 is accepted", () => {
		const r = parseSSHTarget("ubuntu@host:65535");
		expect(r.ok).toBe(true);
		if (r.ok) expect(r.port).toBe(65535);
	});

	test("empty string → error", () => {
		expect(parseSSHTarget("").ok).toBe(false);
	});

	test("trims whitespace", () => {
		const r = parseSSHTarget("  ubuntu@host  ");
		expect(r.ok).toBe(true);
		if (r.ok) expect(r.target).toBe("ubuntu@host");
	});
});

// Incident: after installing on root@server the desktop registered root@server, whose home has no bot
// hostd, so creating the first crewmate went nowhere. The ship lives under the atlas-bot user.
describe("botTarget", () => {
	test("swaps the admin user for atlas-bot", () => {
		expect(botTarget("root@136.148.72.141")).toBe("atlas-bot@136.148.72.141");
		expect(botTarget("ubuntu@my-server.com")).toBe("atlas-bot@my-server.com");
	});
	test("keeps IPv6 brackets", () => {
		expect(botTarget("root@[::1]")).toBe("atlas-bot@[::1]");
	});
});
