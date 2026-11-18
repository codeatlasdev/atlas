<script lang="ts">
	/** Collapsible list of user theme files that failed to load fully, with each problem's path and message. */
	import { Collapsible } from "@codeatlasdev/ui";
	import { TriangleAlert } from "@lucide/svelte";
	import { tilde } from "#lib/format.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	let open = $state(false);
</script>

{#if themes.issues.length}
	<div class="issues">
		<Collapsible bind:open title={m.settings_theme_issues_files({ count: themes.issues.length })}>
			<ul class="files">
				{#each themes.issues as f (f.file)}
					<li>
						<span class="file"><TriangleAlert size={12} /> {tilde(f.file)}</span>
						<ul>
							{#each f.issues as issue, i (i)}
								<li class:err={issue.level === "error"}>
									<code>{issue.path}</code>
									<span>{issue.message}</span>
								</li>
							{/each}
						</ul>
					</li>
				{/each}
			</ul>
		</Collapsible>
	</div>
{/if}

<style>
	.issues {
		padding: 4px 14px 8px;
	}
	ul {
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.files {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 6px 0;
	}
	.file {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		font-weight: 550;
		color: var(--ca-text-primary);
	}
	.files ul {
		margin-top: 4px;
		padding-left: 18px;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.files ul li {
		display: flex;
		gap: 8px;
		font-size: 12px;
		line-height: 1.4;
		color: var(--ca-text-secondary);
	}
	.files ul li.err {
		color: var(--ca-semantic-error);
	}
	code {
		flex-shrink: 0;
		font-size: 11px;
	}
</style>
