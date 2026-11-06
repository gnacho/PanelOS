//! Panel hardware the app drives directly: backlight, ambient light and
//! proximity (STK3x3x through IIO), SoC temperature.

use std::fs;
use std::path::PathBuf;

const BACKLIGHT: &str = "/sys/class/backlight/backlight";

pub struct Backlight {
    max: u32,
    current: u32,
}

impl Backlight {
    pub fn open() -> Self {
        let max = fs::read_to_string(format!("{BACKLIGHT}/max_brightness"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(255);
        let current = fs::read_to_string(format!("{BACKLIGHT}/brightness"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(max);
        Self { max, current }
    }

    /// Sets the level in 0..=1000 (per mille); writes only on change.
    pub fn set_permille(&mut self, permille: u32) {
        let level = (self.max * permille.min(1000) + 500) / 1000;
        if level != self.current {
            if fs::write(format!("{BACKLIGHT}/brightness"), level.to_string()).is_ok() {
                self.current = level;
            }
        }
    }
}

pub struct LightSensor {
    dir: Option<PathBuf>,
    /// Available scales (lux per count), most sensitive first.
    scales: Vec<f64>,
    index: usize,
    last_lux: Option<f64>,
}

fn read_f64(p: PathBuf) -> Option<f64> {
    fs::read_to_string(p).ok()?.trim().parse().ok()
}

impl LightSensor {
    pub fn open() -> Self {
        let dir = fs::read_dir("/sys/bus/iio/devices").ok().and_then(|entries| {
            entries.filter_map(Result::ok).map(|e| e.path()).find(|p| {
                fs::read_to_string(p.join("name")).map(|n| n.trim().starts_with("stk3")).unwrap_or(false)
            })
        });
        let mut scales: Vec<f64> = dir
            .as_ref()
            .and_then(|d| fs::read_to_string(d.join("in_illuminance_scale_available")).ok())
            .map(|s| s.split_whitespace().filter_map(|v| v.parse().ok()).collect())
            .unwrap_or_default();
        scales.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let current = dir.as_ref().and_then(|d| read_f64(d.join("in_illuminance_scale"))).unwrap_or(0.1);
        let index = scales.iter().position(|s| (s - current).abs() < 1e-6).unwrap_or(0);
        if scales.is_empty() {
            scales.push(current);
        }
        Self { dir, scales, index, last_lux: None }
    }

    /// Ambient light in lux. The sensor saturates at 65535 counts (about
    /// 6500 lx at the most sensitive scale), so the scale is switched
    /// automatically: coarser when saturated, finer when the reading is low.
    pub fn lux(&mut self) -> Option<f64> {
        let dir = self.dir.clone()?;
        let raw = read_f64(dir.join("in_illuminance_raw"))?;
        let scale = self.scales[self.index];
        let mut new_index = self.index;
        if raw >= 60000.0 && self.index + 1 < self.scales.len() {
            new_index += 1;
        } else if raw < 400.0 && self.index > 0 {
            new_index -= 1;
        }
        if new_index != self.index {
            let value = format!("{}", self.scales[new_index]);
            if fs::write(dir.join("in_illuminance_scale"), value).is_ok() {
                self.index = new_index;
            }
        }
        let lux = raw * scale;
        // A saturated reading only says "at least this bright".
        self.last_lux = Some(lux);
        self.last_lux
    }

    pub fn proximity(&self) -> Option<f64> {
        read_f64(self.dir.as_ref()?.join("in_proximity_raw"))
    }
}

/// Learns the resting proximity level and reports an approach after two
/// consecutive readings clearly above it.
#[derive(Default)]
pub struct ProximityWake {
    rest: Option<f64>,
    above: u32,
}

impl ProximityWake {
    /// Someone (or something) is close to the panel right now.
    pub fn near(&self) -> bool {
        self.above >= 2
    }

    pub fn update(&mut self, value: f64) -> bool {
        let rest = *self.rest.get_or_insert(value);
        let threshold = (rest * 2.0).max(rest + 60.0);
        if value > threshold {
            self.above = self.above.saturating_add(1);
        } else {
            self.above = 0;
            // Follow slow changes of the resting level (dust, temperature).
            self.rest = Some(rest * 0.98 + value * 0.02);
        }
        self.above == 2
    }
}

/// Backlight level (per mille) for an ambient light level: minimum in the
/// dark, maximum from about 500 lx, logarithmic in between.
pub fn auto_brightness_permille(lux: f64) -> u32 {
    let lux = lux.max(1.0);
    let t = (lux.ln() / 500f64.ln()).clamp(0.0, 1.0);
    (60.0 + t * 940.0) as u32
}

/// Wi-Fi signal level in dBm (`iw dev wlan0 link`; rtw88 does not fill
/// /proc/net/wireless).
pub fn wifi_dbm() -> Option<f64> {
    let out = std::process::Command::new("iw").args(["dev", "wlan0", "link"]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().find(|l| l.trim_start().starts_with("signal:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

/// Speaker volume: the RK809 codec's "Master Playback Volume" (0..255 =
/// -95..0 dB, 0.375 dB per step). 100 % = 0 dB, every percent 0.4 dB less,
/// 0 % = off. (The boot default is 200 = -20.6 dB, about 48 %.)
pub fn set_volume(percent: u8) {
    let p = percent.min(100) as f64;
    let value = if p == 0.0 { 0 } else { (255.0 - (100.0 - p) * 0.4 / 0.375).round().clamp(1.0, 255.0) as i64 };
    let result = (|| -> alsa::Result<()> {
        let mixer = alsa::mixer::Mixer::new("hw:CARD=rk809", false)?;
        let id = alsa::mixer::SelemId::new("Master", 0);
        if let Some(selem) = mixer.find_selem(&id) {
            selem.set_playback_volume_all(value)?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        eprintln!("nspanel-app: volume: {e}");
    }
}

pub fn soc_temperature() -> Option<f64> {
    Some(read_f64(PathBuf::from("/sys/class/thermal/thermal_zone0/temp"))? / 1000.0)
}
