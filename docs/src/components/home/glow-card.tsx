import type { ReactNode } from "react";

export function GlowCard({
	children,
	className = "",
}: {
	children: ReactNode;
	className?: string;
}) {
	return (
		<div
			className={`rounded-xl border border-fd-border bg-fd-card p-6 transition-all duration-300 hover:border-fd-ring/20 hover:shadow-[0_0_40px_rgba(129,140,248,0.06)] ${className}`}
		>
			{children}
		</div>
	);
}
