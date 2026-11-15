"""PanelOS: panels running PanelOS as devices of their own, their settings, and screen control.

A panel talks to Home Assistant on its own: over the WebSocket API with its
own login, and over the ESPHome native API for the voice satellite. This
integration gives each panel:

* its device with the display, its settings and its sensors (screen,
  brightness, theme, screensaver, dashboard, display mode, illuminance,
  presence, …). The panel reports their states with the websocket command
  ``panelos/report`` and gets their commands back on ``panelos/subscribe``.
  The ESPHome device keeps the voice satellite (with its volume, microphone
  and wake word sensors) and is shown as connected via this device;
* the dashboard mode's settings (which Home Assistant dashboard the panel's
  web view shows, with or without its header and sidebar) and the assistant
  mode's settings (weather, media player, quick tiles, full-screen
  conversation), set in the integration's options;
* how much the recorded sensors (illuminance, SoC temperature, Wi-Fi signal)
  must change before the panel sends a new value, also in the options;
* actions that put something on a panel's screen: ``panelos.show_view``
  (home, dashboard, weather, media, a camera) and ``panelos.show_message``;
* critical alerts and status screens shared by every panel (``house.py``),
  with ``panelos.show_status`` / ``panelos.clear_status`` for the status
  screens an automation starts;
* the PanelOS page in the sidebar (administrators): every setting of a panel
  beside a live picture of its screen (``page.py``).

Panels follow their settings, show requests and commands with
``panelos/subscribe``, so the panel's Home Assistant user does not need to be
an administrator.
"""

from __future__ import annotations

from typing import Any

import voluptuous as vol

from homeassistant.components import websocket_api
from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant, ServiceCall, callback
from homeassistant.exceptions import HomeAssistantError, ServiceValidationError
from homeassistant.helpers import config_validation as cv
from homeassistant.helpers.dispatcher import async_dispatcher_connect, async_dispatcher_send
from homeassistant.helpers.service import async_register_admin_service

from .const import (
    CONF_PANEL_ID,
    DOMAIN,
    PLATFORMS,
    SERVICE_CLEAR_STATUS,
    SERVICE_SHOW_MESSAGE,
    SERVICE_SHOW_STATUS,
    SERVICE_SHOW_VIEW,
    SIGNAL_CONFIG_UPDATED,
    SIGNAL_LINKS,
    SIGNAL_SHOW,
    VIEWS,
)
from .house import House
from .page import async_hide_page, async_setup_page, async_show_page
from .panel import Panel, find_entry, links, panel_config

type PanelOSConfigEntry = ConfigEntry[Panel]

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
        vol.Optional("title", default=""): vol.All(cv.string, vol.Length(max=100)),
        vol.Required("message"): vol.All(cv.string, vol.Length(max=1000)),
        vol.Optional("icon", default=""): vol.All(cv.string, vol.Length(max=64)),
        vol.Optional("seconds", default=30): _SECONDS,
        vol.Optional("chime", default=False): cv.boolean,
    }
)


STATUS_ACTION_SCHEMA = vol.Schema({vol.Required("screen"): vol.All(cv.string, vol.Match(r"^[a-z0-9_]{1,40}$"))})


def _loaded_panel(hass: HomeAssistant, panel_id: str) -> Panel | None:
    entry = find_entry(hass, panel_id)
    return getattr(entry, "runtime_data", None) if entry else None


@websocket_api.websocket_command(
    {vol.Required("type"): "panelos/subscribe", vol.Optional("panel_id", default=""): str}
)
@callback
def ws_subscribe(hass: HomeAssistant, connection: websocket_api.ActiveConnection, msg: dict[str, Any]) -> None:
    """Send a panel its settings now and on every change, then its show requests and commands."""
    panel_id = msg["panel_id"]
    key = hash((id(connection), msg["id"]))

    @callback
    def send_config() -> None:
        entry = find_entry(hass, panel_id)
        config = panel_config(entry) if entry else {}
        connection.send_message(websocket_api.event_message(msg["id"], {"config": config}))

    @callback
    def config_changed(changed_panel: str) -> None:
        if not panel_id or changed_panel in (panel_id, ""):
            send_config()

    @callback
    def show(target: str, request: dict[str, Any]) -> None:
        # A target is a panel id; a panel that subscribed without one is the
        # only entry's panel.
        if not target or target == panel_id or (
            (entry := find_entry(hass, panel_id)) is not None and panel_config(entry)["panel_id"] == target
        ):
            connection.send_message(websocket_api.event_message(msg["id"], {"show": request}))

    @callback
    def command(cmd: dict[str, Any]) -> None:
        connection.send_message(websocket_api.event_message(msg["id"], {"command": cmd}))

    unsub_config = async_dispatcher_connect(hass, SIGNAL_CONFIG_UPDATED, config_changed)
    unsub_show = async_dispatcher_connect(hass, SIGNAL_SHOW, show)
    links(hass)[key] = (panel_id, id(connection), command)

    @callback
    def unsubscribe() -> None:
        unsub_config()
        unsub_show()
        links(hass).pop(key, None)
        async_dispatcher_send(hass, SIGNAL_LINKS)

    connection.subscriptions[msg["id"]] = unsubscribe
    connection.send_result(msg["id"])
    send_config()
    # The critical alerts and status screens active right now.
    if (entry := find_entry(hass, panel_id)) is not None:
        for request in hass.data[DOMAIN]["house"].requests_for(panel_config(entry)["panel_id"]):
            connection.send_message(websocket_api.event_message(msg["id"], {"show": request}))
    async_dispatcher_send(hass, SIGNAL_LINKS)


@websocket_api.websocket_command(
    {
        vol.Required("type"): "panelos/report",
        vol.Optional("panel_id", default=""): str,
        vol.Optional("device"): dict,
        vol.Optional("options"): dict,
        vol.Optional("states"): dict,
        vol.Optional("dashboard"): str,
    }
)
@callback
def ws_report(hass: HomeAssistant, connection: websocket_api.ActiveConnection, msg: dict[str, Any]) -> None:
    """The panel's device info, select options and entity states (changes only)."""
    panel = _loaded_panel(hass, msg["panel_id"])
    if panel is None:
        connection.send_error(msg["id"], "not_found", f"No PanelOS entry for the panel '{msg['panel_id']}'")
        return
    # Only the connection that follows the panel's subscription reports for it.
    if not panel.linked_on(id(connection)):
        connection.send_error(msg["id"], "not_subscribed", "Subscribe with panelos/subscribe first")
        return
    panel.apply_report(msg)
    connection.send_result(msg["id"])


async def async_setup(hass: HomeAssistant, config: dict[str, Any]) -> bool:
    """Register the websocket commands, the actions and the sidebar page's files once."""
    websocket_api.async_register_command(hass, ws_subscribe)
    websocket_api.async_register_command(hass, ws_report)
    house = House(hass)
    hass.data.setdefault(DOMAIN, {})["house"] = house
    await house.async_load()
    await async_setup_page(hass)

    async def handle_show(call: ServiceCall) -> None:
        request = dict(call.data)
        target = request.pop(CONF_PANEL_ID, "")
        panels = {pid for pid, _, _ in links(hass).values()}
        if not panels or (target and target not in panels):
            raise HomeAssistantError(
                f"No PanelOS panel is connected{f' as {target}' if target else ''} (connected: {sorted(panels) or 'none'})"
            )
        request["type"] = "view" if call.service == SERVICE_SHOW_VIEW else "message"
        async_dispatcher_send(hass, SIGNAL_SHOW, target, request)

    # Admin users and automations only: the actions put a camera or a message
    # on the panel.
    async_register_admin_service(hass, DOMAIN, SERVICE_SHOW_VIEW, handle_show, schema=SHOW_VIEW_SCHEMA)
    async_register_admin_service(hass, DOMAIN, SERVICE_SHOW_MESSAGE, handle_show, schema=SHOW_MESSAGE_SCHEMA)

    async def handle_status(call: ServiceCall) -> None:
        try:
            if call.service == SERVICE_SHOW_STATUS:
                house.show_status(call.data["screen"])
            else:
                house.clear_status(call.data["screen"])
        except vol.Invalid as err:
            raise ServiceValidationError(str(err)) from err

    # Status screens whose trigger is an action (set on the PanelOS page).
    async_register_admin_service(hass, DOMAIN, SERVICE_SHOW_STATUS, handle_status, schema=STATUS_ACTION_SCHEMA)
    async_register_admin_service(hass, DOMAIN, SERVICE_CLEAR_STATUS, handle_status, schema=STATUS_ACTION_SCHEMA)
    return True


async def async_setup_entry(hass: HomeAssistant, entry: PanelOSConfigEntry) -> bool:
    """Set up one panel: its device and entities, and its settings."""
    panel = Panel(hass, entry)
    entry.runtime_data = panel
    panel.async_setup()
    await hass.config_entries.async_forward_entry_setups(entry, PLATFORMS)
    await async_show_page(hass)
    entry.async_on_unload(entry.add_update_listener(_options_updated))
    async_dispatcher_send(hass, SIGNAL_CONFIG_UPDATED, panel_config(entry)["panel_id"])
    # Shared alerts and status screens for this panel too.
    hass.data[DOMAIN]["house"].evaluate()
    return True


async def _options_updated(hass: HomeAssistant, entry: ConfigEntry) -> None:
    async_dispatcher_send(hass, SIGNAL_CONFIG_UPDATED, panel_config(entry)["panel_id"])
    # Another dashboard, or other resource settings: work the skip list out again.
    if panel := getattr(entry, "runtime_data", None):
        panel.refresh_skip()


async def async_unload_entry(hass: HomeAssistant, entry: PanelOSConfigEntry) -> bool:
    """Unload a panel's entry (the panel falls back to its defaults)."""
    if not await hass.config_entries.async_unload_platforms(entry, PLATFORMS):
        return False
    if not any(
        other.entry_id != entry.entry_id and getattr(other, "runtime_data", None)
        for other in hass.config_entries.async_entries(DOMAIN)
    ):
        async_hide_page(hass)
    panel_id = panel_config(entry)["panel_id"]
    hass.async_create_task(_after_unload(hass, panel_id))
    return True


async def _after_unload(hass: HomeAssistant, panel_id: str) -> None:
    # The entry is gone from the list only after unload returns.
    async_dispatcher_send(hass, SIGNAL_CONFIG_UPDATED, panel_id)
    hass.data[DOMAIN]["house"].evaluate()
