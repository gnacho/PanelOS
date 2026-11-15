//! Notifications page: Home Assistant's persistent notifications (followed
//! live), newest first; a tap marks one read, "Clear all" dismisses them.

use std::collections::HashMap;

use serde_json::{json, Value};

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
}

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

/// The notifications page (only when the panel has one).
pub fn refresh(ui: &AppWindow, _s: &mut crate::UiState) {
    let pages = ui.get_pages();
    let Some(slot) = (0..slint::Model::row_count(&pages)).filter_map(|i| slint::Model::row_data(&pages, i)).find(|p| p.kind == "info")
    else {
        return;
    };
    ui.set_info_look(slot.variant);
    let bell = icons::glyph("bell-outline").map(String::from).unwrap_or_default();
    let rows: Vec<InfoNote> = ordered()
        .into_iter()
        .map(|(_, n)| InfoNote {
            title: if n.title.is_empty() { "Home Assistant".into() } else { n.title.into() },
            message: n.message.into(),
            icon: bell.as_str().into(),
            level: 0,
            source: "Home Assistant".into(),
            when: ago(n.created).into(),
            image: Default::default(),
            has_image: false,
            actions: Default::default(),
            unread: n.unread,
            paper: Default::default(),
        })
        .collect();
    if rows.is_empty() && ui.get_info_empty_title().is_empty() {
        ui.set_info_empty_title(texts::fresh("info.empty.titles").into());
        ui.set_info_empty_sub(texts::fresh("info.empty.subs").into());
    }
    if let Some(m) = crate::pages::update_rows(&ui.get_info_notes(), rows) {
        ui.set_info_notes(m);
    }
}

/// Taps on the page (`info.*`).
pub fn act(ui: &AppWindow, what: &str, value: &str) {
    let n: usize = value.parse().unwrap_or(0);
    match what {
        "info.read" => {
            if let Some((id, _)) = ordered().get(n) {
                NOTES.with(|m| {
                    if let Some(note) = m.borrow_mut().get_mut(id) {
                        note.unread = false;
                    }
                });
            }
        }
        "info.clear" => {
            for (id, _) in ordered() {
                crate::call("persistent_notification", "dismiss", "", json!({ "notification_id": id }));
            }
        }
        _ => eprintln!("nspanel-app: act {what} {value} (not handled)"),
    }
    with_state(|s| refresh(ui, s));
}
