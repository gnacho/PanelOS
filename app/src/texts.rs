//! The UI's word lists (titles, hints, empty-state lines, …) live in
//! `text.pak`: a header and a masked JSON object {place: [lines]}. A place
//! with several lines shows one of them, a different one each time it opens.

use std::collections::HashMap;
use std::sync::OnceLock;

const PACK: &[u8] = include_bytes!("text.pak");

fn unmask(data: &[u8]) -> Vec<u8> {
    let mut s: u32 = 0x5EED_1234;
    data.iter()
        .map(|b| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            b ^ (s >> 24) as u8
        })
        .collect()
}

fn all() -> &'static HashMap<String, Vec<String>> {
    static TEXTS: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
    TEXTS.get_or_init(|| {
        let Some(body) = PACK.strip_prefix(b"PNLT\x01") else {
            eprintln!("nspanel-app: text pack has an unknown header");
            return HashMap::new();
        };
        serde_json::from_slice(&unmask(body)).unwrap_or_else(|e| {
            eprintln!("nspanel-app: text pack: {e}");
            HashMap::new()
        })
    })
}

/// Every line of a place (empty if the place is unknown).
pub fn list(place: &str) -> &'static [String] {
    all().get(place).map(Vec::as_slice).unwrap_or(&[])
}

/// Line `i` of a place (for places whose lines have fixed roles), or "".
pub fn nth(place: &str, i: usize) -> String {
    list(place).get(i).cloned().unwrap_or_default()
}

/// One line of a place, picked at random but never one of `avoid` while
/// others are left; returns its index too.
pub fn pick(place: &str, avoid: &[usize]) -> (usize, String) {
    let lines = list(place);
    if lines.is_empty() {
        return (0, String::new());
    }
    let free: Vec<usize> = (0..lines.len()).filter(|i| !avoid.contains(i)).collect();
    let choices = if free.is_empty() { (0..lines.len()).collect() } else { free };
    let i = choices[random() as usize % choices.len()];
    (i, lines[i].clone())
}

/// A line of a place that was not shown since the place last ran out of
/// unseen lines (UI thread).
pub fn fresh(place: &str) -> String {
    use std::cell::RefCell;
    thread_local!(static USED: RefCell<HashMap<String, Vec<usize>>> = RefCell::new(HashMap::new()));
    USED.with(|u| {
        let mut u = u.borrow_mut();
        let used = u.entry(place.to_string()).or_default();
        if used.len() >= list(place).len() {
            used.clear();
        }
        let (i, line) = pick(place, used);
        used.push(i);
        line
    })
}

fn random() -> u64 {
    use std::cell::Cell;
    thread_local!(static SEED: Cell<u64> = Cell::new(
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1
    ));
    SEED.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        x
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn pack_reads() {
        assert!(!super::list("set.title").is_empty());
        assert!(super::list("load.lines").len() >= 14);
    }
}
