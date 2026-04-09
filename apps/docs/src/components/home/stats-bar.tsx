export function StatsBar({ stats }: { stats: string[] }) {
	return (
		<div className="flex flex-wrap items-center justify-center gap-x-6 gap-y-2 border-y border-fd-border py-4 text-sm text-fd-muted-foreground">
			{stats.map((stat, i) => (
				<span key={stat} className="flex items-center gap-2">
					{i > 0 && <span className="hidden text-fd-border sm:inline">·</span>}
					{stat}
				</span>
			))}
		</div>
	);
}
