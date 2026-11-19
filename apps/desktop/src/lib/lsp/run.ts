/** Runs the buttons of the LSP status popover and hints (`ActionId`s from `status.ts`). */
import { toast } from "@codeatlasdev/ui";
import {
	cancelInstall,
	docOf,
	installServer,
	reopenAll,
	restartServer,
	serverFor,
} from "#lib/lsp/client.svelte.ts";
import { hostGate, postponeHost, restartHost } from "#lib/lsp/hostGate.ts";
import { openLog } from "#lib/lsp/log.ts";
import { hostRestartPrompt } from "#lib/lsp/hostPrompt.ts";
import type { ActionId } from "#lib/lsp/status.ts";
import { uiStateOf } from "#lib/lsp/uiState.svelte.ts";
import { confirmDialog } from "#lib/stores/ask.svelte.ts";
import { setPref } from "#lib/stores/prefs.svelte.ts";
import { updates } from "#lib/stores/updates.svelte.ts";
import { m } from "#lib/i18n/m.ts";

const DOCS_URL = "https://github.com/codeatlasdev/atlas/blob/main/docs/editor.md#language-servers";

async function restartHostConfirmed() {
	const p = hostRestartPrompt(hostGate.facts.ptys);
	if (
		!(await confirmDialog(p.title, {
			description: p.description,
			confirmLabel: p.confirmLabel,
			destructive: p.destructive,
		}))
	)
		return;
	await restartHost();
}

/** Carry out `id` for the document at `path`. `payload` is the action's own data (copy-command). */
export async function runAction(id: ActionId, path: string, payload?: string): Promise<void> {
	const doc = docOf(path);
	const srv = serverFor(path);
	const server = srv?.name ?? doc?.server ?? "";
	switch (id) {
		case "restart-host":
			return restartHostConfirmed();
		case "restart-host-later":
			return postponeHost();
		case "enable-setting":
			await setPref("enable_language_servers", true);
			return;
		case "open-settings-languages":
			return updates.show("editor/languages");
		case "docs":
			window.open(DOCS_URL, "_blank");
			return;
		case "install":
			if (server) await installServer(server).catch((e) => toast.error(String(e)));
			return;
		case "cancel-install":
			if (server) await cancelInstall(server).catch((e) => toast.error(String(e)));
			return;
		case "retry":
			return reopenAll();
		case "restart-server":
			if (server) await restartServer(server, srv?.root).catch((e) => toast.error(String(e)));
			return;
		case "open-log":
			if (server) await openLog(server);
			return;
		case "copy-command":
			if (payload) {
				await navigator.clipboard.writeText(payload).catch(() => {});
				toast.success(m.workbench_lsp_command_copied());
			}
			return;
	}
}

/** Current state of `path`'s document, for callers that only have the path. */
export const stateOfPath = (path: string) => {
	const doc = docOf(path);
	return doc ? uiStateOf(doc) : undefined;
};
