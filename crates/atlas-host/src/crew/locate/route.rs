//! URL path -> the route files of a work tree that render it, for the frameworks Atlas projects
//! use: SvelteKit (`src/routes/**/+page.svelte`), Next (`app/**/page.tsx`, `pages/**`) and Flutter
//! (`GoRoute(path: '...')`).
//!
//! A route file is found by *matching* the URL's segments against the file's own path segments, so
//! `[id]` matches any one segment, `[...rest]` / `[[...rest]]` any tail, and `(group)` folders and
//! `@parallel` slots take no part. Several files may match (a static `/settings` beats `/[tab]`):
//! the more specific match comes first.

use std::path::Path;

/// One file that renders the URL path, and how specific the match was (higher is better: static
/// segments count more than parameters, parameters more than catch-alls).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteFile {
    /// Relative to the work tree, `/`-separated.
    pub file: String,
    pub framework: &'static str,
    pub specificity: u32,
}

/// The path of a URL, without query, fragment or a trailing slash (`/` stays `/`).
pub fn url_path(url: &str) -> String {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let path = rest.find('/').map(|i| &rest[i..]).unwrap_or("/");
    let path = path.split(['?', '#']).next().unwrap_or("/");
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".into()
    } else {
        trimmed.to_string()
    }
}

fn segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

/// How a file's route segments match the URL's: `None` when they do not, else the specificity.
fn match_segments(pattern: &[String], url: &[&str]) -> Option<u32> {
    let mut score = 0u32;
    let mut i = 0usize;
    for (pi, seg) in pattern.iter().enumerate() {
        if is_catch_all(seg) {
            // `[[...x]]` may match nothing; `[...x]` needs at least one segment. Either must be last.
            let optional = seg.starts_with("[[");
            if pi != pattern.len() - 1 || (!optional && i >= url.len()) {
                return None;
            }
            return Some(score);
        }
        if is_optional_param(seg) {
            // `[[lang]]` matches one segment or none; it never needs to be last, but it takes the
            // next URL segment only when the rest of the pattern still matches without it.
            let rest = &pattern[pi + 1..];
            if let Some(with) = url.get(i).and_then(|_| match_segments(rest, &url[i + 1..])) {
                return Some(score + 10 + with);
            }
            return match_segments(rest, &url[i..]).map(|r| score + r);
        }
        let part = *url.get(i)?;
        if is_param(seg) {
            score += 10;
        } else if *seg == part {
            score += 100;
        } else {
            return None;
        }
        i += 1;
    }
    (i == url.len()).then_some(score)
}

fn is_catch_all(seg: &str) -> bool {
    seg.starts_with("[...") || seg.starts_with("[[...")
}

fn is_optional_param(seg: &str) -> bool {
    seg.starts_with("[[") && seg.ends_with("]]") && !is_catch_all(seg)
}

fn is_param(seg: &str) -> bool {
    seg.starts_with('[') && seg.ends_with(']')
}

/// The route segments of a folder path: route groups `(x)` and parallel slots `@x` do not appear
/// in the URL.
fn route_segments(dirs: &str) -> Vec<String> {
    dirs.split('/')
        .filter(|s| {
            !s.is_empty() && !(s.starts_with('(') && s.ends_with(')')) && !s.starts_with('@')
        })
        .map(str::to_string)
        .collect()
}

/// Every route file in `files` (relative, `/`-separated paths) that renders `url_path`, best first.
/// A monorepo is handled by matching the framework folders at any depth (`apps/web/src/routes/…`).
pub fn match_files(files: &[String], url_path: &str) -> Vec<RouteFile> {
    let url = segments(url_path);
    let mut out = vec![];
    for f in files {
        if let Some((fw, pattern)) = route_pattern(f)
            && let Some(score) = match_segments(&pattern, &url)
        {
            out.push(RouteFile {
                file: f.clone(),
                framework: fw,
                specificity: score,
            });
        }
    }
    out.sort_by(|a, b| {
        b.specificity
            .cmp(&a.specificity)
            .then(a.file.len().cmp(&b.file.len()))
            .then(a.file.cmp(&b.file))
    });
    out
}

/// The framework and route segments a file declares, or `None` when it is not a route file.
fn route_pattern(file: &str) -> Option<(&'static str, Vec<String>)> {
    let name = file.rsplit('/').next()?;
    // SvelteKit: .../routes/<dirs>/+page.svelte (also +page@layout.svelte).
    if name == "+page.svelte" || (name.starts_with("+page@") && name.ends_with(".svelte")) {
        let (_, after) = split_last_dir(file, "routes")?;
        return Some(("sveltekit", route_segments(dir_of(after))));
    }
    // Next app router: .../app/<dirs>/page.(tsx|jsx|ts|js|mdx)
    if matches!(
        name,
        "page.tsx" | "page.jsx" | "page.ts" | "page.js" | "page.mdx"
    ) && let Some((_, after)) = split_last_dir(file, "app")
    {
        return Some(("next-app", route_segments(dir_of(after))));
    }
    // Next pages router: .../pages/<dirs>/<name>.(tsx|jsx|ts|js); `index` is the folder itself.
    if let Some((_, after)) = split_last_dir(file, "pages") {
        let (stem, ext) = name.rsplit_once('.')?;
        if !matches!(ext, "tsx" | "jsx" | "ts" | "js") || stem.starts_with('_') {
            return None;
        }
        // `pages/api/**` is the API, not a page.
        if after.starts_with("api/") {
            return None;
        }
        let mut segs = route_segments(dir_of(after));
        if stem != "index" {
            segs.push(stem.to_string());
        }
        return Some(("next-pages", segs));
    }
    None
}

/// Split `file` at the last directory called `dir`: `(before, after)` where `after` is the rest of
/// the path below it (`None` when no such directory is in it).
fn split_last_dir<'a>(file: &'a str, dir: &str) -> Option<(&'a str, &'a str)> {
    let needle = format!("/{dir}/");
    if let Some(i) = file.rfind(&needle) {
        return Some((&file[..i], &file[i + needle.len()..]));
    }
    file.strip_prefix(&format!("{dir}/")).map(|r| ("", r))
}

/// The directory part of a path below the routes root (`a/b/page.tsx` -> `a/b`, `page.tsx` -> ``).
fn dir_of(after: &str) -> &str {
    after.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

/// Flutter: the `GoRoute(path: '/x/:id')` declarations in one Dart file, with the 1-based line of the
/// `GoRoute(` call. `path:` is read from the call's own arguments, so the formatter's one-argument-
/// per-line layout works as well as the one-liner.
pub fn go_routes(source: &str) -> Vec<(String, usize)> {
    let mut out = vec![];
    let mut from = 0;
    while let Some(i) = source[from..].find("GoRoute(") {
        let start = from + i + "GoRoute(".len();
        from = start;
        // Look for `path:` in the next few arguments, not in some later route.
        let window = &source[start..source.len().min(start + 200)];
        let window = window.split("GoRoute(").next().unwrap_or(window);
        let Some(p) = window.find("path:") else {
            continue;
        };
        let after = window[p + 5..].trim_start();
        if let Some(q) = after.chars().next().filter(|c| *c == '\'' || *c == '"')
            && let Some(end) = after[1..].find(q)
        {
            let line = source[..start].matches('\n').count() + 1;
            out.push((after[1..1 + end].to_string(), line));
        }
    }
    out
}

/// Whether a `GoRoute` path (nested ones are relative, `:id` is a parameter) can render the URL
/// path. A relative path matches the tail of the URL: nesting is not resolved, so `settings` matches
/// `/app/settings` as well as `/settings` and the specificity says how much was matched.
pub fn go_route_matches(route: &str, url_path: &str) -> Option<u32> {
    let pattern: Vec<String> = segments(route)
        .iter()
        .map(|s| {
            s.strip_prefix(':')
                .map(|p| format!("[{p}]"))
                .unwrap_or_else(|| s.to_string())
        })
        .collect();
    let url = segments(url_path);
    if pattern.is_empty() {
        return url.is_empty().then_some(100);
    }
    if route.starts_with('/') {
        return match_segments(&pattern, &url);
    }
    if url.len() < pattern.len() {
        return None;
    }
    // Relative: the pattern must match the last segments of the URL.
    match_segments(&pattern, &url[url.len() - pattern.len()..]).map(|s| s.saturating_sub(1))
}

/// Walk the work tree for the files route matching can look at: relative `/`-separated paths,
/// without dot folders, `node_modules`, `target`, `build` outputs, bounded in count.
pub fn list_files(root: &Path, max: usize) -> Vec<String> {
    use crate::crew::spawn::layout::{Kind, read_dir_beneath};
    let mut out = vec![];
    // Directories as `/`-separated paths below the root, listed without following a link at any component: the
    // tree is the crewmate's and may change under us (a link is `Kind::Other`, so it is neither listed nor entered).
    let mut stack: Vec<String> = vec![String::new()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = read_dir_beneath(root, &dir) else {
            continue;
        };
        for (name, kind) in entries {
            let rel = if dir.is_empty() {
                name.clone()
            } else {
                format!("{dir}/{name}")
            };
            match kind {
                Kind::Dir => {
                    if !name.starts_with('.') && !SKIP_DIRS.contains(&name.as_str()) {
                        stack.push(rel);
                    }
                }
                Kind::File => {
                    out.push(rel);
                    if out.len() >= max {
                        return out;
                    }
                }
                Kind::Other => {}
            }
        }
    }
    out.sort();
    out
}

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "build",
    "dist",
    "out",
    "vendor",
    "Pods",
    ".dart_tool",
];

/// Every route file of the work tree that renders `url_path`, best first: file-system routers by
/// their path, Flutter by the `GoRoute` declarations in `.dart` files.
pub fn routes_for(root: &Path, files: &[String], url_path: &str) -> Vec<RouteFile> {
    let mut out = match_files(files, url_path);
    for f in files.iter().filter(|f| f.ends_with(".dart")) {
        let Ok(src) = crate::crew::spawn::layout::read_beneath(root, f, 512 * 1024) else {
            continue;
        };
        for (route, line) in go_routes(&src) {
            if let Some(score) = go_route_matches(&route, url_path) {
                out.push(RouteFile {
                    file: format!("{f}:{line}"),
                    framework: "flutter",
                    specificity: score,
                });
            }
        }
    }
    out.sort_by(|a, b| {
        b.specificity
            .cmp(&a.specificity)
            .then(a.file.len().cmp(&b.file.len()))
            .then(a.file.cmp(&b.file))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn best(list: &[&str], path: &str) -> Vec<String> {
        match_files(&files(list), path)
            .into_iter()
            .map(|r| r.file)
            .collect()
    }

    #[test]
    fn the_url_path_drops_host_query_fragment_and_trailing_slash() {
        assert_eq!(url_path("http://localhost:5173/"), "/");
        assert_eq!(url_path("http://localhost:5173"), "/");
        assert_eq!(url_path("https://x.dev/a/b/?q=1#top"), "/a/b");
        assert_eq!(url_path("http://x/settings#theme"), "/settings");
    }

    #[test]
    fn sveltekit_static_param_group_and_rest_segments() {
        let tree = [
            "src/routes/+page.svelte",
            "src/routes/settings/+page.svelte",
            "src/routes/blog/[slug]/+page.svelte",
            "src/routes/(app)/dashboard/+page.svelte",
            "src/routes/docs/[...rest]/+page.svelte",
            "src/routes/opt/[[lang]]/+page.svelte",
            "src/routes/settings/+layout.svelte",
            "src/routes/api/+server.ts",
        ];
        assert_eq!(best(&tree, "/"), ["src/routes/+page.svelte"]);
        assert_eq!(
            best(&tree, "/settings"),
            ["src/routes/settings/+page.svelte"]
        );
        assert_eq!(
            best(&tree, "/blog/hello-world"),
            ["src/routes/blog/[slug]/+page.svelte"]
        );
        assert_eq!(
            best(&tree, "/dashboard"),
            ["src/routes/(app)/dashboard/+page.svelte"],
            "a (group) folder is not in the URL"
        );
        assert_eq!(
            best(&tree, "/docs/a/b/c"),
            ["src/routes/docs/[...rest]/+page.svelte"]
        );
        assert!(best(&tree, "/docs").is_empty(), "[...rest] needs a segment");
        assert_eq!(
            best(&tree, "/opt"),
            ["src/routes/opt/[[lang]]/+page.svelte"],
            "[[...]] may be empty"
        );
        assert!(best(&tree, "/blog").is_empty());
        assert!(best(&tree, "/nope").is_empty());
    }

    #[test]
    fn an_optional_param_takes_a_segment_when_there_is_one_and_may_be_followed_by_more() {
        let tree = [
            "src/routes/opt/[[lang]]/+page.svelte",
            "src/routes/shop/[[cat]]/sale/+page.svelte",
        ];
        assert_eq!(
            best(&tree, "/opt/pt"),
            ["src/routes/opt/[[lang]]/+page.svelte"]
        );
        assert_eq!(
            best(&tree, "/shop/sale"),
            ["src/routes/shop/[[cat]]/sale/+page.svelte"]
        );
        assert_eq!(
            best(&tree, "/shop/shoes/sale"),
            ["src/routes/shop/[[cat]]/sale/+page.svelte"]
        );
        assert!(best(&tree, "/opt/pt/extra").is_empty());
    }

    #[test]
    fn sveltekit_in_a_monorepo_app_folder() {
        let tree = ["apps/web/src/routes/pricing/+page.svelte"];
        assert_eq!(best(&tree, "/pricing"), tree);
    }

    #[test]
    fn next_app_router_params_groups_and_slots() {
        let tree = [
            "app/page.tsx",
            "app/about/page.tsx",
            "app/shop/[id]/page.tsx",
            "app/(marketing)/pricing/page.tsx",
            "app/@modal/login/page.tsx",
            "app/blog/[...slug]/page.tsx",
            "app/about/layout.tsx",
            "src/app/team/page.jsx",
        ];
        assert_eq!(best(&tree, "/"), ["app/page.tsx"]);
        assert_eq!(best(&tree, "/about"), ["app/about/page.tsx"]);
        assert_eq!(best(&tree, "/shop/42"), ["app/shop/[id]/page.tsx"]);
        assert_eq!(
            best(&tree, "/pricing"),
            ["app/(marketing)/pricing/page.tsx"]
        );
        assert_eq!(best(&tree, "/login"), ["app/@modal/login/page.tsx"]);
        assert_eq!(best(&tree, "/blog/a/b"), ["app/blog/[...slug]/page.tsx"]);
        assert_eq!(best(&tree, "/team"), ["src/app/team/page.jsx"]);
    }

    #[test]
    fn next_pages_router_index_params_and_not_the_api() {
        let tree = [
            "pages/index.tsx",
            "pages/about.tsx",
            "pages/posts/index.tsx",
            "pages/posts/[id].tsx",
            "pages/_app.tsx",
            "pages/api/users.ts",
            "src/pages/contact.js",
        ];
        assert_eq!(best(&tree, "/"), ["pages/index.tsx"]);
        assert_eq!(best(&tree, "/about"), ["pages/about.tsx"]);
        assert_eq!(best(&tree, "/posts"), ["pages/posts/index.tsx"]);
        assert_eq!(best(&tree, "/posts/9"), ["pages/posts/[id].tsx"]);
        assert!(
            best(&tree, "/api/users").is_empty(),
            "the API is not a page"
        );
        assert!(best(&tree, "/_app").is_empty());
        assert_eq!(best(&tree, "/contact"), ["src/pages/contact.js"]);
    }

    #[test]
    fn a_static_route_beats_a_param_route_that_also_matches() {
        let tree = [
            "src/routes/[tab]/+page.svelte",
            "src/routes/settings/+page.svelte",
        ];
        assert_eq!(
            best(&tree, "/settings"),
            [
                "src/routes/settings/+page.svelte",
                "src/routes/[tab]/+page.svelte"
            ]
        );
    }

    #[test]
    fn go_route_declarations_are_read_with_their_line() {
        let src = "final router = GoRouter(routes: [\n  GoRoute(path: '/', builder: home),\n  GoRoute(\n    path: \"/computers/:id\",\n    builder: x,\n  ),\n  GoRoute(path: 'settings', builder: s),\n]);\n";
        let got = go_routes(src);
        assert_eq!(got[0], ("/".to_string(), 2));
        assert_eq!(
            got[1].0, "/computers/:id",
            "path: on the next line: {got:?}"
        );
        assert!(got.iter().any(|(p, _)| p == "settings"));
    }

    #[test]
    fn go_route_params_and_relative_paths_match() {
        assert!(go_route_matches("/computers/:id", "/computers/abc").is_some());
        assert!(go_route_matches("/computers/:id", "/computers").is_none());
        assert!(go_route_matches("/", "/").is_some());
        assert!(go_route_matches("/", "/x").is_none());
        assert!(
            go_route_matches("settings", "/app/settings").is_some(),
            "a nested route is relative"
        );
        assert!(go_route_matches("settings", "/app/other").is_none());
        // Absolute beats relative for the same URL.
        assert!(
            go_route_matches("/settings", "/settings").unwrap()
                > go_route_matches("settings", "/settings").unwrap()
        );
    }

    #[test]
    fn flutter_routes_are_found_in_dart_files_of_a_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = tmp.path().join("lib");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(
            lib.join("router.dart"),
            "final r = [\n  GoRoute(path: '/home', builder: h),\n  GoRoute(path: '/computers/:id', builder: c),\n];\n",
        )
        .unwrap();
        std::fs::write(lib.join("other.dart"), "class A {}\n").unwrap();
        let listing = list_files(tmp.path(), 100);
        let got = routes_for(tmp.path(), &listing, "/computers/9");
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!(got[0].file, "lib/router.dart:3");
        assert_eq!(got[0].framework, "flutter");
    }

    #[test]
    fn listing_skips_dependencies_dot_folders_and_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        for d in ["src/routes", "node_modules/x", ".git", "target/debug"] {
            std::fs::create_dir_all(r.join(d)).unwrap();
        }
        std::fs::write(r.join("src/routes/+page.svelte"), "x").unwrap();
        std::fs::write(r.join("node_modules/x/+page.svelte"), "x").unwrap();
        std::fs::write(r.join(".git/config"), "x").unwrap();
        std::fs::write(r.join("target/debug/a"), "x").unwrap();
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            std::fs::write(outside.path().join("secret.svelte"), "x").unwrap();
            std::os::unix::fs::symlink(outside.path(), r.join("src/link")).unwrap();
        }
        assert_eq!(list_files(r, 100), ["src/routes/+page.svelte"]);
    }
}
