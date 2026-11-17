/**
 * A stand-in for `#lib/components/sidebar/projects.ts` with every export the real module has; pass the ones a test cares about.
 *
 * `mock.module` is process-wide in bun: a partial mock leaks into later test files, and a module that imports a name the
 * mock lacks fails to link, depending on file order (seen in CI as "Export named 'addPaths' not found"). Every projects
 * mock goes through here; `src/lib/components/sidebar/projectsMock.test.ts` keeps this list in step with the real module.
 */
export function projectsMock(overrides: Record<string, (...args: never[]) => unknown> = {}) {
	const stub = async () => {};
	return {
		addProject: stub,
		addFolder: stub,
		showDiscover: stub,
		addPaths: stub,
		refreshProjects: stub,
		locateProject: stub,
		relinkProject: stub,
		removeProject: stub,
		removeAllMissing: stub,
		...overrides,
	};
}
