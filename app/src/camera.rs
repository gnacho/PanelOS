//! Camera view: fetches JPEG snapshots through Home Assistant's camera proxy
//! (resized by Home Assistant to the panel's size) while the camera is on
//! screen: about one frame per second in the list, as fast as possible in
//! full screen, nothing while it is scrolled away.

use std::io::Read;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::auth::{Auth, TokenError};

pub const OFF: u8 = 0;
pub const LIST: u8 = 1;
pub const FULL: u8 = 2;

#[derive(Clone, Default)]
pub struct CameraControl {
    pub entity: Arc<Mutex<Option<String>>>,
    pub mode: Arc<AtomicU8>,
}

pub fn decode(jpeg: &[u8]) -> Result<slint::SharedPixelBuffer<slint::Rgb8Pixel>, String> {
    let mut dec = jpeg_decoder::Decoder::new(jpeg);
    let pixels = dec.decode().map_err(|e| e.to_string())?;
    let info = dec.info().ok_or("no image info")?;
    let (w, h) = (info.width as u32, info.height as u32);
    let mut buf = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(w, h);
    let out = buf.make_mut_bytes();
    match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => out.copy_from_slice(&pixels[..out.len()]),
        jpeg_decoder::PixelFormat::L8 => {
            for (i, g) in pixels.iter().enumerate().take((w * h) as usize) {
                out[i * 3..i * 3 + 3].copy_from_slice(&[*g, *g, *g]);
            }
        }
        other => return Err(format!("unsupported JPEG pixel format {other:?}")),
    }
    Ok(buf)
}

pub fn spawn(
    ha_url: String,
    auth: Arc<Auth>,
    control: CameraControl,
    deliver: impl Fn(slint::SharedPixelBuffer<slint::Rgb8Pixel>) + Send + 'static,
) {
    std::thread::spawn(move || loop {
        let mode = control.mode.load(Ordering::SeqCst);
        let entity = control.entity.lock().unwrap().clone();
        let Some(entity) = entity.filter(|_| mode != OFF) else {
            std::thread::sleep(Duration::from_millis(300));
            continue;
        };
        let started = Instant::now();
        let (w, h) = if mode == FULL { (480, 270) } else { (448, 252) };
        let result = (|| -> Result<_, String> {
            let token = match auth.access_token() {
                Ok(t) => t,
                Err(TokenError::NotPaired) => return Err("not paired".into()),
                Err(TokenError::Retry(e)) => return Err(e),
            };
            let url = format!("{ha_url}/api/camera_proxy/{entity}?width={w}&height={h}");
            let resp = ureq::get(&url)
                .set("Authorization", &format!("Bearer {token}"))
                .timeout(Duration::from_secs(10))
                .call()
                .map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            resp.into_reader().take(8 * 1024 * 1024).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            decode(&bytes)
        })();
        match result {
            Ok(frame) => deliver(frame),
            Err(e) => {
                eprintln!("nspanel-app: camera {entity}: {e}");
                std::thread::sleep(Duration::from_secs(3));
            }
        }
        let period = if mode == FULL { Duration::from_millis(100) } else { Duration::from_millis(1000) };
        if let Some(rest) = period.checked_sub(started.elapsed()) {
            std::thread::sleep(rest);
        }
    });
}
