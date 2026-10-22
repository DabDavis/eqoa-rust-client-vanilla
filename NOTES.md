# lane select: CLIENT's server list and character select

Owner of `crates/eqoa/src/pregame/client/` (not `create/`). Port of `viwndserverselect.cpp`,
`viwndcharacterselect.cpp`, `viwndcharcustfrontiers.cpp`, `viwnddisconnect.cpp`, `vishardinfo.cpp`
and VIClient's pregame half.

## Done
- `client/mod.rs`: VIClient's pregame half (m_p4F0E8 state, m_p00058 phase, m_p4F130, the windows
  on top / with the keys, `Req`uests windows make, `Disconnect`, `Msg0777Done`, `DisconnectKind`,
  `ConnectStatusString`, `CharacterList`, `SelectCharacter`, `DeleteCharacter`, `SetCharLook`,
  `CharSelectMsg`, `Msg0777`), `back_from_world(res, shared, Option<reason>)` for the coordinator.
- `server.rs`: the list (50-frame wait, ConnectForList, 4 retries, home shard from game.opt else
  first valid recommended = "Recommended", Sort with the memcmp half-name quirk, the message box).
- `select.rs`: slots, delete mode + VIWndMBox (dialog::MBox), Options box (Frontiers bit),
  busy box, ShowCharacter (race place / capped halls, look, items, attack action), ApplyCamera
  (race camera REVIEW fields), flourish every 5 s, race BGM on slot 0.
- `cust.rs`: VIWndCharCustFrontiers (rows, sliders, warm light, cust framing).
- `notice.rs`: VIWndDisconnect.
- `races.rs`: gRaces / s_classes as constants, tested against the decomp's source token for token.
- additive: `camstyle::RaceCamera.review/.cust`; `settings::home_shard/set_home_shard` (+0x98), and
  `settings::save` keeps the home shard.

## Judgment calls
- The memory card's checks, the "Saving game options" box (1615) and the card error box are left out.
- `Event::Disconnected` carries no server code (m_p4F0EC): reasons 0/10 show without "(code)".
- Customise rebuilds the figure when the look changes (the game re-dresses in place).
- Creation's interface: `Out::Done` is taken when the server's list arrives (CharacterList closes
  creation, the selection goes to the new character). VIClient::CreateCharacter's provisional
  entry (878 "Creating Character...") is only shown if creation hands the form back (not in the
  interface); refusals go to `create.event` while it is open, else to select's box.

## Left
- switch sessions to master's `link::session`; live checks; shots vs ref.
