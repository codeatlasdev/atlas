<script lang="ts">
	/**
	 * ApprovalPrompt — the inline "Wants to run <tool>" bubble with Deny / Allow, or the "Allowed by you" line
	 * once answered. Shown under the running task while the crewmate waits for the user's permission.
	 */
	import { Check } from "@lucide/svelte";
	import { Button } from "@codeatlasdev/ui";
	import type { PendingApproval } from "#lib/crew.ts";
	import { embedIn, msgIn } from "#lib/crew/motion.ts";
	import { fmtTime, formatApprovalInput } from "./taskView.ts";

	interface Props {
		approval: PendingApproval;
		answered: boolean;
		onanswer: (paneKey: string, req: string, allow: boolean) => void;
	}

	let { approval: pa, answered, onanswer }: Props = $props();
</script>

{#if answered}
	<div class="approval-answered" in:msgIn>
		<Check size={12} aria-hidden="true" /> Allowed by you · {fmtTime(Date.now())}
	</div>
{:else}
	<div class="approval-bubble" role="alert" in:embedIn>
		<div class="approval-tool">Wants to run <strong>{pa.tool_name}</strong></div>
		<pre class="approval-input">{formatApprovalInput(pa.tool_name ?? "", pa.tool_input)}</pre>
		<div class="approval-btns">
			<Button size="sm" variant="ghost" onclick={() => onanswer(pa.pane_key, pa.req, false)}>Deny</Button>
			<Button size="sm" onclick={() => onanswer(pa.pane_key, pa.req, true)}>Allow</Button>
		</div>
	</div>
{/if}

<style>
	.approval-bubble {
		max-width: 72%;
		padding: 10px 14px;
		border-radius: 18px;
		border-bottom-left-radius: 4px;
		background: color-mix(in oklab, var(--ca-semantic-warning) 10%, var(--ca-surface-sunken));
		border: 0.5px solid color-mix(in oklab, var(--ca-semantic-warning) 30%, transparent);
		font-size: 13px;
	}
	.approval-tool {
		font-weight: 600;
		margin-bottom: 6px;
	}
	.approval-input {
		font-family: var(--ca-font-mono, monospace);
		font-size: 11px;
		background: color-mix(in oklab, var(--ca-outline-subtle) 40%, transparent);
		border-radius: 6px;
		padding: 6px 8px;
		margin: 6px 0;
		white-space: pre-wrap;
		word-break: break-all;
	}
	.approval-btns {
		display: flex;
		gap: 8px;
		margin-top: 8px;
	}
	.approval-answered {
		font-size: 12px;
		color: var(--ca-text-secondary);
		padding: 2px 4px;
		margin-top: 2px;
	}
</style>
