<script lang="ts">
	/** One message and one formatted value on screen: what a locale switch must re-render in place. */
	import { relativeTime } from "#lib/i18n/fmt.ts";
	import { m } from "#lib/i18n/m.ts";

	let { count = 0, at = 0, now = 0 }: { count?: number; at?: number; now?: number } = $props();
</script>

<p id="label">{m.settings_display_language_label()}</p>
<p id="count">{m.settings_host_sub_current({ count })}</p>
<p id="ago">{relativeTime(at, now)}</p>
