use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::theme;
use crate::tui::app::{Message, ToastVariant};
use crate::tui::clicks::ClickMap;

use super::spinner;

const SLIDE_MS: u64 = 280;
const TOAST_H: u16 = 3;

pub fn render(
    frame: &mut Frame,
    msg: &str,
    variant: &ToastVariant,
    tick: usize,
    age_ms: u64,
    clicks: &ClickMap,
) {
    let t = theme::current();
    let (icon, color) = match variant {
        ToastVariant::Info => ("ℹ", t.primary),
        ToastVariant::Progress => (spinner::frame(tick), t.primary),
        ToastVariant::Success => ("✓", t.success),
        ToastVariant::Error => ("✗", t.error),
        ToastVariant::Warning => ("⚠", t.warning),
    };

    let area = frame.area();
    if area.height < 4 {
        return;
    }

    let content_w = (msg.len() as u16 + 6).min(44);
    let width = content_w + 1;
    if width >= area.width {
        return;
    }

    let final_x = area.width.saturating_sub(width + 1);
    let progress = (age_ms as f64 / SLIDE_MS as f64).min(1.0);
    let eased = 1.0 - (1.0 - progress) * (1.0 - progress);
    let x = (area.width as f64 - (area.width as f64 - final_x as f64) * eased) as u16;

    // Bottom-right, directly above the 1-row footer (last row); height >= 4 checked above.
    let y = area.height - 1 - TOAST_H;
    let toast_area = Rect::new(x, y, width, TOAST_H).intersection(area);
    if toast_area.is_empty() {
        return;
    }
    frame.render_widget(Clear, toast_area);
    clicks.click(toast_area, Message::DismissToast);

    let accent_area =
        Rect::new(toast_area.x, toast_area.y, 1, toast_area.height).intersection(area);
    if !accent_area.is_empty() {
        frame.render_widget(
            Block::default().style(Style::default().bg(color)),
            accent_area,
        );
    }

    let content_x = toast_area
        .x
        .saturating_add(1)
        .min(area.right().saturating_sub(1));
    let content_w = toast_area.width.saturating_sub(1);
    let content_area =
        Rect::new(content_x, toast_area.y, content_w, toast_area.height).intersection(area);
    if !content_area.is_empty() {
        frame.render_widget(
            Block::default().style(Style::default().bg(t.bg_panel)),
            content_area,
        );
    }

    if toast_area.height >= 2 && content_area.width > 2 {
        let text_area = Rect::new(
            content_area.x + 1,
            content_area.y + 1,
            content_area.width.saturating_sub(2),
            1,
        )
        .intersection(area);
        if !text_area.is_empty() {
            let max_msg = text_area.width.saturating_sub(3) as usize;
            let truncated: String = msg.chars().take(max_msg).collect();
            let line = Line::from(vec![
                Span::styled(icon, Style::default().fg(color)),
                Span::styled(format!(" {truncated}"), Style::default().fg(t.text)),
            ]);
            frame.render_widget(Paragraph::new(line), text_area);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn draw(width: u16, height: u16, msg: &str, age_ms: u64) {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        // Must not panic — any buffer overflow panics here
        terminal
            .draw(|frame| {
                render(
                    frame,
                    msg,
                    &ToastVariant::Info,
                    0,
                    age_ms,
                    &ClickMap::default(),
                )
            })
            .unwrap();
    }

    #[test]
    fn test_toast_normal_terminal() {
        draw(157, 43, "Restarted all services", 300);
    }

    #[test]
    fn test_toast_slide_in_at_age_zero() {
        // age_ms=0 → progress=0 → x should be off-screen, nothing rendered
        draw(157, 43, "Restarted all services", 0);
    }

    #[test]
    fn test_toast_exact_crash_dimensions() {
        // Reproduces original crash: width=157, x computed as 157 (off by one)
        draw(157, 43, "Services restarted", 1);
    }

    #[test]
    fn test_toast_narrow_terminal() {
        // width < toast width → must bail without panic
        draw(20, 10, "Restarted all services", 300);
    }

    #[test]
    fn test_toast_very_small_terminal() {
        draw(5, 3, "msg", 300);
    }

    #[test]
    fn test_toast_all_variants() {
        for variant in [
            ToastVariant::Info,
            ToastVariant::Progress,
            ToastVariant::Success,
            ToastVariant::Error,
            ToastVariant::Warning,
        ] {
            let backend = TestBackend::new(120, 30);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| {
                    render(
                        frame,
                        "test message",
                        &variant,
                        5,
                        300,
                        &ClickMap::default(),
                    )
                })
                .unwrap();
        }
    }

    #[test]
    fn test_toast_narrow_40x24_age_zero() {
        // advisory case 1: narrow terminal, age_ms=0 (slide not started)
        draw(40, 24, "Restarted all services", 0);
    }

    #[test]
    fn test_toast_wide_157x43_mid_slide() {
        // advisory case 2: exact crash dimensions, mid-animation
        draw(157, 43, "Restarted all services", 100);
    }
}
