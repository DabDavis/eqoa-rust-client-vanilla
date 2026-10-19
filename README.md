# eqoa-rs: an EQOA client in Rust

A native client for *EverQuest Online Adventures: Frontiers* that owns its own data
structures, rather than running a PS2 inside a program the way
[eqoa-port](https://github.com/DabDavis/eqoa-port) does. The goal is every platform,
phones included (wgpu: Vulkan, Metal, DX12, WebGPU), 64-bit, with the feel of the
original matched exactly.

**No game data is in this repository**, and none is downloaded: you need your own
Frontiers disc image (`run/eqoa.iso`, or `$EQOA_ISO`).

## Method

The language is not what makes the feel exact; the method is. Each system is ported
from the matching decomp ([eqoa-decomp](https://github.com/DabDavis/eqoa-decomp)) one at
a time and proved against a reference:

- **Game logic** (walking, camera, collision, animation) uses the EE's float model,
  crate `ee`: round toward zero, denormals flushed on the way in and out, saturation
  instead of infinity, conditioned compares. Every rule was measured on a console
  (`go-eqoa-vi`). Rust never fuses `a*b+c` on its own, so written order is executed
  order.
- **The reference** is eqoa-port, which runs the game's own functions on the host: a
  test gives the original and the rewrite the same inputs and compares the bits.
  Until that harness exists, crates are held to the fixtures the earlier ports were.
- **Rendering** need not be bit-exact. The VU1 microcode is fully read, and eqoa-port's
  GPU strip programs already redo its maths as shaders; meshes are drawn directly with
  that maths, with no DMA, VIF or GS.

## First slice

A test of the feel before anything broad: load a zone and the player's character off
the disc, port `VIWalkControl` and `VIWalkCamera`, draw with the strip-program shaders,
and compare against eqoa-port frame by frame. If that matches, the rest is breadth:
UI, network (DRDP), sound.

## Crates

| crate | what | checked against |
|---|---|---|
| `ee` | the EE FPU: `mul` `add` `sub` `div` `sqrt` `lt` `le` `eq` | console-measured goldens from `go-eqoa-vi` |
| `iso` | ISO 9660 walk, read one file | the disc |
| `esf` | CSF inflate, the OBJF record tree, DictIDs, surface decode, PrimBuffers, materials, sprites flattened to models | `go-eqoa-esf-tools-v2`: per-surface CRCs (801 surfaces, 4 containers) and per-model geometry (766 models: counts, bbox bits, position CRC) |
| `render` | wgpu: models with their textures, a stand-in shader (texture × layer tint × vertex colour × half-Lambert, alpha test at 128), offscreen capture | by eye, for now |
| `eqoa` | the client binary: `ls`, `tree`, `view` (winit window) and `shot` (PNG, no window) | |

```bash
cargo test          # the surface test skips, loudly, without run/eqoa.iso and ../esfviewer/expected.js
cargo run -- ls
cargo run -- tree CHARCUST.CSF
cargo run --release -- view CHAR.ESF 0x70D898C6     # the human; drag, scroll, ←/→
cargo run --release -- shot ITEM.ESF 5 out.png      # [yaw pitch zoom]
```

Models are Y-up. A model whose texture lives in another container (the human's
shorts, from CHARCUST) draws untextured until the dictionary spans files, as the
game's does (`claude-notes/dictid-cross-file-resolution.md`).

## Licence

MIT (`LICENSE`).
