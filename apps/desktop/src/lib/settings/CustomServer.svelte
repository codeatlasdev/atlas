<script lang="ts">
	import { Button } from "@codeatlasdev/ui";
	import type { LspLanguagePref } from "#lib/prefs.ts";
	import { formatArgs, parseArgs } from "./args.ts";
	import FieldInput from "./FieldInput.svelte";
	import { commandProblem, hasCustom, jsonText, parseJsonObject } from "./languages.ts";
	import { m } from "#lib/i18n/m.ts";

	/**
	 * Custom server of one language: command, arguments and the two JSON blobs the server gets.
	 * Text boxes commit on change; invalid text shows its error and the last valid value stays saved.
	 */
	let {
		language,
		label,
		pref,
		onsave,
		onreset,
	}: {
		language: string;
		/** Human name of the language, for labels. */
		label: string;
		pref: LspLanguagePref | undefined;
		onsave: (patch: Partial<LspLanguagePref>) => void;
		onreset: () => void;
	} = $props();

	let command = $derived(pref?.command ?? "");
	let argsText = $derived(formatArgs(pref?.args ?? []));
	let initText = $derived(jsonText(pref?.initialization_options));
	let settingsText = $derived(jsonText(pref?.settings));

	/** What the user typed in a box that did not parse; cleared on the next valid commit. */
	let argsError = $state("");
	let initError = $state("");
	let settingsError = $state("");
	/** The text currently in the box, so an invalid entry stays visible while it is wrong. */
	let initDraft = $state<string | null>(null);
	let settingsDraft = $state<string | null>(null);
	let argsDraft = $state<string | null>(null);

	let cmdProblem = $derived(commandProblem(command));

	function commitCommand(text: string) {
		const t = text.trim();
		onsave({ command: t === "" ? undefined : t });
	}

	function commitArgs(text: string) {
		const r = parseArgs(text);
		if (!r.ok) {
			argsError = r.error;
			argsDraft = text;
			return;
		}
		argsError = "";
		argsDraft = null;
		onsave({ args: r.args.length === 0 ? undefined : r.args });
	}

	function commitJson(
		key: "initialization_options" | "settings",
		text: string,
		setError: (e: string) => void,
		setDraft: (d: string | null) => void,
	) {
		const r = parseJsonObject(text);
		if (!r.ok) {
			setError(r.error);
			setDraft(text);
			return;
		}
		setError("");
		setDraft(null);
		onsave({ [key]: r.value });
	}

	function reset() {
		argsError = initError = settingsError = "";
		argsDraft = initDraft = settingsDraft = null;
		onreset();
	}
</script>

<div class="custom">
	<label class="field">
		<span class="name">{m.settings_editor_custom_command()}</span>
		<FieldInput
			class="text mono"
			style="width: 100%"
			aria-label={m.settings_editor_custom_command_aria({ language: label })}
			placeholder={m.settings_editor_custom_command_placeholder()}
			spellcheck="false"
			value={command}
			onchange={(e) => commitCommand(e.currentTarget.value)}
		/>
		<span class="help" class:bad={!!cmdProblem}>
			{cmdProblem ?? m.settings_editor_custom_command_help()}
		</span>
	</label>

	<label class="field">
		<span class="name">{m.settings_editor_custom_args()}</span>
		<!-- i18n-ignore: an example flag, not prose -->
		<FieldInput
			class="text mono"
			style="width: 100%"
			aria-label={m.settings_editor_custom_args_aria({ language: label })}
			placeholder="--stdio"
			spellcheck="false"
			value={argsDraft ?? argsText}
			aria-invalid={argsError !== ""}
			oninput={(e) => {
				argsDraft = e.currentTarget.value;
			}}
			onchange={(e) => commitArgs(e.currentTarget.value)}
		/>
		<span class="help" class:bad={argsError !== ""}>
			{argsError || m.settings_editor_custom_args_help()}
		</span>
	</label>

	<label class="field">
		<span class="name">{m.settings_editor_custom_init()}</span>
		<textarea
			class="json"
			class:invalid={initError !== ""}
			rows="4"
			spellcheck="false"
			aria-label={m.settings_editor_custom_init_aria({ language: label })}
			aria-invalid={initError !== ""}
			placeholder={'{ "preferences": { "importModuleSpecifier": "relative" } }'}
			value={initDraft ?? initText}
			oninput={(e) => {
				initDraft = e.currentTarget.value;
			}}
			onchange={(e) =>
				commitJson(
					"initialization_options",
					e.currentTarget.value,
					(msg) => (initError = msg),
					(d) => (initDraft = d),
				)}></textarea>
		<span class="help" class:bad={initError !== ""}>
			{initError
				? m.settings_editor_custom_last_valid_kept({ error: initError })
				: m.settings_editor_custom_init_help()}
		</span>
	</label>

	<label class="field">
		<span class="name">{m.settings_editor_custom_settings()}</span>
		<textarea
			class="json"
			class:invalid={settingsError !== ""}
			rows="4"
			spellcheck="false"
			aria-label={m.settings_editor_custom_settings_aria({ language: label })}
			aria-invalid={settingsError !== ""}
			placeholder={'{ "gopls": { "staticcheck": true } }'}
			value={settingsDraft ?? settingsText}
			oninput={(e) => {
				settingsDraft = e.currentTarget.value;
			}}
			onchange={(e) =>
				commitJson(
					"settings",
					e.currentTarget.value,
					(msg) => (settingsError = msg),
					(d) => (settingsDraft = d),
				)}></textarea>
		<span class="help" class:bad={settingsError !== ""}>
			{settingsError
				? m.settings_editor_custom_last_valid_kept({ error: settingsError })
				: m.settings_editor_custom_settings_help()}
		</span>
	</label>

	<div class="actions">
		<Button
			size="sm"
			variant="outlined"
			disabled={!hasCustom(pref) && !argsDraft && !initDraft && !settingsDraft}
			onclick={reset}>{m.settings_editor_custom_reset()}</Button
		>
		<span class="note">{m.settings_editor_custom_apply_note()}</span>
	</div>
</div>

<style>
	.custom {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
	}
	.name {
		font-size: 12px;
		font-weight: 500;
		color: var(--ca-text-primary);
	}
	.help {
		font-size: 11.5px;
		line-height: 1.4;
		color: var(--ca-text-hint);
	}
	.help.bad {
		color: var(--ca-semantic-error);
	}
	.json {
		box-sizing: border-box;
		width: 100%;
		min-height: 64px;
		padding: 7px 8px;
		resize: vertical;
		font-family: ui-monospace, monospace;
		font-size: 12px;
		line-height: 1.45;
		border-radius: 7px;
		border: 0.5px solid var(--atlas-control-border, var(--ca-outline-default));
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
		user-select: text;
		-webkit-user-select: text;
	}
	.json:focus {
		outline: none;
		border-color: color-mix(in oklab, var(--ca-brand-main) 55%, var(--ca-outline-subtle));
	}
	.json.invalid {
		border-color: var(--ca-semantic-error);
	}
	.actions {
		display: flex;
		align-items: center;
		gap: 12px;
	}
	.note {
		font-size: 11.5px;
		color: var(--ca-text-hint);
	}
</style>
