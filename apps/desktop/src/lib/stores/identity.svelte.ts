/**
 * Resolved project icons. User choices come with the project (`ProjectView.identity`); the repo's
 * own logo is detected once per repo through hostd (`fs_read`, so SSH repos work too) and cached
 * as object URLs for the session. Reads are small (≤ 256 KB) and sequential per repo.
 */
import { b64bytes, host } from "#lib/host.ts";
import {
	MAX_LOGO_BYTES,
	detectionPaths,
	mimeOf,
	resolveIcon,
	type ResolvedIcon,
} from "#lib/shell/identity.ts";
import { store } from "#lib/stores/projects.svelte.ts";

/** An image inside `root`: object URL for this window, data URL for the tray popover. */
export interface Logo {
	url: string;
	data: string;
}

/** The image at `root/rel`, or null when missing, too big or not an image. */
async function load(root: string, rel: string): Promise<Logo | null> {
	const type = mimeOf(rel);
	if (!type) return null;
	try {
		const r = await host.fsRead(`${root}/${rel}`, MAX_LOGO_BYTES);
		if (r.truncated || !r.data) return null;
		const url = URL.createObjectURL(new Blob([b64bytes(r.data) as Uint8Array<ArrayBuffer>], { type }));
		return { url, data: `data:${type};base64,${r.data}` };
	} catch {
		return null;
	}
}

/** First existing logo of a repo: package.json `icon`, then the common paths. */
async function detect(root: string): Promise<(Logo & { path: string }) | null> {
	const pkg = await host
		.fsRead(`${root}/package.json`, 64 * 1024)
		.then((r) => new TextDecoder().decode(b64bytes(r.data)))
		.catch(() => null);
	for (const path of detectionPaths(pkg)) {
		const logo = await load(root, path);
		if (logo) return { path, ...logo };
	}
	return null;
}

class Identities {
	/** repo id → detected logo (null = looked, none found). Absent = not looked yet. */
	detected = $state<Record<string, (Logo & { path: string }) | null>>({});
	/** `${repo id}:${path}` → a user-chosen image (null = failed). */
	images = $state<Record<string, Logo | null>>({});
	#pending = new Set<string>();
	/** Repo id whose "Change icon and color…" dialog is open (`IdentityDialog.svelte`). */
	editing = $state<string | null>(null);

	/** Start detection for a repo (once). Safe to call from effects. */
	ensure(repoId: string) {
		const p = store.projects.find((x) => x.id === repoId);
		if (!p || repoId in this.detected || this.#pending.has(repoId)) return;
		this.#pending.add(repoId);
		void detect(p.root).then((hit) => {
			this.detected[repoId] = hit;
			this.#pending.delete(repoId);
		});
	}

	/** Load a user-chosen image (once per path). */
	ensureImage(repoId: string, rel: string) {
		const key = `${repoId}:${rel}`;
		const p = store.projects.find((x) => x.id === repoId);
		if (!p || key in this.images || this.#pending.has(key)) return;
		this.#pending.add(key);
		void load(p.root, rel).then((img) => {
			this.images[key] = img;
			this.#pending.delete(key);
		});
	}

	/** The repo logo found by detection, for the "Use project logo" button (null = none). */
	logoOf(repoId: string) {
		this.ensure(repoId);
		return this.detected[repoId] ?? null;
	}

	/** Data URL of whichever image `iconOf` shows (for the tray popover), else null. */
	dataUrlOf(repoId: string): string | null {
		const c = store.projects.find((x) => x.id === repoId)?.identity?.icon;
		const own = c?.kind === "image" ? this.images[`${repoId}:${c.value}`] : null;
		return (own ?? this.detected[repoId])?.data ?? null;
	}

	/** What to render for a repo: the user's choice, else the detected logo, else a monogram. */
	iconOf(repoId: string): ResolvedIcon {
		const p = store.projects.find((x) => x.id === repoId);
		const c = p?.identity?.icon;
		if (c?.kind === "image") this.ensureImage(repoId, c.value);
		// Detection backs every case that may fall through (no choice, or a missing image).
		if (!c || c.kind === "image") this.ensure(repoId);
		return resolveIcon(
			p?.name ?? "?",
			p?.identity,
			this.detected[repoId]?.url ?? null,
			(rel) => this.images[`${repoId}:${rel}`]?.url ?? null,
		);
	}
}

export const identities = new Identities();
