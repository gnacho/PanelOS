"""Timer tools for voice assistants: a timer on a PanelOS panel, asked from any device.

Home Assistant's own timer tools always use the device the user talks to. These tools put a timer on a panel
named by its room or name ("set a 10 minute timer on the kitchen panel"): it becomes Home Assistant's timer
of that panel's voice device, so the panel shows and rings it and handles it as its own, and voice on any device
can list or cancel it. Agents that use the Assist API get the tools without any setup.
"""

from __future__ import annotations

import inspect
from typing import Any

from homeassistant.components.intent.const import TIMER_DATA
from homeassistant.components.llm import LLMTools
from homeassistant.core import HomeAssistant, callback
from homeassistant.helpers import area_registry as ar, config_validation as cv, device_registry as dr
from homeassistant.helpers.llm import LLM_API_ASSIST, LLMContext, Tool, ToolAnnotations, ToolInput, ToolResult

try:  # Home Assistant's schema library since 2026.x
    import probatio as schema
except ImportError:  # pragma: no cover
    import voluptuous as schema

from .const import DOMAIN

PANEL_HELP = "The panel's room: one of the PanelOS panels named in the prompt (in that wording)."


def _norm(text: str) -> str:
    return " ".join(text.casefold().replace("_", " ").split())


def _panels(hass: HomeAssistant) -> list[tuple[Any, list[str]]]:
    """The loaded panels with a voice device that has timers, and the names they answer to."""
    found = []
    areas = ar.async_get(hass)
    devices = dr.async_get(hass)
    for entry in hass.config_entries.async_entries(DOMAIN):
        panel = getattr(entry, "runtime_data", None)
        if panel is None or panel.timer_device_id() is None:
            continue
        names = {entry.title, panel.panel_id}
        for device in dr.async_entries_for_config_entry(devices, entry.entry_id):
            names.update(n for n in (device.name_by_user, device.name) if n)
            if device.area_id and (area := areas.async_get_area(device.area_id)):
                names.add(area.name)
                names.update(area.aliases)
        found.append((panel, sorted(n for n in names if n)))
    return found


def _room(hass: HomeAssistant, panel: Any) -> str:
    areas = ar.async_get(hass)
    for device in dr.async_entries_for_config_entry(dr.async_get(hass), panel.entry.entry_id):
        if device.area_id and (area := areas.async_get_area(device.area_id)):
            return area.name
    return panel.entry.title


def _words(text: str) -> list[str]:
    return [w for w in _norm(text).replace("panel", " ").split() if w]


def _named(name: str, said: list[str]) -> bool:
    # every word of the name starts a word the user said ("kitchen" in "kitchen's", inflected words too)
    words = _words(name)
    return bool(words) and all(any(s.startswith(w) for s in said) for w in words)


def _find(hass: HomeAssistant, said: str) -> tuple[Any | None, list[str]]:
    """The panel the user meant (by room or name), or None and the panels there are."""
    panels = _panels(hass)
    known = [_room(hass, p) for p, _ in panels]
    want = _norm(said or "")
    for panel, names in panels:
        if any(_norm(n) == want for n in names):
            return panel, known
    words = _words(want)
    hits = [panel for panel, names in panels if any(_named(n, words) for n in names)]
    if len(hits) == 1:
        return hits[0], known
    if not words and len(panels) == 1:
        # one panel in the home and no room said: "the panel" can only be that one
        return panels[0][0], known
    return None, known


def _clock(seconds: int) -> str:
    h, rest = divmod(max(0, int(seconds)), 3600)
    m, s = divmod(rest, 60)
    return f"{h}:{m:02d}:{s:02d}" if h else f"{m}:{s:02d}"


def _timers(hass: HomeAssistant, panel: Any) -> list[Any]:
    manager = hass.data.get(TIMER_DATA)
    device_id = panel.timer_device_id()
    if manager is None or device_id is None:
        return []
    return [t for t in manager.timers.values() if t.device_id == device_id and not t.conversation_command]


def _no_panel(said: str, known: list[str]) -> ToolResult:
    return ToolResult(
        data={"success": False, "error": f"No panel matches '{said}'", "panels": known},
        error=True,
    )


class StartPanelTimer(Tool):
    """A timer on a panel named by its room or name."""

    name = f"{DOMAIN}__StartPanelTimer"
    title = "Start a timer on a wall panel"
    description = (
        "Starts a timer on a PanelOS wall panel named by its room or name, also when the user talks to another "
        "device (for example 'set a 10 minute timer on the kitchen panel'). The panel shows the timer and "
        "rings when it ends."
    )
    parameters = schema.Schema(
        {
            schema.Required("panel", description=PANEL_HELP): cv.string,
            schema.Optional("hours"): cv.positive_int,
            schema.Optional("minutes"): cv.positive_int,
            schema.Optional("seconds"): cv.positive_int,
            schema.Optional("name", description="The timer's name, if the user gave one."): cv.string,
        }
    )
    annotations = ToolAnnotations(destructive=False, open_world=False)
    integration = DOMAIN

    async def async_call(self, hass: HomeAssistant, tool_input: ToolInput, llm_context: LLMContext) -> ToolResult:
        args = tool_input.tool_args
        said = str(args.get("panel", ""))
        panel, known = _find(hass, said)
        if panel is None:
            return _no_panel(said, known)
        hours, minutes, seconds = (int(args[k]) if args.get(k) else None for k in ("hours", "minutes", "seconds"))
        total = 3600 * (hours or 0) + 60 * (minutes or 0) + (seconds or 0)
        if total <= 0 or total > 86400:
            return ToolResult(data={"success": False, "error": "The timer needs a length up to 24 hours"}, error=True)
        manager = hass.data[TIMER_DATA]
        name = str(args.get("name") or "").strip() or None
        extra: dict[str, Any] = {"name": name}
        if "context" in inspect.signature(manager.start_timer).parameters:  # Home Assistant 2026.11+
            extra["context"] = llm_context.context
        timer_id = manager.start_timer(
            panel.timer_device_id(), hours, minutes, seconds, llm_context.language or hass.config.language, **extra
        )
        return ToolResult(
            data={"success": True, "panel": _room(hass, panel), "length": _clock(total), "name": name or "", "timer_id": timer_id}
        )


class CancelPanelTimer(Tool):
    """Cancel a panel's timer (by name when it has several)."""

    name = f"{DOMAIN}__CancelPanelTimer"
    title = "Cancel a timer on a wall panel"
    description = (
        "Cancels a timer on a PanelOS wall panel named by its room or name. With several timers on the panel, "
        "pass the timer's name, or all=true to cancel every timer on it."
    )
    parameters = schema.Schema(
        {
            schema.Required("panel", description=PANEL_HELP): cv.string,
            schema.Optional("name", description="The timer's name."): cv.string,
            schema.Optional("all", description="Cancel every timer on the panel."): cv.boolean,
        }
    )
    annotations = ToolAnnotations(destructive=True, idempotent=True, open_world=False)
    integration = DOMAIN

    async def async_call(self, hass: HomeAssistant, tool_input: ToolInput, llm_context: LLMContext) -> ToolResult:
        args = tool_input.tool_args
        said = str(args.get("panel", ""))
        panel, known = _find(hass, said)
        if panel is None:
            return _no_panel(said, known)
        timers = _timers(hass, panel)
        if name := _norm(str(args.get("name") or "")):
            timers = [t for t in timers if _norm(t.name or "") == name]
        room = _room(hass, panel)
        if not timers:
            return ToolResult(data={"success": False, "panel": room, "error": "No such timer on the panel"}, error=True)
        if len(timers) > 1 and not args.get("all"):
            return ToolResult(
                data={
                    "success": False,
                    "panel": room,
                    "error": "Several timers run on the panel: ask which one, or cancel all",
                    "timers": [{"name": t.name or "", "left": _clock(t.seconds_left)} for t in timers],
                },
                error=True,
            )
        manager = hass.data[TIMER_DATA]
        for t in timers:
            manager.cancel_timer(t.id)
        return ToolResult(data={"success": True, "panel": room, "cancelled": len(timers)})


class ListPanelTimers(Tool):
    """The timers running on the panels."""

    name = f"{DOMAIN}__ListPanelTimers"
    title = "List the timers on the wall panels"
    description = (
        "Lists the timers on PanelOS wall panels (one panel by its room or name, or all of them) with the exact time "
        "left, also for the panel the user talks to. Use it for questions like 'what timers do I have' or 'how much "
        "time is left'."
    )
    parameters = schema.Schema({schema.Optional("panel", description=PANEL_HELP): cv.string})
    annotations = ToolAnnotations(read_only=True, open_world=False)
    integration = DOMAIN

    async def async_call(self, hass: HomeAssistant, tool_input: ToolInput, llm_context: LLMContext) -> ToolResult:
        said = str(tool_input.tool_args.get("panel") or "")
        if said:
            panel, known = _find(hass, said)
            if panel is None:
                return _no_panel(said, known)
            panels = [panel]
        else:
            panels = [p for p, _ in _panels(hass)]
        return ToolResult(
            data={
                "panels": [
                    {
                        "panel": _room(hass, p),
                        "timers": [
                            {"name": t.name or "", "left": _clock(t.seconds_left), "length": _clock(t.seconds), "paused": not t.is_active}
                            for t in _timers(hass, p)
                        ],
                    }
                    for p in panels
                ]
            }
        )


@callback
def async_get_tools(hass: HomeAssistant, llm_context: LLMContext, api_id: str) -> LLMTools | None:
    """The panel timer tools for agents that use the Assist API, while a panel with timers is set up."""
    if api_id != LLM_API_ASSIST:
        return None
    panels = _panels(hass)
    if not panels:
        return None
    rooms = ", ".join(_room(hass, p) for p, _ in panels)
    prompt = (
        f"PanelOS wall panels: {rooms}. The {DOMAIN}__ timer tools start, list and cancel the timers of these panels, "
        "whether the user talks to the panel itself or to another device (for example 'set a timer on the kitchen "
        f"panel'). The time left on these timers can be told: when asked which timers run or how much time is left, "
        f"call {DOMAIN}__ListPanelTimers and answer from it."
    )
    return LLMTools(tools=[StartPanelTimer(), CancelPanelTimer(), ListPanelTimers()], prompt=prompt)
