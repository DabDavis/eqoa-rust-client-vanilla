# lane select: CLIENT's server list and character select

Owner of `crates/eqoa/src/pregame/client/` (not `create/`). Port of `viwndserverselect.cpp`,
`viwndcharacterselect.cpp`, `viwndcharcustfrontiers.cpp`, `viwnddisconnect.cpp`, `vishardinfo.cpp`
and VIClient's pregame half.

## Done
- `client/mod.rs`: VIClient's pregame half (m_p4F0E8 state, m_p00058 phase, m_p4F130, the windows
  on top / with the keys, `Req`uests windows make, `Disconnect`, `Msg0777Done`, `DisconnectKind`,
  `ConnectStatusString`, `CharacterList`, `SelectCharacter`, `DeleteCharacter`, `SetCharLook`,
  `CharSelectMsg`, `Msg0777`), `back_from_world(res, shared, Option<reason>)` (master's
  `Pregame::back` calls it). Sessions open through `link::session`.
- Creation's contract: `Out::Done` is `CreateCharacter` (the sent form as the next entry, server id
  -1, `create::tables::race(r).model_id(sex)`, no robe, level 1; selected; `ShowWord(413)`);
  `Event::CreateRefused(w)` is the dispatcher's 0x002E/0x0065/0x002F (the word in select's box,
  `m_charCount--`); `Event::Characters` resets creation (`CharacterList`'s `Reset`) and leaves the
  selection where it is; `Out::Back` is `BackToLogin`; 0x0012 goes to `create.event` whatever is up.
- `server.rs`: the list (50-frame wait, ConnectForList, 4 retries, home shard from game.opt else
  first valid recommended = "Recommended", Sort with the memcmp half-name quirk, the message box).
- `select.rs`: slots, delete mode + VIWndMBox (dialog::MBox), Options box (Frontiers bit),
  busy box, ShowCharacter (race place / capped halls, look, items, attack action), ApplyCamera
  (race camera REVIEW fields), flourish every 5 s, race BGM on slot 0.
- `cust.rs`: VIWndCharCustFrontiers (rows, sliders, warm light, cust framing, the hair rows'
  `SetOrbitAngle`, the model at select's spot and angles + 5.934119).
- `notice.rs`: VIWndDisconnect.
- `races.rs`: gRaces / s_classes as constants, tested against the decomp's source token for token.
  DUPLICATES `create/tables.rs`'s data (the coordinator dedupes); its four creation helpers are
  unused here (creation has its own).
- additive: `camstyle::RaceCamera.review/.cust`; `settings::home_shard/set_home_shard` (+0x98), and
  `settings::save` keeps the home shard.

## Checked live (local server, account vanselect)
- list ("Marr's Fist", Recommended / Ready with a saved home shard), the three characters'
  previews in their race sets, the empty slot, the Options box, customise (face, hair length with
  the camera turned), back, Enter Game World into the world; a throwaway (Vanseldel, elf ranger,
  made with mkchar) deleted through the box. Shots `~/.cache/vanilla-lane-select-shots/`
  (`c*`, `d/t*`, `w/w*`) against `~/.cache/vanilla-lane-select-ref/shots/`.

## Judgment calls
- The memory card's checks, the "Saving game options" box (1615) and the card error box are left out.
- `Event::Disconnected` carries no server code (m_p4F0EC): reasons 0/10 show without "(code)".
- Customise rebuilds the figure when the look changes (the game re-dresses in place).
- The provisional entry's preview wears armour set 0 (`Figure` always applies a look; the console's
  `ShowCharacter` with null arrays leaves the sprite's own until the list replaces the entry).
- The journal's auto flag (m_p50300: auto-connect to shard 0, `Option(1)` on the list) is 0, as the
  retail run against eqoa-port showed.

## Left
- creation live through select (needs the merged STATION login in the run).
