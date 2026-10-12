/**
 * In-app replacements for `confirm()` / `prompt()` (native web dialogs look foreign in
 * a desktop app and `prompt()` is unreliable in WKWebView). One dialog at a time;
 * `AskDialog.svelte` renders `ask.current`.
 */
export interface AskRequest {
	title: string;
	description?: string;
	/** Present = text prompt with this initial value. */
	input?: string;
	placeholder?: string;
	confirmLabel: string;
	destructive: boolean;
	resolve: (v: string | boolean | null) => void;
}

export const ask = $state<{ current: AskRequest | null }>({ current: null });

function open<T extends string | boolean | null>(req: Omit<AskRequest, "resolve">): Promise<T> {
	// A newer question cancels the one still showing.
	ask.current?.resolve(req.input === undefined ? false : null);
	return new Promise<T>((resolve) => {
		ask.current = { ...req, resolve: resolve as AskRequest["resolve"] };
	});
}

/** Yes/no question. `destructive` paints the confirm button red. */
export function confirmDialog(
	title: string,
	opts: { description?: string; confirmLabel?: string; destructive?: boolean } = {},
): Promise<boolean> {
	return open<boolean>({
		title,
		description: opts.description,
		confirmLabel: opts.confirmLabel ?? "Confirm",
		destructive: opts.destructive ?? false,
	});
}

/** Text question; resolves to the trimmed text, or `null` when cancelled or left empty. */
export async function promptDialog(
	title: string,
	opts: { value?: string; placeholder?: string; description?: string; confirmLabel?: string } = {},
): Promise<string | null> {
	const v = await open<string | null>({
		title,
		description: opts.description,
		input: opts.value ?? "",
		placeholder: opts.placeholder,
		confirmLabel: opts.confirmLabel ?? "OK",
		destructive: false,
	});
	return v?.trim() ? v.trim() : null;
}
