"""The PanelOS page in the sidebar: every panel's settings beside a live picture of its screen.

For administrators. The page uses these websocket commands:

* ``panelos/page/subscribe``: the panels with their device, states and
  settings, again after every change;
* ``panelos/page/options``: change integration options of a panel (dashboard
  and assistant mode, sensor reporting);
* ``panelos/page/command``: set a setting of the panel's device, the same as
  its entities;
* ``panelos/page/screen``: a fresh PNG of what the panel shows;
* ``panelos/page/resources``: the card resources with their size and whether
  the panel's dashboard needs them (for the resources it should skip).

The panel sends that picture with ``panelos/screen`` when its subscription
asks for it (command ``screenshot``).
"""

from __future__ import annotations

import base64
import binascii
from pathlib import Path
import re
from typing import Any

import voluptuous as vol

from homeassistant.components import frontend, panel_custom, websocket_api
from homeassistant.components.http import StaticPathConfig
from homeassistant.core import CALLBACK_TYPE, HomeAssistant, callback
from homeassistant.exceptions import HomeAssistantError
from homeassistant.helpers import config_validation as cv
from homeassistant.helpers.dispatcher import async_dispatcher_connect
from homeassistant.helpers.event import async_call_later
from homeassistant.loader import async_get_integration

from .config_flow import dashboard_choices
from .const import (
    CONF_DASHBOARD,
    CONF_DEVICE,
    CONF_MEDIA_PLAYER,
    CONF_SHOW_CONVERSATION,
    CONF_SHOW_HEADER,
    CONF_KEEP_RESOURCES,
    CONF_SHOW_SIDEBAR,
    CONF_SKIP_UNUSED,
    CONF_TILES,
    CONF_WEATHER,
    CONF_ZOOM,
    DOMAIN,
    REPORTING,
    SIGNAL_CONFIG_UPDATED,
    SIGNAL_LINKS,
    SIGNAL_PAGE,
    TILE_DOMAINS,
)
from .panel import KEY_DOMAIN, Panel, find_entry, panel_config
from .resources import async_resource_list

FRONTEND_DIR = Path(__file__).parent / "frontend"
STATIC_URL = "/panelos_static"
PAGE_URL = "panelos"
PAGE_ELEMENT = "panelos-page"
_PNG_MAX = 4 * 1024 * 1024
_DASHBOARD_PATH = re.compile(r"^[a-z0-9_-]{0,64}$")
_EMPTY_CLEARS = (CONF_WEATHER, CONF_MEDIA_PLAYER)

OPTIONS_SCHEMA = vol.Schema(
    {
        vol.Optional(CONF_DASHBOARD): vol.All(str, vol.Match(_DASHBOARD_PATH)),
        vol.Optional(CONF_SHOW_HEADER): bool,
        vol.Optional(CONF_SHOW_SIDEBAR): bool,
        vol.Optional(CONF_WEATHER): vol.Any("", cv.entity_domain("weather")),
        vol.Optional(CONF_MEDIA_PLAYER): vol.Any("", cv.entity_domain("media_player")),
        vol.Optional(CONF_TILES): vol.All([cv.entity_domain(list(TILE_DOMAINS))], vol.Length(max=24)),
        vol.Optional(CONF_SHOW_CONVERSATION): bool,
        vol.Optional(CONF_SKIP_UNUSED): bool,
        vol.Optional(CONF_KEEP_RESOURCES): vol.All([vol.All(str, vol.Length(max=300))], vol.Length(max=100)),
        vol.Optional(CONF_ZOOM): vol.All(vol.Coerce(int), vol.Range(min=50, max=150)),
        **{
            vol.Optional(key): vol.All(vol.Coerce(float), vol.Range(min=low, max=high))
            for key, (_default, low, high, _step, _unit) in REPORTING.items()
        },
    }
)


async def async_setup_page(hass: HomeAssistant) -> None:
    """Serve the page's files and register its commands (once per run)."""
    version = (await async_get_integration(hass, DOMAIN)).version or "0"
    hass.data.setdefault(DOMAIN, {})["version"] = str(version)
    await hass.http.async_register_static_paths([StaticPathConfig(STATIC_URL, str(FRONTEND_DIR), cache_headers=False)])
    # The PanelOS mark as an icon ("panelos:mark") everywhere in the frontend.
    frontend.add_extra_js_url(hass, f"{STATIC_URL}/panelos-icons.js?v={version}")
    for command in (ws_page_subscribe, ws_page_options, ws_page_command, ws_page_screen, ws_page_resources, ws_screen):
        websocket_api.async_register_command(hass, command)


async def async_show_page(hass: HomeAssistant) -> None:
    """Add the sidebar entry (while at least one panel is set up)."""
    if PAGE_URL in hass.data.get("frontend_panels", {}):
        return
    version = hass.data[DOMAIN]["version"]
    await panel_custom.async_register_panel(
        hass,
        webcomponent_name=PAGE_ELEMENT,
        frontend_url_path=PAGE_URL,
        module_url=f"{STATIC_URL}/panelos-page.js?v={version}",
        sidebar_title="PanelOS",
        sidebar_icon="panelos:mark",
        require_admin=True,
        config={"static_url": STATIC_URL, "version": version},
    )


@callback
def async_hide_page(hass: HomeAssistant) -> None:
    frontend.async_remove_panel(hass, PAGE_URL, warn_if_unknown=False)


def _panel(hass: HomeAssistant, entry_id: str) -> Panel:
    entry = hass.config_entries.async_get_entry(entry_id)
    panel = getattr(entry, "runtime_data", None) if entry and entry.domain == DOMAIN else None
    if panel is None:
        raise HomeAssistantError(f"No loaded PanelOS entry {entry_id}")
    return panel


def _page_data(hass: HomeAssistant) -> dict[str, Any]:
    panels = []
    for entry in hass.config_entries.async_entries(DOMAIN):
        panel: Panel | None = getattr(entry, "runtime_data", None)
        panels.append(
            {
                "entry_id": entry.entry_id,
                "title": entry.title,
                "device": entry.data.get(CONF_DEVICE, {}),
                "device_id": panel.device_id if panel else None,
                "loaded": panel is not None,
                "connected": bool(panel and panel.connected),
                "states": dict(panel.states) if panel else {},
                "select_options": dict(panel.options) if panel else {},
                "settings": {
                    **panel_config(entry),
                    "skip_unused": bool(entry.options.get(CONF_SKIP_UNUSED, True)),
                    "keep_resources": list(entry.options.get(CONF_KEEP_RESOURCES, [])),
                },
                # The dashboard the web view shows and its card resources.
                "web_dashboard": panel.dashboard if panel else "",
                "resources": list(panel.resources) if panel else [],
            }
        )
    return {
        "panels": panels,
        "dashboards": [dict(choice) for choice in dashboard_choices(hass)],
        "tile_domains": list(TILE_DOMAINS),
        "reporting": {
            key: {"default": default, "min": low, "max": high, "step": step, "unit": unit}
            for key, (default, low, high, step, unit) in REPORTING.items()
        },
    }


@websocket_api.require_admin
@websocket_api.websocket_command({vol.Required("type"): "panelos/page/subscribe"})
@callback
def ws_page_subscribe(hass: HomeAssistant, connection: websocket_api.ActiveConnection, msg: dict[str, Any]) -> None:
    """The page's data now and after every change (at most every 0.4 s)."""
    pending: CALLBACK_TYPE | None = None

    @callback
    def send(_now: Any = None) -> None:
        nonlocal pending
        pending = None
        connection.send_message(websocket_api.event_message(msg["id"], _page_data(hass)))

    @callback
    def changed(*_args: Any) -> None:
        nonlocal pending
        if pending is None:
            pending = async_call_later(hass, 0.4, send)

    unsubs = [async_dispatcher_connect(hass, signal, changed) for signal in (SIGNAL_LINKS, SIGNAL_CONFIG_UPDATED, SIGNAL_PAGE)]

    @callback
    def unsubscribe() -> None:
        for unsub in unsubs:
            unsub()
        if pending:
            pending()

    connection.subscriptions[msg["id"]] = unsubscribe
    connection.send_result(msg["id"])
    send()


@websocket_api.require_admin
@websocket_api.websocket_command(
    {vol.Required("type"): "panelos/page/options", vol.Required("entry_id"): str, vol.Required("options"): dict}
)
@callback
def ws_page_options(hass: HomeAssistant, connection: websocket_api.ActiveConnection, msg: dict[str, Any]) -> None:
    """Change some of a panel's integration options (the others stay)."""
    entry = hass.config_entries.async_get_entry(msg["entry_id"])
    if entry is None or entry.domain != DOMAIN:
        connection.send_error(msg["id"], "not_found", "No such PanelOS entry")
        return
    try:
        changes = OPTIONS_SCHEMA(msg["options"])
    except vol.Invalid as err:
        connection.send_error(msg["id"], "invalid_format", str(err))
        return
    options = {**entry.options, **changes}
    for key in _EMPTY_CLEARS:
        if options.get(key) == "":
            options.pop(key)
    hass.config_entries.async_update_entry(entry, options=options)
    connection.send_result(msg["id"])


@websocket_api.require_admin
@websocket_api.websocket_command(
    {
        vol.Required("type"): "panelos/page/command",
        vol.Required("entry_id"): str,
        vol.Required("key"): vol.In(list(KEY_DOMAIN)),
        vol.Optional("value"): vol.Any(None, bool, int, float, vol.All(str, vol.Length(max=255))),
    }
)
@callback
def ws_page_command(hass: HomeAssistant, connection: websocket_api.ActiveConnection, msg: dict[str, Any]) -> None:
    """Set a setting of the panel's device (as its entity would)."""
    try:
        _panel(hass, msg["entry_id"]).send_command(msg["key"], msg.get("value"))
    except HomeAssistantError as err:
        connection.send_error(msg["id"], "not_connected", str(err))
        return
    connection.send_result(msg["id"])


@websocket_api.require_admin
@websocket_api.websocket_command({vol.Required("type"): "panelos/page/screen", vol.Required("entry_id"): str})
@websocket_api.async_response
async def ws_page_screen(hass: HomeAssistant, connection: websocket_api.ActiveConnection, msg: dict[str, Any]) -> None:
    """A fresh picture of the panel's screen (PNG, base64)."""
    try:
        png = await _panel(hass, msg["entry_id"]).async_screenshot()
    except HomeAssistantError as err:
        connection.send_error(msg["id"], "no_picture", str(err))
        return
    connection.send_result(msg["id"], {"png": base64.b64encode(png).decode()})


@websocket_api.require_admin
@websocket_api.websocket_command({vol.Required("type"): "panelos/page/resources", vol.Required("entry_id"): str})
@websocket_api.async_response
async def ws_page_resources(hass: HomeAssistant, connection: websocket_api.ActiveConnection, msg: dict[str, Any]) -> None:
    """The card resources, their size and whether the panel's dashboard needs them."""
    entry = hass.config_entries.async_get_entry(msg["entry_id"])
    if entry is None or entry.domain != DOMAIN:
        connection.send_error(msg["id"], "not_found", "No such PanelOS entry")
        return
    panel: Panel | None = getattr(entry, "runtime_data", None)
    dashboard = panel.dashboard if panel else panel_config(entry)["dashboard"]
    connection.send_result(
        msg["id"], {"dashboard": dashboard, "resources": await async_resource_list(hass, dashboard)}
    )


@websocket_api.websocket_command(
    {vol.Required("type"): "panelos/screen", vol.Optional("panel_id", default=""): str, vol.Required("png"): str}
)
@callback
def ws_screen(hass: HomeAssistant, connection: websocket_api.ActiveConnection, msg: dict[str, Any]) -> None:
    """The panel's answer to a screenshot command."""
    entry = find_entry(hass, msg["panel_id"])
    panel: Panel | None = getattr(entry, "runtime_data", None) if entry else None
    if panel is None or not panel.linked_on(id(connection)):
        connection.send_error(msg["id"], "not_subscribed", "Subscribe with panelos/subscribe first")
        return
    try:
        png = base64.b64decode(msg["png"], validate=True)
    except (binascii.Error, ValueError):
        connection.send_error(msg["id"], "invalid_format", "Not base64")
        return
    if len(png) > _PNG_MAX or not png.startswith(b"\x89PNG\r\n\x1a\n"):
        connection.send_error(msg["id"], "invalid_format", "Not a PNG")
        return
    panel.screen_received(png)
    connection.send_result(msg["id"])
