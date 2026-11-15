//! Notifications page: Home Assistant's persistent notifications (followed
//! live), unread and newest first; a tap opens one (and marks it read), its
//! card dismisses it, "Clear all" dismisses them all. The fridge look's notes
//! are drawn by the app; the pager's buttons scroll and open.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::draw::{self, Job};
use crate::pages::{now, parse_iso};
use crate::{icons, texts, with_state, AppWindow, InfoNote};

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    title: String,
    message: String,
    created: i64,
    unread: bool,
}

thread_local! {
    static NOTES: std::cell::RefCell<HashMap<String, Note>> = std::cell::RefCell::new(HashMap::new());
    /// The note open (its id), the pager's first list row.
    static OPEN: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    static FIRST: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static FRIDGE_EMPTY: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// The buttons of an open note: dismiss it in Home Assistant, or just close it.
const ACTIONS: [&str; 2] = ["Dismiss", "OK"];

/// A change of Home Assistant's notifications.
pub fn update(kind: &str, notes: &Value) {
    let Some(map) = notes.as_object() else { return };
    NOTES.with(|n| {
        let mut n = n.borrow_mut();
        if kind == "current" {
            n.clear();
        }
        for (id, v) in map {
            if kind == "removed" {
                n.remove(id);
                continue;
            }
            let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            let unread = n.get(id).map(|old| old.unread).unwrap_or(true);
            n.insert(
                id.clone(),
                Note { title: text("title"), message: text("message"), created: parse_iso(&text("created_at")).unwrap_or(now()), unread },
            );
        }
    });
}

fn ago(t: i64) -> String {
    let d = (now() - t).max(0);
    match d {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", d / 60),
        3600..=86399 => format!("{} h ago", d / 3600),
        _ => crate::pages::local_format(t, "%b %-d"),
    }
}

fn ordered() -> Vec<(String, Note)> {
    let mut list: Vec<(String, Note)> = NOTES.with(|n| n.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect());
    list.sort_by(|a, b| b.1.unread.cmp(&a.1.unread).then(b.1.created.cmp(&a.1.created)));
    list
}

fn note_row(n: &Note, look: i32, big: bool) -> InfoNote {
    let bell = icons::glyph("bell-outline").map(String::from).unwrap_or_default();
    let title = if n.title.is_empty() { "Home Assistant".to_string() } else { n.title.clone() };
    let when = ago(n.created);
    // the fridge look: the note drawn by the app (seeded by its text, so it keeps its colour and tilt)
    let paper = if look == 1 {
        draw::get(Job::Fridge {
            title: title.clone(),
            message: n.message.clone(),
            foot: format!("Home Assistant · {when}"),
            level: 0,
            seed: draw::seed(&format!("{title}\n{}", n.message)),
            big,
        })
    } else {
        Default::default()
    };
    InfoNote {
        title: title.into(),
        message: n.message.clone().into(),
        icon: bell.as_str().into(),
        level: 0,
        source: "Home Assistant".into(),
        when: when.into(),
        image: Default::default(),
        has_image: false,
        actions: slint::ModelRc::new(slint::VecModel::from(ACTIONS.iter().map(|a| slint::SharedString::from(*a)).collect::<Vec<_>>())),
        unread: n.unread,
        paper,
    }
}

/// The notifications page (only when the panel has one).
pub fn refresh(ui: &AppWindow, _s: &mut crate::UiState) {
    let pages = ui.get_pages();
    let Some(slot) = (0..slint::Model::row_count(&pages)).filter_map(|i| slint::Model::row_data(&pages, i)).find(|p| p.kind == "info")
    else {
        return;
    };
    // the fridge needs the notes drawn by the app (GPU build); otherwise the plain look
    let look = if !draw::AVAILABLE && slot.variant == 1 { 0 } else { slot.variant };
    ui.set_info_look(look);
    let list = ordered();
    // the fridge's notes when a page on the list has that look
    let look = if draw::AVAILABLE && crate::page_looks(ui, "info").contains(&1) { 1 } else { look };
    let rows: Vec<InfoNote> = list.iter().map(|(_, n)| note_row(n, look, false)).collect();
    // the open note (gone when Home Assistant dismissed it)
    let open = OPEN.with(|o| o.borrow().clone()).and_then(|id| list.iter().position(|(k, _)| *k == id));
    match open {
        Some(i) => {
            ui.set_info_card_note(note_row(&list[i].1, look, true));
            ui.set_info_card_index(i as i32 + 1);
            ui.set_info_card_count(list.len() as i32);
            ui.set_info_card(true);
        }
        None => {
            OPEN.with(|o| *o.borrow_mut() = None);
            ui.set_info_card(false);
        }
    }
    let first = FIRST.with(|f| f.get()).min(list.len().saturating_sub(6));
    FIRST.with(|f| f.set(first));
    ui.set_info_pager_first(first as i32);
    if look == 1 {
        let title = FRIDGE_EMPTY.with(|e| {
            if e.borrow().is_empty() {
                *e.borrow_mut() = texts::fresh("info.fridge.empty");
            }
            e.borrow().clone()
        });
        ui.set_info_fridge_empty(draw::get(Job::Fridge {
            title,
            message: "Nothing new on the fridge.".into(),
            foot: "the fridge · now".into(),
            level: 0,
            seed: 4242,
            big: false,
        }));
    }
    // the pager's and the news look's clock and lines; the pager's body and the fridge door
    ui.set_info_clock(crate::pages::local_format(now(), "%H:%M").into());
    if slot.variant == 2 && ui.get_info_pager_body().size().width == 0 {
        let finish = crate::page_option(_s, "info", "finish").and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "graphite".into());
        ui.set_info_pager_body(crate::quick::picture(&format!("info/pager-{finish}.png")));
        ui.set_info_pager_led(crate::quick::picture("info/pager-led.png"));
    }
    if slot.variant == 1 {
        let door = if slint::ComponentHandle::global::<crate::Theme>(ui).get_dark() { "info/door-dark.png" } else { "info/door-light.png" };
        ui.set_info_fridge_door(crate::quick::picture(door));
    }
    if ui.get_info_news_ticker().is_empty() {
        ui.set_info_news_ticker(texts::fresh("info.news.tickers").into());
        ui.set_info_news_empty(texts::fresh("info.news.empty").into());
        ui.set_info_pager_empty(texts::fresh("info.pager.empty").into());
    }
    if rows.is_empty() && ui.get_info_empty_title().is_empty() {
        ui.set_info_empty_title(texts::fresh("info.empty.titles").into());
        ui.set_info_empty_sub(texts::fresh("info.empty.subs").into());
    }
    if let Some(m) = crate::pages::update_rows(&ui.get_info_notes(), rows) {
        ui.set_info_notes(m);
    }
}

fn open_note(id: Option<String>) {
    if let Some(id) = &id {
        NOTES.with(|m| {
            if let Some(note) = m.borrow_mut().get_mut(id) {
                note.unread = false;
            }
        });
    }
    OPEN.with(|o| *o.borrow_mut() = id);
}

/// Taps on the page (`info.*`).
pub fn act(ui: &AppWindow, what: &str, value: &str) {
    let n: usize = value.parse().unwrap_or(0);
    let list = ordered();
    let open = OPEN.with(|o| o.borrow().clone()).and_then(|id| list.iter().position(|(k, _)| *k == id));
    match what {
        "info.read" => open_note(list.get(n).map(|(id, _)| id.clone())),
        "info.close" => open_note(None),
        "info.action" => {
            if let (Some(i), 0) = (open, n) {
                crate::call("persistent_notification", "dismiss", "", json!({ "notification_id": list[i].0 }));
            }
            open_note(None);
        }
        // the pager: in the list the arrows scroll and OK opens the top row; in a message they page and OK goes back
        "info.up" | "info.down" => match open {
            Some(i) => {
                let j = if what == "info.up" { i.saturating_sub(1) } else { (i + 1).min(list.len().saturating_sub(1)) };
                open_note(list.get(j).map(|(id, _)| id.clone()));
            }
            None => FIRST.with(|f| f.set(if what == "info.up" { f.get().saturating_sub(1) } else { f.get() + 1 })),
        },
        "info.ok" => match open {
            Some(_) => open_note(None),
            None => open_note(list.get(FIRST.with(|f| f.get())).map(|(id, _)| id.clone())),
        },
        "info.clear" => {
            for (id, _) in list {
                crate::call("persistent_notification", "dismiss", "", json!({ "notification_id": id }));
            }
        }
        _ => eprintln!("nspanel-app: act {what} {value} (not handled)"),
    }
    with_state(|s| refresh(ui, s));
}
