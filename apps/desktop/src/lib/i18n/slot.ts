/**
 * A sentence with a styled piece inside it (a key cap, a bold name, a path in code) is one message with a placeholder
 * for the piece: word order differs per language, so code can never cut the sentence into fragments around the element.
 * This renders the message with a marker in the placeholder and splits the text there, so the template draws
 * `{before}<Kbd>{key}</Kbd>{after}` and the translation decides where the piece sits.
 *
 *     {@const [before, after] = aroundSlot((slot) => m.onboarding_tour_press_palette({ key: slot }))}
 *
 * The marker is a private-use character, never part of a real message. A message that holds no placeholder (a
 * translation that dropped it) gives `[text, ""]`; `i18n:check` rejects that before it ships.
 */
const SLOT = "\uE000";

export function aroundSlot(render: (slot: string) => string): [before: string, after: string] {
	const [before, ...rest] = render(SLOT).split(SLOT);
	return [before ?? "", rest.join(SLOT)];
}
