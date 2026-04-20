import { afterEach, beforeEach, describe, expect, test } from "bun:test";

// panelEnv uses a singleton, so we need to re-import fresh for each test
// We test createEnv behavior through panelEnv

const originalNodeEnv = process.env.NODE_ENV;

describe("panelEnv", () => {
	beforeEach(() => {
		// bun test sets NODE_ENV=test which fails validation — reset to valid value
		delete process.env.NODE_ENV;
	});

	afterEach(() => {
		// Reset module cache to clear singleton between tests
		delete require.cache[require.resolve("./index")];
		// Restore env vars to avoid pollution between tests
		process.env.NODE_ENV = originalNodeEnv;
		delete process.env.PORT;
		delete process.env.PANEL_URL;
		delete process.env.GITHUB_CLIENT_ID;
		delete process.env.GITHUB_CLIENT_SECRET;
	});

	test("returns defaults when no env vars set", async () => {
		const { panelEnv } = await import("./index");
		const env = panelEnv();
		expect(env.PORT).toBe(3100);
		expect(env.DATABASE_URL).toBe("postgres://atlas:atlas@localhost:5435/atlas_panel");
		expect(env.JWT_SECRET).toBe("atlas-dev-secret-change-in-production");
		expect(env.ENCRYPTION_KEY).toBe("atlas-dev-secret-change-in-production");
		expect(env.NODE_ENV).toBe("development");
		expect(env.PANEL_URL).toBeUndefined();
		expect(env.GITHUB_CLIENT_ID).toBeUndefined();
		expect(env.GITHUB_CLIENT_SECRET).toBeUndefined();
	});

	test("reads PORT from env and coerces to number", async () => {
		process.env.PORT = "4000";
		const { panelEnv } = await import("./index");
		expect(panelEnv().PORT).toBe(4000);
	});

	test("reads optional vars when set", async () => {
		process.env.PANEL_URL = "https://panel.example.com";
		process.env.GITHUB_CLIENT_ID = "gh-id-123";
		const { panelEnv } = await import("./index");
		const env = panelEnv();
		expect(env.PANEL_URL).toBe("https://panel.example.com");
		expect(env.GITHUB_CLIENT_ID).toBe("gh-id-123");
	});

	test("validates NODE_ENV enum", async () => {
		process.env.NODE_ENV = "production";
		const { panelEnv } = await import("./index");
		expect(panelEnv().NODE_ENV).toBe("production");
	});

	test("rejects invalid NODE_ENV", async () => {
		process.env.NODE_ENV = "staging";
		const { panelEnv } = await import("./index");
		expect(() => panelEnv()).toThrow("Invalid environment variables");
	});

	test("singleton: returns same instance on repeated calls", async () => {
		const { panelEnv } = await import("./index");
		const a = panelEnv();
		const b = panelEnv();
		expect(a).toBe(b);
	});

	test("error message includes field name", async () => {
		process.env.NODE_ENV = "invalid";
		const { panelEnv } = await import("./index");
		expect(() => panelEnv()).toThrow("NODE_ENV");
	});
});
