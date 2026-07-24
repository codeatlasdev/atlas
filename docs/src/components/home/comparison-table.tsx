type Cell = true | false | "partial" | string;

interface ComparisonRow {
	feature: string;
	values: Cell[];
}

const columns = ["Atlas", "Coolify", "Kamal", "Raw K8s", "Managed PaaS"];

const rows: ComparisonRow[] = [
	{ feature: "Self-hosted", values: [true, true, true, true, false] },
	{ feature: "K3s + Swarm", values: [true, false, false, "K8s only", false] },
	{ feature: "DNS automation", values: [true, "partial", false, false, true] },
	{ feature: "Encrypted secrets", values: [true, true, false, false, true] },
	{ feature: "Preview environments", values: [true, false, false, false, true] },
	{ feature: "Monitoring included", values: [true, false, false, false, true] },
	{ feature: "Multi-node", values: [true, true, true, true, true] },
	{ feature: "No vendor lock-in", values: [true, true, true, true, false] },
];

function CellValue({ value }: { value: Cell }) {
	if (value === true) return <span className="text-emerald-400">✓</span>;
	if (value === false) return <span className="text-fd-muted-foreground/50">✗</span>;
	if (value === "partial") return <span className="text-amber-400">~</span>;
	return <span className="text-fd-muted-foreground text-xs">{value}</span>;
}

export function ComparisonTable() {
	return (
		<div className="overflow-x-auto rounded-xl border border-fd-border">
			<table className="w-full text-sm">
				<thead>
					<tr className="border-b border-fd-border">
						<th className="px-4 py-3 text-left font-medium text-fd-muted-foreground" />
						{columns.map((col, i) => (
							<th
								key={col}
								className={`px-4 py-3 text-center font-medium ${
									i === 0 ? "bg-fd-primary/5 text-fd-primary" : "text-fd-muted-foreground"
								}`}
							>
								{col}
							</th>
						))}
					</tr>
				</thead>
				<tbody>
					{rows.map((row) => (
						<tr key={row.feature} className="border-b border-fd-border/50 last:border-0">
							<td className="px-4 py-3 text-fd-foreground/80">{row.feature}</td>
							{row.values.map((val, i) => (
								<td
									key={columns[i]}
									className={`px-4 py-3 text-center ${i === 0 ? "bg-fd-primary/[0.03]" : ""}`}
								>
									<CellValue value={val} />
								</td>
							))}
						</tr>
					))}
				</tbody>
			</table>
		</div>
	);
}
