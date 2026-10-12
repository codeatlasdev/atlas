/**
 * Zed-style docks around the worktree's pane tree: left (project / git panels),
 * right (context sidebar), bottom (a terminal tab group). Also pane zoom and the
 * centered layout. Persisted per app in localStorage.
 *
 * Two Arc-style shell sidebars sit outside the worktree: the spaces `sidebar` (left,
 * pinned by default) and the `right` context sidebar (hidden by default). When one is
 * not pinned, `peek` slides it over the content from its window edge.
 */
import { clampSize, DOCK_DEFAULTS, type DockSide } from "#lib/docksize.ts";

const KEY = "atlas.docks";

export type LeftPanel = "explorer" | "scm" | "search";

interface DockState {
	left: boolean;
	leftPanel: LeftPanel;
	right: boolean;
	bottom: boolean;
	/** Left dock width in px. */
	leftSize: number;
	/** Right dock width in px. */
	rightSize: number;
	/** Bottom dock height in px. */
	bottomSize: number;
	/** The focused pane fills the worktree area (docks hidden). */
	zoomed: boolean;
	centered: boolean;
	/** Spaces sidebar (window edge, left) pinned beside the content; false = hidden, peeks on hover. */
	sidebar: boolean;
}

const defaults: DockState = {
	left: false,
	leftPanel: "explorer",
	right: false,
	bottom: false,
	leftSize: DOCK_DEFAULTS.left,
	rightSize: DOCK_DEFAULTS.right,
	bottomSize: DOCK_DEFAULTS.bottom,
	zoomed: false,
	centered: false,
	sidebar: true,
};

function load(): DockState {
	try {
		const saved = JSON.parse(localStorage.getItem(KEY) ?? "{}");
		return {
			...defaults,
			...saved,
			leftSize: clampSize("left", saved.leftSize ?? defaults.leftSize),
			rightSize: clampSize("right", saved.rightSize ?? defaults.rightSize),
			zoomed: false,
		};
	} catch {
		return { ...defaults };
	}
}

export const docks = $state<DockState>(load());

/** Transient edge-peek of a hidden sidebar; never persisted. */
export const peek = $state<{ left: boolean; right: boolean }>({ left: false, right: false });

$effect.root(() => {
	$effect(() => {
		const { zoomed: _, ...keep } = docks;
		localStorage.setItem(KEY, JSON.stringify(keep));
	});
});

/** Show `panel` in the left dock; toggles the dock off if it is already showing it. */
export function toggleLeftPanel(panel: LeftPanel) {
	if (docks.left && docks.leftPanel === panel) docks.left = false;
	else {
		docks.leftPanel = panel;
		docks.left = true;
	}
	docks.zoomed = false;
}

/** Toggle one dock (and leave pane zoom). Pinning or hiding drops any peek. */
export function toggleDock(side: DockSide) {
	docks[side] = !docks[side];
	docks.zoomed = false;
	if (side === "right") peek.right = false;
}

/** Pin / hide the spaces sidebar. Hiding from a peek keeps it visible until the pointer leaves. */
export function toggleSidebar() {
	docks.sidebar = !docks.sidebar;
	peek.left = false;
}

/** All docks: if any is open, close all; else open all. */
export function toggleAllDocks() {
	const any = docks.left || docks.right || docks.bottom;
	docks.left = docks.right = docks.bottom = !any;
	docks.zoomed = false;
}
