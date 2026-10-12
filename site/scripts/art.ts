// bun run art --name hero --prompt "..." [--size 1440x810, max 1440 per side] [--seed 42] [--ref a.png]
// Generates art with Magnific (FLUX.2 [pro]) into src/assets/gen/<name>.jpg and records
// prompt + seed in art/prompts.json so every image can be regenerated or varied.
// MAGNIFIC_API_KEY comes from the environment (never printed).
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const API = "https://api.magnific.com/v1/ai/text-to-image/flux-2-pro";
const ROOT = resolve(import.meta.dir, "..");
const usage = 'bun run art --name hero --prompt "..." [--size 1440x810, max 1440 per side] [--seed 42] [--ref file]';

const argv = Bun.argv.slice(2);
const get = (k: string) => {
	const i = argv.indexOf(`--${k}`);
	return i >= 0 ? argv[i + 1] : undefined;
};
const need = (v: string | undefined) => {
	if (!v) {
		console.error(`Usage: ${usage}`);
		process.exit(1);
	}
	return v;
};

function secret(key: string): string {
	const value = process.env[key];
	if (!value) {
		console.error(`Set ${key} in the environment`);
		process.exit(1);
	}
	return value;
}

interface Task {
	data: { error: string | null; generated: string[]; status: string; task_id: string };
}

const name = need(get("name"));
const prompt = need(get("prompt"));
const [width, height] = (get("size") ?? "1440x810").split("x").map(Number);
const seed = Number(get("seed") ?? Math.floor(Math.random() * 2 ** 31));
const refs = (get("ref") ?? "").split(",").filter(Boolean).slice(0, 4);
const headers = { "Content-Type": "application/json", "x-magnific-api-key": secret("MAGNIFIC_API_KEY") };
const inputs = Object.fromEntries(
	refs.map((r, i) => [i === 0 ? "input_image" : `input_image_${i + 1}`, readFileSync(resolve(r)).toString("base64")]),
);

async function call(url: string, init?: RequestInit): Promise<Task> {
	const res = await fetch(url, { headers, ...init });
	if (!res.ok) throw new Error(`Magnific HTTP ${res.status}: ${await res.text()}`);
	return (await res.json()) as Task;
}

async function finished(t: Task["data"]): Promise<Task["data"]> {
	if (t.status !== "CREATED" && t.status !== "IN_PROGRESS") return t;
	await Bun.sleep(2500);
	return finished((await call(`${API}/${t.task_id}`)).data);
}

const created = await call(API, { body: JSON.stringify({ height, prompt, seed, width, ...inputs }), method: "POST" });
const task = await finished(created.data);
const [url] = task.generated;
if (task.status !== "COMPLETED" || !url) throw new Error(`Magnific: ${task.status} ${task.error ?? ""}`);

const dir = join(ROOT, "src/assets/gen");
mkdirSync(dir, { recursive: true });
const file = join(dir, `${name}.jpg`);
writeFileSync(file, Buffer.from(await (await fetch(url)).arrayBuffer()));

const manifest = join(ROOT, "art/prompts.json");
const log = existsSync(manifest) ? JSON.parse(readFileSync(manifest, "utf8")) : {};
log[name] = { height, model: "flux-2-pro", prompt, refs, seed, width };
writeFileSync(manifest, `${JSON.stringify(log, null, "\t")}\n`);
console.log(`ok ${file}`);
