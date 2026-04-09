import type { LucideIcon } from "lucide-react";
import { GlowCard } from "./glow-card";

interface BentoItem {
	title: string;
	description: string;
	icon: LucideIcon;
	wide?: boolean;
}

export function BentoGrid({ items }: { items: BentoItem[] }) {
	return (
		<div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
			{items.map((item) => (
				<GlowCard key={item.title} className={item.wide ? "lg:col-span-2" : ""}>
					<item.icon className="mb-3 size-5 text-fd-primary" />
					<h3 className="mb-1 text-lg font-semibold text-fd-foreground">{item.title}</h3>
					<p className="text-sm leading-relaxed text-fd-muted-foreground">{item.description}</p>
				</GlowCard>
			))}
		</div>
	);
}
