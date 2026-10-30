<script lang="ts">
	import type { Tool } from "#lib/bots/look.ts";

	// Drawn around the grip at (0, 0); the hand is painted on top by BotFace.
	// `t` is the rig's tool channel: needle angle, lantern swing, megaphone pulse, pin depth.
	let { tool, t = 0, glow }: { tool: Tool; t?: number; glow: string } = $props();

	const BRASS = "#d6a84a";
	const BRASS_DARK = "#9c7424";
	const INK = "#14142b";
	const STEEL = "#aab4c3";
	const STEEL_DARK = "#6b7686";
</script>

{#if tool === "compass"}
	<g transform="translate(0 -78)">
		<rect x="-10" y="40" width="20" height="40" rx="8" fill={BRASS_DARK} />
		<circle r="56" fill={BRASS} stroke={BRASS_DARK} stroke-width="6" />
		<circle r="42" fill="#fff8e6" />
		<g stroke={BRASS_DARK} stroke-width="4" stroke-linecap="round">
			<path d="M0 -36v10M0 36v-10M-36 0h10M36 0h-10" />
		</g>
		<g transform="rotate({t})">
			<path d="M0 -34 L10 0 L0 6 Z" fill="#e5484d" />
			<path d="M0 34 L-10 0 L0 -6 Z" fill={INK} />
		</g>
		<circle r="5" fill={BRASS_DARK} />
	</g>
{:else if tool === "spyglass"}
	<g transform="rotate(-8)">
		<rect x="-190" y="-20" width="80" height="40" rx="6" fill={BRASS} />
		<rect x="-196" y="-24" width="14" height="48" rx="4" fill={BRASS_DARK} />
		<rect x="-118" y="-16" width="80" height="32" rx="6" fill="#7a4b2a" />
		<rect x="-40" y="-12" width="70" height="24" rx="6" fill={BRASS} />
		<rect x="-122" y="-22" width="10" height="44" rx="4" fill={BRASS_DARK} />
		<rect x="24" y="-14" width="12" height="28" rx="4" fill={BRASS_DARK} />
		<circle cx="-196" cy="0" r="10" fill="#bfe3ff" opacity=".8" />
	</g>
{:else if tool === "wrench"}
	<g>
		<rect x="-13" y="-120" width="26" height="150" rx="13" fill={STEEL} />
		<rect x="-13" y="-120" width="8" height="150" rx="4" fill="#fff" opacity=".35" />
		<path
			d="M-34 -150 a44 44 0 1 0 68 0 l-14 30 h-40 Z"
			fill={STEEL}
			stroke={STEEL_DARK}
			stroke-width="5"
			stroke-linejoin="round"
		/>
		<circle cx="0" cy="-138" r="14" fill={STEEL_DARK} opacity=".55" />
	</g>
{:else if tool === "quill"}
	<g transform="rotate(18)">
		<path d="M0 34 L-6 10 L6 10 Z" fill={INK} />
		<path
			d="M0 12 C-40 -40 -30 -120 26 -178 C40 -120 34 -50 0 12 Z"
			fill="#fbfbff"
			stroke="#cfd6e6"
			stroke-width="4"
		/>
		<path
			d="M0 10 C6 -60 14 -110 24 -170"
			stroke="#9aa6c2"
			stroke-width="4"
			fill="none"
			stroke-linecap="round"
		/>
		<g stroke="#cfd6e6" stroke-width="3" stroke-linecap="round">
			<path d="M4 -40 L-18 -56M8 -80 L-14 -100M14 -120 L-4 -140" />
		</g>
	</g>
{:else if tool === "magnifier"}
	<g transform="rotate(-20)">
		<rect x="-11" y="-30" width="22" height="70" rx="11" fill="#7a4b2a" />
		<circle cy="-82" r="54" fill="#cfeaff" opacity=".55" />
		<circle cy="-82" r="54" fill="none" stroke={INK} stroke-width="12" />
		<path
			d="M-26 -108 a36 36 0 0 1 30 -12"
			stroke="#fff"
			stroke-width="8"
			stroke-linecap="round"
			fill="none"
		/>
	</g>
{:else if tool === "megaphone"}
	<g transform="scale({1 + t * 0.08})">
		<rect x="-12" y="-6" width="24" height="44" rx="8" fill={INK} />
		<path
			d="M-10 -26 L120 -76 L120 52 L-10 6 Z"
			fill="#f2f3f7"
			stroke="#c6cad6"
			stroke-width="5"
			stroke-linejoin="round"
		/>
		<ellipse cx="120" cy="-12" rx="16" ry="64" fill={glow} stroke="#c6cad6" stroke-width="5" />
		<path d="M-10 -26 L-30 -26 L-30 6 L-10 6 Z" fill="#e5484d" />
		{#if t > 0.15}
			<g stroke={glow} stroke-width="8" stroke-linecap="round" fill="none" opacity={t}>
				<path d="M156 -60 q22 48 0 96" />
				<path d="M184 -84 q34 72 0 144" />
			</g>
		{/if}
	</g>
{:else if tool === "pin"}
	<g>
		<rect x="-6" y="-80" width="12" height="130" rx="6" fill={INK} />
		<path d="M0 -60 C-48 -100 -48 -170 0 -176 C48 -170 48 -100 0 -60 Z" fill="#e5484d" />
		<circle cy="-140" r="16" fill="#fff" />
	</g>
{:else if tool === "lantern"}
	<g transform="rotate({t})">
		<path d="M-24 0 a24 24 0 0 1 48 0" stroke={INK} stroke-width="7" fill="none" />
		<rect x="-34" y="16" width="68" height="12" rx="4" fill={INK} />
		<circle cy="74" r="58" fill="#ffd66b" opacity=".18" />
		<rect x="-28" y="28" width="56" height="86" rx="12" fill="#ffe7a3" stroke={INK} stroke-width="7" />
		<path d="M0 98 c-14 -10 -10 -30 0 -44 c10 14 14 34 0 44 Z" fill="#ff9f1a" />
		<rect x="-34" y="112" width="68" height="12" rx="4" fill={INK} />
	</g>
{/if}
