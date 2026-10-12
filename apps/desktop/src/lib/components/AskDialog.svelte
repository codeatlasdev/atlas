<script lang="ts">
	import { Button, Dialog, TextField } from "@codeatlasdev/ui";
	import { ask } from "#lib/stores/ask.svelte.ts";

	let req = $derived(ask.current);
	let text = $state("");

	// Seed the field each time a prompt opens.
	$effect(() => {
		text = req?.input ?? "";
	});

	function close(result: string | boolean | null) {
		const r = ask.current;
		ask.current = null;
		r?.resolve(result);
	}

	const answer = () => close(req?.input === undefined ? true : text);
	const cancel = () => close(req?.input === undefined ? false : null);
</script>

<Dialog
	open={!!req}
	onOpenChange={(o) => !o && cancel()}
	title={req?.title}
	description={req?.description}
	size="sm"
>
	{#if req?.input !== undefined}
		<form
			onsubmit={(e) => {
				e.preventDefault();
				answer();
			}}
		>
			<!-- svelte-ignore a11y_autofocus -->
			<TextField bind:value={text} placeholder={req.placeholder} autocomplete="off" />
		</form>
	{/if}
	{#snippet actions()}
		<Button variant="ghost" onclick={cancel}>Cancel</Button>
		<Button variant={req?.destructive ? "destructive" : "filled"} onclick={answer}>
			{req?.confirmLabel ?? "OK"}
		</Button>
	{/snippet}
</Dialog>
