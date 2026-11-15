# Home Assistant integration: `panelos`

Custom integration for panels running PanelOS (`custom_components/panelos` in this repository). With it a panel is a device of its own in Home Assistant:

- **The panel's device** (maker PanelOS, model for example NSPanel Pro 86 Gen1) with the display, the settings and the sensors as entities (see [Entities](#entities)).
- **Dashboard mode settings** (integration options): which Home Assistant dashboard the panel's web view shows, with or without its header and sidebar.
- **Assistant mode settings** (integration options): weather entity, media player, quick tiles (lights, switches, shutters, fans, thermostats, scenes, scripts, sensors, input booleans), full-screen conversation on/off.
- **Sensor reporting** (integration options): how much the illuminance, SoC temperature and Wi-Fi signal must change before the panel sends a new value (see below).
- **The PanelOS page** in the sidebar (administrators): every setting of a panel in one place, beside a live picture of its screen (see [The PanelOS page](#the-panelos-page)).
- **Critical alerts and status screens**, shared by every panel (each with its own panels on or off): the integration follows their sensors and shows them on the panels (see [Alerts and status screens](#alerts-and-status-screens)).
- **Actions** that put something on a panel's screen (administrators and automations only):
  - `panelos.show_view`: `view` = `home`, `dashboard`, `weather`, `media` or `camera` (with `entity_id: camera.…`), `seconds` (default 30, 0 = until someone touches the screen), `panel_id` (empty = every panel).
  - `panelos.show_message`: `title` (up to 100 characters), `message` (up to 1000), `icon` (`mdi:…`), `seconds`, `chime`, `panel_id`.
  - `panelos.show_status` / `panelos.clear_status`: `screen` = the ID of a status screen whose trigger is "Automation" (as the PanelOS page shows it).
  The view and message actions fail with an error if no panel is connected.

The voice satellite stays on Home Assistant's built-in ESPHome integration: the panel is an ESPHome device for it, named after the panel with "Voice" added, and shown as connected via the panel's device. Without this integration the panel still works (native dashboard, voice through ESPHome), but its display and settings have no entities in Home Assistant.

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
5. From the unpacked ZIP, copy the folder `custom_components/panelos` into that `custom_components` folder. The result is `config/custom_components/panelos`, with the `__init__.py` and `manifest.json` files directly inside.
6. Restart Home Assistant: **Settings → System**, the power icon at the top right, **Restart Home Assistant**.

## Upgrading from 0.0.3

Up to PanelOS 0.0.3 the integration was called NSPanel Pro (`nspanel_pro`); since 0.0.4 it is PanelOS (`panelos`), and the panel is a device of its own.

1. Write the new image to the panel (install guide, "Updating later").
2. Install PanelOS as above, then remove the old **NSPanel Pro** integration (**Settings → Devices & services**, its entry, **Delete**) and the folder `custom_components/nspanel_pro`.
3. Restart Home Assistant and add PanelOS ([Set up](#set-up)); set its options again (weather, media player, tiles, dashboard).

The panel's settings and sensors move from its ESPHome device to the PanelOS device and keep their entity ids, history and settings; the ESPHome device keeps the voice satellite.

## Set up

1. **Settings → Devices & services → Add integration**, search for **PanelOS** and select it.
2. Panel ID: the panel's network name, the same as its ESPHome device's (`nspanel-pro` by default). Then choose the weather entity, the media player and the quick tiles.
3. The panel's device appears with its entities; they become available as soon as the panel connects. Switch the panel to the mode you want with its **Display mode** select (or on the panel: settings → Display mode).

Options can be changed any time (**Configure** on the integration entry, then **Dashboard mode**, **Assistant mode** or **Sensor reporting**); the panel follows at once.

## Entities

On the panel's device (this integration):

| Entity | Kind | What it does |
|---|---|---|
| Screen | switch | Off starts the screensaver now, on wakes the panel |
| Dashboard | select | The dashboards the panel's Home Assistant user may open |
| Theme | select | Dark, Light or Auto (by the light in the room) |
| Screensaver after | select | Never, 30 s … 30 min |
| Screensaver style | select | Clock or Screen off |
| Display mode | select | Dashboard or Assistant |
| Brightness | number | Backlight when automatic brightness is off, 1–100 % |
| Automatic brightness | switch | Backlight follows the light in the room |
| Wake on approach | switch | The proximity sensor wakes the screen |
| Illuminance | sensor | Light at the panel, lx |
| Presence | binary sensor | Someone in front of the panel (proximity sensor) |
| SoC temperature, Wi-Fi signal, IP address, App version | sensors | Diagnostics |
| Restart app, Reboot | buttons | |

On the voice device (ESPHome): the Assist satellite, Volume, Mute microphone, and the Wake level, Wake SNR and Wake score sensors (sent with every wake word, see [app.md](app.md#esphome-native-api)), plus the selects Home Assistant creates for satellites (Assistant, Wake word, Finished speaking detection).

Up to PanelOS 0.0.3 the panel's display, settings and sensors were on the ESPHome device. The integration takes these entities over with the same entity ids, history and settings. If the panel still runs an older PanelOS, they stay on the ESPHome device until it is updated, then move.

## The PanelOS page

**PanelOS** in the sidebar (with the PanelOS mark as its icon) is a page of its own in the PanelOS look: Space Grotesk headings, Noto Sans text, Signal red only for what is lit or active, Graphite or Paper ground following Home Assistant's dark or light mode. It holds every panel: on a wide screen a menu on the left (the PanelOS logo, a panel switcher, the sections, All panels, Add a panel) with the selected panel's screen beside it as a live picture; on a medium screen a top bar with section pills; on a phone a bottom bar and a small floating screen. Editors open as drawers (wide) or sheets (phone); less used settings are behind **More options**, explanations behind the (i) buttons, and no text is ever cut.

- **Overview**: the panel at a glance.
- **Pages**: the pages in swipe order (the first is the start page), each with its look (drawn as a small picture) and its options: a weather page's background and widgets on its 4 × 4 grid, the player and the turntable's wood, the timers page's functions and colours, the quick controls' entities and the mixing desk's labels, the cameras' fit, start view and doorbell, the calendars written on the wall calendar, the pager's finish. A page type can be added more than once (several weather pages, each with its own background and widgets); a page can be hidden without losing its settings.
- **Alerts**: critical alerts and status screens (shared by every panel, see below), with a test on one panel.
- **Voice**, **Display** (theme, brightness, screensaver after and when, do not disturb, new notifications) and **Device** (sensor reporting, touch calibration, restart, reboot, name and area).
- **All panels**: what each panel can do, and **Copy settings** to other panels (each part copied, adapted to what the target can do, or skipped, with the reason).
- **Add a panel** starts the integration's setup for another panel, with its defaults or a copy of an existing panel.

What a panel can do (its touch screen, pictures, web view, video, microphone, speaker, sensors) comes from the panel itself (`caps` in its device report); sections, page types, looks and options it cannot do are left out or shown greyed with the reason. "Sending to the panel…" turns into "Up to date on the panel" once the integration has stored the change and sent it on.

The page uses the websocket commands `panelos/page/subscribe`, `panelos/page/options`, `panelos/page/command`, `panelos/page/screen`, `panelos/page/resources`, `panelos/page/house` (alerts and status screens), `panelos/page/test` (show one on a panel for a few seconds) and `panelos/page/touch_test` (administrators only); the panel sends the picture as a PNG with `panelos/screen` when asked (command `screenshot`).

## Alerts and status screens

Stored once for all panels (`.storage/panelos.house`), each item with the panels it appears on:

- **Critical alerts**: binary sensors that count as critical (suggested by their device class: smoke, carbon monoxide, gas, water leak, problem, tamper, safety). While one is on, every panel it is shown on takes the whole screen in red with its sound, even in do not disturb, until the sensor clears; holding the button silences it when the alert allows holding.
- **Status screens**: a state at home shown on the panels in its own colour (a room in use, a call going on, the robot vacuum at work …), with the panel's own words for its kind unless it has its own headline and line. Triggers: an entity's state (on, off, above or below a number, equal to a text), "after" another status screen ends (with a countdown), or an automation (`panelos.show_status` / `panelos.clear_status`).

A panel that connects gets what is active at that moment.

## Dashboard mode

Dashboard mode is a secondary function. The panel is made first of all for the assistant: the voice satellite and the assistant mode's home screen. A Home Assistant dashboard in the web view works within the limits of the panel's hardware (four Cortex-A35 cores, a Mali-G31 GPU, 2 GB of RAM), so a dashboard made for the panel should be light and simple: a few cards on a view, few custom cards, no heavy effects (blur behind bars, large animations), live camera streams only where they matter. A dashboard that is pleasant on a computer can take tens of seconds to load on the panel and scroll slowly.

The panel shows the dashboard as a browser does, custom cards included. It is logged in as its own Home Assistant user, so that user must be allowed to see the dashboard: a dashboard for administrators only, or views shown only to some users, stay empty on the panel.

While a dashboard loads, the panel shows its loading screen (the PanelOS mark writing itself in Morse) over Home Assistant's own start-up and reveals the dashboard once it has settled: cards drawn, icons in, nothing changing any more (at most 45 s).

Options for dashboard mode, besides the dashboard, header and sidebar:

- **Size** (`zoom`, 50–150 %, default 100): how large the dashboard is drawn; below 100 % more of it fits on the screen. It adds no extra step (WebKit lays the page out at that size), but below 100 % more cards are in view to lay out and draw, and icons and pictures are scaled. A dashboard designed for 480 × 480 at 100 % is lighter and sharper than a scaled one; use Size for a dashboard made for other screens.
- **Loading screen** (`loading_screen`, default on): off, Home Assistant's own start-up shows instead and the dashboard appears while it loads, sooner but unfinished at first.
- **Leave out unused card resources** (`skip_unused_resources`, default on): Home Assistant loads every card resource on every dashboard, used or not. The integration works out from the dashboard the panel shows which ones it needs (a resource counts as needed when its name matches a custom card type the dashboard uses or one of its top-level settings that is not switched off, for example `kiosk_mode`; an icon set counts when its file name has a word starting with "icon" and an icon prefix the dashboard uses, for example `hass-hue-icons.js` for `hue:` icons) and the panel leaves out the rest. It is worked out again when the panel shows another dashboard and when a dashboard is saved. On one dashboard this cut the first load from about 21 s to 15–17 s.
- **Always load these card resources** (`keep_resources`): for a resource a dashboard needs although its name matches none of its cards.

## Sensor reporting

Every value the panel sends is a row in the recorder's database (history, logbook, long-term statistics). The panel therefore sends a sensor only when it has changed by at least a set step from the value it sent last:

| Option | Default | Range | How it is measured |
|---|---|---|---|
| Illuminance | 15 % | 1–100 % | Checked twice a second; the step is a share of the last value sent, and at least a third of the percentage in lux (5 lx at 15 %), so the noise of a dark room is not sent. A light switched on shows up at once. |
| SoC temperature | 1.0 °C | 0.1–10 °C | Read every minute, averaged over about five minutes (the sensor works in steps of about 0.45 °C and flips between neighbouring steps). |
| Wi-Fi signal | 3 dB | 1–20 dB | Read every minute, averaged over about five minutes. |

Lower values give a finer history and automations that react to smaller changes, at the cost of more database writes. Higher values mean fewer writes, and only larger changes show up. On one panel these three sensors sent about 3000 values a day before; replaying a day of its readings with the defaults gave about 200. The panel's own automatic brightness, theme and Wi-Fi roaming use the full readings and do not depend on these options. Without the integration the panel uses the defaults.

## How it talks to the panel

The panel uses its own WebSocket connection to Home Assistant (its own user, no administrator rights needed):

- It subscribes with `{"type": "panelos/subscribe", "panel_id": "nspanel-pro"}`. Events carry `{"config": {...}}` (now and after every options change; `{}` when there is no entry for the panel; the pages, `screensaver`, `dnd`, `popups`, the reporting steps as `illuminance_change`, `soc_temperature_change` and `wifi_signal_change`), `{"show": {...}}` (an action call, a status screen `status` / `status_clear`, a critical alert `critical` / `critical_clear`, or `touch_test`) or `{"command": {"key": "theme", "value": "Dark"}}` (an entity of the panel's device was set; the keys are the ones below).
- It reports with `{"type": "panelos/report", "panel_id": "nspanel-pro", "device": {...}, "options": {...}, "states": {...}}`: the device (name, maker, model, version, MAC, what it can do as `caps`) once per connection, the options of the selects and the entity states (`screen`, `dashboard`, `theme`, `brightness`, `auto_brightness`, `screensaver`, `screensaver_style`, `proximity_wake`, `display_mode`, `illuminance`, `presence`, `soc_temperature`, `wifi_signal`, `ip_address`, `app_version`) when they change. Only the connection that holds the panel's subscription may report for it.

## Example: doorbell

```yaml
actions:
  - action: panelos.show_view
    data:
      view: camera
      entity_id: camera.front_door
      seconds: 45
```

## Licence

AGPL-3.0-or-later, like the rest of the project.
