"""Constants for the NSPanel Pro (open firmware) integration."""

DOMAIN = "nspanel_pro"

CONF_PANEL_ID = "panel_id"
CONF_WEATHER = "weather_entity"
CONF_MEDIA_PLAYER = "media_player_entity"
CONF_TILES = "tile_entities"
CONF_SHOW_CONVERSATION = "show_conversation"

# How much the panel's recorded sensors must change before a new value is
# sent (each value is a row in the recorder's database).
CONF_ILLUMINANCE_CHANGE = "illuminance_change"
CONF_SOC_TEMPERATURE_CHANGE = "soc_temperature_change"
CONF_WIFI_SIGNAL_CHANGE = "wifi_signal_change"
# key: (default, min, max, step, unit)
REPORTING = {
    CONF_ILLUMINANCE_CHANGE: (15, 1, 100, 1, "%"),
    CONF_SOC_TEMPERATURE_CHANGE: (1.0, 0.1, 10, 0.1, "°C"),
    CONF_WIFI_SIGNAL_CHANGE: (3, 1, 20, 1, "dB"),
}

DEFAULT_PANEL_ID = "nspanel-pro"

SERVICE_SHOW_VIEW = "show_view"
SERVICE_SHOW_MESSAGE = "show_message"
VIEWS = ["home", "dashboard", "weather", "media", "camera"]

# Internal signals; the panels follow them through nspanel_pro/subscribe.
SIGNAL_CONFIG_UPDATED = "nspanel_pro_config_updated"
SIGNAL_SHOW = "nspanel_pro_show"
