//! Durable, event-driven workday ledger. Locus stores records; Graphite renders them.
use crate::{focus::FocusState, relations::LocusClient};
use chrono::{Local, TimeZone};
use locus::RelationEndpoint;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

const RELATION: &str = "org.rsynapse.workday.summary";
const TOTAL: f64 = 8.0 * 3600.0;
const ACTIVE: f64 = 4.0 * 3600.0;

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
struct Day {
    started: f64,
    last_active: f64,
    active_seconds: f64,
    unlocked_seconds: f64,
    checkpoint: f64,
    active: bool,
    unlocked: bool,
}

#[derive(Default)]
pub struct Workdays {
    days: BTreeMap<String, Day>,
    previous: Option<(f64, bool, bool)>,
}

fn date(timestamp: f64) -> String {
    Local
        .timestamp_opt(timestamp as i64, 0)
        .single()
        .expect("valid timestamp")
        .format("%Y-%m-%d")
        .to_string()
}

fn next_midnight(timestamp: f64) -> f64 {
    let day = Local
        .timestamp_opt(timestamp as i64, 0)
        .single()
        .unwrap()
        .date_naive()
        .succ_opt()
        .unwrap();
    Local
        .from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .unwrap()
        .timestamp() as f64
}

impl Workdays {
    pub async fn load(locus: &LocusClient) -> anyhow::Result<Self> {
        let mut ledger = Self::default();
        for record in locus.list(RELATION).await? {
            if let RelationEndpoint::StableKey { id, .. } = record.subject {
                if let Some(json) = record.metadata.get("summary") {
                    if let Ok(day) = serde_json::from_str(json) {
                        ledger.days.insert(id, day);
                    }
                }
            }
        }
        // One discovery/backfill request for today's existing activity history,
        // not a polling source. Missing history starts with the next observation.
        let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
        if !ledger.days.contains_key(&date(now)) {
            if let Err(error) = ledger.seed_today(now).await {
                eprintln!("workday history unavailable: {error}");
            }
        }
        Ok(ledger)
    }

    async fn seed_today(&mut self, now: f64) -> anyhow::Result<()> {
        #[derive(Deserialize)]
        struct Series {
            datapoints: Vec<(Option<f64>, f64)>,
        }
        let today = Local
            .timestamp_opt(now as i64, 0)
            .single()
            .unwrap()
            .date_naive();
        let midnight = Local
            .from_local_datetime(&today.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .unwrap()
            .timestamp();
        let url =
            std::env::var("GRAPHITE_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_owned());
        let prefix = std::env::var("METRIC_PREFIX").unwrap_or_else(|_| "rsynapse".to_owned());
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()?;
        let rows: Vec<Series> = client
            .get(format!("{url}/render"))
            .query(&[
                (
                    "target",
                    format!("keepLastValue(consolidateBy({prefix}.activity.state, 'last'))"),
                ),
                ("from", midnight.to_string()),
                ("until", (now as i64).to_string()),
                ("format", "json".to_owned()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if let Some(series) = rows.first() {
            for (index, (value, timestamp)) in series.datapoints.iter().enumerate() {
                let Some(value) = value else {
                    continue;
                };
                let end = series
                    .datapoints
                    .get(index + 1)
                    .map(|p| p.1)
                    .unwrap_or(now)
                    .min(now);
                let active = *value == 1.0;
                let unlocked = *value != 2.0;
                self.observe_flags(*timestamp, active, unlocked);
                self.observe_flags(end, active, unlocked);
            }
        }
        self.previous = None; // Do not count unobserved restart gaps.
        Ok(())
    }

    fn observe_flags(&mut self, now: f64, active: bool, unlocked: bool) {
        if let Some((mut cursor, was_active, was_unlocked)) = self.previous {
            while cursor < now {
                let end = next_midnight(cursor).min(now);
                let key = date(cursor);
                if was_active || self.days.contains_key(&key) {
                    let day = self.days.entry(key).or_default();
                    if day.started == 0.0 && was_active {
                        day.started = cursor;
                    }
                    if day.started > 0.0 {
                        if was_active {
                            day.active_seconds += end - cursor;
                            day.last_active = end;
                        }
                        if was_unlocked {
                            day.unlocked_seconds += end - cursor;
                        }
                        day.checkpoint = end;
                    }
                }
                cursor = end;
            }
        }
        let key = date(now);
        if active || self.days.contains_key(&key) {
            let day = self.days.entry(key).or_default();
            if day.started == 0.0 && active {
                day.started = now;
            }
            if active {
                day.last_active = now;
            }
            day.checkpoint = now;
            day.active = active;
            day.unlocked = unlocked;
        }
        self.previous = Some((now, active, unlocked));
    }

    pub async fn observe(
        &mut self,
        state: &FocusState,
        locus: &LocusClient,
    ) -> anyhow::Result<HashMap<String, f64>> {
        let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
        let initial = self.previous.is_none();
        let before = self.days.clone();
        self.observe_flags(now, !state.idle && !state.locked, !state.locked);
        let mut metrics = HashMap::new();
        for (key, day) in &self.days {
            if initial || before.get(key) != Some(day) {
                locus
                    .set_one(
                        RelationEndpoint::stable_key("org.rsynapse.calendar-day", key),
                        RELATION,
                        RelationEndpoint::stable_key("org.rsynapse.workday", key),
                        HashMap::from([("summary".to_owned(), serde_json::to_string(day)?)]),
                    )
                    .await?;
                Self::metrics(&mut metrics, key, day);
            }
            if key == &date(now) {
                Self::metrics(&mut metrics, "today", day);
            }
        }
        Ok(metrics)
    }

    fn metrics(metrics: &mut HashMap<String, f64>, key: &str, day: &Day) {
        for (field, value) in [
            ("started", day.started),
            ("planned_finish", day.started + TOTAL),
            ("last_active", day.last_active),
            ("active_seconds", day.active_seconds),
            ("unlocked_seconds", day.unlocked_seconds),
            ("checkpoint", day.checkpoint),
            ("active", if day.active { 1.0 } else { 0.0 }),
            ("unlocked", if day.unlocked { 1.0 } else { 0.0 }),
            ("span_delta", day.last_active - day.started - TOTAL),
            ("unlocked_delta", day.unlocked_seconds - TOTAL),
            ("active_delta", day.active_seconds - ACTIVE),
        ] {
            metrics.insert(format!("workday.{key}.{field}.state"), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excludes_prestart_idle_and_distinguishes_all_three_targets() {
        let start = Local
            .with_ymd_and_hms(2026, 10, 2, 9, 0, 0)
            .unwrap()
            .timestamp() as f64;
        let mut ledger = Workdays::default();
        ledger.observe_flags(start - 3600.0, false, true);
        ledger.observe_flags(start, true, true);
        ledger.observe_flags(start + 3600.0, false, true);
        ledger.observe_flags(start + 7200.0, false, false);
        ledger.observe_flags(start + 10800.0, true, true);
        ledger.observe_flags(start + 14400.0, false, false);
        let day = &ledger.days[&date(start)];
        assert_eq!(day.started, start);
        assert_eq!(day.active_seconds, 7200.0);
        assert_eq!(day.unlocked_seconds, 10800.0);
        assert_eq!(day.last_active - start, 14400.0);
    }
    #[test]
    fn midnight_splits_using_local_calendar_and_idle_does_not_start_a_day() {
        let start = Local
            .with_ymd_and_hms(2026, 10, 2, 23, 0, 0)
            .unwrap()
            .timestamp() as f64;
        let midnight = next_midnight(start);
        let mut ledger = Workdays::default();
        ledger.observe_flags(start, true, true);
        ledger.observe_flags(midnight + 3600.0, false, false);
        assert_eq!(ledger.days[&date(start)].active_seconds, midnight - start);
        assert_eq!(ledger.days[&date(midnight)].started, midnight);
        assert_eq!(ledger.days[&date(midnight)].active_seconds, 3600.0);
        let mut idle = Workdays::default();
        idle.observe_flags(start, false, true);
        idle.observe_flags(midnight + 3600.0, false, true);
        assert!(idle.days.is_empty());
    }

    #[test]
    fn restored_records_preserve_start_without_counting_disconnected_time() {
        let start = Local
            .with_ymd_and_hms(2026, 10, 2, 9, 0, 0)
            .unwrap()
            .timestamp() as f64;
        let mut original = Workdays::default();
        original.observe_flags(start, true, true);
        original.observe_flags(start + 3600.0, false, false);
        let serialized = serde_json::to_string(&original.days).unwrap();
        let mut restored = Workdays {
            days: serde_json::from_str(&serialized).unwrap(),
            previous: None,
        };
        restored.observe_flags(start + 7200.0, true, true);
        assert_eq!(restored.days[&date(start)].started, start);
        assert_eq!(restored.days[&date(start)].active_seconds, 3600.0);
        restored.observe_flags(start + 10800.0, false, false);
        assert_eq!(restored.days[&date(start)].active_seconds, 7200.0);
        assert_eq!(restored.days[&date(start)].unlocked_seconds, 7200.0);
    }
}
