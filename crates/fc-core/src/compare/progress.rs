use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::model::ScanProgress;

/// Receives progress reports; called from worker threads.
pub type ProgressFn<'a> = &'a (dyn Fn(ScanProgress) + Sync);

const INTERVAL: Duration = Duration::from_millis(100);

/// Thread-safe scan progress counter that reports at most every 100 ms.
pub(crate) struct ProgressTracker<'a> {
    progress: Option<ProgressFn<'a>>,
    items: AtomicU64,
    bytes_compared: AtomicU64,
    bytes_total: AtomicU64,
    current: Mutex<String>,
    last_report: Mutex<Option<Instant>>,
}

impl<'a> ProgressTracker<'a> {
    pub fn new(progress: Option<ProgressFn<'a>>) -> Self {
        Self {
            progress,
            items: AtomicU64::new(0),
            bytes_compared: AtomicU64::new(0),
            bytes_total: AtomicU64::new(0),
            current: Mutex::new(String::new()),
            last_report: Mutex::new(None),
        }
    }

    pub fn items(&self) -> u64 {
        self.items.load(Ordering::Relaxed)
    }

    pub fn item(&self) {
        self.items.fetch_add(1, Ordering::Relaxed);
        self.maybe_report();
    }

    pub fn folder(&self, path: &str) {
        if self.progress.is_none() {
            return;
        }
        *self.current.lock().unwrap() = path.to_string();
        self.maybe_report();
    }

    /// Switches to the content verification phase.
    pub fn begin_content(&self, total_bytes: u64) {
        self.bytes_total.store(total_bytes, Ordering::Relaxed);
        self.current.lock().unwrap().clear();
        if let Some(p) = self.progress {
            p(ScanProgress {
                items_scanned: self.items(),
                current_folder: String::new(),
                bytes_compared: 0,
                bytes_total: total_bytes,
            });
        }
    }

    pub fn content_file(&self, path: &str) {
        self.folder(path);
    }

    pub fn content_bytes(&self, bytes: u64) {
        self.bytes_compared.fetch_add(bytes, Ordering::Relaxed);
        self.maybe_report();
    }

    pub fn report_final(&self) {
        if let Some(p) = self.progress {
            p(ScanProgress {
                items_scanned: self.items(),
                ..ScanProgress::default()
            });
        }
    }

    fn maybe_report(&self) {
        let Some(p) = self.progress else { return };
        let now = Instant::now();
        {
            let mut last = self.last_report.lock().unwrap();
            if last.is_some_and(|t| now.duration_since(t) < INTERVAL) {
                return;
            }
            *last = Some(now);
        }
        p(ScanProgress {
            items_scanned: self.items(),
            current_folder: self.current.lock().unwrap().clone(),
            bytes_compared: self.bytes_compared.load(Ordering::Relaxed),
            bytes_total: self.bytes_total.load(Ordering::Relaxed),
        });
    }
}
