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
| `esf` | CSF inflate, the OBJF record tree, DictIDs, surfaces, PrimBuffers, materials, sprites flattened to models; `Library`, the dictionary across files; zones (terrain with pre-translations, actors) | `go-eqoa-esf-tools-v2`: per-surface CRCs (801 surfaces) and per-model geometry (766 models: counts, bbox bits, position CRC); which records register a DictID, and that the first file loaded wins, from the decomp |
| `vi` | the game logic, ported from the decomp and generic over a float model: `VIWalkControl`, `VIWalkCamera`, `VICameraEffect`; and **`VIScene::Collide`**: the world and zone BSPs, rooms, collision buffers (all three packings), placed actors and their insertion, Simple/Sub/Group/LOD sprites, `GetEnvFlags` and the liquid-surface ray, loaded from the world file as `VIESFParse` loads it (`world.rs`) | **bit for bit against the decomp compiled for the host** (eqoa-port's `make oracle`), under the `Ieee` model: 18 walk-control and 11 camera methods; `VIMatrix44` Transform/InvertAffine/Mul, `VIPlane`, the transformed `VICollSphere::Init`, the triangle/edge/vertex sweeps, and `CollideObj` over 102 real TUNARIA buffers; the BSP walk against brute force over every room and actor (3,000 sweeps). Negative controls fail |
| `render` | wgpu: instanced models, a texture cache by DictID, the strip programs' lighting (colour + ambient + sun, alpha as sun visibility) and back-face culling, the base layer's alpha test, offscreen capture | by eye, for now |
| `eqoa` | the client binary: `ls`, `tree`, `zones`, `view` (winit window) and `shot` (PNG, no window); walking (`walker.rs`, driven as `VIClient::UpdateControls` drives it in third person) on the game's own collision world, the zone and its neighbours drawn | by eye |

```bash
cargo test          # the disc tests skip, loudly, without run/eqoa.iso and ../esfviewer/
cargo run --release -- zones TUNARIA.ESF                 # 175 zones
cargo run --release -- view TUNARIA.ESF zone 84          # Freeport's river; WASD, Q/E, Shift, drag
cargo run --release -- view CHAR.ESF 0x70D898C6          # the human; drag, scroll, ←/→
cargo run --release -- shot TUNARIA.ESF zone 84 out.png [x y z yaw pitch]
cargo run --release -- view TUNARIA.ESF walk 84          # walk: WASD, ←/→ orbit, ↑/↓ zoom, drag
cargo run --release -- shot TUNARIA.ESF walk 84 out.png 3000 1   # hold forward 3 s of game time
EQOA_SPAWN=24900,13660,3.14 cargo run --release -- shot TUNARIA.ESF walk 84 out.png 7000 1   # into the river
```

Walking in TUNARIA uses the whole world's collision (175 zones, 55,289
buffers, 229,777 placed actors; built in about 2 s): the character stands on
terrain and on placed objects, slides round a tree stump, and in the river
gets the water flags, the swimming locomotion state and the surface height from
the environment ray, then climbs out on the far bank.

### The oracle, and the two float models

The `vi` crate is written once, generic over a float model. `Ee` is the console
(crate `ee`: chop, flush, saturate, conditioned compares) and is what the client
runs. `Ieee` is eqoa-port's arithmetic (SSE single precision, no contraction,
glibc's libm), and exists for one reason: eqoa-port's `build/oracle` runs the
decomp's own functions on raw object bytes, and under `Ieee` this port must
return the same bytes. That proves the control flow, the constants and the
operation order; the `ee` crate's console goldens prove the arithmetic. The
structs are `#[repr(C)]` in the EE's layout (checked against the oracle's
`sizeof`/`offsetof` first), so nothing is translated on the way across.

```bash
(cd ../eqoa-port && make oracle)
cargo test --release -p vi        # the oracle tests skip, loudly, without it
```

The 32-bit glibc and the host's agree on `sinf`, `cosf`, `powf`, `atan2f`,
`fmodf` and `sqrtf` over 20,000 samples each (`libm_matches`), which the
comparison rests on.

### The dictionary

The game keeps one dictionary per stage, and every file it loads registers into
it; a record whose DictID is already present is skipped, so **the first file
loaded wins** (`ParseSurfaceObj` finds before it adds; `VIMap::Insert` keeps an
existing key). It matters: CHAR.ESF and CHARCUST.CSF share 16 DictIDs and 8 of
them carry different bytes (`cargo run --release -p eqoa --example dict_overlap`).
GroupSprite members are read as the decomp reads them, (angles, scale,
translation), not as the Go tools do (`claude-notes/groupsprite-member-order-20261019.md`).
Which records register is read off the decomp's 27 `m_dict->Add` sites, not off
the Go tools, whose table missed point sprites (all 362 of ITEM.ESF's
unresolved GroupSprite members) and registered version-0/1 PrimBuffers. A scene
loads its own file first, then CHARCUST.CSF and CHARFACE.CSF; the game's real
order per stage is not yet measured.

### Not yet

- Collision: `VIHSprite` (needs the pose evaluator) and `VICSprite` (a capsule)
  collide as nothing; the RAY collide (`Collide(VICollRay &)`) is not ported.
  The world level (BSP walk, rooms, actor insertion) is checked against brute
  force, not yet against the oracle: that needs eqoa-port to load the world.
- `Ee`'s `sinf`/`cosf`/`powf`/`atan2f` are the host's on conditioned operands;
  newlib's (ported and console-checked in `go-eqoa-vi/vimath`) are next.
- The character is drawn in its bind pose: no animation (`vianim` in Go).
- `VICameraEffect`'s blend into a scripted camera (a quaternion slerp).
- The camera preset's own numbers (`ApplyCameraPreset`); the walk camera's
  defaults stand in.
- Time of day: ambient and sun are fixed placeholders until `VIScene::SetupTimeOfDay`.
- The point light, static lighting, fog, and every material layer after the first.
- Zone streaming: a walk loads the whole world file and draws the zones within
  2.5 km of the chosen one.
- A lighting band along terrain patch edges (zone 84 at x = 25000): not a gap,
  the edges' vertex colours or normals; to revisit with the real lighting.

## Licence

MIT (`LICENSE`).
