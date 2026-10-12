use crossterm::event::{Event as CrosstermEvent, EventStream, KeyEvent, KeyEventKind, MouseEvent};
use futures_util::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    FocusGained,
    /// Bracketed paste payload.
    Paste(String),
    Tick,
}

pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<Event>,
    _tx: mpsc::UnboundedSender<Event>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let mut stream = EventStream::new();
            let mut interval = tokio::time::interval(tick_rate);
            // Don't burst-fire missed ticks (e.g. after a slow render)
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                let event = tokio::select! {
                    // Tick fires first when both are ready — keeps animations smooth
                    biased;
                    _ = interval.tick() => Event::Tick,
                    maybe = stream.next() => {
                        match maybe {
                            // Release/Repeat events (kitty protocol, Windows) would double-fire.
                            Some(Ok(CrosstermEvent::Key(k))) if k.kind == KeyEventKind::Press => {
                                Event::Key(k)
                            }
                            Some(Ok(CrosstermEvent::Key(_))) => continue,
                            Some(Ok(CrosstermEvent::Mouse(m))) => Event::Mouse(m),
                            Some(Ok(CrosstermEvent::Resize(w, h))) => Event::Resize(w, h),
                            Some(Ok(CrosstermEvent::FocusGained)) => Event::FocusGained,
                            Some(Ok(CrosstermEvent::Paste(s))) => Event::Paste(s),
                            // FocusLost, etc. — skip
                            Some(Ok(_)) => continue,
                            // Stream error (unlikely) — skip
                            Some(Err(_)) => continue,
                            // Terminal closed — shut down
                            None => break,
                        }
                    }
                };
                if tx_clone.send(event).is_err() {
                    break;
                }
            }
        });
        Self { rx, _tx: tx }
    }

    pub async fn next(&mut self) -> Option<Event> {
        self.rx.recv().await
    }

    /// Non-blocking poll. `Err(Empty)` = nothing pending, `Err(Disconnected)` = closed.
    pub fn try_next(&mut self) -> Result<Event, mpsc::error::TryRecvError> {
        self.rx.try_recv()
    }

    /// Create an EventHandler for testing (returns sender too)
    pub fn test_channel() -> (mpsc::UnboundedSender<Event>, Self) {
        let (tx, rx) = mpsc::unbounded_channel();
        let tx_clone = tx.clone();
        (tx_clone, Self { rx, _tx: tx })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEventKind, KeyEventState, KeyModifiers};

    #[tokio::test]
    async fn test_event_channel_send_receive() {
        let (tx, mut handler) = EventHandler::test_channel();
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        tx.send(Event::Key(key)).unwrap();
        let received = handler.next().await.unwrap();
        assert!(matches!(received, Event::Key(_)));
    }

    #[test]
    fn test_event_debug_impl() {
        let key = KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        let event = Event::Key(key);
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("Key"));

        let mouse = MouseEvent {
            kind: crossterm::event::MouseEventKind::Moved,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        let mouse_event = Event::Mouse(mouse);
        let debug_str = format!("{:?}", mouse_event);
        assert!(debug_str.contains("Mouse"));

        let resize = Event::Resize(80, 24);
        assert!(format!("{:?}", resize).contains("Resize"));

        let tick = Event::Tick;
        assert!(format!("{:?}", tick).contains("Tick"));
    }

    #[tokio::test]
    async fn test_tick_event() {
        let (tx, mut handler) = EventHandler::test_channel();
        tx.send(Event::Tick).unwrap();
        let event = handler.next().await.unwrap();
        assert!(matches!(event, Event::Tick));
    }
}
