//! CronSource — reads cron tasks from Nomen memory (`cron/*` topics) and emits events on schedule.
//!
//! Schedule types:
//! - Cron expression: "0 */6 * * *"
//! - ISO datetime (one-shot): "2026-04-01T09:00:00Z"
//! - Interval: "30m", "6h"

use anyhow::Result;
use chrono::{DateTime, Utc};
use nocelium_channels::{Event, Payload, Source};
use nocelium_memory::MemoryClient;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// How often to reload tasks from Nomen (seconds).
const RELOAD_INTERVAL_SECS: u64 = 300;

#[derive(Debug, Clone, Deserialize)]
struct CronTaskConfig {
    schedule: String,
    payload: String,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone)]
enum Schedule {
    Cron(Box<cron::Schedule>),
    OneShot(DateTime<Utc>),
    Interval(std::time::Duration),
}

#[derive(Debug, Clone)]
struct CronTask {
    #[allow(dead_code)]
    id: String,
    schedule: Schedule,
    payload: String,
    next_run: DateTime<Utc>,
}

pub struct CronSource;

impl CronSource {
    /// Start the cron source. Runs forever, emitting events on the tx channel.
    pub async fn start(tx: mpsc::Sender<Event>, memory: Arc<MemoryClient>) {
        tracing::info!("CronSource starting");

        let mut tasks: HashMap<String, CronTask> = HashMap::new();
        let mut last_reload = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(RELOAD_INTERVAL_SECS + 1))
            .unwrap_or_else(std::time::Instant::now);

        loop {
            // Reload tasks periodically
            if last_reload.elapsed().as_secs() >= RELOAD_INTERVAL_SECS || tasks.is_empty() {
                match load_tasks(&memory).await {
                    Ok(new_tasks) => {
                        let count = new_tasks.len();
                        tasks = new_tasks;
                        tracing::info!(task_count = count, "CronSource loaded tasks");
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "CronSource failed to load tasks");
                    }
                }
                last_reload = std::time::Instant::now();
            }

            if tasks.is_empty() {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                continue;
            }

            // Find the next task to fire
            let now = Utc::now();
            let next = tasks.values().min_by_key(|t| t.next_run);
            let Some(next_task) = next else {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                continue;
            };

            if next_task.next_run > now {
                let wait = (next_task.next_run - now)
                    .to_std()
                    .unwrap_or(std::time::Duration::from_secs(1));
                // Cap wait to reload interval so we pick up changes
                let wait = wait.min(std::time::Duration::from_secs(RELOAD_INTERVAL_SECS));
                tokio::time::sleep(wait).await;
                continue;
            }

            // Fire all due tasks
            let due_ids: Vec<String> = tasks
                .iter()
                .filter(|(_, t)| t.next_run <= Utc::now())
                .map(|(id, _)| id.clone())
                .collect();

            for task_id in due_ids {
                let task = match tasks.get(&task_id) {
                    Some(t) => t.clone(),
                    None => continue,
                };

                tracing::info!(task_id = %task_id, "CronSource firing task");

                let event = Event::new(
                    Source::Cron(task_id.clone()),
                    Payload::Message(Box::new(nocelium_channels::event::Message {
                        id: format!("cron-{}-{}", task_id, Utc::now().timestamp()),
                        text: task.payload.clone(),
                        ..Default::default()
                    })),
                );

                if tx.send(event).await.is_err() {
                    tracing::error!("CronSource: event channel closed, stopping");
                    return;
                }

                // Advance next_run or remove one-shot tasks
                match &task.schedule {
                    Schedule::OneShot(_) => {
                        tasks.remove(&task_id);
                        // Delete from Nomen
                        let topic = format!("cron/{}", task_id);
                        if let Err(e) = memory.delete(&topic).await {
                            tracing::error!(error = %e, topic = %topic, "Failed to delete one-shot cron task");
                        } else {
                            tracing::info!(task_id = %task_id, "One-shot cron task deleted from Nomen");
                        }
                    }
                    Schedule::Cron(sched) => {
                        if let Some(task) = tasks.get_mut(&task_id) {
                            task.next_run = next_cron_run(sched);
                        }
                    }
                    Schedule::Interval(dur) => {
                        if let Some(task) = tasks.get_mut(&task_id) {
                            task.next_run = Utc::now() + chrono::Duration::from_std(*dur).unwrap_or(chrono::Duration::seconds(60));
                        }
                    }
                }
            }
        }
    }
}

async fn load_tasks(memory: &MemoryClient) -> Result<HashMap<String, CronTask>> {
    // Search for cron/* topics
    let memories = memory.search("cron/", 100, None, None).await
        .map_err(|e| anyhow::anyhow!("Memory search failed: {e}"))?;

    let mut tasks = HashMap::new();

    for mem in memories {
        if !mem.topic.starts_with("cron/") {
            continue;
        }

        let task_id = mem.topic.strip_prefix("cron/").unwrap_or(&mem.topic).to_string();

        let config: CronTaskConfig = match serde_json::from_str(&mem.detail) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(topic = %mem.topic, error = %e, "Invalid cron task config, skipping");
                continue;
            }
        };

        if !config.enabled {
            tracing::debug!(task_id = %task_id, "Cron task disabled, skipping");
            continue;
        }

        let schedule = match parse_schedule(&config.schedule) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(task_id = %task_id, schedule = %config.schedule, error = %e, "Invalid schedule, skipping");
                continue;
            }
        };

        let next_run = compute_next_run(&schedule);
        tasks.insert(
            task_id.clone(),
            CronTask {
                id: task_id,
                schedule,
                payload: config.payload,
                next_run,
            },
        );
    }

    Ok(tasks)
}

fn parse_schedule(s: &str) -> Result<Schedule> {
    // Try ISO datetime first
    if let Ok(dt) = s.parse::<DateTime<Utc>>() {
        return Ok(Schedule::OneShot(dt));
    }
    // Also try with chrono's flexible parsing
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(Schedule::OneShot(dt.with_timezone(&Utc)));
    }

    // Try interval: "30m", "6h", "1d"
    if let Some(dur) = parse_interval(s) {
        return Ok(Schedule::Interval(dur));
    }

    // Try cron expression
    use std::str::FromStr;
    let cron_sched = cron::Schedule::from_str(s)
        .map_err(|e| anyhow::anyhow!("Invalid cron expression '{}': {}", s, e))?;
    Ok(Schedule::Cron(Box::new(cron_sched)))
}

fn parse_interval(s: &str) -> Option<std::time::Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (num_str, suffix) = if let Some(n) = s.strip_suffix('m') {
        (n, "m")
    } else if let Some(n) = s.strip_suffix('h') {
        (n, "h")
    } else if let Some(n) = s.strip_suffix('d') {
        (n, "d")
    } else if let Some(n) = s.strip_suffix('s') {
        (n, "s")
    } else {
        return None;
    };

    let num: u64 = num_str.parse().ok()?;
    let secs = match suffix {
        "s" => num,
        "m" => num * 60,
        "h" => num * 3600,
        "d" => num * 86400,
        _ => return None,
    };
    Some(std::time::Duration::from_secs(secs))
}

fn compute_next_run(schedule: &Schedule) -> DateTime<Utc> {
    match schedule {
        Schedule::OneShot(dt) => *dt,
        Schedule::Cron(sched) => next_cron_run(sched),
        Schedule::Interval(dur) => {
            Utc::now() + chrono::Duration::from_std(*dur).unwrap_or(chrono::Duration::seconds(60))
        }
    }
}

fn next_cron_run(sched: &cron::Schedule) -> DateTime<Utc> {
    sched
        .upcoming(Utc)
        .next()
        .unwrap_or_else(|| Utc::now() + chrono::Duration::hours(1))
}
