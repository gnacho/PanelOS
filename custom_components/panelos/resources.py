"""Card resources (Lovelace resources) and which of them a dashboard needs.

Home Assistant loads every resource on every dashboard, used or not; on a
panel each one costs load time. A resource counts as needed when its folder
or file name matches a custom card type the dashboard uses
(``custom:mushroom-light-card`` → ``mushroom``), one of its top-level settings
that is not switched off (``kiosk_mode`` → ``kiosk-mode``), or, for an icon
set, an icon prefix the dashboard uses: a file whose name has a word starting
with "icon" and the prefix as a word (``hue:`` → ``hass-hue-icons.js``).
"""

from __future__ import annotations

import json
import os
import re
from typing import Any

from homeassistant.core import HomeAssistant

_CUSTOM = re.compile(r"custom:([a-z0-9_-]+)")
_ICON = re.compile(r'"icon":\s*"([a-z0-9_-]+):')
_BUILT_IN_ICONS = {"mdi", "hass", "hassio"}


def _norm(text: str) -> str:
    return text.lower().replace("_", "-")


async def async_resources(hass: HomeAssistant) -> list[dict[str, Any]]:
    """The resources (url, type) in Home Assistant's order."""
    data = hass.data.get("lovelace")
    collection = getattr(data, "resources", None)
    if collection is None:
        return []
    await collection.async_get_info()
    return [{"url": item.get("url", ""), "type": item.get("type", "")} for item in collection.async_items()]


async def async_dashboard_config(hass: HomeAssistant, url_path: str) -> dict[str, Any] | None:
    data = hass.data.get("lovelace")
    dashboards = getattr(data, "dashboards", None) or {}
    key = None if url_path in ("", "lovelace") else url_path
    dashboard = dashboards.get(key)
    if dashboard is None:
        return None
    try:
        return await dashboard.async_load(False)
    except Exception:  # noqa: BLE001 - a dashboard that does not load needs nothing
        return None


def _file_size(hass: HomeAssistant, url: str) -> int | None:
    path = url.split("?")[0]
    if path.startswith("/hacsfiles/"):
        file = hass.config.path("www", "community", *path[len("/hacsfiles/"):].split("/"))
    elif path.startswith("/local/"):
        file = hass.config.path("www", *path[len("/local/"):].split("/"))
    else:
        return None
    try:
        return os.path.getsize(file)
    except OSError:
        return None


def needed_by(config: dict[str, Any] | None, url: str) -> bool:
    """Whether a dashboard (its config) needs the resource at url."""
    if config is None:
        return True
    text = json.dumps(config)
    types = {_norm(t) for t in _CUSTOM.findall(text)}
    prefixes = {_norm(p) for p in _ICON.findall(text)} - _BUILT_IN_ICONS
    keys = {
        _norm(key)
        for key, value in config.items()
        if key != "views" and not (isinstance(value, dict) and value.get("enabled") is False)
    }
    path = url.split("?")[0].strip("/").split("/")
    names = {_norm(p) for p in path[-2:]}
    names |= {_norm(os.path.splitext(p)[0]) for p in path[-1:]}
    names |= {n.removeprefix("lovelace-") for n in names}
    names |= {n.removesuffix("-card").removesuffix("-bundle") for n in names}
    names.discard("")
    words = set(_norm(os.path.splitext(path[-1])[0]).split("-"))
    if prefixes & words and any(w.startswith("icon") for w in words):
        return True
    for name in names:
        if name in keys or any(t == name or t.startswith(name + "-") or name.startswith(t) for t in types):
            return True
    return False


async def async_resource_list(hass: HomeAssistant, dashboard: str) -> list[dict[str, Any]]:
    """Resources with their size on disk (when known) and whether the dashboard needs them."""
    resources = await async_resources(hass)
    config = await async_dashboard_config(hass, dashboard)
    sizes = await hass.async_add_executor_job(lambda: [_file_size(hass, r["url"]) for r in resources])
    return [
        {**r, "size": size, "needed": needed_by(config, r["url"])}
        for r, size in zip(resources, sizes, strict=True)
    ]
