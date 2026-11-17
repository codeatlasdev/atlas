import { sveltekit } from "@sveltejs/kit/vite";
import adapter from "@sveltejs/adapter-static";
import { paraglideVitePlugin } from "@inlang/paraglide-js";
import { caThemePlugin } from "@codeatlasdev/ui/vite";
import { defineConfig } from "vite";
import { labBridge } from "./scripts/lab-bridge.ts";
import { PARAGLIDE } from "./scripts/paraglide.ts";
import { pdfjsAssets } from "./scripts/pdfjs-assets.ts";

export default defineConfig({
	plugins: [
		paraglideVitePlugin({ ...PARAGLIDE }),
		caThemePlugin(),
		pdfjsAssets(),
		labBridge(),
		sveltekit({ adapter: adapter({ fallback: "index.html" }) }),
	],
	clearScreen: false,
	server: { port: Number(process.env.ATLAS_LAB_PORT) || 1420, strictPort: true },
});
