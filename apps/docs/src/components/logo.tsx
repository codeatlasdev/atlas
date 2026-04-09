import Image from "next/image";

export function Logo({ size = 24 }: { size?: number }) {
	return (
		<span className="flex items-center gap-2">
			<Image src="/logo.svg" alt="" width={size} height={size} />
			<span className="bg-gradient-to-r from-indigo-400 to-sky-400 bg-clip-text text-sm font-bold tracking-tight text-transparent">
				atlas
			</span>
		</span>
	);
}
