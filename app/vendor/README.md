# Vendored crates

Crates used in a modified form, through `[patch.crates-io]` in `app/Cargo.toml`.

## i-slint-renderer-skia 1.18.1

Slint's Skia renderer (GPL-3.0-only, among other licences; see its `LICENSES`),
used by the GPU build. Changes:

- `itemrenderer.rs`, `draw_image_impl`: the image origin is no longer rounded to
  whole device pixels, so images moving slowly (the home page's drifting sky)
  move smoothly instead of a pixel at a time.

Update: take the same version from crates.io and apply the changes again (each
is marked with a `PanelOS:` comment).
