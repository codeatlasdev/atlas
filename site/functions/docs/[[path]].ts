// English page with a /pt-br/ twin: a visitor who wants Portuguese (choice, country, browser) is redirected to it; see _lib/locale.ts.
import { localizedPage } from "../_lib/locale";

export const onRequestGet = localizedPage;
export const onRequestHead = localizedPage;
