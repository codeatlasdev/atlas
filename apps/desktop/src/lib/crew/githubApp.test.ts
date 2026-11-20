// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import fixture from "../__fixtures__/crew-github-app.json" with { type: "json" };
import {
	clock,
	installationsSignature,
	installUrl,
	manifestArgs,
	parseManifest,
	parseStatus,
	permissionList,
	publicUrl,
	secondsUntil,
	validOrgLogin,
	webhookState,
} from "./githubApp.ts";

// `crew-github-app.json` is what a real hostd answered (written by `crew::tests::github_app`), so a renamed field on
// either side fails here.

describe("parseStatus of the real replies", () => {
	test("the app keeps its owner, bot identity, permissions and where it is installed", () => {
		const app = parseStatus(fixture.status);
		expect(app).toMatchObject({
			id: 987654,
			slug: "atlas-crew-box",
			name: "atlas-crew-box",
			owner: "acme",
			identity: "atlas-crew-box[bot]",
			createdAt: "2026-11-19T10:00:00Z",
			installationsError: null,
		});
		expect(app.installations).toEqual([{ id: 7, account: "acme", repositorySelection: "selected" }]);
		expect(app.permissions.contents).toBe("write");
		expect(app.permissions.workflows).toBeUndefined();
	});

	test("no app is null, and so is anything that is not an app", () => {
		expect(parseStatus(fixture.status_without_app)).toBeNull();
		expect(parseStatus(null)).toBeNull();
		expect(parseStatus({ app: { id: 1 } })).toBeNull();
	});

	test("an app GitHub could not be asked about carries the reason and no installations", () => {
		const app = parseStatus({
			app: { ...fixture.status.app, installations: [], installations_error: "GitHub answered 502" },
		});
		expect(app.installations).toEqual([]);
		expect(app.installationsError).toBe("GitHub answered 502");
	});

	test("the manifest replies are read for the page and the listener", () => {
		const org = parseManifest(fixture.manifest_org_with_workflows);
		expect(org.postUrl).toBe("https://github.com/organizations/acme/settings/apps/new");
		expect(org.manifest.default_permissions.workflows).toBe("write");
		const user = parseManifest(fixture.manifest_user);
		expect(user.postUrl).toBe("https://github.com/settings/apps/new");
		expect(user.manifest.default_permissions.workflows).toBeUndefined();
		expect(parseManifest({ manifest: {} })).toBeNull();
	});
});

describe("the webhook (#645)", () => {
	test("the status carries the webhook as hostd sends it", () => {
		expect(parseStatus(fixture.status).webhook).toEqual({
			url: null,
			synced: false,
			verifiedInstallations: 0,
			failing: false,
		});
	});

	test("an older hostd without the webhook reads as off", () => {
		const { webhook: _gone, ...older } = fixture.status.app;
		expect(parseStatus({ app: older }).webhook).toEqual({
			url: null,
			synced: false,
			verifiedInstallations: 0,
			failing: false,
		});
	});

	test("the address follows hostd's own rules (crew::store::normalize_public_url)", () => {
		expect(publicUrl("  ")).toEqual({ ok: true, url: null });
		expect(publicUrl(" https://crew.example.com/ ")).toEqual({ ok: true, url: "https://crew.example.com" });
		expect(publicUrl("https://crew.example.com/atlas/")).toEqual({
			ok: true,
			url: "https://crew.example.com/atlas",
		});
		expect(publicUrl("http://crew.example.com")).toEqual({ ok: false, reason: "https" });
		expect(publicUrl("crew.example.com")).toEqual({ ok: false, reason: "url" });
		expect(publicUrl("https://me:pw@crew.example.com")).toEqual({ ok: false, reason: "url" });
		expect(publicUrl("https://crew.example.com/?a=1")).toEqual({ ok: false, reason: "url" });
		expect(publicUrl("https://crew.example.com/#x")).toEqual({ ok: false, reason: "url" });
		expect(publicUrl(`https://crew.example.com/${"a".repeat(200)}`)).toEqual({ ok: false, reason: "long" });
	});

	test("what the card says follows the order hostd decides in", () => {
		const w = { url: "https://x.dev", synced: true, verifiedInstallations: 1, failing: false };
		expect(webhookState({ ...w, url: null })).toBe("off");
		expect(webhookState({ ...w, synced: false })).toBe("unsynced");
		expect(webhookState({ ...w, verifiedInstallations: 0 })).toBe("pending");
		expect(webhookState(w)).toBe("live");
		expect(webhookState({ ...w, failing: true })).toBe("failing");
	});
});

describe("the form's choices", () => {
	test("my account needs nothing else; an organization needs a login hostd accepts", () => {
		expect(manifestArgs("user", "ignored", true)).toEqual({ owner_kind: "user", workflows: true });
		expect(manifestArgs("org", "  acme ", false)).toEqual({
			owner_kind: "org",
			org: "acme",
			workflows: false,
		});
		expect(manifestArgs("org", "", false)).toBeNull();
		expect(manifestArgs("org", "a/b", false)).toBeNull();
	});

	test("a GitHub login is 1 to 39 letters, digits and hyphens", () => {
		expect(validOrgLogin("a")).toBe(true);
		expect(validOrgLogin("a".repeat(39))).toBe(true);
		expect(validOrgLogin("a".repeat(40))).toBe(false);
		expect(validOrgLogin("my org")).toBe(false);
		expect(validOrgLogin("../x")).toBe(false);
	});
});

describe("what the card shows", () => {
	test("the install page of an app, with the slug escaped", () => {
		expect(installUrl("atlas-crew-box")).toBe("https://github.com/apps/atlas-crew-box/installations/new");
		expect(installUrl("a/b")).toBe("https://github.com/apps/a%2Fb/installations/new");
	});

	test("permissions list write before read, each group by name", () => {
		expect(permissionList(parseStatus(fixture.status).permissions)).toEqual([
			{ name: "contents", level: "write" },
			{ name: "issues", level: "write" },
			{ name: "pull_requests", level: "write" },
			{ name: "actions", level: "read" },
			{ name: "checks", level: "read" },
			{ name: "metadata", level: "read" },
		]);
	});

	test("the installations signature changes when an account or its scope changes, not when the order does", () => {
		const base = parseStatus(fixture.status);
		const two = {
			...base,
			installations: [...base.installations, { id: 9, account: "me", repositorySelection: "all" }],
		};
		const swapped = { ...two, installations: [...two.installations].reverse() };
		const widened = { ...base, installations: [{ ...base.installations[0], repositorySelection: "all" }] };
		expect(installationsSignature(two)).toBe(installationsSignature(swapped));
		expect(installationsSignature(base)).not.toBe(installationsSignature(two));
		expect(installationsSignature(base)).not.toBe(installationsSignature(widened));
		expect(installationsSignature(null)).toBe(installationsSignature({ ...base, installations: [] }));
	});

	test("the countdown never goes below zero and reads m:ss", () => {
		expect(secondsUntil(10_000, 9_001)).toBe(1);
		expect(secondsUntil(10_000, 10_500)).toBe(0);
		expect(clock(600)).toBe("10:00");
		expect(clock(65)).toBe("1:05");
		expect(clock(-3)).toBe("0:00");
	});
});
