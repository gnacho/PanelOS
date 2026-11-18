"""A notify entity per panel: ``notify.send_message`` puts a notification on that panel (its list and window)."""

from __future__ import annotations

from homeassistant.components.notify import NotifyEntity
from homeassistant.core import HomeAssistant
from homeassistant.helpers.device_registry import DeviceInfo
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from . import PanelOSConfigEntry
from .const import DOMAIN
from .panel import Panel


async def async_setup_entry(
    hass: HomeAssistant, entry: PanelOSConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    async_add_entities([PanelOSNotify(entry.runtime_data)])


class PanelOSNotify(NotifyEntity):
    """The panel's notifications."""

    _attr_has_entity_name = True
    _attr_translation_key = "notifications"

    def __init__(self, panel: Panel) -> None:
        self._panel = panel
        self._attr_unique_id = panel.unique_id("notify")
        self._attr_device_info = DeviceInfo(identifiers={(DOMAIN, panel.panel_id)})

    async def async_send_message(self, message: str, title: str | None = None) -> None:
        self.hass.data[DOMAIN]["notes"].add([self._panel.panel_id], {"title": title or "", "message": message})
