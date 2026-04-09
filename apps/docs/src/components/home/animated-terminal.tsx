"use client";

interface TerminalLine {
	text: string;
	type: "command" | "output" | "success" | "blank";
}

export function AnimatedTerminal({
	lines,
	title = "Terminal",
}: {
	lines: TerminalLine[];
	title?: string;
}) {
	return (
		<div className="w-full max-w-2xl overflow-hidden rounded-xl border border-fd-border bg-fd-background shadow-[0_0_60px_rgba(129,140,248,0.06)]">
			<div className="flex items-center gap-2 border-b border-fd-border px-4 py-3">
				<span className="size-3 rounded-full bg-red-400" />
				<span className="size-3 rounded-full bg-amber-400" />
				<span className="size-3 rounded-full bg-emerald-400" />
				<span className="ml-2 text-xs text-fd-muted-foreground">{title}</span>
			</div>
			<div className="p-4 font-[family-name:var(--font-mono)] text-sm leading-relaxed">
				{lines.map((line, i) => (
					<div
						key={`${line.type}-${line.text}-${i}`}
						className="animate-fade-in-up opacity-0"
						style={{ animationDelay: `${400 + i * 120}ms` }}
					>
						{line.type === "blank" ? (
							<div className="h-4" />
						) : line.type === "command" ? (
							<div>
								<span className="text-fd-muted-foreground">$ </span>
								<span className="text-fd-foreground">{line.text}</span>
							</div>
						) : line.type === "success" ? (
							<div>
								<span className="text-emerald-400">✓ </span>
								<span className="text-fd-muted-foreground">{line.text}</span>
							</div>
						) : (
							<div className="text-fd-muted-foreground">{line.text}</div>
						)}
					</div>
				))}
				<span className="inline-block h-4 w-2 animate-[terminal-cursor_1s_infinite] bg-fd-primary" />
			</div>
		</div>
	);
}
