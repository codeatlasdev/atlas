//! `locate` on fixture work trees: the image path rules, the width rules, mapping a chosen element
//! to route and component files, and the full handler against a fake runner that writes a canned
//! DOM report (no browser needed).

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::rank::DomReport;
use super::*;
use crate::crew::capture::tests::fake_runner;

/// PNG header (signature + IHDR) of a `width` x `height` image: all the width reader looks at.
pub(crate) fn png_header(width: u32, height: u32) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend_from_slice(&13u32.to_be_bytes());
    v.extend_from_slice(b"IHDR");
    v.extend_from_slice(&width.to_be_bytes());
    v.extend_from_slice(&height.to_be_bytes());
    v.extend_from_slice(&[8, 6, 0, 0, 0]);
    v
}

/// The canned page the fake runner reports: `/pricing`, a card with a buy button.
pub(crate) fn pricing_dom(url: &str, width: u32) -> Value {
    json!({
        "url": url, "width": width, "page_w": width, "page_h": 2000,
        "elements": [
            {"tag":"body","selector":"body","x":0,"y":0,"w":width,"h":2000,"depth":0},
            {"tag":"main","role":"main","selector":"main","x":0,"y":80,"w":width,"h":1700,"depth":1,"main":true},
            {"tag":"div","testid":"plan-card","selector":"[data-testid=\"plan-card\"]","x":200,"y":400,"w":400,"h":300,"depth":2},
            {"tag":"button","role":"button","name":"Buy now","text":"Buy now","testid":"buy-button",
             "selector":"[data-testid=\"buy-button\"]","x":240,"y":620,"w":160,"h":48,"depth":3}
        ]
    })
}

/// A fake runner that writes the PNG and the DOM report for the one cell `locate` asks for.
pub(crate) fn dom_runner(dir: &Path, dom: &Value) -> std::path::PathBuf {
    let body = format!(
        "W=$(printf '%s' \"$SPEC\" | sed -n 's/.*\"widths\":\\[\\([0-9]*\\)\\].*/\\1/p')\n\
         printf 'PNG' > \"$OUT/chromium-${{W}}px-light.png\"\n\
         cat > \"$OUT/chromium-${{W}}px-light.dom.json\" <<'DOMEOF'\n{dom}\nDOMEOF\n\
         printf '{{\"shots\":[{{\"engine\":\"chromium\",\"width\":%s,\"theme\":\"light\",\"path\":\"%s/chromium-%spx-light.png\",\"dom\":\"%s/chromium-%spx-light.dom.json\"}}]}}' \"$W\" \"$OUT\" \"$W\" \"$OUT\" \"$W\"\n"
    );
    fake_runner(dir, &body)
}

/// A SvelteKit tree with a pricing route whose card lives in a component.
pub(crate) fn sveltekit_tree(root: &Path) {
    let w = |p: &str, c: &str| {
        let f = root.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, c).unwrap();
    };
    w(
        "src/routes/pricing/+page.svelte",
        "<script>import PlanCard from '$lib/PlanCard.svelte'</script>\n<h1>Pricing</h1>\n<PlanCard />\n",
    );
    w(
        "src/lib/PlanCard.svelte",
        "<div data-testid=\"plan-card\"><button data-testid=\"buy-button\">Buy now</button></div>\n",
    );
    w("src/routes/+page.svelte", "<h1>Home</h1>\n");
}

#[test]
fn image_widths_are_read_from_the_headers() {
    assert_eq!(image_width(&png_header(390, 800)), Some(390));
    assert_eq!(image_width(&png_header(1440, 3000)), Some(1440));
    // GIF: "GIF89a" then little-endian width.
    let mut gif = b"GIF89a".to_vec();
    gif.extend_from_slice(&[0x86, 0x01, 0x10, 0x00]);
    assert_eq!(image_width(&gif), Some(390));
    // JPEG: SOI, a SOF0 segment with height then width.
    let jpeg = [
        0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x03, 0x20, 0x05, 0xa0, 0x03, 0x01, 0x22, 0x00,
        0x02, 0x11, 0x01, 0x03, 0x11, 0x01,
    ];
    assert_eq!(image_width(&jpeg), Some(1440));
    assert_eq!(image_width(b"not an image at all, just text"), None);
    assert_eq!(image_width(b""), None);
}

#[test]
fn the_capture_width_follows_the_image_only_for_the_two_atlas_widths() {
    assert_eq!(capture_width(None, Some(390)), 390);
    assert_eq!(capture_width(None, Some(1440)), 1440);
    assert_eq!(
        capture_width(None, Some(1170)),
        1440,
        "a retina phone shot is not 390"
    );
    assert_eq!(capture_width(None, None), 1440);
    assert_eq!(capture_width(Some(768), Some(390)), 768, "asked wins");
}

#[test]
fn an_image_outside_the_work_tree_is_refused_even_through_a_symlink() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path().join("work");
    let other = tmp.path().join("other");
    std::fs::create_dir_all(work.join(".atlas/attachments/t1")).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(
        work.join(".atlas/attachments/t1/a.png"),
        png_header(390, 10),
    )
    .unwrap();
    std::fs::write(other.join("secret.png"), png_header(390, 10)).unwrap();

    assert_eq!(
        read_image(&work, ".atlas/attachments/t1/a.png").unwrap(),
        png_header(390, 10)
    );
    let abs = work.join(".atlas/attachments/t1/a.png");
    assert!(read_image(&work, abs.to_str().unwrap()).is_ok());
    assert!(read_image(&work, "./.atlas/attachments/t1/a.png").is_ok());

    let e = read_image(&work, "../other/secret.png").unwrap_err();
    assert!(e.contains("outside your work tree"), "{e}");
    let e = read_image(&work, other.join("secret.png").to_str().unwrap()).unwrap_err();
    assert!(e.contains("outside your work tree"), "{e}");
    std::os::unix::fs::symlink(&other, work.join("link")).unwrap();
    let e = read_image(&work, "link/secret.png").unwrap_err();
    assert!(e.contains("outside your work tree"), "{e}");
    // A link to a file (the last component) is refused the same way.
    std::os::unix::fs::symlink(other.join("secret.png"), work.join("leaf.png")).unwrap();
    let e = read_image(&work, "leaf.png").unwrap_err();
    assert!(e.contains("outside your work tree"), "{e}");
    let e = read_image(&work, "missing.png").unwrap_err();
    assert!(e.contains("no image"), "{e}");
    let e = read_image(&work, ".atlas").unwrap_err();
    assert!(e.contains("not a file"), "{e}");
}

// Security review 5 (the class): `locate` told an agent whether any file the crew user can stat exists
// ("outside your work tree" for one that does, "no image" for one that does not).
#[test]
fn locate_does_not_say_whether_a_path_outside_the_work_tree_exists() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(tmp.path().join("exists.png"), "x").unwrap();
    let say = |p: &str| read_image(&work, p).unwrap_err().replace(p, "<path>");
    assert_eq!(
        say(tmp.path().join("exists.png").to_str().unwrap()),
        say(tmp.path().join("gone.png").to_str().unwrap())
    );
    assert_eq!(say("../exists.png"), say("../gone.png"));
}

#[test]
fn request_arguments_are_validated() {
    assert!(Request::from_args(&json!({"image": "a.png"})).is_ok());
    assert!(
        Request::from_args(
            &json!({"image": "a.png", "rect": [1, 2, 3, 4], "url": "http://x/", "width": 390})
        )
        .is_ok()
    );
    assert!(Request::from_args(&json!({"image": ""})).is_err());
    assert!(Request::from_args(&json!({})).is_err());
    assert!(Request::from_args(&json!({"image": "a", "rect": [1, 2, 0, 4]})).is_err());
    assert!(Request::from_args(&json!({"image": "a", "rect": [1, 2, 3]})).is_err());
    assert!(Request::from_args(&json!({"image": "a", "extra": 1})).is_err());
}

#[test]
fn a_chosen_element_maps_to_its_route_file_and_the_component_that_holds_its_testid() {
    let tmp = tempfile::tempdir().unwrap();
    sveltekit_tree(tmp.path());
    let files = route::list_files(tmp.path(), 1000);
    let dom: DomReport =
        serde_json::from_value(pricing_dom("http://localhost:5173/pricing", 1440)).unwrap();
    let rect = Rect::from_array([235.0, 615.0, 170.0, 58.0]).unwrap();

    let (got, ambiguous) = candidates(tmp.path(), &files, &dom, Some(rect));

    assert!(!ambiguous, "{got:?}");
    let top = &got[0];
    assert_eq!(top.testid.as_deref(), Some("buy-button"));
    assert_eq!(top.selector, "[data-testid=\"buy-button\"]");
    assert_eq!(
        top.route_file.as_deref(),
        Some("src/routes/pricing/+page.svelte")
    );
    assert_eq!(
        top.component_file.as_deref(),
        Some("src/lib/PlanCard.svelte")
    );
    assert_eq!(top.text, "Buy now");
}

#[test]
fn an_element_without_a_testid_is_found_by_its_text_and_unknown_text_has_no_component() {
    let tmp = tempfile::tempdir().unwrap();
    sveltekit_tree(tmp.path());
    let files = route::list_files(tmp.path(), 1000);
    let mut v = pricing_dom("http://localhost:5173/pricing", 1440);
    v["elements"] = json!([
        {"tag":"h2","role":"heading","name":"Pricing","text":"Pricing","selector":"h2","x":0,"y":0,"w":300,"h":40,"depth":2},
        {"tag":"p","text":"Text that is in no source file","selector":"p","x":0,"y":100,"w":300,"h":40,"depth":2}
    ]);
    let dom: DomReport = serde_json::from_value(v).unwrap();

    let (h, _) = candidates(
        tmp.path(),
        &files,
        &dom,
        Some(Rect::from_array([0.0, 0.0, 300.0, 40.0]).unwrap()),
    );
    assert_eq!(h[0].selector, "h2");
    // "Pricing" appears only in the route file, which is reported as the route; the component falls
    // back to it rather than inventing one.
    assert_eq!(
        h[0].component_file.as_deref(),
        Some("src/routes/pricing/+page.svelte")
    );

    let (p, _) = candidates(
        tmp.path(),
        &files,
        &dom,
        Some(Rect::from_array([0.0, 100.0, 300.0, 40.0]).unwrap()),
    );
    assert_eq!(p[0].selector, "p");
    assert_eq!(
        p[0].component_file.as_deref(),
        Some("src/routes/pricing/+page.svelte")
    );
}

#[test]
fn a_url_with_no_route_file_still_returns_the_element_without_a_route() {
    let tmp = tempfile::tempdir().unwrap();
    sveltekit_tree(tmp.path());
    let files = route::list_files(tmp.path(), 1000);
    let dom: DomReport =
        serde_json::from_value(pricing_dom("http://localhost:5173/unknown", 1440)).unwrap();
    let (got, _) = candidates(
        tmp.path(),
        &files,
        &dom,
        Some(Rect::from_array([235.0, 615.0, 170.0, 58.0]).unwrap()),
    );
    assert_eq!(got[0].route_file, None);
    assert_eq!(
        got[0].component_file.as_deref(),
        Some("src/lib/PlanCard.svelte")
    );
}

fn crewmate_in(hostd: &Path, work: &Path) -> crate::crew::store::Crewmate {
    serde_json::from_value(json!({
        "id": "c1", "name": "Loc", "role": "r", "agent": "omp", "approval": "auto", "created_at": 1,
        "workdir": work.to_str().unwrap(),
    }))
    .unwrap_or_else(|e| panic!("crewmate fixture for {}: {e}", hostd.display()))
}

#[tokio::test]
async fn handle_runs_a_dom_capture_and_returns_ranked_candidates_with_sources() {
    let _g = capture::TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let hostd = tmp.path().join("hostd");
    let store = crate::crew::store::CrewStore::new(&hostd);
    let work = tmp.path().join("work");
    sveltekit_tree(&work);
    std::fs::create_dir_all(work.join(".atlas/attachments/t1")).unwrap();
    std::fs::write(
        work.join(".atlas/attachments/t1/shot.png"),
        png_header(1440, 2000),
    )
    .unwrap();
    store.save_crewmate(&crewmate_in(&hostd, &work)).unwrap();
    *capture::TEST_RUNNER.lock() = Some((
        dom_runner(
            tmp.path(),
            &pricing_dom("http://localhost:5173/pricing", 1440),
        ),
        Duration::from_secs(20),
    ));

    let got = handle(
        &hostd,
        &store,
        "c1",
        &json!({"image": ".atlas/attachments/t1/shot.png", "rect": [235, 615, 170, 58], "url": "http://localhost:5173/pricing"}),
    )
    .await;
    *capture::TEST_RUNNER.lock() = None;

    let v = got.unwrap();
    assert_eq!(v["width"], 1440);
    assert_eq!(v["ambiguous"], false);
    let top = &v["candidates"][0];
    assert_eq!(top["testid"], "buy-button");
    assert_eq!(top["route_file"], "src/routes/pricing/+page.svelte");
    assert_eq!(top["component_file"], "src/lib/PlanCard.svelte");
    assert!(
        std::fs::read_dir(hostd.join("capture/runs"))
            .map(|d| d.count() == 0)
            .unwrap_or(true),
        "the scratch dir of the run is removed"
    );
}

#[tokio::test]
async fn a_390_wide_image_is_captured_at_390_and_a_retina_rect_is_scaled_onto_the_page() {
    let _g = capture::TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let hostd = tmp.path().join("hostd");
    let store = crate::crew::store::CrewStore::new(&hostd);
    let work = tmp.path().join("work");
    sveltekit_tree(&work);
    // A 780 px wide image of a page captured at 390: the rect is in image pixels (x2).
    std::fs::write(work.join("retina.png"), png_header(780, 4000)).unwrap();
    std::fs::write(work.join("phone.png"), png_header(390, 2000)).unwrap();
    store.save_crewmate(&crewmate_in(&hostd, &work)).unwrap();
    *capture::TEST_RUNNER.lock() = Some((
        dom_runner(
            tmp.path(),
            &pricing_dom("http://localhost:5173/pricing", 390),
        ),
        Duration::from_secs(20),
    ));

    let phone = handle(
        &hostd, &store, "c1",
        &json!({"image": "phone.png", "rect": [235, 615, 170, 58], "url": "http://localhost:5173/pricing"}),
    )
    .await
    .unwrap();
    assert_eq!(
        phone["width"], 390,
        "an image 390 px wide is captured at 390"
    );
    assert_eq!(phone["candidates"][0]["testid"], "buy-button");

    // Asked width 390 for a 780 px image: the rect [470, 1230, 340, 116] lands on the same button.
    let retina = handle(
        &hostd, &store, "c1",
        &json!({"image": "retina.png", "width": 390, "rect": [470, 1230, 340, 116], "url": "http://localhost:5173/pricing"}),
    )
    .await
    .unwrap();
    *capture::TEST_RUNNER.lock() = None;
    assert_eq!(retina["width"], 390);
    assert_eq!(retina["candidates"][0]["testid"], "buy-button", "{retina}");
}

#[tokio::test]
async fn two_equal_candidates_are_flagged_ambiguous_with_the_instruction_to_ask() {
    let _g = capture::TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let hostd = tmp.path().join("hostd");
    let store = crate::crew::store::CrewStore::new(&hostd);
    let work = tmp.path().join("work");
    sveltekit_tree(&work);
    std::fs::write(work.join("a.png"), png_header(1440, 2000)).unwrap();
    store.save_crewmate(&crewmate_in(&hostd, &work)).unwrap();
    let mut dom = pricing_dom("http://localhost:5173/pricing", 1440);
    dom["elements"] = json!([
        {"tag":"div","testid":"left","selector":"[data-testid=\"left\"]","x":0,"y":0,"w":100,"h":100,"depth":1},
        {"tag":"div","testid":"right","selector":"[data-testid=\"right\"]","x":100,"y":0,"w":100,"h":100,"depth":1}
    ]);
    *capture::TEST_RUNNER.lock() = Some((dom_runner(tmp.path(), &dom), Duration::from_secs(20)));

    let v = handle(
        &hostd, &store, "c1",
        &json!({"image": "a.png", "rect": [50, 0, 100, 100], "url": "http://localhost:5173/pricing"}),
    )
    .await
    .unwrap();
    *capture::TEST_RUNNER.lock() = None;

    assert_eq!(v["ambiguous"], true, "{v}");
    assert!(v["next"].as_str().unwrap().contains("ask_user"), "{v}");
    assert_eq!(v["candidates"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn errors_the_agent_reads_a_missing_url_an_outside_image_a_rect_over_nothing() {
    let _g = capture::TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let hostd = tmp.path().join("hostd");
    let store = crate::crew::store::CrewStore::new(&hostd);
    let work = tmp.path().join("work");
    sveltekit_tree(&work);
    std::fs::write(work.join("a.png"), png_header(1440, 2000)).unwrap();
    std::fs::write(tmp.path().join("outside.png"), png_header(1440, 2000)).unwrap();
    store.save_crewmate(&crewmate_in(&hostd, &work)).unwrap();
    *capture::TEST_RUNNER.lock() = Some((
        dom_runner(
            tmp.path(),
            &pricing_dom("http://localhost:5173/pricing", 1440),
        ),
        Duration::from_secs(20),
    ));

    let e = handle(&hostd, &store, "c1", &json!({"image": "a.png"}))
        .await
        .unwrap_err();
    assert!(e.contains("`url`"), "{e}");
    let e = handle(
        &hostd,
        &store,
        "c1",
        &json!({"image": "../outside.png", "url": "http://x/"}),
    )
    .await
    .unwrap_err();
    assert!(e.contains("outside your work tree"), "{e}");
    let e = handle(
        &hostd,
        &store,
        "c1",
        &json!({"image": "a.png", "url": "file:///etc/passwd"}),
    )
    .await
    .unwrap_err();
    assert!(e.contains("http"), "{e}");
    let e = handle(
        &hostd, &store, "c1",
        &json!({"image": "a.png", "rect": [5000, 5000, 10, 10], "url": "http://localhost:5173/pricing"}),
    )
    .await
    .unwrap_err();
    assert!(e.contains("no element"), "{e}");
    let e = handle(
        &hostd,
        &store,
        "nobody",
        &json!({"image": "a.png", "url": "http://x/"}),
    )
    .await
    .unwrap_err();
    assert!(e.contains("not readable"), "{e}");
    *capture::TEST_RUNNER.lock() = None;
}

// Security review of the crew isolation, N5: `locate` runs in hostd (the crew user) on the crewmate's tree and
// told the agent which files contain a string the agent chose. A file the crewmate swapped for a link to
// `hostd.token` (or a directory for a link) between the listing and the read was read through it, so the agent
// could learn the token one probe at a time. Links are never followed, at any component.
#[cfg(unix)]
#[test]
fn locate_never_reads_through_a_link_out_of_the_tree() {
    use std::os::unix::fs::symlink;
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("work");
    let outside = t.path().join("outside");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("secret.svelte"), "data-testid=\"buy-button\"").unwrap();
    std::fs::write(
        outside.join("app.dart"),
        "GoRoute(path: '/pricing', builder: (c, s) => P())",
    )
    .unwrap();
    std::fs::write(root.join("src/real.svelte"), "data-testid=\"buy-button\"").unwrap();
    symlink(outside.join("secret.svelte"), root.join("src/file.svelte")).unwrap();
    symlink(&outside, root.join("dir")).unwrap();
    symlink(outside.join("app.dart"), root.join("src/app.dart")).unwrap();
    let files: Vec<String> = ["src/file.svelte", "dir/secret.svelte", "src/real.svelte"]
        .map(String::from)
        .into();
    assert_eq!(
        grep_sources(&root, &files, "buy-button"),
        ["src/real.svelte"],
        "a file read through a link out of the tree"
    );
    let routes = route::routes_for(&root, &["src/app.dart".to_string()], "/pricing");
    assert!(
        routes.is_empty(),
        "a Dart file read through a link: {routes:?}"
    );
}
