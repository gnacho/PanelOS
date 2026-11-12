"""Config and options flow for the NSPanel Pro (open firmware) integration."""

from __future__ import annotations

from typing import Any

import voluptuous as vol

from homeassistant.config_entries import ConfigEntry, ConfigFlow, ConfigFlowResult, OptionsFlow
from homeassistant.core import callback
from homeassistant.helpers import selector

from .const import (
    CONF_MEDIA_PLAYER,
    CONF_PANEL_ID,
    CONF_SHOW_CONVERSATION,
    CONF_TILES,
    CONF_WEATHER,
    DEFAULT_PANEL_ID,
    DOMAIN,
    REPORTING,
)


def _assistant_schema(defaults: dict[str, Any]) -> vol.Schema:
    return vol.Schema(
        {
            vol.Optional(CONF_WEATHER, description={"suggested_value": defaults.get(CONF_WEATHER)}): selector.EntitySelector(
                selector.EntitySelectorConfig(domain="weather")
            ),
            vol.Optional(
                CONF_MEDIA_PLAYER, description={"suggested_value": defaults.get(CONF_MEDIA_PLAYER)}
            ): selector.EntitySelector(selector.EntitySelectorConfig(domain="media_player")),
            vol.Optional(CONF_TILES, default=defaults.get(CONF_TILES, [])): selector.EntitySelector(
                selector.EntitySelectorConfig(
                    domain=["light", "switch", "cover", "fan", "input_boolean", "scene", "script", "sensor", "climate"],
                    multiple=True,
                )
            ),
            vol.Optional(CONF_SHOW_CONVERSATION, default=defaults.get(CONF_SHOW_CONVERSATION, True)): bool,
        }
    )


def _reporting_schema(defaults: dict[str, Any]) -> vol.Schema:
    return vol.Schema(
        {
            vol.Required(key, default=defaults.get(key, default)): selector.NumberSelector(
                selector.NumberSelectorConfig(
                    min=low, max=high, step=step, unit_of_measurement=unit, mode=selector.NumberSelectorMode.SLIDER
                )
            )
            for key, (default, low, high, step, unit) in REPORTING.items()
        }
    )


class NSPanelProConfigFlow(ConfigFlow, domain=DOMAIN):
    """Add a panel."""

    VERSION = 1

    async def async_step_user(self, user_input: dict[str, Any] | None = None) -> ConfigFlowResult:
        if user_input is not None:
            panel_id = user_input[CONF_PANEL_ID].strip()
            await self.async_set_unique_id(panel_id)
            self._abort_if_unique_id_configured()
            data = {CONF_PANEL_ID: panel_id}
            options = {k: v for k, v in user_input.items() if k != CONF_PANEL_ID}
            return self.async_create_entry(title=f"NSPanel Pro ({panel_id})", data=data, options=options)
        schema = vol.Schema({vol.Required(CONF_PANEL_ID, default=DEFAULT_PANEL_ID): str}).extend(
            _assistant_schema({}).schema
        )
        return self.async_show_form(step_id="user", data_schema=schema)

    @staticmethod
    @callback
    def async_get_options_flow(config_entry: ConfigEntry) -> OptionsFlow:
        return NSPanelProOptionsFlow()


class NSPanelProOptionsFlow(OptionsFlow):
    """Change what the panel's assistant mode shows and how its sensors report."""

    async def async_step_init(self, user_input: dict[str, Any] | None = None) -> ConfigFlowResult:
        return self.async_show_menu(step_id="init", menu_options=["assistant", "sensor_reporting"])

    async def async_step_assistant(self, user_input: dict[str, Any] | None = None) -> ConfigFlowResult:
        schema = _assistant_schema(dict(self.config_entry.options))
        if user_input is not None:
            return self._save(schema, user_input)
        return self.async_show_form(step_id="assistant", data_schema=schema)

    async def async_step_sensor_reporting(self, user_input: dict[str, Any] | None = None) -> ConfigFlowResult:
        schema = _reporting_schema(dict(self.config_entry.options))
        if user_input is not None:
            return self._save(schema, user_input)
        return self.async_show_form(step_id="sensor_reporting", data_schema=schema)

    def _save(self, schema: vol.Schema, user_input: dict[str, Any]) -> ConfigFlowResult:
        # A step replaces only its own fields (a cleared entity field is
        # missing from user_input and must not keep its old value).
        own = {str(key) for key in schema.schema}
        kept = {k: v for k, v in self.config_entry.options.items() if k not in own}
        return self.async_create_entry(data={**kept, **user_input})
