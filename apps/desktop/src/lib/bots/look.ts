/**
 * What a bot looks like: body colour, eye style, hat, tool, outfit and eyewear. Pure data, no DOM,
 * so the editor, the avatar and persisted settings share one definition.
 */

export const BOT_COLORS = [
	"#3d5bff", // Atlas blue
	"#38bdf8",
	"#8b5cf6",
	"#ec4899",
	"#f43f5e",
	"#f97316",
	"#eab308",
	"#22c55e",
	"#14b8a6",
] as const;

export const EYE_STYLES = ["round", "dots", "happy", "visor", "cyclops", "sleepy"] as const;
export type EyeStyle = (typeof EYE_STYLES)[number];

export const HATS = [
	"none",
	"straw",
	"cap",
	"beanie",
	"tophat",
	"crown",
	"party",
	"wizard",
	"headphones",
	"antenna",
	"bicorne",
	"bandana",
] as const;
export type Hat = (typeof HATS)[number];

/** What the crewmate holds in its right hand. Each tool has its own "working" gesture (rig.ts). */
export const TOOLS = [
	"none",
	"compass",
	"spyglass",
	"wrench",
	"quill",
	"magnifier",
	"megaphone",
	"pin",
	"lantern",
] as const;
export type Tool = (typeof TOOLS)[number];

/** Clothing drawn over the lower half of the shell. */
export const OUTFITS = ["none", "scarf", "coat", "apron", "satchel", "tie"] as const;
export type Outfit = (typeof OUTFITS)[number];

/** Eyewear drawn over the eyes (BotFaceGear.svelte); shown at every size. */
export const FACES = ["none", "eyepatch", "monocle", "round-glasses", "sunglasses", "goggles"] as const;
export type Face = (typeof FACES)[number];

export interface BotLook {
	color: string;
	eyes: EyeStyle;
	hat: Hat;
	tool: Tool;
	outfit: Outfit;
	face: Face;
}

export const EYE_LABELS: Record<EyeStyle, string> = {
	round: "Round",
	dots: "Dots",
	happy: "Happy",
	visor: "Visor",
	cyclops: "Cyclops",
	sleepy: "Sleepy",
};

export const HAT_LABELS: Record<Hat, string> = {
	none: "None",
	straw: "Straw hat",
	cap: "Cap",
	beanie: "Beanie",
	tophat: "Top hat",
	crown: "Crown",
	party: "Party",
	wizard: "Wizard",
	headphones: "Headphones",
	antenna: "Antenna",
	bicorne: "Map bicorne",
	bandana: "Bandana",
};

export const TOOL_LABELS: Record<Tool, string> = {
	none: "None",
	compass: "Compass",
	spyglass: "Spyglass",
	wrench: "Wrench",
	quill: "Quill",
	magnifier: "Magnifier",
	megaphone: "Megaphone",
	pin: "Map pin",
	lantern: "Lantern",
};

export const OUTFIT_LABELS: Record<Outfit, string> = {
	none: "None",
	scarf: "Scarf",
	coat: "Captain coat",
	apron: "Tool apron",
	satchel: "Satchel",
	tie: "Tie",
};

export const FACE_LABELS: Record<Face, string> = {
	none: "None",
	eyepatch: "Eyepatch",
	monocle: "Monocle",
	"round-glasses": "Glasses",
	sunglasses: "Sunglasses",
	goggles: "Goggles",
};

/** One-click presets: dress for a job, keep the crewmate's own colour and eyes. */
export const ROLE_KITS: {
	id: string;
	label: string;
	kit: Pick<BotLook, "hat" | "tool" | "outfit" | "face">;
}[] = [
	{ id: "captain", label: "Captain", kit: { hat: "bicorne", tool: "compass", outfit: "coat", face: "none" } },
	{ id: "sales", label: "Sales", kit: { hat: "straw", tool: "pin", outfit: "satchel", face: "none" } },
	{
		id: "reviewer",
		label: "Reviewer",
		kit: { hat: "none", tool: "magnifier", outfit: "tie", face: "round-glasses" },
	},
	{
		id: "lookout",
		label: "Lookout",
		kit: { hat: "bandana", tool: "spyglass", outfit: "none", face: "eyepatch" },
	},
	{ id: "scribe", label: "Scribe", kit: { hat: "beanie", tool: "quill", outfit: "scarf", face: "monocle" } },
	{
		id: "keeper",
		label: "Lighthouse keeper",
		kit: { hat: "wizard", tool: "lantern", outfit: "coat", face: "none" },
	},
];

const HEX = /^#[0-9a-f]{6}$/i;

function hash(seed: string): number {
	let h = 0;
	for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
	return h;
}

/** Stable look for a bot that never customised itself. */
export function defaultLook(seed: string): BotLook {
	return {
		color: BOT_COLORS[hash(seed) % BOT_COLORS.length],
		eyes: "round",
		hat: "straw",
		tool: "none",
		outfit: "none",
		face: "none",
	};
}

/**
 * Persisted/remote looks are untrusted: keep every valid field, replace each
 * invalid one with the seed's default (a bad hat must not reset the colour).
 */
export function normalizeLook(raw: unknown, seed: string): BotLook {
	const d = defaultLook(seed);
	if (!raw || typeof raw !== "object") return d;
	const r = raw as Record<string, unknown>;
	return {
		color: typeof r.color === "string" && HEX.test(r.color) ? r.color.toLowerCase() : d.color,
		eyes: EYE_STYLES.includes(r.eyes as EyeStyle) ? (r.eyes as EyeStyle) : d.eyes,
		hat: HATS.includes(r.hat as Hat) ? (r.hat as Hat) : d.hat,
		tool: TOOLS.includes(r.tool as Tool) ? (r.tool as Tool) : d.tool,
		outfit: OUTFITS.includes(r.outfit as Outfit) ? (r.outfit as Outfit) : d.outfit,
		face: FACES.includes(r.face as Face) ? (r.face as Face) : d.face,
	};
}

export function randomLook(rand: () => number = Math.random): BotLook {
	const pick = <T>(xs: readonly T[]) => xs[Math.min(xs.length - 1, Math.floor(rand() * xs.length))];
	return {
		color: pick(BOT_COLORS),
		eyes: pick(EYE_STYLES),
		hat: pick(HATS),
		tool: pick(TOOLS),
		outfit: pick(OUTFITS),
		face: pick(FACES),
	};
}

/** Mix a #rrggbb colour toward white (amount > 0) or black (amount < 0); amount in [-1, 1]. */
export function shade(hex: string, amount: number): string {
	const a = Math.max(-1, Math.min(1, amount));
	const target = a > 0 ? 255 : 0;
	const n = Number.parseInt(hex.slice(1), 16);
	const ch = (s: number) => {
		const c = (n >> s) & 255;
		return Math.round(c + (target - c) * Math.abs(a))
			.toString(16)
			.padStart(2, "0");
	};
	return `#${ch(16)}${ch(8)}${ch(0)}`;
}
