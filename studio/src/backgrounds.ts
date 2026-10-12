// `bun studio bg <a|b|c|v|--all>`: the cobalt background stills (Magnific FLUX.2 pro) with the prompts and seeds of the
// 1.0 film, so the field can be regenerated or extended. Stills are registered as `bg:<id>` with prompt and seed in the manifest.

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { registerAsset } from "./lib/assets.ts";
import { parseArgs } from "./lib/films.ts";
import { CACHE, secret, StudioError } from "./lib/paths.ts";

interface Job {
	prompt: string;
	width: number;
	height: number;
	seed: number;
}

const STYLE =
	"saturated electric cobalt blue (#1b33f0) and deep navy (#0b1672) only, no purple, no violet, no magenta, cinematic, ultra clean, soft film grain, no text, no letters, no logos, no people";

/** The 1.0 stills: `a` light field, `b` orbital rings, `c` planet horizon, `v` vertical. Seeds 21-24. */
export const JOBS: Record<string, Job> = {
	a: { prompt: `wide abstract field of smooth cobalt blue light, soft luminous gradients and gentle volumetric glow, a large calm dark-navy area in the center for overlaid UI, tiny fine dust particles, ${STYLE}`, width: 1440, height: 816, seed: 21 },
	b: { prompt: `minimal abstract concentric thin luminous rings around a faint glowing cobalt sphere, depth of field, deep navy background, crisp white-blue rim light, ${STYLE}`, width: 1440, height: 816, seed: 22 },
	c: { prompt: `a planet horizon curve glowing electric cobalt at the bottom edge of the frame, deep navy space above with sparse stars, atmospheric glow, ${STYLE}`, width: 1440, height: 816, seed: 23 },
	v: { prompt: `tall vertical abstract cobalt blue gradient, light beams drifting upward into deep navy, fine dust, empty space in the middle, ${STYLE}`, width: 816, height: 1440, seed: 24 },
};

const API = "https://api.magnific.com/v1/ai/text-to-image/flux-2-pro";

interface Task {
	status: string;
	task_id: string;
	generated?: string[];
	error?: string;
}

async function call(url: string, key: string, init?: RequestInit): Promise<Task> {
	const res = await fetch(url, { ...init, headers: { "Content-Type": "application/json", "x-magnific-api-key": key } });
	if (!res.ok) throw new StudioError(`Magnific ${res.status}`, (await res.text()).slice(0, 300));
	const body: { data: Task } = await res.json();
	return body.data;
}

async function generate(id: string, job: Job, key: string): Promise<void> {
	let t = await call(API, key, { method: "POST", body: JSON.stringify(job) });
	while (t.status === "CREATED" || t.status === "IN_PROGRESS") {
		await Bun.sleep(2500);
		t = await call(`${API}/${t.task_id}`, key);
	}
	const url = t.generated?.[0];
	if (t.status !== "COMPLETED" || !url) throw new StudioError(`bg ${id}: ${t.status} ${t.error ?? ""}`);
	const dir = join(CACHE, "background");
	mkdirSync(dir, { recursive: true });
	const file = join(dir, `cobalt_${id}.jpg`);
	await Bun.write(file, await (await fetch(url)).arrayBuffer());
	await registerAsset(`bg:${id}`, "background", file, "bun studio bg", { generator: "Magnific FLUX.2 pro", prompt: job.prompt, seed: job.seed, size: `${job.width}x${job.height}` });
	console.log(`✓ bg:${id}`);
}

export async function main(argv: string[]): Promise<void> {
	const { pos, flags } = parseArgs(argv);
	const ids = flags.all === true ? Object.keys(JOBS) : pos;
	if (ids.length === 0) throw new StudioError("which still?", `bun studio bg <${Object.keys(JOBS).join("|")}> | --all   (needs MAGNIFIC_API_KEY)`);
	const key = secret("MAGNIFIC_API_KEY");
	for (const id of ids) {
		const job = JOBS[id];
		if (!job) throw new StudioError(`unknown still "${id}"`, `known: ${Object.keys(JOBS).join(", ")}`);
		await generate(id, job, key);
	}
}
