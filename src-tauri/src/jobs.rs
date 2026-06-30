use crate::models::{JobLifecycle, JobLogEntry, JobStatus};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct AppState {
    pub jobs: Arc<Mutex<HashMap<String, JobRecord>>>,
}

pub struct JobRecord {
    pub status: JobStatus,
    pub cancel: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct JobReporter {
    jobs: Arc<Mutex<HashMap<String, JobRecord>>>,
    id: String,
    cancel: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub struct JobCanceled;

impl JobReporter {
    pub fn new(
        jobs: Arc<Mutex<HashMap<String, JobRecord>>>,
        id: String,
        cancel: Arc<AtomicBool>,
    ) -> Self {
        Self { jobs, id, cancel }
    }

    pub fn stage(&self, stage: &str, progress: f32, message: &str) -> Result<(), JobCanceled> {
        self.check_canceled()?;
        self.update(|status| {
            status.lifecycle = JobLifecycle::Running;
            status.stage = stage.to_string();
            status.progress = progress.clamp(0.0, 1.0);
            status.logs.push(log_entry(stage, message));
        });
        Ok(())
    }

    pub fn log(&self, stage: &str, message: &str) {
        self.update(|status| {
            status.logs.push(log_entry(stage, message));
        });
    }

    pub fn succeed(&self, output_path: String) {
        self.update(|status| {
            status.lifecycle = JobLifecycle::Succeeded;
            status.stage = "complete".to_string();
            status.progress = 1.0;
            status.output_path = Some(output_path);
            status.finished_at = Some(Utc::now().to_rfc3339());
            status
                .logs
                .push(log_entry("complete", "protection job completed"));
        });
    }

    pub fn fail(&self, error: String) {
        self.update(|status| {
            status.lifecycle = JobLifecycle::Failed;
            status.error = Some(error.clone());
            status.finished_at = Some(Utc::now().to_rfc3339());
            status.logs.push(log_entry("error", &error));
        });
    }

    pub fn canceled(&self) {
        self.update(|status| {
            status.lifecycle = JobLifecycle::Canceled;
            status.stage = "canceled".to_string();
            status.finished_at = Some(Utc::now().to_rfc3339());
            status
                .logs
                .push(log_entry("canceled", "job canceled by user"));
        });
    }

    pub fn check_canceled(&self) -> Result<(), JobCanceled> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(JobCanceled)
        } else {
            Ok(())
        }
    }

    fn update<F>(&self, mutator: F)
    where
        F: FnOnce(&mut JobStatus),
    {
        if let Ok(mut jobs) = self.jobs.lock() {
            if let Some(record) = jobs.get_mut(&self.id) {
                mutator(&mut record.status);
                if record.status.logs.len() > 500 {
                    let keep_from = record.status.logs.len() - 500;
                    record.status.logs.drain(0..keep_from);
                }
            }
        }
    }
}

pub fn insert_queued_job(state: &AppState, id: String, cancel: Arc<AtomicBool>) {
    let now = Utc::now().to_rfc3339();
    let status = JobStatus {
        id: id.clone(),
        lifecycle: JobLifecycle::Queued,
        stage: "queued".to_string(),
        progress: 0.0,
        logs: vec![log_entry("queued", "job queued")],
        output_path: None,
        error: None,
        started_at: Some(now),
        finished_at: None,
    };
    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.insert(id, JobRecord { status, cancel });
    }
}

pub fn get_status(state: &AppState, id: &str) -> Option<JobStatus> {
    state
        .jobs
        .lock()
        .ok()
        .and_then(|jobs| jobs.get(id).map(|record| record.status.clone()))
}

pub fn cancel_job(state: &AppState, id: &str) -> bool {
    state
        .jobs
        .lock()
        .ok()
        .and_then(|jobs| jobs.get(id).map(|record| record.cancel.clone()))
        .map(|cancel| {
            cancel.store(true, Ordering::Relaxed);
            true
        })
        .unwrap_or(false)
}

fn log_entry(stage: &str, message: &str) -> JobLogEntry {
    JobLogEntry {
        timestamp: Utc::now().to_rfc3339(),
        stage: stage.to_string(),
        message: message.to_string(),
    }
}
