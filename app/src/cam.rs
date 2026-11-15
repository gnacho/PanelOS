//! Cameras page: the camera entities picked for it, one live (frames through
//! Home Assistant's camera proxy while the page is on screen) or all as a
//! grid. The blurred background is the frame shrunk to a few pixels: the GPU
//! scales it up smoothly, which looks like a strong blur and costs nothing.

use std::sync::atomic::Ordering;

use slint::{Color, ComponentHandle, ModelRc, Rgb8Pixel, Rgba8Pixel, SharedPixelBuffer, VecModel};

use crate::camera::{CameraControl, FULL, LIST, OFF};
use crate::{with_state, AppWindow, CamItem};

thread_local! {
    static CONTROL: std::cell::RefCell<Option<CameraControl>> = const { std::cell::RefCell::new(None) };
    /// All cameras: they take turns in the fetcher, one frame each.
    static GRID: slint::Timer = slint::Timer::default();
}

/// Starts the frame fetcher (once Home Assistant's address and login are known).
fn ensure_started(ui: &AppWindow, s: &crate::UiState) {
    if CONTROL.with(|c| c.borrow().is_some()) {
        return;
    }
    let (Some(url), Some(auth)) = (s.config.as_ref().map(|c| c.ha_url.clone()), s.auth.clone()) else { return };
    let control = CameraControl::default();
    let weak = ui.as_weak();
    let fetching = control.entity.clone();
    crate::camera::spawn(url, auth, control.clone(), move |frame| {
        // RGBA here, off the UI thread: the renderer takes it as it is.
        let small = shrink(&frame, 12);
        let frame = rgba(&frame);
        let entity = fetching.lock().unwrap().clone().unwrap_or_default();
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                show_frame(&ui, &entity, frame, small);
            }
        });
    });
    CONTROL.with(|c| *c.borrow_mut() = Some(control));
}

/// A frame for the camera on screen: into its tile, the blur behind it and
/// the corner colours.
fn show_frame(ui: &AppWindow, entity: &str, frame: SharedPixelBuffer<Rgba8Pixel>, small: SharedPixelBuffer<Rgb8Pixel>) {
    let cams = ui.get_cam_items();
    let sel = ui.get_cam_sel().max(0) as usize;
    let index = with_state(|s| crate::page_entities(s, "camera").iter().position(|e| e == entity)).unwrap_or(sel);
    if let Some(mut c) = slint::Model::row_data(&cams, index) {
        c.frame = slint::Image::from_rgba8(frame);
        c.state = 0;
        slint::Model::set_row_data(&cams, index, c);
    }
    // the blurred copy belongs to the camera shown big (and the visitor alert's picture)
    if index != sel {
        return;
    }
    if ui.get_cam_ring() {
        if let Some(c) = slint::Model::row_data(&cams, index) {
            ui.set_cam_ring_frame(c.frame);
        }
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
    if ui.get_cam_ring() {
        ui.set_cam_ring_bg(ui.get_cam_bg());
    }
}

pub fn rgba(src: &SharedPixelBuffer<Rgb8Pixel>) -> SharedPixelBuffer<Rgba8Pixel> {
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
    let grid = shown && ui.get_cam_view() == 1 && entities.len() > 1;
    CONTROL.with(|c| {
        if let Some(c) = c.borrow().as_ref() {
            if !grid {
                *c.entity.lock().unwrap() = entities.get(sel).cloned();
            }
            c.mode.store(if !shown { OFF } else if grid { LIST } else { FULL }, Ordering::SeqCst);
        }
    });
    GRID.with(|t| {
        if !grid {
            t.stop();
        } else if !t.running() {
            let list = entities.clone();
            let turn = std::cell::Cell::new(0usize);
            t.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(1100), move || {
                let next = turn.get() % list.len();
                turn.set(next + 1);
                CONTROL.with(|c| {
                    if let Some(c) = c.borrow().as_ref() {
                        *c.entity.lock().unwrap() = Some(list[next].clone());
                    }
                });
            });
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
    // the picture: whole (letterboxed) or filling the screen; the start view from the page's look
    let fit = crate::page_option(s, "camera", "fit").and_then(|v| v.as_str().map(|f| f == "whole")).unwrap_or(false);
    ui.set_cam_fit(fit as i32);
    // the start view ("one" / "all"), taken when it changes in the integration (the chip switches it on the panel)
    thread_local!(static VIEW: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) });
    let view = crate::page_option(s, "camera", "view").and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
    if VIEW.with(|v| v.replace(view.clone())) != view && !view.is_empty() {
        ui.set_cam_view(if view == "all" && entities.len() > 1 { 1 } else { 0 });
    }
    doorbell(ui, s);
}

thread_local! {
    /// The visitor alert: the doorbell's last state, when it rang.
    static RING_STATE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    static RING_AT: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
}

/// How long the visitor alert stays when nobody answers.
const RING_SECONDS: u64 = 60;

/// The doorbell (the camera page's `ring` option, a binary sensor): when it turns on, the panel wakes
/// on the camera page with the picture big, a chime (if do not disturb lets it through), and the door
/// lock's "hold to open" when the page has a `lock`. Called with the states.
fn doorbell(ui: &AppWindow, s: &crate::UiState) {
    let Some(entity) = crate::page_option(s, "camera", "ring").and_then(|v| v.as_str().map(str::to_string)) else { return };
    let state = s.states.get(&entity).map(|e| e.state.clone()).unwrap_or_default();
    let before = RING_STATE.with(|r| r.replace(state.clone()));
    let rang = state == "on" && before != "on" && !before.is_empty();
    // over by itself after a minute
    if RING_AT.with(|r| r.get()).is_some_and(|t| t.elapsed().as_secs() >= RING_SECONDS) {
        RING_AT.with(|r| r.set(None));
        ui.set_cam_ring(false);
    }
    if !rang {
        return;
    }
    RING_AT.with(|r| r.set(Some(std::time::Instant::now())));
    let entities = crate::page_entities(s, "camera");
    let sel = (ui.get_cam_sel().max(0) as usize).min(entities.len().saturating_sub(1));
    let name = entities
        .get(sel)
        .and_then(|id| s.states.get(id))
        .and_then(|e| e.attrs.get("friendly_name"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Camera")
        .to_string();
    ui.set_cam_ring_title("Someone is at the door".into());
    ui.set_cam_ring_sub(format!("{name} · {}", crate::pages::local_format(crate::pages::now(), "%H:%M")).into());
    ui.set_cam_can_open(crate::page_option(s, "camera", "lock").and_then(|v| v.as_str().map(|l| !l.is_empty())).unwrap_or(false));
    ui.set_cam_can_talk(false);
    ui.set_cam_view(0);
    ui.set_cam_ring(true);
    if crate::dnd_allows(s, "ring") {
        s.player.play(crate::voice::Sound::Chime);
    }
    // the camera page (a temporary one when the list has none), after this state update
    let weak = ui.as_weak();
    slint::Timer::single_shot(std::time::Duration::ZERO, move || {
        let Some(ui) = weak.upgrade() else { return };
        crate::wake(&ui);
        if !crate::go_to_page(&ui, "camera") {
            crate::open_temporary_page(&ui, "camera");
        }
        with_state(|s| refresh(&ui, s));
    });
}

/// Taps on the page (`cam.*`).
pub fn act(ui: &AppWindow, what: &str, value: &str) {
    let n: i32 = value.parse().unwrap_or(0);
    match what {
        "cam.ring-close" => {
            RING_AT.with(|r| r.set(None));
            ui.set_cam_ring(false);
        }
        "cam.open" => {
            let lock = with_state(|s| crate::page_option(s, "camera", "lock").and_then(|v| v.as_str().map(str::to_string)));
            if let Some(lock) = lock.filter(|l| !l.is_empty()) {
                let domain = lock.split('.').next().unwrap_or("lock");
                let service = if domain == "lock" { "unlock" } else { "open_cover" };
                crate::call(domain, service, &lock, serde_json::json!({}));
            }
            RING_AT.with(|r| r.set(None));
            ui.set_cam_ring(false);
        }
        "cam.view" => ui.set_cam_view(n),
        "cam.pick" => {
            ui.set_cam_sel(n);
            ui.set_cam_view(0);
        }
        _ => eprintln!("nspanel-app: act {what} {value} (not handled)"),
    }
    with_state(|s| refresh(ui, s));
}
