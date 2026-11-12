//! Idle Wi-Fi roam check.
//!
//! wpa_supplicant changes the access point only when the link breaks. After
//! an outage the panel can stay on a farther access point for hours, although
//! the nearer one is back. So about every 30 minutes, and only while the panel
//! is idle (no voice run, no sound, no ringing timer), the panel scans and
//! moves to an access point of the same network that is at least 10 dB
//! stronger.
//!
//! The move is a one-shot `roam`: the network is never tied to one access
//! point, so a panel that is moved to another room keeps working.
//!
//! The thresholds are the same as in ESPHome's `wifi.roam`: no scan at -49 dBm
//! or better, and a move needs at least 10 dB more.

use std::process::Command;
use std::time::Duration;

const IFACE: &str = "wlan0";
/// First check after the app starts.
const FIRST_CHECK: Duration = Duration::from_secs(5 * 60);
/// Between two checks.
const INTERVAL: Duration = Duration::from_secs(30 * 60);
/// When the panel is busy, not connected, or wpa_supplicant refuses the
/// scan: the next try.
const RETRY: Duration = Duration::from_secs(60);
/// Scan results are read this long after the scan request (2.4 and 5 GHz
/// take about 3–5 s).
const SCAN_WAIT: Duration = Duration::from_secs(8);
/// After a move: when the new link is checked.
const MOVE_WAIT: Duration = Duration::from_secs(15);
/// No scan at this signal or better (dBm).
const GOOD_SIGNAL: i32 = -49;
/// A move needs an access point at least this much stronger (dB).
const MIN_IMPROVEMENT: i32 = 10;

/// The current connection.
#[derive(Debug, Clone, PartialEq)]
struct Link {
    bssid: String,
    ssid: String,
}

/// One access point from the scan results.
#[derive(Debug, Clone, PartialEq)]
struct Ap {
    bssid: String,
    freq: u32,
    signal: i32,
    ssid: String,
}

#[derive(Debug, PartialEq)]
enum Decision {
    /// `current`: the current access point's signal; `best`: the strongest
    /// other access point of the same network, if any.
    Stay { current: i32, best: Option<i32> },
    Move { current: i32, to: Ap },
}

fn wpa_cli(args: &[&str]) -> Option<String> {
    let out = Command::new("wpa_cli").arg("-i").arg(IFACE).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn ok(reply: Option<String>) -> bool {
    reply.is_some_and(|r| r.trim() == "OK")
}

/// `wpa_cli status`: the current link, only when fully connected.
fn parse_status(text: &str) -> Option<Link> {
    let get = |key: &str| text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('='));
    if get("wpa_state")? != "COMPLETED" {
        return None;
    }
    Some(Link { bssid: get("bssid")?.to_ascii_lowercase(), ssid: get("ssid")?.to_string() })
}

/// `wpa_cli signal_poll`: the link's signal in dBm.
fn parse_signal(text: &str) -> Option<i32> {
    text.lines().find_map(|l| l.strip_prefix("RSSI="))?.trim().parse().ok()
}

/// `wpa_cli scan_results`: a header line, then bssid, frequency, signal
/// level, flags and SSID, separated by tabs.
fn parse_scan(text: &str) -> Vec<Ap> {
    text.lines()
        .skip_while(|l| !l.starts_with("bssid"))
        .skip(1)
        .filter_map(|l| {
            let mut f = l.split('\t');
            let bssid = f.next()?.to_ascii_lowercase();
            let freq = f.next()?.parse().ok()?;
            let signal = f.next()?.parse().ok()?;
            let _flags = f.next()?;
            let ssid = f.next().unwrap_or_default().to_string();
            Some(Ap { bssid, freq, signal, ssid })
        })
        .collect()
}

/// The current access point is compared as the same scan saw it (the same
/// measurement as the others); `link_signal` is used when the scan missed it.
fn decide(link: &Link, link_signal: i32, aps: &[Ap]) -> Decision {
    let current = aps.iter().find(|a| a.bssid == link.bssid).map_or(link_signal, |a| a.signal);
    let best = aps.iter().filter(|a| a.ssid == link.ssid && a.bssid != link.bssid).max_by_key(|a| a.signal);
    match best {
        Some(b) if b.signal >= current + MIN_IMPROVEMENT => Decision::Move { current, to: b.clone() },
        _ => Decision::Stay { current, best: best.map(|b| b.signal) },
    }
}

/// One check. False when it could not run (not connected, scan refused, the
/// panel got busy): the caller tries again soon.
fn check(idle: &dyn Fn() -> bool) -> bool {
    let Some(link) = wpa_cli(&["status"]).as_deref().and_then(parse_status) else { return false };
    let Some(signal) = wpa_cli(&["signal_poll"]).as_deref().and_then(parse_signal) else { return false };
    if signal >= GOOD_SIGNAL {
        eprintln!("nspanel-app: roam check: {signal} dBm on {}, strong enough, no scan", link.bssid);
        return true;
    }
    // FAIL-BUSY while wpa_supplicant scans or connects by itself.
    if !ok(wpa_cli(&["scan"])) {
        return false;
    }
    std::thread::sleep(SCAN_WAIT);
    let aps = wpa_cli(&["scan_results"]).map(|t| parse_scan(&t)).unwrap_or_default();
    match decide(&link, signal, &aps) {
        Decision::Stay { current, best: Some(best) } => {
            eprintln!("nspanel-app: roam check: {current} dBm on {}, strongest other access point {best} dBm, staying", link.bssid)
        }
        Decision::Stay { current, best: None } => {
            eprintln!("nspanel-app: roam check: {current} dBm on {}, no other access point of the network, staying", link.bssid)
        }
        Decision::Move { current, to } => {
            // A voice run may have started during the scan.
            if !idle() {
                return false;
            }
            eprintln!(
                "nspanel-app: roam check: moving from {} ({current} dBm) to {} ({} dBm, {} MHz)",
                link.bssid, to.bssid, to.signal, to.freq
            );
            if !ok(wpa_cli(&["roam", &to.bssid])) {
                eprintln!("nspanel-app: roam check: wpa_supplicant refused the move");
                return true;
            }
            std::thread::sleep(MOVE_WAIT);
            match wpa_cli(&["status"]).as_deref().and_then(parse_status) {
                Some(now) if now.bssid == to.bssid => eprintln!("nspanel-app: roam check: now on {}", now.bssid),
                Some(now) => eprintln!("nspanel-app: roam check: the move did not happen, on {}", now.bssid),
                None => eprintln!("nspanel-app: roam check: not connected after the move, wpa_supplicant reconnects by itself"),
            }
        }
    }
    true
}

/// Runs the roam check thread; `idle` says whether the panel is idle.
pub fn spawn(idle: impl Fn() -> bool + Send + 'static) {
    let started = std::thread::Builder::new().name("roam".into()).spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            let done = idle() && check(&idle);
            std::thread::sleep(if done { INTERVAL } else { RETRY });
        }
    });
    if let Err(e) = started {
        eprintln!("nspanel-app: roam check not started: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: &str = "bssid=AA:00:00:00:00:01\nfreq=2462\nssid=Home\nid=0\nmode=station\nkey_mgmt=WPA2-PSK\nwpa_state=COMPLETED\nip_address=10.0.0.5\n";

    fn ap(bssid: &str, signal: i32, ssid: &str) -> Ap {
        Ap { bssid: bssid.into(), freq: 2437, signal, ssid: ssid.into() }
    }

    #[test]
    fn status_only_when_connected() {
        let link = parse_status(STATUS).unwrap();
        assert_eq!(link, Link { bssid: "aa:00:00:00:00:01".into(), ssid: "Home".into() });
        assert_eq!(parse_status(&STATUS.replace("COMPLETED", "SCANNING")), None);
    }

    #[test]
    fn signal_poll() {
        assert_eq!(parse_signal("RSSI=-67\nLINKSPEED=58\nNOISE=9999\nFREQUENCY=2462\n"), Some(-67));
        assert_eq!(parse_signal("FAIL\n"), None);
    }

    #[test]
    fn scan_results() {
        let text = "bssid / frequency / signal level / flags / ssid\n\
                    aa:00:00:00:00:01\t2462\t-67\t[WPA2-PSK-CCMP][ESS]\tHome\n\
                    AA:00:00:00:00:02\t2437\t-45\t[WPA2-PSK-CCMP][ESS]\tHome\n\
                    aa:00:00:00:00:03\t5180\t-80\t[ESS]\t\n";
        let aps = parse_scan(text);
        assert_eq!(aps.len(), 3);
        assert_eq!(aps[1], Ap { bssid: "aa:00:00:00:00:02".into(), freq: 2437, signal: -45, ssid: "Home".into() });
        assert_eq!(aps[2].ssid, "");
    }

    #[test]
    fn moves_only_for_ten_db_on_the_same_network() {
        let link = parse_status(STATUS).unwrap();
        let current = ap("aa:00:00:00:00:01", -67, "Home");
        // 9 dB more: stays.
        let aps = [current.clone(), ap("aa:00:00:00:00:02", -58, "Home")];
        assert_eq!(decide(&link, -66, &aps), Decision::Stay { current: -67, best: Some(-58) });
        // 10 dB more: moves.
        let aps = [current.clone(), ap("aa:00:00:00:00:02", -57, "Home")];
        assert!(matches!(decide(&link, -66, &aps), Decision::Move { current: -67, to } if to.bssid == "aa:00:00:00:00:02"));
        // A stronger access point of another network does not count.
        let aps = [current.clone(), ap("aa:00:00:00:00:03", -30, "Other")];
        assert_eq!(decide(&link, -66, &aps), Decision::Stay { current: -67, best: None });
        // The scan missed the current access point: the link's signal counts.
        let aps = [ap("aa:00:00:00:00:02", -57, "Home")];
        assert_eq!(decide(&link, -66, &aps), Decision::Stay { current: -66, best: Some(-57) });
    }
}
