use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, MouseEvent};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
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
            let mut interval = tokio::time::interval(tick_rate);
            loop {
                let event = tokio::select! {
                    _ = interval.tick() => Event::Tick,
                    maybe_event = Self::poll_crossterm() => {
                        match maybe_event {
                            Some(e) => e,
                            None => continue,
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

    async fn poll_crossterm() -> Option<Event> {
        tokio::task::spawn_blocking(|| {
            if event::poll(Duration::from_millis(50)).ok()? {
                match event::read().ok()? {
                    CrosstermEvent::Key(k) => Some(Event::Key(k)),
                    CrosstermEvent::Mouse(m) => Some(Event::Mouse(m)),
                    CrosstermEvent::Resize(w, h) => Some(Event::Resize(w, h)),
                    _ => None,
                }
            } else {
                None
            }
        })
        .await
        .ok()
        .flatten()
    }

    /// Create an EventHandler for testing (returns sender too)
    pub fn test_channel() -> (mpsc::UnboundedSender<Event>, Self) {
        let (tx, rx) = mpsc::unbounded_channel();
        (tx.clone(), Self { rx, _tx: tx })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEventKind, KeyEventState, KeyModifiers};

    #[tokio::test]
    async fn test_event_channel_send_receive() {
        let (tx, mut handler) = EventHandler::test_channel();

        let key_event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        tx.send(Event::Key(key_event)).unwrap();

        let received = handler.next().await.unwrap();
        assert!(matches!(received, Event::Key(k) if k.code == KeyCode::Char('q')));
    }

    #[test]
    fn test_event_debug_impl() {
        let tick = Event::Tick;
        let debug_str = format!("{:?}", tick);
        assert_eq!(debug_str, "Tick");

        let resize = Event::Resize(80, 24);
        let debug_str = format!("{:?}", resize);
        assert!(debug_str.contains("Resize"));
        assert!(debug_str.contains("80"));
        assert!(debug_str.contains("24"));

        let key_event = KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        let key = Event::Key(key_event);
        let debug_str = format!("{:?}", key);
        assert!(debug_str.contains("Key"));
    }

    #[tokio::test]
    async fn test_tick_event() {
        let (tx, mut handler) = EventHandler::test_channel();

        tx.send(Event::Tick).unwrap();
        tx.send(Event::Tick).unwrap();
        tx.send(Event::Tick).unwrap();

        let e1 = handler.next().await.unwrap();
        let e2 = handler.next().await.unwrap();
        let e3 = handler.next().await.unwrap();

        assert!(matches!(e1, Event::Tick));
        assert!(matches!(e2, Event::Tick));
        assert!(matches!(e3, Event::Tick));
    }
}
