<script lang="ts">
	import CrewArtIntro from "#lib/crew/CrewArtIntro.svelte";
</script>

<div class="stage">
	<CrewArtIntro />
</div>

<style>
	:global(body) {
		margin: 0;
		background: #0b1130;
	}
	.stage {
		position: relative;
		width: 640px;
		height: 440px;
		margin: 40px auto;
		background: #132050;
		border-radius: 12px;
		overflow: hidden;
	}
</style>
