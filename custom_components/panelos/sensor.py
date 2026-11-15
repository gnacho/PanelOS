"""Sensors of a panel: illuminance, SoC temperature, Wi-Fi signal, IP address, app version.

How much the first three must change before the panel sends a new value is
set in the integration's options (sensor reporting).
"""

from __future__ import annotations

from homeassistant.components.sensor import (
    SensorDeviceClass,
    SensorEntity,
    SensorEntityDescription,
    SensorStateClass,
)
from homeassistant.const import LIGHT_LUX, SIGNAL_STRENGTH_DECIBELS_MILLIWATT, EntityCategory, UnitOfTemperature
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from . import PanelOSConfigEntry
from .entity import PanelOSEntity

DESCRIPTIONS = {
    "illuminance": SensorEntityDescription(
        key="illuminance",
        translation_key="illuminance",
        device_class=SensorDeviceClass.ILLUMINANCE,
        state_class=SensorStateClass.MEASUREMENT,
        native_unit_of_measurement=LIGHT_LUX,
        suggested_display_precision=0,
    ),
    "soc_temperature": SensorEntityDescription(
        key="soc_temperature",
        translation_key="soc_temperature",
        device_class=SensorDeviceClass.TEMPERATURE,
        state_class=SensorStateClass.MEASUREMENT,
        native_unit_of_measurement=UnitOfTemperature.CELSIUS,
        suggested_display_precision=1,
        entity_category=EntityCategory.DIAGNOSTIC,
    ),
    "wifi_signal": SensorEntityDescription(
        key="wifi_signal",
        translation_key="wifi_signal",
        device_class=SensorDeviceClass.SIGNAL_STRENGTH,
        state_class=SensorStateClass.MEASUREMENT,
        native_unit_of_measurement=SIGNAL_STRENGTH_DECIBELS_MILLIWATT,
        suggested_display_precision=0,
        entity_category=EntityCategory.DIAGNOSTIC,
    ),
    "ip_address": SensorEntityDescription(
        key="ip_address",
        translation_key="ip_address",
        icon="mdi:ip-network",
        entity_category=EntityCategory.DIAGNOSTIC,
    ),
    "app_version": SensorEntityDescription(
        key="app_version",
        translation_key="app_version",
        icon="mdi:package-variant",
        entity_category=EntityCategory.DIAGNOSTIC,
    ),
}

_TEXT = frozenset({"ip_address", "app_version"})


async def async_setup_entry(
    hass: HomeAssistant, entry: PanelOSConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    entry.runtime_data.add_platform("sensor", async_add_entities, lambda panel, key: PanelOSSensor(panel, DESCRIPTIONS[key]))


class PanelOSSensor(PanelOSEntity, SensorEntity):
    @property
    def native_value(self) -> float | str | None:
        value = self._value
        if self._key in _TEXT:
            return value if isinstance(value, str) and value else None
        return value if isinstance(value, (int, float)) and not isinstance(value, bool) else None
