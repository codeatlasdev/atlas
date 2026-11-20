// Open Graph share images: which image a page gets, and the layout of one. Pure (no `astro:` or Paraglide imports, no fonts or
// rasterizer) so bun test pins the mapping; the cards' words come from src/lib/og-cards.ts and the rendering from og-render.ts.
// The image of a page lives at /og/<locale>/<key>.png: `<locale>` is the URL segment of its language (`en`, `pt-br`), `<key>` is
// `home`, `download`, `changelog`, `privacy`, `code-signing-policy` or `docs/<slug>`.
import { delocalizePath, LOCALIZED_PAGES, URL_SEGMENT, type Locale } from "./i18n";

export const OG_WIDTH = 1200;
export const OG_HEIGHT = 630;

/** The key of each page of LOCALIZED_PAGES. */
const PAGE_KEYS = Object.fromEntries(LOCALIZED_PAGES.map((p) => [p, p === "/" ? "home" : p.slice(1, -1)])) as Record<string, string>;

/** The `<locale>` segment of an image URL (English has no URL segment of its own, but its images still need a folder). */
export const ogSegment = (locale: Locale): string => URL_SEGMENT[locale] || locale;

/** The key of the image of a page, from its path without the locale segment. A path with no image of its own gets the home image. */
export function ogKeyOf(path: string): string {
	const known = PAGE_KEYS[path];
	if (known) return known;
	const slug = path.match(/^\/docs\/([^/]+)\/$/)?.[1];
	return slug ? `docs/${slug}` : "home";
}

/** `/og/pt-br/docs/cli.png`: the image of a page, from its pathname (`/pt-br/docs/cli/`). */
export function ogPathOf(pathname: string): string {
	const { locale, path } = delocalizePath(pathname);
	return `/og/${ogSegment(locale)}/${ogKeyOf(path)}.png`;
}

export interface OgCard {
	title: string;
	subtitle: string;
	/** Small label top right ("Docs"). */
	kicker?: string;
	/** Small line bottom right, in the page's language. */
	footer: string;
	/** What the bottom left shows: the page's address without the scheme. */
	url: string;
	/** Cobalt (marketing pages) or paper (docs, changelog, legal pages), like the `ground` of the page. */
	ground: "cobalt" | "paper";
}

/** Font size of a title so title + two subtitle lines + footer fit the 630 px card (two lines of about 18 characters at 124 px, of about 34 at 96 px). */
export function titleSize(title: string): number {
	const n = [...title].length;
	return n <= 12 ? 200 : n <= 20 ? 160 : n <= 30 ? 124 : n <= 64 ? 96 : 80;
}

/** A satori element (the subset of React elements it reads: `{ type, props }`). */
export interface OgNode {
	type: string;
	props: { style?: Record<string, string | number>; children?: OgNode | string | (OgNode | string | undefined)[]; [prop: string]: unknown };
}
const el = (type: string, style: Record<string, string | number>, children?: OgNode["props"]["children"], extra: Record<string, unknown> = {}): OgNode => ({
	type,
	props: { style, children: Array.isArray(children) ? children.filter((c) => c !== undefined) : children, ...extra },
});

const COLORS = {
	cobalt: { bg: "linear-gradient(135deg, #1b33f0 0%, #0b1672 100%)", ink: "#eef0f8", soft: "#c5ceff", mark: 0.14 },
	paper: { bg: "#eef0f8", ink: "#0c1038", soft: "#3a4170", mark: 0.09 },
};
export const DISPLAY = "Archivo Display";
export const TEXT = "Archivo";

/** The layout of one image: the logo and the kicker on top, the title and subtitle, the address and the words of the language below. */
export function ogTree(card: OgCard, logo: string): OgNode {
	const c = COLORS[card.ground];
	const logoImg = (size: number, style: Record<string, string | number> = {}) => el("img", { width: size, height: size, ...style }, undefined, { src: logo, width: size, height: size });
	return el("div", { width: OG_WIDTH, height: OG_HEIGHT, display: "flex", flexDirection: "column", boxSizing: "border-box", padding: "60px 72px 56px", background: c.bg, color: c.ink, fontFamily: TEXT, position: "relative" }, [
		// Big translucent logo behind the text, bottom right
		logoImg(560, { position: "absolute", right: -90, bottom: -130, opacity: c.mark }),
		card.ground === "paper" ? el("div", { position: "absolute", left: 0, top: 0, width: 20, height: OG_HEIGHT, background: "#1b33f0" }) : undefined,
		el("div", { display: "flex", alignItems: "center", width: "100%" }, [
			card.ground === "cobalt"
				? el("div", { display: "flex", alignItems: "center", justifyContent: "center", width: 84, height: 84, borderRadius: 20, background: "#eef0f8" }, logoImg(60))
				: logoImg(84),
			el("div", { fontFamily: DISPLAY, fontSize: 64, marginLeft: 22, letterSpacing: "-0.01em" }, "Atlas"),
			card.kicker
				? el("div", { marginLeft: "auto", fontSize: 28, fontWeight: 500, textTransform: "uppercase", letterSpacing: "0.12em", border: `3px solid ${c.ink}`, borderRadius: 999, padding: "8px 26px" }, card.kicker)
				: undefined,
		]),
		el("div", { display: "flex", flexDirection: "column", flexGrow: 1, justifyContent: "center", maxWidth: 1056 }, [
			el("div", { fontFamily: DISPLAY, fontSize: titleSize(card.title), lineHeight: 0.94, letterSpacing: "-0.012em" }, card.title),
			el("div", { fontSize: 36, lineHeight: 1.3, marginTop: 28, color: c.soft, lineClamp: 2 }, card.subtitle),
		]),
		el("div", { display: "flex", justifyContent: "space-between", fontSize: 26, color: c.soft }, [el("div", {}, card.url), el("div", {}, card.footer)]),
	]);
}
