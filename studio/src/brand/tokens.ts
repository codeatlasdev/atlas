// Brand tokens. The website is the source of truth (site/src/styles/global.css); `bun studio check` fails if
// these drift from it. Fonts are the site's own files, copied into studio/assets/fonts (SIL OFL, see LICENSES.txt).

export const COLOR = {
	blue: "#1b33f0",
	blueDeep: "#0b1672",
	paper: "#eef0f8",
	ink: "#0c1038",
	ink2: "#3b4075",
	white: "#ffffff",
	// status only, never decoration
	go: "#5df0a6",
	wait: "#ffd23f",
	fail: "#ff5c4a",
	port: "#7fe9ff",
} as const;

export const FONT = {
	sans: `"Archivo", "Helvetica Neue", Arial, sans-serif`,
	mono: `"JetBrains Mono", ui-monospace, "SF Mono", Menlo, Consolas, monospace`,
	files: {
		sans: "assets/fonts/archivo-var.woff2",
		mono: "assets/fonts/jetbrains-mono-var.woff2",
	},
} as const;

/** CSS custom properties emitted on the composition root, so scene CSS reads like the site's. */
export const ROOT_VARS = `--blue:${COLOR.blue};--blue-deep:${COLOR.blueDeep};--paper:${COLOR.paper};--ink:${COLOR.ink};--white:${COLOR.white};--go:${COLOR.go};--wait:${COLOR.wait};--fail:${COLOR.fail};--port:${COLOR.port};--font:${FONT.sans};--mono:${FONT.mono}`;

/** Text shadow every card uses so type reads over the bright part of the field. */
export const TEXT_SHADOW = "0 2px 30px rgba(5,10,60,.85)";

/** Syntax theme for code cards: the cobalt theme the 1.0 film's YAML card used. */
export const CODE_THEME = {
	name: "atlas-cobalt",
	type: "dark" as const,
	colors: { "editor.background": "#00000000", "editor.foreground": "#EEF0F8" },
	tokenColors: [
		{ scope: ["entity.name.tag.yaml", "support.type.property-name"], settings: { foreground: COLOR.port } },
		{ scope: ["string", "string.unquoted.plain.out.yaml"], settings: { foreground: "#EEF0F8" } },
		{ scope: ["constant.numeric", "constant.language"], settings: { foreground: COLOR.wait } },
		{ scope: ["punctuation"], settings: { foreground: "#8A93D6" } },
		{ scope: ["string.quoted.double.yaml", "string.quoted"], settings: { foreground: COLOR.go } },
		{ scope: ["comment"], settings: { foreground: "#8A93D6", fontStyle: "italic" } },
		{ scope: ["keyword", "storage"], settings: { foreground: "#BFD0FF" } },
	],
};
