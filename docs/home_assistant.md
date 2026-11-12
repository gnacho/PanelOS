# Home Assistant integration: `nspanel_pro`

Optional custom integration for panels running PanelOS (`custom_components/nspanel_pro` in this repository). The panel works without it (native dashboard, browser-free, settings and voice through Home Assistant's built-in ESPHome integration); the integration adds:

- **Assistant mode settings** (integration options): weather entity, media player, quick tiles (lights, switches, shutters, fans, thermostats, scenes, scripts, sensors, input booleans), full-screen conversation on/off.
- **Sensor reporting** (integration options): how much the illuminance, SoC temperature and Wi-Fi signal must change before the panel sends a new value (see below).
- **Actions** that put something on a panel's screen (administrators and automations only):
  - `nspanel_pro.show_view`: `view` = `home`, `dashboard`, `weather`, `media` or `camera` (with `entity_id: camera.…`), `seconds` (default 30, 0 = until someone touches the screen), `panel_id` (empty = every panel).
  - `nspanel_pro.show_message`: `title` (up to 100 characters), `message` (up to 1000), `icon` (`mdi:…`), `seconds`, `chime`, `panel_id`.
  Both fail with an error if no panel is connected.

## Install

Pick the way that matches your Home Assistant. You need an administrator user in Home Assistant.

### A. Marketplace (Home Assistant 2026.11 and later)

The Marketplace is built into Home Assistant since 2026.11 (it replaces HACS).

1. In Home Assistant open **Settings → Marketplace**.
2. The first time, connect it to GitHub when it asks: it shows a code, you open the GitHub page it links, sign in to GitHub and enter the code. A free GitHub account is enough.
3. Select **Add from link** and enter the repository URL: `https://github.com/lauden-io/PanelOS`
4. Open **PanelOS** in the list and select **Download**. (The repository must be public for this; the Marketplace cannot read private repositories.)
5. Restart Home Assistant: **Settings → System**, the power icon at the top right, **Restart Home Assistant**.
6. Continue with [Set up](#set-up).

### B. HACS (Home Assistant before 2026.11, with HACS installed)

1. Open **HACS** in the sidebar.
2. Select the **three dots** at the top right → **Custom repositories**.
3. Repository: `https://github.com/lauden-io/PanelOS`, Type: **Integration** → **Add**, then close the window.
4. Search for **PanelOS** in HACS, open it and select **Download**, then **Download** again.
5. Restart Home Assistant: **Settings → System**, the power icon at the top right, **Restart Home Assistant**.
6. Continue with [Set up](#set-up).

### C. By hand (any Home Assistant)

1. On the PanelOS GitHub page select **Code → Download ZIP** and unpack the file on your computer.
2. Make the Home Assistant configuration folder reachable from the computer. The simplest way: **Settings → Apps → App store → Samba share** → Install. On its Configuration tab set a user name and password, save, then **Start** it.
3. Open the configuration folder:
   - Windows: in File Explorer type `\\<Home Assistant IP address>\config` into the address bar and log in with the Samba user name and password.
   - Mac: Finder → Go → Connect to Server → `smb://<Home Assistant IP address>/config`.
4. In the `config` folder, create a folder named `custom_components` if it is not there yet.
5. From the unpacked ZIP, copy the folder `custom_components/nspanel_pro` into that `custom_components` folder. The result is `config/custom_components/nspanel_pro`, with the `__init__.py` and `manifest.json` files directly inside.
6. Restart Home Assistant: **Settings → System**, the power icon at the top right, **Restart Home Assistant**.

## Set up

1. **Settings → Devices & services → Add integration**, search for **NSPanel Pro (open firmware)** and select it.
2. Panel ID: the panel's ESPHome name (`nspanel-pro`). Then choose the weather entity, the media player and the quick tiles.
3. Switch the panel to assistant mode: its **Display mode** select on the ESPHome device page in Home Assistant, or gear icon → Display mode on the panel.

Options can be changed any time (**Configure** on the integration entry, then **Assistant mode** or **Sensor reporting**); the panel follows at once.

## Sensor reporting

Every value the panel sends is a row in the recorder's database (history, logbook, long-term statistics). The panel therefore sends a sensor only when it has changed by at least a set step from the value it sent last:

| Option | Default | Range | How it is measured |
|---|---|---|---|
| Illuminance | 15 % | 1–100 % | Checked twice a second; the step is a share of the last value sent, and at least a third of the percentage in lux (5 lx at 15 %), so the noise of a dark room is not sent. A light switched on shows up at once. |
| SoC temperature | 1.0 °C | 0.1–10 °C | Read every minute, averaged over about five minutes (the sensor works in steps of about 0.45 °C and flips between neighbouring steps). |
| Wi-Fi signal | 3 dB | 1–20 dB | Read every minute, averaged over about five minutes. |

Lower values give a finer history and automations that react to smaller changes, at the cost of more database writes. Higher values mean fewer writes, and only larger changes show up. On one panel these three sensors sent about 3000 values a day before; replaying a day of its readings with the defaults gave about 200. The panel's own automatic brightness, theme and Wi-Fi roaming use the full readings and do not depend on these options. Without the integration the panel uses the defaults.

## How it talks to the panel

The panel subscribes over its own WebSocket connection with `{"type": "nspanel_pro/subscribe", "panel_id": "nspanel-pro"}`. Events carry either `{"config": {...}}` (now and after every options change; `{}` when there is no entry for the panel; the reporting steps as `illuminance_change`, `soc_temperature_change` and `wifi_signal_change`) or `{"show": {...}}` (an action call). No administrator rights are needed for the panel's Home Assistant user.

## Example: doorbell

```yaml
actions:
  - action: nspanel_pro.show_view
    data:
      view: camera
      entity_id: camera.front_door
      seconds: 45
```

## Licence

AGPL-3.0-or-later, like the rest of the project.
