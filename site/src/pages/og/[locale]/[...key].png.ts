// /og/<locale>/<key>.png: the share image of every page, rendered at build time (src/lib/og-render.ts). `<locale>` is `en` or `pt-br`.
import type { APIRoute, GetStaticPaths } from "astro";
import { ogEntries } from "../../../lib/og-cards";
import { renderOg } from "../../../lib/og-render";
import { ogSegment } from "../../../lib/og";

export const getStaticPaths = (async () =>
	(await ogEntries()).map(({ locale, key, card }) => ({ params: { locale: ogSegment(locale), key }, props: { card } }))) satisfies GetStaticPaths;

export const GET: APIRoute = async ({ props }) => new Response(new Uint8Array(await renderOg(props.card)), { headers: { "content-type": "image/png" } });
