# lane-station NOTES

Porting STATION (the Station login) into `crates/eqoa/src/pregame/station/`.

## Done
- `net/src/auth.rs`: the Station API's other requests (56 create, 42 password, 44/48, 46 billing,
  50/52, 54, 62, 64, 66 forgot, 2 ping) and every reply `OnMessage` dispatches (`parse_reply`), at
  the decomp's offsets: a reply is the message less its length word, so the login reply's `result`
  is wire 12 (go-eqoa-server's error flag at wire 8 is never read by STATION).
- `station/pak.rs` (`VIFlatFileSystem`), `station/xml.rs` (what libxml 2.3.6's `xmlParseMemory`
  accepts, as a tree; strict: the disc's ChangeCC.xml is refused, as on the console),
  `station/page.rs` (the parser's handlers, `VIGUIObject`/`Text`/`Button`/`ToggleButton`/
  `ComboBox`, `VIGUIPage` with its selection grid, selector and pulse), `station/dialog.rs`
  (`VIGUIDialog`), `station/fader.rs` (`VIFader`).
- `ui.rs`: `Layout::fill_blended` (additive; STATION's fills blend, `fill_fan` is opaque).

- merged master (`pregame::link`). `station/api.rs`: `VIStationAPI` over `link::Stream` (the
  connection counts as up at the server's first bytes, its hello: a stream has no connect event).
- `station/renderer.rs` (`VIWndGenericRenderer`: Load, PushPage/PageBack, OnAction's table, the
  listener's calls, SaveAccount, the news over `link::HttpGet`), `station/mod.rs` (the stage:
  `VIStation`'s frame twice a game frame, the first into `Quiet`; the fader; keys; pointer).
- `settings.rs`: `saved_login`/`save_login` (+0x10, +0x50, +0x90, plain text as the card kept
  it), `save` keeps them (was: always zero). `editbox.rs`: `password` (`m_x18C`, stars).

- tests on the disc's pages over a scripted server (`renderer.rs`): the login's bytes and both
  answers, Save Account (ticked and not), a new account (both pages, the checks, the request
  bytes, the login after), a password change, the news and Play, Disconnect; the stage's pointer.
- live (local go-eqoa-server): login as vanstation through the pages, news, Play, CLIENT's list.

## Next
- live: a new account, a password change, Save Account; shots at 640x480 against ref2; planted
  bugs; README.
