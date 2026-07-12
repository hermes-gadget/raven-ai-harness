//! Event handling and keyboard binding for Raven TUI.

use crossterm::event::{self, Event as CEvent, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Submit,
    NextPanel,
    PrevPanel,
    FocusPanel(super::app::Panel),
    ScrollUp,
    ScrollDown,
    ToggleHelp,
    ToggleSearch,
    CancelSearch,
    Quit,
    Tick,
    InsertChar(char),
    InsertNewline,
    DeletePrev,
    DeleteNext,
    MoveCursorLeft,
    MoveCursorRight,
    MoveCursorHome,
    MoveCursorEnd,
    RefreshOrch,
}

#[derive(Debug)]
pub enum Event {
    Terminal(CEvent),
    Tick,
}

pub struct EventHandler {
    pub(crate) terminal_rx: mpsc::UnboundedReceiver<CEvent>,
    pub(crate) tick_rate: Duration,
}

impl EventHandler {
    pub fn new(tick_rate_ms: u64) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let tick_rate = Duration::from_millis(tick_rate_ms);
        tokio::spawn(async move {
            loop {
                if event::poll(Duration::from_millis(50)).unwrap_or(false)
                    && let Ok(evt) = event::read()
                    && tx.send(evt).is_err()
                {
                    break;
                }
            }
        });
        Self {
            terminal_rx: rx,
            tick_rate,
        }
    }

    pub async fn next(&mut self) -> Event {
        tokio::select! {
            Some(evt) = self.terminal_rx.recv() => Event::Terminal(evt),
            _ = tokio::time::sleep(self.tick_rate) => Event::Tick,
        }
    }

    pub fn handle_event(&self, event: &CEvent) -> Option<Action> {
        match event {
            CEvent::Key(key) => self.handle_key(key),
            CEvent::Resize(_, _) => Some(Action::RefreshOrch),
            _ => None,
        }
    }

    fn handle_key(&self, key: &KeyEvent) -> Option<Action> {
        use super::app::Panel;

        if key.modifiers == KeyModifiers::CONTROL {
            return match key.code {
                KeyCode::Char('d') => Some(Action::Quit),
                KeyCode::Char('f') => Some(Action::ToggleSearch),
                _ => None,
            };
        }

        // Alt+1..8: jump to tab
        if key.modifiers == KeyModifiers::ALT {
            return match key.code {
                KeyCode::Char('1') => Some(Action::FocusPanel(Panel::Chat)),
                KeyCode::Char('2') => Some(Action::FocusPanel(Panel::Agents)),
                KeyCode::Char('3') => Some(Action::FocusPanel(Panel::TaskGraph)),
                KeyCode::Char('4') => Some(Action::FocusPanel(Panel::Locks)),
                KeyCode::Char('5') => Some(Action::FocusPanel(Panel::Tools)),
                KeyCode::Char('6') => Some(Action::FocusPanel(Panel::Logs)),
                KeyCode::Char('7') => Some(Action::FocusPanel(Panel::History)),
                KeyCode::Char('8') => Some(Action::FocusPanel(Panel::Conflicts)),
                _ => None,
            };
        }

        if matches!(key.code, KeyCode::Enter)
            && key
                .modifiers
                .intersects(KeyModifiers::SHIFT | KeyModifiers::ALT)
        {
            return Some(Action::InsertNewline);
        }

        match key.code {
            KeyCode::Enter => Some(Action::Submit),
            KeyCode::Char('?') => Some(Action::ToggleHelp),
            KeyCode::Char(c) => Some(Action::InsertChar(c)),
            KeyCode::Backspace => Some(Action::DeletePrev),
            KeyCode::Delete => Some(Action::DeleteNext),
            KeyCode::Left => Some(Action::MoveCursorLeft),
            KeyCode::Right => Some(Action::MoveCursorRight),
            KeyCode::Home => Some(Action::MoveCursorHome),
            KeyCode::End => Some(Action::MoveCursorEnd),
            KeyCode::Tab => Some(Action::NextPanel),
            KeyCode::BackTab => Some(Action::PrevPanel),
            KeyCode::Up | KeyCode::PageUp => Some(Action::ScrollUp),
            KeyCode::Down | KeyCode::PageDown => Some(Action::ScrollDown),
            KeyCode::Esc => Some(Action::CancelSearch),
            _ => None,
        }
    }
}
