"""Music Assistant and the panel's player (alarms with music play through Music Assistant).

Music Assistant plays on a Home Assistant media player only once that player is added to its
"Home Assistant MediaPlayers" provider (domain ``hass_players``); it then makes its own player of it,
which Home Assistant shows as a ``music_assistant`` entity (the one ``music_assistant.play_media``
needs). This module adds the panel's player there through Home Assistant's own Music Assistant
connection (no extra login) and finds that entity, so nobody has to set it up by hand.
"""

from __future__ import annotations

import logging
from typing import Any

from homeassistant.core import HomeAssistant
from homeassistant.helpers import entity_registry as er

_LOGGER = logging.getLogger(__name__)

MA_DOMAIN = "music_assistant"
HASS_PLAYERS = "hass_players"


def _mass(hass: HomeAssistant) -> Any | None:
    """Home Assistant's Music Assistant client (None without the integration or before it is ready)."""
    for entry in hass.config_entries.async_entries(MA_DOMAIN):
        mass = getattr(getattr(entry, "runtime_data", None), "mass", None)
        if mass is not None:
            return mass
    return None


def _friendly(hass: HomeAssistant, entity_id: str) -> str:
    state = hass.states.get(entity_id)
    return str(state.attributes.get("friendly_name") or "") if state else ""


def _ma_player(hass: HomeAssistant, mass: Any, entity_id: str) -> Any | None:
    """The Music Assistant player made of a Home Assistant media player."""
    try:
        players = list(mass.players)
    except Exception:  # noqa: BLE001 - an old or not yet connected client
        return None
    name = _friendly(hass, entity_id)
    for player in players:
        extra = getattr(player, "extra_attributes", None) or getattr(player, "extra_data", None) or {}
        if getattr(player, "player_id", None) == entity_id or (isinstance(extra, dict) and extra.get("entity_id") == entity_id):
            return player
    for player in players:
        if name and getattr(player, "name", None) == name and HASS_PLAYERS in str(getattr(player, "provider", "")):
            return player
    return None


def ma_entity(hass: HomeAssistant, entity_id: str) -> str:
    """The Music Assistant entity for the panel's player ("" when Music Assistant does not have it)."""
    mass = _mass(hass)
    if mass is None or not entity_id:
        return ""
    player = _ma_player(hass, mass, entity_id)
    if player is None:
        return ""
    return er.async_get(hass).async_get_entity_id("media_player", MA_DOMAIN, player.player_id) or ""


class NotAllowed(Exception):
    """Music Assistant does not let Home Assistant's connection change its settings."""


def _ma_panel(hass: HomeAssistant) -> str:
    """The sidebar path of Music Assistant's own page in Home Assistant (its app), "" without one."""
    panels = hass.data.get("frontend_panels") or {}
    for path, panel in panels.items():
        config = getattr(panel, "config", None) or {}
        if "music_assistant" in str(config.get("addon", "")) or "music_assistant" in path:
            return path
    return ""


async def async_status(hass: HomeAssistant, entity_id: str) -> dict[str, Any]:
    """Whether Music Assistant knows the panel's player, for the PanelOS page (with the player's name and
    Music Assistant's page, for adding it there by hand)."""
    mass = _mass(hass)
    if mass is None:
        return {"available": False}
    base = {"available": True, "entity": entity_id, "player_name": _friendly(hass, entity_id) if entity_id else "", "ma_panel": _ma_panel(hass)}
    if not entity_id:
        return {**base, "added": False, "entity_ma": ""}
    return {**base, "added": _ma_player(hass, mass, entity_id) is not None, "entity_ma": ma_entity(hass, entity_id)}


async def async_add(hass: HomeAssistant, entity_id: str) -> dict[str, Any]:
    """Adds the panel's player to Music Assistant's Home Assistant player provider (if it is not there yet)."""
    mass = _mass(hass)
    if mass is None:
        raise ValueError("Music Assistant is not set up in Home Assistant")
    if not entity_id:
        raise ValueError("The panel has no media player yet")
    if _ma_player(hass, mass, entity_id) is not None:
        return await async_status(hass, entity_id)
    configs = await mass.config.get_provider_configs(provider_domain=HASS_PLAYERS, include_values=True)
    if not configs:
        raise ValueError('Music Assistant has no "Home Assistant MediaPlayers" provider; add it once in Music Assistant\'s settings')
    config = configs[0]
    # the provider's list of players: the multi-value setting whose choices are Home Assistant media players
    # (the choices come with the provider's setting definitions; the chosen ones with its stored values)
    entries = await mass.config.get_provider_config_entries(HASS_PLAYERS, instance_id=config.instance_id)
    key = None
    for entry in entries:
        options = [str(getattr(o, "value", "")) for o in (getattr(entry, "options", None) or [])]
        if getattr(entry, "multi_value", False) and (entity_id in options or any(o.startswith("media_player.") for o in options)):
            key = entry.key
            break
    if key is None:
        raise ValueError("Music Assistant's Home Assistant player list was not found")
    stored = (config.values or {}).get(key)
    current = list(getattr(stored, "value", None) or [])
    if entity_id not in current:
        try:
            await mass.config.save_provider_config(HASS_PLAYERS, {key: [*current, entity_id]}, instance_id=config.instance_id)
        except Exception as err:
            # Music Assistant lets only its administrators change providers; Home Assistant's
            # connection is often not one
            if "permission" in str(err).lower():
                raise NotAllowed(str(err)) from err
            raise
        _LOGGER.info("Added %s to Music Assistant's Home Assistant players", entity_id)
    return await async_status(hass, entity_id)
