//! Material Design Icons (the set Home Assistant uses): "mdi:name" -> glyph,
//! plus Home Assistant-like default icons per domain and state.

include!(concat!(env!("OUT_DIR"), "/mdi_table.rs"));

/// Glyph for an icon name such as "mdi:lightbulb" or "lightbulb".
pub fn glyph(name: &str) -> Option<char> {
    let name = name.strip_prefix("mdi:").unwrap_or(name);
    MDI.binary_search_by(|(n, _)| n.cmp(&name))
        .ok()
        .and_then(|i| char::from_u32(MDI[i].1))
}

/// Default icon name when the entity has no `icon` attribute.
pub fn default_icon(domain: &str, state: &str, device_class: Option<&str>) -> &'static str {
    let on = !matches!(state, "off" | "closed" | "unavailable" | "unknown" | "idle" | "standby" | "paused");
    match domain {
        "light" => if on { "lightbulb" } else { "lightbulb-outline" },
        "switch" | "input_boolean" => if on { "toggle-switch-variant" } else { "toggle-switch-variant-off" },
        "fan" => "fan",
        "cover" => match device_class {
            Some("blind") => if on { "blinds-open" } else { "blinds" },
            Some("curtain") => if on { "curtains" } else { "curtains-closed" },
            Some("garage") => if on { "garage-open" } else { "garage" },
            _ => if on { "window-shutter-open" } else { "window-shutter" },
        },
        "climate" => match state {
            "heat" => "fire",
            "cool" => "snowflake",
            "dry" => "water-percent",
            "fan_only" => "fan",
            "heat_cool" | "auto" => "thermostat-auto",
            _ => "thermostat",
        },
        "media_player" => match device_class {
            Some("tv") => if state == "off" { "television-off" } else { "television" },
            _ => if state == "off" { "speaker-off" } else { "speaker" },
        },
        "camera" => "video",
        "sensor" => match device_class {
            Some("temperature") => "thermometer",
            Some("humidity") => "water-percent",
            Some("power") => "flash",
            Some("energy") => "lightning-bolt",
            Some("illuminance") => "brightness-5",
            Some("battery") => "battery",
            _ => "eye",
        },
        "binary_sensor" => match device_class {
            Some("motion") | Some("occupancy") | Some("presence") => if on { "motion-sensor" } else { "motion-sensor-off" },
            Some("door") => if on { "door-open" } else { "door-closed" },
            Some("window") => if on { "window-open" } else { "window-closed" },
            _ => if on { "checkbox-marked-circle" } else { "checkbox-blank-circle-outline" },
        },
        "lock" => if state == "locked" { "lock" } else { "lock-open" },
        "scene" => "palette",
        "script" => "script-text",
        "alarm_control_panel" => "shield-home",
        _ => "help-circle-outline",
    }
}
