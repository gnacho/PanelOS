// The PanelOS page in Home Assistant's sidebar: every setting of a panel
// beside a live picture of its screen, in PanelOS's own look (brand guide:
// Signal red for what is active, Graphite / Paper grounds, Space Grotesk
// headings, Noto Sans text). Data and changes go over the websocket
// commands in page.py (panelos/page/*).

const PICTURE_EVERY_MS = 2500;
const PICTURE_AFTER_CHANGE_MS = 700;

// The PanelOS mark: dots always Signal, dashes the text colour of the ground.
const MARK_DOTS = "M2 8a2 2 0 1 0 4 0a2 2 0 1 0-4 0zM18 16a2 2 0 1 0 4 0a2 2 0 1 0-4 0z";
const MARK_DASHES = "M12 6h8a2 2 0 0 1 0 4h-8a2 2 0 0 1 0-4zM4 14h8a2 2 0 0 1 0 4H4a2 2 0 0 1 0-4z";
const ICON = {
  menu: "M3,6H21V8H3V6M3,11H21V13H3V11M3,16H21V18H3V16Z",
  panel: "M4,4H20A2,2 0 0,1 22,6V18A2,2 0 0,1 20,20H4A2,2 0 0,1 2,18V6A2,2 0 0,1 4,4M4,6V18H20V6H4Z",
  display: "M12,7A5,5 0 0,1 17,12A5,5 0 0,1 12,17A5,5 0 0,1 7,12A5,5 0 0,1 12,7M12,9A3,3 0 0,0 9,12A3,3 0 0,0 12,15A3,3 0 0,0 15,12A3,3 0 0,0 12,9M12,2L14.39,5.42C13.65,5.15 12.84,5 12,5C11.16,5 10.35,5.15 9.61,5.42L12,2M3.34,7L7.5,6.65C6.9,7.16 6.36,7.78 5.94,8.5C5.5,9.24 5.25,10 5.11,10.79L3.34,7M3.36,17L5.12,13.23C5.26,14 5.53,14.78 5.95,15.5C6.37,16.24 6.91,16.86 7.5,17.37L3.36,17M20.65,7L18.88,10.79C18.74,10 18.47,9.23 18.05,8.5C17.63,7.78 17.1,7.15 16.5,6.64L20.65,7M20.64,17L16.5,17.36C17.09,16.85 17.62,16.22 18.04,15.5C18.46,14.77 18.73,14 18.87,13.21L20.64,17M12,22L9.59,18.56C10.33,18.83 11.14,19 12,19C12.82,19 13.63,18.83 14.37,18.56L12,22Z",
  dashboard: "M13,3V9H21V3M13,21H21V11H13M3,21H11V15H3M3,13H11V3H3V13Z",
  assistant: "M12,2A3,3 0 0,1 15,5V11A3,3 0 0,1 12,14A3,3 0 0,1 9,11V5A3,3 0 0,1 12,2M19,11C19,14.53 16.39,17.44 13,17.93V21H11V17.93C7.61,17.44 5,14.53 5,11H7A5,5 0 0,0 12,16A5,5 0 0,0 17,11H19Z",
  sensors: "M3,3H5V19H21V21H3V3M7,17V10H9V17H7M11,17V6H13V17H11M15,17V13H17V17H15Z",
  device: "M12,15.5A3.5,3.5 0 0,1 8.5,12A3.5,3.5 0 0,1 12,8.5A3.5,3.5 0 0,1 15.5,12A3.5,3.5 0 0,1 12,15.5M19.43,12.97C19.47,12.65 19.5,12.33 19.5,12C19.5,11.67 19.47,11.34 19.43,11L21.54,9.37C21.73,9.22 21.78,8.95 21.66,8.73L19.66,5.27C19.54,5.05 19.27,4.96 19.05,5.05L16.56,6.05C16.04,5.66 15.5,5.32 14.87,5.07L14.5,2.42C14.46,2.18 14.25,2 14,2H10C9.75,2 9.54,2.18 9.5,2.42L9.13,5.07C8.5,5.32 7.96,5.66 7.44,6.05L4.95,5.05C4.73,4.96 4.46,5.05 4.34,5.27L2.34,8.73C2.21,8.95 2.27,9.22 2.46,9.37L4.57,11C4.53,11.34 4.5,11.67 4.5,12C4.5,12.33 4.53,12.65 4.57,12.97L2.46,14.63C2.27,14.78 2.21,15.05 2.34,15.27L4.34,18.73C4.46,18.95 4.73,19.03 4.95,18.95L7.44,17.94C7.96,18.34 8.5,18.68 9.13,18.93L9.5,21.58C9.54,21.82 9.75,22 10,22H14C14.25,22 14.46,21.82 14.5,21.58L14.87,18.93C15.5,18.67 16.04,18.34 16.56,17.94L19.05,18.95C19.27,19.03 19.54,18.95 19.66,18.73L21.66,15.27C21.78,15.05 21.73,14.78 21.54,14.63L19.43,12.97Z",
  info: "M11,9H13V7H11M12,20C7.59,20 4,16.41 4,12C4,7.59 7.59,4 12,4C16.41,4 20,7.59 20,12C20,16.41 16.41,20 12,20M12,2A10,10 0 0,0 2,12A10,10 0 0,0 12,22A10,10 0 0,0 22,12A10,10 0 0,0 12,2M11,17H13V11H11V17Z",
  close: "M19,6.41L17.59,5L12,10.59L6.41,5L5,6.41L10.59,12L5,17.59L6.41,19L12,13.41L17.59,19L19,17.59L13.41,12L19,6.41Z",
  plus: "M19,13H13V19H11V13H5V11H11V5H13V11H19V13Z",
  chevron: "M7.41,8.58L12,13.17L16.59,8.58L18,10L12,16L6,10L7.41,8.58Z",
};

const SECTIONS = [
  { id: "panel", title: "Panel", icon: "panel" },
  { id: "pages", title: "Pages", icon: "dashboard" },
  { id: "display", title: "Display", icon: "display" },
  { id: "dashboard", title: "Dashboard", icon: "dashboard" },
  { id: "assistant", title: "Assistant", icon: "assistant" },
  { id: "sensors", title: "Sensors", icon: "sensors" },
  { id: "device", title: "Device", icon: "device" },
];

// Explanations behind the (i) buttons; plain texts (only the panel is playful).
const INFO = {
  display: "How the panel looks and when its screen sleeps. Automatic brightness follows the light in the room; the Auto theme switches between dark and light by the same light.",
  dashboard: "Dashboard mode shows a Home Assistant dashboard in the panel's web view, custom cards included. The panel is logged in as its own Home Assistant user: dashboards for administrators only, or views shown only to some users, stay empty on it. \"Chosen on the panel\" leaves the choice to the panel's Dashboard setting.",
  resources: "Home Assistant loads every card resource on every dashboard, used or not; on the panel each one costs load time. The panel leaves out the ones the dashboard it shows does not need, worked out again whenever the dashboard changes: a resource counts as used when its name matches a card type or a setting of the dashboard. Switch one on to always load it.",
  assistant: "Assistant mode is the smart-display home screen: clock, weather, the media player and quick tiles, and the conversation full screen while you talk to it.",
  sensors: "Every value the panel sends is a row in the recorder's database. The panel sends a sensor only when it has changed by at least this much since the value it sent last. Lower: finer history, more writes. Higher: fewer writes, only larger changes show up.",
  device: "Restart app restarts the panel's software (a few seconds); Reboot restarts the whole panel. Both need a second tap.",
  preview: "The panel's screen as it is now, every few seconds while this page is open. Tap it to wake the screen.",
};

// The pages the panel can show, in the words of the page list.
const PAGE_NAMES = {
  home: "Home", weather: "Weather", clock: "Clock", music: "Music", timers: "Timers", thermostat: "Thermostat",
  quick: "Quick controls", camera: "Cameras", calendar: "Calendar", todo: "To-do", info: "Notifications",
};
const LOOKS = JSON.parse(atob("eyJ3ZWF0aGVyIjpbIkNhcmRzIiwiTGl2aW5nIHBob3RvIl0sIm11c2ljIjpbIlBsYWluIiwiRnVsbCBjb3ZlciIsIlR1cm50YWJsZSIsIkNhc3NldHRlIiwiSGFsZiBjYXNzZXR0ZSJdLCJ0aW1lcnMiOlsiRnVsbCBDaXJjbGUiLCJEb25lIHRvIGEgVHVybiIsIlNhbmRzIG9mIFRpbWUiLCJJbiB0aGUgRmFzdCBMYW5lIiwiRmxpcCBTaWRlIl0sInRoZXJtb3N0YXQiOlsiQ29tZm9ydCBab25lIiwiUHVzaCBNeSBCdXR0b25zIiwiT2xkIEZsYW1lIiwiSW4gdGhlIEZhc3QgTGFuZSJdLCJxdWljayI6WyJUaWxlcyIsIk1peGluZyBkZXNrIiwiRmxpZ2h0IGRlY2siXSwiY2FsZW5kYXIiOlsiUGxhaW4gU2FpbGluZyIsIk5vdyBCb2FyZGluZyIsIlNlaXplIHRoZSBEYXkiLCJIYW5nIGluIFRoZXJlIl0sInRvZG8iOlsiQ2xlYW4gU2xhdGUiLCJQYXBlciBUcmFpbCIsIlJvb3QgQWNjZXNzIiwiTGFiZWwgTWUgSW1wcmVzc2VkIiwiTm90ZSB0byBTZWxmIl0sImluZm8iOlsiSGVhZHMgVXAiLCJGcmlkZ2UgTWFnbmV0IiwiQmVlcCBCZWVwIiwiQnJlYWtpbmcgTmV3cyJdfQ=="));

// Home backgrounds (photo / drawn / line / synthwave), the synthwave scenes, the voice feedback looks.
const NAMES = JSON.parse(atob("eyJiZyI6WyJQaG90byBcdTAwYjcgbmF0dXJlIiwiUGhvdG8gXHUwMGI3IGNvdW50cnlzaWRlIiwiUGhvdG8gXHUwMGI3IGNpdHkiLCJEcmF3biBcdTAwYjcgY291bnRyeXNpZGUiLCJEcmF3biBcdTAwYjcgY2l0eSIsIkRyYXduIFx1MDBiNyBuZXV0cmFsIiwiRHJhd24gXHUwMGI3IGJpZyBjaXR5IiwiTGluZSIsIkluIHRoZSBGYXN0IExhbmUiXSwic2NlbmUiOlsiUmlkZSBpbnRvIHRoZSBTdW5zZXQiLCJDb25jcmV0ZSBKdW5nbGUiLCJMaWZlJ3MgYSBCZWFjaCIsIlN0YXJyeS1FeWVkIl0sInZvaWNlIjpbIlBsYWluIiwiVGVybWluYWwiLCJDb21pYyIsIk9zY2lsbG9zY29wZSJdfQ=="));

// Readable names of integrations, for entities with the same name.
const PLATFORMS = {
  met: "Met.no", open_meteo: "Open-Meteo", openweathermap: "OpenWeatherMap",
  accuweather: "AccuWeather", pirateweather: "Pirate Weather", music_assistant: "Music Assistant", cast: "Google Cast",
  esphome: "ESPHome", sonos: "Sonos", dlna_dmr: "DLNA", jellyfin: "Jellyfin", apple_tv: "Apple TV",
};
// The name of an integration for a label: a known one, else its domain in words.
const platformName = (p) => p && (PLATFORMS[p] || p.charAt(0).toUpperCase() + p.slice(1).replace(/_/g, " "));

const REPORTING = {
  illuminance_change: { label: "Illuminance", now: "illuminance", nowUnit: "lx" },
  soc_temperature_change: { label: "SoC temperature", now: "soc_temperature", nowUnit: "°C" },
  wifi_signal_change: { label: "Wi-Fi signal", now: "wifi_signal", nowUnit: "dBm" },
};

const STYLE = `
@font-face { font-family: "PanelOS Text"; src: url("{STATIC}/fonts/NotoSans-Latin.woff2") format("woff2"); font-weight: 400 700; font-display: swap; }
@font-face { font-family: "PanelOS Heading"; src: url("{STATIC}/fonts/SpaceGrotesk-Latin.woff2") format("woff2"); font-weight: 400 700; font-display: swap; }
:host {
  --signal: #FF4A1C;
  --signal-soft: rgba(255, 74, 28, 0.14);
  --ground: #F5F3EE; --surface: #FFFFFF; --raised: #EBE8E1; --line: #DAD6CD;
  --text: #101114; --muted: #5F5E5A; --faint: #9A978F; --ok: #1F9D55; --nav: #EFECE5;
  --shadow: 0 1px 2px rgba(16,17,20,0.06), 0 8px 24px rgba(16,17,20,0.06);
  --pattern: url("{STATIC}/pattern-light.svg");
  --logo: url("{STATIC}/logo.svg");
  display: block; min-height: 100vh; background: var(--ground); color: var(--text);
  font-family: "PanelOS Text", "Noto Sans", system-ui, sans-serif; font-size: 15px; line-height: 1.45;
  -webkit-font-smoothing: antialiased;
}
:host([dark]) {
  --ground: #121314; --surface: #1A1B1E; --raised: #232528; --line: #2A2C30;
  --text: #F5F3EE; --muted: #A3A19C; --faint: #6B6A67; --ok: #3CCB7F; --nav: #0E0F10;
  --shadow: none;
  --pattern: url("{STATIC}/pattern-dark.svg");
  --logo: url("{STATIC}/logo-dark.svg");
}
* { box-sizing: border-box; }
[hidden] { display: none !important; }
h1, h2, h3 { font-family: "PanelOS Heading", "Space Grotesk", "PanelOS Text", sans-serif; font-weight: 500; letter-spacing: -0.01em; margin: 0; }
button { font: inherit; color: inherit; }
svg { display: block; }

.shell { display: grid; grid-template-columns: 232px minmax(0, 1fr) 380px; min-height: 100vh; }
:host([size=medium]) .shell { grid-template-columns: minmax(0, 1fr) 340px; }
:host([size=narrow]) .shell { grid-template-columns: minmax(0, 1fr); }

/* navigation */
.nav { background: var(--nav); border-right: 1px solid var(--line); position: sticky; top: 0; height: 100vh; display: flex; flex-direction: column; }
:host([size=medium]) .nav, :host([size=narrow]) .nav { display: none; }
.brand { height: 128px; background: var(--pattern) center / cover; display: flex; align-items: flex-end; padding: 0 22px 20px; border-bottom: 1px solid var(--line); }
.brand i { display: block; width: 150px; height: 22px; background: var(--logo) left center / contain no-repeat; }
.links { padding: 18px 12px; display: flex; flex-direction: column; gap: 4px; }
.links button { display: flex; align-items: center; gap: 14px; border: 0; background: none; border-radius: 12px; padding: 11px 12px; cursor: pointer; text-align: left; color: var(--muted); font-size: 15px; }
.links button svg { width: 20px; height: 20px; fill: currentColor; flex: none; }
.links button:hover { background: var(--raised); color: var(--text); }
.links button.on { background: var(--signal-soft); color: var(--text); }
.links button.on svg { fill: var(--signal); }
.panels { padding: 0 12px 8px; display: flex; flex-direction: column; gap: 4px; }
.panels button { border: 1px solid var(--line); background: var(--surface); border-radius: 12px; padding: 9px 12px; text-align: left; cursor: pointer; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.panels button.on { border-color: var(--signal); }

/* top bar (medium / narrow) */
.top { display: none; position: sticky; top: 0; z-index: 3; background: var(--ground); border-bottom: 1px solid var(--line); }
:host([size=medium]) .top, :host([size=narrow]) .top { display: block; }
.top-row { display: flex; align-items: center; gap: 10px; height: 56px; padding: 0 16px; }
.top-row i { display: block; width: 124px; height: 18px; background: var(--logo) left center / contain no-repeat; }
.menu { display: none; border: 0; background: none; width: 40px; height: 40px; border-radius: 20px; margin-left: -8px; cursor: pointer; }
.menu svg { width: 24px; height: 24px; fill: var(--text); margin: auto; }
:host([narrow]) .menu { display: grid; }
.tabs { display: flex; gap: 6px; padding: 0 12px 10px; overflow-x: auto; scrollbar-width: none; }
.tabs::-webkit-scrollbar { display: none; }
.tabs button { border: 1px solid var(--line); background: var(--surface); border-radius: 999px; padding: 6px 14px; white-space: nowrap; cursor: pointer; color: var(--muted); font-size: 14px; }
.tabs button.on { border-color: var(--signal); color: var(--text); background: var(--signal-soft); }

/* content */
.content { padding: 28px 32px 64px; display: flex; flex-direction: column; gap: 22px; max-width: 760px; width: 100%; }
:host([size=narrow]) .content { padding: 16px 16px 48px; gap: 16px; }
.hero { display: flex; flex-direction: column; gap: 6px; padding: 4px 2px 6px; }
.hero h1 { font-size: 30px; line-height: 1.15; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
:host([size=narrow]) .hero h1 { font-size: 24px; }
.hero .meta { display: flex; flex-wrap: wrap; gap: 6px 14px; color: var(--muted); font-size: 14px; align-items: center; }
.state { display: inline-flex; align-items: center; gap: 7px; }
.state b { width: 8px; height: 8px; border-radius: 50%; background: var(--faint); }
.state.on b { background: var(--ok); }
.card { background: var(--surface); border: 1px solid var(--line); border-radius: 18px; padding: 8px 22px 10px; box-shadow: var(--shadow); scroll-margin-top: 120px; }
:host([size=narrow]) .card { padding: 6px 16px 8px; border-radius: 16px; }
.card-head { display: flex; align-items: center; gap: 10px; min-height: 56px; position: relative; }
.card-head h2 { font-size: 19px; flex: 1; }
.ib { width: 34px; height: 34px; border-radius: 17px; border: 0; background: none; display: grid; place-items: center; cursor: pointer; color: var(--faint); flex: none; }
.ib svg { width: 20px; height: 20px; fill: currentColor; }
.ib:hover, .ib[aria-expanded=true] { color: var(--signal); background: var(--signal-soft); }
.pop { position: absolute; right: 0; top: 48px; z-index: 4; width: min(340px, 100%); background: var(--raised); color: var(--text); border: 1px solid var(--line); border-radius: 14px; padding: 12px 14px; font-size: 14px; line-height: 1.5; box-shadow: 0 10px 30px rgba(0,0,0,0.25); }
.row { display: flex; align-items: center; justify-content: space-between; gap: 16px; min-height: 64px; padding: 10px 0; border-top: 1px solid var(--line); }
.row > .label { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.row > .control { flex: 0 1 auto; display: flex; align-items: center; gap: 12px; min-width: 0; justify-content: flex-end; }
.row.stack { flex-direction: column; align-items: stretch; gap: 10px; padding: 14px 0 16px; }
.row.stack > .control { justify-content: flex-start; flex-wrap: wrap; width: 100%; }
:host([size=narrow]) .row:not(.stack) { flex-wrap: wrap; gap: 8px 16px; }

.seg { display: inline-flex; background: var(--raised); border-radius: 999px; padding: 3px; gap: 2px; max-width: 100%; overflow-x: auto; scrollbar-width: none; }
.seg button { border: 0; background: none; font-size: 14px; padding: 7px 15px; border-radius: 999px; cursor: pointer; white-space: nowrap; color: var(--muted); }
.seg button.on { background: var(--signal); color: #fff; }
.sel { position: relative; max-width: 300px; min-width: 170px; }
.sel select { appearance: none; width: 100%; font: inherit; font-size: 14px; color: var(--text); background: var(--raised); border: 1px solid transparent; border-radius: 12px; padding: 9px 36px 9px 14px; cursor: pointer; text-overflow: ellipsis; }
.sel select:focus-visible { outline: 2px solid var(--signal); outline-offset: 1px; }
.sel svg { position: absolute; right: 10px; top: 50%; transform: translateY(-50%); width: 20px; height: 20px; fill: var(--muted); pointer-events: none; }
.switch { position: relative; width: 48px; height: 28px; flex: none; }
.switch input { opacity: 0; width: 100%; height: 100%; margin: 0; position: absolute; cursor: pointer; z-index: 1; }
.switch span { position: absolute; inset: 0; border-radius: 999px; background: var(--raised); border: 1px solid var(--line); transition: background 0.15s; }
.switch span::after { content: ""; position: absolute; width: 20px; height: 20px; left: 3px; top: 3px; border-radius: 50%; background: var(--faint); transition: transform 0.15s, background 0.15s; }
.switch input:checked + span { background: var(--signal); border-color: var(--signal); }
.switch input:checked + span::after { transform: translateX(20px); background: #fff; }
.switch input:disabled + span { opacity: 0.45; }
.slider { display: flex; align-items: center; gap: 12px; }
input[type=range] { -webkit-appearance: none; appearance: none; width: 210px; max-width: 52vw; height: 28px; background: transparent; cursor: pointer; }
input[type=range]::-webkit-slider-runnable-track { height: 6px; border-radius: 3px; background: linear-gradient(to right, var(--signal) var(--fill, 0%), var(--raised) var(--fill, 0%)); }
input[type=range]::-moz-range-track { height: 6px; border-radius: 3px; background: var(--raised); }
input[type=range]::-moz-range-progress { height: 6px; border-radius: 3px; background: var(--signal); }
input[type=range]::-webkit-slider-thumb { -webkit-appearance: none; width: 20px; height: 20px; border-radius: 50%; background: var(--surface); border: 2px solid var(--signal); margin-top: -7px; }
input[type=range]::-moz-range-thumb { width: 16px; height: 16px; border-radius: 50%; background: var(--surface); border: 2px solid var(--signal); }
input[type=range]:disabled { opacity: 0.4; cursor: default; }
.value { min-width: 66px; text-align: right; font-variant-numeric: tabular-nums; color: var(--muted); font-size: 14px; }
.now { color: var(--faint); font-size: 13px; font-variant-numeric: tabular-nums; }
.chips { display: flex; flex-wrap: wrap; gap: 8px; row-gap: 8px; min-width: 0; max-width: 100%; }
.chip { display: inline-flex; align-items: center; gap: 4px; background: var(--raised); border-radius: 999px; padding: 6px 6px 6px 14px; font-size: 14px; max-width: 100%; min-width: 0; }
.chip span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
.chip button { border: 0; background: none; width: 26px; height: 26px; border-radius: 13px; display: grid; place-items: center; cursor: pointer; color: var(--muted); flex: none; }
.chip button svg { width: 16px; height: 16px; fill: currentColor; }
.chip button:hover { color: var(--signal); }
.add { position: relative; display: inline-flex; }
.add .sel { min-width: 0; max-width: 260px; }
.pages { display: flex; flex-direction: column; gap: 10px; }
.page-item { border: 1px solid var(--line); border-radius: 14px; padding: 4px 14px 6px; }
.page-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 44px; }
.page-name { font-family: "Space Grotesk", sans-serif; font-weight: 500; font-size: 16px; }
.page-head .btn svg { width: 16px; height: 16px; fill: currentColor; vertical-align: middle; }
.page-head .btn:disabled { opacity: 0.35; cursor: default; }
.res-head { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; justify-content: space-between; }
.res-head .sum { color: var(--muted); font-size: 14px; }
.btn { border: 1px solid var(--line); background: var(--surface); border-radius: 999px; padding: 8px 16px; cursor: pointer; font-size: 14px; white-space: nowrap; }
.btn:hover { border-color: var(--signal); }
.btn.primary { background: var(--signal); border-color: var(--signal); color: #fff; }
.btn.armed { border-color: var(--signal); color: var(--signal); background: var(--signal-soft); }
.btn:disabled { opacity: 0.45; cursor: default; }
.res { display: flex; flex-direction: column; margin-top: 4px; }
.res .item { display: flex; align-items: center; gap: 12px; padding: 9px 0; border-top: 1px solid var(--line); }
.res .item:first-child { border-top: 0; }
.res .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 14px; }
.res .name small { color: var(--faint); margin-left: 8px; }
.res .size { color: var(--muted); font-size: 13px; font-variant-numeric: tabular-nums; min-width: 64px; text-align: right; }
.res .tag { color: var(--signal); font-size: 12px; border: 1px solid var(--signal); border-radius: 999px; padding: 1px 8px; white-space: nowrap; }
.facts { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 12px 28px; padding: 6px 0 16px; font-size: 14px; border-top: 1px solid var(--line); padding-top: 16px; }
.facts dt { color: var(--muted); }
.facts dd { margin: 0; font-variant-numeric: tabular-nums; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.actions { display: flex; gap: 10px; flex-wrap: wrap; padding: 4px 0 14px; }

/* live preview */
.preview { position: sticky; top: 0; height: 100vh; padding: 28px 28px 28px 0; display: flex; flex-direction: column; gap: 14px; }
:host([size=narrow]) .preview { position: static; height: auto; padding: 16px 16px 0; align-items: center; }
.bezel { position: relative; aspect-ratio: 1; width: 100%; max-width: 352px; background: #0B0B0C; border-radius: 30px; padding: 20px; box-shadow: 0 24px 50px rgba(0,0,0,0.30), inset 0 0 0 1px rgba(255,255,255,0.07); cursor: pointer; }
:host([size=narrow]) .bezel { max-width: 300px; }
.screen { width: 100%; height: 100%; border-radius: 6px; overflow: hidden; background: #000; position: relative; }
.screen img { width: 100%; height: 100%; display: block; }
.screen .off { position: absolute; inset: 0; display: grid; place-items: center; color: #6B6A67; font-size: 14px; }
.pv-foot { display: flex; align-items: center; justify-content: space-between; max-width: 352px; width: 100%; color: var(--muted); font-size: 14px; position: relative; }
.empty { padding: 64px 24px; text-align: center; color: var(--muted); }
.toast { position: fixed; left: 50%; bottom: 24px; transform: translateX(-50%); z-index: 9; background: #101114; color: #F5F3EE; padding: 12px 18px; border-radius: 12px; font-size: 14px; box-shadow: 0 8px 24px rgba(0,0,0,0.3); max-width: calc(100vw - 32px); }
`;

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (key === "class") node.className = value;
    else if (key.startsWith("on")) node.addEventListener(key.slice(2), value);
    else if (value === true) node.setAttribute(key, "");
    else if (value !== false && value != null) node.setAttribute(key, value);
  }
  for (const child of children.flat()) {
    if (child != null && child !== false) node.append(child.nodeType ? child : document.createTextNode(String(child)));
  }
  return node;
}

function svg(paths, cls) {
  const ns = "http://www.w3.org/2000/svg";
  const s = document.createElementNS(ns, "svg");
  s.setAttribute("viewBox", "0 0 24 24");
  if (cls) s.setAttribute("class", cls);
  for (const [d, fill] of Array.isArray(paths) ? paths : [[paths]]) {
    const p = document.createElementNS(ns, "path");
    p.setAttribute("d", d);
    if (fill) p.setAttribute("fill", fill);
    s.append(p);
  }
  return s;
}

const mb = (bytes) => (bytes >= 1048576 ? `${(bytes / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`);

class PanelOSPage extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: "open" });
    this._data = null;
    this._selected = null;
    this._controls = [];
    this._built = null;
    this._showAllResources = false;
    this._section = "panel";
    this._picture = { timer: null, busy: false, url: null, due: 0 };
    this._onVisibility = () => this._schedulePicture(0);
    this._resize = new ResizeObserver(() => this._fit());
    this._closePops = (ev) => {
      if (!ev.composedPath().some((n) => n.classList && (n.classList.contains("pop") || n.classList.contains("ib")))) {
        this.shadowRoot.querySelectorAll(".pop").forEach((p) => p.remove());
        this.shadowRoot.querySelectorAll(".ib[aria-expanded=true]").forEach((b) => b.setAttribute("aria-expanded", "false"));
      }
    };
  }

  set hass(hass) {
    const first = !this._hass;
    this._hass = hass;
    this.toggleAttribute("dark", Boolean(hass?.themes?.darkMode));
    if (first && this.isConnected) this._subscribe();
  }

  get hass() {
    return this._hass;
  }

  set narrow(value) {
    this.toggleAttribute("narrow", Boolean(value));
  }

  set panel(value) {
    this._static = value?.config?.static_url || "/panelos_static";
  }

  set route(_value) {}

  connectedCallback() {
    document.addEventListener("visibilitychange", this._onVisibility);
    this.shadowRoot.addEventListener("click", this._closePops);
    this._resize.observe(this);
    if (this._hass && !this._unsub) this._subscribe();
    this._fit();
    this._render();
  }

  disconnectedCallback() {
    document.removeEventListener("visibilitychange", this._onVisibility);
    this._resize.disconnect();
    clearTimeout(this._picture.timer);
    this._picture.timer = null;
    if (this._unsub) {
      this._unsub.then((unsub) => unsub()).catch(() => {});
      this._unsub = null;
    }
  }

  _fit() {
    const w = this.clientWidth || window.innerWidth;
    const size = w >= 1180 ? "wide" : w >= 860 ? "medium" : "narrow";
    const before = this.getAttribute("size");
    if (before === size) return;
    this.setAttribute("size", size);
    // The preview moves between its column and the top of the page.
    if (this._built && (before === "narrow") !== (size === "narrow")) this._render();
  }

  _subscribe() {
    this._unsub = this._hass.connection.subscribeMessage((data) => this._receive(data), { type: "panelos/page/subscribe" });
    this._unsub.catch((err) => this._toast(`PanelOS: ${err.message || err}`));
  }

  _receive(data) {
    this._data = data;
    const ids = data.panels.map((p) => p.entry_id);
    if (!ids.includes(this._selected)) this._selected = ids[0] || null;
    if (this._built !== this._structureKey()) this._render();
    else this._update();
    this._schedulePicture(this._picture.url ? PICTURE_EVERY_MS : 0);
  }

  get _current() {
    return this._data?.panels.find((p) => p.entry_id === this._selected) || null;
  }

  _structureKey() {
    return `${this._selected}|${(this._data?.panels || []).map((p) => p.entry_id).join(",")}`;
  }

  // -- websocket ------------------------------------------------------------

  async _command(key, value) {
    try {
      await this._hass.callWS({ type: "panelos/page/command", entry_id: this._selected, key, value });
      this._schedulePicture(PICTURE_AFTER_CHANGE_MS);
    } catch (err) {
      this._toast(err.message || String(err));
    }
  }

  async _options(options) {
    try {
      await this._hass.callWS({ type: "panelos/page/options", entry_id: this._selected, options });
      this._schedulePicture(PICTURE_AFTER_CHANGE_MS);
    } catch (err) {
      this._toast(err.message || String(err));
    }
  }

  // A picture is due after `delay` ms, or earlier if one is already due sooner.
  _schedulePicture(delay) {
    if (!this.isConnected || document.visibilityState !== "visible") return;
    const due = Date.now() + delay;
    if (this._picture.timer && this._picture.due <= due) return;
    clearTimeout(this._picture.timer);
    this._picture.due = due;
    this._picture.timer = setTimeout(() => {
      this._picture.timer = null;
      this._fetchPicture();
    }, delay);
  }

  async _fetchPicture() {
    const panel = this._current;
    if (!panel || !panel.connected || this._picture.busy) {
      this._schedulePicture(PICTURE_EVERY_MS);
      return;
    }
    this._picture.busy = true;
    try {
      const result = await this._hass.callWS({ type: "panelos/page/screen", entry_id: panel.entry_id });
      const bytes = Uint8Array.from(atob(result.png), (c) => c.charCodeAt(0));
      const url = URL.createObjectURL(new Blob([bytes], { type: "image/png" }));
      if (this._picture.url) URL.revokeObjectURL(this._picture.url);
      this._picture.url = url;
    } catch (_err) {
      // keeps the last picture; the next try comes on schedule
    } finally {
      this._picture.busy = false;
      this._showPicture();
      this._schedulePicture(PICTURE_EVERY_MS);
    }
  }

  _showPicture() {
    const screen = this.shadowRoot.querySelector(".screen");
    if (!screen) return;
    const panel = this._current;
    if (panel?.connected && this._picture.url) {
      let img = screen.querySelector("img");
      if (!img) {
        screen.replaceChildren();
        img = el("img", { alt: "The panel's screen" });
        screen.append(img);
      }
      if (img.src !== this._picture.url) img.src = this._picture.url;
    } else {
      screen.replaceChildren(el("div", { class: "off" }, panel?.connected ? "Waiting for a picture…" : "Offline"));
    }
  }

  _toast(text) {
    this.shadowRoot.querySelector(".toast")?.remove();
    const toast = el("div", { class: "toast", role: "status" }, text);
    this.shadowRoot.append(toast);
    setTimeout(() => toast.remove(), 4000);
  }

  // -- building blocks -----------------------------------------------------

  _info(key, label) {
    const button = el("button", { class: "ib", "aria-label": `About ${label}`, "aria-expanded": "false" }, svg(ICON.info));
    button.addEventListener("click", () => {
      const host = button.parentElement;
      const open = host.querySelector(".pop");
      this.shadowRoot.querySelectorAll(".pop").forEach((p) => p.remove());
      this.shadowRoot.querySelectorAll(".ib[aria-expanded=true]").forEach((b) => b.setAttribute("aria-expanded", "false"));
      if (!open) {
        host.append(el("div", { class: "pop", role: "note" }, INFO[key]));
        button.setAttribute("aria-expanded", "true");
      }
    });
    return button;
  }

  _card(id, title, rows, infoKey = id) {
    return el(
      "section",
      { class: "card", id: `s-${id}` },
      el("div", { class: "card-head" }, el("h2", {}, title), INFO[infoKey] ? this._info(infoKey, title) : null),
      ...rows
    );
  }

  _row(label, control, stack = false) {
    return el("div", { class: stack ? "row stack" : "row" }, el("div", { class: "label", title: label }, label), el("div", { class: "control" }, control));
  }

  _segmented(label, choices, get, set) {
    const group = el("div", { class: "seg", role: "radiogroup", "aria-label": label });
    let built = "";
    this._controls.push(() => {
      const list = choices();
      const key = JSON.stringify(list);
      if (key !== built) {
        group.replaceChildren(...list.map((c) => el("button", { role: "radio", onclick: () => set(c.value) }, c.label)));
        built = key;
      }
      const current = get();
      [...group.children].forEach((b, i) => {
        const on = list[i]?.value === current;
        b.classList.toggle("on", on);
        b.setAttribute("aria-checked", String(on));
      });
    });
    return group;
  }

  _switch(label, get, set, disabled = () => false) {
    const input = el("input", { type: "checkbox", role: "switch", "aria-label": label, onchange: () => set(input.checked) });
    this._controls.push(() => {
      input.checked = Boolean(get());
      input.disabled = disabled();
    });
    return el("label", { class: "switch" }, input, el("span"));
  }

  _select(label, choices, get, set) {
    const select = el("select", { "aria-label": label, onchange: () => set(select.value) });
    let built = "";
    this._controls.push(() => {
      if (this.shadowRoot.activeElement === select) return;
      const list = choices();
      const current = get() ?? "";
      if (current !== "" && !list.some((c) => c.value === current)) list.push({ value: current, label: current });
      const key = JSON.stringify(list);
      if (key !== built) {
        select.replaceChildren(...list.map((c) => el("option", { value: c.value }, c.label)));
        built = key;
      }
      select.value = current;
      select.title = list.find((c) => c.value === current)?.label || "";
    });
    return el("div", { class: "sel" }, select, svg(ICON.chevron));
  }

  _slider(label, { min, max, step }, get, set, format, disabled = () => false) {
    const value = el("span", { class: "value" });
    const input = el("input", { type: "range", min, max, step, "aria-label": label });
    const paint = () => input.style.setProperty("--fill", `${((input.value - min) / (max - min)) * 100}%`);
    input.addEventListener("input", () => {
      value.textContent = format(Number(input.value));
      paint();
    });
    input.addEventListener("change", () => set(Number(input.value)));
    let dragging = false;
    input.addEventListener("pointerdown", () => (dragging = true));
    input.addEventListener("pointerup", () => (dragging = false));
    this._controls.push(() => {
      input.disabled = disabled();
      if (dragging || this.shadowRoot.activeElement === input) return;
      const v = get();
      if (v == null) return;
      input.value = v;
      value.textContent = format(Number(v));
      paint();
    });
    return el("div", { class: "slider" }, input, value);
  }

  _entities(prefixes) {
    const states = this._hass?.states || {};
    const list = Object.keys(states)
      .filter((id) => prefixes.some((p) => id.startsWith(`${p}.`)))
      .map((id) => ({ value: id, label: states[id].attributes.friendly_name || id }));
    // Same name twice (e.g. two "Home" weathers): the integration in brackets.
    const seen = {};
    for (const c of list) seen[c.label] = (seen[c.label] || 0) + 1;
    for (const c of list) {
      if (seen[c.label] > 1) c.label = `${c.label} (${platformName(this._hass.entities?.[c.value]?.platform) || c.value})`;
    }
    return list.sort((a, b) => a.label.localeCompare(b.label));
  }

  _name(entityId) {
    return this._hass?.states[entityId]?.attributes.friendly_name || entityId;
  }

  // -- sections ------------------------------------------------------------

  _hero() {
    const title = el("h1");
    const meta = el("div", { class: "meta" });
    this._controls.push(() => {
      const p = this._current || {};
      const d = p.device || {};
      title.textContent = d.name || p.title || "Panel";
      title.title = title.textContent;
      meta.replaceChildren(
        el("span", { class: p.connected ? "state on" : "state" }, el("b"), p.connected ? "Connected" : "Offline"),
        d.model ? el("span", {}, [d.manufacturer, d.model].filter(Boolean).join(" ")) : null,
        (p.states?.app_version || d.sw_version) ? el("span", {}, `Version ${p.states?.app_version || d.sw_version}`) : null
      );
    });
    return el("section", { class: "hero", id: "s-panel" }, title, meta);
  }

  _display() {
    const st = () => this._current?.states || {};
    const opts = (key) => (this._current?.select_options[key] || []).map((o) => ({ value: o, label: o }));
    return this._card("display", "Display", [
      this._row("Display mode", this._segmented("Display mode", () => opts("display_mode"), () => st().display_mode, (v) => this._command("display_mode", v))),
      this._row("Theme", this._segmented("Theme", () => opts("theme"), () => st().theme, (v) => this._command("theme", v))),
      this._row("Brightness", this._slider("Brightness", { min: 1, max: 100, step: 1 }, () => st().brightness, (v) => this._command("brightness", v), (v) => `${v} %`, () => Boolean(st().auto_brightness))),
      this._row("Automatic brightness", this._switch("Automatic brightness", () => st().auto_brightness, (v) => this._command("auto_brightness", v))),
      this._row("Screensaver after", this._select("Screensaver after", () => opts("screensaver"), () => st().screensaver, (v) => this._command("screensaver", v))),
      this._row("Screensaver", this._segmented("Screensaver", () => opts("screensaver_style"), () => st().screensaver_style, (v) => this._command("screensaver_style", v))),
      this._row("Wake on approach", this._switch("Wake on approach", () => st().proximity_wake, (v) => this._command("proximity_wake", v))),
    ]);
  }

  _resourcesRow() {
    const head = el("div", { class: "res-head" });
    const list = el("div", { class: "res" });
    const p = () => this._current || {};
    const settings = () => p().settings || {};
    const keep = () => new Set(settings().keep_resources || []);
    const loaded = (r) => r.needed || !settings().skip_unused || keep().has(r.url);
    const auto = this._switch("Leave out unused card resources", () => settings().skip_unused, (v) => this._options({ skip_unused_resources: v }));
    this._controls.push(() => {
      const res = p().resources || [];
      if (!res.length) {
        head.replaceChildren(el("span", { class: "sum" }, p().web_dashboard ? "…" : "–"), auto);
        list.replaceChildren();
        return;
      }
      const out = res.filter((r) => !loaded(r));
      const saved = out.reduce((sum, r) => sum + (r.size || 0), 0);
      const toggleAll = el("button", { class: "btn", onclick: () => { this._showAllResources = !this._showAllResources; this._update(); } }, this._showAllResources ? "Hide the list" : "Show all");
      head.replaceChildren(
        el("span", { class: "sum" }, `${res.length - out.length} of ${res.length} loaded${saved ? ` · ${mb(saved)} left out` : ""}`),
        el("span", { class: "chips" }, toggleAll, auto)
      );
      if (!this._showAllResources) {
        list.replaceChildren();
        return;
      }
      list.replaceChildren(
        ...res.map((r) => {
          const path = r.url.split("?")[0];
          const parts = path.split("/").filter(Boolean);
          const file = parts.pop() || path;
          const folder = parts.length ? parts[parts.length - 1] : "";
          const input = el("input", { type: "checkbox", role: "switch", "aria-label": `Always load ${file}` });
          input.checked = loaded(r);
          // Used ones always load; with leaving out off everything loads.
          input.disabled = r.needed || !settings().skip_unused;
          input.addEventListener("change", () => {
            const now = keep();
            if (input.checked) now.add(r.url);
            else now.delete(r.url);
            this._options({ keep_resources: [...now] });
          });
          return el(
            "div",
            { class: "item" },
            el("div", { class: "name", title: path }, folder && folder !== "local" ? folder : file, folder && folder !== file.replace(/\.js$/, "") ? el("small", {}, file) : null),
            r.needed ? el("span", { class: "tag" }, "Used") : null,
            el("span", { class: "size" }, r.size ? mb(r.size) : "–"),
            el("label", { class: "switch" }, input, el("span"))
          );
        })
      );
    });
    const label = el("div", { class: "label" }, "Card resources");
    return el("div", { class: "row stack" }, el("div", { class: "card-head", style: "min-height:auto" }, label, this._info("resources", "card resources")), head, list);
  }

  _dashboardMode() {
    const settings = () => this._current?.settings || {};
    return this._card("dashboard", "Dashboard", [
      this._row("Dashboard", this._select("Dashboard", () => [...(this._data?.dashboards || [])], () => settings().dashboard, (v) => this._options({ dashboard: v }))),
      this._row("Header", this._switch("Header", () => settings().show_header, (v) => this._options({ show_header: v }))),
      this._row("Sidebar", this._switch("Sidebar", () => settings().show_sidebar, (v) => this._options({ show_sidebar: v }))),
      this._row("Size", this._slider("Size", { min: 50, max: 150, step: 5 }, () => settings().zoom ?? 100, (v) => this._options({ zoom: v }), (v) => `${v} %`)),
      this._row("Loading screen", this._switch("Loading screen", () => settings().loading_screen ?? true, (v) => this._options({ loading_screen: v }))),
      this._resourcesRow(),
    ]);
  }

  _assistantMode() {
    const settings = () => this._current?.settings || {};
    const none = { value: "", label: "None" };
    const chips = el("div", { class: "chips" });
    const add = this._select(
      "Add a quick tile",
      () => [{ value: "", label: "Add a tile…" }, ...this._entities(this._data?.tile_domains || []).filter((c) => !(settings().tile_entities || []).includes(c.value))],
      () => "",
      (v) => v && this._options({ tile_entities: [...(settings().tile_entities || []), v] })
    );
    this._controls.push(() => {
      const tiles = settings().tile_entities || [];
      chips.replaceChildren(
        ...tiles.map((id) =>
          el("span", { class: "chip" }, el("span", { title: id }, this._name(id)),
            el("button", { "aria-label": `Remove ${this._name(id)}`, onclick: () => this._options({ tile_entities: tiles.filter((t) => t !== id) }) }, svg(ICON.close)))
        ),
        el("span", { class: "add" }, add)
      );
    });
    return this._card("assistant", "Assistant", [
      this._row("Weather", this._select("Weather", () => [none, ...this._entities(["weather"])], () => settings().weather_entity, (v) => this._options({ weather_entity: v }))),
      this._row("Media player", this._select("Media player", () => [none, ...this._entities(["media_player"])], () => settings().media_player_entity, (v) => this._options({ media_player_entity: v }))),
      this._row("Quick tiles", chips, true),
      this._row("Full-screen conversation", this._switch("Full-screen conversation", () => settings().show_conversation, (v) => this._options({ show_conversation: v }))),
      this._row("Voice feedback", this._select("Voice feedback", () => NAMES.voice.map((n, i) => ({ value: String(i), label: n })), () => String(settings().voice_look ?? 0), (v) => this._options({ voice_look: Number(v) }))),
    ]);
  }

  // The page list: what the panel swipes through, in this order; each page's
  // look and the entities it shows.
  _pages() {
    const settings = () => this._current?.settings || {};
    const kinds = () => this._data?.page_kinds || {};
    const pages = () => settings().pages || [];
    const list = el("div", { class: "pages" });
    const save = (next) => this._options({ pages: next });
    const add = this._select(
      "Add a page",
      () => [{ value: "", label: "Add a page…" }, ...Object.keys(kinds()).map((k) => ({ value: k, label: PAGE_NAMES[k] || k }))],
      () => "",
      (v) => v && save([...pages(), { kind: v }])
    );
    let built = "";
    this._controls.push(() => {
      const now = pages();
      const key = JSON.stringify([now, Object.keys(this._hass?.states || {}).length]);
      if (key === built) return;
      built = key;
      list.replaceChildren(
        ...now.map((page, i) => {
          const kind = kinds()[page.kind] || { looks: 1, domains: [], most: 0 };
          const change = (patch) => save(now.map((p, j) => (j === i ? { ...p, ...patch } : p)));
          const move = (d) => {
            const next = [...now];
            [next[i], next[i + d]] = [next[i + d], next[i]];
            save(next);
          };
          const head = el(
            "div",
            { class: "page-head" },
            el("span", { class: "page-name" }, `${i + 1}. ${PAGE_NAMES[page.kind] || page.kind}`),
            el(
              "span",
              { class: "chips" },
              el("button", { class: "btn", "aria-label": "Earlier", disabled: i === 0, onclick: () => move(-1) }, "↑"),
              el("button", { class: "btn", "aria-label": "Later", disabled: i === now.length - 1, onclick: () => move(1) }, "↓"),
              el("button", { class: "btn", "aria-label": `Remove ${PAGE_NAMES[page.kind] || page.kind}`, disabled: now.length === 1, onclick: () => save(now.filter((_, j) => j !== i)) }, svg(ICON.close))
            )
          );
          const rows = [head];
          if (kind.looks > 1) {
            const names = LOOKS[page.kind] || [];
            const looks = Array.from({ length: kind.looks }, (_, v) => ({ value: String(v), label: names[v] || `Look ${v + 1}` }));
            const look = el("select", { "aria-label": "Look", onchange: (e) => change({ variant: Number(e.target.value) }) },
              ...looks.map((c) => el("option", { value: c.value }, c.label)));
            look.value = String(page.variant || 0);
            rows.push(el("div", { class: "row" }, el("div", { class: "label" }, "Look"), el("div", { class: "control" }, el("div", { class: "sel" }, look, svg(ICON.chevron)))));
          }
          // the home page's background (and the synthwave scene)
          if (page.kind === "home") {
            const opts = page.options || {};
            const pick = (label, list, key, fallback) => {
              const sel = el("select", { "aria-label": label, onchange: (e) => change({ options: { ...opts, [key]: Number(e.target.value) } }) },
                ...list.map((name, v) => el("option", { value: String(v) }, name)));
              sel.value = String(opts[key] ?? fallback);
              return el("div", { class: "row" }, el("div", { class: "label" }, label), el("div", { class: "control" }, el("div", { class: "sel" }, sel, svg(ICON.chevron))));
            };
            rows.push(pick("Background", NAMES.bg, "theme", 0));
            if ((opts.theme ?? 0) === 8) rows.push(pick("Scene", NAMES.scene, "scene", 0));
          }
          if (kind.most > 0) {
            const ents = page.entities || [];
            const chips = el("div", { class: "chips" },
              ...ents.map((id) => el("span", { class: "chip" }, el("span", { title: id }, this._name(id)),
                el("button", { "aria-label": `Remove ${this._name(id)}`, onclick: () => change({ entities: ents.filter((e) => e !== id) }) }, svg(ICON.close)))));
            if (ents.length < kind.most) {
              const choices = this._entities(kind.domains).filter((c) => !ents.includes(c.value));
              const pick = el("select", { "aria-label": "Add", onchange: (e) => e.target.value && change({ entities: [...ents, e.target.value] }) },
                el("option", { value: "" }, kind.most === 1 ? "Choose…" : "Add…"), ...choices.map((c) => el("option", { value: c.value }, c.label)));
              chips.append(el("span", { class: "add" }, el("div", { class: "sel" }, pick, svg(ICON.chevron))));
            }
            rows.push(el("div", { class: "row stack" }, el("div", { class: "label" }, kind.most === 1 ? "Shows" : "Shows these"), el("div", { class: "control" }, chips)));
          }
          return el("div", { class: "page-item" }, ...rows);
        })
      );
    });
    return this._card("pages", "Pages", [list, this._row("Add", add)]);
  }

  _sensors() {
    const settings = () => this._current?.settings || {};
    const limits = this._data?.reporting || {};
    return this._card(
      "sensors",
      "Sensor reporting",
      Object.entries(limits).map(([key, lim]) => {
        const meta = REPORTING[key] || { label: key };
        const now = el("span", { class: "now" });
        this._controls.push(() => {
          const v = this._current?.states?.[meta.now];
          now.textContent = typeof v === "number" ? `now ${v} ${meta.nowUnit}` : "";
        });
        const label = el("div", { class: "label" }, meta.label, el("div", {}, now));
        const control = this._slider(meta.label, lim, () => settings()[key], (v) => this._options({ [key]: v }), (v) => `${lim.step < 1 ? v.toFixed(1) : Math.round(v)} ${lim.unit}`);
        return el("div", { class: "row" }, label, el("div", { class: "control" }, control));
      }),
      "sensors"
    );
  }

  _device() {
    const facts = el("dl", { class: "facts" });
    const fmt = (v, unit, digits = 0) => (typeof v === "number" ? `${v.toFixed(digits)} ${unit}` : "–");
    this._controls.push(() => {
      const p = this._current || {};
      const s = p.states || {};
      const d = p.device || {};
      const rows = [
        ["Model", [d.manufacturer, d.model].filter(Boolean).join(" ") || "–"],
        ["Version", s.app_version || d.sw_version || "–"],
        ["IP address", s.ip_address || "–"],
        ["Wi-Fi signal", fmt(s.wifi_signal, "dBm")],
        ["SoC temperature", fmt(s.soc_temperature, "°C", 1)],
        ["Illuminance", fmt(s.illuminance, "lx")],
        ["Someone in front", s.presence == null ? "–" : s.presence ? "Yes" : "No"],
      ];
      facts.replaceChildren(...rows.flatMap(([k, v]) => [el("dt", {}, k), el("dd", { title: v }, v)]));
    });
    const armed = (label, key) => {
      let timer = null;
      const button = el("button", { class: "btn" }, label);
      button.addEventListener("click", () => {
        if (button.classList.contains("armed")) {
          clearTimeout(timer);
          button.classList.remove("armed");
          button.textContent = label;
          this._command(key, null);
          return;
        }
        button.classList.add("armed");
        button.textContent = `Tap again: ${label.toLowerCase()}`;
        timer = setTimeout(() => {
          button.classList.remove("armed");
          button.textContent = label;
        }, 3000);
      });
      this._controls.push(() => (button.disabled = !this._current?.connected));
      return button;
    };
    return this._card("device", "Device", [facts, el("div", { class: "actions" }, armed("Restart app", "restart_app"), armed("Reboot", "reboot"))]);
  }

  _preview() {
    const foot = el("div", { class: "pv-foot" });
    this._controls.push(() => {
      const p = this._current || {};
      foot.replaceChildren(
        el("span", { class: p.connected ? "state on" : "state" }, el("b"), p.connected ? "Live" : "Offline"),
        this._info("preview", "the picture")
      );
    });
    return el(
      "aside",
      { class: "preview" },
      el("div", { class: "bezel", title: "Wake the screen", onclick: () => this._current?.connected && this._command("screen", true) }, el("div", { class: "screen" })),
      foot
    );
  }

  // -- navigation --------------------------------------------------------------

  _go(id) {
    this._section = id;
    this.shadowRoot.getElementById(`s-${id}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
    this._markSection();
  }

  _markSection() {
    this.shadowRoot.querySelectorAll("[data-section]").forEach((b) => b.classList.toggle("on", b.dataset.section === this._section));
  }

  _watchSections(root) {
    this._io?.disconnect();
    // Which sections touch the band near the top of the view; the first of
    // them (in page order) is the current one.
    const inBand = new Map();
    this._io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) inBand.set(e.target.id.slice(2), e.isIntersecting);
        const first = SECTIONS.find((s) => inBand.get(s.id));
        if (first && first.id !== this._section) {
          this._section = first.id;
          this._markSection();
        }
      },
      { rootMargin: "-15% 0px -70% 0px" }
    );
    SECTIONS.forEach((s) => {
      const node = root.querySelector(`#s-${s.id}`);
      if (node) this._io.observe(node);
    });
  }

  // -- render ----------------------------------------------------------------

  _render() {
    const root = this.shadowRoot;
    this._controls = [];
    const style = el("style", {}, STYLE.replaceAll("{STATIC}", this._static || "/panelos_static"));
    const menu = el("button", { class: "menu", "aria-label": "Menu", onclick: () => this.dispatchEvent(new Event("hass-toggle-menu", { bubbles: true, composed: true })) }, svg(ICON.menu));
    if (!this._data || !this._data.panels.length) {
      root.replaceChildren(
        style,
        el("div", { class: "top", style: "display:block" }, el("div", { class: "top-row" }, menu, el("i"))),
        el("div", { class: "empty" }, !this._data ? "Loading…" : "No panel is set up yet: Settings → Devices & services → Add integration → PanelOS.")
      );
      this._built = this._data ? this._structureKey() : null;
      return;
    }
    const navLinks = SECTIONS.map((s) => el("button", { "data-section": s.id, onclick: () => this._go(s.id) }, svg(ICON[s.icon]), s.title));
    const panels = this._data.panels.length > 1
      ? el("div", { class: "panels" }, this._data.panels.map((p) => el("button", { class: p.entry_id === this._selected ? "on" : "", onclick: () => this._pick(p.entry_id) }, p.device?.name || p.title)))
      : null;
    const nav = el("nav", { class: "nav" }, el("div", { class: "brand" }, el("i", { role: "img", "aria-label": "PanelOS" })), panels, el("div", { class: "links" }, navLinks));
    const tabs = el("div", { class: "tabs" },
      this._data.panels.length > 1 ? this._data.panels.map((p) => el("button", { class: p.entry_id === this._selected ? "on" : "", onclick: () => this._pick(p.entry_id) }, p.device?.name || p.title)) : SECTIONS.map((s) => el("button", { "data-section": s.id, onclick: () => this._go(s.id) }, s.title)));
    const top = el("div", { class: "top" }, el("div", { class: "top-row" }, menu, el("i", { role: "img", "aria-label": "PanelOS" })), tabs);
    const content = el("div", { class: "content" }, this._hero(), this._pages(), this._display(), this._dashboardMode(), this._assistantMode(), this._sensors(), this._device());
    const preview = this._preview();
    const main = el("main", {}, top, this.getAttribute("size") === "narrow" ? preview : null, content);
    const shell = el("div", { class: "shell" }, nav, main, this.getAttribute("size") === "narrow" ? null : preview);
    root.replaceChildren(style, shell);
    this._built = this._structureKey();

    this._update();
    this._showPicture();
    this._markSection();
    this._watchSections(root);
  }

  _pick(id) {
    this._selected = id;
    if (this._picture.url) URL.revokeObjectURL(this._picture.url);
    this._picture.url = null;
    this._render();
    this._schedulePicture(0);
  }

  _update() {
    for (const update of this._controls) update();
  }
}

customElements.define("panelos-page", PanelOSPage);
