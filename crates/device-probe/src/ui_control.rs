use std::{collections::VecDeque, sync::Mutex};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Pause,
    Resume(u64),
}
#[derive(Default)]
pub struct Inbox {
    queue: VecDeque<Command>,
    finish: Option<&'static str>,
}
impl Inbox {
    pub fn receive(&mut self, line: &str) {
        if self.finish.is_some() {
            return;
        }
        if line == "finish" {
            self.close("user_finish");
            return;
        }
        let command = if line == "pause" {
            Some(Command::Pause)
        } else {
            line.strip_prefix("resume ")
                .and_then(|s| s.parse().ok())
                .map(Command::Resume)
        };
        if let Some(command) = command {
            if self.queue.len() >= 16 {
                self.close("control_queue_overflow");
            } else {
                self.queue.push_back(command);
            }
        } else {
            self.close("invalid_control_command");
        }
    }
    pub fn close(&mut self, reason: &'static str) {
        if self.finish.is_none() {
            self.finish = Some(reason);
        }
        self.queue.clear();
    }
    pub fn finish_reason(&self) -> Option<&'static str> {
        self.finish
    }
    pub fn stop_pending(&self) -> bool {
        self.finish.is_some() || self.queue.contains(&Command::Pause)
    }
    pub fn pop(&mut self) -> Option<Command> {
        self.queue.pop_front()
    }
}
pub static INBOX: Mutex<Inbox> = Mutex::new(Inbox {
    queue: VecDeque::new(),
    finish: None,
});
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finish_and_eof_override_queued_resume() {
        let mut inbox = Inbox::default();
        inbox.receive("resume 1");
        inbox.receive("finish");
        inbox.receive("resume 2");
        assert_eq!(inbox.pop(), None);
        assert_eq!(inbox.finish_reason(), Some("user_finish"));
        let mut inbox = Inbox::default();
        inbox.receive("pause");
        inbox.receive("resume 2");
        assert_eq!(inbox.pop(), Some(Command::Pause));
        assert_eq!(inbox.pop(), Some(Command::Resume(2)));
        inbox.close("control_pipe_closed");
        assert_eq!(inbox.finish_reason(), Some("control_pipe_closed"));
    }
    #[test]
    fn queue_is_bounded_and_bad_input_finishes() {
        let mut inbox = Inbox::default();
        for _ in 0..17 {
            inbox.receive("pause");
        }
        assert_eq!(inbox.finish_reason(), Some("control_queue_overflow"));
        let mut inbox = Inbox::default();
        inbox.receive("resume nonsense");
        assert_eq!(inbox.finish_reason(), Some("invalid_control_command"));
    }
}
