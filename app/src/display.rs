//! Display output through DRM/KMS: two dumb buffers (XRGB8888) shown with
//! page flips, so the UI never tears and the CPU draws only into the buffer
//! that is not on screen.

use std::fs::OpenOptions;
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, RawFd};

use drm::buffer::{Buffer, DrmFourcc};
use drm::control::{connector, crtc, framebuffer, Device as ControlDevice, Event, PageFlipFlags};
use drm::Device as BasicDevice;
use slint::platform::software_renderer::{PremultipliedRgbaColor, TargetPixel};

/// One pixel of a DRM XRGB8888 buffer (little-endian byte order B, G, R, X).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Xrgb8888 {
    b: u8,
    g: u8,
    r: u8,
    x: u8,
}

impl TargetPixel for Xrgb8888 {
    fn blend(&mut self, color: PremultipliedRgbaColor) {
        let keep = (255 - color.alpha) as u16;
        let mix = |dst: u8, src: u8| ((dst as u16 * keep + 127) / 255) as u8 + src;
        self.r = mix(self.r, color.red);
        self.g = mix(self.g, color.green);
        self.b = mix(self.b, color.blue);
    }

    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { b, g, r, x: 255 }
    }
}

struct Card(std::fs::File);

impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl BasicDevice for Card {}
impl ControlDevice for Card {}

pub struct Display {
    card: Card,
    crtc: crtc::Handle,
    framebuffers: [framebuffer::Handle; 2],
    pixels: [&'static mut [Xrgb8888]; 2],
    stride: usize,
    pub width: u32,
    pub height: u32,
    back: usize,
    flip_pending: bool,
}

impl Display {
    pub fn open(path: &str) -> io::Result<Self> {
        let card = Card(OpenOptions::new().read(true).write(true).open(path)?);
        let resources = card.resource_handles()?;

        let connector = resources
            .connectors()
            .iter()
            .filter_map(|&handle| card.get_connector(handle, true).ok())
            .find(|c| c.state() == connector::State::Connected)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no connected display"))?;
        let mode = *connector
            .modes()
            .first()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "display has no mode"))?;
        let (width, height) = mode.size();

        let encoder_handle = connector
            .current_encoder()
            .or_else(|| connector.encoders().first().copied())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no encoder"))?;
        let encoder = card.get_encoder(encoder_handle)?;
        let crtc = encoder
            .crtc()
            .or_else(|| resources.filter_crtcs(encoder.possible_crtcs()).first().copied())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no CRTC"))?;

        let mut framebuffers = Vec::new();
        let mut pixels = Vec::new();
        let mut stride = 0;
        for _ in 0..2 {
            // The buffers live as long as the program, so they are leaked on
            // purpose; this keeps the memory mappings simple ('static).
            let buffer: &'static mut _ = Box::leak(Box::new(card.create_dumb_buffer(
                (width as u32, height as u32),
                DrmFourcc::Xrgb8888,
                32,
            )?));
            framebuffers.push(card.add_framebuffer(&*buffer, 24, 32)?);
            stride = buffer.pitch() as usize / 4;
            let mapping = Box::leak(Box::new(card.map_dumb_buffer(buffer)?));
            let bytes: &'static mut [u8] = &mut mapping[..];
            let len = bytes.len() / 4;
            // SAFETY: the mapping is page aligned and lives for the whole
            // program; Xrgb8888 is four plain bytes.
            let px = unsafe { std::slice::from_raw_parts_mut(bytes.as_mut_ptr().cast::<Xrgb8888>(), len) };
            px.fill(Xrgb8888::from_rgb(0, 0, 0));
            pixels.push(px);
        }
        let framebuffers = [framebuffers[0], framebuffers[1]];
        let pixels: [&'static mut [Xrgb8888]; 2] = pixels.try_into().ok().unwrap();

        card.set_crtc(crtc, Some(framebuffers[0]), (0, 0), &[connector.handle()], Some(mode))?;

        Ok(Self {
            card,
            crtc,
            framebuffers,
            pixels,
            stride,
            width: width as u32,
            height: height as u32,
            back: 1,
            flip_pending: false,
        })
    }

    pub fn fd(&self) -> RawFd {
        self.card.0.as_raw_fd()
    }

    pub fn flip_pending(&self) -> bool {
        self.flip_pending
    }

    /// The buffer that is not on screen, and its stride in pixels.
    pub fn back_buffer(&mut self) -> (&mut [Xrgb8888], usize) {
        let stride = self.stride;
        (&mut *self.pixels[self.back], stride)
    }

    /// Shows the back buffer at the next vertical blank.
    pub fn flip(&mut self) -> io::Result<()> {
        self.card
            .page_flip(self.crtc, self.framebuffers[self.back], PageFlipFlags::EVENT, None)?;
        self.flip_pending = true;
        Ok(())
    }

    /// Writes the buffer that is on screen to a PPM file (development aid).
    pub fn dump_front(&self, path: &str) -> io::Result<()> {
        use std::io::Write;
        let front = &self.pixels[self.back ^ 1];
        let (w, h) = (self.width as usize, self.height as usize);
        let mut out = Vec::with_capacity(w * h * 3 + 32);
        write!(out, "P6\n{w} {h}\n255\n")?;
        for y in 0..h {
            for px in &front[y * self.stride..y * self.stride + w] {
                out.extend_from_slice(&[px.r, px.g, px.b]);
            }
        }
        std::fs::write(path, out)
    }

    /// Reads DRM events after the fd became readable.
    pub fn handle_events(&mut self) -> io::Result<()> {
        for event in self.card.receive_events()? {
            if let Event::PageFlip(_) = event {
                self.flip_pending = false;
                self.back ^= 1;
            }
        }
        Ok(())
    }
}
