// Releases resolved at build time for /download: the primary button (the default channel) and one card per other channel. Asset names carry the version, so a stable
// `/releases/latest/download/<asset>` URL does not exist; the build reads the release's own manifest.json
// (version, date, sha256 and size per asset). If GitHub is unreachable the page still builds and falls back to
// the releases page (`resolved: false`), and the client script resolves the version again at runtime.
import { CHANNELS, DEFAULT_CHANNEL, atomTags, resolve, stableTag as newestStable, type Channel } from "../../functions/_lib/channels";
import { SITE } from "./site";

const REPO = "codeatlasdev/atlas";
const TARGET = "aarch64-apple-darwin";

export interface Asset {
	name: string;
	url: string;
	sha256: string;
	size: number;
}
export interface Release {
	resolved: boolean;
	version: string;
	tag: string;
	/** ISO date (yyyy-mm-dd) */
	date: string;
	notes: string;
	dmg?: Asset;
	cli?: Asset;
}

const fallback: Release = { resolved: false, version: "", tag: "", date: "", notes: SITE.releases };

const timeout = () => AbortSignal.timeout(10_000);

/** The tag `releases/latest` redirects to: the newest build of the default channel (an alpha pre-GA), no API quota, no token. */
async function redirectTag(): Promise<string | null> {
	const head = await fetch(`https://github.com/${REPO}/releases/latest`, { method: "HEAD", redirect: "manual", signal: timeout() });
	const tag = head.headers.get("location")?.match(/\/releases\/tag\/([^/?#]+)$/)?.[1];
	return tag && /^v?[0-9A-Za-z._-]+$/.test(tag) ? tag : null;
}

/** The newest tag without a prerelease label: the feed (paged back with `?after=` behind a run of alphas) and the redirect, which is
 * only a candidate because it is an alpha pre-GA. Same rule as the edge (`stableTag` in functions/_lib/channels.ts). */
async function stableTag(feed: string[]): Promise<string | null> {
	return newestStable(feed, await redirectTag().catch(() => null), async (after) => atomTags(await (await fetch(`https://github.com/${REPO}/releases.atom?after=${encodeURIComponent(after)}`, { signal: timeout() })).text()));
}

/** The release behind `tag` from its own manifest.json; null when it is not published (yet). */
async function releaseOf(tag: string): Promise<Release | null> {
	const res = await fetch(`https://github.com/${REPO}/releases/download/${tag}/manifest.json`, { signal: timeout() });
	if (!res.ok) return null;
	const m = (await res.json()) as { version: string; date: string; assets: Record<string, Record<string, { url: string; sha256: string; size: number }>> };
	const a = m.assets?.[TARGET];
	if (!m.version || !a) return null;
	return {
		resolved: true,
		version: m.version,
		tag,
		date: m.date.slice(0, 10),
		notes: `${SITE.releases.replace("/latest", "")}/tag/${tag}`,
		dmg: a.desktop_dmg && { name: a.desktop_dmg.url.split("/").pop() ?? "", ...a.desktop_dmg },
		cli: a.cli && { name: a.cli.url.split("/").pop() ?? "", ...a.cli },
	};
}

/** The newest published release `channel` accepts, walking down past tags whose release is not published yet (a tag is pushed before
 * its release is built). Same resolution as the edge: `releases.atom` + the stable tag. Never older than stable. */
async function releaseFor(channel: Channel, stable: string | null, feed: string[]): Promise<Release | null> {
	let candidates = feed;
	for (let tag = resolve(channel, candidates, stable); tag; tag = resolve(channel, candidates, stable)) {
		const rel = await releaseOf(tag);
		if (rel) return rel;
		if (tag === stable) return null;
		candidates = candidates.filter((t) => t !== tag);
	}
	return null;
}

async function feedTags(): Promise<string[]> {
	const res = await fetch(`https://github.com/${REPO}/releases.atom`, { signal: timeout() });
	return res.ok ? atomTags(await res.text()) : [];
}

/** The release the primary Download button offers: the DEFAULT channel's (Alpha until a real Stable exists), which is Stable's own
 * release when nothing newer is published. The page then hydrates from /release.json, which resolves the same way at the edge. */
export async function latestRelease(): Promise<Release> {
	try {
		const feed = await feedTags().catch(() => []);
		return (await releaseFor(DEFAULT_CHANNEL, await stableTag(feed), feed)) ?? fallback;
	} catch {
		return fallback;
	}
}

/** What every channel except the default offers at build time. A channel other than Stable with nothing newer than Stable is `null`:
 * the page then says "No release yet" and the edge (/channels.json) fills it in at runtime. Stable is always its own release. */
export async function channelReleases(): Promise<Partial<Record<Channel, Release | null>>> {
	const others = CHANNELS.filter((c) => c !== DEFAULT_CHANNEL);
	const out: Partial<Record<Channel, Release | null>> = Object.fromEntries(others.map((c) => [c, null]));
	try {
		const feed = await feedTags().catch(() => []);
		const stable = await stableTag(feed);
		for (const c of others) {
			const rel = await releaseFor(c, stable, feed);
			out[c] = c !== "stable" && rel?.tag === stable ? null : rel;
		}
	} catch {
		// Offline build: the cards keep their empty state and the edge fills them in.
	}
	return out;
}
