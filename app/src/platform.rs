//! Slint platform for the panel: the software renderer draws into the DRM
//! back buffer, touch comes from evdev, and one poll(2) loop waits for page
//! flips, touch input, work sent from other threads and the next Slint timer.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::os::fd::RawFd;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use slint::platform::software_renderer::MinimalSoftwareWindow;
use slint::platform::{EventLoopProxy, Platform, PointerEventButton, WindowAdapter, WindowEvent};
use slint::{EventLoopError, LogicalPosition, PlatformError};

use crate::display::Display;
use crate::touch::{Touch, TouchEvent};

type Job = Box<dyn FnOnce() + Send>;

/// Set by SIGUSR1: write the current screen to /tmp/nspanel-screen.ppm.
static SCREENSHOT: AtomicBool = AtomicBool::new(false);

extern "C" fn on_sigusr1(_: libc::c_int) {
    SCREENSHOT.store(true, Ordering::SeqCst);
}

static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
/// Milliseconds (since START) of the last touch, for the screensaver.
static LAST_INPUT_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn now_ms() -> u64 {
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Time since the last touch (or since start).
pub fn idle_time() -> Duration {
    Duration::from_millis(now_ms().saturating_sub(LAST_INPUT_MS.load(Ordering::SeqCst)))
}

/// Counts as user activity (touch, approach, wake command).
pub fn note_activity() {
    LAST_INPUT_MS.store(now_ms(), Ordering::SeqCst);
}

/// Set by SIGUSR2: scroll the dashboard one page (development aid).
pub static SCROLL_REQUEST: AtomicBool = AtomicBool::new(false);

extern "C" fn on_sigusr2(_: libc::c_int) {
    SCROLL_REQUEST.store(true, Ordering::SeqCst);
}

pub fn install_screenshot_signal() {
    // SAFETY: the handlers only store to atomics.
    unsafe {
        libc::signal(libc::SIGUSR1, on_sigusr1 as *const () as libc::sighandler_t);
        libc::signal(libc::SIGUSR2, on_sigusr2 as *const () as libc::sighandler_t);
    }
}

/// Lets other threads run code on the UI thread (slint::invoke_from_event_loop).
#[derive(Clone)]
struct Wakeup {
    fd: RawFd,
    jobs: Arc<Mutex<VecDeque<Job>>>,
    quit: Arc<AtomicBool>,
}

impl Wakeup {
    fn new() -> Self {
        // SAFETY: eventfd(2) with flags only.
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        assert!(fd >= 0, "eventfd failed");
        Self { fd, jobs: Arc::default(), quit: Arc::default() }
    }

    fn wake(&self) {
        let one: u64 = 1;
        // SAFETY: writes 8 bytes from a local u64 to the eventfd.
        unsafe { libc::write(self.fd, (&one as *const u64).cast(), 8) };
    }

    fn drain(&self) {
        let mut counter: u64 = 0;
        // SAFETY: reads 8 bytes into a local u64 (non-blocking).
        unsafe { libc::read(self.fd, (&mut counter as *mut u64).cast(), 8) };
        let jobs: Vec<Job> = self.jobs.lock().unwrap().drain(..).collect();
        for job in jobs {
            job();
        }
    }
}

impl EventLoopProxy for Wakeup {
    fn quit_event_loop(&self) -> Result<(), EventLoopError> {
        self.quit.store(true, Ordering::SeqCst);
        self.wake();
        Ok(())
    }

    fn invoke_from_event_loop(&self, event: Box<dyn FnOnce() + Send>) -> Result<(), EventLoopError> {
        self.jobs.lock().unwrap().push_back(event);
        self.wake();
        Ok(())
    }
}

pub struct PanelPlatform {
    window: Rc<MinimalSoftwareWindow>,
    start: Instant,
    display: RefCell<Display>,
    touch: RefCell<Option<Touch>>,
    wakeup: Wakeup,
    /// Development: FIFO with lines "tap x y" / "down x y" / "move x y" / "up x y".
    dev_input: Option<RawFd>,
    dev_buf: RefCell<String>,
}

impl PanelPlatform {
    pub fn new(window: Rc<MinimalSoftwareWindow>, display: Display, touch: Option<Touch>) -> Self {
        let dev_input = std::env::var("NSPANEL_DEV_INPUT").ok().and_then(|path| {
            let c = std::ffi::CString::new(path).ok()?;
            // SAFETY: mkfifo/open on a C string path.
            unsafe {
                libc::mkfifo(c.as_ptr(), 0o600);
                let fd = libc::open(c.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK | libc::O_CLOEXEC);
                (fd >= 0).then_some(fd)
            }
        });
        let _ = now_ms();
        Self {
            window,
            start: Instant::now(),
            display: RefCell::new(display),
            touch: RefCell::new(touch),
            wakeup: Wakeup::new(),
            dev_input,
            dev_buf: RefCell::new(String::new()),
        }
    }

    fn dispatch_touch(&self, event: TouchEvent) {
        note_activity();
        let window_event = match event {
            TouchEvent::Down(x, y) => WindowEvent::PointerPressed {
                position: LogicalPosition::new(x, y),
                button: PointerEventButton::Left,
            },
            TouchEvent::Move(x, y) => WindowEvent::PointerMoved { position: LogicalPosition::new(x, y) },
            TouchEvent::Up(x, y) => WindowEvent::PointerReleased {
                position: LogicalPosition::new(x, y),
                button: PointerEventButton::Left,
            },
        };
        self.window.dispatch_event(window_event);
        if let TouchEvent::Up(..) = event {
            self.window.dispatch_event(WindowEvent::PointerExited);
        }
    }

    fn read_dev_input(&self, fd: RawFd) -> Vec<TouchEvent> {
        let mut buf = [0u8; 512];
        // SAFETY: read into a local buffer.
        let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n <= 0 {
            return Vec::new();
        }
        let mut pending = self.dev_buf.borrow_mut();
        pending.push_str(&String::from_utf8_lossy(&buf[..n as usize]));
        let mut out = Vec::new();
        while let Some(pos) = pending.find('\n') {
            let line: String = pending.drain(..=pos).collect();
            // Development: JSON lines are app commands (show requests, assist states).
            if line.trim_start().starts_with('{') {
                DEV_COMMANDS.lock().unwrap().push(line.trim().to_string());
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() != 3 {
                continue;
            }
            let (Ok(x), Ok(y)) = (parts[1].parse::<f32>(), parts[2].parse::<f32>()) else { continue };
            match parts[0] {
                "tap" => {
                    out.push(TouchEvent::Down(x, y));
                    out.push(TouchEvent::Up(x, y));
                }
                "down" => out.push(TouchEvent::Down(x, y)),
                "move" => out.push(TouchEvent::Move(x, y)),
                "up" => out.push(TouchEvent::Up(x, y)),
                _ => {}
            }
        }
        out
    }
}

/// JSON commands from the development input FIFO, handled by the UI timer.
pub static DEV_COMMANDS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn platform_error(e: std::io::Error) -> PlatformError {
    PlatformError::Other(e.to_string())
}

impl Platform for PanelPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }

    fn duration_since_start(&self) -> Duration {
        self.start.elapsed()
    }

    fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
        Some(Box::new(self.wakeup.clone()))
    }

    fn run_event_loop(&self) -> Result<(), PlatformError> {
        loop {
            self.wakeup.drain();
            if self.wakeup.quit.load(Ordering::SeqCst) {
                return Ok(());
            }
            if SCREENSHOT.swap(false, Ordering::SeqCst) {
                if let Err(e) = self.display.borrow().dump_front("/tmp/nspanel-screen.ppm") {
                    eprintln!("nspanel-app: screenshot failed: {e}");
                }
            }
            slint::platform::update_timers_and_animations();

            {
                let mut display = self.display.borrow_mut();
                if !display.flip_pending() {
                    let drawn = self.window.draw_if_needed(|renderer| {
                        let (buffer, stride) = display.back_buffer();
                        renderer.render(buffer, stride);
                    });
                    if drawn {
                        display.flip().map_err(platform_error)?;
                    }
                }
            }

            let flip_pending = self.display.borrow().flip_pending();
            let timeout_ms: i32 = if self.window.has_active_animations() {
                if flip_pending { -1 } else { 16 }
            } else {
                match slint::platform::duration_until_next_timer_update() {
                    Some(d) => d.as_millis().min(i32::MAX as u128) as i32,
                    None => -1,
                }
            };

            let display_fd = self.display.borrow().fd();
            let touch_fd = self.touch.borrow().as_ref().map(|t| t.fd()).unwrap_or(-1);
            let mut fds = [
                libc::pollfd { fd: display_fd, events: libc::POLLIN, revents: 0 },
                libc::pollfd { fd: touch_fd, events: libc::POLLIN, revents: 0 },
                libc::pollfd { fd: self.wakeup.fd, events: libc::POLLIN, revents: 0 },
                libc::pollfd { fd: self.dev_input.unwrap_or(-1), events: libc::POLLIN, revents: 0 },
            ];
            // SAFETY: poll(2) on a local array; a negative fd is ignored.
            let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout_ms) };
            if n < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(platform_error(err));
            }

            if fds[0].revents & libc::POLLIN != 0 {
                self.display.borrow_mut().handle_events().map_err(platform_error)?;
            }

            if fds[1].revents & libc::POLLIN != 0 {
                let events = self.touch.borrow_mut().as_mut().map(|t| t.read_events()).unwrap_or_default();
                for event in events {
                    self.dispatch_touch(event);
                }
            }

            if let Some(fd) = self.dev_input {
                if fds[3].revents & libc::POLLIN != 0 {
                    for event in self.read_dev_input(fd) {
                        self.dispatch_touch(event);
                    }
                }
            }
        }
    }
}
