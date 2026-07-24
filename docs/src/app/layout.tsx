import { RootProvider } from "fumadocs-ui/provider/next";
import type { Metadata } from "next";
import { Inter, JetBrains_Mono } from "next/font/google";
import type { ReactNode } from "react";
import "./global.css";

const inter = Inter({ subsets: ["latin"] });
const mono = JetBrains_Mono({ subsets: ["latin"], variable: "--font-mono" });

export const metadata: Metadata = {
	title: {
		default: "Atlas",
		template: "%s | Atlas",
	},
	description:
		"Your servers. Zero config. Production-ready. The open-source platform that turns any VPS into a full-stack deployment target.",
};

export default function RootLayout({ children }: { children: ReactNode }) {
	return (
		<html lang="en" className={`${inter.className} ${mono.variable}`} suppressHydrationWarning>
			<body className="flex min-h-screen flex-col">
				<RootProvider>{children}</RootProvider>
			</body>
		</html>
	);
}
