"""Uploaded sounds: the PanelOS page uploads a sound once (MP3, WAV, OGG or FLAC up to
10 MB); it is converted with Home Assistant's ffmpeg to 16-bit PCM WAV (mono, 32 kHz,
at most a minute: it loops while it rings) and kept under ``/config/panelos/sounds``.
The panels fetch the WAV (``GET /api/panelos/sound/<id>`` with their login) and keep a
copy, so it rings even when Home Assistant is away.
"""

from __future__ import annotations

import asyncio
import logging
import os
import secrets
import tempfile
import wave
from urllib.parse import unquote

from aiohttp import web

from homeassistant.components.ffmpeg import get_ffmpeg_manager
from homeassistant.components.http import HomeAssistantView
from homeassistant.core import HomeAssistant

from .const import DOMAIN, SOUND_KEEP_SECONDS, SOUND_UPLOAD_MAX

_LOGGER = logging.getLogger(__name__)

_TYPES = ("audio/", "application/ogg", "application/octet-stream")


def _wav_seconds(path: str) -> float:
    with wave.open(path, "rb") as w:
        return round(w.getnframes() / float(w.getframerate()), 1)


class SoundUploadView(HomeAssistantView):
    """POST /api/panelos/sounds (administrators): a sound file, converted and added to the list."""

    url = "/api/panelos/sounds"
    name = "api:panelos:sounds"

    async def post(self, request: web.Request) -> web.Response:
        hass: HomeAssistant = request.app["hass"]
        user = request.get("hass_user")
        if user is None or not user.is_admin:
            return self.json_message("Administrators only", 403)
        house = hass.data.get(DOMAIN, {}).get("house")
        if house is None:
            return self.json_message("PanelOS is not set up", 400)
        try:
            reader = await request.multipart()
        except (ValueError, AssertionError):
            return self.json_message("Not a file upload", 400)
        name = ""
        source = None
        try:
            while (part := await reader.next()) is not None:
                if part.name == "name":
                    name = (await part.text()).strip()[:40]
                elif part.name == "file":
                    ctype = (part.headers.get("Content-Type") or "").lower()
                    if ctype and not ctype.startswith(_TYPES):
                        return self.json_message("Not a sound file", 400)
                    name = name or os.path.splitext(unquote(part.filename or ""))[0].strip()[:40]
                    fd, source = tempfile.mkstemp(prefix="panelos-upload-")
                    size = 0
                    with os.fdopen(fd, "wb") as f:
                        while chunk := await part.read_chunk(256 * 1024):
                            size += len(chunk)
                            if size > SOUND_UPLOAD_MAX:
                                return self.json_message("Up to 10 MB, please", 400)
                            await hass.async_add_executor_job(f.write, chunk)
            if source is None:
                return self.json_message("No file", 400)
            sound_id = f"snd_{secrets.token_hex(5)}"
            target = house.sound_path(sound_id)
            await hass.async_add_executor_job(lambda: os.makedirs(os.path.dirname(target), exist_ok=True))
            # ffmpeg reads whatever it can (MP3, WAV, OGG, FLAC …) and writes what every panel plays
            proc = await asyncio.create_subprocess_exec(
                get_ffmpeg_manager(hass).binary, "-v", "error", "-y", "-i", source,
                "-t", str(SOUND_KEEP_SECONDS), "-vn", "-ac", "1", "-ar", "32000", "-sample_fmt", "s16",
                "-map_metadata", "-1", "-f", "wav", target,
                stdout=asyncio.subprocess.DEVNULL, stderr=asyncio.subprocess.PIPE,
            )
            try:
                _, err = await asyncio.wait_for(proc.communicate(), 60)
            except TimeoutError:
                proc.kill()
                return self.json_message("The sound took too long to convert", 400)
            if proc.returncode != 0 or not os.path.exists(target):
                _LOGGER.warning("PanelOS sound upload not converted: %s", err.decode(errors="replace")[-300:])
                return self.json_message("This file could not be read as a sound", 400)
            secs = await hass.async_add_executor_job(_wav_seconds, target)
            if secs < 0.2:
                await hass.async_add_executor_job(os.remove, target)
                return self.json_message("The sound is empty", 400)
            item = {"id": sound_id, "name": name or "Sound", "bytes": os.path.getsize(target), "secs": secs}
            await house.async_add_sound(item)
            return self.json(item)
        finally:
            if source:
                await hass.async_add_executor_job(lambda: os.path.exists(source) and os.remove(source))


class SoundFileView(HomeAssistantView):
    """GET /api/panelos/sound/<id>: an uploaded sound as WAV (the panels' copies, the page's preview)."""

    url = "/api/panelos/sound/{sound_id}"
    name = "api:panelos:sound"

    async def get(self, request: web.Request, sound_id: str) -> web.StreamResponse:
        hass: HomeAssistant = request.app["hass"]
        house = hass.data.get(DOMAIN, {}).get("house")
        known = {s["id"] for s in house.data.get("sounds", [])} if house else set()
        if sound_id not in known:
            return self.json_message("No such sound", 404)
        path = house.sound_path(sound_id)
        if not await hass.async_add_executor_job(os.path.exists, path):
            return self.json_message("No such sound", 404)
        return web.FileResponse(path, headers={"Content-Type": "audio/wav", "Cache-Control": "private, max-age=3600"})


def async_register_views(hass: HomeAssistant) -> None:
    hass.http.register_view(SoundUploadView)
    hass.http.register_view(SoundFileView)

