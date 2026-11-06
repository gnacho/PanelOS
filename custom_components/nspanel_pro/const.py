"""Constants for the NSPanel Pro (open firmware) integration."""

DOMAIN = "nspanel_pro"

CONF_PANEL_ID = "panel_id"
CONF_WEATHER = "weather_entity"
CONF_MEDIA_PLAYER = "media_player_entity"
CONF_TILES = "tile_entities"
CONF_SHOW_CONVERSATION = "show_conversation"

DEFAULT_PANEL_ID = "nspanel-pro"

SERVICE_SHOW_VIEW = "show_view"
SERVICE_SHOW_MESSAGE = "show_message"
VIEWS = ["home", "dashboard", "weather", "media", "camera"]

# Internal signals; the panels follow them through nspanel_pro/subscribe.
SIGNAL_CONFIG_UPDATED = "nspanel_pro_config_updated"
SIGNAL_SHOW = "nspanel_pro_show"
