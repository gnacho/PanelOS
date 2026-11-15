# Security

What PanelOS protects, how, and what is left open on purpose while the project is in development.

## Network services

The panel listens on:

| Port | Service | Who may use it |
|---|---|---|
| TCP 6053 | ESPHome native API (`app/src/esphome.rs`, `app/src/api_link.rs`) | Home Assistant only, encrypted |
| TCP 22 | SSH (dropbear) | the holder of the SSH key in the image |
| UDP 5353 | mDNS (`app/src/mdns.rs`) | answers only to the panel's own network |
| TCP 8080 | login callback (`app/src/pairing.rs`) | only until the panel is paired |

There is no firewall (the kernel is built without netfilter); every service above checks its own clients.

## ESPHome API

- **Only Home Assistant connects.** The panel takes connections only from the address of its Home Assistant URL (`ha_url` in `app.json`). If Home Assistant reaches the panel from another address (a proxy, a container network), add it to `app.json`: `"api_allow": ["192.168.1.10"]`.
- **Encrypted.** Like an ESPHome device without a key in its YAML, the panel reports that it supports encryption and can be given a key. Right after adding the panel, Home Assistant creates a key, sends it over a Noise session with the all-zero key, and connects encrypted from then on (`Noise_NNpsk0_25519_ChaChaPoly_SHA256`). The key is in `/mnt/persist/nspanel/api_key` (0600). From then on the panel refuses plaintext and every other key.
- **New key or a new Home Assistant:** delete the panel in Home Assistant (which clears the key there), then on the panel `rm /mnt/persist/nspanel/api_key` and restart the app; add the panel again.
- **Limits:** plaintext frames up to 256 KB, Noise frames up to 64 KB, at most three connections, an idle connection that is not the main session closes after 60 s. A malformed message ends only its own connection, never the API.
- **Voice:** a run streams the microphones for at most 30 s whatever the events say; Home Assistant's "continue the conversation" only counts while the panel's own run is going on.
- **Audio from URLs** (answers, announcements) is fetched only from Home Assistant's host, without redirects, up to 16 MB, at 8–48 kHz, 1–2 channels and 3 minutes; one announcement at a time.

## Home Assistant connection

- The panel logs in with Home Assistant's normal login (OAuth, QR code). The refresh token is in `/mnt/persist/nspanel/ha_auth.json` (0600) and never logged. Logging out revokes it in Home Assistant.
- The login token goes only to Home Assistant itself (camera, artwork), never to another host.
- **Pictures of the screen** for the PanelOS page in Home Assistant are taken only when an administrator has the page open, and go only to Home Assistant over the panel's own connection.
- **No TLS yet:** the connection is plain `http://` and `ws://`, readable by anyone on the same network. Use the panel only in a network you trust, and pair it with a Home Assistant user that is not an administrator.

## SSH

- Root logs in with the SSH key only; the root password is locked, also on the serial console.
- The host key is created on the first start and kept in `/mnt/persist/nspanel/dropbear_ed25519_host_key` (`S49sshhostkey`), so it survives reboots and updates. The commands in the guides accept it the first time and check it after (`/root/.ssh/panelos_known_hosts`). If it ever changes on purpose (e.g. after restoring the stock firmware): `ssh-keygen -R <panel IP> -f /root/.ssh/panelos_known_hosts`.

## Build

- Kernel and Buildroot are fetched at fixed commits (`tools/fetch_sources.sh` stops when a tag moved).
- Wake word models come from a fixed commit or a release and are checked against `tools/wakewords.sha256`.
- Rust crates are pinned with checksums in `app/Cargo.lock`.

## Development aids

These stay on while PanelOS is in development; they make testing and debugging possible without opening the panel. A release image has to leave them out.

| Aid | What it does | Risk |
|---|---|---|
| SSH key in the image (`NSPANEL_SSH_PUBKEY_FILE`) | root login for the builder | a release image must not carry anyone's key |
| `S98logforward` | copies syslog lines into the kernel log, so they survive in pstore and can be read after a crash | log lines (below) outlive a reboot |
| Log lines of the voice satellite | every pipeline event with the first 100 characters of its values (heard text, answer), timer names, show requests | private speech in `/var/log/messages` (RAM) and pstore |
| `SIGUSR1` | screenshot to `/tmp/nspanel-screen.ppm` | may capture a message or a camera |
| `/tmp/nspanel-voice-test.wav`, `/tmp/nspanel-voice-followup.wav` | sent to Home Assistant as speech instead of the microphones | root only |
| `/tmp/nspanel-proximity-test` | proximity test view (`docs/bringup.md`) | root only |
| `NSPANEL_API_DEBUG`, `NSPANEL_AEC_DUMP` | API message log, microphone dump | root only |
| `NSPANEL_WEB_STATS`, `NSPANEL_DEV_DARK`, `WEBKIT_*` | web view frame timing in the log; backlight kept off; WebKit's own switches passed to the web helper | root only |
| `/mnt/persist/panelos-web-data/console-log` | the dashboard web view writes the page's console messages to the log | dashboard details in `/var/log/messages` (RAM) |
| debugfs, `/dev/mem`, magic SysRq, unsigned kernel modules | kernel debugging | root only |

All of these need root on the panel, which needs the SSH key.

## Not covered

- **No verified boot.** The stock U-Boot starts whatever is in the boot partition; physical access (BOOT button and USB) or root on the panel means full control.
- The panel app runs as root.
