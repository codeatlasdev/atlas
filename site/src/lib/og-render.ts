// Rasterizes an OG card at build time: satori lays the tree of og.ts out as SVG (text becomes paths, so no font is needed afterwards),
// sharp turns the SVG into a PNG. No browser, no network. The fonts are static instances of the site's Archivo (scripts/og-fonts.sh).
import satori from "satori";
import sharp from "sharp";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { DISPLAY, OG_HEIGHT, OG_WIDTH, ogTree, TEXT, type OgCard } from "./og";

const file = (rel: string) => readFileSync(resolve(process.cwd(), rel));

let assets: { fonts: Parameters<typeof satori>[1]["fonts"]; logo: string } | undefined;
function load() {
	assets ??= {
		fonts: [
			{ name: DISPLAY, data: file("src/assets/fonts/archivo-display.ttf"), weight: 800, style: "normal" },
			{ name: TEXT, data: file("src/assets/fonts/archivo-text.ttf"), weight: 500, style: "normal" },
		],
		logo: `data:image/svg+xml;base64,${file("public/favicon.svg").toString("base64")}`,
	};
	return assets;
}

/** The PNG (1200x630) of a card. */
export async function renderOg(card: OgCard): Promise<Buffer> {
	const { fonts, logo } = load();
	const svg = await satori(ogTree(card, logo) as unknown as Parameters<typeof satori>[0], { width: OG_WIDTH, height: OG_HEIGHT, fonts });
	return sharp(Buffer.from(svg)).png({ compressionLevel: 9 }).toBuffer();
}
