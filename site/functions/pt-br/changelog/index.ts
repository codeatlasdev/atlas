// GET /pt-br/changelog/ -> the same handler as /changelog/: the Portuguese shell carries its own words and stories, nothing here is language-specific.
export { onRequestGet, onRequestHead } from "../../changelog/index";
