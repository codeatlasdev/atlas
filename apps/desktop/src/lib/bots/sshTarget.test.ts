// @ts-nocheck
import { describe, test, expect } from "bun:test";
import { parseSSHTarget } from "./sshTarget.ts";

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
