"""Base of the panel's entities: state from its reports, commands back to it."""

from __future__ import annotations

from typing import Any

from homeassistant.core import callback
from homeassistant.helpers.device_registry import DeviceInfo
from homeassistant.helpers.dispatcher import async_dispatcher_connect
from homeassistant.helpers.entity import Entity, EntityDescription

from .const import DOMAIN
from .panel import Panel


class PanelOSEntity(Entity):
    """An entity of a panel; available while the panel is connected."""

    _attr_has_entity_name = True
    _attr_should_poll = False

    def __init__(self, panel: Panel, description: EntityDescription) -> None:
        self.entity_description = description
        self._panel = panel
        self._attr_unique_id = panel.unique_id(description.key)
        self._attr_device_info = DeviceInfo(identifiers={(DOMAIN, panel.panel_id)})

    @property
    def _key(self) -> str:
        return self.entity_description.key

    @property
    def _value(self) -> Any:
        return self._panel.states.get(self._key)

    @property
    def available(self) -> bool:
        return self._panel.connected and self._key in self._panel.states

    async def async_added_to_hass(self) -> None:
        self.async_on_remove(async_dispatcher_connect(self.hass, self._panel.signal, self._panel_updated))

    @callback
    def _panel_updated(self, keys: set[str] | None) -> None:
        if keys is None or self._key in keys:
            self.async_write_ha_state()

    def _command(self, value: Any) -> None:
        self._panel.send_command(self._key, value)
