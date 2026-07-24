interface Step {
	number: number;
	title: string;
	code: string;
	time: string;
}

export function StepFlow({ steps }: { steps: Step[] }) {
	return (
		<div className="grid grid-cols-1 gap-6 md:grid-cols-3">
			{steps.map((step) => (
				<div key={step.number} className="relative">
					<div className="mb-3 flex items-center gap-3">
						<span className="flex size-8 items-center justify-center rounded-full bg-fd-primary/10 text-sm font-bold text-fd-primary">
							{step.number}
						</span>
						<span className="text-sm font-medium text-fd-foreground">{step.title}</span>
					</div>
					<div className="rounded-lg border border-fd-border bg-fd-background px-4 py-3 font-[family-name:var(--font-mono)] text-sm text-fd-muted-foreground">
						<span className="text-fd-muted-foreground/60">$ </span>
						{step.code}
					</div>
					<div className="mt-2 text-xs text-fd-muted-foreground/60">{step.time}</div>
				</div>
			))}
		</div>
	);
}
