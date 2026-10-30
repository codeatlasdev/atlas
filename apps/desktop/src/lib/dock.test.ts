// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
	DOCK_REPLY_LINGER_MS,
	EMPTY_DOCK_SNAPSHOT,
	agentLabel,
	buildSnapshot,
	initialState,
	reduce,
	replyOf,
	resolveTarget,
	windowSize,
} from "./dock.ts";

const fixture = (name) => JSON.parse(readFileSync(join(import.meta.dir, "__fixtures__", name), "utf8"));

/** The agents the REAL producer (`buildSnapshot`) makes, the same JSON Rust reads (`dock-snapshot.json`). */
const snap = fixture("dock-snapshot.json");
const [api, web, planner] = snap.agents;

const run = (events, from = initialState()) =>
	events.reduce(
		(acc, ev) => {
			const r = reduce(acc.state, ev);
			return { state: r.state, effects: [...acc.effects, ...r.effects] };
		},
		{ state: from, effects: [] },
	);

describe("resolveTarget: who gets the message", () => {
	test("an explicit chip beats an @mention, the sticky target and recency", () => {
		const r = resolveTarget({ text: "hi @web", chip: api.tabId, sticky: web.tabId, agents: snap.agents });
		expect(r).toEqual({ kind: "target", tabId: api.tabId, text: "hi @web" });
	});

	test("an @mention picks the tab by label, worktree, project or agent kind and is stripped from the text", () => {
		const byWorktree = resolveTarget({ text: "fix login @feat-login please", agents: snap.agents });
		expect(byWorktree).toEqual({ kind: "target", tabId: web.tabId, text: "fix login please" });
		const byProject = resolveTarget({ text: "@billing run tests", agents: snap.agents });
		expect(byProject.tabId).toBe(planner.tabId);
		expect(byProject.text).toBe("run tests");
	});

	test("an @mention with several matches asks, listing exactly the candidates, and keeps the text untouched", () => {
		// `@omp` is the agent kind of two tabs
		const r = resolveTarget({ text: "@omp status?", agents: snap.agents });
		expect(r.kind).toBe("ask");
		expect(r.candidates.map((c) => c.tabId).sort()).toEqual([api.tabId, planner.tabId].sort());
		expect(r.text).toBe("@omp status?");
	});

	test("an @mention that matches nothing is plain text, not an error and not a guess", () => {
		const r = resolveTarget({ text: "mail bob@example.com and @nobody", agents: snap.agents });
		// falls through to the most recently active agent; the text is sent as typed
		expect(r.kind).toBe("target");
		expect(r.text).toBe("mail bob@example.com and @nobody");
	});

	test("an e-mail address is never read as a mention", () => {
		const r = resolveTarget({ text: "ask bob@api.dev", agents: snap.agents });
		expect(r.text).toBe("ask bob@api.dev");
	});

	test("without a chip or mention the sticky (last used) target wins over recency", () => {
		const r = resolveTarget({ text: "go", sticky: planner.tabId, agents: snap.agents });
		expect(r).toEqual({ kind: "target", tabId: planner.tabId, text: "go" });
	});

	test("a sticky target that is gone falls back to the most recently active agent", () => {
		const r = resolveTarget({ text: "go", sticky: "closed-tab", agents: snap.agents });
		expect(r.tabId).toBe(snap.agents[0].tabId);
	});

	test("the most recently active agent is the first of the snapshot (Rust order = recency)", () => {
		const r = resolveTarget({ text: "go", agents: snap.agents });
		expect(r).toEqual({ kind: "target", tabId: snap.agents[0].tabId, text: "go" });
	});

	test("no agent running says so (never launches one by itself)", () => {
		expect(resolveTarget({ text: "go", agents: [] })).toEqual({ kind: "none", text: "go" });
		// a chip for a tab that no longer exists is not a target either
		expect(resolveTarget({ text: "go", chip: "closed-tab", agents: [] }).kind).toBe("none");
	});

	// Incident (first agent-instance run): the pill and the To menu said "No agent running" with one agent live,
	// because a blank composer resolved to `empty` without naming who WOULD receive the next message.
	test("a blank message still names the agent that would receive it (sticky, then most recent)", () => {
		expect(resolveTarget({ text: "", agents: snap.agents })).toEqual({
			kind: "empty",
			tabId: snap.agents[0].tabId,
		});
		expect(resolveTarget({ text: "  ", sticky: planner.tabId, agents: snap.agents })).toEqual({
			kind: "empty",
			tabId: planner.tabId,
		});
		expect(resolveTarget({ text: "", chip: web.tabId, agents: snap.agents })).toEqual({
			kind: "empty",
			tabId: web.tabId,
		});
		expect(resolveTarget({ text: "", agents: [] })).toEqual({ kind: "none", text: "" });
	});

	test("a message that is only a mention has nothing to send", () => {
		const r = resolveTarget({ text: "  @web  ", agents: snap.agents });
		expect(r).toEqual({ kind: "empty", tabId: web.tabId });
		expect(resolveTarget({ text: "   ", agents: snap.agents }).kind).toBe("empty");
	});

	test("an @mention is matched case-insensitively and ignores punctuation after it", () => {
		const r = resolveTarget({ text: "@WEB, run it", agents: snap.agents });
		expect(r).toEqual({ kind: "target", tabId: web.tabId, text: "run it" });
	});
});

describe("agentLabel and the snapshot producer", () => {
	test("a tab reads as `project · agent` with its worktree when it differs from the project", () => {
		expect(agentLabel(api)).toBe("api · omp");
		expect(agentLabel(web)).toBe("web · claude (feat-login)");
	});

	test("buildSnapshot keeps agent tabs with a pty, drops chat tabs, orders by recency and carries last_message", () => {
		const tabs = [
			{ id: "t1", worktreeId: "w1", agent: "omp", pty: "p1", chat: false },
			{ id: "t2", worktreeId: "w1", agent: "claude", pty: "p2" },
			{ id: "t3", worktreeId: "w1", agent: "omp", pty: "p3", chat: true },
			{ id: "t4", worktreeId: "w1", pty: "p4" },
			{ id: "t5", worktreeId: "w1", agent: "codex" },
		];
		const out = buildSnapshot({
			now: 7,
			tabs,
			worktree: () => ({ name: "main", project: "api" }),
			status: (id) =>
				id === "t2" ? { state: "working", last_message: "tests green", updated_at: 5 } : undefined,
			recent: (id) => (id === "t2" ? 0 : 1),
		});
		expect(out.at).toBe(7);
		expect(out.agents.map((a) => a.tabId)).toEqual(["t2", "t1"]);
		expect(out.agents[0]).toMatchObject({
			state: "working",
			lastMessage: "tests green",
			updatedAt: 5,
			pty: "p2",
		});
		expect(out.agents[1].state).toBe("idle");
		expect(out.agents[1].lastMessage).toBeNull();
	});
});

describe("reduce: the dock state machine", () => {
	test("starts as the idle pill", () => {
		expect(initialState().mode).toBe("pill");
	});

	test("the summon key opens the composer once and closes it the second time", () => {
		const open = run([{ type: "summon" }]);
		expect(open.state.mode).toBe("composer");
		expect(open.effects).toContainEqual({ type: "focusComposer" });
		const closed = run([{ type: "summon" }], open.state);
		expect(closed.state.mode).toBe("pill");
	});

	test("Esc closes the composer, drops nothing the user typed until closed, and returns to the pill", () => {
		const typed = run([{ type: "summon" }, { type: "edit", text: "hello" }]);
		expect(typed.state.text).toBe("hello");
		const esc = run([{ type: "esc" }], typed.state);
		expect(esc.state.mode).toBe("pill");
		expect(esc.state.text).toBe("");
	});

	test("Esc in every state ends in the pill and never leaves a pending send", () => {
		for (const mode of ["pill", "composer", "ask", "sending", "replying", "needsInput", "error"]) {
			const from = {
				...initialState(),
				mode,
				text: "x",
				pending: mode === "sending" ? { tabId: "t" } : null,
			};
			const r = reduce(from, { type: "esc" });
			expect(r.state.mode, mode).toBe("pill");
			expect(r.state.pending, mode).toBeNull();
		}
	});

	test("send with a single target goes Sending and asks Rust to deliver, with the mention stripped", () => {
		const r = run([
			{ type: "summon" },
			{ type: "edit", text: "@web run it" },
			{ type: "send", agents: snap.agents },
		]);
		expect(r.state.mode).toBe("sending");
		expect(r.effects).toContainEqual({ type: "deliver", tabId: web.tabId, text: "run it" });
	});

	test("send with an ambiguous mention asks instead of delivering", () => {
		const r = run([
			{ type: "summon" },
			{ type: "edit", text: "@omp status" },
			{ type: "send", agents: snap.agents },
		]);
		expect(r.state.mode).toBe("ask");
		expect(r.state.candidates).toHaveLength(2);
		expect(r.effects.some((e) => e.type === "deliver")).toBe(false);
	});

	test("choosing a candidate delivers to it and remembers it as the sticky target", () => {
		const asked = run([
			{ type: "summon" },
			{ type: "edit", text: "@omp status" },
			{ type: "send", agents: snap.agents },
		]);
		const r = run([{ type: "choose", tabId: planner.tabId }], asked.state);
		expect(r.state.mode).toBe("sending");
		expect(r.state.sticky).toBe(planner.tabId);
		expect(r.effects).toContainEqual({ type: "deliver", tabId: planner.tabId, text: "status" });
	});

	test("sending with no agent running is an inline error with the text kept", () => {
		const r = run([{ type: "summon" }, { type: "edit", text: "go" }, { type: "send", agents: [] }]);
		expect(r.state.mode).toBe("error");
		expect(r.state.text).toBe("go");
		expect(r.state.error).toMatch(/no agent/i);
		expect(r.effects.some((e) => e.type === "deliver")).toBe(false);
	});

	// Pressing Enter on an empty composer must do nothing, also when no agent runs (no "No agent" error for a blank box).
	test("a blank message is never an error, with or without agents", () => {
		for (const agents of [snap.agents, []]) {
			const r = run([{ type: "summon" }, { type: "send", agents }]);
			expect(r.state.mode).toBe("composer");
			expect(r.state.error).toBeNull();
			expect(r.effects.some((e) => e.type === "deliver")).toBe(false);
		}
	});

	test("an empty message is not sent", () => {
		const r = run([{ type: "summon" }, { type: "edit", text: "  " }, { type: "send", agents: snap.agents }]);
		expect(r.state.mode).toBe("composer");
		expect(r.effects.some((e) => e.type === "deliver")).toBe(false);
	});

	test("a delivery that works goes to Replying and clears the text; the target becomes sticky", () => {
		const sending = run([
			{ type: "summon" },
			{ type: "edit", text: "go" },
			{ type: "send", agents: snap.agents },
		]);
		const r = run([{ type: "sendOk" }], sending.state);
		expect(r.state.mode).toBe("replying");
		expect(r.state.text).toBe("");
		expect(r.state.replyTab).toBe(snap.agents[0].tabId);
		expect(r.state.sticky).toBe(snap.agents[0].tabId);
	});

	test("a delivery that fails keeps the text and says why (sendErr never loses what was typed)", () => {
		const sending = run([
			{ type: "summon" },
			{ type: "edit", text: "go" },
			{ type: "send", agents: snap.agents },
		]);
		const r = run([{ type: "sendErr", message: "terminal host not reachable" }], sending.state);
		expect(r.state.mode).toBe("error");
		expect(r.state.text).toBe("go");
		expect(r.state.error).toBe("terminal host not reachable");
	});

	test("editing after an error returns to the composer", () => {
		const failed = run([
			{ type: "summon" },
			{ type: "edit", text: "go" },
			{ type: "send", agents: [] },
			{ type: "edit", text: "go!" },
		]);
		expect(failed.state.mode).toBe("composer");
		expect(failed.state.error).toBeNull();
	});

	test("the reply fades to the pill when the agent is done and the linger passed", () => {
		const replying = { ...initialState(), mode: "replying", replyTab: web.tabId, replySince: 100 };
		const stay = reduce(replying, {
			type: "tick",
			now: 100 + DOCK_REPLY_LINGER_MS - 1,
			agents: [{ ...web, state: "done" }],
		});
		expect(stay.state.mode).toBe("replying");
		const gone = reduce(replying, {
			type: "tick",
			now: 100 + DOCK_REPLY_LINGER_MS + 1,
			agents: [{ ...web, state: "done" }],
		});
		expect(gone.state.mode).toBe("pill");
		// still working: the reply stays however long it takes
		const working = reduce(replying, {
			type: "tick",
			now: 100 + DOCK_REPLY_LINGER_MS * 9,
			agents: [{ ...web, state: "working" }],
		});
		expect(working.state.mode).toBe("replying");
	});

	test("an agent that needs input raises the card even if the dock was idle, and never interrupts typing", () => {
		const waiting = { ...web, state: "waiting", lastMessage: "run rm -rf?" };
		const idle = reduce(initialState(), { type: "agents", agents: [waiting] });
		expect(idle.state.mode).toBe("needsInput");
		expect(idle.state.needs).toBe(web.tabId);
		// the composer is open with text: the card is queued as a badge, the composer stays
		const typing = run([{ type: "summon" }, { type: "edit", text: "half a thought" }]);
		const r = reduce(typing.state, { type: "agents", agents: [waiting] });
		expect(r.state.mode).toBe("composer");
		expect(r.state.text).toBe("half a thought");
		expect(r.state.queuedNeeds).toBe(web.tabId);
	});

	test("the needs-input card goes away on its own once the agent stops waiting", () => {
		const card = { ...initialState(), mode: "needsInput", needs: web.tabId };
		const r = reduce(card, { type: "agents", agents: [{ ...web, state: "working" }] });
		expect(r.state.mode).toBe("pill");
		expect(r.state.needs).toBeNull();
	});

	test("a queued card shows when the composer closes", () => {
		const typing = { ...run([{ type: "summon" }]).state, queuedNeeds: web.tabId };
		const r = reduce(typing, { type: "esc" });
		expect(r.state.mode).toBe("needsInput");
		expect(r.state.needs).toBe(web.tabId);
	});

	test("opening in Atlas asks Rust to focus that tab and returns to the pill", () => {
		const card = { ...initialState(), mode: "needsInput", needs: web.tabId };
		const r = reduce(card, { type: "openInAtlas" });
		expect(r.effects).toContainEqual({ type: "openInAtlas", tabId: web.tabId });
		expect(r.state.mode).toBe("pill");
	});

	test("the pill badge counts the agents that need you", () => {
		const r = reduce(initialState(), {
			type: "agents",
			agents: [{ ...web, state: "waiting" }, { ...api, state: "blocked" }, planner],
		});
		expect(r.state.attention).toBe(2);
	});
});

describe("replyOf: the short preview", () => {
	test("is the agent's last message, trimmed to six lines, for the replying tab only", () => {
		const long = Array.from({ length: 10 }, (_, i) => `line ${i + 1}`).join("\n");
		const out = replyOf({ ...web, lastMessage: long }, { state: "working" });
		expect(out.lines).toHaveLength(6);
		expect(out.lines[0]).toBe("line 5");
		expect(out.lines[5]).toBe("line 10");
	});

	test("says what the agent is doing when it has not written anything yet", () => {
		expect(replyOf({ ...web, lastMessage: null, state: "working" }).lines).toEqual([]);
		expect(replyOf({ ...web, lastMessage: null, state: "working" }).busy).toBe(true);
		expect(replyOf({ ...web, lastMessage: "ok", state: "done" }).busy).toBe(false);
	});
});

describe("attachments: a capture sent to an agent from the quick card", () => {
	const shot = {
		path: "/Users/me/Pictures/Atlas Captures/Screenshot 2026-10-26 at 14.03.22.png",
		kind: "screenshot",
	};
	const clip = { path: "/tmp/clip.mp4", kind: "recording" };

	test("attaching opens the composer from the pill and from a reply, and focuses it", () => {
		for (const from of [{ mode: "pill" }, { mode: "replying" }, { mode: "needsInput" }, { mode: "error" }]) {
			const r = reduce({ ...initialState(), ...from }, { type: "attach", ...shot });
			expect(r.state.mode, from.mode).toBe("composer");
			expect(r.state.attachments).toEqual([shot]);
			expect(r.effects).toEqual([{ type: "focusComposer" }]);
		}
	});

	test("attaching to an open composer keeps what was typed and the chip", () => {
		const typed = run([
			{ type: "summon" },
			{ type: "edit", text: "see this" },
			{ type: "pin", tabId: web.tabId },
		]).state;
		const r = reduce(typed, { type: "attach", ...shot });
		expect(r.state.text).toBe("see this");
		expect(r.state.chip).toBe(web.tabId);
		expect(r.state.attachments).toEqual([shot]);
	});

	test("the same file twice is one chip; two files are two, in order", () => {
		const s = run([
			{ type: "attach", ...shot },
			{ type: "attach", ...shot },
			{ type: "attach", ...clip },
		]).state;
		expect(s.attachments).toEqual([shot, clip]);
	});

	test("detaching removes only that file; detaching an unknown path is a no-op", () => {
		const s = run([
			{ type: "attach", ...shot },
			{ type: "attach", ...clip },
		]).state;
		expect(reduce(s, { type: "detach", path: shot.path }).state.attachments).toEqual([clip]);
		expect(reduce(s, { type: "detach", path: "/nope" }).state.attachments).toEqual([shot, clip]);
	});

	/** A screenshot alone is a complete message: "look at this" needs no words (CleanShot's send-to-chat). */
	test("an attachment alone can be sent; nothing at all still cannot", () => {
		const s = run([{ type: "attach", ...shot }]).state;
		const r = reduce(s, { type: "send", agents: snap.agents });
		expect(r.effects).toHaveLength(1);
		expect(r.effects[0].type).toBe("deliver");
		expect(reduce(initialState(), { type: "send", agents: snap.agents }).effects).toEqual([]);
	});

	/** Agents read an image by path (`@/abs/path.png` in omp and Claude Code); a path with spaces is quoted so it
	 *  stays one token, and the text comes first so the instruction reads before the file. */
	test("the delivered text carries the paths, quoted when they contain spaces, after what was typed", () => {
		const s = run([
			{ type: "summon" },
			{ type: "edit", text: "what is wrong here?" },
			{ type: "attach", ...shot },
			{ type: "attach", ...clip },
		]).state;
		const r = reduce(s, { type: "send", agents: snap.agents });
		expect(r.effects[0].text).toBe(`what is wrong here?\n@"${shot.path}"\n@${clip.path}`);
	});

	test("an attachment alone is delivered as just the paths", () => {
		const r = reduce(run([{ type: "attach", ...clip }]).state, { type: "send", agents: snap.agents });
		expect(r.effects[0].text).toBe("@/tmp/clip.mp4");
	});

	test("a mention still picks the agent and is stripped before the paths are added", () => {
		const s = run([
			{ type: "summon" },
			{ type: "edit", text: "@web look" },
			{ type: "attach", ...shot },
		]).state;
		const r = reduce(s, { type: "send", agents: snap.agents });
		expect(r.effects[0].tabId).toBe(web.tabId);
		expect(r.effects[0].text.startsWith("look\n@")).toBe(true);
	});

	test("an ambiguous mention asks, and the choice delivers with the attachments", () => {
		const s = run([{ type: "summon" }, { type: "edit", text: "@omp hi" }, { type: "attach", ...shot }]).state;
		const asked = reduce(s, { type: "send", agents: snap.agents });
		expect(asked.state.mode).toBe("ask");
		expect(asked.effects).toEqual([]);
		expect(asked.state.attachments).toEqual([shot]);
		const chosen = reduce(asked.state, { type: "choose", tabId: api.tabId });
		expect(chosen.effects[0].tabId).toBe(api.tabId);
		expect(chosen.effects[0].text).toBe(`hi\n@"${shot.path}"`);
	});

	/** Incident class: a failed send must keep what the user had (text AND files), a good one clears both. */
	test("a failed delivery keeps the attachments; a delivered one and Esc clear them", () => {
		const sending = run([
			{ type: "attach", ...shot },
			{ type: "send", agents: snap.agents },
		]).state;
		expect(reduce(sending, { type: "sendErr", message: "gone" }).state.attachments).toEqual([shot]);
		expect(reduce(sending, { type: "sendOk", now: 1 }).state.attachments).toEqual([]);
		expect(reduce(run([{ type: "attach", ...shot }]).state, { type: "esc" }).state.attachments).toEqual([]);
	});

	test("with no agent running an attachment-only send is the same inline error and keeps the files", () => {
		const r = reduce(run([{ type: "attach", ...shot }]).state, { type: "send", agents: [] });
		expect(r.state.mode).toBe("error");
		expect(r.state.attachments).toEqual([shot]);
	});
});

describe("windowSize: the one window grows and shrinks per mode", () => {
	test("each mode has a width and a minimum height, the composer is the widest", () => {
		const sizes = ["pill", "composer", "ask", "sending", "replying", "needsInput", "error"].map((m) => [
			m,
			windowSize(m),
		]);
		for (const [m, s] of sizes) {
			expect(s.width, m).toBeGreaterThan(100);
			expect(s.height, m).toBeGreaterThanOrEqual(36);
		}
		expect(windowSize("composer").width).toBe(520);
		expect(windowSize("pill").height).toBeLessThan(windowSize("composer").height);
	});
});

describe("contract with Rust: dock-snapshot.json", () => {
	test("is exactly what buildSnapshot produces (Rust reads this very file in dock.rs)", () => {
		const tabs = [
			{ id: "tab-api", worktreeId: "wt-api", agent: "omp", pty: "pty-1" },
			{ id: "tab-web", worktreeId: "wt-web", agent: "claude", pty: "pty-2" },
			{ id: "tab-plan", worktreeId: "wt-plan", agent: "omp", pty: "pty-3" },
		];
		const worktrees = {
			"wt-api": { name: "main", project: "api" },
			"wt-web": { name: "feat-login", project: "web" },
			"wt-plan": { name: "main", project: "billing" },
		};
		const status = {
			"tab-api": { state: "working", last_message: "Reading src/routes/login.ts…", updated_at: 1000 },
			"tab-web": { state: "waiting", last_message: "Run `rm -rf node_modules`?", updated_at: 900 },
		};
		const built = buildSnapshot({
			now: 1234,
			tabs,
			worktree: (id) => worktrees[id],
			status: (id) => status[id],
			recent: (id) => tabs.findIndex((t) => t.id === id),
		});
		expect(built).toEqual(snap);
		expect(EMPTY_DOCK_SNAPSHOT.agents).toEqual([]);
	});
});
