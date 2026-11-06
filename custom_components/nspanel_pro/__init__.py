"""NSPanel Pro (open firmware): assistant mode settings and screen control.

The panel already talks to Home Assistant on its own (WebSocket API with its
own login, and the ESPHome native API for its settings, sensors and the voice
satellite). This integration adds what is easier to do from Home Assistant:

* the assistant mode's settings (weather, media player, quick tiles,
  full-screen conversation), set in the integration's options;
* actions that put something on a panel's screen: ``nspanel_pro.show_view``
  (home, dashboard, weather, media, a camera) and ``nspanel_pro.show_message``.

Panels follow both with the websocket command ``nspanel_pro/subscribe``
(their settings at once and on every change, then the show requests), so the
panel's Home Assistant user does not need to be an administrator.
"""

from __future__ import annotations

from typing import Any

import voluptuous as vol

from homeassistant.components import websocket_api
from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant, ServiceCall, callback
from homeassistant.exceptions import HomeAssistantError
from homeassistant.helpers import config_validation as cv
from homeassistant.helpers.dispatcher import async_dispatcher_connect, async_dispatcher_send

from .const import (
    CONF_MEDIA_PLAYER,
    CONF_PANEL_ID,
    CONF_SHOW_CONVERSATION,
    CONF_TILES,
    CONF_WEATHER,
    DOMAIN,
    SERVICE_SHOW_MESSAGE,
    SERVICE_SHOW_VIEW,
    SIGNAL_CONFIG_UPDATED,
    SIGNAL_SHOW,
    VIEWS,
)

CONFIG_SCHEMA = cv.config_entry_only_config_schema(DOMAIN)

_SECONDS = vol.All(vol.Coerce(int), vol.Range(min=0, max=3600))

SHOW_VIEW_SCHEMA = vol.Schema(
    {
        vol.Optional(CONF_PANEL_ID, default=""): cv.string,
        vol.Required("view"): vol.In(VIEWS),
        vol.Optional("entity_id"): cv.entity_id,
        vol.Optional("seconds", default=30): _SECONDS,
    }
)

SHOW_MESSAGE_SCHEMA = vol.Schema(
    {
        vol.Optional(CONF_PANEL_ID, default=""): cv.string,
        vol.Optional("title", default=""): cv.string,
        vol.Required("message"): cv.string,
        vol.Optional("icon", default=""): cv.string,
        vol.Optional("seconds", default=30): _SECONDS,
        vol.Optional("chime", default=False): cv.boolean,
    }
)


def _panel_config(entry: ConfigEntry) -> dict[str, Any]:
    options = {**entry.data, **entry.options}
    return {
        "panel_id": options.get(CONF_PANEL_ID, ""),
        "weather_entity": options.get(CONF_WEATHER, ""),
        "media_player_entity": options.get(CONF_MEDIA_PLAYER, ""),
        "tile_entities": list(options.get(CONF_TILES, [])),
        "show_conversation": bool(options.get(CONF_SHOW_CONVERSATION, True)),
    }


def _find_entry(hass: HomeAssistant, panel_id: str) -> ConfigEntry | None:
    entries = hass.config_entries.async_entries(DOMAIN)
    for entry in entries:
        if {**entry.data, **entry.options}.get(CONF_PANEL_ID) == panel_id:
            return entry
    return entries[0] if len(entries) == 1 else None


def _connected_panels(hass: HomeAssistant) -> dict[int, str]:
    """Websocket subscriptions of the panels: id(connection) + msg id → panel id."""
    return hass.data.setdefault(DOMAIN, {}).setdefault("panels", {})


@websocket_api.websocket_command(
    {vol.Required("type"): "nspanel_pro/subscribe", vol.Optional("panel_id", default=""): str}
)
@callback
def ws_subscribe(hass: HomeAssistant, connection: websocket_api.ActiveConnection, msg: dict[str, Any]) -> None:
    """Send a panel its settings now and on every change, then its show requests."""
    panel_id = msg["panel_id"]
    key = hash((id(connection), msg["id"]))

    @callback
    def send_config() -> None:
        entry = _find_entry(hass, panel_id)
        config = _panel_config(entry) if entry else {}
        connection.send_message(websocket_api.event_message(msg["id"], {"config": config}))

    @callback
    def config_changed(changed_panel: str) -> None:
        if not panel_id or changed_panel in (panel_id, ""):
            send_config()

    @callback
    def show(target: str, request: dict[str, Any]) -> None:
        if not target or target == panel_id:
            connection.send_message(websocket_api.event_message(msg["id"], {"show": request}))

    unsub_config = async_dispatcher_connect(hass, SIGNAL_CONFIG_UPDATED, config_changed)
    unsub_show = async_dispatcher_connect(hass, SIGNAL_SHOW, show)
    _connected_panels(hass)[key] = panel_id

    @callback
    def unsubscribe() -> None:
        unsub_config()
        unsub_show()
        _connected_panels(hass).pop(key, None)

    connection.subscriptions[msg["id"]] = unsubscribe
    connection.send_result(msg["id"])
    send_config()


async def async_setup(hass: HomeAssistant, config: dict[str, Any]) -> bool:
    """Register the websocket command and the actions once."""
    websocket_api.async_register_command(hass, ws_subscribe)

    async def handle_show(call: ServiceCall) -> None:
        request = dict(call.data)
        target = request.pop(CONF_PANEL_ID, "")
        panels = set(_connected_panels(hass).values())
        if not panels or (target and target not in panels):
            raise HomeAssistantError(
                f"No NSPanel Pro is connected{f' as {target}' if target else ''} (connected: {sorted(panels) or 'none'})"
            )
        request["type"] = "view" if call.service == SERVICE_SHOW_VIEW else "message"
        async_dispatcher_send(hass, SIGNAL_SHOW, target, request)

    hass.services.async_register(DOMAIN, SERVICE_SHOW_VIEW, handle_show, schema=SHOW_VIEW_SCHEMA)
    hass.services.async_register(DOMAIN, SERVICE_SHOW_MESSAGE, handle_show, schema=SHOW_MESSAGE_SCHEMA)
    return True


async def async_setup_entry(hass: HomeAssistant, entry: ConfigEntry) -> bool:
    """Set up one panel."""
    entry.async_on_unload(entry.add_update_listener(_options_updated))
    async_dispatcher_send(hass, SIGNAL_CONFIG_UPDATED, _panel_config(entry)["panel_id"])
    return True


async def _options_updated(hass: HomeAssistant, entry: ConfigEntry) -> None:
    async_dispatcher_send(hass, SIGNAL_CONFIG_UPDATED, _panel_config(entry)["panel_id"])


async def async_unload_entry(hass: HomeAssistant, entry: ConfigEntry) -> bool:
    """Unload a panel's entry (the panel falls back to its defaults)."""
    panel_id = _panel_config(entry)["panel_id"]
    hass.async_create_task(_after_unload(hass, panel_id))
    return True


async def _after_unload(hass: HomeAssistant, panel_id: str) -> None:
    # The entry is gone from the list only after unload returns.
    async_dispatcher_send(hass, SIGNAL_CONFIG_UPDATED, panel_id)
