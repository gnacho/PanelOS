//! Voice satellite: microphones (PDM, 16 kHz) → on-device wake word →
//! audio to Home Assistant's Assist pipeline over the ESPHome API → spoken
//! answer (TTS audio over the API) on the speaker. Follow-up listening when
//! Home Assistant asks for it (`continue_conversation`), like ESPHome voice
//! satellites. Echo cancellation (WebRTC AEC3, `native/aec.cc`) with the
//! player's own output as the reference, so the wake word also works while
//! the panel talks or beeps (it interrupts the answer).

use std::ffi::CString;
use std::os::raw::{c_char, c_int};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use alsa::pcm::{Access, Format, HwParams, PCM};
use alsa::{Direction, ValueOr};

pub const MODEL_DIR: &str = "/usr/share/nspanel/wakewords";
const CAPTURE_DEVICE: &str = "hw:CARD=pdmmics,DEV=0";
const PLAYBACK_DEVICE: &str = "hw:CARD=rk809,DEV=0";
const MIC_CHANNELS: usize = 4; // PDM data lines 0 and 1; the microphones are channels 2 and 3
const MIC_GAIN: i32 = 8;
const CHUNK: usize = 512; // samples per read (32 ms at 16 kHz)
const AEC_FRAME: usize = 160; // 10 ms at 16 kHz

/// What the speaker plays, as 16 kHz mono, for the echo canceller: the player
/// appends while it writes to ALSA, the microphone thread takes 10 ms per
/// microphone frame (zeros when it is empty).
pub type EchoRef = Arc<Mutex<VecDeque<i16>>>;

/// voice thread → ESPHome API thread
pub enum ToApi {
    /// Level (dBFS), SNR (dB, None without enough background) and model score
    /// (0..1) of a wake word, sent right before its `Start`.
    WakeInfo { level_dbfs: f32, score: f32, snr_db: Option<f32> },
    Start { conversation_id: String, wake_word_phrase: String },
    Audio(Vec<u8>),
    Stop,
    AnnounceFinished { success: bool },
}

/// ESPHome API thread → voice thread
pub enum FromApi {
    Started { error: bool },
    Event { kind: i32, data: Vec<(String, String)> },
    TtsAudio(Vec<u8>),
    SetWakeWords(Vec<String>),
    /// Home Assistant announcement: media URL (absolute or relative to Home Assistant) and its text.
    Announce { media_id: String, text: String },
}

/// voice thread → UI
/// The microphone level while a run streams (0 … 1 for -60 … -20 dBFS, f32
/// bits): the voice view's halo and trace read it.
pub static MIC_LEVEL: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

#[derive(Debug, Clone)]
pub enum VoiceUi {
    Listening,
    Heard(String),
    Thinking,
    Answer(String),
    Idle,
    Error(String),
    /// "Stop" said while the panel spoke or a timer rang.
    Stopped,
}

#[derive(Debug, Clone)]
pub struct WakeWord {
    pub id: String,
    pub phrase: String,
    pub path: String,
    pub cutoff: f32,
    pub window: i32,
}

/// Models that run on the panel but are not offered as wake words: "stop"
/// listens only while the panel speaks or a timer rings, and stops it (as on
/// ESPHome voice satellites).
pub const INTERNAL_WORDS: &[&str] = &["stop"];

/// Wake word models installed on the panel (`<id>.tflite` + `<id>.json`).
pub fn available_wake_words() -> Vec<WakeWord> {
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir(MODEL_DIR) else { return out };
    for entry in dir.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        if id == "vad" {
            continue;
        }
        let model = path.with_extension("tflite");
        if !model.exists() {
            continue;
        }
        let cutoff = v["micro"]["probability_cutoff"].as_f64().unwrap_or(0.97) as f32;
        out.push(WakeWord {
            phrase: v["wake_word"].as_str().unwrap_or(&id).to_string(),
            cutoff,
            window: v["micro"]["sliding_window_size"].as_i64().unwrap_or(5) as i32,
            path: model.to_string_lossy().into_owned(),
            id,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

#[repr(C)]
struct MwwEngine {
    _private: [u8; 0],
}

extern "C" {
    fn mww_create() -> *mut MwwEngine;
    fn mww_add_model(e: *mut MwwEngine, path: *const c_char, cutoff: f32, window: c_int) -> c_int;
    fn mww_set_enabled(e: *mut MwwEngine, index: c_int, enabled: c_int);
    fn mww_reset(e: *mut MwwEngine);
    fn mww_process(e: *mut MwwEngine, samples: *const i16, count: usize) -> c_int;
    fn mww_last_score(e: *mut MwwEngine) -> f32;
}

struct Engine {
    ptr: *mut MwwEngine,
    words: Vec<WakeWord>,
    last: Option<usize>,
}

#[repr(C)]
struct AecEngine {
    _private: [u8; 0],
}

extern "C" {
    fn aec_create(noise_suppression: c_int) -> *mut AecEngine;
    fn aec_process(e: *mut AecEngine, far: *const i16, near: *const i16, out: *mut i16) -> c_int;
}

/// Echo canceller for 10 ms frames; passes the microphones through if the
/// library cannot start, and while the speaker has been silent for 2 s (it
/// costs about a quarter of a core while it runs: 31 % of one core for the
/// app with it).
struct Aec {
    ptr: *mut AecEngine,
    near: Vec<i16>,
    last_far: Option<Instant>,
    /// Microphone and processed energy while the speaker plays (for the log).
    energy: (f64, f64),
}

impl Aec {
    fn new() -> Self {
        // Noise suppression off: the wake word frontend has its own noise
        // reduction and Home Assistant's STT gets the raw (echo-free) voice.
        // SAFETY: plain C API; the engine lives as long as the app.
        let ptr = unsafe { aec_create(-1) };
        if ptr.is_null() {
            eprintln!("nspanel-app: echo cancellation not available");
        }
        Self { ptr, near: Vec::new(), last_far: None, energy: (0.0, 0.0) }
    }

    /// Appends microphone samples and returns the processed ones (whole 10 ms frames).
    fn process(&mut self, mic: &[i16], echo: &EchoRef, out: &mut Vec<i16>) {
        out.clear();
        if self.ptr.is_null() {
            out.extend_from_slice(mic);
            return;
        }
        self.near.extend_from_slice(mic);
        let mut far = [0i16; AEC_FRAME];
        let mut frame = [0i16; AEC_FRAME];
        let whole = self.near.len() / AEC_FRAME * AEC_FRAME;
        for start in (0..whole).step_by(AEC_FRAME) {
            let playing = {
                let mut q = echo.lock().unwrap();
                let playing = !q.is_empty();
                for f in far.iter_mut() {
                    *f = q.pop_front().unwrap_or(0);
                }
                playing
            };
            if playing {
                self.last_far = Some(Instant::now());
            }
            let near = &self.near[start..start + AEC_FRAME];
            if self.last_far.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
                // SAFETY: three buffers of AEC_FRAME samples.
                unsafe { aec_process(self.ptr, far.as_ptr(), near.as_ptr(), frame.as_mut_ptr()) };
                if playing {
                    self.energy.0 += near.iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
                    self.energy.1 += frame.iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
                }
                out.extend_from_slice(&frame);
            } else {
                out.extend_from_slice(near);
            }
        }
        self.near.drain(..whole);
    }

    /// Microphone energy reduction while the speaker played (dB), since the last call.
    fn take_reduction(&mut self) -> Option<f64> {
        let (raw, clean) = std::mem::take(&mut self.energy);
        (raw > 0.0 && clean > 0.0).then(|| 10.0 * (raw / clean).log10())
    }
}

// SAFETY: the echo canceller is only used from the voice thread.
unsafe impl Send for Aec {}

/// A follow-up window in which nobody starts to speak ends after this time
/// (Home Assistant itself waits 15 s for speech, so the panel used to show
/// listening for 18 s).
const FOLLOW_UP_SILENCE: Duration = Duration::from_secs(5);
/// Speech start on the panel: 10 ms frames this much above the room's level...
const SPEECH_DB: f64 = 12.0;
/// ...for about this many frames (net) in the follow-up window.
const SPEECH_FRAMES: u32 = 15;

/// Speech start detector for the follow-up window. Home Assistant reports
/// its own speech start (event 11); this one is the panel's, so a reply that
/// starts just before the 5 s mark is not cut off while Home Assistant is
/// still processing the audio. The room's level is followed while the panel
/// is idle and silent (falls fast, rises slowly).
struct SpeechStart {
    floor: f64,
    score: u32,
    peak: f64,
}

impl SpeechStart {
    fn new() -> Self {
        Self { floor: 45.0, score: 0, peak: f64::MIN }
    }

    fn level(frame: &[i16]) -> f64 {
        let ms = frame.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / frame.len().max(1) as f64;
        10.0 * (ms + 1.0).log10()
    }

    fn track(&mut self, samples: &[i16]) {
        for f in samples.chunks(AEC_FRAME) {
            let db = Self::level(f);
            let k = if db < self.floor { 0.05 } else { 0.002 };
            self.floor += (db - self.floor) * k;
        }
    }

    fn begin(&mut self) {
        self.score = 0;
        self.peak = f64::MIN;
    }

    /// True once speech has started in the window.
    fn speech(&mut self, samples: &[i16]) -> bool {
        for f in samples.chunks(AEC_FRAME) {
            let over = Self::level(f) - self.floor;
            self.peak = self.peak.max(over);
            if over > SPEECH_DB {
                self.score += 1;
            } else {
                self.score = self.score.saturating_sub(1);
            }
            if self.score >= SPEECH_FRAMES {
                return true;
            }
        }
        false
    }
}

impl Engine {
    fn new(words: &[WakeWord]) -> Option<Self> {
        // SAFETY: plain C API; the engine lives as long as this struct.
        let ptr = unsafe { mww_create() };
        if ptr.is_null() {
            return None;
        }
        let mut loaded = Vec::new();
        for w in words {
            let path = CString::new(w.path.clone()).ok()?;
            // SAFETY: valid engine and C string.
            let idx = unsafe { mww_add_model(ptr, path.as_ptr(), w.cutoff, w.window) };
            if idx >= 0 {
                loaded.push(w.clone());
            } else {
                eprintln!("nspanel-app: cannot load wake word model {}", w.path);
            }
        }
        Some(Self { ptr, words: loaded, last: None })
    }

    fn set_active(&mut self, active: &[String], stop_word: bool) {
        for (i, w) in self.words.iter().enumerate() {
            let on = if INTERNAL_WORDS.contains(&w.id.as_str()) { stop_word } else { active.contains(&w.id) };
            // SAFETY: index within the loaded models.
            unsafe { mww_set_enabled(self.ptr, i as c_int, on as c_int) };
        }
    }

    fn reset(&mut self) {
        // SAFETY: valid engine.
        unsafe { mww_reset(self.ptr) };
    }

    fn process(&mut self, samples: &[i16]) -> Option<&WakeWord> {
        // SAFETY: valid engine, slice pointer and length.
        let idx = unsafe { mww_process(self.ptr, samples.as_ptr(), samples.len()) };
        self.last = (idx >= 0).then_some(idx as usize);
        self.last.and_then(|i| self.words.get(i))
    }

    /// The model found by the last `process` call.
    fn last_detection(&self) -> Option<&WakeWord> {
        self.last.and_then(|i| self.words.get(i))
    }

    /// Window-averaged probability (0..1) of the last detection.
    fn last_score(&self) -> f32 {
        // SAFETY: valid engine.
        unsafe { mww_last_score(self.ptr) }
    }
}

/// What the wake word model heard around a detection (the processed
/// microphone signal), sent with every wake word before the run starts, so
/// that Home Assistant can let the satellite that heard the command best
/// answer when several wake at once:
/// - level: RMS of the last second, dBFS;
/// - SNR: loudest 100 ms of the last second over the RMS of the second
///   before it, dB (the same definition on every satellite).
struct WakeLevel {
    window: VecDeque<i16>,
}

impl WakeLevel {
    const SECOND: usize = 16000; // 1 s at 16 kHz
    const SAMPLES: usize = 2 * Self::SECOND; // the last second and the one before
    const PEAK: usize = 1600; // 100 ms
    const STEP: usize = 160; // the 100 ms window moves in 10 ms steps
    /// Background shorter than this (right after a run): no SNR.
    const MIN_BACKGROUND: usize = 3200;

    fn new() -> Self {
        Self { window: VecDeque::with_capacity(Self::SAMPLES) }
    }

    fn push(&mut self, samples: &[i16]) {
        for &s in samples {
            if self.window.len() == Self::SAMPLES {
                self.window.pop_front();
            }
            self.window.push_back(s);
        }
    }

    fn mean_square<'a>(samples: impl Iterator<Item = &'a i16>) -> f64 {
        let (mut sum, mut n) = (0.0f64, 0usize);
        for &s in samples {
            sum += (s as f64).powi(2);
            n += 1;
        }
        sum / n.max(1) as f64
    }

    /// RMS of the last second, dBFS.
    fn dbfs(&self) -> f32 {
        let skip = self.window.len().saturating_sub(Self::SECOND);
        let ms = Self::mean_square(self.window.iter().skip(skip));
        (10.0 * (ms.max(1.0) / (32768.0f64 * 32768.0)).log10()) as f32
    }

    /// Loudest 100 ms of the last second over the second before it, dB.
    fn snr_db(&self) -> Option<f32> {
        let len = self.window.len();
        let start = len.saturating_sub(Self::SECOND);
        if start < Self::MIN_BACKGROUND {
            return None;
        }
        let background = Self::mean_square(self.window.iter().take(start)).max(1.0);
        // energy per 10 ms step of the last second, then the loudest 100 ms run
        let steps: Vec<f64> = (start..len)
            .step_by(Self::STEP)
            .map(|i| self.window.range(i..(i + Self::STEP).min(len)).map(|&s| (s as f64).powi(2)).sum())
            .collect();
        let per = Self::PEAK / Self::STEP;
        let loudest = steps
            .windows(per.min(steps.len()).max(1))
            .map(|w| w.iter().sum::<f64>())
            .fold(0.0f64, f64::max)
            / Self::PEAK as f64;
        Some((10.0 * (loudest.max(1.0) / background).log10()) as f32)
    }
}

// SAFETY: the engine is only used from the voice thread.
unsafe impl Send for Engine {}

fn open_capture() -> alsa::Result<PCM> {
    let pcm = PCM::new(CAPTURE_DEVICE, Direction::Capture, false)?;
    {
        let hw = HwParams::any(&pcm)?;
        hw.set_channels(MIC_CHANNELS as u32)?;
        hw.set_rate(16000, ValueOr::Nearest)?;
        hw.set_format(Format::s16())?;
        hw.set_access(Access::RWInterleaved)?;
        hw.set_period_size_near(CHUNK as i64, ValueOr::Nearest)?;
        hw.set_buffer_size_near((CHUNK * 8) as i64)?;
        pcm.hw_params(&hw)?;
    }
    pcm.start()?;
    Ok(pcm)
}

/// Speaker output: 48 kHz stereo; 16 kHz mono input is upsampled ×3.
pub enum Sound {
    Pcm16k(Vec<u8>),
    /// Any 16-bit PCM (from a WAV file), resampled to 48 kHz.
    Pcm { rate: u32, channels: u16, data: Vec<u8> },
    Chime,
    Beep,
    /// 48 kHz mono samples, played over and over until `stop()` (ringing timers, alarms, critical alerts).
    Loop(Arc<Vec<i16>>),
    /// 48 kHz mono pieces as they arrive (the media player's stream); it ends with the stream or a stop.
    Stream(Receiver<Vec<i16>>),
}

/// Longest sound the panel plays from a file (an answer or announcement).
const MAX_AUDIO_SECS: usize = 180;

/// Sample rate and channels the panel plays, and the most data for them.
fn playable(rate: u32, channels: u16) -> Option<usize> {
    ((8000..=48000).contains(&rate) && (1..=2).contains(&channels))
        .then(|| rate as usize * channels as usize * 2 * MAX_AUDIO_SECS)
}

/// 16-bit PCM WAV → (rate, channels, samples bytes).
pub fn parse_wav(bytes: &[u8]) -> Option<(u32, u16, Vec<u8>)> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12;
    let (mut rate, mut channels, mut bits) = (0u32, 0u16, 0u16);
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let len = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
        let body = pos + 8;
        if id == b"fmt " && body + 16 <= bytes.len() {
            channels = u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]);
            rate = u32::from_le_bytes(bytes[body + 4..body + 8].try_into().ok()?);
            bits = u16::from_le_bytes([bytes[body + 14], bytes[body + 15]]);
        } else if id == b"data" {
            let end = if len == 0 || len == u32::MAX as usize { bytes.len() } else { body.saturating_add(len).min(bytes.len()) };
            if bits != 16 {
                return None;
            }
            let max = playable(rate, channels)?;
            let end = end.min(body + max);
            return Some((rate, channels, bytes[body.min(end)..end].to_vec()));
        }
        pos = body.saturating_add(len).saturating_add(len & 1);
    }
    None
}

/// MP3 → (rate, channels, 16-bit samples as bytes).
fn decode_mp3(bytes: &[u8]) -> Option<(u32, u16, Vec<u8>)> {
    let mut dec = minimp3::Decoder::new(bytes);
    let (mut rate, mut channels, mut out, mut max) = (0u32, 0u16, Vec::new(), 0usize);
    loop {
        match dec.next_frame() {
            Ok(f) => {
                if rate == 0 {
                    rate = f.sample_rate as u32;
                    channels = f.channels as u16;
                    max = playable(rate, channels)?;
                }
                if f.sample_rate as u32 == rate && f.channels as u16 == channels {
                    out.extend(f.data.iter().flat_map(|s| s.to_le_bytes()));
                }
                if out.len() >= max {
                    out.truncate(max);
                    break;
                }
            }
            Err(minimp3::Error::Eof) => break,
            Err(minimp3::Error::SkippedData) => continue,
            Err(_) => break,
        }
    }
    (rate > 0 && channels > 0 && !out.is_empty()).then_some((rate, channels, out))
}

pub fn resample_to_48k(rate: u32, channels: u16, data: &[u8]) -> Vec<i16> {
    let frames: Vec<i16> = data
        .chunks_exact(2 * channels as usize)
        .map(|f| {
            let sum: i32 = f.chunks_exact(2).map(|s| i16::from_le_bytes([s[0], s[1]]) as i32).sum();
            (sum / channels as i32) as i16
        })
        .collect();
    if frames.is_empty() {
        return Vec::new();
    }
    let out_len = (frames.len() as u64 * 48000 / rate as u64) as usize;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * rate as f64 / 48000.0;
            let k = pos as usize;
            let frac = pos - k as f64;
            let a = frames[k.min(frames.len() - 1)] as f64;
            let b = frames[(k + 1).min(frames.len() - 1)] as f64;
            (a + (b - a) * frac) as i16
        })
        .collect()
}

pub struct Player {
    tx: Sender<(Sound, u64)>,
    pub busy: Arc<AtomicBool>,
    /// Raised to drop what is playing and queued (wake word during an answer);
    /// sounds queued before the stop carry an older number.
    stop_gen: Arc<std::sync::atomic::AtomicU64>,
    pub echo: EchoRef,
    /// Times the speaker ran dry (ALSA underrun) while playing: audible gaps.
    pub underruns: Arc<AtomicU32>,
}

fn chime() -> Vec<i16> {
    // Two short rising tones, soft attack and release.
    let mut out = Vec::new();
    for (freq, ms) in [(880.0f32, 90u32), (1320.0, 140)] {
        let n = 48 * ms as usize;
        for i in 0..n {
            let t = i as f32 / 48000.0;
            let env = (i as f32 / 480.0).min(1.0) * ((n - i) as f32 / 1440.0).min(1.0);
            out.push((env * 0.25 * (2.0 * std::f32::consts::PI * freq * t).sin() * 32767.0) as i16);
        }
    }
    out
}

pub fn beep() -> Vec<i16> {
    let mut out = Vec::new();
    for i in 0..48 * 400 {
        let on = (i / (48 * 100)) % 2 == 0;
        let t = i as f32 / 48000.0;
        let v = if on { 0.3 * (2.0 * std::f32::consts::PI * 1000.0 * t).sin() } else { 0.0 };
        out.push((v * 32767.0) as i16);
    }
    out
}

fn upsample(pcm16k: &[u8]) -> Vec<i16> {
    let samples: Vec<i16> = pcm16k.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    let mut out = Vec::with_capacity(samples.len() * 3);
    for i in 0..samples.len() {
        let a = samples[i] as i32;
        let b = *samples.get(i + 1).unwrap_or(&samples[i]) as i32;
        out.push(a as i16);
        out.push(((2 * a + b) / 3) as i16);
        out.push(((a + 2 * b) / 3) as i16);
    }
    out
}

impl Player {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<(Sound, u64)>();
        let busy = Arc::new(AtomicBool::new(false));
        let busy2 = busy.clone();
        let stop_gen = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let stop2 = stop_gen.clone();
        let echo: EchoRef = Arc::new(Mutex::new(VecDeque::new()));
        let echo2 = echo.clone();
        let underruns = Arc::new(AtomicU32::new(0));
        let underruns2 = underruns.clone();
        std::thread::spawn(move || {
            let mut pcm: Option<PCM> = None;
            loop {
                let (sound, gen) = match rx.recv_timeout(Duration::from_millis(400)) {
                    Ok(s) => s,
                    Err(RecvTimeoutError::Timeout) => {
                        if let Some(p) = pcm.take() {
                            let _ = p.drain();
                        }
                        busy2.store(false, Ordering::SeqCst);
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => return,
                };
                busy2.store(true, Ordering::SeqCst);
                if gen < stop2.load(Ordering::SeqCst) {
                    continue; // queued before a stop
                }
                let (mut mono, looped, stream): (Arc<Vec<i16>>, bool, Option<Receiver<Vec<i16>>>) = match sound {
                    Sound::Pcm16k(b) => (Arc::new(upsample(&b)), false, None),
                    Sound::Pcm { rate, channels, data } => (Arc::new(resample_to_48k(rate, channels, &data)), false, None),
                    Sound::Chime => (Arc::new(chime()), false, None),
                    Sound::Beep => (Arc::new(beep()), false, None),
                    Sound::Loop(samples) => (samples, true, None),
                    Sound::Stream(rx) => (Arc::new(Vec::new()), false, Some(rx)),
                };
                if mono.is_empty() && stream.is_none() {
                    continue;
                }
                if pcm.is_none() {
                    pcm = (|| -> alsa::Result<PCM> {
                        let p = PCM::new(PLAYBACK_DEVICE, Direction::Playback, false)?;
                        {
                            let hw = HwParams::any(&p)?;
                            hw.set_channels(2)?;
                            hw.set_rate(48000, ValueOr::Nearest)?;
                            hw.set_format(Format::s16())?;
                            hw.set_access(Access::RWInterleaved)?;
                            // 100 ms: the echo reference runs this much ahead of the speaker.
                            hw.set_buffer_size_near(48000 / 10)?;
                            p.hw_params(&hw)?;
                        }
                        Ok(p)
                    })()
                    .map_err(|e| eprintln!("nspanel-app: speaker: {e}"))
                    .ok();
                }
                let Some(p) = pcm.as_ref() else { continue };
                let io = match p.io_i16() {
                    Ok(io) => io,
                    Err(_) => continue,
                };
                let mut first_piece = true;
                'pieces: loop {
                // a stream: the next piece; the first waits for half a second of audio, so short
                // stalls of the network do not empty the speaker
                if let Some(rx) = &stream {
                    let mut got: Vec<i16> = Vec::new();
                    let want = if first_piece { 24000 } else { 1 };
                    first_piece = false;
                    while got.len() < want {
                        match rx.recv_timeout(Duration::from_secs(30)) {
                            Ok(piece) => got.extend(piece),
                            Err(_) => break,
                        }
                    }
                    if got.is_empty() || gen < stop2.load(Ordering::SeqCst) {
                        break 'pieces;
                    }
                    mono = Arc::new(got);
                }
                let stereo: Vec<i16> = mono.iter().flat_map(|s| [*s, *s]).collect();
                let mut stopped = false;
                // 10 ms at a time; each written piece also goes to the echo
                // reference (48 kHz → 16 kHz, mean of three samples). A looped
                // sound starts over until a stop.
                let mut offset = 0;
                loop {
                    if offset >= stereo.len() {
                        if !looped {
                            break;
                        }
                        offset = 0;
                    }
                    if gen < stop2.load(Ordering::SeqCst) {
                        let _ = p.drop();
                        let _ = p.prepare();
                        echo2.lock().unwrap().clear();
                        stopped = true;
                        break;
                    }
                    let end = (offset + 960).min(stereo.len());
                    match io.writei(&stereo[offset..end]) {
                        Ok(frames) => {
                            let first = offset / 2;
                            let mut q = echo2.lock().unwrap();
                            for tri in mono[first..first + frames].chunks(3) {
                                q.push_back((tri.iter().map(|v| *v as i32).sum::<i32>() / tri.len() as i32) as i16);
                            }
                            while q.len() > 16000 {
                                q.pop_front();
                            }
                            drop(q);
                            offset += frames * 2;
                        }
                        Err(_) => {
                            underruns2.fetch_add(1, Ordering::SeqCst);
                            let _ = p.prepare();
                        }
                    }
                }
                if stopped || stream.is_none() {
                    break 'pieces;
                }
                }
            }
        });
        Self { tx, busy, stop_gen, echo, underruns }
    }

    pub fn play(&self, s: Sound) {
        self.busy.store(true, Ordering::SeqCst);
        let _ = self.tx.send((s, self.stop_gen.load(Ordering::SeqCst)));
    }

    /// Drops what is playing and what is queued (later sounds play normally).
    pub fn stop(&self) {
        self.stop_gen.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(PartialEq, Debug)]
enum State {
    Idle,
    Starting,
    Streaming,
    Waiting,
}

pub struct VoiceControl {
    pub muted: Arc<AtomicBool>,
    /// A finished timer rings: listen without a wake word (as the ESPHome
    /// satellites do); a short sentence (e.g. "stop") stops it.
    pub ringing: Arc<AtomicBool>,
    pub active_wake_words: Arc<Mutex<Vec<String>>>,
    /// Home Assistant base URL, for relative announcement media URLs.
    pub ha_url: String,
}

/// Host part of an http(s) URL (without user info and port).
fn url_host(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("http://").or_else(|| url.strip_prefix("https://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?;
    Some(if let Some(v6) = host_port.strip_prefix('[') { v6.split(']').next()? } else { host_port.split(':').next()? })
}

/// Sound is only fetched from Home Assistant's host (any port: e.g. Music
/// Assistant next to it), by name or by the same address.
fn from_home_assistant(url: &str, ha_url: &str) -> bool {
    use std::net::ToSocketAddrs;
    let (Some(host), Some(ha_host)) = (url_host(url), url_host(ha_url)) else { return false };
    if host.eq_ignore_ascii_case(ha_host) {
        return true;
    }
    let ips = |h: &str| (h, 80).to_socket_addrs().map(|a| a.map(|s| s.ip()).collect::<Vec<_>>()).unwrap_or_default();
    let ha_ips = ips(ha_host);
    ips(host).iter().any(|ip| ha_ips.contains(ip))
}

/// Downloads a WAV or MP3 file from Home Assistant and plays it; returns once
/// it has been played.
fn fetch_and_play(url: &str, ha_url: &str, player: &Player) -> Result<(), String> {
    if !from_home_assistant(url, ha_url) {
        return Err("not on Home Assistant's host".into());
    }
    let agent = ureq::AgentBuilder::new().redirects(0).timeout(Duration::from_secs(20)).build();
    let resp = agent.get(url).call().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(resp.into_reader(), 16 * 1024 * 1024), &mut bytes)
        .map_err(|e| e.to_string())?;
    // Answers come as WAV; announcements from Home Assistant as MP3.
    let (rate, channels, data) = parse_wav(&bytes).or_else(|| decode_mp3(&bytes)).ok_or("neither a 16-bit WAV nor an MP3 file")?;
    player.play(Sound::Pcm { rate, channels, data });
    std::thread::sleep(Duration::from_millis(300));
    while player.busy.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

/// One announcement download at a time.
static ANNOUNCING: AtomicBool = AtomicBool::new(false);

/// A voice run is going on, or an answer is awaited or being fetched.
static RUN_ACTIVE: AtomicBool = AtomicBool::new(false);

/// The voice satellite is busy: a run, an awaited or fetched answer, or an
/// announcement. Sound that is already playing is `Player::busy`.
pub fn busy() -> bool {
    RUN_ACTIVE.load(Ordering::Relaxed) || ANNOUNCING.load(Ordering::Relaxed)
}

/// Downloads and plays an announcement; reports the result back to Home Assistant.
fn announce(media_id: String, ha_url: String, player: Arc<Player>, to_api: Sender<ToApi>) {
    if ANNOUNCING.swap(true, Ordering::SeqCst) {
        eprintln!("nspanel-app: announcement refused (another one is playing)");
        let _ = to_api.send(ToApi::AnnounceFinished { success: false });
        return;
    }
    std::thread::spawn(move || {
        let url = if media_id.starts_with("http") { media_id } else { format!("{ha_url}{media_id}") };
        let result = fetch_and_play(&url, &ha_url, &player);
        if let Err(e) = &result {
            eprintln!("nspanel-app: announcement {url}: {e}");
        }
        ANNOUNCING.store(false, Ordering::SeqCst);
        let _ = to_api.send(ToApi::AnnounceFinished { success: result.is_ok() });
    });
}

/// Longest a run streams the microphones to Home Assistant.
const MAX_STREAM: Duration = Duration::from_secs(30);
/// A streamed answer starts once this much audio is in (16 kHz, 16-bit:
/// 0.8 s), so a short stall in the stream does not cut the speech: Home
/// Assistant sends it about as fast as it is spoken, with waits of up to
/// about 0.7 s between chunks.
const TTS_PREBUFFER_BYTES: usize = 16000 * 2 * 8 / 10;
/// ...or after this long, whatever has arrived.
const TTS_PREBUFFER_WAIT: Duration = Duration::from_millis(1500);

/// Runs the voice satellite thread.
pub fn spawn(
    to_api: Sender<ToApi>,
    from_api: Receiver<FromApi>,
    player: Arc<Player>,
    control: VoiceControl,
    ui: impl Fn(VoiceUi) + Send + 'static,
) {
    std::thread::spawn(move || {
        let words = available_wake_words();
        let Some(mut engine) = Engine::new(&words) else {
            eprintln!("nspanel-app: wake word engine not available");
            return;
        };
        engine.set_active(&control.active_wake_words.lock().unwrap(), false);
        let mut stop_word = false;
        let mut last_speaking: Option<Instant> = None;
        eprintln!(
            "nspanel-app: wake words loaded: {:?}, active {:?}",
            engine.words.iter().map(|w| &w.phrase).collect::<Vec<_>>(),
            control.active_wake_words.lock().unwrap()
        );
        let mut pcm = loop {
            match open_capture() {
                Ok(p) => break p,
                Err(e) => {
                    eprintln!("nspanel-app: microphones: {e}");
                    std::thread::sleep(Duration::from_secs(5));
                }
            }
        };
        let mut buf = vec![0i16; CHUNK * MIC_CHANNELS];
        let mut mono = vec![0i16; CHUNK];
        let mut aec = Aec::new();
        let mut clean: Vec<i16> = Vec::with_capacity(CHUNK + AEC_FRAME);
        let mut was_speaking = false;
        // Development: NSPANEL_AEC_DUMP=<file> writes microphone and processed
        // samples (interleaved, 16 kHz, s16le) for checking the echo canceller.
        let mut aec_dump = std::env::var("NSPANEL_AEC_DUMP").ok().and_then(|p| std::fs::File::create(p).ok());
        let mut state = State::Idle;
        let mut since = Instant::now();
        let mut conversation_id = String::new();
        let mut continue_conversation = false;
        let mut follow_up_pending = false;
        // Home Assistant streams the spoken answer in the background and may
        // end the run before the audio starts: wait for the stream end.
        let mut expecting_tts: Option<Instant> = None;
        // Development: /tmp/nspanel-voice-test.wav (16 kHz mono 16-bit) is sent
        // as if spoken after the first wake word, instead of the microphones.
        let mut test_audio: Option<(Vec<i16>, usize)> = None;
        // When Home Assistant cannot stream the answer over the API (it only
        // streams 16 kHz WAV), the panel fetches the TTS URL itself.
        let mut tts_url: Option<String> = None;
        let mut tts_chunks = 0usize;
        // Streamed answer: when chunks arrive, how much audio, the longest
        // wait between two chunks (for finding gaps in the playback).
        let mut tts_first: Option<Instant> = None;
        let mut tts_last: Option<Instant> = None;
        let mut tts_bytes = 0usize;
        let mut tts_max_gap = Duration::ZERO;
        let mut underruns_before = 0u32;
        // Streamed answer audio held back until TTS_PREBUFFER_BYTES are in.
        let mut tts_hold: Option<(Vec<u8>, Instant)> = None;
        // The run ended while the answer is still spoken: back to the idle
        // screen when the speech ends, not before.
        let mut idle_after_speech = false;
        let fetching = Arc::new(AtomicBool::new(false));
        // Development: /tmp/nspanel-voice-followup.wav is sent as the speech of
        // the next follow-up run (to reproduce conversations without a person).
        let mut test_follow_up: Option<Vec<i16>> = None;
        // The current run was started by a ringing timer (no wake word).
        let mut ring_run = false;
        // The current run is a follow-up (no wake word): it ends early when
        // nobody starts to speak (FOLLOW_UP_SILENCE).
        let mut follow_up_run = false;
        let mut speech_seen = false;
        let mut stream_start = Instant::now();
        let mut detector = SpeechStart::new();
        let mut wake_level = WakeLevel::new();
        // After the panel ended a silent follow-up: Home Assistant's late
        // error events are not shown.
        let mut quiet_until: Option<Instant> = None;
        loop {
            // Microphones
            let frames = {
                let io = match pcm.io_i16() {
                    Ok(io) => io,
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(100));
                        continue;
                    }
                };
                match io.readi(&mut buf) {
                    Ok(f) => f,
                    Err(_) => {
                        let _ = pcm.prepare();
                        let _ = pcm.start();
                        0
                    }
                }
            };
            if frames == 0 {
                if pcm.state() == alsa::pcm::State::Disconnected {
                    if let Ok(p) = open_capture() {
                        pcm = p;
                    }
                }
                continue;
            }
            for i in 0..frames {
                let a = buf[i * MIC_CHANNELS + 2] as i32;
                let b = buf[i * MIC_CHANNELS + 3] as i32;
                mono[i] = (((a + b) / 2) * MIC_GAIN).clamp(-32768, 32767) as i16;
            }
            aec.process(&mono[..frames], &player.echo, &mut clean);
            if let Some(f) = aec_dump.as_mut() {
                use std::io::Write;
                // Raw and processed frames line up from the end (the canceller keeps < 10 ms back).
                let raw = &mono[frames - clean.len().min(frames)..frames];
                let mut bytes = Vec::with_capacity(raw.len() * 4);
                for (r, c) in raw.iter().zip(clean.iter()) {
                    bytes.extend_from_slice(&r.to_le_bytes());
                    bytes.extend_from_slice(&c.to_le_bytes());
                }
                let _ = f.write_all(&bytes);
            }
            if clean.is_empty() {
                continue;
            }
            let chunk = &clean[..];
            let muted = control.muted.load(Ordering::SeqCst);

            // Messages from Home Assistant
            while let Ok(msg) = from_api.try_recv() {
                match msg {
                    FromApi::Started { error } => {
                        if state == State::Starting {
                            if error {
                                state = State::Idle;
                                ui(VoiceUi::Error("Home Assistant is not connected".into()));
                            } else {
                                state = State::Streaming;
                                since = Instant::now();
                                stream_start = since;
                                speech_seen = false;
                                detector.begin();
                            }
                        }
                    }
                    FromApi::Event { kind, data } => {
                        let get = |k: &str| data.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
                        since = Instant::now();
                        match kind {
                            // STT VAD start
                            11 => {
                                if follow_up_run && !speech_seen {
                                    speech_seen = true;
                                    eprintln!(
                                        "nspanel-app: voice: follow-up speech after {} ms (Home Assistant)",
                                        stream_start.elapsed().as_millis()
                                    );
                                }
                            }
                            // STT VAD end, STT end: stop sending audio
                            12 | 4 => {
                                if state == State::Streaming {
                                    state = State::Waiting;
                                }
                                test_audio = None;
                                if kind == 4 {
                                    let text = get("text").unwrap_or_default();
                                    let words = text.split_whitespace().count();
                                    if control.ringing.load(Ordering::SeqCst) && words > 0 && words <= 4 {
                                        // Ringing timer + a short sentence = stop
                                        // (speech recognition often returns only a
                                        // fragment of a short word); not sent on to the assistant.
                                        eprintln!("nspanel-app: voice: '{text}' while the timer rings -> stop");
                                        let _ = to_api.send(ToApi::Stop);
                                        player.stop();
                                        ui(VoiceUi::Stopped);
                                        follow_up_pending = false;
                                    } else {
                                        if !text.is_empty() {
                                            ui(VoiceUi::Heard(text));
                                        }
                                        ui(VoiceUi::Thinking);
                                    }
                                }
                            }
                            // Intent end; ignored when no run is going on (a follow-up
                            // only ever continues a conversation of the panel's own).
                            6 if state != State::Idle => {
                                if let Some(id) = get("conversation_id") {
                                    conversation_id = id;
                                }
                                continue_conversation = get("continue_conversation").as_deref() == Some("1");
                            }
                            7 => {
                                if let Some(text) = get("text") {
                                    ui(VoiceUi::Answer(text));
                                }
                            }
                            // TTS stream start, TTS end (URL), stream end
                            98 => {
                                expecting_tts = Some(Instant::now());
                                tts_hold = Some((Vec::new(), Instant::now()));
                                tts_chunks = 0;
                                tts_first = None;
                                tts_last = None;
                                tts_bytes = 0;
                                tts_max_gap = Duration::ZERO;
                                underruns_before = player.underruns.load(Ordering::SeqCst);
                            }
                            8 => {
                                expecting_tts = Some(Instant::now());
                                tts_url = get("url");
                            }
                            99 => {
                                expecting_tts = None;
                                if let Some((held, _)) = tts_hold.take() {
                                    if !held.is_empty() {
                                        player.play(Sound::Pcm16k(held));
                                    }
                                }
                                if let (Some(first), Some(last)) = (tts_first, tts_last) {
                                    eprintln!(
                                        "nspanel-app: tts stream: {tts_chunks} chunks, {:.1} s of audio in {:.1} s, longest wait between chunks {} ms",
                                        tts_bytes as f64 / 32000.0,
                                        last.duration_since(first).as_secs_f64(),
                                        tts_max_gap.as_millis()
                                    );
                                }
                                if tts_chunks == 0 {
                                    if let Some(url) = tts_url.take() {
                                        fetching.store(true, Ordering::SeqCst);
                                        let (player, fetching, ha_url) = (player.clone(), fetching.clone(), control.ha_url.clone());
                                        std::thread::spawn(move || {
                                            if let Err(e) = fetch_and_play(&url, &ha_url, &player) {
                                                eprintln!("nspanel-app: answer audio {url}: {e}");
                                            }
                                            fetching.store(false, Ordering::SeqCst);
                                        });
                                    }
                                }
                                tts_url = None;
                            }
                            // Run end (ignored when no run is going on)
                            2 if state != State::Idle => {
                                state = State::Idle;
                                test_audio = None;
                                ring_run = false;
                                follow_up_run = false;
                                follow_up_pending = continue_conversation;
                                continue_conversation = false;
                                eprintln!(
                                    "nspanel-app: voice: run end, follow-up {}",
                                    if follow_up_pending { "yes (continue_conversation)" } else { "no" }
                                );
                                if !follow_up_pending {
                                    let speaking_now = player.busy.load(Ordering::SeqCst)
                                        || expecting_tts.is_some()
                                        || fetching.load(Ordering::SeqCst);
                                    if speaking_now {
                                        idle_after_speech = true;
                                    } else {
                                        ui(VoiceUi::Idle);
                                    }
                                }
                            }
                            // Error
                            0 => {
                                state = State::Idle;
                                follow_up_pending = false;
                                idle_after_speech = false;
                                ring_run = false;
                                follow_up_run = false;
                                let message = get("message").or_else(|| get("code")).unwrap_or_default();
                                if quiet_until.is_some_and(|t| Instant::now() < t) {
                                    eprintln!("nspanel-app: Assist error after the silent follow-up (not shown): {message}");
                                    ui(VoiceUi::Idle);
                                } else {
                                    eprintln!("nspanel-app: Assist error: {message}");
                                    ui(VoiceUi::Error(message));
                                }
                            }
                            _ => {}
                        }
                    }
                    FromApi::TtsAudio(bytes) => {
                        // More may come until 20 s after the last chunk.
                        expecting_tts = Some(Instant::now());
                        tts_chunks += 1;
                        let now = Instant::now();
                        if let Some(last) = tts_last {
                            tts_max_gap = tts_max_gap.max(now - last);
                        }
                        tts_first.get_or_insert(now);
                        tts_last = Some(now);
                        tts_bytes += bytes.len();
                        match tts_hold.as_mut() {
                            Some((held, _)) => {
                                held.extend_from_slice(&bytes);
                                if held.len() >= TTS_PREBUFFER_BYTES {
                                    let (held, _) = tts_hold.take().unwrap();
                                    player.play(Sound::Pcm16k(held));
                                }
                            }
                            None => player.play(Sound::Pcm16k(bytes)),
                        }
                    }
                    FromApi::SetWakeWords(ids) => {
                        engine.set_active(&ids, stop_word);
                        *control.active_wake_words.lock().unwrap() = ids;
                    }
                    FromApi::Announce { media_id, text } => {
                        ui(VoiceUi::Answer(text));
                        announce(media_id, control.ha_url.clone(), player.clone(), to_api.clone());
                    }
                }
            }
            if tts_hold.as_ref().is_some_and(|(held, since)| !held.is_empty() && since.elapsed() > TTS_PREBUFFER_WAIT) {
                let (held, _) = tts_hold.take().unwrap();
                player.play(Sound::Pcm16k(held));
            }
            if expecting_tts.is_some_and(|t| t.elapsed() > Duration::from_secs(20)) {
                expecting_tts = None;
                tts_hold = None;
            }

            if state == State::Idle && test_audio.is_none() {
                const TEST: &str = "/tmp/nspanel-voice-test.wav";
                if let Ok(bytes) = std::fs::read(TEST) {
                    let _ = std::fs::remove_file(TEST);
                    if let Some((16000, 1, data)) = parse_wav(&bytes) {
                        let samples: Vec<i16> = data.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
                        eprintln!("nspanel-app: voice test with {} samples", samples.len());
                        test_audio = Some((samples, 0));
                        if let Some(word) = engine.words.first() {
                            let phrase = engine
                                .words
                                .iter()
                                .find(|w| control.active_wake_words.lock().unwrap().first() == Some(&w.id))
                                .unwrap_or(word)
                                .phrase
                                .clone();
                            player.play(Sound::Chime);
                            let _ = to_api.send(ToApi::Start { conversation_id: String::new(), wake_word_phrase: phrase });
                            state = State::Starting;
                            since = Instant::now();
                            ui(VoiceUi::Listening);
                        }
                    }
                }
            }

            RUN_ACTIVE.store(
                state != State::Idle || expecting_tts.is_some() || fetching.load(Ordering::SeqCst),
                Ordering::Relaxed,
            );
            match state {
                State::Idle => {
                    let speaking =
                        player.busy.load(Ordering::SeqCst) || expecting_tts.is_some() || fetching.load(Ordering::SeqCst);
                    if was_speaking && !speaking {
                        if let Some(db) = aec.take_reduction() {
                            eprintln!("nspanel-app: aec: microphone level {db:.1} dB lower while the panel spoke");
                        }
                        let underruns = player.underruns.load(Ordering::SeqCst);
                        if underruns != underruns_before {
                            eprintln!("nspanel-app: speaker: ran dry {} times while the panel spoke", underruns - underruns_before);
                        }
                        underruns_before = underruns;
                        if idle_after_speech {
                            idle_after_speech = false;
                            ui(VoiceUi::Idle);
                        }
                    }
                    was_speaking = speaking;
                    if !speaking {
                        detector.track(chunk);
                    }
                    // "Stop" listens while the panel speaks or beeps (and 2 s after).
                    if speaking {
                        last_speaking = Some(Instant::now());
                    }
                    let want_stop = !aec.ptr.is_null() && last_speaking.is_some_and(|t| t.elapsed() < Duration::from_secs(2));
                    if want_stop != stop_word {
                        stop_word = want_stop;
                        engine.set_active(&control.active_wake_words.lock().unwrap(), stop_word);
                    }
                    if control.ringing.load(Ordering::SeqCst) && !muted && !ring_run && !aec.ptr.is_null() {
                        // Ringing timer: listen without a wake word, run after run.
                        ring_run = true;
                        follow_up_pending = false;
                        follow_up_run = false;
                        eprintln!("nspanel-app: voice: listening while the timer rings");
                        // Development: the follow-up test file is also the speech of a ringing run.
                        if let Ok(bytes) = std::fs::read("/tmp/nspanel-voice-followup.wav") {
                            let _ = std::fs::remove_file("/tmp/nspanel-voice-followup.wav");
                            if let Some((16000, 1, data)) = parse_wav(&bytes) {
                                test_audio = Some((data.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect(), 0));
                            }
                        }
                        let _ = to_api.send(ToApi::Start { conversation_id: String::new(), wake_word_phrase: String::new() });
                        state = State::Starting;
                        since = Instant::now();
                    } else if follow_up_pending && !speaking {
                        // Follow-up: listen again without a wake word.
                        follow_up_pending = false;
                        follow_up_run = true;
                        engine.reset();
                        wake_level = WakeLevel::new();
                        eprintln!("nspanel-app: voice: follow-up listening");
                        if test_follow_up.is_none() {
                            if let Ok(bytes) = std::fs::read("/tmp/nspanel-voice-followup.wav") {
                                let _ = std::fs::remove_file("/tmp/nspanel-voice-followup.wav");
                                if let Some((16000, 1, data)) = parse_wav(&bytes) {
                                    test_follow_up = Some(
                                        data.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect(),
                                    );
                                }
                            }
                        }
                        if let Some(samples) = test_follow_up.take() {
                            eprintln!("nspanel-app: voice test (follow-up) with {} samples", samples.len());
                            test_audio = Some((samples, 0));
                        }
                        let _ = to_api.send(ToApi::Start {
                            conversation_id: conversation_id.clone(),
                            wake_word_phrase: String::new(),
                        });
                        state = State::Starting;
                        since = Instant::now();
                        ui(VoiceUi::Listening);
                    } else if !muted && (!speaking || !aec.ptr.is_null()) {
                        // With echo cancellation the wake word also works while
                        // the panel speaks: it cuts the answer off.
                        wake_level.push(chunk);
                        if let Some(word) = engine.process(chunk).cloned().filter(|w| INTERNAL_WORDS.contains(&w.id.as_str())) {
                            // "Stop": end the answer or the ringing, no conversation.
                            eprintln!("nspanel-app: stop word '{}'", word.phrase);
                            player.stop();
                            expecting_tts = None;
                            tts_hold = None;
                            idle_after_speech = false;
                            follow_up_pending = false;
                            engine.reset();
                            ui(VoiceUi::Stopped);
                        } else if let Some(word) = engine.last_detection().cloned() {
                            let (level_dbfs, score, snr_db) = (wake_level.dbfs(), engine.last_score(), wake_level.snr_db());
                            eprintln!(
                                "nspanel-app: wake word '{}'{} level {level_dbfs:.1} dBFS snr {} score {score:.2}",
                                word.phrase,
                                if speaking { " (during playback)" } else { "" },
                                snr_db.map(|v| format!("{v:.1} dB")).unwrap_or_else(|| "n/a".into())
                            );
                            // Before the run: Home Assistant has the values when the pipeline starts.
                            let _ = to_api.send(ToApi::WakeInfo { level_dbfs, score, snr_db });
                            // The next wake word measures only audio heard after this run.
                            wake_level = WakeLevel::new();
                            if speaking {
                                player.stop();
                                expecting_tts = None;
                                follow_up_pending = false;
                            }
                            tts_hold = None;
                            idle_after_speech = false;
                            player.play(Sound::Chime);
                            engine.reset();
                            follow_up_run = false;
                            conversation_id.clear();
                            let _ = to_api.send(ToApi::Start {
                                conversation_id: String::new(),
                                wake_word_phrase: word.phrase.clone(),
                            });
                            state = State::Starting;
                            since = Instant::now();
                            ui(VoiceUi::Listening);
                        }
                    }
                }
                State::Starting => {
                    if since.elapsed() > Duration::from_secs(5) {
                        state = State::Idle;
                        ring_run = false;
                        test_audio = None;
                        ui(VoiceUi::Error("No answer from Home Assistant".into()));
                    }
                }
                State::Streaming => {
                    if follow_up_run && !speech_seen && test_audio.is_none() {
                        if detector.speech(chunk) {
                            speech_seen = true;
                            eprintln!(
                                "nspanel-app: voice: follow-up speech after {} ms (panel)",
                                stream_start.elapsed().as_millis()
                            );
                        } else if stream_start.elapsed() > FOLLOW_UP_SILENCE {
                            // Nobody spoke: abort the run (Home Assistant drops it, no answer).
                            eprintln!(
                                "nspanel-app: voice: no speech in the follow-up window ({} s; room {:.0} dB, loudest {:+.1} dB over it), listening ended",
                                FOLLOW_UP_SILENCE.as_secs(),
                                detector.floor,
                                detector.peak
                            );
                            let _ = to_api.send(ToApi::Stop);
                            state = State::Idle;
                            follow_up_run = false;
                            quiet_until = Some(Instant::now() + Duration::from_secs(3));
                            engine.reset();
                            ui(VoiceUi::Idle);
                            continue;
                        }
                    }
                    {
                        let rms = (chunk.iter().map(|&v| (v as f64) * (v as f64)).sum::<f64>() / chunk.len().max(1) as f64).sqrt();
                        let db = 20.0 * (rms / 32768.0).max(1e-9).log10();
                        MIC_LEVEL.store((((db + 60.0) / 40.0).clamp(0.0, 1.0) as f32).to_bits(), std::sync::atomic::Ordering::Relaxed);
                    }
                    if let Some((samples, pos)) = test_audio.as_mut() {
                        // Test audio, then silence.
                        let mut out = vec![0i16; chunk.len()];
                        for (i, o) in out.iter_mut().enumerate() {
                            *o = *samples.get(*pos + i).unwrap_or(&0);
                        }
                        *pos += chunk.len();
                        let bytes: Vec<u8> = out.iter().flat_map(|s| s.to_le_bytes()).collect();
                        let _ = to_api.send(ToApi::Audio(bytes));
                    } else if !muted {
                        let bytes: Vec<u8> = chunk.iter().flat_map(|s| s.to_le_bytes()).collect();
                        let _ = to_api.send(ToApi::Audio(bytes));
                    }
                    // 15 s without news from Home Assistant, or MAX_STREAM in
                    // all (events cannot keep the microphones open longer).
                    if since.elapsed() > Duration::from_secs(15) || stream_start.elapsed() > MAX_STREAM {
                        let _ = to_api.send(ToApi::Stop);
                        state = State::Waiting;
                    }
                }
                State::Waiting => {
                    if since.elapsed() > Duration::from_secs(30) {
                        state = State::Idle;
                        ui(VoiceUi::Idle);
                    }
                }
            }
        }
    });
}
