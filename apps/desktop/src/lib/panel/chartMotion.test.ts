// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import * as echarts from "echarts/core";
import { LineChart } from "echarts/charts";
import { GridComponent } from "echarts/components";
import { SVGRenderer } from "echarts/renderers";
import "./chartMotion.ts";

// Incident (#289, docs/perf/linux-webkit-study.md): the kit's line series animate every update, and the dashboard feeds five
// charts a new sample every few seconds, so one of them was always mid-animation: ~7 points of a core in the web process on a
// running stack. The real ECharts, with the series the kit builds (`animationDuration: 1000, animationEasing: "cubicOut"`):
// whatever an option asks for, the chart that results has animation off.
echarts.use([LineChart, GridComponent, SVGRenderer]);
const kitLineSeries = (data: number[]) => ({
	type: "line",
	data,
	animationDuration: 1000,
	animationEasing: "cubicOut",
});

test("a chart given the kit's animated series ends up with animation off, on first render and on every update", () => {
	const chart = echarts.init(null, null, { renderer: "svg", ssr: true, width: 200, height: 100 });
	const option = (data: number[]) => ({
		xAxis: { type: "category", data: data.map((_, i) => i) },
		yAxis: { type: "value" },
		series: [kitLineSeries(data)],
	});
	chart.setOption(option([1, 2, 3]));
	expect(chart.getOption().animation).toBe(false);
	chart.setOption(option([2, 3, 4]), { notMerge: false, lazyUpdate: false });
	expect(chart.getOption().animation).toBe(false);
	expect(chart.getOption().series[0].data).toEqual([2, 3, 4]);
	chart.dispose();
});

test("an option that explicitly asks for animation is overridden too (the policy is the app's, not the series')", () => {
	const chart = echarts.init(null, null, { renderer: "svg", ssr: true, width: 200, height: 100 });
	chart.setOption({
		animation: true,
		xAxis: { type: "category", data: [0] },
		yAxis: {},
		series: [kitLineSeries([1])],
	});
	expect(chart.getOption().animation).toBe(false);
	chart.dispose();
});

test("every component that mounts a kit chart pulls in the policy", async () => {
	const { Glob } = await import("bun");
	const { readFileSync } = await import("node:fs");
	const root = new URL("../../", import.meta.url).pathname;
	const bare = [...new Glob("**/*.svelte").scanSync(root)].filter((f) => {
		const s = readFileSync(root + f, "utf8");
		return /@codeatlasdev\/ui\/charts/.test(s) && !/chartMotion\.ts/.test(s);
	});
	expect(bare).toEqual([]);
});
