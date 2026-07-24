import { Logo } from "@/components/logo";
import { source } from "@/lib/source";
import { DocsLayout } from "fumadocs-ui/layouts/docs";
import type { ReactNode } from "react";

export default function Layout({ children }: { children: ReactNode }) {
	return (
		<DocsLayout
			tree={source.getPageTree()}
			nav={{
				title: <Logo size={22} />,
			}}
			links={[
				{
					text: "GitHub",
					url: "https://github.com/codeatlasdev/atlas",
					external: true,
				},
			]}
		>
			{children}
		</DocsLayout>
	);
}
