import { sveltekit } from "@sveltejs/kit/vite";
import adapter from "@sveltejs/adapter-static";
import { caThemePlugin } from "@codeatlasdev/ui/vite";
import { defineConfig } from "vite";
import { labBridge } from "./scripts/lab-bridge.ts";
import { pdfjsAssets } from "./scripts/pdfjs-assets.ts";

export default defineConfig({
	plugins: [
		caThemePlugin(),
		pdfjsAssets(),
		labBridge(),
		sveltekit({ adapter: adapter({ fallback: "index.html" }) }),
	],
	clearScreen: false,
	server: { port: Number(process.env.ATLAS_LAB_PORT) || 1420, strictPort: true },
});
