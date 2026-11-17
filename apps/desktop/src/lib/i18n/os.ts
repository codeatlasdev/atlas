/**
 * The operating system the webview runs on, detected once. `keys.ts` derives `IS_MAC` / `IS_WIN` from it and `m.ts`
 * injects it into messages, so there is no second platform test anywhere. A leaf module (imports nothing) because both
 * of those need it and `keys.ts` also builds text through `m.ts`.
 */
export type Os = "mac" | "windows" | "linux";

/** `Mac|iPhone|iPad` is macOS (app.html adds `custom-chrome` off macOS from the same `navigator.platform` test). */
export const OS: Os =
	typeof navigator === "undefined"
		? "linux"
		: /Mac|iPhone|iPad/.test(navigator.platform)
			? "mac"
			: /Win/.test(navigator.platform)
				? "windows"
				: "linux";
