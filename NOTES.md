# lane-setup notes

Done (crates/eqoa/src/pregame/setup/), 18 tests, each failing with a planted bug:
- mod.rs: VISetup::Process's state machine (legal -> licence -> [movie left out] -> title), the
  window focus, the title's windows, RunPatcher's closing fade then Next::Station, the title music
  (Music::Xm "splash_screen", looped; stopped on leaving). Options = the client's options::WndOptions
  (CLIENT's VIWndOptions is SETUP's code but for its close); message box = textlist::Message.
- legal.rs (VIWndLegal), eula.rs (VIWndEula, ComputeTextRows' quirks kept), splash.rs (VIWndSplash),
  splashfx.rs (VISplashFx: birds/flock and Frontiers' island layer), fader.rs (VIFader, pub: STATION
  links the same file), netsetup.rs (the client's Network Setup page: server.opt + Shared::server).
- ui.rs: Layout::solid (the fader's blended black) and Layout::textured (the island's float quad).

For the coordinator (host.rs/viewer.rs are not this lane's):
- Setup::take_sound() -> Vec<OptionsUp> (Volume/Output): apply each frame as play.rs does
  (s.dev.set_sound_class_volume(c, v.0), s.dev.set_output_mode(st as i32)).
- Setup::capturing()/capture_key(name)/capture_button(bit): the Controls page in the title's Options.
- editbox::set_pad_only is never called in play (PAD_ONLY stays true): the soft keyboard never shows
  there. Network Setup sets it while its box is open and puts true back.
- Setup::start restarts at the legal screen (SLUS starts over after Station's Disconnect).
