//! Touch input straight from the kernel's evdev device (no libinput): the
//! single-touch emulation the multitouch driver provides (ABS_X, ABS_Y,
//! BTN_TOUCH) is enough for a pointer.

use std::fs;
use std::io;
use std::os::fd::RawFd;

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const SYN_REPORT: u16 = 0x00;
const BTN_TOUCH: u16 = 0x14a;
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;

#[repr(C)]
struct InputEvent {
    time: libc::timeval,
    kind: u16,
    code: u16,
    value: i32,
}

#[derive(Debug, Clone, Copy)]
pub enum TouchEvent {
    Down(f32, f32),
    Move(f32, f32),
    Up(f32, f32),
}

pub struct Touch {
    fd: RawFd,
    pub name: String,
    x: i32,
    y: i32,
    down: bool,
    reported_down: bool,
    moved: bool,
}

impl Touch {
    /// Opens the first input device whose name looks like a touchscreen.
    pub fn open() -> io::Result<Self> {
        let mut entries: Vec<_> = fs::read_dir("/sys/class/input")?
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("event"))
            .collect();
        entries.sort();
        for entry in entries {
            let name = fs::read_to_string(format!("/sys/class/input/{entry}/device/name"))
                .unwrap_or_default()
                .trim()
                .to_string();
            if !name.to_lowercase().contains("touch") {
                continue;
            }
            let path = std::ffi::CString::new(format!("/dev/input/{entry}")).unwrap();
            // SAFETY: plain open(2) of a device node.
            let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK | libc::O_CLOEXEC) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            return Ok(Self { fd, name, x: 0, y: 0, down: false, reported_down: false, moved: false });
        }
        Err(io::Error::new(io::ErrorKind::NotFound, "no touchscreen input device"))
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }
}

impl Drop for Touch {
    fn drop(&mut self) {
        // SAFETY: the descriptor is ours and closed once.
        unsafe { libc::close(self.fd) };
    }
}

impl Touch {

    /// Reads everything available and returns the resulting pointer events.
    pub fn read_events(&mut self) -> Vec<TouchEvent> {
        let mut out = Vec::new();
        let size = std::mem::size_of::<InputEvent>();
        let mut buf = vec![0u8; size * 64];
        loop {
            // SAFETY: reading into a byte buffer of the given length.
            let n = unsafe { libc::read(self.fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                break;
            }
            for chunk in buf[..n as usize].chunks_exact(size) {
                // SAFETY: the kernel writes whole input_event structs.
                let ev = unsafe { std::ptr::read_unaligned(chunk.as_ptr().cast::<InputEvent>()) };
                match (ev.kind, ev.code) {
                    (EV_ABS, ABS_X) => {
                        self.x = ev.value;
                        self.moved = true;
                    }
                    (EV_ABS, ABS_Y) => {
                        self.y = ev.value;
                        self.moved = true;
                    }
                    (EV_KEY, BTN_TOUCH) => self.down = ev.value != 0,
                    (EV_SYN, SYN_REPORT) => {
                        let (x, y) = (self.x as f32, self.y as f32);
                        match (self.reported_down, self.down) {
                            (false, true) => out.push(TouchEvent::Down(x, y)),
                            (true, true) if self.moved => out.push(TouchEvent::Move(x, y)),
                            (true, false) => out.push(TouchEvent::Up(x, y)),
                            _ => {}
                        }
                        self.reported_down = self.down;
                        self.moved = false;
                    }
                    _ => {}
                }
            }
        }
        out
    }
}
