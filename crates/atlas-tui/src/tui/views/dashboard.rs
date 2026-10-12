use std::collections::HashMap;

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::runtime::service::ServiceState;
use crate::theme;
use crate::tui::app::{App, Message};
use crate::tui::widgets;
use crate::tui::widgets::service_list::SvcExtra;

/// Smallest terminal the dashboard lays out in.
pub const MIN_SIZE: (u16, u16) = (60, 12);
/// Rows kept for EVENTS (header + a few lines) before the sidebar block is squeezed.
const EVENTS_MIN: u16 = 4;

/// Sidebar width: base 34, grows with the longest service name (cap 44).
pub fn sidebar_width(app: &App) -> u16 {
    let max_name = app
        .manager
        .services()
        .iter()
        .map(|s| s.name.len())
        .max()
        .unwrap_or(8) as u16;
    (max_name + 20).clamp(34, 44) // icon+port+hint overhead = ~20
}

/// Whether the sidebar is drawn at `width`.
pub fn sidebar_visible(app: &App, width: u16) -> bool {
    !app.sidebar_hidden(width, sidebar_width(app))
}

/// Per-service sidebar extras (note, exit code, restart count, done flag).
fn extras(app: &App) -> HashMap<String, SvcExtra> {
    app.manager
        .services()
        .iter()
        .map(|s| {
            let done = s.state == ServiceState::Stopped && app.manager.completed_ok(&s.name);
            (
                s.name.clone(),
                SvcExtra {
                    note: app.manager.service_note(&s.name),
                    exit_code: app.manager.exit_code(&s.name),
                    restarts: app.manager.restart_count(&s.name),
                    done,
                    off: app.manager.is_deselected(&s.name),
                },
            )
        })
        .collect()
}

fn brand_line(app: &App) -> Line<'static> {
    let t = theme::current();
    let mut spans = vec![
        Span::styled("  code", Style::default().fg(t.primary).bold()),
        Span::styled("atlas", Style::default().fg(t.secondary).bold()),
    ];
    if !app.config.name.is_empty() {
        spans.push(Span::styled(
            format!(" · {}", app.config.name),
            Style::default().fg(t.text).bold(),
        ));
    } else {
        spans.push(Span::styled(" dev", Style::default().fg(t.text_dim)));
    }
    if app.config.port_offset > 0 {
        spans.push(Span::styled(
            format!(" ports +{}", app.config.port_offset),
            Style::default().fg(t.text_dim),
        ));
    }
    if app.remote.is_some() {
        spans.push(Span::styled(
            "  ⛓ attached",
            Style::default().fg(t.primary).bold(),
        ));
    }
    if app.dev_mode {
        spans.push(Span::styled(
            "  ⚡ linked",
            Style::default().fg(t.warning).bold(),
        ));
    }
    // Versions are in the update modal and the exit banner; the sidebar is too narrow for them.
    if app.update_on_exit {
        spans.push(Span::styled(
            " ⬆ on exit",
            Style::default().fg(t.success).bold(),
        ));
    } else if crate::update_notice::available().is_some() {
        spans.push(Span::styled(" ⬆", Style::default().fg(t.success).bold()));
    }
    Line::from(spans)
}

pub fn render(frame: &mut Frame, app: &App) {
    let t = theme::current();
    let area = frame.area();

    if area.width < MIN_SIZE.0 || area.height < MIN_SIZE.1 {
        let msg = Paragraph::new(format!(
            "Terminal too small\n\nCurrent: {}x{}\nMinimum: {}x{}",
            area.width, area.height, MIN_SIZE.0, MIN_SIZE.1
        ))
        .alignment(Alignment::Center)
        .style(Style::default().fg(t.warning).bg(t.bg));
        frame.render_widget(msg, area);
        return;
    }

    // Full background
    frame.render_widget(Block::default().style(Style::default().bg(t.bg)), area);

    // Outer vertical: tabs(2) | body
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    let states: Vec<(String, ServiceState)> = app
        .manager
        .services()
        .iter()
        .map(|s| (s.name.clone(), s.state.clone()))
        .collect();
    let badges = app.tab_badges();
    widgets::tabs::render(
        frame,
        outer[0],
        &widgets::tabs::TabsView {
            tabs: &app.tabs,
            active: app.active_tab,
            states: &states,
            badges: &badges,
            tick: app.tick_count as usize,
        },
        &app.clicks,
    );

    let sidebar_w = sidebar_width(app);
    let show_sidebar = sidebar_visible(app, area.width);

    if show_sidebar {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(1),
                Constraint::Length(sidebar_w),
            ])
            .split(outer[1]);

        // │ separator — stop one row early so it doesn't bleed into the footer
        let sep_lines: Vec<Line> = (0..body[1].height.saturating_sub(1))
            .map(|_| Line::from(Span::styled("│", Style::default().fg(t.border))))
            .collect();
        frame.render_widget(Paragraph::new(sep_lines), body[1]);

        // ── Left panel: logs + footer ───────────────────────────────
        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(body[0]);
        render_logs(frame, left[0], app);
        super::footer::render(frame, left[1], app, false);

        // ── Right panel ─────────────────────────────────────────────
        let sidebar_block = Block::default().style(Style::default().bg(t.bg_panel));
        let sidebar_inner = sidebar_block.inner(body[2]);
        frame.render_widget(sidebar_block, body[2]);

        let inset = |r: Rect| Rect {
            x: r.x + 2,
            y: r.y,
            width: r.width.saturating_sub(4),
            height: r.height,
        };

        let services: Vec<&crate::runtime::service::ServiceStatus> = app.manager.services();
        let critical: Vec<(String, String, Option<String>)> = app
            .manager
            .critical_services_failed()
            .into_iter()
            .map(|(name, snippet)| {
                let help = app
                    .config
                    .services
                    .get(&name)
                    .and_then(|d| d.critical_help.clone());
                (name, snippet, help)
            })
            .collect();
        let extras = extras(app);
        let tunnel = app.manager.tunnel_status();
        let data = widgets::service_list::Data {
            services: &services,
            extras: &extras,
            critical: &critical,
            tunnel_kill_on_exit: app.tunnel_kill_on_exit,
            has_tunnel: app.manager.has_tunnel(),
            tunnel: tunnel.as_ref(),
            mobile_phases: &app.sessions.phases,
            operation: app.operation.as_ref(),
        };
        // Services/ops/issues take their content height; EVENTS only gets the rest,
        // so critical_help can never be squeezed out.
        let inner_w = sidebar_inner.width.saturating_sub(4);
        let content_h = widgets::service_list::content_height(&data, inner_w);
        let avail = sidebar_inner.height.saturating_sub(2); // pad + brand
        let list_h = content_h.min(avail.saturating_sub(EVENTS_MIN.min(avail)).max(1));
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),      // top padding
                Constraint::Length(list_h), // services, ops, issues
                Constraint::Min(0),         // events: the remainder
                Constraint::Length(1),      // brand
            ])
            .split(sidebar_inner);

        widgets::service_list::render(
            frame,
            inset(right[1]),
            &data,
            app.tick_count as usize,
            &app.clicks,
        );
        if right[2].height > 1 {
            widgets::events::render(frame, inset(right[2]), &app.event_log);
        }
        frame.render_widget(Paragraph::new(brand_line(app)), right[3]);
    } else {
        // Narrow / zoomed: logs fill the body; status strip + footer below.
        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(1), // status strip
                Constraint::Length(1), // footer
            ])
            .split(outer[1]);
        render_logs(frame, left[0], app);
        super::footer::render_strip(frame, left[1], app);
        // The strip already shows the running operation.
        super::footer::render(frame, left[2], app, true);
    }
}

fn render_logs(frame: &mut Frame, area: Rect, app: &App) {
    let t = theme::current();
    let level_active = app.logs.level_filter != crate::tui::app::LevelFilter::All;
    let range_active = !app.logs.time_range.is_all() || app.logs.time_input.is_some();
    let toggles_active = !app.logs.log_wrap || !app.logs.log_timestamps || app.logs.log_json_pretty;
    let show_bar = app.logs.log_searching
        || !app.logs.log_search_input.is_empty()
        || level_active
        || range_active
        || toggles_active
        || app.logs.search_regex;
    let log_area = if show_bar {
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        frame.render_widget(
            Paragraph::new(search_bar_line(app, parts[1].width))
                .style(Style::default().bg(t.bg_panel)),
            parts[1],
        );
        // Clicking the bar (re)starts a search, same as `/`; inert while already typing.
        if !app.logs.log_searching {
            app.clicks.click(parts[1], Message::StartLogSearch);
        }
        parts[0]
    } else {
        area
    };

    // Index first (new lines, query, level), then the height cache at the width
    // this very frame wraps at — both incremental, so a frame is O(visible).
    app.sync_logs();
    app.sync_rows_for(widgets::log_viewer::text_width_for(log_area.width));
    // Everything that needs `&App` index queries is computed before the
    // long-lived borrows below (the index/cache are RefCells).
    let current = app.current_match();
    let new_lines = app.new_lines_since_pause();
    let index = app.logs.index.borrow();
    let rows = app.logs.rows.borrow();
    let finder = app.finder().filter(|_| app.find_active());
    let find = finder
        .as_ref()
        .map(|finder| widgets::log_viewer::FindHighlight { finder, current });
    let tab = app.tabs.get(app.active_tab).map_or("", String::as_str);
    let pane = widgets::log_viewer::LogPane {
        src: widgets::log_viewer::Source {
            buf: &app.logs.log_buffer,
            index: &index,
            rows: &rows,
            service: app.view_service(),
        },
        scroll: app.logs.log_scroll,
        opts: app.view_opts(),
        find,
        cursor: app.logs.link_cursor.clone(),
        tab,
        narrowed: !app.logs.log_search_input.is_empty() || level_active || range_active,
        new_lines,
    };
    widgets::log_viewer::render(frame, log_area, &pane, &app.logs.log_viewport, &app.clicks);
}

/// `/ query█  find ·re  3/41  wrap off · no ts · json  level: warn+`
fn search_bar_line(app: &App, width: u16) -> Line<'static> {
    let t = theme::current();
    let dim = Style::default().fg(t.text_dim);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let has_query = !app.logs.log_search_input.is_empty();

    if app.logs.log_searching {
        spans.push(Span::styled(" / ", Style::default().fg(t.primary).bold()));
        spans.push(Span::styled(
            app.logs.log_search_input.clone(),
            Style::default().fg(t.text),
        ));
        spans.push(Span::styled("█", dim));
    } else if has_query {
        spans.push(Span::styled(" / ", dim));
        spans.push(Span::styled(
            app.logs.log_search_input.clone(),
            Style::default().fg(t.text_muted),
        ));
    } else {
        spans.push(Span::raw(" "));
    }

    if let Some(text) = &app.logs.time_input {
        // The custom-range prompt takes over the bar while it is open.
        let mut spans = vec![
            Span::styled(" time ", Style::default().fg(t.primary).bold()),
            Span::styled(text.clone(), Style::default().fg(t.text)),
            Span::styled("█", dim),
        ];
        match &app.logs.time_error {
            Some(why) => spans.push(Span::styled(
                format!("  {why}"),
                Style::default().fg(t.error).bold(),
            )),
            None => spans.push(Span::styled(
                "  14:05 or 14:05–14:20 (today) · Enter apply · Esc all time",
                dim,
            )),
        }
        return Line::from(spans);
    }

    if has_query || app.logs.log_searching {
        let mode = format!(
            "  {}{}",
            app.logs.search_mode.label(),
            if app.logs.search_regex { " ·re" } else { "" }
        );
        spans.push(Span::styled(
            mode,
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ));
        if app.search_invalid() {
            spans.push(Span::styled(
                "  invalid regex",
                Style::default().fg(t.error).bold(),
            ));
        } else if has_query {
            let (cur, total) = app.find_counts();
            let counts = if app.find_active() && total > 0 {
                format!("  {cur}/{total}")
            } else {
                format!("  {total}")
            };
            let style = if total == 0 {
                Style::default().fg(t.warning).bold()
            } else {
                Style::default().fg(t.text)
            };
            spans.push(Span::styled(counts, style));
        }
        let hint = if app.logs.log_searching {
            "  Tab find/filter · ^R regex · Enter keep · Esc clear"
        } else if app.find_active() {
            "  n/N next/prev · ]e error · Esc clear"
        } else {
            "  Esc clear"
        };
        spans.push(Span::styled(hint, dim));
    }

    let mut toggles: Vec<String> = Vec::new();
    if !app.logs.log_wrap {
        toggles.push(if app.logs.log_hscroll > 0 {
            format!("wrap off ← +{}", app.logs.log_hscroll)
        } else {
            "wrap off".to_string()
        });
    }
    if !app.logs.log_timestamps {
        toggles.push("no ts".to_string());
    }
    if app.logs.log_json_pretty {
        toggles.push("json".to_string());
    }
    if !toggles.is_empty() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!(" {} ", toggles.join(" · ")),
            Style::default().fg(t.text_muted).bg(t.bg_float),
        ));
    }
    if app.logs.level_filter != crate::tui::app::LevelFilter::All {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!(" level: {} ", app.logs.level_filter.label()),
            Style::default().fg(t.warning).bold(),
        ));
        spans.push(Span::styled("v", dim));
    }
    if !app.logs.time_range.is_all() && app.logs.time_input.is_none() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!(" time: {} ", app.logs.time_range.label()),
            Style::default().fg(t.warning).bold(),
        ));
        spans.push(Span::styled("V", dim));
    }

    // Narrow bars: drop trailing spans until the line fits.
    let mut line = Line::from(spans);
    while line.width() > width as usize && line.spans.len() > 3 {
        line.spans.pop();
    }
    line
}
