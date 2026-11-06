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
    """Change what the panel's assistant mode shows."""

    async def async_step_init(self, user_input: dict[str, Any] | None = None) -> ConfigFlowResult:
        if user_input is not None:
            return self.async_create_entry(data=user_input)
        return self.async_show_form(step_id="init", data_schema=_assistant_schema(dict(self.config_entry.options)))
