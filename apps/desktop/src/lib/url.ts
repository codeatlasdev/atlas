/** Address-bar text → URL: bare hosts get http(s), anything else is a search. */
export function toUrl(input: string): string {
	const s = input.trim();
	if (/^https?:\/\//i.test(s)) return s;
	if (/^(localhost|127\.0\.0\.1|\[::1\])(:\d+)?(\/|$)/i.test(s)) return `http://${s}`;
	if (/^[\w-]+(\.[\w-]+)+(:\d+)?(\/\S*)?$/.test(s) && !s.includes(" ")) return `https://${s}`;
	return `https://duckduckgo.com/?q=${encodeURIComponent(s)}`;
}
