<script lang="ts">
	/**
	 * A path clipped at its START (the file name stays visible). `direction: rtl` on the clipping box moves the
	 * ellipsis to the left, but in an RTL paragraph the bidi algorithm also moves the neutral punctuation at the
	 * string's edges (`.logs/` read `/logs.`, incident #29). The text is therefore isolated as LTR inside the box.
	 */
	let { path, title }: { path: string; title?: string } = $props();
</script>

<span class="path" dir="rtl" {title}><bdi dir="ltr">{path}</bdi></span>

<style>
	.path {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		text-align: left;
	}
</style>
