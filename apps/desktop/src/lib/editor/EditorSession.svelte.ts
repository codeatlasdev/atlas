/**
 * One open text file in the editor: the CodeMirror view and everything around it that is not
 * markup — reading and decoding the file, the `.editorconfig`/large-file plan, saving with the
 * save policy, following changes made to the file by someone else, and the numbers the status bar
 * shows. `Editor.svelte` composes this with the viewers and the toolbar.
 *
 * Plain disk text (`lastSaved`, the watcher's reads) always means the on-disk representation: line
 * endings as written, save policy applied. The view holds `\n` text only (`session.ts`).
 */
import { Compartment, EditorSelection, EditorState, type Extension, type Text } from "@codemirror/state";
import { EditorView, type ViewUpdate } from "@codemirror/view";
import { toast } from "@codeatlasdev/ui";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { atlasTheme } from "#lib/cm.ts";
import { errText } from "#lib/api.ts";
import { b64bytes } from "#lib/b64.ts";
import { host } from "#lib/host.ts";
import { editorConfigDirs } from "#lib/editor/editorconfig.ts";
import { detectEol } from "#lib/editor/detect.ts";
import {
	reduceExternal,
	type ExternalEffect,
	type ExternalEvent,
	type ExternalState,
} from "#lib/editor/externalChange.ts";
import { editorExtensions, reconfigure, type EditorCompartments, type Features } from "#lib/editor/setup.ts";
import { planOpen, planSave, toViewText, type OpenPlan } from "#lib/editor/session.ts";
import { editorPrefs, type EditorPrefs } from "#lib/editor/settings.ts";
import type { StatusInput } from "#lib/editor/statusbar.ts";
import { languageLabel } from "#lib/editor/statusbar.ts";
import { m } from "#lib/i18n/m.ts";

const HEX_BYTES = 4096;
const SAVE_IDLE_MS = 500;

/** The file operations the session needs; `host` has them all, and so does the light editor's direct access. */
export type FileIo = Pick<typeof host, "fsStat" | "fsRead" | "fsWrite" | "fsWatch" | "fsUnwatch">;

export interface Hooks {
	/** Extensions that depend on the component (LSP, language, theme add-ons). */
	extensions: () => Extension[];
	/** The document changed (autosave and LSP sync are driven from here). */
	onChange: (u: ViewUpdate) => void;
	/** Write failed or the file changed under a dirty buffer: the component decides how to ask. */
	onConflict: () => void;
}

/** `Features` allowed by a large-file tier and the user's preferences. */
export function featuresFor(plan: OpenPlan, prefs: EditorPrefs): Features {
	const d = plan.large.disable;
	return {
		bracketColors: prefs.bracket_pair_colorization && !d.bracketColors,
		indentGuides: prefs.indent_guides && !d.indentGuides,
		stickyScroll: prefs.sticky_scroll && !d.stickyScroll,
		foldGutter: !d.foldGutter,
		selectionMatches: !d.selectionMatches,
		minimap: prefs.minimap && !d.minimap,
		wrap: prefs.word_wrap && !d.wrap,
	};
}

export class EditorSession {
	view: EditorView | undefined;
	compartments: EditorCompartments | undefined;
	readonly langC = new Compartment();
	readonly lspC = new Compartment();

	phase = $state<"loading" | "text" | "media" | "large" | "binary" | "error">("loading");
	error = $state("");
	fileSize = $state(0);
	head = $state<Uint8Array | undefined>();
	plan = $state<OpenPlan | undefined>();
	dirty = $state(false);
	wrap = $state(false);
	/** The file changed on disk while the buffer has edits (waiting for the user's choice). */
	conflict = $state<{ text: string } | null>(null);
	/** The file was deleted on disk. */
	deleted = $state(false);
	/** Why external changes are not followed live (the watcher failed): the error, shown with `workbench_editor_watch_failed`; empty when they are. */
	watchError = $state("");
	cursor = $state({ line: 1, col: 1, count: 1, chars: 0, lines: 1 });

	/** What is on disk, as bytes written (see file header). */
	lastSaved = "";
	#ext: ExternalState = {
		diskText: null,
		bufferText: "",
		lastSaved: "",
		mtime: null,
		dirty: false,
		autoReload: true,
	};
	/** Size and mtime of the file as this editor last read or wrote it: the save precondition. */
	#disk: { mtime: number; size: number } | null = null;
	#saveTimer: number | undefined;
	#saving: Promise<void> = Promise.resolve();
	#closing = false;
	#off: Promise<UnlistenFn> | undefined;
	#watchId = "";
	#applying = false;
	/** The text `plan` was made from, until the view exists. */
	#loaded = "";
	/** Bumped by every document change so `text` readers re-run; the copy itself is made on demand. */
	#rev = $state(0);
	#copy: { doc: Text; text: string } | undefined;
	/**
	 * What `lastSaved` is in view terms: the document it was made from and the line ending it was
	 * written with. The buffer is dirty when its document differs from this one (`Text.eq` skips the
	 * shared structure, so a keystroke costs O(change), not O(file)), the line ending changed, or the
	 * save policy would rewrite the file (`clean` false: planSave of the document is not what is on disk).
	 */
	#base: { doc: Text; eol: OpenPlan["eol"]; clean: boolean } | undefined;

	constructor(
		readonly abs: string,
		readonly relPath: string,
		readonly root: string,
		private readonly hooks: Hooks,
		/** Where the file is read and written: hostd for a project's editor, the app itself for the light editor. */
		private readonly io: FileIo = host,
	) {}

	// ---- loading ------------------------------------------------------------------------------

	/** Read the file and build the plan. `null` when it is not decodable text (phase says why). */
	async read(max: number, signal: AbortSignal): Promise<OpenPlan | null> {
		const st = await this.io.fsStat(this.abs);
		this.fileSize = st.size;
		if (signal.aborted) throw new DOMException("aborted", "AbortError");
		const r = await this.io.fsRead(this.abs, max);
		const bytes = b64bytes(r.data);
		let raw: string;
		try {
			raw = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
		} catch {
			this.head = bytes.subarray(0, HEX_BYTES);
			this.phase = "binary";
			return null;
		}
		const files = await this.#editorConfigs();
		const plan = planOpen({ raw, bytes: bytes.length, path: this.abs, files, prefs: editorPrefs() });
		this.plan = plan;
		this.#loaded = plan.text;
		this.lastSaved = raw;
		this.#ext = { ...this.#ext, bufferText: raw, lastSaved: raw, mtime: st.mtime, dirty: false };
		this.#disk = { mtime: st.mtime, size: st.size };
		this.#ext.autoReload = editorPrefs().auto_reload;
		return plan;
	}

	/** The `.editorconfig` files from the file's folder up to the worktree root (none are an error). */
	async #editorConfigs() {
		const dirs = editorConfigDirs(this.abs, this.root);
		const found = await Promise.all(
			dirs.map(async (dir) => {
				try {
					const r = await this.io.fsRead(`${dir}/.editorconfig`, 64 * 1024);
					return { dir, text: new TextDecoder().decode(b64bytes(r.data)) };
				} catch {
					return null;
				}
			}),
		);
		return found.filter((f): f is { dir: string; text: string } => f !== null);
	}

	/** Create the view over `parent`. Call after `read` returned a plan. */
	mount(parent: HTMLElement, plan: OpenPlan): EditorView {
		const prefs = editorPrefs();
		const setup = editorExtensions({ prefs, features: featuresFor(plan, prefs) });
		this.compartments = setup.compartments;
		this.wrap = featuresFor(plan, prefs).wrap;
		const view = new EditorView({
			parent,
			state: EditorState.create({
				doc: plan.text,
				extensions: [
					setup.base,
					this.lspC.of([]),
					atlasTheme,
					this.langC.of([]),
					...this.hooks.extensions(),
					EditorView.updateListener.of((u) => {
						if (u.selectionSet || u.docChanged) this.#cursor(u.state);
						if (!u.docChanged) return;
						this.#rev++;
						if (!this.#applying) this.#edited(u.state);
						this.hooks.onChange(u);
					}),
				],
			}),
		});
		this.view = view;
		this.#base = { doc: view.state.doc, eol: plan.eol, clean: planSave(plan.text, plan) === this.lastSaved };
		this.#cursor(view.state);
		this.#watch();
		return view;
	}

	#cursor(state: EditorState) {
		const main = state.selection.main;
		const line = state.doc.lineAt(main.head);
		let chars = 0;
		let lines = 0;
		for (const r of state.selection.ranges) {
			chars += r.to - r.from;
			if (!r.empty) lines += state.doc.lineAt(r.to).number - state.doc.lineAt(r.from).number + 1;
		}
		this.cursor = {
			line: line.number,
			col: main.head - line.from + 1,
			count: state.selection.ranges.length,
			chars,
			lines,
		};
	}

	/** Reconfigure what depends on preferences (call from an effect that reads `editorPrefs()`). */
	applyPrefs() {
		const v = this.view;
		const plan = this.plan;
		if (!v || !this.compartments || !plan) return;
		const prefs = editorPrefs();
		const features = featuresFor(plan, prefs);
		// A user toggle (⌥Z) stays until the preference itself changes.
		reconfigure(v, this.compartments, { prefs, features: { ...features, wrap: this.wrap || features.wrap } });
		this.#ext.autoReload = prefs.auto_reload;
	}

	setWrap(on: boolean) {
		this.wrap = on;
		this.applyPrefs();
	}

	// ---- saving -------------------------------------------------------------------------------

	/**
	 * Document text for the rich viewers (Markdown, SVG, data views). Copied from the view when read and
	 * cached per document: typing in the editor itself never reads it, so it never pays for it.
	 */
	get text(): string {
		void this.#rev;
		const doc = this.view?.state.doc;
		if (!doc) return this.#loaded;
		if (this.#copy?.doc !== doc) this.#copy = { doc, text: doc.toString() };
		return this.#copy.text;
	}

	/** The document as it is written to disk, with the snapshot it was made from. Copies the document: save and conflict checks only. */
	#snapshot(): { doc: Text; eol: OpenPlan["eol"]; out: string } | undefined {
		const v = this.view;
		const plan = this.plan;
		if (!v || !plan) return undefined;
		const doc = v.state.doc;
		return { doc, eol: plan.eol, out: planSave(doc.toString(), plan) };
	}

	#isDirty(doc: Text): boolean {
		const b = this.#base;
		return !b || !b.clean || b.eol !== this.plan?.eol || !doc.eq(b.doc);
	}

	#edited(state: EditorState) {
		this.#ext.dirty = this.dirty = this.#isDirty(state.doc);
		clearTimeout(this.#saveTimer);
		this.#saveTimer = window.setTimeout(() => void this.save(), SAVE_IDLE_MS);
	}

	/**
	 * Write the buffer. `beforeWrite` may transform the document first (format on save): it runs
	 * on the view, then the final text is read. A dirty buffer in conflict with the disk is never
	 * written by the autosave timer: that would silently overwrite the other change. Neither is a
	 * file that changed since this editor read or wrote it, even if the watcher has not said so yet
	 * (it can be dead, or inside its debounce): `force` (the user's "keep my version") skips that.
	 */
	save(beforeWrite?: () => Promise<void>, force = false): Promise<void> {
		this.#saving = this.#saving.then(() => this.#save(beforeWrite, force));
		return this.#saving;
	}

	async #save(beforeWrite?: () => Promise<void>, force = false) {
		clearTimeout(this.#saveTimer);
		if (this.conflict && !force) return this.#notSaved();
		if (beforeWrite && !this.#closing) await beforeWrite();
		const snap = this.#snapshot();
		if (!snap || (snap.out === this.lastSaved && !this.deleted)) return;
		const out = snap.out;
		try {
			if (!force && (await this.#blockedByDisk())) return;
			await this.io.fsWrite(this.abs, out);
			const st = await this.io.fsStat(this.abs).catch(() => null);
			this.#disk = st ? { mtime: st.mtime, size: st.size } : null;
			this.lastSaved = out;
			this.deleted = false;
			this.#ext = reduceExternal(this.#ext, { kind: "saved", text: out, mtime: st?.mtime ?? 0 }).state;
			this.#base = { doc: snap.doc, eol: snap.eol, clean: true };
			// Edits made while the write was in flight keep the buffer dirty.
			this.#ext.dirty = this.dirty = this.#isDirty(this.view?.state.doc ?? snap.doc);
		} catch (e) {
			toast.error(m.workbench_editor_save_failed({ path: this.relPath, error: errText(e) }));
		}
	}

	/**
	 * The file differs from what this editor last read or wrote: look at it (`#diskChanged`) and tell
	 * the caller whether writing would lose the other change. Only a real conflict blocks (the disk text
	 * differs from the buffer's baseline: a dirty buffer asks, a clean one reloads) or a file that
	 * cannot be read to compare; the same text with a newer mtime (`touch`, git, a formatter) is just
	 * re-baselined, and the write goes ahead. A file that is gone is not a change: saving recreates it.
	 */
	async #blockedByDisk(): Promise<boolean> {
		const known = this.#disk;
		const st = await this.io.fsStat(this.abs).catch(() => null);
		if (!known || !st || (st.mtime === known.mtime && st.size === known.size)) return false;
		const effect = await this.#diskChanged();
		if (effect === "unreadable") {
			toast.error(m.workbench_editor_not_saved_unreadable({ path: this.relPath }));
			return true;
		}
		if (effect !== "reload" && effect !== "prompt-conflict") return false;
		this.#notSaved();
		return true;
	}

	/** A save was refused because of someone else's change: on close that is the user's last chance to hear it. */
	#notSaved() {
		if (this.#closing && this.dirty)
			toast.error(m.workbench_editor_not_saved_changed({ path: this.relPath }));
	}

	/** Discard the buffer and show what is on disk (Revert file). */
	async revert() {
		const text = await this.#readDisk();
		if (text !== null) this.#replaceBuffer(text);
	}

	// ---- external changes ---------------------------------------------------------------------

	async #readDisk(): Promise<string | null> {
		try {
			const r = await this.io.fsRead(
				this.abs,
				this.fileSize > 0 ? Math.max(this.fileSize * 2, 1_000_000) : undefined,
			);
			return new TextDecoder("utf-8", { fatal: true }).decode(b64bytes(r.data));
		} catch {
			return null;
		}
	}

	/** Replace the document with disk text, keeping the caret near where it was. */
	#replaceBuffer(disk: string) {
		const v = this.view;
		if (!v) return;
		const next = toViewText(disk);
		const eol = detectEol(disk);
		if (this.plan) this.plan = { ...this.plan, eol: eol === "mixed" ? "lf" : eol, mixedEol: eol === "mixed" };
		const cur = v.state.doc.toString();
		if (cur !== next) {
			const head = Math.min(v.state.selection.main.head, next.length);
			this.#applying = true;
			try {
				v.dispatch({
					changes: { from: 0, to: v.state.doc.length, insert: next },
					selection: EditorSelection.cursor(head),
					userEvent: "reload",
				});
			} finally {
				this.#applying = false;
			}
		}
		this.lastSaved = disk;
		this.conflict = null;
		this.#ext = { ...this.#ext, bufferText: disk, lastSaved: disk, diskText: null, dirty: false };
		this.dirty = false;
		this.#base = {
			doc: v.state.doc,
			eol: this.plan?.eol ?? "lf",
			clean: !!this.plan && planSave(next, this.plan) === disk,
		};
	}

	#apply(effect: ExternalEffect, diskText?: string) {
		if (effect === "reload" && diskText !== undefined) this.#replaceBuffer(diskText);
		else if (effect === "in-sync" && diskText !== undefined) {
			// Someone wrote what the buffer already says: that is the new baseline.
			this.lastSaved = diskText;
			this.dirty = false;
			this.#base = { doc: this.view!.state.doc, eol: this.plan?.eol ?? "lf", clean: true };
		} else if (effect === "prompt-conflict" && diskText !== undefined) {
			this.conflict = { text: diskText };
			this.hooks.onConflict();
		} else if (effect === "notice-deleted") this.deleted = true;
	}

	/** The user's answer to a conflict (`reload` takes the disk, `keep` writes the buffer over it). */
	async resolve(choice: "reload" | "keep") {
		const r: { state: ExternalState; effect: ExternalEffect } = reduceExternal(this.#ext, {
			kind: "resolve",
			choice,
		});
		const disk = this.#ext.diskText;
		this.#ext = r.state;
		if (choice === "reload" && disk !== null) this.#replaceBuffer(disk);
		else {
			this.conflict = null;
			this.#ext = { ...this.#ext, diskText: null };
			await this.save(undefined, true);
		}
	}

	#watch() {
		const dir = this.abs.slice(0, this.abs.lastIndexOf("/"));
		this.#watchId = `ed:${this.abs}:${Math.random().toString(36).slice(2, 8)}`;
		this.io.fsWatch(dir, this.#watchId).catch((e) => {
			// Saves still compare the file with what we last read (`#changedOnDisk`); only live updates are lost.
			this.watchError = errText(e);
		});
		let timer: number | undefined;
		this.#off = listen<{ ev: string; id?: string; paths?: string[]; kind?: string }>("host://event", (e) => {
			const p = e.payload;
			if (p.ev !== "fs_event" || p.id !== this.#watchId || !p.paths?.includes(this.abs)) return;
			clearTimeout(timer);
			timer = window.setTimeout(() => {
				if (!this.#closing) void this.#diskChanged();
			}, 120);
		});
	}

	/**
	 * Read the file and feed it to the external-change reducer; returns the effect (applied to the
	 * editor unless it is closing). `#disk` is re-baselined to the stat this read saw, so a later save
	 * compares against what is on disk now. `unreadable`: the text could not be read or decoded.
	 */
	async #diskChanged(): Promise<ExternalEffect | "unreadable"> {
		let st: { mtime: number; size: number };
		try {
			st = await this.io.fsStat(this.abs);
		} catch {
			return this.#feed({ kind: "disk-deleted" });
		}
		const text = await this.#readDisk();
		if (text === null) return "unreadable";
		this.#disk = { mtime: st.mtime, size: st.size };
		// The reducer compares the disk with the buffer as it would be written; typing does not keep that
		// copy, so make it now (not for our own write coming back: that one matches `lastSaved`).
		const buffer = text === this.#ext.lastSaved ? undefined : this.#snapshot();
		if (buffer) this.#ext.bufferText = buffer.out;
		return this.#feed({ kind: "disk-changed", text, mtime: st.mtime }, text);
	}

	#feed(ev: ExternalEvent, diskText?: string): ExternalEffect {
		const r = reduceExternal(this.#ext, ev);
		this.#ext = r.state;
		if (!this.#closing) this.#apply(r.effect, diskText);
		return r.effect;
	}

	// ---- status bar ---------------------------------------------------------------------------

	status(
		language: { id: string; label?: string },
		problems: { errors: number; warnings: number },
	): StatusInput {
		const plan = this.plan;
		return {
			line: this.cursor.line,
			col: this.cursor.col,
			selections: { count: this.cursor.count, chars: this.cursor.chars, lines: this.cursor.lines },
			indent: plan?.indent ?? { style: "space", size: 4, detected: false },
			eol: plan?.mixedEol ? "mixed" : (plan?.eol ?? "lf"),
			encoding: "utf-8",
			language: { id: language.id, label: language.label ?? languageLabel(language.id) },
			problems,
			tier: plan?.large.tier ?? "normal",
			wrap: this.wrap,
		};
	}

	/** Change the indentation used for new lines and saving (does not rewrite the file). */
	setIndent(style: "space" | "tab", size: number) {
		if (!this.plan) return;
		this.plan = { ...this.plan, indent: { style, size, detected: false } };
		const v = this.view;
		const c = this.compartments;
		if (v && c) {
			const prefs = editorPrefs();
			reconfigure(v, c, {
				prefs: { ...prefs, tab_size: size, insert_spaces: style === "space" },
				features: featuresFor(this.plan, prefs),
			});
		}
	}

	/** Change the line ending written on save; the file becomes dirty if it differs. */
	setEol(eol: "lf" | "crlf" | "cr") {
		if (!this.plan) return;
		this.plan = { ...this.plan, eol, mixedEol: false };
		this.#edited(this.view!.state);
	}

	/** Flush a pending autosave and release everything (component unmount). */
	async dispose(beforeClose?: () => void) {
		this.#closing = true;
		clearTimeout(this.#saveTimer);
		beforeClose?.();
		if (this.#watchId) void this.io.fsUnwatch(this.#watchId).catch(() => {});
		void this.#off?.then((u) => u());
		await this.save();
		this.view?.destroy();
	}
}
