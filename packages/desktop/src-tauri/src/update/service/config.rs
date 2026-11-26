//! User-facing channel and token changes applied to the updater state.

use super::*;

impl UpdateService {
    pub fn set_source(&self, source: UpdateSource) -> UpdateSnapshot {
        let changed = {
            let mut snapshot = self.snapshot.lock().unwrap();
            if snapshot.source == source || snapshot.state.in_flight() {
                false
            } else {
                snapshot.source = source;
                snapshot.state = UpdateState::Idle;
                snapshot.latest = None;
                snapshot.error = None;
                snapshot.done = None;
                snapshot.total = None;
                snapshot.notes = None;
                true
            }
        };
        if changed {
            self.clear_agent_status();
            self.discard_staged();
            self.notify();
        }
        self.snapshot()
    }

    pub fn set_channel(&self, preference: UpdateChannelPreference) -> UpdateSnapshot {
        let channel = version::effective_channel(preference, &self.build);
        let changed = {
            let mut snapshot = self.snapshot.lock().unwrap();
            if snapshot.channel == channel || snapshot.state.in_flight() {
                false
            } else {
                snapshot.channel = channel;
                snapshot.state = UpdateState::Idle;
                snapshot.latest = None;
                snapshot.error = None;
                snapshot.done = None;
                snapshot.total = None;
                snapshot.notes = None;
                true
            }
        };
        if changed {
            self.clear_agent_status();
            let remove = self.staged_version().is_some_and(|version| {
                channel == UpdateChannel::Stable && version::is_prerelease(&version)
            });
            if remove {
                self.discard_staged();
            }
            self.notify();
        }
        self.snapshot()
    }

    pub fn clear_token_requirement(&self) -> UpdateSnapshot {
        if !self.is_updatable() {
            return self.snapshot();
        }
        let mut snapshot = self.snapshot.lock().unwrap();
        if snapshot.state.in_flight() {
            drop(snapshot);
            return self.snapshot();
        }
        if snapshot.state == UpdateState::AuthRequired {
            snapshot.state = UpdateState::Idle;
            snapshot.error = None;
            snapshot.done = None;
            snapshot.total = None;
        }
        drop(snapshot);
        self.notify();
        self.snapshot()
    }
}
