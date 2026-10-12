import { ws } from "#lib/stores/workspace.svelte.ts";
import { openPreview } from "#lib/stores/browser.svelte.ts";
import { toast } from "@codeatlasdev/ui";
import { api, type ServiceDef } from "#lib/api.ts";
import { store } from "#lib/stores/projects.svelte.ts";

/** Pending `confirm:` gate, rendered by ConfirmSheet. */
export const pending = $state<{ value: { id: string; target: string; label: string; text: string } | null }>({
	value: null,
});

export async function runAction(id: string, target: string, confirmed = false) {
	const res = await store.run(`action:${id}:${target}`, () => api.action(id, target, confirmed));
	if (!res) return;
	if (res.needs_confirm) {
		pending.value = { id, target, label: res.label ?? target, text: res.needs_confirm };
	} else {
		toast.info(`${res.action ?? target} started`);
	}
}

/** Open a service like the TUI's `open_key`: browser for web/api, copy the connection URL for data stores. */
export async function openService(svc: Pick<ServiceDef, "name" | "target">) {
	const t = svc.target;
	if (!t) return;
	if (t.how === "browser") {
		const w = ws.selected;
		await store.run("url", () => (w ? openPreview(w.id, t.url) : api.openUrl(t.url)));
	} else {
		await navigator.clipboard.writeText(t.url);
		toast.success(`Copied ${t.url}`);
	}
}

/**
 * Bare-key shortcuts from atlas.yaml, as in the TUI: a service's `open_key` opens it,
 * an action's `key` runs it. Ignored while typing, with modifiers, or with a dialog open.
 */
export function configKey(e: KeyboardEvent): boolean {
	if (e.metaKey || e.ctrlKey || e.altKey || e.key.length !== 1) return false;
	const t = e.target as HTMLElement | null;
	if (t?.closest("input, textarea, [contenteditable], .cm-editor, [role=dialog]")) return false;
	if (document.querySelector("[role=dialog][data-state=open]") || activeKind() === "config") return false;
	const p = store.current;
	const cfg = p && store.configs[p.id];
	if (!p || !cfg) return false;
	const svc = cfg.services.find((s) => s.open_key === e.key);
	if (svc?.target) {
		openService(svc);
		return true;
	}
	const action = cfg.actions.find((a) => a.key === e.key && !a.session);
	if (action && p.running) {
		runAction(p.id, action.key);
		return true;
	}
	return false;
}

function activeKind() {
	const w = ws.selected;
	return w ? ws.tab(ws.focusedGroup(w.id).activeTab)?.kind : undefined;
}
