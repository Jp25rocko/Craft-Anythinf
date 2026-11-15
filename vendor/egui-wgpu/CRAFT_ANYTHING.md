# egui-wgpu 0.36.2, patched for Craft Anything

This is a copy of `egui-wgpu` 0.36.2 (MIT OR Apache-2.0, see `LICENSE-*`) with one change, used through
`[patch.crates-io]` in the workspace `Cargo.toml`.

## The change

When the window draws with WARP (Windows' software renderer: the DX12 adapter of type `Cpu`), egui's
shader filters textures by hand and never reads a sampler.

- `src/lib.rs`: `RenderState::create` turns on `predictable_texture_filtering` for WARP (`is_warp`).
- `src/egui.wgsl`: the choice between hardware and manual filtering is the pipeline constant
  `HARDWARE_FILTERING` instead of a uniform, so with manual filtering the compiled shader contains no
  sampler access at all.
- `src/renderer.rs`: the fragment stage sets `HARDWARE_FILTERING` from `predictable_texture_filtering`.

On every other adapter nothing changes: hardware filtering, as upstream.

## Why

wgpu 25 and later bind DX12 samplers through one sampler heap: each shader reads its sampler's index from
a buffer and picks the sampler from the heap. On WARP a texture read through a sampler is about ten times
slower than the same read with `textureLoad`; the heap lookup is the likely reason (not confirmed inside
WARP). Measured on a Windows 11 VM without a GPU (QEMU, 6 vCPUs, desktop over RDP, window 2294×1264,
release build):

| | upstream | patched |
|---|---|---|
| Empty egui window, redrawn continuously | 0.25 s, 5.3 G cycles a frame | 0.03 s (the display's rate), 0.4 G cycles |
| Craft Anything, mouse moving over it | 2.6 frames/s, 5 of 6 cores busy | 13 frames/s, 2.6 cores |

A sampled profile of the upstream build put 99% of the process's time in WARP's rasterizer and the code it
compiles. Screenshots of the app drawn with egui's hardware filtering and with its manual filtering differed
by at most 1 level in 34 pixels.

## Removing it

Delete this directory and its `[patch.crates-io]` entry once egui-wgpu leaves the sampler out of the
pipeline when it filters by hand, or wgpu's DX12 backend samples quickly on WARP. Measure frame times on
WARP (`WGPU_BACKEND=dx12 CRAFT_ANYTHING_RENDERER=software`) before and after.
