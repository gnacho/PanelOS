//! The line under the loading screen's mark while a dashboard loads: one of
//! the first lines opens a load, then another one every 6 s, none twice in a
//! load. UI thread only.

use std::cell::RefCell;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use slint::ComponentHandle;

use crate::AppWindow;

/// The first ones say what is being done: one of them opens a load.
const OPENERS: usize = 14;

fn lines() -> &'static [String] {
    crate::texts::list("load.lines")
}

const EVERY: Duration = Duration::from_secs(6);

struct Rotation {
    used: Vec<usize>,
    seed: u64,
    /// The line shows in text a (else b): the two cross-fade.
    on_a: bool,
    _timer: slint::Timer,
}

thread_local! {
    static ROTATION: RefCell<Option<Rotation>> = const { RefCell::new(None) };
}

fn random(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

/// A load started: the first line, then the next ones until `stop`.
pub fn start(ui: &AppWindow) {
    let mut seed = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
    let lines = lines();
    if lines.is_empty() {
        return;
    }
    let first = (random(&mut seed) % OPENERS.min(lines.len()) as u64) as usize;
    ui.set_loading_line_a(lines[first].as_str().into());
    ui.set_loading_line_b("".into());
    ui.set_loading_show_a(true);
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    timer.start(slint::TimerMode::Repeated, EVERY, move || {
        let Some(ui) = weak.upgrade() else { return };
        ROTATION.with(|r| {
            if let Some(rotation) = r.borrow_mut().as_mut() {
                rotation.next(&ui);
            }
        });
    });
    ROTATION.with(|r| *r.borrow_mut() = Some(Rotation { used: vec![first], seed, on_a: true, _timer: timer }));
}

/// The page is ready: no more lines.
pub fn stop() {
    ROTATION.with(|r| *r.borrow_mut() = None);
}

impl Rotation {
    fn next(&mut self, ui: &AppWindow) {
        let lines = lines();
        if self.used.len() >= lines.len() {
            self.used.clear();
        }
        let free: Vec<usize> = (0..lines.len()).filter(|i| !self.used.contains(i)).collect();
        if free.is_empty() {
            return;
        }
        let pick = free[(random(&mut self.seed) % free.len() as u64) as usize];
        self.used.push(pick);
        self.on_a = !self.on_a;
        if self.on_a {
            ui.set_loading_line_a(lines[pick].as_str().into());
        } else {
            ui.set_loading_line_b(lines[pick].as_str().into());
        }
        ui.set_loading_show_a(self.on_a);
    }
}
