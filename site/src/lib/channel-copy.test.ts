// @ts-nocheck bun test runtime types are not part of the site tsconfig
import { describe, expect, test } from "bun:test";
import { CHANNEL_COPY, stageAlphaNote } from "./channel-copy";
import { setLocale } from "./paraglide/runtime.js";
import copy from "../../../crates/atlas-core/data/channels.json";

// The channel copy is written once, in English, in crates/atlas-core/data/channels.json (the CLI and Atlas Desktop print it). The download
// page shows it through messages so it can be Portuguese; the English of those messages must stay word for word that file.
describe("channel copy on the download page", () => {
	test("in English, every message is the text of channels.json", () => {
		setLocale("en", { reload: false });
		for (const c of copy.channels) {
			const words = CHANNEL_COPY[c.id];
			expect({ id: c.id, promise: words.promise(), risk: words.risk(), audience: words.audience(), switchTo: words.switchTo() }).toEqual({
				id: c.id, promise: c.promise, risk: c.risk_label, audience: c.audience, switchTo: c.switch_to,
			});
		}
		expect(stageAlphaNote()).toBe(copy.stage_note);
	});

	test("in Portuguese, every channel has its own text for every field", () => {
		setLocale("pt-BR", { reload: false });
		for (const c of copy.channels) {
			const words = CHANNEL_COPY[c.id];
			for (const [field, en] of [[words.promise(), c.promise], [words.risk(), c.risk_label], [words.audience(), c.audience], [words.switchTo(), c.switch_to]]) {
				expect(field).not.toBe(en);
				expect(field.length).toBeGreaterThan(10);
			}
		}
		setLocale("en", { reload: false });
	});
});
