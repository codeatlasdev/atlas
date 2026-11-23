import { registerPreprocessor } from "echarts/core";

/**
 * The dashboard charts (service sparklines, the resources chart) take a new sample every few seconds, and the kit's
 * series animate every update (500-1000 ms of canvas redraws per chart, five charts), so a running stack always had
 * a chart mid-animation: web process 7 points of a core on an otherwise still dashboard (docs/perf/linux-webkit-study.md).
 * The kit offers no switch, but ECharts runs every option through preprocessors: one that turns animation off makes a
 * sample a single redraw. Imported by every component that mounts a kit chart.
 */
let done = false;
export function chartsDoNotAnimate() {
	if (done) return;
	done = true;
	registerPreprocessor((option) => {
		(option as { animation?: boolean }).animation = false;
	});
}
chartsDoNotAnimate();
