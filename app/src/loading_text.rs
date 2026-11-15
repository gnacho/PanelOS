//! The line under the loading screen's mark while a dashboard loads: one of
//! the first lines opens a load, then another one every 6 s, none twice in a
//! load. UI thread only.

use std::cell::RefCell;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use slint::ComponentHandle;

use crate::MainWindow;

/// The first ones say what is being done: one of them opens a load.
const OPENERS: usize = 14;
const LINES: [&str; 20] = [
    "Warming up the dashboard…",
    "Waking up Home Assistant…",
    "Fetching all your buttons…",
    "Dusting off the dashboard…",
    "Herding the entities…",
    "Polishing every tile…",
    "Teaching the cards their places…",
    "Summoning the dashboard…",
    "Asking Home Assistant nicely…",
    "Rounding up the sensors…",
    "Counting the light bulbs…",
    "Untangling the YAML…",
    "Laying out your home…",
    "Building your home, tile by tile…",
    "The dashboard is putting on shoes.",
    "Loading. Patience is a feature.",
    "Still loading. Still worth it.",
    "Any second now. Or thirty.",
    "Big dashboard, small panel. Wait.",
    "Hold on, the cards are shuffling.",
];
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
pub fn start(ui: &MainWindow) {
    let mut seed = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
    let first = (random(&mut seed) % OPENERS as u64) as usize;
    ui.set_loading_line_a(LINES[first].into());
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
    fn next(&mut self, ui: &MainWindow) {
        if self.used.len() == LINES.len() {
            self.used.clear();
        }
        let free: Vec<usize> = (0..LINES.len()).filter(|i| !self.used.contains(i)).collect();
        let pick = free[(random(&mut self.seed) % free.len() as u64) as usize];
        self.used.push(pick);
        self.on_a = !self.on_a;
        if self.on_a {
            ui.set_loading_line_a(LINES[pick].into());
        } else {
            ui.set_loading_line_b(LINES[pick].into());
        }
        ui.set_loading_show_a(self.on_a);
    }
}
