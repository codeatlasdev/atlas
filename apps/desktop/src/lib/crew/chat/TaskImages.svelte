<script lang="ts">
	/**
	 * TaskImages — the images sent with a task, under its prompt bubble. Each one is fetched from the crew server
	 * when the bubble shows (`crewmate_attachment`, in pieces) and cached by task and name for the session; a click
	 * opens it large. A fetch that fails says so on that image only.
	 */
	import { hostCall } from "#lib/host.ts";
	import type { Attachment } from "#lib/crew/attachments.ts";
	import { cachedAttachment } from "#lib/crew/attachmentCache.ts";
	import ImageThumb from "./ImageThumb.svelte";

	interface Props {
		attachments: Attachment[];
		taskId: string;
		crewmateId: string;
		hostKey: string;
		/** Where the row sits: under the owner's bubble (`end`) or at the left of a card (`start`). */
		align?: "start" | "end";
	}

	let { attachments, taskId, crewmateId, hostKey, align = "end" }: Props = $props();

	let opened = $state<string | null>(null);

	const route = $derived({ id: crewmateId, task: taskId, ...(hostKey ? { host: hostKey } : {}) });

	const loads = $derived(
		attachments.map((a) => ({
			a,
			load: cachedAttachment(
				`${hostKey}\n${crewmateId}\n${taskId}\n${a.name}`,
				() => ({
					...route,
					name: a.name,
				}),
				(cmd, args) => hostCall(cmd, args),
			),
		})),
	);
</script>

<ul class="images" class:start={align === "start"} aria-label="Attached images">
	{#each loads as { a, load } (a.name)}
		<li>
			{#await load}
				<span class="ph" role="img" aria-label={`Loading ${a.name}`}></span>
			{:then img}
				<ImageThumb
					bytes={img.bytes}
					mime={img.mime}
					alt={a.name}
					size={96}
					onclick={() => (opened = opened === a.name ? null : a.name)}
				/>
				{#if opened === a.name}
					<button class="lightbox" aria-label={`Close ${a.name}`} onclick={() => (opened = null)}>
						<ImageThumb bytes={img.bytes} mime={img.mime} alt={a.name} size={480} />
					</button>
				{/if}
			{:catch}
				<span class="ph failed" role="img" aria-label={`${a.name} could not be loaded`}>!</span>
			{/await}
		</li>
	{/each}
</ul>

<style>
	.images {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		justify-content: flex-end;
		list-style: none;
		margin: 6px 0 0;
		padding: 0;
	}
	.images.start {
		justify-content: flex-start;
		margin-top: 0;
	}
	.ph {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 96px;
		height: 96px;
		border-radius: 10px;
		border: 1px dashed var(--ca-outline-subtle);
		color: var(--ca-text-secondary);
	}
	.lightbox {
		position: fixed;
		inset: 0;
		z-index: 100;
		display: flex;
		align-items: center;
		justify-content: center;
		border: none;
		padding: 0;
		cursor: default;
		background: color-mix(in oklab, black 60%, transparent);
	}
</style>
