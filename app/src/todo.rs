//! To-do page: the to-do lists picked for it, the chosen list's items (read
//! with `todo.get_items` over Home Assistant's REST API in a thread: every
//! minute, when its state changes and right after a tick), ticked here with
//! `todo.update_item`.

use std::io::Read;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use slint::ComponentHandle;

use crate::pages::{local_format, now, parse_iso};
use crate::{icons, texts, with_state, AppWindow, TodoItem, TodoList};

#[derive(Clone, Debug, PartialEq)]
struct Item {
    uid: String,
    summary: String,
    done: bool,
    due: String,
    note: String,
}

thread_local! {
    static ITEMS: std::cell::RefCell<Vec<Item>> = const { std::cell::RefCell::new(Vec::new()) };
}
/// The list to read, and a request to read it now.
static WANTED: Mutex<(String, bool)> = Mutex::new((String::new(), false));
static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn ensure_started(ui: &AppWindow, s: &crate::UiState) {
    if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let (Some(url), Some(auth)) = (s.config.as_ref().map(|c| c.ha_url.clone()), s.auth.clone()) else {
        STARTED.store(false, std::sync::atomic::Ordering::SeqCst);
        return;
    };
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let mut last = String::new();
        let mut next = Instant::now();
        loop {
            let (list, now_please) = {
                let mut w = WANTED.lock().unwrap();
                let r = w.clone();
                w.1 = false;
                r
            };
            if !list.is_empty() && (list != last || now_please || Instant::now() >= next) {
                last = list.clone();
                next = Instant::now() + Duration::from_secs(60);
                if let Some(items) = fetch(&url, &auth, &list) {
                    let weak = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        ITEMS.with(|i| *i.borrow_mut() = items);
                        if let Some(ui) = weak.upgrade() {
                            with_state(|s| refresh(&ui, s));
                        }
                    });
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
}

fn fetch(url: &str, auth: &std::sync::Arc<crate::auth::Auth>, list: &str) -> Option<Vec<Item>> {
    let token = auth.access_token().ok()?;
    let resp = ureq::post(&format!("{url}/api/services/todo/get_items?return_response"))
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(10))
        .send_string(&json!({ "entity_id": list }).to_string());
    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            eprintln!("nspanel-app: to-do {list}: {e}");
            return None;
        }
    };
    let mut body = String::new();
    resp.into_reader().take(2 * 1024 * 1024).read_to_string(&mut body).ok()?;
    let v: Value = serde_json::from_str(&body).ok()?;
    let items = v.get("service_response")?.get(list)?.get("items")?.as_array()?;
    Some(
        items
            .iter()
            .map(|i| {
                let text = |k: &str| i.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                Item {
                    uid: text("uid"),
                    summary: text("summary"),
                    done: text("status") == "completed",
                    due: text("due"),
                    note: text("description").lines().next().unwrap_or("").to_string(),
                }
            })
            .collect(),
    )
}

/// The due date as the list shows it, and how urgent it is (0 later, 1 today, 2 overdue).
fn due(d: &str) -> (String, i32) {
    if d.is_empty() {
        return (String::new(), 0);
    }
    let t = now();
    let today = local_format(t, "%Y-%m-%d");
    let tomorrow = local_format(t + 86400, "%Y-%m-%d");
    let (date, time) = match parse_iso(d) {
        Some(e) if d.len() > 10 => (local_format(e, "%Y-%m-%d"), Some(local_format(e, "%H:%M"))),
        _ => (d.get(..10).unwrap_or(d).to_string(), None),
    };
    let state = if date < today { 2 } else if date == today { 1 } else { 0 };
    let day = if date == today {
        "Today".to_string()
    } else if date == tomorrow {
        "Tomorrow".into()
    } else {
        // "Nov 20" for other days (the local date of the due day).
        let e = parse_iso(&format!("{date}T12:00:00")).unwrap_or(t);
        local_format(e, "%b %-d")
    };
    (time.map(|t| format!("{day} {t}")).unwrap_or(day), state)
}

fn row(i: &Item) -> TodoItem {
    let (due, due_state) = due(&i.due);
    TodoItem {
        text: i.summary.clone().into(),
        note: i.note.clone().into(),
        due: due.into(),
        due_state,
        done: i.done,
        tape: Default::default(),
        hand: Default::default(),
    }
}

/// The to-do page (only when the panel has one).
pub fn refresh(ui: &AppWindow, s: &mut crate::UiState) {
    let pages = ui.get_pages();
    let Some(slot) = (0..slint::Model::row_count(&pages)).filter_map(|i| slint::Model::row_data(&pages, i)).find(|p| p.kind == "todo")
    else {
        return;
    };
    let entities = crate::page_entities(s, "todo");
    if entities.is_empty() {
        return;
    }
    ui.set_todo_look(slot.variant);
    let sel = (ui.get_todo_sel().max(0) as usize).min(entities.len() - 1);
    {
        let mut w = WANTED.lock().unwrap();
        // A changed count of open items in HA: read the list again.
        let open: String = s.states.get(&entities[sel]).map(|e| e.state.clone()).unwrap_or_default();
        thread_local!(static OPEN: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) });
        if OPEN.with(|o| o.replace(open.clone())) != open {
            w.1 = true;
        }
        w.0 = entities[sel].clone();
    }
    ensure_started(ui, s);
    let lists: Vec<TodoList> = entities
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let st = s.states.get(id);
            TodoList {
                name: st.and_then(|e| e.attrs.get("friendly_name")).and_then(Value::as_str).unwrap_or(id).into(),
                icon: icons::glyph(if id.contains("shopping") { "cart-outline" } else { "format-list-checks" })
                    .map(String::from)
                    .unwrap_or_default()
                    .into(),
                open: st.and_then(|e| e.state.parse().ok()).unwrap_or(0),
                tint: (i % 4) as i32,
            }
        })
        .collect();
    if let Some(m) = crate::pages::update_rows(&ui.get_todo_lists(), lists) {
        ui.set_todo_lists(m);
    }
    ui.set_todo_sel(sel as i32);
    let items = ITEMS.with(|i| i.borrow().clone());
    let open: Vec<TodoItem> = items.iter().filter(|i| !i.done).map(row).collect();
    let done: Vec<TodoItem> = items.iter().filter(|i| i.done).map(row).collect();
    if open.is_empty() && ui.get_todo_empty_title().is_empty() {
        ui.set_todo_empty_title(texts::fresh("todo.empty.titles").into());
        ui.set_todo_empty_sub(texts::fresh("todo.empty.subs").into());
    }
    if let Some(m) = crate::pages::update_rows(&ui.get_todo_open(), open) {
        ui.set_todo_open(m);
    }
    if let Some(m) = crate::pages::update_rows(&ui.get_todo_done(), done) {
        ui.set_todo_done(m);
    }
}

/// Taps on the page (`todo.*`).
pub fn act(ui: &AppWindow, what: &str, value: &str) {
    let n: usize = value.parse().unwrap_or(0);
    match what {
        "todo.check" | "todo.uncheck" => {
            let list = WANTED.lock().unwrap().0.clone();
            let items = ITEMS.with(|i| i.borrow().clone());
            let pick: Vec<&Item> = items.iter().filter(|i| i.done == (what == "todo.uncheck")).collect();
            let Some(item) = pick.get(n) else { return };
            let status = if what == "todo.check" { "completed" } else { "needs_action" };
            let key = if item.uid.is_empty() { item.summary.clone() } else { item.uid.clone() };
            crate::call("todo", "update_item", &list, json!({ "item": key, "status": status }));
            // Shown ticked at once; the list is read again right after.
            let uid = item.uid.clone();
            ITEMS.with(|i| {
                for it in i.borrow_mut().iter_mut() {
                    if it.uid == uid {
                        it.done = what == "todo.check";
                    }
                }
            });
            WANTED.lock().unwrap().1 = true;
            with_state(|s| refresh(ui, s));
        }
        "todo.done-toggle" => ui.set_todo_show_done(!ui.get_todo_show_done()),
        _ => eprintln!("nspanel-app: act {what} {value} (not handled)"),
    }
}

