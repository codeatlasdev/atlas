/**
 * The reason a language server is unavailable (`reason` of `lsp_open`, `lsp_status`, `lsp_catalog`) in the app's
 * language. The daemon sends English `message` and `hint` plus the `code` and `args` they were written from
 * (`crates/atlas-lsp/src/install.rs`, `Failure`); this writes them from the catalog and falls back to the English when
 * the code or an argument is unknown, as with a daemon from before `args`. Kept apart from `#lib/hostText.ts` so the
 * language-server messages load with the editor, not at startup.
 */
import { m } from "#lib/i18n/m.ts";

export interface LspReasonWire {
	code: string;
	message: string;
	hint?: string;
	command?: string;
	args?: Record<string, unknown>;
}

const str = (v: unknown): string | undefined => (typeof v === "string" ? v : undefined);

/** The message and hint of a language server reason in the app's language; English when the code or args are unknown. */
export function lspReasonText(r: LspReasonWire): { message: string; hint?: string } {
	const a = r.args ?? {};
	const server = str(a.server);
	const detail = str(a.detail);
	let message: string | undefined;
	let hint: string | undefined;
	switch (r.code) {
		case "no_go":
			if (server === undefined) break;
			message = m.hostd_wire_lsp_no_go_message({ server });
			hint = m.hostd_wire_lsp_no_go_hint({ server });
			break;
		case "no_curl":
			if (server === undefined) break;
			message = m.hostd_wire_lsp_no_curl_message({ server });
			hint = m.hostd_wire_lsp_no_curl_hint();
			break;
		case "offline":
			if (server === undefined || detail === undefined) break;
			message = m.hostd_wire_lsp_offline_message({ server, detail });
			hint = m.hostd_wire_lsp_offline_hint();
			break;
		case "unsupported_platform":
			if (server === undefined || detail === undefined) break;
			message = m.hostd_wire_lsp_unsupported_platform_message({ server, detail });
			hint = m.hostd_wire_lsp_unsupported_platform_hint();
			break;
		case "install_failed":
			if (server === undefined || detail === undefined) break;
			message = m.hostd_wire_lsp_install_failed_message({ server, detail });
			hint = m.hostd_wire_lsp_install_failed_hint();
			break;
		case "custom_not_found": {
			const program = str(a.program);
			if (program === undefined) break;
			message = m.hostd_wire_lsp_custom_not_found_message({ program });
			hint = m.hostd_wire_lsp_custom_not_found_hint();
			break;
		}
		case "crashed":
			if (server === undefined) break;
			if (a.cause === "repeated") {
				message = m.hostd_wire_lsp_crashed_repeated_message({ server });
				hint = m.hostd_wire_lsp_crashed_repeated_hint();
			} else if (detail !== undefined) {
				message = m.hostd_wire_lsp_crashed_start_message({ server, detail });
				hint = m.hostd_wire_lsp_crashed_start_hint();
			}
			break;
	}
	return message === undefined
		? { message: r.message, hint: r.hint }
		: { message, hint: r.hint === undefined ? undefined : hint };
}
