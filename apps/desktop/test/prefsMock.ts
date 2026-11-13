/**
 * A stand-in for `#lib/stores/prefs.svelte.ts` with every export the real module has.
 *
 * `mock.module` is process-wide in bun: a partial mock (only `prefs`) leaks into later test
 * files, and a module that imports `setPref` then fails to link, depending on file order
 * (seen in CI as "Export named 'setPref' not found"). Every prefs mock goes through here;
 * `prefsMock.test.ts` keeps this export list in step with the real module.
 */
export function prefsMock(value: Record<string, unknown>) {
	return {
		prefs: { value },
		loadPrefs: async () => {},
		setPref: async () => {},
	};
}
