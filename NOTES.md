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

## Next
- merge master (`pregame::link`), the API state machine on `link::Stream`, the news on
  `link::HttpGet`, the renderer, the stage glue, editbox password stars, Save Account.
