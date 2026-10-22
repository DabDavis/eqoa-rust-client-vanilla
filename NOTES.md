# lane-create NOTES (dropped at merge)

Porting CLIENT's character creation into `crates/eqoa/src/pregame/client/create/`.

## Done
- `tables.rs`: gRaces / s_classes (decomp `virace.cpp`, `viclass.cpp`), GetRace/GetClass,
  IsValidRaceClassCityCombo(Expansion), IsStatHighlighted, sRaces, sClasses, sStats, sHomes,
  GetSexWord, CapitalizeName; CLIENT data read at run time: GetRaceCamera's review/cust framings
  (gRaceCameras/Ex), GetRaceInfo (gRaceInfo/Ex).
- `mod.rs`: VIWndCharCreateBlank (the form, OnMessage 260/262-267, Focus, Reset), `Create`'s
  interface (new/open/is_open/frame/event/key/point/typing/scene + reset/form/current/page).
- `race.rs`, `class.rs`, `name.rs`, `misc.rs`, `review.rs`: the five pages, 2D + 3D + pointer.
- editbox.rs: + `set_allow_spaces`, `pos`, `end` (SetAllowSpaces, GetPos, GetEnd) — additive.

## Judgment calls
- `Out::Done` comes when Create SENDS 0x002B (VIClient::CreateCharacter closes creation at once);
  the provisional entry, preview, word 413 and the server's answer are character select's
  (`form()` = what was sent). `open()` is `Focus()` (the form kept, as the console keeps it after
  a refusal); `reset()` is `Reset()` (CharacterList / BackToLogin).
- No BGM/VO1 voice-overs: no module (CLIENT, SUPPORT, SETUP, STATION) names VO1/VO2 files; the only
  creation music is the race page's `PlayBgm(0, race bgm)` (humans by city).
- Keys are queued and handled at the next frame (they need the session, the stage, the words).
- A look change rebuilds the body's GPU model (`redress`), keeping the sprite and its clock.

## Left
- live run on the local server (TEMP hook), shots vs ref2/50-56, planted-bug checks.
