use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::runtime::service::ServiceState;
use crate::theme;
use crate::tui::app::App;
use crate::tui::widgets;

pub fn render(frame: &mut Frame, app: &App) {
    let t = theme::current();
    let area = frame.area();

    // Min size guard
    if area.width < 60 || area.height < 12 {
        let msg = Paragraph::new("Terminal too small\n\nMinimum: 60x12")
            .alignment(Alignment::Center)
            .style(Style::default().fg(t.warning).bg(t.bg));
        frame.render_widget(msg, area);
        return;
    }

    // Clear
    frame.render_widget(Block::default().style(Style::default().bg(t.bg)), area);

    // Layout: header(2) | content | footer(1)
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // tabs
            Constraint::Min(5),   // content
            Constraint::Length(1), // footer
        ])
        .split(area);

    // Header: tabs
    let states: Vec<(String, ServiceState)> = app
        .manager
        .services()
        .iter()
        .map(|s| (s.name.clone(), s.state.clone()))
        .collect();
    widgets::tabs::render(frame, main_layout[0], &app.tabs, app.active_tab, &states);

    // Adaptive sidebar: hide if terminal < 100 cols
    let show_sidebar = area.width >= 100;

    if show_sidebar {
        let content_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(30),    // logs
                Constraint::Length(32), // sidebar
            ])
            .split(main_layout[1]);

        // Logs pane
        render_logs(frame, content_layout[0], app);

        // Sidebar
        let sidebar_block = Block::default()
            .style(Style::default().bg(t.bg_panel))
            .padding(Padding::new(2, 2, 1, 1));
        let sidebar_inner = sidebar_block.inner(content_layout[1]);
        frame.render_widget(sidebar_block, content_layout[1]);

        let services: Vec<&crate::runtime::service::ServiceStatus> =
            app.manager.services();
        widgets::service_list::render(frame, sidebar_inner, &services);
    } else {
        // No sidebar — logs take full width
        render_logs(frame, main_layout[1], app);
    }

    // Footer
    let footer = Line::from(vec![
        Span::styled(" ? ", Style::default().fg(t.text_dim)),
        Span::styled("Help", Style::default().fg(t.text_dim)),
        Span::raw("  "),
        Span::styled("q ", Style::default().fg(t.text_dim)),
        Span::styled("Quit", Style::default().fg(t.text_dim)),
        Span::raw("  "),
        Span::styled("r ", Style::default().fg(t.text_dim)),
        Span::styled("Restart", Style::default().fg(t.text_dim)),
        Span::raw("  "),
        Span::styled(": ", Style::default().fg(t.text_dim)),
        Span::styled("Commands", Style::default().fg(t.text_dim)),
        Span::raw("  "),
        Span::styled("tab ", Style::default().fg(t.text_dim)),
        Span::styled("Switch", Style::default().fg(t.text_dim)),
    ]);
    frame.render_widget(Paragraph::new(footer), main_layout[2]);
}

fn render_logs(frame: &mut Frame, area: Rect, app: &App) {
    let filtered = app.filtered_logs();
    widgets::log_viewer::render(frame, area, &filtered, app.log_scroll);
}
