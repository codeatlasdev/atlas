import { describe, expect, test } from "bun:test";
import { Box } from "@opentui/core";
import { createTestRenderer } from "@opentui/core/testing";
import { DeployPanel, Divider, Header, MutableText, StatusLine, theme } from "./index";

// Helper: render components and capture text frame
async function render(
	width: number,
	height: number,
	setup: (root: ReturnType<typeof Box>) => void,
) {
	const { renderer, renderOnce, captureCharFrame } = await createTestRenderer({ width, height });
	const root = Box({ width: "100%", flexDirection: "column" });
	setup(root);
	renderer.root.add(root);
	await renderOnce();
	const frame = captureCharFrame();
	renderer.destroy();
	return frame;
}

describe("Header", () => {
	test("renders diamond, title, and subtitle", async () => {
		const frame = await render(60, 5, (root) => {
			root.add(Header("atlas status", "demo mode"));
		});
		expect(frame).toContain("◆");
		expect(frame).toContain("atlas status");
		expect(frame).toContain("demo mode");
	});

	test("renders without subtitle", async () => {
		const frame = await render(60, 5, (root) => {
			root.add(Header("atlas deploy"));
		});
		expect(frame).toContain("◆");
		expect(frame).toContain("atlas deploy");
	});
});

describe("StatusLine", () => {
	test("renders label and value", async () => {
		const frame = await render(60, 3, (root) => {
			root.add(StatusLine("project", "atlas-app"));
		});
		expect(frame).toContain("project");
		expect(frame).toContain("atlas-app");
	});
});

describe("Divider", () => {
	test("renders horizontal line", async () => {
		const frame = await render(70, 3, (root) => {
			root.add(Divider());
		});
		expect(frame).toContain("─".repeat(10));
	});
});

describe("MutableText", () => {
	test("renders initial content", async () => {
		const frame = await render(60, 3, (root) => {
			const mt = MutableText("  ◆ Loading...", theme.primary);
			root.add(mt.node);
		});
		expect(frame).toContain("◆ Loading...");
	});

	test("exposes update method", () => {
		const mt = MutableText("initial", theme.text);
		expect(typeof mt.update).toBe("function");
		expect(mt.node).toBeDefined();
	});
});

describe("DeployPanel", () => {
	test("renders title and all step icons", async () => {
		const frame = await render(60, 12, (root) => {
			root.add(
				DeployPanel("Deploying", [
					{ name: "Build server", status: "done" },
					{ name: "Build worker", status: "running" },
					{ name: "Push to GHCR", status: "pending" },
					{ name: "Deploy to cluster", status: "failed" },
				]),
			);
		});

		expect(frame).toContain("Deploying");
		expect(frame).toContain("✓ Build server");
		expect(frame).toContain("◆ Build worker");
		expect(frame).toContain("○ Push to GHCR");
		expect(frame).toContain("✗ Deploy to cluster");
	});

	test("renders with border", async () => {
		const frame = await render(60, 8, (root) => {
			root.add(
				DeployPanel("Test", [{ name: "Step 1", status: "done" }]),
			);
		});
		// Rounded border characters
		expect(frame).toContain("╭");
		expect(frame).toContain("╰");
	});
});

describe("full status layout", () => {
	test("renders complete status screen", async () => {
		const frame = await render(70, 18, (root) => {
			root.add(Header("atlas status", "demo mode"));
			root.add(Divider());
			root.add(StatusLine("runtime ", "K3s v1.31.4", theme.primary));
			root.add(StatusLine("node    ", "demo-server 4 CPU 8GB", theme.muted));
			root.add(StatusLine("memory  ", "2148/7892MB (27%)", theme.text));
		});

		expect(frame).toContain("atlas status");
		expect(frame).toContain("demo mode");
		expect(frame).toContain("K3s v1.31.4");
		expect(frame).toContain("demo-server");
		expect(frame).toContain("2148/7892MB");
	});
});
