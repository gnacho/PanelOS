"""Binary sensor of a panel: someone in front of it (proximity sensor)."""

from __future__ import annotations

from homeassistant.components.binary_sensor import (
    BinarySensorDeviceClass,
    BinarySensorEntity,
    BinarySensorEntityDescription,
)
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from . import PanelOSConfigEntry
from .entity import PanelOSEntity

DESCRIPTIONS = {
    "presence": BinarySensorEntityDescription(
        key="presence", translation_key="presence", device_class=BinarySensorDeviceClass.OCCUPANCY
    ),
}


async def async_setup_entry(
    hass: HomeAssistant, entry: PanelOSConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    entry.runtime_data.add_platform(
        "binary_sensor", async_add_entities, lambda panel, key: PanelOSBinarySensor(panel, DESCRIPTIONS[key])
    )


class PanelOSBinarySensor(PanelOSEntity, BinarySensorEntity):
    @property
    def is_on(self) -> bool | None:
        return self._value if isinstance(self._value, bool) else None
