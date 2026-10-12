// @ts-nocheck node: imports and vite hook types are not part of the app tsconfig
// Vite plugin: pdf.js loads CMaps, standard fonts, ICC profiles and wasm decoders by URL from inside
// its worker. Serve them same-origin under `/pdfjs/` (dev middleware + emitted into the bundle) so no
// CDN is used and the existing `connect-src 'self'` covers the fetches. The .wasm binaries are skipped:
// the viewer runs pdf.js with `useWasm: false`, which loads the *_nowasm_fallback.js decoders instead.
import { readFileSync, readdirSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";

export function pdfjsAssets() {
	const root = dirname(createRequire(import.meta.url).resolve("pdfjs-dist/package.json"));
	const files = ["cmaps", "standard_fonts", "iccs", "wasm"].flatMap((d) =>
		readdirSync(join(root, d))
			.filter((f) => !f.startsWith("LICENSE") && !f.endsWith(".wasm"))
			.map((f) => `${d}/${f}`),
	);
	return {
		name: "atlas:pdfjs-assets",
		configureServer(server) {
			server.middlewares.use("/pdfjs/", (req, res, next) => {
				const rel = decodeURIComponent((req.url ?? "").split("?")[0]).replace(/^\//, "");
				if (!files.includes(rel)) return next();
				res.setHeader("Content-Type", rel.endsWith(".js") ? "text/javascript" : "application/octet-stream");
				res.end(readFileSync(join(root, rel)));
			});
		},
		generateBundle() {
			for (const f of files)
				this.emitFile({ type: "asset", fileName: `pdfjs/${f}`, source: readFileSync(join(root, f)) });
		},
	};
}
