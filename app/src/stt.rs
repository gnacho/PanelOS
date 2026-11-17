//! Speech recognition of a short piece of audio through Home Assistant (`POST /api/stt/<engine>`), outside any
//! conversation: what someone says while a timer rings is only checked for a stop word, never answered.
//! The engine and its language are the panel's own assistant's (the integration sends them as `stt`).

use std::sync::Mutex;
use std::time::Duration;

/// The panel's speech recognition: (engine entity, language), from the integration's settings.
static ENGINE: Mutex<Option<(String, String)>> = Mutex::new(None);

pub fn set_engine(engine: &str, language: &str) {
    let want = (!engine.is_empty()).then(|| (engine.to_string(), language.to_string()));
    let mut e = ENGINE.lock().unwrap();
    if *e != want {
        if let Some((engine, language)) = &want {
            eprintln!("nspanel-app: speech recognition for stop words: {engine} ({language})");
        }
        *e = want;
    }
}

pub fn engine() -> Option<(String, String)> {
    ENGINE.lock().unwrap().clone()
}

/// The text of 16 kHz mono samples (blocking; call it off the audio thread).
pub fn recognize(samples: &[i16]) -> Result<String, String> {
    let (engine, language) = engine().ok_or("no speech recognition known for the panel")?;
    let (ha_url, auth) = crate::sounds::ha().ok_or("no Home Assistant")?;
    let token = auth.access_token().map_err(|_| "no login".to_string())?;
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let resp = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(8))
        .build()
        .post(&format!("{ha_url}/api/stt/{engine}"))
        .set("Authorization", &format!("Bearer {token}"))
        .set("X-Speech-Content", &format!("format=wav; codec=pcm; sample_rate=16000; bit_rate=16; channel=1; language={language}"))
        .send_bytes(&bytes)
        .map_err(|e| e.to_string())?;
    let body = resp.into_string().map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    if v["result"] != "success" {
        return Ok(String::new());
    }
    Ok(v["text"].as_str().unwrap_or("").to_string())
}

/// A short phrase that means "stop" (Hungarian and English): at most four words, one of them a stop word, or a
/// two-word stop phrase. Anything else (another speaker's answer, a sentence to someone) is not.
pub fn is_stop(text: &str) -> bool {
    let clean: String = text.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    let words: Vec<&str> = clean.split_whitespace().collect();
    if words.is_empty() || words.len() > 4 {
        return false;
    }
    const WORDS: &[&str] = &[
        "állj", "álljál", "álljon", "stop", "stopp", "sztop", "elég", "csend", "csendet", "hallgass", "kuss", "enough", "quiet", "silence", "cancel",
    ];
    const PAIRS: &[(&str, &str)] = &[
        ("kapcsold", "ki"),
        ("állítsd", "le"),
        ("állítsd", "meg"),
        ("hagyd", "abba"),
        ("turn", "off"),
        ("shut", "up"),
    ];
    words.iter().any(|w| WORDS.contains(w)) || words.windows(2).any(|p| PAIRS.contains(&(p[0], p[1])))
}

#[cfg(test)]
mod tests {
    use super::is_stop;

    #[test]
    fn stop_words() {
        assert!(is_stop("Állj."));
        assert!(is_stop("Állj, állj!"));
        assert!(is_stop("Stop."));
        assert!(is_stop("Kapcsold ki!"));
        assert!(is_stop("Elég már."));
        assert!(!is_stop("Beállítottam egy egyperces időzítőt."));
        assert!(!is_stop("Köszönöm szépen."));
        assert!(!is_stop(""));
        assert!(!is_stop("Set a one minute timer on the panel and then stop."));
    }
}
