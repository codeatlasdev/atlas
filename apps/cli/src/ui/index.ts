/**
 * Atlas TUI — UI abstraction layer over OpenTUI.
 *
 * Two modes:
 * - Interactive (default): Full TUI with OpenTUI renderer
 * - Non-interactive (--yes): Plain console output for CI/CD
 */

import { Box, type Renderable, Text, createCliRenderer } from "@opentui/core";

// ── Theme ──

export const theme = {
	primary: "#8B5CF6",
	success: "#22C55E",
	error: "#EF4444",
	warning: "#F59E0B",
	muted: "#6B7280",
	bg: "#0F0F0F",
	bgPanel: "#1A1A2E",
	border: "#2D2D44",
	text: "#E5E5E5",
} as const;

// ── Renderer singleton ──

let _renderer: Awaited<ReturnType<typeof createCliRenderer>> | null = null;

export async function getRenderer() {
	if (!_renderer) _renderer = await createCliRenderer();
	return _renderer;
}

export function destroyRenderer() {
	_renderer?.destroy();
	_renderer = null;
}

// ── Components ──

export function Header(title: string, subtitle?: string) {
	const box = Box({
		width: "100%",
		flexDirection: "row",
		gap: 2,
		paddingBottom: 1,
	});

	box.add(Text({ content: "◆", fg: theme.primary, bold: true }));
	box.add(Text({ content: title, fg: theme.text, bold: true }));
	if (subtitle) box.add(Text({ content: subtitle, fg: theme.muted }));

	return box;
}

export function StatusLine(label: string, value: string, color = theme.text) {
	const row = Box({ flexDirection: "row", gap: 1 });
	row.add(Text({ content: `  ${label}`, fg: theme.muted }));
	row.add(Text({ content: value, fg: color }));
	return row;
}

export function Divider() {
	return Text({ content: "─".repeat(60), fg: theme.border });
}

// ── Updatable text helper ──

export function MutableText(initial: string, color = theme.text) {
	const node = Text({ content: initial, fg: color }) as Renderable & Record<string, unknown>;
	return {
		node,
		update(content: string, fg?: string) {
			node.content = content;
			if (fg) node.fg = fg;
		},
	};
}

// ── Spinner ──

export interface SpinnerHandle {
	start(msg: string): void;
	stop(msg: string): void;
	fail(msg: string): void;
}

export function createSpinner(auto: boolean): SpinnerHandle {
	if (auto) {
		return {
			start: (m) => console.log(`→ ${m}`),
			stop: (m) => console.log(`✓ ${m}`),
			fail: (m) => console.error(`✗ ${m}`),
		};
	}

	const FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
	let frame = 0;
	let interval: ReturnType<typeof setInterval> | null = null;
	const text = MutableText("", theme.primary);

	return {
		start(msg) {
			if (interval) clearInterval(interval);
			text.update(`${FRAMES[0]} ${msg}`, theme.primary);
			frame = 0;
			interval = setInterval(() => {
				frame = (frame + 1) % FRAMES.length;
				text.update(`${FRAMES[frame]} ${msg}`, theme.primary);
			}, 80);
		},
		stop(msg) {
			if (interval) clearInterval(interval);
			interval = null;
			text.update(`✓ ${msg}`, theme.success);
		},
		fail(msg) {
			if (interval) clearInterval(interval);
			interval = null;
			text.update(`✗ ${msg}`, theme.error);
		},
	};
}

// ── Deploy Panel ──

export interface DeployStep {
	name: string;
	status: "pending" | "running" | "done" | "failed";
}

export function DeployPanel(title: string, steps: DeployStep[]) {
	const container = Box({
		width: "100%",
		flexDirection: "column",
		border: true,
		borderStyle: "rounded",
		borderColor: theme.border,
		padding: 1,
	});

	container.add(Text({ content: `  ${title}`, fg: theme.primary, bold: true }));
	container.add(Text({ content: "", fg: theme.border }));

	for (const step of steps) {
		const icon =
			step.status === "done"
				? "✓"
				: step.status === "failed"
					? "✗"
					: step.status === "running"
						? "◆"
						: "○";
		const color =
			step.status === "done"
				? theme.success
				: step.status === "failed"
					? theme.error
					: step.status === "running"
						? theme.primary
						: theme.muted;

		container.add(Text({ content: `  ${icon} ${step.name}`, fg: color }));
	}

	return container;
}
