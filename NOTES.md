# lane-station NOTES

Porting STATION (the Station login) into `crates/eqoa/src/pregame/station/`.

## Done
- `net/src/auth.rs`: the Station API's other requests (56 create, 42 password, 44/48, 46 billing,
  50/52, 54, 62, 64, 66 forgot, 2 ping) and every reply `OnMessage` dispatches (`parse_reply`), at
  the decomp's offsets: a reply is the message less its length word, so the login reply's `result`
  is wire 12 (go-eqoa-server's error flag at wire 8 is never read by STATION).

## Next
- pak reader, XML parser, GUI page objects, dialog, fader, API state machine, renderer, stage glue.
