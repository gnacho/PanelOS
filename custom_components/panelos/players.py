"""The media players for a panel's music page picker: ``GET /api/panelos/players`` with the panel's
login. A panel logs in as an ordinary (non-administrator) user, so it cannot render a template
(administrators only); this answers the same short list: every available media player with the
few attributes the picker shows, much smaller than the whole state list.
"""

from __future__ import annotations

from aiohttp import web

from homeassistant.components.http import HomeAssistantView
from homeassistant.core import HomeAssistant

_ATTRS = ("mass_player_type", "media_title", "media_artist", "device_class")


class PlayersView(HomeAssistantView):
    """GET /api/panelos/players: the available media players (entity id, state, a few attributes)."""

    url = "/api/panelos/players"
    name = "api:panelos:players"

    async def get(self, request: web.Request) -> web.Response:
        hass: HomeAssistant = request.app["hass"]
        players = []
        for state in hass.states.async_all("media_player"):
            if state.state == "unavailable":
                continue
            attributes = {"friendly_name": state.name, "group_members": list(state.attributes.get("group_members") or [])}
            for key in _ATTRS:
                if state.attributes.get(key) is not None:
                    attributes[key] = state.attributes[key]
            players.append({"entity_id": state.entity_id, "state": state.state, "attributes": attributes})
        return self.json(players)


def async_register_players_view(hass: HomeAssistant) -> None:
    hass.http.register_view(PlayersView)
