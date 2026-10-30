/** ⌘-hover on a path in terminal output: underline it only when it is a file that exists in the worktree. */
import { fileTarget } from "./termPath.ts";

/** How long an answer is trusted: a file that exists stays one longer than a miss (a build may create it). */
const HIT_MS = 5000;
const MISS_MS = 2000;
/** Most paths remembered; the oldest go first. */
const MAX = 256;

type Stat = (abs: string) => Promise<{ kind: string }>;

export class FileProbe {
	#stat: Stat;
	#seen = new Map<string, { file: boolean; until: number }>();

	constructor(stat: Stat) {
		this.#stat = stat;
	}

	get size() {
		return this.#seen.size;
	}

	/** Does `token` (`src/a.ts:12:3`) name an existing file inside `root`, resolved from `cwd`? */
	async isFile(token: string, cwd: string | null, root: string): Promise<boolean> {
		const hit = fileTarget(token, cwd, root);
		if (!hit) return false;
		const abs = `${root.replace(/\/+$/, "")}/${hit.path}`;
		const now = Date.now();
		const known = this.#seen.get(abs);
		if (known && known.until > now) return known.file;
		const file = await this.#stat(abs).then(
			(s) => s.kind === "file",
			() => false,
		);
		this.#seen.delete(abs);
		this.#seen.set(abs, { file, until: now + (file ? HIT_MS : MISS_MS) });
		const oldest = this.#seen.size > MAX ? this.#seen.keys().next().value : undefined;
		if (oldest !== undefined) this.#seen.delete(oldest);
		return file;
	}
}
