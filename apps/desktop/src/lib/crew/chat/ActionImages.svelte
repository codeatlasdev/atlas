<script lang="ts">
	/**
	 * ActionImages — what the owner adds to a card that can be retried (a refused or unproven task): a button that
	 * opens the image picker, a paste target, and the pending images each with a remove button. It only reports files;
	 * `chat.attachToAction` decides what is accepted (the same rules as a task's images).
	 */
	import { Button } from "@codeatlasdev/ui";
	import { imagesFromClipboard, type PendingImage } from "#lib/crew/attachments.ts";
	import { m } from "#lib/i18n/m.ts";

	interface Props {
		images: PendingImage[];
		disabled?: boolean;
		onfiles: (files: File[]) => void;
		onremove: (index: number) => void;
	}

	let { images, disabled = false, onfiles, onremove }: Props = $props();

	let picker = $state<HTMLInputElement | null>(null);

	function picked() {
		const files = [...(picker?.files ?? [])];
		if (picker) picker.value = "";
		if (files.length) onfiles(files);
	}

	function pasted(e: ClipboardEvent) {
		const files = imagesFromClipboard(e as never);
		if (!files.length) return;
		e.preventDefault();
		onfiles(files);
	}
</script>

<div class="action-images" onpaste={pasted} role="group" aria-label={m.crew_chat_action_images_group()}>
	{#if images.length}
		<ul class="pending">
			{#each images as im, i (im.name + i)}
				<li>
					<span class="name">{im.name}</span>
					<button
						class="rm"
						aria-label={m.crew_chat_composer_remove_image({ name: im.name })}
						{disabled}
						onclick={() => onremove(i)}>×</button
					>
				</li>
			{/each}
		</ul>
	{/if}
	<input
		bind:this={picker}
		type="file"
		accept="image/png,image/jpeg,image/webp,image/gif"
		multiple
		hidden
		onchange={picked}
	/>
	<Button variant="ghost" size="sm" {disabled} onclick={() => picker?.click()}
		>{m.crew_chat_action_images_add_a_screenshot()}</Button
	>
</div>

<style>
	.action-images {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 6px;
	}
	.pending {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.pending li {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 2px 4px 2px 8px;
		font-size: 12px;
		border-radius: 10px;
		border: 1px solid var(--ca-outline-subtle);
		color: var(--ca-text-secondary);
	}
	.name {
		max-width: 160px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.rm {
		border: none;
		background: none;
		color: inherit;
		cursor: default;
		font-size: 14px;
		line-height: 1;
	}
</style>
