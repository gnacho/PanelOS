//! Development aid (`NSPANEL_WEB_STATS=1`): timing of the web view's frames
//! and of touches from the development input (`NSPANEL_DEV_INPUT`), logged
//! once per gesture: how long until the first new frame is on screen, how
//! many frames came and were shown, and where the time went (read back in
//! the helper, copy in the app, drawing, waiting for the flip).
//!
//! Times are CLOCK_MONOTONIC microseconds, the same clock in the helper.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

/// A frame's way from WebKit to the screen.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameTimes {
    /// WebKit handed the frame to the helper.
    pub produced: u64,
    /// glReadPixels in the helper.
    pub readback: u64,
    /// The app's reader thread had it copied.
    pub copied: u64,
    /// The UI thread took it.
    pub on_ui: u64,
    /// Drawn into the back buffer (duration of the whole draw).
    pub draw: u64,
    /// On screen (page flip done).
    pub shown: u64,
}

#[derive(Default)]
struct Stats {
    gesture: Option<(String, u64, u64)>,
    pending: Option<FrameTimes>,
    flipping: Option<FrameTimes>,
    produced: Vec<u64>,
    shown: Vec<FrameTimes>,
}

static STATS: Mutex<Option<Stats>> = Mutex::new(None);
static ENABLED: AtomicU8 = AtomicU8::new(2);

pub fn enabled() -> bool {
    match ENABLED.load(Ordering::Relaxed) {
        0 => false,
        1 => true,
        _ => {
            let on = std::env::var_os("NSPANEL_WEB_STATS").is_some();
            ENABLED.store(on as u8, Ordering::Relaxed);
            on
        }
    }
}

pub fn now_us() -> u64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: clock_gettime into a local timespec.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as u64 * 1_000_000 + ts.tv_nsec as u64 / 1000
}

fn with(f: impl FnOnce(&mut Stats)) {
    if !enabled() {
        return;
    }
    let mut guard = STATS.lock().unwrap();
    f(guard.get_or_insert_with(Stats::default));
}

/// Reader thread: a frame from the helper is copied.
pub fn frame_copied(produced: u64, readback: u64) -> FrameTimes {
    let t = FrameTimes { produced, readback, copied: now_us(), ..Default::default() };
    with(|s| s.produced.push(produced));
    t
}

/// UI thread: the frame is set on the web layer (replaces one not drawn yet).
pub fn frame_on_ui(mut t: FrameTimes) {
    t.on_ui = now_us();
    with(|s| s.pending = Some(t));
}

/// Platform: a draw finished (it holds the pending frame, if any).
pub fn drawn(started: u64) {
    with(|s| {
        if let Some(mut t) = s.pending.take() {
            t.draw = now_us() - started;
            s.flipping = Some(t);
        }
    });
}

/// Platform: the page flip is done.
pub fn flipped() {
    with(|s| {
        if let Some(mut t) = s.flipping.take() {
            t.shown = now_us();
            s.shown.push(t);
        }
    });
}

/// Development input: a gesture starts (`end` = when its last touch is due).
pub fn gesture(kind: &str, end: u64) {
    with(|s| {
        s.gesture = Some((kind.to_string(), now_us(), end));
        s.produced.clear();
        s.shown.clear();
    });
}

/// Called often (event loop): logs a gesture 1.5 s after its last touch.
pub fn report_if_due() {
    let mut line = None;
    with(|s| {
        let Some((kind, start, end)) = s.gesture.clone() else { return };
        if now_us() < end + 1_500_000 {
            return;
        }
        s.gesture = None;
        let shown: Vec<&FrameTimes> = s.shown.iter().filter(|t| t.produced >= start).collect();
        let produced = s.produced.iter().filter(|&&p| p >= start).count();
        let ms = |us: u64| us as f64 / 1000.0;
        let avg = |f: &dyn Fn(&FrameTimes) -> u64| {
            if shown.is_empty() { 0.0 } else { ms(shown.iter().map(|t| f(t)).sum::<u64>() / shown.len() as u64) }
        };
        let max = |f: &dyn Fn(&FrameTimes) -> u64| ms(shown.iter().map(|t| f(t)).max().unwrap_or(0));
        let first = shown.first().map(|t| ms(t.shown.saturating_sub(start))).unwrap_or(-1.0);
        let last = shown.last().map(|t| ms(t.shown.saturating_sub(start))).unwrap_or(-1.0);
        let during: Vec<&&FrameTimes> = shown.iter().filter(|t| t.shown <= end).collect();
        let fps = if end > start { during.len() as f64 * 1e6 / (end - start) as f64 } else { 0.0 };
        line = Some(format!(
            "web stats {kind}: first frame on screen {first:.0} ms, last {last:.0} ms; made {produced}, shown {} ({fps:.1}/s)\n\
             web stats {kind}: avg/max ms: readback {:.1}/{:.1}, to app {:.1}/{:.1}, to UI {:.1}/{:.1}\n\
             web stats {kind}: avg/max ms: draw {:.1}/{:.1}, made->on screen {:.1}/{:.1}",
            shown.len(),
            avg(&|t| t.readback), max(&|t| t.readback),
            avg(&|t| t.copied.saturating_sub(t.produced + t.readback)), max(&|t| t.copied.saturating_sub(t.produced + t.readback)),
            avg(&|t| t.on_ui.saturating_sub(t.copied)), max(&|t| t.on_ui.saturating_sub(t.copied)),
            avg(&|t| t.draw), max(&|t| t.draw),
            avg(&|t| t.shown.saturating_sub(t.produced)), max(&|t| t.shown.saturating_sub(t.produced)),
        ));
    });
    if let Some(line) = line {
        for part in line.lines() {
            eprintln!("nspanel-app: {part}");
        }
    }
}
