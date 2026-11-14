<!-- General: what it does, behaviour, project. -->
<script lang="ts">
	import type { Crewmate } from "#lib/crew.ts";
	import type { CrewmateSettings } from "#lib/crew/settings/settings.svelte.ts";
	import RoleCard from "#lib/crew/settings/RoleCard.svelte";
	import BehaviourCard from "#lib/crew/settings/BehaviourCard.svelte";
	import ProjectCard from "#lib/crew/settings/ProjectCard.svelte";

	let { crewmate, settings }: { crewmate: Crewmate; settings: CrewmateSettings } = $props();
</script>

<RoleCard {crewmate} {settings} />
<BehaviourCard {crewmate} {settings} />
<ProjectCard {crewmate} />
