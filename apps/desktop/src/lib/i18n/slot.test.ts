// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { m } from "#lib/i18n/m.ts";
import { inLocale } from "../../../test/locale.ts";
import { aroundSlot } from "./slot.ts";

describe("aroundSlot", () => {
	test("splits a message at its placeholder, wherever the language puts it", () => {
		expect(aroundSlot((s) => `Press ${s} any time.`)).toEqual(["Press ", " any time."]);
		expect(aroundSlot((s) => `${s} for every command`)).toEqual(["", " for every command"]);
		expect(aroundSlot((s) => `Open it with ${s}`)).toEqual(["Open it with ", ""]);
	});

	test("a message without the placeholder keeps its whole text before the piece", () => {
		expect(aroundSlot(() => "No key here")).toEqual(["No key here", ""]);
	});

	// A translation that loses the placeholder would draw a sentence with no key cap and no error: every message
	// that is split here must still carry it, in every language the catalog has.
	test("every shipped message drawn around a piece carries the placeholder in each language", async () => {
		for (const tag of ["en", "pt-BR"] as const) {
			await inLocale(tag, () => {
				const slot = "\uE000";
				const texts = [
					m.onboarding_tour_press_palette({ key: slot }),
					m.onboarding_phases_jump_anywhere({ key: slot }),
					m.onboarding_phases_scanning_title({ count: 3, number: slot }),
					m.onboarding_phases_found_title({ count: 1, number: slot }),
					m.settings_theme_import_dialog_detected({ format: slot }),
					m.settings_theme_import_dialog_copy_only({ format: slot }),
					m.settings_theme_import_package_imported_one({ name: slot }),
					m.settings_theme_import_package_hint({ path: slot }),
				];
				for (const text of texts) expect(text).toContain(slot);
			});
		}
	});
});
