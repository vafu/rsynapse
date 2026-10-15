use proj_model::{GoalEventInfo, GoalIdentityInfo, GoalStatsInfo, WorkKind, WorkSessionInfo};

pub(super) fn clip(mut s: WorkSessionInfo, from: u64, to: u64) -> Option<WorkSessionInfo> {
    let start = s.started_at_ms.max(from);
    let end = s.ended_at_ms.min(to);
    if end < start || s.started_at_ms >= to || s.ended_at_ms < from {
        return None;
    }
    if end == start && s.started_at_ms != s.ended_at_ms {
        return None;
    }
    let wall = s.ended_at_ms.saturating_sub(s.started_at_ms);
    if wall > 0 {
        s.duration_ms = ((s.duration_ms as u128) * (end - start) as u128 / wall as u128) as u64;
    }
    s.started_at_ms = start;
    s.ended_at_ms = end;
    Some(s)
}

pub(super) fn stats(
    identity: GoalIdentityInfo,
    events: &[GoalEventInfo],
    sessions: &[WorkSessionInfo],
    from: u64,
    to: u64,
) -> GoalStatsInfo {
    let mut stats = GoalStatsInfo {
        instance_id: identity.instance_id,
        key: identity.key,
        title: identity.goal.title,
        project_id: identity.goal.project_id,
        project_name: identity.project_name,
        status: identity.goal.status,
        created_at_ms: identity.created_at_ms,
        observed_at_ms: identity.observed_at_ms,
        history_partial: identity.created_at_ms == 0,
        ..Default::default()
    };
    let mut progress: Option<u64> = None;
    for event in events {
        if !matches!(
            event.kind.as_str(),
            "created" | "observed" | "status-changed" | "deleted"
        ) {
            continue;
        }
        let new = if event.kind == "deleted" {
            "deleted"
        } else {
            event.goal.status.as_str()
        };
        if new != "in-progress" {
            if let Some(start) = progress.take() {
                stats.in_progress_ms += event.at_ms.min(to).saturating_sub(start.max(from));
            }
        } else if progress.is_none() {
            progress = Some(event.at_ms);
            if stats.created_at_ms > 0 && event.kind != "observed" && stats.first_started_at_ms == 0
            {
                stats.first_started_at_ms = event.at_ms;
            }
        }
        if new == "completed" && event.kind != "observed" && event.old_status != "completed" {
            stats.completion_count += 1;
            stats.completed_at_ms = event.at_ms;
        }
    }
    if let Some(start) = progress {
        stats.in_progress_ms += to.saturating_sub(start.max(from));
    }
    if stats.completed_at_ms > 0 {
        if stats.created_at_ms > 0 {
            stats.lead_time_ms = stats.completed_at_ms.saturating_sub(stats.created_at_ms);
        }
        if stats.first_started_at_ms > 0 {
            stats.cycle_time_ms = stats
                .completed_at_ms
                .saturating_sub(stats.first_started_at_ms);
        }
    }
    for session in sessions {
        match session.kind {
            WorkKind::Active => stats.active_ms += session.duration_ms,
            WorkKind::Idle => stats.idle_ms += session.duration_ms,
            WorkKind::Agent => stats.agent_ms += session.duration_ms,
        }
        if session.end_reason.contains("unknown-duration") {
            stats.interrupted_sessions += 1;
            stats.history_partial = true;
        }
    }
    stats
}

#[cfg(test)]
mod test;
