"""Constants for the PanelOS integration."""

DOMAIN = "panelos"

CONF_PANEL_ID = "panel_id"
CONF_WEATHER = "weather_entity"
CONF_MEDIA_PLAYER = "media_player_entity"
CONF_TILES = "tile_entities"
CONF_SHOW_CONVERSATION = "show_conversation"

# Dashboard mode: the Home Assistant dashboard in the panel's web view.
CONF_DASHBOARD = "dashboard"
CONF_SHOW_HEADER = "show_header"
CONF_SHOW_SIDEBAR = "show_sidebar"
# Card resources (Lovelace resources): Home Assistant loads every resource on
# every dashboard, used or not. With "skip unused" on (default) the panel's
# web view leaves out the ones the dashboard it shows does not need; worked
# out again whenever the dashboard or its configuration changes. Resources
# in "keep" are always loaded (when the guess misses one).
CONF_SKIP_UNUSED = "skip_unused_resources"
CONF_KEEP_RESOURCES = "keep_resources"
# Size of the dashboard in the web view, percent.
CONF_ZOOM = "zoom"
ZOOM_RANGE = (50, 150, 5)
# The dashboard chosen on the panel itself (its settings or its Dashboard
# entity) applies while this is selected.
DASHBOARD_FROM_PANEL = ""

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

# Entities the assistant mode's quick tiles can show.
TILE_DOMAINS = ("light", "switch", "cover", "fan", "input_boolean", "scene", "script", "sensor", "climate")

SERVICE_SHOW_VIEW = "show_view"
SERVICE_SHOW_MESSAGE = "show_message"
VIEWS = ["home", "dashboard", "weather", "media", "camera"]

# Internal signals; the panels follow them through panelos/subscribe.
SIGNAL_CONFIG_UPDATED = "panelos_config_updated"
SIGNAL_SHOW = "panelos_show"

# What the panel last told about itself (name, model, version, MAC), kept in
# the entry so the device is complete before the panel connects.
CONF_DEVICE = "device"

# The panel's own device: its entities are fed by panelos/report and send
# their commands back through panelos/subscribe. The voice satellite stays
# on the ESPHome device, which is linked to this one ("Connected via").
PLATFORMS = ["binary_sensor", "button", "number", "select", "sensor", "switch"]

# Entity keys per platform, as the panel names them in panelos/report and in
# the commands it gets back.
ENTITY_KEYS = {
    "switch": ("screen", "auto_brightness", "proximity_wake"),
    "select": ("dashboard", "theme", "screensaver", "screensaver_style", "display_mode"),
    "number": ("brightness",),
    "sensor": ("illuminance", "soc_temperature", "wifi_signal", "ip_address", "app_version"),
    "binary_sensor": ("presence",),
    "button": ("restart_app", "reboot"),
}

# Until PanelOS 0.0.3 these entities were on the panel's ESPHome device: the
# ESPHome platform and entity name of each, to take their registry entries
# over (entity ids, history and settings stay).
ESPHOME_ENTITIES = {
    "screen": ("switch", "Screen"),
    "auto_brightness": ("switch", "Automatic brightness"),
    "proximity_wake": ("switch", "Wake on approach"),
    "dashboard": ("select", "Dashboard"),
    "theme": ("select", "Theme"),
    "screensaver": ("select", "Screensaver after"),
    "screensaver_style": ("select", "Screensaver style"),
    "display_mode": ("select", "Display mode"),
    "brightness": ("number", "Brightness"),
    "illuminance": ("sensor", "Illuminance"),
    "soc_temperature": ("sensor", "SoC temperature"),
    "wifi_signal": ("sensor", "Wi-Fi signal"),
    "ip_address": ("text_sensor", "IP address"),
    "app_version": ("text_sensor", "App version"),
    "presence": ("binary_sensor", "Presence"),
    "restart_app": ("button", "Restart app"),
    "reboot": ("button", "Reboot"),
}

SIGNAL_LINKS = "panelos_links"
# Something the sidebar page shows changed (a report, a link).
SIGNAL_PAGE = "panelos_page"
