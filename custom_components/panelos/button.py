"""Buttons of a panel: restart the panel app, reboot the panel."""

from __future__ import annotations

from homeassistant.components.button import ButtonDeviceClass, ButtonEntity, ButtonEntityDescription
from homeassistant.const import EntityCategory
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from . import PanelOSConfigEntry
from .entity import PanelOSEntity

DESCRIPTIONS = {
    key: ButtonEntityDescription(
        key=key, translation_key=key, device_class=ButtonDeviceClass.RESTART, entity_category=EntityCategory.CONFIG
    )
    for key in ("restart_app", "reboot")
}


async def async_setup_entry(
    hass: HomeAssistant, entry: PanelOSConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    entry.runtime_data.add_platform("button", async_add_entities, lambda panel, key: PanelOSButton(panel, DESCRIPTIONS[key]))


class PanelOSButton(PanelOSEntity, ButtonEntity):
    @property
    def available(self) -> bool:
        return self._panel.connected

    async def async_press(self) -> None:
        self._command(None)
