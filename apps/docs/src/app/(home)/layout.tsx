import { Logo } from "@/components/logo";
import { HomeLayout } from "fumadocs-ui/layouts/home";
import type { ReactNode } from "react";

export default function Layout({ children }: { children: ReactNode }) {
	return (
		<HomeLayout
			nav={{
				title: <Logo size={22} />,
			}}
			links={[
				{ text: "Docs", url: "/docs" },
				{
					text: "GitHub",
					url: "https://github.com/codeatlasdev/atlas",
					external: true,
				},
			]}
		>
			{children}
		</HomeLayout>
	);
}
