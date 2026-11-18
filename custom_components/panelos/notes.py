"""PanelOS's own notifications: from the panels' notify entities and the ``panelos.notify`` action.

They go into each target panel's notifications list and pop up in its window. The integration keeps them
until they are answered (a panel that comes back gets them again): OK or a button marks one read on that
panel, Dismiss removes it everywhere. A button fires ``panelos_notification_action``.
"""

from __future__ import annotations

import time
import uuid
from typing import Any

from homeassistant.core import HomeAssistant, callback
from homeassistant.helpers.dispatcher import async_dispatcher_send
from homeassistant.helpers.storage import Store

from .const import EVENT_NOTIFICATION_ACTION, NOTES_STORE, SIGNAL_SHOW

STORE_VERSION = 1
# a panel's list keeps the newest ones
MAX_NOTES = 50


class Notes:
    """The store of PanelOS notifications (one for the integration, each with its panels)."""

    def __init__(self, hass: HomeAssistant) -> None:
        self.hass = hass
        self._store: Store[dict[str, Any]] = Store(hass, STORE_VERSION, NOTES_STORE)
        self._items: list[dict[str, Any]] = []

    async def async_load(self) -> None:
        data = await self._store.async_load() or {}
        self._items = [i for i in data.get("items", []) if isinstance(i, dict) and i.get("id")]

    def _save(self) -> None:
        self._store.async_delay_save(lambda: {"items": self._items}, 1)

    def for_panel(self, panel_id: str) -> list[dict[str, Any]]:
        """A panel's notifications, newest first, each with its read state on that panel."""
        out = []
        for item in sorted(self._items, key=lambda i: i.get("created", 0), reverse=True):
            if panel_id in item.get("panels", []):
                out.append({k: v for k, v in item.items() if k not in ("panels", "read_by")} | {"read": panel_id in item.get("read_by", [])})
        return out[:MAX_NOTES]

    def request_for(self, panel_id: str, new: list[str] | None = None) -> dict[str, Any]:
        """The show request with a panel's list; `new` ones pop up (on a reconnect: the unread ones)."""
        notes = self.for_panel(panel_id)
        if new is None:
            new = [n["id"] for n in notes if not n["read"]]
        return {"type": "notes", "notes": notes, "new": new}

    @callback
    def _send(self, panels: set[str], new: list[str] | None = None) -> None:
        for panel_id in panels:
            async_dispatcher_send(self.hass, SIGNAL_SHOW, panel_id, self.request_for(panel_id, new if new is not None else []))

    @callback
    def add(self, panels: list[str], data: dict[str, Any]) -> str:
        """A new notification for these panels."""
        note_id = uuid.uuid4().hex[:12]
        item = {
            "id": note_id,
            "title": str(data.get("title") or ""),
            "message": str(data.get("message") or ""),
            "icon": str(data.get("icon") or ""),
            "level": "warning" if data.get("level") == "warning" else "info",
            "picture": str(data.get("picture") or ""),
            "buttons": [str(b) for b in (data.get("buttons") or [])][:3],
            "until_answered": bool(data.get("until_answered")),
            "created": int(time.time()),
            "panels": list(dict.fromkeys(panels)),
            "read_by": [],
        }
        self._items.append(item)
        # the oldest go when there are too many
        if len(self._items) > MAX_NOTES * 4:
            self._items = sorted(self._items, key=lambda i: i.get("created", 0))[-MAX_NOTES * 4:]
        self._save()
        self._send(set(item["panels"]), [note_id])
        return note_id

    @callback
    def answer(self, panel_id: str, note_id: str, answer: str, button: int | None = None) -> bool:
        """A panel's answer: "ok" / "read" (read there), "button" (the event, then read), "dismiss" (gone everywhere)."""
        item = next((i for i in self._items if i["id"] == note_id), None)
        if item is None:
            return False
        if answer == "dismiss":
            self._items.remove(item)
            self._save()
            self._send(set(item.get("panels", [])))
            return True
        if answer == "button" and button is not None and 0 <= button < len(item.get("buttons", [])):
            self.hass.bus.async_fire(
                EVENT_NOTIFICATION_ACTION,
                {"notification_id": note_id, "action": item["buttons"][button], "index": button, "panel_id": panel_id},
            )
        if panel_id not in item.setdefault("read_by", []):
            item["read_by"].append(panel_id)
            self._save()
            self._send({panel_id})
        return True
