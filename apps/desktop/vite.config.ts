import { sveltekit } from "@sveltejs/kit/vite";
import adapter from "@sveltejs/adapter-static";
import { caThemePlugin } from "@codeatlasdev/ui/vite";
import { defineConfig } from "vite";
import { pdfjsAssets } from "./scripts/pdfjs-assets.ts";

export default defineConfig({
	plugins: [caThemePlugin(), pdfjsAssets(), sveltekit({ adapter: adapter({ fallback: "index.html" }) })],
	clearScreen: false,
	server: { port: 1420, strictPort: true },
});
