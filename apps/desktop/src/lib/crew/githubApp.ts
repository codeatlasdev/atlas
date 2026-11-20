/**
 * The crew server's GitHub App (#643): what `crew_github_app_status` / `_manifest` answer, read defensively, and the
 * small pure rules the Settings card shows. The wire shape is `crates/atlas-host`'s (`crew/github/app`); the real
 * payloads are `__fixtures__/crew-github-app.json`, written by that code's own test.
 */

/** One place the app is installed. */
export interface AppInstallation {
	id: number;
	account: string;
	/** `all` or `selected` (GitHub's words). */
	repositorySelection: string;
}

export interface GithubApp {
	id: number;
	slug: string;
	name: string;
	/** The login that owns the app. */
	owner: string;
	/** Permission name → `read` | `write` | `admin`, as GitHub names them. */
	permissions: Record<string, string>;
	createdAt: string;
	/** `<slug>[bot]`: who the crew's commits come from. */
	identity: string;
	installations: AppInstallation[];
	/** GitHub could not be asked for the installations (the app itself is still known). */
	installationsError: string | null;
	webhook: AppWebhook;
}

/** `crew_github_app_status.app.webhook` (#645): where GitHub delivers, and how that is going. */
export interface AppWebhook {
	/** The crew's public URL; `null`: no webhook, events come from the 2-minute poll. */
	url: string | null;
	/** GitHub's hook config points at `url`. */
	synced: boolean;
	/** Installations that delivered a correctly signed event: their repos are no longer polled. */
	verifiedInstallations: number;
	/** Deliveries are failing and every repo is polled again. */
	failing: boolean;
}

export type WebhookState = "off" | "unsynced" | "pending" | "live" | "failing";

/** What the card says about the webhook, in the order hostd decides: no URL, GitHub not told, failing, no signed
 * delivery yet, live. */
export function webhookState(w: AppWebhook): WebhookState {
	if (!w.url) return "off";
	if (!w.synced) return "unsynced";
	if (w.failing) return "failing";
	return w.verifiedInstallations > 0 ? "live" : "pending";
}

export type PublicUrl = { ok: true; url: string | null } | { ok: false; reason: "https" | "url" | "long" };

/** The public URL as hostd keeps it (`crew::store::normalize_public_url`), checked here so a refused one is never
 * sent: blank is off; otherwise `https://` with a host and no user, password, query or fragment, at most 200
 * characters, without a trailing slash. hostd checks again and is the authority. */
export function publicUrl(raw: string): PublicUrl {
	const s = raw.trim();
	if (!s) return { ok: true, url: null };
	if (s.length > 200) return { ok: false, reason: "long" };
	let u: URL;
	try {
		u = new URL(s);
	} catch {
		return { ok: false, reason: "url" };
	}
	if (u.protocol !== "https:") return { ok: false, reason: "https" };
	if (!u.hostname || u.username || u.password || u.search || u.hash || s.includes("?") || s.includes("#")) {
		return { ok: false, reason: "url" };
	}
	return { ok: true, url: u.href.replace(/\/+$/, "") };
}

/** The manifest reply: `manifest` goes to the loopback listener untouched, `postUrl` is GitHub's "new app" page. */
export interface ManifestReply {
	manifest: Record<string, unknown>;
	postUrl: string;
}

const isObject = (v: unknown): v is Record<string, unknown> =>
	!!v && typeof v === "object" && !Array.isArray(v);
const str = (v: unknown): string => (typeof v === "string" ? v : "");

/** The `crew_github_app_status` reply: the app, or `null` when none is set up. */
export function parseStatus(raw: unknown): GithubApp | null {
	const app = isObject(raw) ? raw.app : null;
	if (!isObject(app) || !str(app.slug)) return null;
	const identity = isObject(app.identity) ? str(app.identity.name) : "";
	return {
		id: typeof app.id === "number" ? app.id : 0,
		slug: str(app.slug),
		name: str(app.name) || str(app.slug),
		owner: str(app.owner),
		permissions: isObject(app.permissions)
			? Object.fromEntries(
					Object.entries(app.permissions).filter((e): e is [string, string] => typeof e[1] === "string"),
				)
			: {},
		createdAt: str(app.created_at),
		identity: identity || `${str(app.slug)}[bot]`,
		installations: (Array.isArray(app.installations) ? app.installations : []).filter(isObject).map((i) => ({
			id: typeof i.id === "number" ? i.id : 0,
			account: str(i.account),
			repositorySelection: str(i.repository_selection),
		})),
		installationsError: str(app.installations_error) || null,
		webhook: parseWebhook(app.webhook),
	};
}

/** An older hostd sends no `webhook`: it reads as off. */
function parseWebhook(raw: unknown): AppWebhook {
	const w = isObject(raw) ? raw : {};
	return {
		url: str(w.url) || null,
		synced: w.synced === true,
		verifiedInstallations: typeof w.verified_installations === "number" ? w.verified_installations : 0,
		failing: w.failing === true,
	};
}

/** The `crew_github_app_manifest` reply; `null` when it is not what hostd sends. */
export function parseManifest(raw: unknown): ManifestReply | null {
	if (!isObject(raw) || !isObject(raw.manifest) || !str(raw.post_url)) return null;
	return { manifest: raw.manifest, postUrl: str(raw.post_url) };
}

/** Who owns the new app; an organization's login is typed separately. */
export type OwnerKind = "user" | "org";

/** A GitHub login as hostd accepts it for an organization: letters, digits and hyphens, 1 to 39. */
export function validOrgLogin(s: string): boolean {
	return /^[A-Za-z0-9-]{1,39}$/.test(s);
}

/** `crew_github_app_manifest`'s arguments for the form's choices; `null` while an organization is chosen without a valid login. */
export function manifestArgs(
	owner: OwnerKind,
	orgInput: string,
	workflows: boolean,
): { owner_kind: "user" | "org"; org?: string; workflows: boolean } | null {
	if (owner === "user") return { owner_kind: "user", workflows };
	const org = orgInput.trim();
	return validOrgLogin(org) ? { owner_kind: "org", org, workflows } : null;
}

/** GitHub's page to install the app on repositories, or to change which ones it reaches. */
export const installUrl = (slug: string) =>
	`https://github.com/apps/${encodeURIComponent(slug)}/installations/new`;

/** The permissions the card lists: write before read, then by name. */
export function permissionList(permissions: Record<string, string>): { name: string; level: string }[] {
	const rank = (level: string) => (level === "read" ? 1 : 0);
	return Object.entries(permissions)
		.map(([name, level]) => ({ name, level }))
		.sort((a, b) => rank(a.level) - rank(b.level) || a.name.localeCompare(b.name));
}

/** `<slug of the permission>` as a person reads it (`pull_requests` → `pull requests`). */
export const permissionLabel = (name: string) => name.replaceAll("_", " ");

/** What changed about the installations: any account or scope added, removed or edited. */
export function installationsSignature(app: GithubApp | null): string {
	return JSON.stringify(
		(app?.installations ?? []).map((i) => [i.id, i.account, i.repositorySelection]).sort(),
	);
}

/** Seconds left until `deadline` (ms since the epoch), never negative. */
export const secondsUntil = (deadline: number, now: number) =>
	Math.max(0, Math.ceil((deadline - now) / 1000));

/** `m:ss` for a countdown. */
export function clock(seconds: number): string {
	const s = Math.max(0, Math.floor(seconds));
	return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}
