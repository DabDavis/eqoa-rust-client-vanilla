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
| `vi` (atmosphere) | `VIAtmosphere` whole (`vi::atmosphere`): `Init` (the 200 weather particles out of the module's noise, the wind table, the 128 stars), `SetWeather`/`SetDefaultSky`/`SetPlaneOfSky`, `Update` (the sun, corona and moon rotations), `ProcessWeather` (the 10-unit box at the camera, rain and snow by the effect mix, each column dropped to the ground or a liquid surface, splashes), `ProcessTransitions` (the 30-second fade: sun/corona/moon alphas, the dome's third layer, the intensity table, the rain's two voices and the thunder), the lightning (`ResetLightning`, `RecomputeLightningPatches`, `GetLightning`), `RenderStars` (the shooting star too), `RenderWeather`/`RenderSplashes`, the Lavastorm's fireballs on `vi::particle`, `SetSkyColor`; and `VIScene::SetSky`. `vi::tod`: the overcast weight (`GetOvercastInterp` × the profiles'), the Lavastorm weight, the flash on the sun. **The weather types are 1-3 rain and 4-6 snow** at 0.15/0.5/1, read off the code (`vi.h`'s names say rain, snow, sleet, sandstorm, Lavastorm, thunderstorm) | **frame by frame against the decomp** (eqoa-port's `build/oracle_atmos`: `viatmosphere.cpp` with `vimaterial.cpp` and the particle TUs over a recording raster, sound device and scene; `vi/tests/oracle_atmos.rs`): 4,200 frames through night skies, a rain fade, a queued change, snow, an instant change, FX.CSF's own Lavastorm, clearing and both planes of sky, every billboard (785,578), sound call (13,199), sky sprite and model-view, flash (227 frames), material layer, the fade state and all 200 particles equal bit for bit under `Ieee`; a planted difference and a mutated constant fail |
| `vi` (effect volumes) | `VIEffectVolume`, `VIBoid` and `VIEffectVolumeSprite::Raster` (`vi::effect`): `GlobalInit`'s cards, the five kinds (falling leaves, light shafts with the sun-following flag, ground fog, dust storm, a flock of boids steered by Reynolds' rules with their locomotion), the hour window with its five-minute ramps, the distance fade, the 60 Hz wall-clock step, the re-seed when the box drifts; recorded as `VIRFan` fans (material, colour, normal, uv and position of all six vertices) and boid sprites with their model-views. A mote born on frame 60 reads `sDustCard[60]`, past the table (the oracle's build has `sFogCard[0]` there); `Set(x, y, z)`'s three draws in `VIBoid::Init` are evaluated last argument first, as the oracle's compiler does | **frame by frame against the decomp** (eqoa-port's `build/oracle_effect`; `vi/tests/oracle_effect.rs`): the cards, then 900 frames of one volume of each kind under a camera circling through their fade distances over a day, a clock that stutters and hitches and a box moved to re-seed: 24,792 fans, 2,727 boid sprites, every scene call and each volume's state with all its particles equal bit for bit under `Ieee`; a planted difference and a mutated card fail. **Not drawn by the client yet** (the fans need an immediate-mode path in `render`, the boids the CSprite machinery) |
| `drdp` | DRDP, the game's network library, for the client role: the wire buffer and varints, the state channels' XOR-and-RLE deltas, guaranteed messages in 1,300-byte fragments, best effort, acks and flush numbers, RTT and timeouts, instances and resets, routes and the leaky bucket, the CRC trailer with the protocol version. Sends go to an outbox; datagrams come in behind their sender's address | **byte for byte against the decomp** (`build/oracle_drdp`: the decomp's library compiled with only the socket replaced): two instances through a lossy, reordering, duplicating network, 132,000 replies over six seeds; and a real PS2-Sandstorm capture replayed into one client (the 26 KB MemoryDump, 24 state channels); a 1 ms clock fault fails |
| `net` | The client above DRDP: the Station login (TCP 9735: the 148-byte request with the password DES-CBC under `des_string_to_key("LPSS-V1.0$@Z>.ADJ")`, the 168-byte reply, the AES-256 UDP token), `VIMemStream`'s zigzag and strings, and `VIClient`'s session: the version hello and 0x0904, the server list on type 252, `ConnectShard` and 0x0001, the character list, 0x002A, the promotion (a type-255 notice), the MemoryDump head that places the player (its DRDP source is the player's id), and 0x0014/0x00C9/0x00CA. Then the world: the 0x40 self-state (`SendPosition`, in `vi::selfstate`) every 100 ms, and the entities on channels 0x00-0x17 (`ChannelEntity`'s slot rules, `VIEntity`/`VICreatureEntity::ReadUpdate`, `Extrapolate`'s blend). `eqoa login`, `eqoa play` and `eqoa playshot <host> <user> <out.png> [ms]` (password in `EQOA_PASSWORD`) | live against go-eqoa-server: login to standing in Freeport in about a second, all 24 entity slots filled and drawn, the player's model as the server names it. **The 0x40 encoder against the console**: all 413 self-states the PS2 sent in the Sandstorm capture re-encode to its own bytes |
| `net` (MemoryDump) | `Msg000D` whole (`net::dump`): `VIPlayer::Read`, the quick-chat sets, quests, the inventory and the bank with its lots and bids (`ReadNetItem`, `VIItem`), a corpse, `VIAbility`s, the speed, the crafting slots, Combat Mastery; the stat channel 0x42 (`ReadStats`) | `tests/dump_capture.rs`: Sandstorm's 26,084-byte dump out of `pcaps/ps2_sandstorm.pcap` (replayed through the Rust DRDP) reads to the 39 zero bytes every retail dump ends with, the crafting slots on retail's markers; cut into its last field it reads short |
| `eqoa play` | On a server: the world streamed around the player, NPCs dressed, armed and animated between updates, name plates (`VINameSprite`'s two phases in the game's Arial), chat (0x0A7A/0x0A7B in; `ChatLine` out: `/say`, `/g`, `/gu`, `/s`, `/t`, `/r` on their channels), targeting (`CycleTarget`, 0x0010), attacking (`ToggleAutoAttack` held, 0x000F and the 0x40's byte). Talking (`InteractWithTarget` on Q by the npc's flags; the reply 0x0046 through `VIClientText::Expand` into `VIWndPrivChat`'s box, queued as `m_dialogs` is; Enter answers it). Teleports in the world (0x00B2: `Msg00B2`/`Respawn`, answered with 0x0049; coach travel and GM `#tele` both arrive this way; another world's file, by `gWorldInfo`'s six disc paths, replaces the world and the walk begins again there: Freeport to Odus by `#goto`). Server-driven actions (0x0775) and the level and experience (0x0020). Groups: `InviteEntity` (0x0625), the invitation's prompt (0x0773), `AcceptInvite`/`DeclineInvite`, leave/remove/disband, the roster (0x0772) and the members' health and place (state channel 0x40). Abilities: the dump's list kept by 0x00B3/B4/B5/B6/B8/CD with `Msg00B4`..`Msg00CD`'s rules, used from the HUD's menus as `Send0032` (token, slot, target). Keys are the game's own (`VIWndGame::OnKeyDown`) where they do not collide with walking on WASD: Space attacks, Tab targets, Q talks, Enter or `/` opens a chat line, R and T start `/reply` and `/tell`. The chat log and the target's name are drawn plainly: `VIWndGame`'s windows are not ported | live against go-eqoa-server; `eqoa playshot` with `EQOA_CHAR`, `EQOA_SAY`, `EQOA_TAB`, `EQOA_TARGET`, `EQOA_ATTACK` drives it headless |
| `vi` (characters) | `VICSpriteCust`'s tables and `ApplyLook` (`SetRobe`, `SetArmorSlot`, `SetHair`, `SetFace`) over a CSprite's 19 text slots; items and the helm attached at their nodes (`AttachItem`, `SetHelm`); creatures between updates (`ExtrapolateMove`/`ExtrapolateGround`, and a player's own `VIWalkControl` via `SetUpdatePhysics`) | by eye on a live server: Freeport's NPCs in their armour, helms, faces, hair and weapons. The tables are the decomp's constants; the oracle has no raster to apply them through yet |
| `vi` (combat, words) | `GetSwing`, `SwingEffect`, `SwingDelay`, `AttackAnim` (`vi::swing`), `VIHashResourceID`, `VICSprite::SetAction`, `VIStringTable` (WORDS.TXT); in `eqoa play`, `AutoAttackTick` with `TargetInRange`, `CheckPower`, `HasAmmoFor` | **the swing tables equal the decomp over their whole domains** (175,180 entries; oracle op 800 loads CLIENT); `SetAction` in the animation oracle's script; the string table's rules by unit test. The swing's spell effect is not drawn (`VISpellEffects` not ported) |
| `vi` (spell effects) | `VISpellEffectSystem` (24 playbacks, handles, the minute's backstop), `VISpellEffectDriver`, `VISpellEventDriver` with all 17 event types (emitters placed at an actor's node or orbiting it, motif blends, attractors, actions and attacks, waits on an action, the clock, the cast time or an animation's "Spell FX" trigger, projectiles in their four flights, lightning at the target, sounds and sprites made and moved, camera shakes, rumble), `VILightning` (`vi::lightning`), the 0xC200 record; in `eqoa play`, `CastingCombat` (0x00B1), `Msg0776` and the player's swing, drawn (particles, lightning strips) and the sound sprites they make heard (`UpdateSoundEmitters`' second half). `EQOA_SPELL=n` with `eqoa shot ... walk` casts SPELLFX's n-th effect offline | **call for call against the decomp**: eqoa-port's `build/oracle_spell` (the spell, particle and lightning TUs over a fake world) and the same fake here (`tests/oracle_spell.rs`): all 238 SPELLFX effects, 16,000 frames, 46,895 world calls, 511,901 lightning strip records and 3,512,650 billboards equal bit for bit; a planted record, a longer wait and a changed world rule fail. In the client by eye (`EQOA_SPELL`). Not ported: the weapon trail, a hand's projectile sprite, the animated sprites an effect makes (tracked, not drawn), the fade (unused on the disc), rumble; the effects have a particle system of their own |
| the HUD | `VIUI` over a 2D layer (`render::ui`: quads in the 640x448 frame, the GS's blend modes, the frame kept 4:3): the atlas (`GetUITexture`), `Blit`/`BlitEx`/`BlitHorz`/`BlitVert`/`BlitRect`/`BlitBar`/`BlitFrame`/`BlitFillFrame`, "ArialUI" (`DrawString`, `DrawCenteredString`, '?' for a missing glyph), `VIUIWindow` (`Draw`, `TextBox`, `DrawTextInternal` with its inline icons), `VIUITextScroll`; `VIWndGame::OnDrawSelf` (frame, compass, health/power/experience gauges, the target's bar with its scrolling name, health and disposition from 0x0010), `VIWndChat` + `VIChatText` (the box that slides up, 20-second lines, the game's wrap) | **`vi::ui` (VIUI, VIUIWindow, VIUITextScroll) records the decomp's raster calls exactly**: `tests/oracle_ui.rs` drives 4,540 calls (every atlas id through every blitter, tiling, bars, frames, words, window animation, `TextBox`'s flags, inline icons at a fixed clock, 700 frames of both scroll styles) through eqoa-port's `oracle_ui` (the decomp's UI TUs over a recording raster) and here; 10,750 raster calls equal; planted bugs fail. The HUD and chat box (CLIENT) by eye on a live server. Also drawn: the pet's bar (`DrawPet`, 0x007E/0x007F), the breath gauge under liquid (`DrawStatus`), the target's details box (`DrawTargetInfo`, Q held on a player), and the three quick menus (`VIWndHUDMenu`: `DrawList` with the abilities' icons and lockout badge, `DrawStack`; `UpdateQuickBars`' `BuildAbilityMenu0/1` and `BuildQuickMenu`; the arrows are the d-pad, E is X, the camera moves to I/J/K/L). The start menu (Escape in play, the character framed by `ApplyRaceCamera` from CLIENT's `gRaceCameras`: `VIWndStartMenu`'s frame, `VIWndMainMenu` with its seven bars, the character's bar, the stats column with `UpdateLocation`'s zone and ninth and the effects, sub-menus over it, `VIKeyFlash`'s repeat; its menus are CLIENT's data, read with gWorldInfo out of the decrypted image at `run/client.bin` through its relocations; entries go to `DoAction` or the mode-13 path; Quit asks with the message box). The group's bars (`DrawGroup`, from 0x0772's roster and state channel 0x40) and `VIWndMBox`, the Yes/No box an invitation asks with. The spell book (`VIWndSpellBook`, the start menu's Ability: level tabs, six ability plates (`BlitAbilityIcon`), the selected one's stats and description, the slots, five or ten; its popup Memorize/Arrange/Unmemorize All as `Send0033`/`Send003A`/`Send00FE`, answered by 0x00B4/0x00B6/0x00FF; 0x00FD sets an ability usable). The who list (`VIWndWhoList` over `VIClientPart38334`: 0x0E01/0x0E02 with a serial, ten rows of name, race, class and level, "searching..." while the reply is out; `VIWndOptionList`'s Tell, through the edit box as `/t`, and Invite, `InviteToGroup` with the row's id). `VIWndEdit`, the edit box its entries open (the soft keyboard, or the compact legend for a keyboard, the default here: `EQOA_SOFT_KEYBOARD=1`): Message, buddies, ignore, report. The spinvell bntookry (`VIWndSpellBook`, the start menu's Abili"Inventory": l`VIWndItemMenu` ovelr `VIWndList`/`VIWndMenu`/`VIWndMiniMenu`, the four tabs, `UpdateQuickBarsix` arebuilds each frame wityh `VIInfoText`'s plredicates, (`Bleach itAem's subilityIco-men`)u, the selected onitem's stats and description with `StatWithItem`'s "Str: 5 -> 7", `VIWndInspect`, `VIWndQuantity`, the destroy prompt, the quick-slots, fivmode or ten; item icons pfropupm ITEMemorize/Arrange/Unmemorize All as `ICON.ESend0033`/`Send003A`/`Send00FE`, ans"?" whered one is missing; the inventory kept by 0x003B4/-0x00B6/40x00FF;, 0x00FD sets0A and abil0x0060 wityh `VIItemList`'s rules; use/equip 0x003E, destroy 0x003C, ablrrange 0x003D, prepare 0x000A, and the HUD's quick item uses it too). Not drawn yet: compass patches |
| `snd` | The sound, as the PS2 makes it: the SPU2 (eqoa-port's `spu2.c`: ADPCM, Gaussian interpolation, ADSR, the two cores' mix), SNDDRV, the IOP's sound driver (`modules/snddrv`: the command handlers, the 100 Hz XM thread, the BGM stream, the XM player reading its module in place), with the IOP's threads as state machines on the SPU2's sample clock; and `VISoundDevice`, `VIWave`, `VIXm` (the EE half: playbacks and handles, priority stealing, the SPU and XM-pool allocators, uploads a block a frame, the BGM request slots and cross-fade), generic over the float model | **sample for sample against the decomp**: eqoa-port's `build/oracle_snd` runs the decomp's SNDDRV over the same SPU2 on a deterministic coroutine IOP, and the decomp's own VISoundDevice above it. `tests/oracle_snd.rs`: UI.CSF's waves, AMBTRACK.ESF's modules and two BGMs through every command, 1,924,000 frames and the status blocks equal; `tests/oracle_snddev.rs`: 2,233 calls as the game makes them, 729 packets equal to the byte, 1,132,000 frames equal. Planted samples, the ADPCM rounding, the XM pan law and the BGM fade step each fail them. Found: `VIWave::Create` sets volume 1 and pan 0 over the record's |
| `eqoa` (sound) | `audio.rs`: SNDDRV on the audio device's callback thread (cpal, 48 kHz, resampled when the device will not take it), the EE's SIF to it (a call at once, one waiting for a vblank left running and the link busy, as `sceSifCheckStatRpc`); `sound.rs`: `VIScene::SetSoundTrack` (the ambient tracker music of the terrain profile under the camera, day and night, its 15-second cross-fades, `amb_underwater` under water), the music regions (`VIStreamAudioSprite`, 0x2E00, on BGM slot 1), `VIBattleMusic`'s streamed half (a random `battleNN.bgm` on slot 0 while the player's combat byte is set), `VIUI::SoundPlay` and `PlayCue` (UI.CSF and FX.CSF resident and flushed up at start, names from UISOUND.TXT, volumes from SUPPORT's `sUISounds`: level up, dialogs, the answer box, `respawn` at world entry), and the characters' sounds (`vi::csprite`: the play list's sound per layer, fired by the animation's Primary/Secondary Sound triggers in `VICSprite::Process` at `CalcVolumeAndPan`'s volume and pan with a `rand()` pitch, and the continuous sound; a creature's waves shared while it is loaded); `vi::emit`: `QueryEmittersAtLocation` and the sprites' `QueryEmitters`, `VICamera`'s view matrix. `eqoa listen <world> <x> <z> <s> <out.wav> [hour] [walk] [cues] [fight]` renders it offline | the emitter query against brute force over every placed actor (`vi/tests/emitters.rs`: TUNARIA's 1,101 music regions, 4,404 points, keys and order equal; a moved key or the wrong room fails it). The scene logic is otherwise held to the decomp by reading and by its WAVs (footsteps every 400 ms walking, the battle track and back), not yet by an oracle. The weather's intensity damps the ambient track, and the atmosphere's rain (`rain_lt`, two voices) and thunder (`lightning_far02`) play out of SKY.CSF (resident). Not ported: the outdoors probe, sound sprites (SPELLFX's), groups' battle music |
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

- Above the login: the windows past the HUD, chat box, dialogs and start menu
  (guild windows, mail, map, options); the start menu's entries for them say so. Groups' actions
  are also on F5-F8. `drdp` leaves out
  endpoint transfer, the master and leader roles and ICMP (see its crate docs).

- Collision: `VICSprite`'s capsule (its sphere `Collide`) collides as nothing
  (the walk filters skip CSprites anyway; a ray never hits one). The ray half
  (`VICollRay`: the leaves and `CollideObj` against the oracle, the scene walk
  against brute force with the sphere's) is ported. `VIHSprite` props (900 placements of 67
  sprites in Tunaria) collide through each node attachment under its posed node,
  as `VIHSprite::Collide` does, but that is checked only by the BSP walk test,
  not against the oracle.
  The world level (BSP walk, rooms, actor insertion) is checked against brute
  force, not yet against the oracle: that needs eqoa-port to load the world.
- newlib's huge-angle `sinf`/`cosf` reduction (|x| > 2^7·π/2) and `acosf`'s
  |x| > 1 path are not ported (as in the Go); those fall back to the host.
- Character animation: `SetItemAction` (a no-op) is not ported. Death is
  (`SetDeath` against the animation oracle; creatures at state 14, the player at
  0x005E with `ResetCamera`/`DeathCamera`'s orbit and the 4-second fade, by reading).
  Weapon trails are (`vi::trail` against `oracle_trail`; `ProcessWeaponTrails`
  on the weapon group's two point sprites, by reading).
- `VICameraEffect`'s blend into a scripted camera (a quaternion slerp).
- Time of day: `InitTimeOfDay`'s table, `SetupTimeOfDayLights` (with the
  overcast weight and the lightning flash), `SetupTimeOfDayFog` (the four
  env-flag fogs too) and the sky colour of `SetupTimeOfDaySky` are ported
  (`vi::tod`), over the world's terrain-profile grid (`VIWorld::CalcProfileBlend`,
  `SetProfiles`). The hour runs from 10:00 at 24 h per hour (`EQOA_HOUR` pins
  it); a live game gets it from the server. The table, the profiles and the
  blend are held to the oracle; the Lights/Fog/Sky functions that combine them
  write the raster, so they are not.
- The sky and the weather (`eqoa/src/weather.rs`, `sky.rs`): the atmosphere
  is held to the oracle; in the client its two rays go through the scene's
  ray walk, the fireballs ride their own particle system, and the stars and lightning
  patches are drawn after the world at the far plane (the game draws them
  before it with Z off), which differs only behind a blended surface.
  `EQOA_WEATHER=t[,fade]` sets the weather in `shot`, `view` and `listen`;
  the server sets it with 0x00C5 (fade) and 0x00C7 (instant).
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
