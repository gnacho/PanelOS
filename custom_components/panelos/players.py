"""The media players for a panel's music page picker: ``GET /api/panelos/players`` with the panel's
login. A panel logs in as an ordinary (non-administrator) user, so it cannot render a template
(administrators only); this answers the same short list: every available media player with the
few attributes the picker shows, much smaller than the whole state list. A panel's own player in Music Assistant
carries ``sendspin_id`` (the panel's Sendspin client id), so each panel finds itself in the list.
"""

from __future__ import annotations

from aiohttp import web

from homeassistant.components.http import HomeAssistantView
from homeassistant.core import HomeAssistant
from homeassistant.helpers import entity_registry as er

from .const import DOMAIN

_ATTRS = ("mass_player_type", "media_title", "media_artist", "device_class")


class PlayersView(HomeAssistantView):
    """GET /api/panelos/players: the available media players (entity id, state, a few attributes)."""

    url = "/api/panelos/players"
    name = "api:panelos:players"

    async def get(self, request: web.Request) -> web.Response:
        hass: HomeAssistant = request.app["hass"]
        own = _panel_players(hass)
        players = []
        for state in hass.states.async_all("media_player"):
            if state.state == "unavailable":
                continue
            attributes = {"friendly_name": state.name, "group_members": list(state.attributes.get("group_members") or [])}
            for key in _ATTRS:
                if state.attributes.get(key) is not None:
                    attributes[key] = state.attributes[key]
            if state.entity_id in own:
                attributes["sendspin_id"] = own[state.entity_id]
            players.append({"entity_id": state.entity_id, "state": state.state, "attributes": attributes})
        return self.json(players)


def _panel_players(hass: HomeAssistant) -> dict[str, str]:
    """Music Assistant's entity of each panel's Sendspin player → that panel's Sendspin client id."""
    from .ma import _ma_player, _mass, sendspin_id  # noqa: PLC0415
    from .panel import panel_config  # noqa: PLC0415

    mass = _mass(hass)
    if mass is None:
        return {}
    registry = er.async_get(hass)
    own: dict[str, str] = {}
    for entry in hass.config_entries.async_entries(DOMAIN):
        try:
            entity = panel_config(entry).get("media_player") or ""
        except Exception:  # noqa: BLE001 - an entry being set up
            continue
        sid = sendspin_id(hass, entity)
        player = _ma_player(hass, mass, entity) if sid else None
        if player is None:
            continue
        ma_entity = registry.async_get_entity_id("media_player", "music_assistant", player.player_id)
        if ma_entity:
            own[ma_entity] = sid
    return own


def async_register_players_view(hass: HomeAssistant) -> None:
    hass.http.register_view(PlayersView)
