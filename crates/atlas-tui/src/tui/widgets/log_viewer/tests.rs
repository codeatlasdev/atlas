use super::style::service_color;
use super::*;
use crate::runtime::logs::LogStream;

fn entry(content: &str) -> LogEntry {
    LogEntry {
        seq: 0,
        timestamp: 0,
        service: "api".into(),
        stream: LogStream::Stdout,
        content: content.into(),
        level: LogLevel::Info,
        time_str: "12:00:00".into(),
        mark: false,
        restart: false,
        truncated: false,
    }
}

/// A real buffer + index + height cache over `entries` (one service,
/// seqs rewritten 0..n), so render tests go through the production path.
struct Fixture {
    buf: LogBuffer,
    index: ViewIndex,
    rows: RowCache,
}

impl Fixture {
    fn new(entries: &[LogEntry]) -> Self {
        let mut buf = LogBuffer::new(100_000);
        for e in entries {
            if e.mark {
                buf.push_mark("api".into());
            } else if e.restart {
                buf.push_restart("api".into());
            } else {
                buf.push("api".into(), e.content.clone(), e.stream);
            }
        }
        let key = crate::runtime::logindex::ViewKey {
            service: None,
            hidden: vec![],
            min_level: None,
            filter: None,
            find: None,
            json: false,
            window: Default::default(),
        };
        let mut index = ViewIndex::new();
        index.sync(&buf, &key);
        Self {
            buf,
            index,
            rows: RowCache::new(),
        }
    }

    fn pane<'a>(&'a mut self, width: u16, opts: ViewOpts) -> LogPane<'a> {
        self.rows.sync(
            &self.buf,
            None,
            self.index.seqs(0..self.index.len()),
            text_width_for(width),
            &opts,
        );
        LogPane {
            src: Source {
                buf: &self.buf,
                index: &self.index,
                rows: &self.rows,
                service: None,
            },
            scroll: 0,
            opts,
            find: None,
            cursor: None,
            tab: "All",
            narrowed: false,
            new_lines: 0,
        }
    }
}

fn draw(term: &mut Terminal<backend::TestBackend>, pane: &LogPane<'_>) -> ClickMap {
    let vp = Cell::new(LogViewport::default());
    let clicks = ClickMap::default();
    term.draw(|f| render(f, f.area(), pane, &vp, &clicks))
        .unwrap();
    clicks
}

fn rows_text(term: &Terminal<backend::TestBackend>) -> Vec<String> {
    let buf = term.backend().buffer();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect()
}

/// Row count must equal the rows actually produced (hanging indent
/// included), or the scroll math drifts and the newest lines get cut off.
#[test]
fn entry_rows_matches_wrap_spans() {
    let cases = [
        "short",
        "",
        "\x1b[31merror\x1b[0m with ansi and a fairly long tail of text to wrap around",
        "tabs\there\tand\tthere",
        "wide 日本語のテキストが折り返されるかどうか確認する",
        &"x".repeat(250),
        r#"{"level":30,"msg":"hello world","path":"/a/b/c","n":12345678}"#,
    ];
    let opt_sets = [
        ViewOpts::default(),
        ViewOpts {
            show_service: false,
            ..ViewOpts::default()
        },
        ViewOpts {
            timestamps: false,
            ..ViewOpts::default()
        },
        ViewOpts {
            json: true,
            ..ViewOpts::default()
        },
    ];
    for content in cases {
        let e = entry(content);
        for opts in &opt_sets {
            for w in [1u16, 7, 20, 33, 80] {
                let prefix = Prefix::new(&e, opts).width();
                let produced = wrap_spans(entry_spans(&e, opts, None, None), w as usize, prefix);
                assert_eq!(
                    entry_rows(&e, w, opts),
                    produced.len(),
                    "{content:?} @ {w} {opts:?}"
                );
                for line in &produced {
                    // A single glyph wider than the view (tab, CJK) may overflow alone.
                    assert!(
                        line.width() <= (w as usize).max(TAB_WIDTH),
                        "{content:?} @ {w}"
                    );
                }
            }
        }
    }
}

#[test]
fn continuation_rows_hang_under_the_message_column() {
    let e = entry(&"x".repeat(60));
    let opts = ViewOpts::default();
    let prefix = Prefix::new(&e, &opts).width(); // "12:00:00 " + "[api] " = 15
    assert_eq!(prefix, 15);
    let lines = wrap_spans(entry_spans(&e, &opts, None, None), 30, prefix);
    assert!(lines.len() > 2);
    for line in &lines[1..] {
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.starts_with(&" ".repeat(prefix)), "{text:?}");
        assert!(!text[prefix..].starts_with(' '));
    }
}

#[test]
fn bottom_anchored_shows_newest_and_clamps_scroll() {
    let long = "y".repeat(100);
    let entries: Vec<LogEntry> = (0..20)
        .map(|i| entry(&format!("line{i} {long}")))
        .chain(std::iter::once(entry("NEWEST")))
        .collect();
    let mut fx = Fixture::new(&entries);
    let vp = Cell::new(LogViewport::default());
    let mut term = Terminal::new(backend::TestBackend::new(40, 10)).unwrap();

    let mut screen = |term: &mut Terminal<backend::TestBackend>, scroll| {
        let mut p = fx.pane(40, ViewOpts::default());
        p.scroll = scroll;
        term.draw(|f| render(f, f.area(), &p, &vp, &ClickMap::default()))
            .unwrap();
        format!("{:?}", term.backend().buffer())
    };

    assert!(screen(&mut term, 0).contains("NEWEST"));
    let max = vp.get().max_scroll;
    assert!(max > 0);
    // Over-scrolling clamps to the top instead of blanking the view.
    let top = screen(&mut term, usize::MAX);
    assert!(top.contains("line0"));
    assert!(!top.contains("NEWEST"));
    assert_eq!(top, screen(&mut term, max));
}

#[test]
fn no_blank_top_row_and_single_service_drops_prefix() {
    let entries = [entry("first line"), entry("second line")];
    let mut term = Terminal::new(backend::TestBackend::new(40, 4)).unwrap();
    let opts = ViewOpts {
        show_service: false,
        ..ViewOpts::default()
    };
    draw(&mut term, &Fixture::new(&entries).pane(40, opts));
    let rows = rows_text(&term);
    // No blank top row: content starts on the very first row.
    assert!(rows[0].contains("first line"), "{rows:?}");
    assert!(rows[1].contains("second line"));
    assert!(!rows.iter().any(|r| r.contains("[api]")));

    // Full pane: first row carries content, not padding.
    let many: Vec<LogEntry> = (0..10).map(|i| entry(&format!("l{i}"))).collect();
    draw(&mut term, &Fixture::new(&many).pane(40, opts));
    assert!(rows_text(&term)[0].contains("l6"));
}

#[test]
fn service_palette_avoids_warn_and_error_hues() {
    let t = theme::current();
    let close = |a: Color, b: Color| match (a, b) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let d = |x: u8, y: u8| (x as i32 - y as i32).pow(2);
            d(r1, r2) + d(g1, g2) + d(b1, b2) < 90 * 90
        }
        _ => false,
    };
    for i in 0..200 {
        let c = service_color(&format!("svc-{i}"));
        assert!(!close(c, t.warning) && !close(c, t.error), "{c:?}");
        // Red-dominant colors read as errors; none allowed.
        if let Color::Rgb(r, g, b) = c {
            assert!(!(r > g && r > b), "{c:?} is warm");
        }
    }
}

#[test]
fn smartcase_and_regex_finder() {
    let f = Finder::new("error", false).unwrap();
    assert!(f.is_match("Some ERROR here"), "lowercase ⇒ insensitive");
    let f = Finder::new("Error", false).unwrap();
    assert!(f.is_match("an Error"));
    assert!(!f.is_match("an error"), "uppercase ⇒ sensitive");
    // Literal mode escapes regex syntax.
    let f = Finder::new("a.c", false).unwrap();
    assert!(f.is_match("xa.cx") && !f.is_match("abc"));
    // Regex mode.
    let f = Finder::new(r"fail\w+ \d+", true).unwrap();
    assert_eq!(f.ranges("a failed 42 b failing 7"), vec![2..11, 14..23]);
    // Invalid regex / empty → no finder (caller shows an error state).
    assert!(Finder::new("(", true).is_none());
    assert!(Finder::new("", false).is_none());
}

#[test]
fn find_highlights_all_matches_and_current_is_stronger() {
    let entries = [entry("foo bar foo"), entry("nothing"), entry("foo")];
    let finder = Finder::new("foo", false).unwrap();
    let mut term = Terminal::new(backend::TestBackend::new(60, 5)).unwrap();
    let mut fx = Fixture::new(&entries);
    let mut p = fx.pane(60, ViewOpts::default());
    p.find = Some(FindHighlight {
        finder: &finder,
        current: Some((0, 1)), // second "foo" of the first entry
    });
    draw(&mut term, &p);
    let t = theme::current();
    let buf = term.backend().buffer();
    let (mut hits, mut currents) = (0, 0);
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let c = &buf[(x, y)];
            if c.bg == t.accent {
                currents += 1;
            } else if c.bg == t.bg_float {
                hits += 1;
            }
        }
    }
    // 3 non-current "foo"s ⇒ 2 occurrences × 3 cols; the current one is 3 cols.
    assert_eq!(currents, 3);
    assert_eq!(hits, 6);
}

#[test]
fn empty_filtered_view_says_zero_matches() {
    let mut term = Terminal::new(backend::TestBackend::new(40, 5)).unwrap();
    let mut fx = Fixture::new(&[]);
    let mut p = fx.pane(40, ViewOpts::default());
    p.narrowed = true;
    p.tab = "web";
    draw(&mut term, &p);
    assert!(
        rows_text(&term)
            .iter()
            .any(|r| r.contains("No matches in web"))
    );
    // Not narrowed: plain blank.
    p.narrowed = false;
    draw(&mut term, &p);
    assert!(!rows_text(&term).iter().any(|r| r.contains("matches")));
}

#[test]
fn follow_chip_shows_new_count_and_is_clickable() {
    let long = "z".repeat(100);
    let entries: Vec<LogEntry> = (0..10).map(|_| entry(&long)).collect();
    let mut term = Terminal::new(backend::TestBackend::new(40, 8)).unwrap();
    let mut fx = Fixture::new(&entries);
    let mut p = fx.pane(40, ViewOpts::default());
    p.scroll = 3;
    p.new_lines = 7;
    let clicks = draw(&mut term, &p);
    let last = rows_text(&term).pop().unwrap();
    assert!(last.contains("↓ 7 new · G"), "{last:?}");
    let x = last.find('↓').map(|b| last[..b].chars().count()).unwrap() as u16;
    assert!(matches!(
        clicks.click_at(x, 7),
        Some(Message::ScrollToBottom)
    ));
    // Following: no chip.
    p.scroll = 0;
    draw(&mut term, &p);
    assert!(!rows_text(&term).iter().any(|r| r.contains("new · G")));
}

#[test]
fn wrap_off_truncates_and_pans_horizontally() {
    let e = entry(&format!("{}{}", "a".repeat(30), "TAIL"));
    let entries = [e.clone()];
    let mut term = Terminal::new(backend::TestBackend::new(30, 3)).unwrap();
    let mut opts = ViewOpts {
        wrap: false,
        show_service: false,
        timestamps: false,
        ..ViewOpts::default()
    };
    assert_eq!(entry_rows(&e, 20, &opts), 1);
    draw(&mut term, &Fixture::new(&entries).pane(30, opts));
    let row = rows_text(&term)[0].clone();
    assert!(row.contains('→') && !row.contains("TAIL"), "{row:?}");
    // Pan right: tail appears, `← +N` chip appears, no more `→`.
    opts.hscroll = 12;
    let vp = Cell::new(LogViewport::default());
    let mut fx = Fixture::new(&entries);
    let p = fx.pane(30, opts);
    term.draw(|f| render(f, f.area(), &p, &vp, &ClickMap::default()))
        .unwrap();
    let rows = rows_text(&term);
    assert!(
        rows[0].contains("TAIL") && !rows[0].contains('→'),
        "{rows:?}"
    );
    assert!(
        !rows[0].contains('←'),
        "the offset lives in the status bar, not over text"
    );
    assert_eq!(vp.get().max_hscroll, 34 - vp.get().text_width as usize);
}

#[test]
fn json_lines_compact_with_level() {
    let e = entry(r#"{"level":50,"time":1,"msg":"db down","host":"x","code":503}"#);
    let (text, level) = display_content(&e, true);
    assert_eq!(level, LogLevel::Error);
    assert!(text.starts_with("ERROR db down"), "{text}");
    assert!(text.contains("host=x") && text.contains("code=503"));
    assert!(!text.contains("time"), "noise keys dropped: {text}");
    // pino numeric levels and string levels.
    let lvl = |c: &str| display_content(&entry(c), true).1;
    assert_eq!(lvl(r#"{"level":30,"msg":"a"}"#), LogLevel::Info);
    assert_eq!(lvl(r#"{"level":40,"msg":"a"}"#), LogLevel::Warn);
    assert_eq!(lvl(r#"{"level":"warn","msg":"a"}"#), LogLevel::Warn);
    assert_eq!(lvl(r#"{"level":"fatal","msg":"a"}"#), LogLevel::Error);
    // Toggle off ⇒ raw; non-JSON untouched.
    assert_eq!(display_content(&e, false).0, e.content);
    let plain = entry("not { json }");
    assert_eq!(display_content(&plain, true).0, "not { json }");
}

#[test]
fn restart_separator_is_one_row_and_never_searched() {
    let mut e = entry("──── 12:00:00 restarted ────");
    e.restart = true;
    assert_eq!(entry_rows(&e, 5, &ViewOpts::default()), 1);
    assert!(entry_links(&e, false).is_empty());
    let mut term = Terminal::new(backend::TestBackend::new(60, 4)).unwrap();
    let entries = [entry("before"), e, entry("after")];
    draw(
        &mut term,
        &Fixture::new(&entries).pane(60, ViewOpts::default()),
    );
    let rows = rows_text(&term).join("\n");
    assert!(rows.contains("before") && rows.contains("restarted") && rows.contains("after"));
}

#[test]
fn links_cover_urls_paths_and_osc8_and_are_underlined() {
    let e = entry(
        "fail at src/a.rs:12:3 see https://x.test/p, \x1b]8;;http://osc.test\x1b\\here\x1b]8;;\x1b\\",
    );
    let links = entry_links(&e, false);
    let targets: Vec<_> = links.iter().map(|l| l.target.clone()).collect();
    use crate::runtime::loglinks::Target;
    assert_eq!(
        targets,
        vec![
            Target::Path {
                path: "src/a.rs".into(),
                line: 12,
                col: Some(3)
            },
            Target::Url("https://x.test/p".into()),
            Target::Url("http://osc.test".into()),
        ]
    );
    // Drawn underlined; the cursor link gets the stronger background.
    let spans = entry_spans(&e, &ViewOpts::default(), None, None);
    assert!(spans.iter().any(|s| {
        s.content.contains("src/a.rs:12:3") && s.style.add_modifier.contains(Modifier::UNDERLINED)
    }));
    let cur = Cursor {
        seq: e.seq,
        link: 1,
    };
    let spans = entry_spans(&e, &ViewOpts::default(), None, Some(&cur));
    let t = theme::current();
    assert!(
        spans
            .iter()
            .any(|s| s.content.contains("https://x.test/p") && s.style.bg == Some(t.bg_float))
    );
    assert!(
        !spans
            .iter()
            .any(|s| s.content.contains("src/a.rs") && s.style.bg == Some(t.bg_float))
    );
}

#[test]
fn plain_lines_without_links_skip_the_scan() {
    assert!(entry_links(&entry("nothing to see here, 12:30:45 ok"), false).is_empty());
}

/// Drawing a 10-row window costs the window, not the buffer: the same near the bottom of a buffer 8 times
/// larger (flat, 1x). A render that walked the lines to find the window would cost 8x. (It used to time 50
/// frames of 200k lines against 20 ms each, which a loaded runner can fail with nothing regressed.)
#[test]
fn render_cost_is_bounded_by_the_window_not_the_buffer() {
    let fixture = |lines: usize| {
        let mut buf = LogBuffer::new(1_000_000);
        for i in 0..lines {
            buf.push(
                "api".into(),
                format!("line {i} some text to wrap around"),
                LogStream::Stdout,
            );
        }
        let key = crate::runtime::logindex::ViewKey {
            service: None,
            hidden: vec![],
            min_level: None,
            filter: None,
            find: None,
            json: false,
            window: Default::default(),
        };
        let mut index = ViewIndex::new();
        index.sync(&buf, &key);
        let mut rows = RowCache::new();
        rows.sync(
            &buf,
            None,
            index.seqs(0..index.len()),
            text_width_for(60),
            &ViewOpts::default(),
        );
        (buf, index, rows)
    };
    fn pane<'a>((buf, index, rows): &'a (LogBuffer, ViewIndex, RowCache)) -> LogPane<'a> {
        LogPane {
            src: Source {
                buf,
                index,
                rows,
                service: None,
            },
            scroll: 100,
            opts: ViewOpts::default(),
            find: None,
            cursor: None,
            tab: "All",
            narrowed: false,
            new_lines: 0,
        }
    }
    let (small, big) = (fixture(10_000), fixture(80_000));
    let (small_pane, big_pane) = (pane(&small), pane(&big));
    let mut small_term = Terminal::new(backend::TestBackend::new(60, 10)).unwrap();
    let mut big_term = Terminal::new(backend::TestBackend::new(60, 10)).unwrap();
    let ratio = atlas_core::scaling::growth(
        15,
        || {
            for _ in 0..50 {
                draw(&mut small_term, &small_pane);
            }
        },
        || {
            for _ in 0..50 {
                draw(&mut big_term, &big_pane);
            }
        },
    );
    assert!(
        ratio < 4.0,
        "a window of a buffer 8x larger cost {ratio:.1}x (flat is 1x, walking the buffer 8x)"
    );
    assert!(rows_text(&big_term).iter().any(|r| r.contains("line ")));
}

#[test]
fn mark_renders_as_one_dim_row() {
    let mut e = entry("──── 12:00:00 mark ────");
    e.mark = true;
    assert_eq!(entry_rows(&e, 5, &ViewOpts::default()), 1);
}
