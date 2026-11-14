// @ts-nocheck
import { describe, test, expect } from "bun:test";
import { crewServerLabel, crewServerTarget, isCrewServerKey, parseSSHTarget } from "./sshTarget.ts";

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

// Incident: after installing on root@server the desktop registered root@server, whose home has no crewmate
// hostd, so creating the first crewmate went nowhere. The crew server runs hostd under its own service user.
describe("crewServerTarget", () => {
	test("swaps the admin user for atlas-crew by default", () => {
		expect(crewServerTarget("root@136.148.72.141")).toBe("atlas-crew@136.148.72.141");
		expect(crewServerTarget("ubuntu@my-server.com")).toBe("atlas-crew@my-server.com");
	});
	test("uses the service user the installer reported (an existing atlas-bot is reused)", () => {
		expect(crewServerTarget("root@my-server.com", "atlas-bot")).toBe("atlas-bot@my-server.com");
	});
	test("keeps IPv6 brackets", () => {
		expect(crewServerTarget("root@[::1]")).toBe("atlas-crew@[::1]");
	});
});

describe("crew service users", () => {
	test("both atlas-crew and atlas-bot keys are crew servers; admin users and local are not", () => {
		expect(isCrewServerKey("ssh:atlas-crew@host")).toBe(true);
		expect(isCrewServerKey("ssh:atlas-bot@host")).toBe(true);
		expect(isCrewServerKey("ssh:root@host")).toBe(false);
		expect(isCrewServerKey("")).toBe(false);
	});
	test("the label drops either service user", () => {
		expect(crewServerLabel("atlas-crew@box")).toBe("box");
		expect(crewServerLabel("atlas-bot@box")).toBe("box");
		expect(crewServerLabel("My server")).toBe("My server");
	});
});
