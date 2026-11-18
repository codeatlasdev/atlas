import { sveltekit } from "@sveltejs/kit/vite";
import adapter from "@sveltejs/adapter-static";
import { paraglideVitePlugin } from "@inlang/paraglide-js";
import { caThemePlugin } from "@codeatlasdev/ui/vite";
import { defineConfig } from "vite";
import { labBridge } from "./scripts/lab-bridge.ts";
import { generateMessages, messagesWatcher, paraglideOptions, pseudoEnabled } from "./scripts/paraglide.ts";
import { pdfjsAssets } from "./scripts/pdfjs-assets.ts";

export default defineConfig(({ command }) => {
	// The pseudo-locale en-XA is a language of development builds only (dev server, Lab, a Tauri debug build): see `pseudoEnabled`.
	const pseudo = pseudoEnabled(command);
	// The inputs of Paraglide come from the one source tree (i18n/messages); write them before the plugin compiles.
	generateMessages(pseudo);
	return {
		plugins: [
			messagesWatcher(pseudo),
			paraglideVitePlugin({ ...paraglideOptions(pseudo) }),
			caThemePlugin(),
			pdfjsAssets(),
			labBridge(),
			sveltekit({ adapter: adapter({ fallback: "index.html" }) }),
		],
		clearScreen: false,
		server: { port: Number(process.env.ATLAS_LAB_PORT) || 1420, strictPort: true },
	};
});
