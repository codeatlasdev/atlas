import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { assertRole, requireAuth, signToken, verifyToken } from "./index";

const JWT_SECRET = "test-jwt-secret-for-atlas-auth-min32";

beforeAll(() => {
	process.env.JWT_SECRET = JWT_SECRET;
});

afterAll(() => {
	delete process.env.JWT_SECRET;
});

describe("signToken / verifyToken", () => {
	test("roundtrip: verify(sign(payload)) returns AuthContext", async () => {
		const token = await signToken({ sub: 1, org: 10, role: "admin", username: "matheus" });
		const auth = await verifyToken(token);
		expect(auth).toEqual({
			userId: 1,
			orgId: 10,
			role: "admin",
			username: "matheus",
		});
	});

	test("token is a valid JWT format (3 dot-separated parts)", async () => {
		const token = await signToken({ sub: 1, org: 1, role: "dev", username: "test" });
		expect(token.split(".")).toHaveLength(3);
	});

	test("returns null for tampered token", async () => {
		const token = await signToken({ sub: 1, org: 1, role: "dev", username: "test" });
		const tampered = `${token.slice(0, -5)}XXXXX`;
		expect(await verifyToken(tampered)).toBeNull();
	});

	test("returns null for garbage input", async () => {
		expect(await verifyToken("not.a.jwt")).toBeNull();
		expect(await verifyToken("")).toBeNull();
		expect(await verifyToken("single")).toBeNull();
	});

	test("returns null for expired token", async () => {
		const key = await crypto.subtle.importKey(
			"raw",
			new TextEncoder().encode(JWT_SECRET).buffer as ArrayBuffer,
			{ name: "HMAC", hash: "SHA-256" },
			false,
			["sign"],
		);
		const header = btoa(JSON.stringify({ alg: "HS256", typ: "JWT" }))
			.replace(/\+/g, "-")
			.replace(/\//g, "_")
			.replace(/=+$/, "");
		const body = btoa(JSON.stringify({ sub: 1, org: 1, role: "dev", username: "test", exp: 1 }))
			.replace(/\+/g, "-")
			.replace(/\//g, "_")
			.replace(/=+$/, "");
		const data = new TextEncoder().encode(`${header}.${body}`);
		const sig = new Uint8Array(
			await crypto.subtle.sign("HMAC", key, data.buffer as ArrayBuffer),
		);
		const sigStr = btoa(String.fromCharCode(...sig))
			.replace(/\+/g, "-")
			.replace(/\//g, "_")
			.replace(/=+$/, "");
		const expired = `${header}.${body}.${sigStr}`;
		expect(await verifyToken(expired)).toBeNull();
	});
});

describe("requireAuth", () => {
	test("extracts token from Bearer header", async () => {
		const token = await signToken({ sub: 5, org: 2, role: "dev", username: "dev1" });
		const auth = await requireAuth(`Bearer ${token}`);
		expect(auth.userId).toBe(5);
		expect(auth.role).toBe("dev");
	});

	test("works without Bearer prefix", async () => {
		const token = await signToken({ sub: 1, org: 1, role: "admin", username: "admin1" });
		const auth = await requireAuth(token);
		expect(auth.userId).toBe(1);
	});

	test("throws Unauthorized for undefined", async () => {
		await expect(requireAuth(undefined)).rejects.toThrow("Unauthorized");
	});

	test("throws Unauthorized for empty string", async () => {
		await expect(requireAuth("")).rejects.toThrow("Unauthorized");
	});

	test("throws Invalid token for bad token", async () => {
		await expect(requireAuth("Bearer bad.token.here")).rejects.toThrow("Invalid token");
	});
});

describe("assertRole", () => {
	const admin = { userId: 1, orgId: 1, role: "admin" as const, username: "admin" };
	const dev = { userId: 2, orgId: 1, role: "dev" as const, username: "dev" };
	const viewer = { userId: 3, orgId: 1, role: "viewer" as const, username: "viewer" };

	test("passes when role matches", () => {
		expect(() => assertRole(admin, "admin")).not.toThrow();
		expect(() => assertRole(dev, "dev")).not.toThrow();
		expect(() => assertRole(viewer, "viewer")).not.toThrow();
	});

	test("passes when role is in allowed list", () => {
		expect(() => assertRole(admin, "admin", "dev")).not.toThrow();
		expect(() => assertRole(dev, "admin", "dev")).not.toThrow();
	});

	test("throws when role not in allowed list", () => {
		expect(() => assertRole(viewer, "admin")).toThrow("Requires role: admin");
		expect(() => assertRole(viewer, "admin", "dev")).toThrow("Requires role: admin or dev");
		expect(() => assertRole(dev, "admin")).toThrow("Requires role: admin");
	});
});
