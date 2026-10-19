# eqoa-rs: an EQOA client in Rust

A native client for *EverQuest Online Adventures: Frontiers* that owns its own data
structures, rather than running a PS2 inside a program the way
[eqoa-port](https://github.com/DabDavis/eqoa-port) does. The goal is every platform,
phones included (wgpu: Vulkan, Metal, DX12, WebGPU), 64-bit, with the feel of the
original matched exactly.

**No game data is in this repository**, and none is downloaded: you need your own
Frontiers disc image (`run/eqoa.iso`, or `$EQOA_ISO`). The in-game HUD also needs
the player's own decrypted SUPPORT module image (`run/support.bin`, or `$EQOA_SUPPORT`):
its UI atlas table is the module's data, not on the disc in the clear, and is read at
run time the way eqoa-port's gendata reads it. Without it `play` runs with plain text
for the chat and the target.

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
| `ee` | the EE FPU: `mul` `add` `sub` `div` `sqrt` `lt` `le` `eq`; and `ee::libm`, newlib as SUPPORT has it compiled: `sinf` `cosf` `powf` (with its `_XOPEN_` wrapper) `atanf` `atan2f` (wrapper too) `acosf` `sqrtf` `fmodf` `scalbnf` | console-measured goldens from `go-eqoa-vi`; the libm bit for bit against go-eqoa-vi's console-checked ports over a 24,000-input corpus (`tools/vimath-corpus` writes it), except `atan2f(denormal, ±0)`, where the wrapper's `c.eq.s` flushes the denormal (disassembly at 0x0012711C) and the Go does not |
| `iso` | ISO 9660 walk, read one file | the disc |
| `esf` | CSF inflate, the OBJF record tree, DictIDs, surfaces, PrimBuffers, materials, sprites flattened to models; `Library`, the dictionary across files; zones (terrain with pre-translations, actors) | `go-eqoa-esf-tools-v2`: per-surface CRCs (801 surfaces) and per-model geometry (766 models: counts, bbox bits, position CRC); which records register a DictID, and that the first file loaded wins, from the decomp |
| `vi` | the game logic, ported from the decomp and generic over a float model: `VIWalkControl`, `VIWalkCamera`, `VICameraEffect`; and **`VIScene::Collide`**: the world and zone BSPs, rooms, collision buffers (all three packings), placed actors and their insertion, Simple/Sub/Group/LOD sprites, `GetEnvFlags` and the liquid-surface ray, loaded from the world file as `VIESFParse` loads it (`world.rs`) | **bit for bit against the decomp compiled for the host** (eqoa-port's `make oracle`), under the `Ieee` model: 18 walk-control and 11 camera methods; `VIMatrix44` Transform/InvertAffine/Mul, `VIPlane`, the transformed `VICollSphere::Init`, the triangle/edge/vertex sweeps, and `CollideObj` over 102 real TUNARIA buffers; the BSP walk against brute force over every room and actor (3,000 sweeps). Negative controls fail |
| `vi` (animation) | `VIHSprite` (the pose evaluator: plays, their timing and 250 ms blends, freezes, LOD rest frames, the hierarchy, bind pose and inverse, centre transform, skin palette), `VIHSpriteAnim` (full and packed clips, triggers), `VICSprite` (66-entry play list, priorities, `SetDefaultEntries`, `SetDefaults`, the `SetLocomotion`/`SetAnimation` arbiter and its two-layer sync), `VIWalkControl::UpdateSprite`, and the loader for CSprite records | **bit for bit against the decomp** (oracle op 500 builds the human from CHAR.ESF as a real VICSprite in a bare VIScene): 320 steps of a locomotion script, all 40 node matrices equal at every step; planted bugs fail |
| `vi` (time of day) | `VIScene::InitTimeOfDay`'s 24 hourly lights, sun directions and sky colours; `SetupTimeOfDayLights`/`Fog`/`Sky`'s interpolation; the world's terrain-profile grid, `VIWorld::SetProfiles` and `CalcProfileBlend` | **bit for bit against the decomp** for the table, the 22 profiles and 4,200 blends over TUNARIA's real grid (oracle ops 600, 601); planted bugs fail. The Setup* combinations write the raster and are not compared |
| `drdp` | DRDP, the game's network library, for the client role: the wire buffer and varints, the state channels' XOR-and-RLE deltas, guaranteed messages in 1,300-byte fragments, best effort, acks and flush numbers, RTT and timeouts, instances and resets, routes and the leaky bucket, the CRC trailer with the protocol version. Sends go to an outbox; datagrams come in behind their sender's address | **byte for byte against the decomp** (`build/oracle_drdp`: the decomp's library compiled with only the socket replaced): two instances through a lossy, reordering, duplicating network, 132,000 replies over six seeds; and a real PS2-Sandstorm capture replayed into one client (the 26 KB MemoryDump, 24 state channels); a 1 ms clock fault fails |
| `net` | The client above DRDP: the Station login (TCP 9735: the 148-byte request with the password DES-CBC under `des_string_to_key("LPSS-V1.0$@Z>.ADJ")`, the 168-byte reply, the AES-256 UDP token), `VIMemStream`'s zigzag and strings, and `VIClient`'s session: the version hello and 0x0904, the server list on type 252, `ConnectShard` and 0x0001, the character list, 0x002A, the promotion (a type-255 notice), the MemoryDump head that places the player (its DRDP source is the player's id), and 0x0014/0x00C9/0x00CA. Then the world: the 0x40 self-state (`SendPosition`, in `vi::selfstate`) every 100 ms, and the entities on channels 0x00-0x17 (`ChannelEntity`'s slot rules, `VIEntity`/`VICreatureEntity::ReadUpdate`, `Extrapolate`'s blend). `eqoa login`, `eqoa play` and `eqoa playshot <host> <user> <out.png> [ms]` (password in `EQOA_PASSWORD`) | live against go-eqoa-server: login to standing in Freeport in about a second, all 24 entity slots filled and drawn, the player's model as the server names it. **The 0x40 encoder against the console**: all 413 self-states the PS2 sent in the Sandstorm capture re-encode to its own bytes |
| `net` (MemoryDump) | `Msg000D` whole (`net::dump`): `VIPlayer::Read`, the quick-chat sets, quests, the inventory and the bank with its lots and bids (`ReadNetItem`, `VIItem`), a corpse, `VIAbility`s, the speed, the crafting slots, Combat Mastery; the stat channel 0x42 (`ReadStats`) | `tests/dump_capture.rs`: Sandstorm's 26,084-byte dump out of `pcaps/ps2_sandstorm.pcap` (replayed through the Rust DRDP) reads to the 39 zero bytes every retail dump ends with, the crafting slots on retail's markers; cut into its last field it reads short |
| `eqoa play` | On a server: the world streamed around the player, NPCs dressed, armed and animated between updates, name plates (`VINameSprite`'s two phases in the game's Arial), chat (0x0A7A/0x0A7B in, 0x000B out; Enter to type), targeting (`CycleTarget` on Tab, 0x0010), attacking (`ToggleAutoAttack` held on F, 0x000F and the 0x40's byte). The chat log and the target's name are drawn plainly: `VIWndGame`'s windows are not ported | live against go-eqoa-server; `eqoa playshot` with `EQOA_CHAR`, `EQOA_SAY`, `EQOA_TAB`, `EQOA_TARGET`, `EQOA_ATTACK` drives it headless |
| `vi` (characters) | `VICSpriteCust`'s tables and `ApplyLook` (`SetRobe`, `SetArmorSlot`, `SetHair`, `SetFace`) over a CSprite's 19 text slots; items and the helm attached at their nodes (`AttachItem`, `SetHelm`); creatures between updates (`ExtrapolateMove`/`ExtrapolateGround`, and a player's own `VIWalkControl` via `SetUpdatePhysics`) | by eye on a live server: Freeport's NPCs in their armour, helms, faces, hair and weapons. The tables are the decomp's constants; the oracle has no raster to apply them through yet |
| `vi` (combat, words) | `GetSwing`, `SwingEffect`, `SwingDelay`, `AttackAnim` (`vi::swing`), `VIHashResourceID`, `VICSprite::SetAction`, `VIStringTable` (WORDS.TXT); in `eqoa play`, `AutoAttackTick` with `TargetInRange`, `CheckPower`, `HasAmmoFor` | **the swing tables equal the decomp over their whole domains** (175,180 entries; oracle op 800 loads CLIENT); `SetAction` in the animation oracle's script; the string table's rules by unit test. The swing's spell effect is not drawn (`VISpellEffects` not ported) |
| the HUD | `VIUI` over a 2D layer (`render::ui`: quads in the 640x448 frame, the GS's blend modes, the frame kept 4:3): the atlas (`GetUITexture`), `Blit`/`BlitEx`/`BlitHorz`/`BlitVert`/`BlitRect`/`BlitBar`/`BlitFrame`/`BlitFillFrame`, "ArialUI" (`DrawString`, `DrawCenteredString`, '?' for a missing glyph), `VIUIWindow` (`Draw`, `TextBox`, `DrawTextInternal` with its inline icons), `VIUITextScroll`; `VIWndGame::OnDrawSelf` (frame, compass, health/power/experience gauges, the target's bar with its scrolling name, health and disposition from 0x0010), `VIWndChat` + `VIChatText` (the box that slides up, 20-second lines, the game's wrap) | by eye on a live server: the HUD where the decomp puts it. Not drawn yet: the group's and pet's bars, compass patches, breath, the target's details box |
| `render` | wgpu: instanced models, a texture cache by DictID, the strip programs' lighting (colour + ambient + sun, alpha as sun visibility) and back-face culling, GPU skinning (`VIRStrip4`'s four weighted palette matrices), and linear fog on view depth as `VIRaster::SetFog` sets it, colour arithmetic on the bytes as the GS does it (no sRGB anywhere), and one pass per material layer with `VIMaterial::ConstructTextPass`'s GS state: the alpha test or one of four blend modes (add, subtract, alpha, multiply), colour-only writes for blended layers, wrap modes, the UV transform and its scroll, the layer colour as c/255; offscreen capture | by eye, for now |
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

- Above the login: the MemoryDump past the player's position, the opcodes after
  world entry, name plates and the rest of the UI, chat, targeting and combat,
  the stat and group channels, and sound. `drdp` leaves out
  endpoint transfer, the master and leader roles and ICMP (see its crate docs).

- Collision: `VICSprite` (a capsule) collides as nothing, and the RAY collide
  (`Collide(VICollRay &)`) is not ported. `VIHSprite` props (900 placements of 67
  sprites in Tunaria) collide through each node attachment under its posed node,
  as `VIHSprite::Collide` does, but that is checked only by the BSP walk test,
  not against the oracle.
  The world level (BSP walk, rooms, actor insertion) is checked against brute
  force, not yet against the oracle: that needs eqoa-port to load the world.
- newlib's huge-angle `sinf`/`cosf` reduction (|x| > 2^7·π/2) and `acosf`'s
  |x| > 1 path are not ported (as in the Go); those fall back to the host.
- Character animation stops at the pose: items, sounds, particles, weapon
  trails, faces, armour and death are not ported (`SetItemAction` is a no-op).
- `VICameraEffect`'s blend into a scripted camera (a quaternion slerp).
- The camera preset's own numbers (`ApplyCameraPreset`); the walk camera's
  defaults stand in.
- Time of day is clear-sky only: `InitTimeOfDay`'s table,
  `SetupTimeOfDayLights`, `SetupTimeOfDayFog` (the four env-flag fogs too) and
  the sky colour of `SetupTimeOfDaySky` are ported (`vi::tod`), over the world's
  terrain-profile grid (`VIWorld::CalcProfileBlend`, `SetProfiles`). Not ported:
  the weather that sets the overcast weight, sky-type transitions
  (`ProcessTransitions`' 30 s fade: the sky is always settled), stars, lightning
  and the Lavastorm. The dome, sun, corona and moon are drawn (`RenderSky`,
  `VIAtmosphere::Update`'s placement, `SetSky`'s type, out of DATA2/SKY.CSF). The hour runs
  from 10:00 at 24 h per hour (`EQOA_HOUR` pins it); a live game gets it from the
  server. The table, the profiles and the blend are held to the oracle; the
  Lights/Fog/Sky functions that combine them write the raster, so they are not.
- The point light and static lighting. Blended layers are drawn after every
  opaque one, not interleaved strip by strip as the game draws them.
- Zone streaming is `VIWorld::StreamLeaf`'s radial rule (load within 900 of a
  zone's bounds, let go past 950, a zone a frame) for drawing only: collision
  still loads the whole file (two seconds for Tunaria), and `VIZone::Stream`'s
  finer per-static radii (500/550) are not ported.
- A lighting band along terrain patch edges (zone 84 at x = 25000): not a gap,
  the edges' vertex colours or normals; to revisit with the real lighting.

## Licence

MIT (`LICENSE`).
