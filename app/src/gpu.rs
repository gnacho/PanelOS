//! GPU output (feature `gpu`): Skia draws the UI with OpenGL ES (Mesa Panfrost
//! on the Mali-G31, libraries from the web bundle) into a GBM surface, and the
//! surface's buffers go to the screen with page flips through [`Display`].
//!
//! The GBM / EGL glue follows Slint's KMS backend (i-slint-backend-linuxkms
//! 1.18: display/gbmdisplay.rs and renderer/skia.rs, GPL-3.0, © SixtyFPS GmbH),
//! adapted to this app's own event loop, display and web view buffers.

use std::cell::{Cell, RefCell};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use drm014::control::{Device as _, FbCmd2Flags};
use gbm::AsRaw;
use i_slint_renderer_skia::{opengl_surface::OpenGLSurface, SkiaRenderer, SkiaSharedContext};
use slint::platform::{Renderer, WindowAdapter, WindowEvent};
use slint::{PhysicalSize, PlatformError, Window};

use crate::display::Display;

/// The DRM device again (a duplicated descriptor), for the GBM device: GBM's
/// crate speaks drm 0.14, the rest of the app drm 0.15. Both only issue ioctls
/// on the same kernel device.
struct GbmCard(std::fs::File);

impl AsFd for GbmCard {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl drm014::Device for GbmCard {}
impl drm014::control::Device for GbmCard {}

/// Raw GBM pointers for EGL (glutin) through raw-window-handle.
struct GbmHandles {
    device: std::ptr::NonNull<std::ffi::c_void>,
    surface: std::ptr::NonNull<std::ffi::c_void>,
}

impl raw_window_handle::HasWindowHandle for GbmHandles {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        // SAFETY: the surface lives as long as the program (it is leaked in `Gpu::new`).
        Ok(unsafe {
            raw_window_handle::WindowHandle::borrow_raw(raw_window_handle::RawWindowHandle::Gbm(
                raw_window_handle::GbmWindowHandle::new(self.surface),
            ))
        })
    }
}

impl raw_window_handle::HasDisplayHandle for GbmHandles {
    fn display_handle(&self) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        // SAFETY: the device lives as long as the program (it is leaked in `Gpu::new`).
        Ok(unsafe {
            raw_window_handle::DisplayHandle::borrow_raw(raw_window_handle::RawDisplayHandle::Gbm(
                raw_window_handle::GbmDisplayHandle::new(self.device),
            ))
        })
    }
}

/// GBM buffers of the UI on their way to the screen.
struct Buffers {
    surface: &'static gbm::Surface<u32>,
    device: &'static gbm::Device<GbmCard>,
    /// On screen.
    shown: RefCell<Option<gbm::BufferObject<u32>>>,
    /// Waiting for its page flip.
    pending: RefCell<Option<gbm::BufferObject<u32>>>,
}

/// The window: Slint's window plus the Skia renderer on the GBM surface.
pub struct GpuWindow {
    window: Window,
    renderer: SkiaRenderer,
    size: PhysicalSize,
    needs_redraw: Cell<bool>,
    buffers: Rc<Buffers>,
    display: Rc<RefCell<Display>>,
}

impl std::ops::Deref for GpuWindow {
    type Target = Window;
    fn deref(&self) -> &Window {
        &self.window
    }
}

impl WindowAdapter for GpuWindow {
    fn window(&self) -> &Window {
        &self.window
    }

    fn renderer(&self) -> &dyn Renderer {
        &self.renderer
    }

    fn size(&self) -> PhysicalSize {
        self.size
    }

    fn request_redraw(&self) {
        self.needs_redraw.set(true);
    }
}

thread_local! {
    /// Called with what is on the screen (width, height, RGB rows top down)
    /// after the next frame is drawn.
    static READBACK: RefCell<Vec<Box<dyn FnOnce(u32, u32, Vec<u8>)>>> = const { RefCell::new(Vec::new()) };
    /// GL functions for reading the frame back and for the passes drawn after Skia.
    static GL: RefCell<Option<Rc<glow::Context>>> = const { RefCell::new(None) };
}

/// Asks for a picture of the next frame (development screenshot, the screen
/// pictures for Home Assistant). The frame is drawn at once if nothing else is.
pub fn read_back_next_frame(sink: impl FnOnce(u32, u32, Vec<u8>) + 'static) {
    READBACK.with(|r| r.borrow_mut().push(Box::new(sink)));
}

/// Whether a picture of the next frame is waiting.
pub fn readback_pending() -> bool {
    READBACK.with(|r| !r.borrow().is_empty())
}

/// The GL context's functions, loaded through EGL (libEGL.so.1 from the bundle).
fn load_gl() -> Option<Rc<glow::Context>> {
    GL.with(|g| {
        if g.borrow().is_none() {
            // SAFETY: dlopen / dlsym on constant C strings; the library stays loaded.
            let get = unsafe {
                let lib = libc::dlopen(c"libEGL.so.1".as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL);
                if lib.is_null() {
                    return None;
                }
                let sym = libc::dlsym(lib, c"eglGetProcAddress".as_ptr());
                if sym.is_null() {
                    return None;
                }
                std::mem::transmute::<*mut libc::c_void, extern "C" fn(*const libc::c_char) -> *const libc::c_void>(sym)
            };
            // SAFETY: eglGetProcAddress returns the current context's functions.
            let gl = unsafe { glow::Context::from_loader_function_cstr(|name| get(name.as_ptr())) };
            *g.borrow_mut() = Some(Rc::new(gl));
        }
        g.borrow().clone()
    })
}

/// Reads the back buffer (after Skia's flush, before the swap) for the waiting sinks.
fn serve_readbacks(width: u32, height: u32) {
    let sinks: Vec<_> = READBACK.with(|r| std::mem::take(&mut *r.borrow_mut()));
    if sinks.is_empty() {
        return;
    }
    let Some(gl) = load_gl() else { return };
    use glow::HasContext;
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    // SAFETY: reads the bound draw framebuffer (the window surface) into a buffer of the right size.
    unsafe {
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
        gl.read_pixels(0, 0, width as i32, height as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut rgba)));
    }
    let (w, h) = (width as usize, height as usize);
    let mut rgb = Vec::with_capacity(w * h * 3);
    // GL rows run bottom up.
    for y in (0..h).rev() {
        for px in rgba[y * w * 4..(y + 1) * w * 4].chunks_exact(4) {
            rgb.extend_from_slice(&px[..3]);
        }
    }
    for sink in sinks {
        sink(width, height, rgb.clone());
    }
}

impl GpuWindow {
    /// Creates the GBM surface on the display's device and the Skia renderer on it.
    pub fn new(display: Rc<RefCell<Display>>) -> Result<Rc<Self>, PlatformError> {
        let (width, height, fd) = {
            let d = display.borrow();
            (d.width, d.height, d.dup_fd().map_err(|e| PlatformError::Other(format!("dup DRM fd: {e}")))?)
        };
        let card = GbmCard(std::fs::File::from(OwnedFd::from(fd)));
        let device: &'static gbm::Device<GbmCard> = Box::leak(Box::new(
            gbm::Device::new(card).map_err(|e| PlatformError::Other(format!("GBM device: {e}")))?,
        ));
        let surface: &'static gbm::Surface<u32> = Box::leak(Box::new(
            device
                .create_surface::<u32>(
                    width,
                    height,
                    gbm::Format::Xrgb8888,
                    gbm::BufferObjectFlags::SCANOUT | gbm::BufferObjectFlags::RENDERING,
                )
                .map_err(|e| PlatformError::Other(format!("GBM surface: {e}")))?,
        ));
        let handles = Arc::new(GbmHandles {
            device: std::ptr::NonNull::new(device.as_raw() as *mut std::ffi::c_void).expect("GBM device"),
            surface: std::ptr::NonNull::new(surface.as_raw() as *mut std::ffi::c_void).expect("GBM surface"),
        });
        let size = PhysicalSize::new(width, height);
        // Mali reports BAD_MATCH for an XRGB GBM surface with an 8-bit alpha config.
        let config = glutin::config::ConfigTemplateBuilder::new().with_transparency(false).with_alpha_size(0);
        let filter = |c: &glutin::config::Config| match c {
            glutin::config::Config::Egl(egl) => egl.native_visual() == gbm::Format::Xrgb8888 as u32,
            #[allow(unreachable_patterns)]
            _ => false,
        };
        let gl_surface = OpenGLSurface::new_with_config(handles.clone(), handles, size, None, config, Some(&filter))?;
        let renderer = SkiaRenderer::new_with_surface(&SkiaSharedContext::default(), Box::new(gl_surface));
        let buffers = Rc::new(Buffers { surface, device, shown: RefCell::new(None), pending: RefCell::new(None) });

        // Before each swap: the previous frame must have reached the screen
        // (at most one frame queued), then anything that reads the frame.
        {
            let display = Rc::downgrade(&display);
            let buffers = Rc::downgrade(&buffers);
            renderer.set_pre_present_callback(Some(Box::new(move || {
                if let (Some(display), Some(buffers)) = (display.upgrade(), buffers.upgrade()) {
                    wait_for_flip(&display, &buffers);
                }
                serve_readbacks(width, height);
            })));
        }

        let win = Rc::new_cyclic(|weak: &Weak<GpuWindow>| GpuWindow {
            window: Window::new(weak.clone() as Weak<dyn WindowAdapter>),
            renderer,
            size,
            needs_redraw: Cell::new(true),
            buffers,
            display,
        });
        win.window.dispatch_event(WindowEvent::Resized { size: size.to_logical(1.0) });
        Ok(win)
    }

    /// Draws and queues the frame for the screen if something changed.
    /// Returns whether a frame was drawn.
    pub fn draw_if_needed(&self) -> Result<bool, PlatformError> {
        let readback = READBACK.with(|r| !r.borrow().is_empty());
        if !self.needs_redraw.take() && !self.window.has_active_animations() && !readback {
            return Ok(false);
        }
        let _ = self.renderer.render()?;
        self.present().map_err(|e| PlatformError::Other(format!("present: {e}")))?;
        Ok(true)
    }

    /// The buffer Skia just swapped in goes to the screen at the next vertical blank.
    fn present(&self) -> std::io::Result<()> {
        // SAFETY: called right after eglSwapBuffers (inside render), as GBM requires.
        let mut bo = unsafe { self.buffers.surface.lock_front_buffer() }.map_err(std::io::Error::other)?;
        let fb = match bo.userdata() {
            Some(&fb) => fb,
            None => {
                let flags = match drm014::buffer::PlanarBuffer::modifier(&bo) {
                    Some(m) if !matches!(m, drm014::buffer::DrmModifier::Invalid) => FbCmd2Flags::MODIFIERS,
                    _ => FbCmd2Flags::empty(),
                };
                let fb: u32 = self.buffers.device.add_planar_framebuffer(&bo, flags)?.into();
                bo.set_userdata(fb);
                fb
            }
        };
        self.display.borrow_mut().flip_raw(fb)?;
        *self.buffers.pending.borrow_mut() = Some(bo);
        Ok(())
    }

    /// A page flip completed (called by the event loop after `Display::handle_events`).
    pub fn flipped(&self) {
        flip_done(&self.display, &self.buffers);
    }
}

/// After a flip: the buffer that was pending is on screen now (if the UI's
/// own buffer was flipped), the one before goes back to GBM.
fn flip_done(display: &Rc<RefCell<Display>>, buffers: &Buffers) {
    let d = display.borrow();
    if d.flip_pending() {
        return;
    }
    if d.gbm_on_screen() {
        if let Some(bo) = buffers.pending.borrow_mut().take() {
            *buffers.shown.borrow_mut() = Some(bo);
        }
    } else {
        // A web buffer is on screen: the UI's buffers are free again.
        buffers.shown.borrow_mut().take();
        buffers.pending.borrow_mut().take();
    }
}

/// Blocks until no page flip is pending (inside rendering, before the swap).
fn wait_for_flip(display: &Rc<RefCell<Display>>, buffers: &Buffers) {
    loop {
        let (pending, fd) = {
            let d = display.borrow();
            (d.flip_pending(), d.fd())
        };
        if !pending {
            break;
        }
        let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
        // SAFETY: poll(2) on one local pollfd.
        let n = unsafe { libc::poll(&mut pfd, 1, 100) };
        if n < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if n == 0 {
            continue;
        }
        let released = display.borrow_mut().handle_events();
        match released {
            Ok(Some(i)) => crate::platform::web_buffer_freed(i),
            Ok(None) => {}
            Err(e) => {
                eprintln!("nspanel-app: DRM events: {e}");
                break;
            }
        }
        flip_done(display, buffers);
    }
}
