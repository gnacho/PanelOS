//! Cameras page: the camera entities picked for it, one live (frames through
//! Home Assistant's camera proxy while the page is on screen) or all as a
//! grid. The blurred background is the frame shrunk to a few pixels: the GPU
//! scales it up smoothly, which looks like a strong blur and costs nothing.

use std::sync::atomic::Ordering;

use slint::{Color, ComponentHandle, ModelRc, Rgb8Pixel, Rgba8Pixel, SharedPixelBuffer, VecModel};

use crate::camera::{CameraControl, FULL, OFF};
use crate::{with_state, AppWindow, CamItem};

thread_local! {
    static CONTROL: std::cell::RefCell<Option<CameraControl>> = const { std::cell::RefCell::new(None) };
}

/// Starts the frame fetcher (once Home Assistant's address and login are known).
fn ensure_started(ui: &AppWindow, s: &crate::UiState) {
    if CONTROL.with(|c| c.borrow().is_some()) {
        return;
    }
    let (Some(url), Some(auth)) = (s.config.as_ref().map(|c| c.ha_url.clone()), s.auth.clone()) else { return };
    let control = CameraControl::default();
    let weak = ui.as_weak();
    crate::camera::spawn(url, auth, control.clone(), move |frame| {
        // RGBA here, off the UI thread: the renderer takes it as it is.
        let small = shrink(&frame, 12);
        let frame = rgba(&frame);
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                show_frame(&ui, frame, small);
            }
        });
    });
    CONTROL.with(|c| *c.borrow_mut() = Some(control));
}

/// A frame for the camera on screen: into its tile, the blur behind it and
/// the corner colours.
fn show_frame(ui: &AppWindow, frame: SharedPixelBuffer<Rgba8Pixel>, small: SharedPixelBuffer<Rgb8Pixel>) {
    let cams = ui.get_cam_items();
    let sel = ui.get_cam_sel().max(0) as usize;
    if let Some(mut c) = slint::Model::row_data(&cams, sel) {
        c.frame = slint::Image::from_rgba8(frame);
        c.state = 0;
        slint::Model::set_row_data(&cams, sel, c);
    }
    let (w, h) = (small.width() as usize, small.height() as usize);
    let px = small.as_slice();
    let at = |x: usize, y: usize| {
        let p = px[y.min(h - 1) * w + x.min(w - 1)];
        Color::from_rgb_u8(p.r, p.g, p.b)
    };
    let corners = vec![at(0, 0), at(w - 1, 0), at(0, h - 1), at(w - 1, h - 1)];
    ui.set_cam_bg_corners(ModelRc::new(VecModel::from(corners)));
    ui.set_cam_bg(slint::Image::from_rgba8(rgba(&small)));
}

fn rgba(src: &SharedPixelBuffer<Rgb8Pixel>) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut out = SharedPixelBuffer::<Rgba8Pixel>::new(src.width(), src.height());
    for (o, p) in out.make_mut_slice().iter_mut().zip(src.as_slice()) {
        *o = Rgba8Pixel { r: p.r, g: p.g, b: p.b, a: 255 };
    }
    out
}

/// The frame scaled down to `n` pixels across (box filter).
fn shrink(frame: &SharedPixelBuffer<Rgb8Pixel>, n: u32) -> SharedPixelBuffer<Rgb8Pixel> {
    let (w, h) = (frame.width(), frame.height());
    let (nw, nh) = (n.min(w).max(1), ((n * h) / w.max(1)).clamp(1, h.max(1)));
    let mut out = SharedPixelBuffer::<Rgb8Pixel>::new(nw, nh);
    let src = frame.as_slice();
    let dst = out.make_mut_slice();
    for oy in 0..nh {
        for ox in 0..nw {
            let (x0, x1) = (ox * w / nw, ((ox + 1) * w / nw).max(ox * w / nw + 1));
            let (y0, y1) = (oy * h / nh, ((oy + 1) * h / nh).max(oy * h / nh + 1));
            let (mut r, mut g, mut b, mut k) = (0u32, 0u32, 0u32, 0u32);
            for y in (y0..y1).step_by(2) {
                for x in (x0..x1).step_by(2) {
                    let p = src[(y * w + x) as usize];
                    r += p.r as u32;
                    g += p.g as u32;
                    b += p.b as u32;
                    k += 1;
                }
            }
            let k = k.max(1);
            dst[(oy * nw + ox) as usize] = Rgb8Pixel { r: (r / k) as u8, g: (g / k) as u8, b: (b / k) as u8 };
        }
    }
    out
}

/// The cameras page from the states; frames only while it is on screen.
pub fn refresh(ui: &AppWindow, s: &mut crate::UiState) {
    let pages = ui.get_pages();
    let index = (0..slint::Model::row_count(&pages)).find(|&i| slint::Model::row_data(&pages, i).is_some_and(|p| p.kind == "camera"));
    let entities = crate::page_entities(s, "camera");
    let shown = index.is_some_and(|i| i as i32 == ui.get_page()) && !ui.get_screensaver_on();
    let sel = (ui.get_cam_sel().max(0) as usize).min(entities.len().saturating_sub(1));
    CONTROL.with(|c| {
        if let Some(c) = c.borrow().as_ref() {
            *c.entity.lock().unwrap() = entities.get(sel).cloned();
            c.mode.store(if shown && ui.get_cam_view() == 0 { FULL } else { OFF }, Ordering::SeqCst);
        }
    });
    if index.is_none() || entities.is_empty() {
        return;
    }
    ensure_started(ui, s);
    let cams = ui.get_cam_items();
    let rows: Vec<CamItem> = entities
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let st = s.states.get(id);
            let state = match st.map(|e| e.state.as_str()).unwrap_or("unavailable") {
                "unavailable" | "unknown" => 1,
                _ => 0,
            };
            let old = slint::Model::row_data(&cams, i);
            CamItem {
                name: st
                    .and_then(|e| e.attrs.get("friendly_name"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(id)
                    .into(),
                frame: old.map(|o| o.frame).unwrap_or_default(),
                state,
            }
        })
        .collect();
    if let Some(m) = crate::pages::update_rows(&cams, rows) {
        ui.set_cam_items(m);
    }
    ui.set_cam_sel(sel as i32);
}

/// Taps on the page (`cam.*`).
pub fn act(ui: &AppWindow, what: &str, value: &str) {
    let n: i32 = value.parse().unwrap_or(0);
    match what {
        "cam.view" => ui.set_cam_view(n),
        "cam.pick" => {
            ui.set_cam_sel(n);
            ui.set_cam_view(0);
        }
        _ => eprintln!("nspanel-app: act {what} {value} (not handled)"),
    }
    with_state(|s| refresh(ui, s));
}
