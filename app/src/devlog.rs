//! Development log keeping: while the panel runs a development install (the UI bundle from
//! /mnt/data/panelos-ui-dev), the whole system log (kernel, Wi-Fi, the app) is copied every second
//! to the data partition, where a power cut does not take it, and queued for Home Assistant (the
//! PanelOS integration keeps it in a file). Lines from a time without a link wait in the queue and go
//! when the link is back. A release install (the bundle image) keeps none of this.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const SOURCE: &str = "/var/log/messages";
const DIR: &str = "/mnt/data/panelos-log";
const FILE: &str = "/mnt/data/panelos-log/panel.log";
/// A log file is turned over at this size; this many old ones are kept (panel.log.1 …).
const MAX_BYTES: u64 = 2 * 1024 * 1024;
const KEEP: usize = 3;
/// Lines waiting for Home Assistant (the oldest go first when it is longer).
const QUEUE_LINES: usize = 4000;

/// Lines for Home Assistant, oldest first.
pub type Queue = Arc<Mutex<VecDeque<String>>>;

static QUEUE: std::sync::OnceLock<Queue> = std::sync::OnceLock::new();

/// Up to `most` waiting lines for Home Assistant (taken out of the queue); empty without the copier.
pub fn take(most: usize) -> Vec<String> {
    let Some(q) = QUEUE.get() else { return Vec::new() };
    let mut q = q.lock().unwrap();
    let n = most.min(q.len());
    q.drain(..n).collect()
}

/// True on a development install.
pub fn enabled() -> bool {
    Path::new("/mnt/data/panelos-ui-dev/bin/nspanel-app").exists()
}

/// Starts the copier (only on a development install).
pub fn spawn() {
    if !enabled() {
        return;
    }
    let queue: Queue = Arc::new(Mutex::new(VecDeque::new()));
    let q = queue.clone();
    if let Err(e) = std::thread::Builder::new().name("devlog".into()).spawn(move || run(q)) {
        eprintln!("nspanel-app: development log not started: {e}");
        return;
    }
    let _ = QUEUE.set(queue);
}

fn run(queue: Queue) {
    let _ = std::fs::create_dir_all(DIR);
    // from the start of the current log: what happened since boot is kept too
    let mut offset = 0u64;
    let mut partial = String::new();
    loop {
        if let Ok(mut src) = File::open(SOURCE) {
            let len = src.metadata().map(|m| m.len()).unwrap_or(0);
            // the system log was turned over: read the new one from its start
            if len < offset {
                offset = 0;
            }
            if len > offset && src.seek(SeekFrom::Start(offset)).is_ok() {
                let mut buf = Vec::new();
                if src.take(len - offset).read_to_end(&mut buf).is_ok() {
                    offset += buf.len() as u64;
                    partial.push_str(&String::from_utf8_lossy(&buf));
                    let mut lines: Vec<String> = Vec::new();
                    while let Some(n) = partial.find('\n') {
                        let line: String = partial.drain(..=n).collect();
                        let line = line.trim_end().to_string();
                        // the log forwarder's copies of the same lines (for pstore) are left out
                        let copy = line.contains("kernel: [") && line.contains("] syslog: ");
                        if !line.is_empty() && !copy {
                            lines.push(line);
                        }
                    }
                    if !lines.is_empty() {
                        append(&lines);
                        let mut q = queue.lock().unwrap();
                        q.extend(lines);
                        let extra = q.len().saturating_sub(QUEUE_LINES);
                        q.drain(..extra);
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Appends to the kept log (flushed to the card at once), turning it over at its size.
fn append(lines: &[String]) {
    if std::fs::metadata(FILE).map(|m| m.len()).unwrap_or(0) > MAX_BYTES {
        for i in (1..KEEP).rev() {
            let _ = std::fs::rename(format!("{FILE}.{i}"), format!("{FILE}.{}", i + 1));
        }
        let _ = std::fs::rename(FILE, format!("{FILE}.1"));
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(FILE) {
        let mut text = lines.join("\n");
        text.push('\n');
        let _ = f.write_all(text.as_bytes());
        let _ = f.sync_data();
    }
}
