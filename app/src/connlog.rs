//! The connection log: Home Assistant sessions ending and coming back (with the cause), the Wi-Fi roam checks with
//! the access points they saw, moves and the link watch's reconnects, one line each with the local time, kept on the
//! persistent partition (`/mnt/persist/nspanel/connection.log`, the last ~256 KB in two files). The system log lives
//! in RAM and covers minutes while the integration's live picture is open, so a drop in the night could not be looked
//! at afterwards.

use std::io::Write;
use std::sync::Mutex;

const PATH: &str = "/mnt/persist/nspanel/connection.log";
const OLD: &str = "/mnt/persist/nspanel/connection.log.1";
const MAX: u64 = 128 * 1024;

static LOCK: Mutex<()> = Mutex::new(());

/// One line in the connection log (and in the system log, as before).
pub fn note(line: &str) {
    eprintln!("nspanel-app: {line}");
    let _g = LOCK.lock();
    if std::fs::metadata(PATH).is_ok_and(|m| m.len() > MAX) {
        let _ = std::fs::rename(PATH, OLD);
    }
    let when = crate::pages::local_format(crate::pages::now(), "%Y-%m-%d %H:%M:%S");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(PATH) {
        let _ = writeln!(f, "{when} {line}");
    }
}
