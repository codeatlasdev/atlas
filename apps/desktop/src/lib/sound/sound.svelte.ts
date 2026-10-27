/**
 * The app's earcons. Wires the pure policy + engine to the real world: settings.json (`sounds`), the macOS
 * "Play user interface sound effects" switch (read once from Rust), window focus and the Web Audio gesture rule.
 * Everything that decides *whether* a sound plays lives in policy.ts; this file only supplies the facts.
 */
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "#lib/api.ts";
import { createEngine } from "./engine.ts";
import { DEFAULT_SOUND_PREFS, parseSoundPrefs, type Category, type SoundPrefs } from "./policy.ts";

const LAST_KEY = "atlas.sound.last";

class Sound {
	prefs = $state<SoundPrefs>(structuredClone(DEFAULT_SOUND_PREFS));
	/** macOS "Play user interface sound effects"; true until Rust says otherwise (the preference defaults to on). */
	systemUiSounds = $state(true);
	foreground = true;
	loaded = $state(false);

	#engine = createEngine({
		AudioContext:
			window.AudioContext ??
			(window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext,
		fetchAsset: async (name) => {
			const res = await fetch(`/sounds/${name}.m4a`);
			if (!res.ok) throw new Error(`sound ${name}: ${res.status}`);
			return res.arrayBuffer();
		},
		persisted: {
			load: () => {
				try {
					return JSON.parse(localStorage.getItem(LAST_KEY) ?? "{}");
				} catch {
					return {};
				}
			},
			save: (last) => localStorage.setItem(LAST_KEY, JSON.stringify(last)),
		},
		context: () => ({
			now: Date.now(),
			foreground: this.foreground,
			systemUiSounds: this.systemUiSounds,
			prefs: this.prefs,
			reducedMotion: window.matchMedia("(prefers-reduced-motion: reduce)").matches,
		}),
	});

	/** Load prefs + system switch, start tracking focus and the first gesture. Call once; returns the stop function. */
	async init(): Promise<() => void> {
		const [settings, system, focused] = await Promise.all([
			api.settings(),
			invoke<boolean>("system_ui_sounds").catch(() => true),
			getCurrentWindow()
				.isFocused()
				.catch(() => true),
		]);
		this.prefs = parseSoundPrefs(settings.sounds);
		this.systemUiSounds = system;
		this.foreground = focused;
		this.loaded = true;

		// WebKit refuses audio before a user gesture; the first one creates the context, later ones resume it.
		const gesture = () => this.#engine.gesture();
		const events = ["pointerdown", "keydown"] as const;
		for (const e of events) window.addEventListener(e, gesture, { capture: true, passive: true });
		const off = await getCurrentWindow().onFocusChanged(({ payload }) => {
			this.foreground = payload;
			// The preference can change while the app runs; re-read it when the user comes back.
			if (payload)
				void invoke<boolean>("system_ui_sounds")
					.then((v) => (this.systemUiSounds = v))
					.catch(() => {});
		});
		return () => {
			for (const e of events) window.removeEventListener(e, gesture, { capture: true });
			off();
		};
	}

	/** Play an earcon if the policy allows. Never throws. */
	play(category: Category) {
		if (this.loaded) void this.#engine.play(category);
	}

	/** The cold-start signature: held until the first gesture (the webview refuses audio before one). */
	signature() {
		this.#engine.playOnGesture("boot");
	}

	/** Settings → Sounds → Play: ignores cooldowns and the system switch, never volume 0. */
	preview(category: Category) {
		this.#engine.gesture();
		void this.#engine.play(category, { preview: true });
	}

	async save(next: SoundPrefs) {
		this.prefs = next;
		await api.settingsPatch({ sounds: $state.snapshot(next) });
	}
}

export const sound = new Sound();
