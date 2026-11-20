// Build-time data of the landing page (components/home/Home.astro): media URLs and the structured data. The copy is in i18n/messages/home_*.
import { mediaVersion } from "./media";
import { SITE } from "./site";

/** `/media/<f>?v=<version>`; the version comes from the tracked manifest (see media.ts). */
export const mediaUrl = (f: string) => {
	const v = mediaVersion(f);
	return v ? `/media/${f}?v=${v}` : `/media/${f}`;
};

/** schema.org data of the home page in the page's language: the app, and the questions and answers shown on the page. */
export function homeJsonLd(description: string, faq: { q: string; a: string }[]) {
	return {
		"@context": "https://schema.org",
		"@graph": [
			{
				"@type": "SoftwareApplication",
				name: "Atlas",
				applicationCategory: "DeveloperApplication",
				operatingSystem: ["macOS 13+ (Apple silicon)", "Windows 10 (1809+) and 11 (x64)", "Linux x86_64 (glibc 2.35+)"],
				description,
				url: SITE.url,
				offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
				license: ["https://opensource.org/licenses/MIT", "https://opensource.org/licenses/Apache-2.0"],
				publisher: { "@type": "Organization", name: "CodeAtlas", url: SITE.codeatlas },
			},
			{
				"@type": "FAQPage",
				mainEntity: faq.map((f) => ({ "@type": "Question", name: f.q, acceptedAnswer: { "@type": "Answer", text: f.a } })),
			},
		],
	};
}
