// A fake omp for the crew's tests: speaks omp's `--mode rpc` and loads the REAL generated approval
// extension (`--extension`) the way omp's extension runner does, including the limit omp puts on every
// `tool_call` handler (`extensionHandlers.toolCallTimeoutMs`, default 30000, read from the `--config`
// overlay; on expiry the tool is blocked: "Extension <path> timed out after <ms>ms", as in
// omp 18 `extensibility/extensions/runner.ts`).
//
// A virtual clock makes minutes of omp time cost milliseconds: FAKE_OMP_TIME_SCALE virtual seconds pass per
// real second, for the handler limit and `AbortSignal.timeout` (the extension's own wait included). The
// handler limit and the extension's wait are therefore compared exactly as in production; only the owner's
// answer (the test, in real time) has to be scaled by the same factor.
//
// One prompt = one `bash` tool call (`echo hi`) through the extension; the reply says what happened.
import { copyFileSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import readline from "node:readline";
import { pathToFileURL } from "node:url";

const SCALE = Number(process.env.FAKE_OMP_TIME_SCALE || "1");
// Only the two timers that matter are virtual; `setTimeout` is left alone on purpose: node's fetch (undici) arms
// its own 300 s header timeout with it, which omp's runtime (bun) does not have, and scaling it failed the
// long waits with "fetch failed".
const virtual = (ms) => ms / SCALE;
AbortSignal.timeout = (ms) => {
	const c = new AbortController();
	setTimeout(() => c.abort(new DOMException("The operation timed out.", "TimeoutError")), virtual(ms));
	return c.signal;
};

const arg = (name) => {
	const i = process.argv.indexOf(name);
	return i < 0 ? undefined : process.argv[i + 1];
};

// omp: "Positive finite active-work timeout for extension tool_call handlers; invalid values use 30000ms".
function handlerLimitMs() {
	const overlay = arg("--config");
	const text = overlay ? readFileSync(overlay, "utf8") : "";
	const m = text.match(/^extensionHandlers:[ \t]*\n(?:[ \t]+\S.*\n)*?[ \t]+toolCallTimeoutMs:[ \t]*(\S+)[ \t]*$/m);
	const n = m ? Number(m[1]) : NaN;
	return Number.isFinite(n) && n > 0 ? n : 30_000;
}

const out = (v) => process.stdout.write(`${JSON.stringify(v)}\n`);

const extPath = arg("--extension");
const handlers = [];
if (extPath) {
	const copy = join(mkdtempSync(join(tmpdir(), "fake-omp-")), "extension.mjs");
	copyFileSync(extPath, copy); // the .ts has no type syntax; importing a .mjs works on every node
	const mod = await import(pathToFileURL(copy).href);
	mod.default({ on: (event, handler) => event === "tool_call" && handlers.push(handler) });
}

async function emitToolCall(event) {
	const limit = handlerLimitMs();
	for (const handler of handlers) {
		let timer;
		const expired = new Promise((resolve) => {
			timer = setTimeout(() => resolve("timeout"), virtual(limit));
		});
		let failure;
		const done = Promise.resolve()
			.then(() => handler(event, {}))
			.catch((e) => {
				failure = e;
			});
		const result = await Promise.race([done, expired]);
		clearTimeout(timer);
		if (result === "timeout") return { block: true, reason: `Extension ${extPath} timed out after ${limit}ms` };
		if (failure) return { block: true, reason: `Extension ${extPath} failed: ${failure.message}` };
		if (result?.block) return result;
	}
	return undefined;
}

async function prompt(id) {
	const call = { toolCallId: "c1", toolName: "bash", args: { command: "echo hi" } };
	out({ type: "tool_execution_start", ...call });
	const verdict = await emitToolCall({ type: "tool_call", toolCallId: "c1", toolName: "bash", input: call.args });
	const blocked = verdict?.block === true;
	out({
		type: "tool_execution_end",
		toolCallId: "c1",
		toolName: "bash",
		result: blocked ? verdict.reason : "hi",
		isError: blocked,
	});
	const reply = blocked ? `blocked: ${verdict.reason}` : "ran: echo hi";
	out({ type: "agent_end", messages: [{ role: "assistant", content: reply }] });
	out({ type: "prompt_result", id });
}

const rl = readline.createInterface({ input: process.stdin });
rl.on("line", async (line) => {
	const msg = JSON.parse(line);
	if (msg.type === "prompt") await prompt(msg.id);
	if (msg.type === "get_state") {
		process.stdout.write(
			`${JSON.stringify({ type: "response", id: msg.id, data: { sessionFile: "/s.jsonl" } })}\n`,
			() => process.exit(0),
		);
	}
});
