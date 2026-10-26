import { StateEffect, StateField, type Extension } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { envOf } from "#lib/lsp/context.ts";
import { hoverLink, isGoToModifier, isMac } from "#lib/lsp/navigation.ts";

/**
 * ⌘-hover underlines the identifier under the pointer (Ctrl on other platforms) and ⌘-click jumps to
 * its definition. The modifier is read on every pointer move *and* on key down/up, so pressing ⌘ while
 * the pointer rests on a symbol underlines it at once, like VS Code and Zed.
 */
export const setLink = StateEffect.define<{ from: number; to: number } | null>();

const linkMark = Decoration.mark({ class: "cm-lsp-link" });

export const linkField = StateField.define<DecorationSet>({
	create: () => Decoration.none,
	update(deco, tr) {
		for (const e of tr.effects)
			if (e.is(setLink))
				return e.value ? Decoration.set([linkMark.range(e.value.from, e.value.to)]) : Decoration.none;
		return tr.docChanged ? Decoration.none : deco;
	},
	provide: (f) => EditorView.decorations.from(f),
});

export const linkPlugin = ViewPlugin.fromClass(
	class {
		pointer: { x: number; y: number } | null = null;
		/** Is the go-to modifier held right now? Kept so a scroll can recompute without a mouse event. */
		mod = false;
		constructor(readonly view: EditorView) {}
		update(u: ViewUpdate) {
			// The pointer rests while the content moves under it (scroll, a caret-driven scroll, a font change):
			// what it is over changed with no mouse event, so recompute from the retained position. A doc change
			// already dropped the underline (its range is stale); the next mouse move redraws it.
			if (this.mod && !u.docChanged && (u.viewportChanged || u.geometryChanged || u.selectionSet))
				queueMicrotask(() => this.view.dom.isConnected && this.refresh(this.mod));
		}
		refresh(mod: boolean) {
			this.mod = mod;
			const v = this.view;
			const at = mod && this.pointer ? v.posAtCoords(this.pointer) : null;
			const link = envOf(v) ? hoverLink(v.state, at) : null;
			const cur = v.state.field(linkField).size > 0;
			if (link || cur) v.dispatch({ effects: setLink.of(link) });
		}
	},
	{
		eventHandlers: {
			mousemove(e) {
				this.pointer = { x: e.clientX, y: e.clientY };
				this.refresh(isGoToModifier(e, isMac()));
			},
			mouseleave() {
				this.pointer = null;
				this.refresh(false);
			},
			keydown(e) {
				if (e.key === "Meta" || e.key === "Control")
					this.refresh(isGoToModifier({ ...e, [e.key === "Meta" ? "metaKey" : "ctrlKey"]: true }, isMac()));
			},
			keyup(e) {
				if (e.key === "Meta" || e.key === "Control") this.refresh(false);
			},
			blur() {
				this.refresh(false);
			},
			scroll() {
				this.refresh(this.mod);
			},
		},
	},
);

/** The ⌘-hover underline: field + the plugin that feeds it from the pointer and the modifier. */
export const linkHover: Extension = [linkField, linkPlugin];
