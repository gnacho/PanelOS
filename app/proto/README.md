# ESPHome native API protocol

`api.proto` and `api_options.proto` are copied unchanged from
[esphome/aioesphomeapi](https://github.com/esphome/aioesphomeapi) (`aioesphomeapi/`, main branch,
commit 804f35fe325c, 2026-11-06), MIT licence (`AIOESPHOMEAPI-LICENSE.txt`).

The panel app speaks this protocol as a server on TCP port 6053, so Home Assistant's built-in
ESPHome integration can add the panel as a device. Rust types are generated at build time
(`build.rs`, prost + protox, no `protoc` needed).
