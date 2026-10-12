// Injected into every embedded-browser page (Atlas Desktop, Design Mode).
// Holds no secret and makes no requests: the app reads the picked element with
// `__atlasGrab.take()` through Tauri's eval callback, so a page can't reach the app.
(() => {
	if (window.__atlasGrab) return;
	let picked = null;
	const ATTRS = ["id", "class", "role", "name", "type", "href", "src", "alt", "placeholder"];
	const STYLES = [
		"display", "position", "width", "height", "margin", "padding", "color", "background-color",
		"font-family", "font-size", "font-weight", "line-height", "border", "border-radius", "gap", "flex-direction",
	];
	const SECRET = /(token|secret|password|api[_-]?key)=\S+/gi;
	const redact = (s) => String(s ?? "").replace(SECRET, "$1=[redacted]");

	function selector(el) {
		if (el.id) return `#${CSS.escape(el.id)}`;
		const parts = [];
		for (let e = el; e && e.nodeType === 1 && e !== document.body && parts.length < 6; e = e.parentElement) {
			if (e.id) {
				parts.unshift(`#${CSS.escape(e.id)}`);
				break;
			}
			let p = e.tagName.toLowerCase();
			const cls = [...e.classList].filter((c) => !/^svelte-|^css-|\d{3,}/.test(c)).slice(0, 2);
			if (cls.length) p += "." + cls.map((c) => CSS.escape(c)).join(".");
			const sib = e.parentElement ? [...e.parentElement.children].filter((x) => x.tagName === e.tagName) : [];
			if (sib.length > 1) p += `:nth-of-type(${sib.indexOf(e) + 1})`;
			parts.unshift(p);
		}
		return parts.join(" > ");
	}

	function describe(el) {
		const r = el.getBoundingClientRect();
		const cs = getComputedStyle(el);
		const attributes = {};
		for (const a of el.attributes) if (ATTRS.includes(a.name) || a.name.startsWith("aria-")) attributes[a.name] = redact(a.value);
		const ancestors = [];
		for (let e = el.parentElement; e && e !== document.body && ancestors.length < 5; e = e.parentElement) ancestors.push(selector(e));
		const nearby = [...(el.parentElement?.children ?? [])]
			.filter((x) => x !== el)
			.map((x) => x.innerText?.trim())
			.filter(Boolean)
			.slice(0, 3);
		return {
			url: redact(location.href),
			title: document.title,
			viewport: { width: innerWidth, height: innerHeight, dpr: devicePixelRatio },
			target: {
				tagName: el.tagName.toLowerCase(),
				selector: selector(el),
				htmlSnippet: redact(el.outerHTML).slice(0, 4096),
				textSnippet: redact(el.innerText ?? "").slice(0, 500),
				attributes,
				computedStyles: Object.fromEntries(STYLES.map((k) => [k, cs.getPropertyValue(k)])),
				rectViewport: { x: r.x, y: r.y, width: r.width, height: r.height },
				rectPage: { x: r.x + scrollX, y: r.y + scrollY, width: r.width, height: r.height },
			},
			ancestorPath: ancestors,
			nearbyText: nearby.map((t) => redact(t).slice(0, 200)),
		};
	}

	let box = null;
	let on = false;
	const over = (e) => {
		if (!box) return;
		const r = e.target.getBoundingClientRect();
		Object.assign(box.style, { left: `${r.x}px`, top: `${r.y}px`, width: `${r.width}px`, height: `${r.height}px`, display: "block" });
	};
	const click = (e) => {
		e.preventDefault();
		e.stopPropagation();
		picked = describe(e.target);
		disable();
	};
	function enable() {
		if (on) return;
		on = true;
		box = document.createElement("div");
		Object.assign(box.style, {
			position: "fixed", zIndex: 2147483647, pointerEvents: "none", display: "none",
			outline: "2px solid #e5484d", background: "rgba(229,72,77,.12)", borderRadius: "3px",
		});
		document.documentElement.appendChild(box);
		addEventListener("mouseover", over, true);
		addEventListener("click", click, true);
	}
	function disable() {
		on = false;
		box?.remove();
		box = null;
		removeEventListener("mouseover", over, true);
		removeEventListener("click", click, true);
	}

	// Accessibility-ish snapshot with stable refs for `atlas browser click @eN`.
	const ROLE_TAGS = { a: "link", button: "button", input: "textbox", textarea: "textbox", select: "combobox", h1: "heading", h2: "heading", h3: "heading", img: "img" };
	function snapshot() {
		window.__atlasRefs = {};
		const out = [];
		let n = 0;
		for (const el of document.querySelectorAll("a,button,input,textarea,select,[role],h1,h2,h3,img,label")) {
			const r = el.getBoundingClientRect();
			if (!r.width || !r.height || getComputedStyle(el).visibility === "hidden") continue;
			const ref = `e${++n}`;
			window.__atlasRefs[ref] = el;
			const role = el.getAttribute("role") || ROLE_TAGS[el.tagName.toLowerCase()] || el.tagName.toLowerCase();
			const name = (el.getAttribute("aria-label") || el.innerText || el.value || el.placeholder || el.alt || "").trim().slice(0, 80);
			out.push({ ref: `@${ref}`, role, name: redact(name), rect: { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height) } });
			if (n >= 300) break;
		}
		return { url: location.href, title: document.title, nodes: out };
	}
	const byRef = (ref) => window.__atlasRefs?.[String(ref).replace(/^@/, "")];
	function clickRef(ref) {
		const el = byRef(ref);
		if (!el) return { ok: false, error: `unknown ref ${ref}; take a new snapshot` };
		el.scrollIntoView({ block: "center" });
		el.click();
		return { ok: true };
	}
	function fillRef(ref, text) {
		const el = byRef(ref);
		if (!el) return { ok: false, error: `unknown ref ${ref}; take a new snapshot` };
		el.focus();
		const proto = el.tagName === "TEXTAREA" ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
		Object.getOwnPropertyDescriptor(proto, "value")?.set?.call(el, text);
		el.dispatchEvent(new Event("input", { bubbles: true }));
		el.dispatchEvent(new Event("change", { bubbles: true }));
		return { ok: true };
	}

	function take() {
		const p = picked;
		picked = null;
		return p;
	}

	window.__atlasGrab = { enable, disable, take, active: () => on, snapshot, clickRef, fillRef };
})();
