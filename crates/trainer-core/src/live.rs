//! Acknowledged lifecycle and tokens for explicit, non-replayed Resume requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Running,
    Paused,
    Finished,
}
pub struct Lifecycle {
    pub state: State,
    pub epoch: u64,
}
impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            state: State::Paused,
            epoch: 0,
        }
    }
}
impl Lifecycle {
    pub fn paused(&mut self) {
        self.state = State::Paused;
        self.epoch += 1;
    }
    pub fn can_resume(&self, epoch: u64, fresh: bool) -> bool {
        self.state == State::Paused && self.epoch == epoch && fresh
    }
    pub fn running(&mut self) {
        self.state = State::Running;
    }
    pub fn finish(&mut self) {
        self.state = State::Finished;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resume_needs_current_pause_token_and_fresh_data() {
        let mut s = Lifecycle::default();
        s.running();
        assert!(!s.can_resume(0, true));
        s.paused();
        assert!(!s.can_resume(0, true));
        assert!(!s.can_resume(1, false));
        assert!(s.can_resume(1, true));
        s.running();
        assert!(!s.can_resume(1, true));
        s.paused();
        assert!(!s.can_resume(1, true));
        assert!(s.can_resume(2, true));
        s.finish();
        assert!(!s.can_resume(2, true));
    }
}

#[cfg(test)]
mod startup_regression {
    use super::*;
    use crate::{diagnostic::resistance_request, ftms::Request};
    #[test]
    fn suito_is_not_paused_before_start_or_paused_twice() {
        let mut acknowledged = RunAcknowledgements::default();
        assert_eq!(
            acknowledged.settle_requests(),
            vec![resistance_request(0).unwrap()]
        );
        acknowledged.accepted(Request::Start);
        assert_eq!(
            acknowledged.settle_requests(),
            vec![resistance_request(0).unwrap(), Request::Pause]
        );
        // A rejected Pause must leave the acknowledged run state unchanged.
        assert!(acknowledged.running);
        acknowledged.accepted(Request::Pause);
        assert_eq!(
            acknowledged.settle_requests(),
            vec![resistance_request(0).unwrap()]
        );
        assert!(acknowledged.ever_started && acknowledged.pause_acknowledged);
        acknowledged.accepted(Request::Start);
        assert!(!acknowledged.pause_acknowledged);
        assert_eq!(acknowledged.settle_requests().last(), Some(&Request::Pause));
    }
}

/// Tracks only accepted app commands; a device Started report is not a Start acknowledgement.
#[derive(Default)]
pub struct RunAcknowledgements {
    pub running: bool,
    pub ever_started: bool,
    pub pause_acknowledged: bool,
}
impl RunAcknowledgements {
    pub fn accepted(&mut self, request: crate::ftms::Request) {
        match request {
            crate::ftms::Request::Start => {
                self.running = true;
                self.ever_started = true;
                self.pause_acknowledged = false;
            }
            crate::ftms::Request::Pause => {
                self.running = false;
                self.pause_acknowledged = true;
            }
            _ => {}
        }
    }
    pub fn settle_requests(&self) -> Vec<crate::ftms::Request> {
        let mut requests = vec![crate::diagnostic::resistance_request(0).unwrap()];
        if self.running {
            requests.push(crate::ftms::Request::Pause);
        }
        requests
    }
}
