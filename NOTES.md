# lane-create NOTES (dropped at merge)

Porting CLIENT's character creation into `crates/eqoa/src/pregame/client/create/`.

## Done
- `tables.rs`: gRaces / s_classes (decomp `virace.cpp`, `viclass.cpp`), GetRace/GetClass,
  IsValidRaceClassCityCombo(Expansion), IsStatHighlighted, sRaces, sClasses, sStats, sHomes,
  GetSexWord, CapitalizeName; CLIENT data read at run time: GetRaceCamera's review/cust framings
  (gRaceCameras/Ex), GetRaceInfo (gRaceInfo/Ex). Dedupe with the select lane's tables at merge.
- `mod.rs`: VIWndCharCreateBlank (the form, OnMessage 260/262-267, Focus, Reset), `Create`'s
  interface (new/open/is_open/frame/event/key/point/typing/scene + reset/form/current/page),
  the shared legend (`legend`, `legend_sized`: the bar measured on the word the decomp measures).
- `race.rs`, `class.rs`, `name.rs`, `misc.rs`, `review.rs`: the five pages, 2D + 3D + pointer.
- editbox.rs: + `set_allow_spaces`, `pos`, `end` (SetAllowSpaces, GetPos, GetEnd) — additive.
- Formatting: the house style is rustfmt, edition 2024, `max_width=200,use_small_heuristics=Max`
  (stage.rs, figure.rs, pointer.rs, editbox.rs are clean under it; no rustfmt.toml is committed).

## Checked
- 29 unit tests (`cargo test -p eqoa -- pregame::client::create`), each with a control; the whole
  crate 318 passed. Planted bugs, each failing at least one: the race list's expansion filter, the
  human cities' class masks swapped, CapitalizeName, the 20-point check, the cursor skipping
  invalid classes, a 3-letter name, the appearance values' wrap, Surefall's home word, the race
  list's scroll top, 265 routed to the appearance page, `create_character` not called, and the
  race and class pages' hit notes.
- Live on the local server (`vancreate`, TEMP hook): Vancrea (Elf female Bard, two points spent)
  made and seen in the list; Generate Name gave "Talthas"; a second Vancrea came back as
  `Event::CreateRefused(829)` (logged by the TEMP hook: the word on screen is character select's,
  CharSelectMsg; the form kept for a reopen is unit-tested, not live); pad only: Zandor (Dwarf
  female Cleric) made with the soft keyboard and seen.
  Music: barbaria → darkelve → dwarves, none on a sex change. Test characters deleted after.
- Shots in `~/.cache/vanilla-lane-create-shots/` (50-64 keyboard, 70-74 pad, 80-81 legend) against
  `~/.cache/vanilla-boot/ref2/50-56`.

## Judgment calls
- `Out::Done` comes when Create SENDS 0x002B (VIClient::CreateCharacter closes creation at once);
  the provisional entry, preview, word 413 and the server's answer are character select's
  (`form()` = what was sent). `open()` is `Focus()` (the form kept, as the console keeps it after
  a refusal); `reset()` is `Reset()` (CharacterList / BackToLogin).
- No BGM/VO1 voice-overs: no module (CLIENT, SUPPORT, SETUP, STATION) names VO1/VO2 files; the only
  creation music is the race page's `PlayBgm(0, race bgm)` (humans by city).
- Keys are queued and handled at the next frame (they need the session, the stage, the words).
- A look change rebuilds the body's GPU model (`redress`), keeping the sprite and its clock.
- The race cameras' review/cust framings are read in `tables.rs`, not `camstyle.rs` (no shared edit).

## For the merge
- Drop the TEMP commits (48f0a78, aab37ff): `client/mod.rs` is the select lane's.
- Shared code: the stage has no sky dome (the viewport shows the 2D backdrop where the PS2 shows
  sky); `Figure::new` puts armour set 0 tint 0 on where the decomp only sets hair and face (looks
  the same); the pregame host has no raster glyph notes, so each page notes its own legend.
