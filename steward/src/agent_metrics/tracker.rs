use super::{Session, Snapshot, UsageEvent};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::Instant,
};

struct Response {
    ready: Instant,
    owner: Session,
    available_seconds: f64,
    available_since: Option<Instant>,
}
struct Member {
    session: Session,
    since: Instant,
    busy_since: Option<Instant>,
    unread: Option<Response>,
}

#[derive(Default)]
pub(crate) struct AgentTracker {
    members: HashMap<String, Member>,
    known: HashSet<String>,
    initialized: bool,
    focused_window: Option<u64>,
    available: bool,
    active_links: HashMap<u64, String>,
    pending: HashMap<String, f64>,
    gauges: HashMap<String, f64>,
    retired: VecDeque<Session>,
    usage_pending: VecDeque<UsageEvent>,
    usage_seen: HashMap<(String, String), (u64, u64)>,
}

impl AgentTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, snapshot: Snapshot, now: Instant) {
        self.active_links = snapshot.active_links;
        let live: HashSet<_> = snapshot.sessions.iter().map(|s| s.key.clone()).collect();
        let removed: Vec<_> = self
            .members
            .keys()
            .filter(|key| !live.contains(*key))
            .cloned()
            .collect();
        for key in removed {
            let member = self.members.remove(&key).unwrap();
            self.close_state(&member, now);
            self.retired.push_back(member.session.clone());
            if self.retired.len() > 1024 {
                self.retired.pop_front();
            }
            if let Some(reply) = member.unread {
                self.increment(&reply.owner, "responses.unread_cancelled.count", 1.0);
            }
        }
        for session in snapshot.sessions {
            let member = self.members.remove(&session.key);
            let mut member = match member {
                Some(mut member) => {
                    if member.session.state != session.state
                        || member.session.prefixes() != session.prefixes()
                        || member.session.model_prefixes() != session.model_prefixes()
                    {
                        self.close_state(&member, now);
                        member.since = now;
                    }
                    if busy(&member.session.state) && session.state == "idle" {
                        self.increment(&session, "responses.completed.count", 1.0);
                        if let Some(start) = member.busy_since.take() {
                            self.increment(
                                &session,
                                "work_cycles.seconds_sum",
                                now.duration_since(start).as_secs_f64(),
                            );
                            self.increment(&session, "work_cycles.count", 1.0);
                        }
                        if !session.subagent {
                            member.unread = Some(Response {
                                ready: now,
                                owner: session.clone(),
                                available_seconds: 0.0,
                                available_since: self.available.then_some(now),
                            });
                        }
                    } else if session.state != "idle" {
                        if let Some(reply) = member.unread.take() {
                            self.increment(&reply.owner, "responses.unread_cancelled.count", 1.0);
                        }
                    }
                    if !busy(&member.session.state) && busy(&session.state) {
                        member.busy_since = Some(now);
                    }
                    if !busy(&session.state) && session.state != "idle" {
                        member.busy_since = None;
                    }
                    member.session = session;
                    member
                }
                None => {
                    // First roster is a baseline. Never fabricate starts or
                    // reaction times for sessions already running/idle on attach.
                    if self.initialized && self.known.insert(session.key.clone()) {
                        self.increment(&session, "sessions.started.count", 1.0);
                    }
                    self.known.insert(session.key.clone());
                    Member {
                        session,
                        since: now,
                        busy_since: None,
                        unread: None,
                    }
                }
            };
            if let Some(reply) = &mut member.unread {
                reply.owner.window = member.session.window;
            }
            self.members.insert(member.session.key.clone(), member);
        }
        self.initialized = true;
        let queued = std::mem::take(&mut self.usage_pending);
        for event in queued {
            self.usage(event);
        }
        self.read_responses(now);
        self.refresh_gauges();
    }

    pub fn focus(&mut self, window: Option<u64>, available: bool, now: Instant) {
        self.focused_window = window;
        if self.available != available {
            for member in self.members.values_mut() {
                if let Some(reply) = &mut member.unread {
                    if let Some(start) = reply.available_since.take() {
                        reply.available_seconds += now.duration_since(start).as_secs_f64();
                    }
                    reply.available_since = available.then_some(now);
                }
            }
        }
        self.available = available;
        self.read_responses(now);
        self.refresh_gauges();
    }

    fn read_responses(&mut self, now: Instant) {
        if !self.available {
            return;
        }
        let read: Vec<_> = self
            .members
            .iter()
            .filter_map(|(key, member)| {
                let reply = member.unread.as_ref()?;
                let window = reply.owner.window?;
                if self.focused_window != Some(window) {
                    return None;
                }
                if self
                    .active_links
                    .get(&window)
                    .is_some_and(|path| path != key)
                {
                    return None;
                }
                Some(key.clone())
            })
            .collect();
        for key in read {
            let reply = self.members.get_mut(&key).unwrap().unread.take().unwrap();
            let available = reply.available_seconds
                + reply
                    .available_since
                    .map(|start| now.duration_since(start).as_secs_f64())
                    .unwrap_or(0.0);
            self.increment(
                &reply.owner,
                "response_latency.seconds_sum",
                now.duration_since(reply.ready).as_secs_f64(),
            );
            self.increment(
                &reply.owner,
                "response_latency.available_seconds_sum",
                available,
            );
            self.increment(&reply.owner, "response_latency.count", 1.0);
            self.increment(&reply.owner, "responses.read.count", 1.0);
        }
    }

    fn close_state(&mut self, member: &Member, now: Instant) {
        let duration = now.duration_since(member.since).as_secs_f64();
        if duration > 0.0 {
            self.increment(
                &member.session,
                &format!(
                    "state.{}.seconds",
                    crate::focus::sanitize(&member.session.state)
                ),
                duration,
            );
            for prefix in member.session.model_prefixes() {
                *self
                    .pending
                    .entry(format!(
                        "{prefix}.state.{}.seconds",
                        crate::focus::sanitize(&member.session.state)
                    ))
                    .or_default() += duration;
            }
        }
    }

    pub fn usage(&mut self, event: UsageEvent) {
        let session = self
            .members
            .get(&event.key)
            .map(|m| &m.session)
            .or_else(|| self.retired.iter().rev().find(|s| s.key == event.key))
            .cloned();
        let Some(mut session) = session else {
            if self.usage_pending.len() == 1024 {
                self.usage_pending.pop_front();
            }
            self.usage_pending.push_back(event);
            return;
        };
        let identity = (event.owner, event.key);
        let stamp = (event.epoch, event.revision);
        if self
            .usage_seen
            .get(&identity)
            .is_some_and(|previous| *previous >= stamp)
        {
            return;
        }
        if self.usage_seen.len() >= 4096 && !self.usage_seen.contains_key(&identity) {
            // Bound per-session replay bookkeeping; live and recent sessions are retained.
            self.usage_seen.retain(|(_, key), _| {
                self.members.contains_key(key) || self.retired.iter().any(|s| &s.key == key)
            });
        }
        self.usage_seen.insert(identity, stamp);
        session.model = event.model;
        session.effort = event.effort;
        for (counter, value) in event.delta {
            if !matches!(
                counter.as_str(),
                "input"
                    | "output"
                    | "cache_read_input"
                    | "cache_write_input"
                    | "reasoning_output"
                    | "total"
            ) {
                continue;
            }
            let suffix = format!("tokens.{counter}.count");
            self.increment(&session, &suffix, value as f64);
            for prefix in session.model_prefixes() {
                *self
                    .pending
                    .entry(format!("{prefix}.{suffix}"))
                    .or_default() += value as f64;
            }
        }
    }

    fn increment(&mut self, session: &Session, suffix: &str, value: f64) {
        for prefix in session.prefixes() {
            *self
                .pending
                .entry(format!("{prefix}.{suffix}"))
                .or_default() += value;
        }
    }

    fn refresh_gauges(&mut self) {
        let mut next: HashMap<String, f64> = HashMap::new();
        for member in self.members.values() {
            for prefix in member.session.prefixes() {
                *next
                    .entry(format!("{prefix}.sessions.live.state"))
                    .or_default() += 1.0;
                *next
                    .entry(format!("{prefix}.sessions.busy.state"))
                    .or_default() += if busy(&member.session.state) {
                    1.0
                } else {
                    0.0
                };
                *next
                    .entry(format!("{prefix}.responses.waiting.state"))
                    .or_default() += if member.unread.is_some() { 1.0 } else { 0.0 };
            }
        }
        for name in self.gauges.keys() {
            next.entry(name.clone()).or_default();
        }
        for (name, value) in &next {
            if self.gauges.get(name) != Some(value) {
                self.pending.insert(name.clone(), *value);
            }
        }
        self.gauges = next;
    }

    pub fn finish(&mut self, now: Instant) {
        let members = std::mem::take(&mut self.members);
        for member in members.values() {
            self.close_state(member, now);
        }
        self.refresh_gauges();
    }
    pub fn drain(&mut self) -> HashMap<String, f64> {
        std::mem::take(&mut self.pending)
    }
}

fn busy(state: &str) -> bool {
    matches!(state, "thinking" | "tool-use" | "compacting")
}

#[cfg(test)]
mod tests;
