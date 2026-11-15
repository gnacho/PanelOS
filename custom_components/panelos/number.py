"""Number of a panel: manual backlight brightness."""

from __future__ import annotations

from homeassistant.components.number import NumberEntity, NumberEntityDescription, NumberMode
from homeassistant.const import PERCENTAGE, EntityCategory
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from . import PanelOSConfigEntry
from .entity import PanelOSEntity

DESCRIPTIONS = {
    "brightness": NumberEntityDescription(
        key="brightness",
        translation_key="brightness",
        icon="mdi:brightness-6",
        entity_category=EntityCategory.CONFIG,
        native_min_value=1,
        native_max_value=100,
        native_step=1,
        native_unit_of_measurement=PERCENTAGE,
        mode=NumberMode.SLIDER,
    ),
}


async def async_setup_entry(
    hass: HomeAssistant, entry: PanelOSConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    entry.runtime_data.add_platform("number", async_add_entities, lambda panel, key: PanelOSNumber(panel, DESCRIPTIONS[key]))


class PanelOSNumber(PanelOSEntity, NumberEntity):
    @property
    def native_value(self) -> float | None:
        value = self._value
        return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None

    async def async_set_native_value(self, value: float) -> None:
        self._command(round(value))
