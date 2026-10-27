#!/usr/bin/env python3
"""pagetest -- the page's own behaviour, in a headless Chromium: updates,
problem reports, crash reports and what the page catches.

Serves a copy of web/dist (build.sh's output) with a stand-in for the report
server (cmd/reportd's /report and /crash, checked as it checks them, kept in
memory), then plays deploys against an open page by writing new builds and
version.json under it:

  update    an untouched page reloads into a new build at once; one with a
            disc picked is told, with what is new in it (the sections of its
            update notes the page's lack), and Later holds for its while
            (`?later=`) or until a newer build; Reload
            brings the newest; a reload that still brings the old page is a
            problem, not a second reload
  report    F3 opens the problem dialog; Send posts what reportd takes, the
            account name and password taken out
  crash     a panic opens the crash dialog; it posts to /crash, and F3 opens
            nothing new after it
  offer     a tab that died while shown is offered next visit; one hidden is
            not
  missing   a page whose build is gone says so in its status line
  hidden    a hidden page's game is ticked at 30 frames a second (tick.js, a
            worker's timer, which a hidden page's own are not), a shown one's
            not at all
  disc      the disc is named when picked (with a second image in --other-iso)
  2002      the 2002 disc (--iso-2002) is taken in the second input, the
            Frontiers disc there refused; F9's ask opens its picker inside
            the key's activation (not without one) and puts up its notice,
            whose button opens it again and takes the disc picked
  remember  with Remember ticked a disc's handle is kept (a file of the
            origin's own storage stands in for one): the next visit puts it
            back in its input, or offers it on a button when the browser must
            ask; unticked it is forgotten; a file moved since is said so; a
            ticked click opens the picker that can be remembered; ticked
            after a pick, that picker opens at once (or a button offers it);
            Start asks the browser for a kept disc in its click; a picker
            that can't start in the kept folder opens without it; a report
            says what is remembered (the picker is a stand-in: the real one
            can't be driven headless)
  typed     while the client says an edit box has the keys, a touch puts
            the focus in the page's input (a phone's own keyboard comes up
            with it), and what is typed there comes to the canvas as key
            events, none of it kept; with none, a touch puts no keyboard up
  spellfx   the community page (spellfx.html): the server's spells (a
            stand-in answers <api>/spells), filtered by class and by a
            stand-in visual; a pick needs a spell and a visual; the pick is
            sent as the server takes it, a refusal said, the name kept for the
            next visit; a cast lands by the spell (a harmful one at a second
            human, never the dummy), or on the caster and a second human at
            once, kept; W and S step through the visuals from the page and
            from the preview, not while typing; the game's page links it (the
            preview and the disc's visuals are drive's: a visual list is
            handed over as the preview sends it)
  fullscreen  Start puts the page full screen in its click where it can stay
            so: a computer whose browser keeps Escape for the game (Keyboard
            Lock), Escape locked; a touch device, where a touch on the game
            puts it back once it is left, not while an edit box has the
            keyboard (the touch puts that up). Not without Keyboard Lock (a
            stand-in), not with another window of the game open, not on an
            iPhone; a computer's left stays left at a click
  install   the page names its build's manifest, Chromium finds nothing in
            the way of installing it (a page without one: errors), the icons
            are PNGs in the build; the card's button (a stand-in offer) asks
            the browser's prompt, and says what came of it, a report too; a
            browser that has not offered it: nothing on a computer, its menu
            on a phone (Firefox's named), the Home Screen on an iPhone; none
            of it started as the app
  software  a browser drawing without the graphics card (a stand-in: no
            WebGPU, WebGL2 naming Windows' Microsoft Basic Render Driver; then
            the client's graphics line naming it) is told so on the card's
            line, in a notice (once) and on the status line in the world; a
            graphics card is not

Each check has its negative control. Needs the `websockets` package.

    web/build.sh && web/pagetest.py --iso run/eqoa.iso
    web/pagetest.py --serve --http-port 8794   # the site alone, for drive.py --base
"""
import argparse, asyncio, functools, http.server, json, os, re, shutil, socketserver, subprocess, sys, tempfile, threading, time, urllib.request
from pathlib import Path

import websockets

HERE = Path(__file__).resolve().parent

# what the community page's server (go-eqoa-server's account service) answers
# at <api>/spells, four spells of four classes, three still on a stand-in (Test Bolt's visual is set)
SPELLFX_SPELLS = [
    {"id": 1, "name": "Test Bolt", "level": 3, "classes": [13], "line": "Target, Decrease Hitpoints by 10", "harmful": True,
     "target": "Target", "cast": 2.0, "visual": "0x11111111", "standin": False},
    {"id": 2, "name": "Test Heal", "level": 1, "classes": [9, 7], "line": "Target, Increase HP by 20", "harmful": False,
     "target": "Target", "cast": 1.5, "visual": "0x22222222", "standin": True, "kind": "heal"},
    {"id": 3, "name": "Test Ward", "level": 5, "classes": [9], "line": "Self, Increase AC by 5, 30min", "harmful": False,
     "target": "Self", "cast": 3.0, "visual": "0x33333333", "standin": True, "kind": "buff"},
    {"id": 4, "name": "Test Tap", "level": 8, "classes": [11], "line": "Target, Drain HP by 30", "harmful": True,
     "target": "Target", "cast": 2.5, "visual": "0x44444444", "standin": True, "kind": "damage", "got": "master class", "master": "Lich"},
]
SPELLFX_API = "/eqoa/account/api/spellfx"


class Site:
    """A copy of web/dist to play deploys on, and what was reported."""

    def __init__(self, dist):
        tmp = Path("~/.cache/eqoa-rs-web").expanduser()
        tmp.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix="site-", dir=tmp))
        shutil.copytree(dist, self.root, dirs_exist_ok=True)
        self.first = json.loads((self.root / "version.json").read_text())["build"]
        self.page = (dist / "index.html").read_text()
        self.posts = []
        self.keep = None
        # the community page's picks, as its server was sent them
        self.picks = []

    def deploy(self, build, page=True, version=True, notes=None):
        """A build named `build` (a copy of the first's files, `notes` put on
        top of its update notes): its directory, then the page naming it,
        then version.json, as deploy.sh goes."""
        d = self.root / "v" / build
        if not d.exists():
            shutil.copytree(self.root / "v" / self.first, d)
            (d / "build.json").write_text(json.dumps({"build": build, "commit": "test"}))
            if notes:
                old = (d / "update-notes.md").read_text()
                (d / "update-notes.md").write_text(old.replace("\n## ", f"\n{notes}\n## ", 1))
        if page:
            index = self.root / "index.html"
            index.write_text(self.page.replace(f"v/{self.first}/", f"v/{build}/"))
            # each build's page later than the last, as builds are made
            # minutes apart: the server's revalidation goes by whole seconds
            self.made = max(getattr(self, "made", 0), time.time()) + 2
            os.utime(index, (self.made, self.made))
        if version:
            (self.root / "version.json").write_text(json.dumps({"build": build, "commit": "test"}))

    def serve(self, port):
        site = self

        class Handler(http.server.SimpleHTTPRequestHandler):
            extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, ".wasm": "application/wasm", ".js": "text/javascript"}

            def log_message(self, *a):
                pass

            def end_headers(self):
                # as nginx: the page and version.json revalidated, builds kept
                self.send_header("Cache-Control", "public, max-age=31536000, immutable" if self.path.startswith("/v/") else "no-cache")
                super().end_headers()

            def answer_json(self, code, answer):
                data = json.dumps(answer).encode()
                self.send_response(code)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def do_GET(self):
                if self.path.split("?")[0] == SPELLFX_API + "/spells":
                    return self.answer_json(200, {"spells": SPELLFX_SPELLS})
                return super().do_GET()

            def do_POST(self):
                route = self.path.split("?")[0]
                body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
                code, answer = 404, {"ok": False, "error": "no such route"}
                if route in ("/reports/report", "/reports/crash"):
                    code, answer = site.take(route.rsplit("/", 1)[1], body)
                elif route == SPELLFX_API + "/suggest":
                    code, answer = site.pick(body)
                data = json.dumps(answer).encode()
                self.send_response(code)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

        socketserver.ThreadingTCPServer.allow_reuse_address = True
        httpd = socketserver.ThreadingTCPServer(("127.0.0.1", port), functools.partial(Handler, directory=str(self.root)))
        httpd.daemon_threads = True
        threading.Thread(target=httpd.serve_forever, daemon=True).start()
        return httpd

    def pick(self, body):
        """The community page's pick, checked as its server checks it (the
        contract: a spell, a visual, a name and a note of their lengths);
        the name 'flood' is refused as an address that sent too many."""
        try:
            req = json.loads(body)
        except ValueError:
            return 400, {"error": "bad body"}
        if req.get("name") == "flood":
            return 429, {"error": "Too many picks from here: try again in an hour."}
        ok = (isinstance(req.get("spell"), int) and isinstance(req.get("visual"), str) and req["visual"].startswith("0x")
              and isinstance(req.get("name", ""), str) and len(req.get("name", "")) <= 32
              and isinstance(req.get("note", ""), str) and len(req.get("note", "")) <= 200)
        if not ok:
            return 400, {"error": "a spell, a visual, a name of 32 and a note of 200 at most"}
        self.picks.append(req)
        return 200, {"ok": True, "count": sum(1 for p in self.picks if p["spell"] == req["spell"])}

    def take(self, kind, body):
        """reportd's checks on a /report or a /crash (cmd/reportd/main.go)."""
        try:
            req = json.loads(body)
        except ValueError:
            return 400, {"ok": False, "error": "bad body"}
        fields = {"report": 512 * 1024, "note": 1000, "version": 64, "platform": 64, "channel": 64}
        if kind == "report":
            fields.update(name=64, screenshot=8 * 1024 * 1024)
        else:
            fields["report"] = 256 * 1024
            fields["note"] = 200
        for k, limit in fields.items():
            v = req.get(k, "")
            if not isinstance(v, str) or len(v) > limit:
                return 400, {"ok": False, "error": f"bad {k}"}
        if len(req["report"].strip()) < 32:
            return 400, {"ok": False, "error": "empty report"}
        self.posts.append((kind, req))
        if self.keep:
            (self.keep / f"{len(self.posts):03d}-{kind}.json").write_text(json.dumps(req))
        return 200, {"ok": True, "id": f"test-{len(self.posts)}"}


class Page:
    """One Chromium tab over the DevTools protocol."""

    def __init__(self, ws):
        self.ws, self.seq, self.pending, self.lines = ws, 0, {}, []
        # file choosers the page opened (Page.setInterceptFileChooserDialog)
        self.choosers = []

    async def reader(self):
        async for raw in self.ws:
            m = json.loads(raw)
            if "id" in m and m["id"] in self.pending:
                self.pending.pop(m["id"]).set_result(m.get("result", m.get("error")))
            elif m.get("method") == "Page.fileChooserOpened":
                self.choosers.append(m["params"])
            elif m.get("method") == "Runtime.consoleAPICalled":
                self.lines.append(" ".join(str(x.get("value", x.get("description", ""))) for x in m["params"]["args"]))
            elif m.get("method") == "Runtime.exceptionThrown":
                d = m["params"]["exceptionDetails"]
                self.lines.append("EXCEPTION " + d.get("exception", {}).get("description", d.get("text", "")))

    async def call(self, method, **params):
        self.seq += 1
        fut = asyncio.get_running_loop().create_future()
        self.pending[self.seq] = fut
        await self.ws.send(json.dumps({"id": self.seq, "method": method, "params": params}))
        return await asyncio.wait_for(fut, 30)

    async def js(self, expr):
        r = await self.call("Runtime.evaluate", expression=expr, awaitPromise=True, returnByValue=True)
        return (r or {}).get("result", {}).get("value")

    async def until(self, expr, secs=15):
        end = time.monotonic() + secs
        while time.monotonic() < end:
            try:
                v = await self.js(expr)
            except (asyncio.TimeoutError, websockets.ConnectionClosed):
                v = None
            if v:
                return v
            await asyncio.sleep(0.1)
        return None

    async def open(self, url):
        await self.call("Page.navigate", url=url)
        return await self.ready()

    async def ready(self):
        return await self.until("document.readyState === 'complete' && document.getElementById('status').textContent === 'ready' && window.eqoaBuild")

    async def stay(self):
        """Marks this page: a reload clears the mark (`reloaded`)."""
        await self.js("window.__stay = 1")

    async def stayed(self):
        return await self.js("window.__stay") == 1

    async def reloaded(self):
        return await self.until("!window.__stay && document.readyState === 'complete' && document.getElementById('status').textContent === 'ready' && window.eqoaBuild")

    async def key(self, k, code, vk):
        for kind in ("keyDown", "keyUp"):
            await self.call("Input.dispatchKeyEvent", type=kind, key=k, code=code, windowsVirtualKeyCode=vk)

    async def pick(self, iso, selector="#iso"):
        doc = await self.call("DOM.getDocument")
        node = await self.call("DOM.querySelector", nodeId=doc["root"]["nodeId"], selector=selector)
        await self.call("DOM.setFileInputFiles", nodeId=node["nodeId"], files=[str(Path(iso).resolve())])

    async def gesture(self, expr):
        """`expr` evaluated as from the user's own gesture (its activation)."""
        r = await self.call("Runtime.evaluate", expression=expr, userGesture=True, awaitPromise=True, returnByValue=True)
        return (r or {}).get("result", {}).get("value")


async def tests(a, site, base, page):
    failed = []

    def check(name, ok, detail=""):
        print(f"{'ok  ' if ok else 'FAIL'} {name}{': ' + detail if detail and not ok else ''}", flush=True)
        if not ok:
            failed.append(name)

    notices = "JSON.stringify([...document.querySelectorAll('#notices .notice')].map(n => n.dataset.key))"
    q = "?reports=/reports&later=3000"

    # ---- update
    first = site.first
    check("the page is its build", await page.open(base + q) == first, str(await page.js("window.eqoaBuild")))

    # ---- iOS: its pickers grey a disc image out under the inputs' accept
    # (every browser there is WebKit), so the page drops it there (main.js
    # `onIOS`); the control, a desktop page keeps it
    accepts = "document.getElementById('iso').accept + '|' + document.getElementById('iso2002').accept"
    check("a desktop page's disc inputs keep their filter (control)", await page.js(accepts) == ".iso,.bin,.img|.iso,.bin,.img")
    ua = await page.js("navigator.userAgent")
    await page.call("Emulation.setUserAgentOverride", userAgent="Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/131.0.6778.73 Mobile/15E148 Safari/604.1")
    await page.open(base + q)
    check("on an iPhone they take any file", await page.js(accepts) == "|", str(await page.js(accepts)))
    await page.call("Emulation.setUserAgentOverride", userAgent=ua)
    await page.open(base + q)
    check("and back on the desktop, filtered again", await page.js(accepts) == ".iso,.bin,.img|.iso,.bin,.img")
    await page.stay()
    latest = await page.js("eqoaCheckUpdate()")
    await asyncio.sleep(0.5)
    check("no new build: no reload, no notice (control)", latest == first and await page.stayed() and await page.js(notices) == "[]")

    site.deploy("b" * 12)
    await page.js("eqoaCheckUpdate()")
    check("an untouched page reloads into a new build", await page.reloaded() == "b" * 12)

    await page.pick(a.iso)
    await page.stay()
    site.deploy("c" * 12)
    await page.js("eqoaCheckUpdate()")
    shown = await page.until(f"{notices}.includes('update')", 5)
    check("a page with a disc picked is told, not reloaded", bool(shown) and await page.stayed() and await page.js("window.eqoaBuild") == "b" * 12)
    told = "document.querySelector('#notices .notice[data-key=update] .text').textContent"
    check("no notes new to it: no What's new (control)", "What's new" not in await page.js(told))
    await page.js("[...document.querySelectorAll('#notices .notice button')].find(b => b.textContent === 'Later').click()")
    await page.js("eqoaCheckUpdate()")
    check("Later holds for that build", "update" not in await page.js(notices))
    await asyncio.sleep(3.2)
    await page.js("eqoaCheckUpdate()")
    check("Later's while over, it is told again", bool(await page.until(f"{notices}.includes('update')", 5)))
    await page.js("[...document.querySelectorAll('#notices .notice button')].find(b => b.textContent === 'Later').click()")
    site.deploy("d" * 12, notes="## 2099-01-01 · A test section\n- A test bullet, **bold**\n  carried on\n")
    await page.js("eqoaCheckUpdate()")
    check("a newer build is told again after Later", bool(await page.until(f"{notices}.includes('update')", 5)))
    text = await page.js(told)
    check("the notice says what is new in it", "What's new:\n2099-01-01 · A test section\n• A test bullet, bold carried on" in text, repr(text[-160:]))
    await page.js("[...document.querySelectorAll('#notices .notice button')].find(b => b.textContent === 'Reload').click()")
    check("Reload brings the newest", await page.reloaded() == "d" * 12)

    # version.json moves on and the page stays behind (a cache keeping it):
    # one reload, then a problem, not a reload every minute
    site.deploy("e" * 12, page=False)
    await page.stay()
    await page.js("eqoaCheckUpdate()")
    still = await page.reloaded()
    await page.stay()
    await page.js("eqoaCheckUpdate()")
    stale = await page.until(f"{notices}.includes('problem:stale-page')", 5)
    check("a reload that brings the old page is a problem, not a loop", still == "d" * 12 and bool(stale) and await page.stayed())
    site.deploy("e" * 12)

    # ---- report
    await page.open(base + q)
    # what the client names when it sends Station a login (web::secret)
    await page.js("eqoaSecret('tester1', 's3cretpw'); console.log('a line with tester1 and s3cretpw in it')")
    await page.js("document.getElementById('eqoa').focus()")
    await page.key("F3", "F3", 114)
    opened = await page.until("!document.getElementById('report').hidden && document.getElementById('rp-title').textContent", 5)
    check("F3 opens the problem dialog", opened == "Report a problem", str(opened))
    await page.js("document.getElementById('rp-note').value = 'the note'; document.getElementById('rp-name').value = 'Tester'; document.getElementById('rp-send').click()")
    sent = await page.until("document.getElementById('rp-done').textContent.startsWith('Sent as')", 10)
    kind, req = site.posts[-1] if site.posts else (None, {})
    report = req.get("report", "")
    check("Send posts a problem report reportd takes", bool(sent) and kind == "report" and req.get("channel") == "web"
          and req.get("version", "").startswith("web-") and req.get("note") == "the note" and req.get("name") == "Tester"
          and 0 < len(req.get("platform", "")) <= 64 and "--- trace ---" in report and "[log]" in report, json.dumps({k: v[:80] for k, v in req.items()}))
    check("the account name and the password are taken out", "a line with <account> and <password> in it" in report
          and "tester1" not in report and "s3cretpw" not in report)
    check("the log ring had them (control)", any("a line with tester1 and s3cretpw" in l for l in page.lines))
    await page.js("document.getElementById('rp-close').click()")
    # the picture is the player's to leave out (F3 20261023-083108): its box
    # unticked, the report goes with none; ticked (the default), with one. A
    # stand-in client hands over a frame, as the game does when one is drawn.
    await page.js("window._client = window.eqoaClient; window.eqoaClient = { wantShot() { setTimeout(() => window.eqoaFrameDrawn && window.eqoaFrameDrawn(), 30); } }; eqoaReport.running(true)")
    shots = []
    for keep in (False, True):
        await page.js("document.getElementById('eqoa').focus()")
        await page.key("F3", "F3", 114)
        row = await page.until("!document.getElementById('report').hidden && !document.getElementById('rp-shot-row').hidden", 5)
        n = len(site.posts)
        await page.js(f"{{ const b = document.getElementById('rp-shot'); b.checked = {'true' if keep else 'false'}; b.dispatchEvent(new Event('change'));"
                      " document.getElementById('rp-note').value = 'picture test'; document.getElementById('rp-send').click() }")
        await page.until("document.getElementById('rp-done').textContent.startsWith('Sent as')", 10)
        kind, req = site.posts[-1] if len(site.posts) > n else (None, {})
        shots.append((bool(row), kind, req.get("note"), len(req.get("screenshot", ""))))
        await page.js("document.getElementById('rp-close').click()")
    await page.js("eqoaReport.running(false); window.eqoaClient = window._client")
    check("a picture unticked is left out", shots[0][:3] == (True, "report", "picture test") and shots[0][3] == 0, str(shots))
    check("ticked, the default, it goes (control)", shots[1][:3] == (True, "report", "picture test") and shots[1][3] > 0, str(shots))
    # the mark a running game keeps for a tab that dies is scrubbed as it is
    # kept: the next visit may not know what this one signed in with
    await page.js("console.log('kept tester1 and s3cretpw'); eqoaReport.running(true)")
    kept = await page.js("localStorage.getItem('eqoa.session') || ''") or ""
    check("the mark kept has them taken out", "kept <account> and <password>" in kept and "s3cretpw" not in kept, kept[:120])
    check("the log ring had them when it was kept (control)", any("kept tester1 and s3cretpw" in l for l in page.lines))
    await page.js("eqoaReport.running(false); localStorage.removeItem('eqoa.session')")

    # ---- pointer: the mouse look's pointer lock refused (winit drops the
    # promise the browser answers with) is no problem of the page's (F3
    # 20261023-193535), only a line in the log; a rejection of the page's own
    # nobody handles is one (the control)
    await page.open(base + q)
    await page.js("window.__refused = 0; document.addEventListener('pointerlockerror', () => { window.__refused += 1 }); document.getElementById('eqoa').requestPointerLock(); 0")
    refused = await page.until("window.__refused > 0", 5)
    await asyncio.sleep(0.5)
    shown = await page.js(notices) or ""
    check("a refused pointer lock is no problem", bool(refused) and "problem:page" not in shown
          and any(l.startswith("the pointer lock was refused") for l in page.lines), f"refused {refused}, notices {shown}")
    await page.js("Promise.reject(new Error('a test rejection')); 0")
    check("an unhandled rejection is one (control)", bool(await page.until(f"{notices}.includes('problem:page')", 5)))

    # ---- crash
    n = len(site.posts)
    await page.js("try { eqoaClient.crashTest() } catch (e) { window.__trap = String(e) }")
    title = await page.until("!document.getElementById('report').hidden && document.getElementById('rp-title').textContent", 5)
    check("a panic opens the crash dialog", title == "The client stopped", str(title))
    await page.js("document.getElementById('rp-note').value = 'crashed'; document.getElementById('rp-send').click()")
    await page.until("document.getElementById('rp-done').textContent.startsWith('Sent')", 10)
    kind, req = site.posts[-1] if len(site.posts) > n else (None, {})
    check("the crash report goes to /crash with the panic and the log", kind == "crash" and "crash_test: the client stopped on purpose" in req.get("report", "")
          and "[log]" in req.get("report", "") and req.get("note") == "crashed")
    await page.js("document.getElementById('rp-close').click()")
    await page.key("F3", "F3", 114)
    await asyncio.sleep(0.5)
    check("after a crash F3 opens no problem report", await page.js("document.getElementById('rp-title').textContent") == "The client stopped")

    # ---- offer: the mark a tab leaves when it dies while shown. The tab
    # dies for real (its renderer crashed: no pagehide takes the mark away).
    async def died(hidden):
        await page.open(base + q)
        mark = json.dumps({"when": int(time.time() * 1000), "build": "x", "version": "web-x", "hidden": hidden, "state": "s", "log": ["l"]})
        await page.js(f"localStorage.setItem('eqoa.session', {json.dumps(mark)})")
        asyncio.ensure_future(page.call("Page.crash")).add_done_callback(lambda f: f.exception())
        await asyncio.sleep(1.5)
        await page.open(base + q)
        await asyncio.sleep(0.5)
        return await page.js(notices) or ""

    check("a tab that died while shown is offered", "last-session" in await died(False))
    check("one that was hidden is not (control)", "last-session" not in await died(True))
    await page.js("localStorage.removeItem('eqoa.session')")

    # ---- missing: a page naming a build that is not there
    (site.root / "gone.html").write_text(site.page.replace(f"v/{site.first}/", "v/" + "f" * 12 + "/"))
    await page.call("Page.navigate", url=base + "gone.html" + q)
    said = await page.until("document.getElementById('status').textContent.includes('did not load')", 10)
    check("a page whose build is gone says so", bool(said))

    # ---- hidden: another tab in front (the browser's own hiding: no
    # animation frames, the page's timers held back); the ticker runs the
    # game while the page is hidden (web::hidden_tick), never while it is
    # shown
    def devtools(path, method="GET"):
        with urllib.request.urlopen(urllib.request.Request(f"http://127.0.0.1:{a.port}/json/{path}", method=method)) as r:
            body = r.read()
        return json.loads(body) if body.strip().startswith(b"{") else body

    await page.open(base + q)
    ticks = "window.eqoaHiddenTicks || 0"
    check("the page has a ticker for when it is hidden", await page.js("typeof window.eqoaHiddenTicks") == "number")
    t0 = await page.js(ticks)
    await asyncio.sleep(1.5)
    check("a shown page's game is not ticked (control)", await page.js(ticks) == t0 and await page.js("document.visibilityState") == "visible")
    blank = devtools("new?about:blank", "PUT")
    devtools(f"activate/{blank['id']}")
    await asyncio.sleep(0.5)
    t1 = await page.js(ticks)
    await asyncio.sleep(3)
    n = await page.js(ticks) - t1
    vis = await page.js("document.visibilityState")
    check("a hidden page's game is ticked at 30 frames a second", vis == "hidden" and 75 <= n <= 100, f"{vis}, {n} ticks in 3 s")
    devtools(f"activate/{a.target_id}")
    devtools(f"close/{blank['id']}")
    await asyncio.sleep(0.5)
    t2 = await page.js(ticks)
    await asyncio.sleep(1.5)
    check("shown again, the ticks stop", await page.js(ticks) == t2 and await page.js("document.visibilityState") == "visible")

    # ---- disc
    await page.open(base + q)
    await page.pick(a.iso)
    line = await page.until("document.getElementById('iso-what').className.includes('ok') && document.getElementById('iso-what').textContent", 10)
    check("the Frontiers disc is named", bool(line) and "SLUS-20744" in line, str(line))
    if a.other_iso:
        await page.pick(a.other_iso)
        line = await page.until("document.getElementById('iso-what').className.includes('warn') && document.getElementById('iso-what').textContent", 10)
        check("another disc is named, and what it lacks", bool(line) and "Frontiers" in line, str(line))

    # ---- 2002: the second disc, judged by the client's rules (era.rs)
    if a.iso_2002:
        await page.open(base + q)
        what = "document.getElementById('iso2002-what')"
        await page.pick(a.iso_2002, "#iso2002")
        line = await page.until(f"{what}.className.includes('ok') && {what}.textContent", 15)
        check("the 2002 disc is taken in the second input", bool(line) and "SLUS-20470" in line and "F9" in line, str(line))
        await page.pick(a.iso, "#iso2002")
        line = await page.until(f"{what}.className.includes('warn') && {what}.textContent", 15)
        check("the Frontiers disc there is refused with the client's line (control)", bool(line) and "Frontiers disc the game runs off" in line, str(line))
        # F9's ask (web::hooks_2002 calls window.eqoaAsk2002): the picker opens
        # in a key's activation, not from a script without one; its notice
        # either way
        await page.call("Page.setInterceptFileChooserDialog", enabled=True)
        n = len(page.choosers)
        await page.js("eqoaAsk2002()")
        await asyncio.sleep(0.5)
        quiet = len(page.choosers) == n
        shown = await page.until(f"{notices}.includes('disc2002')", 5)
        check("without an activation the picker does not open, the notice does (control)", quiet and bool(shown), f"choosers {len(page.choosers) - n}")
        await page.gesture("eqoaAsk2002()")
        for _ in range(20):
            if len(page.choosers) > n:
                break
            await asyncio.sleep(0.1)
        check("in an activation the ask opens the picker", len(page.choosers) > n, f"choosers {len(page.choosers) - n}")
        # the notice's button opens it again, and what is picked there is taken
        m = len(page.choosers)
        await page.gesture("[...document.querySelectorAll('#notices .notice[data-key=disc2002] button')].find(b => b.textContent === 'Choose the disc').click()")
        for _ in range(20):
            if len(page.choosers) > m:
                break
            await asyncio.sleep(0.1)
        if len(page.choosers) > m:
            await page.call("DOM.setFileInputFiles", backendNodeId=page.choosers[-1]["backendNodeId"], files=[str(Path(a.iso_2002).resolve())])
        line = await page.until(f"{what}.className.includes('ok') && {what}.textContent", 15)
        check("the notice's button opens it again and takes the disc", len(page.choosers) > m and bool(line) and "SLUS-20470" in line, str(line))
        await page.call("Page.setInterceptFileChooserDialog", enabled=False)
        facts = await page.js("eqoaReport.facts().join('\\n')") or ""
        check("a report names the 2002 disc", "2002 disc: EverQuest Online Adventures (USA), SLUS-20470" in facts, facts[-200:])

    # ---- remember: the disc images kept in this browser (main.js
    # `remembering`), with a file of the origin's own storage (OPFS) standing
    # in for a picked image: its handle is kept as a picked one is
    await page.open(base + q)
    check("the Remember box is offered, unticked", await page.js("!document.getElementById('remember-row').hidden && !document.getElementById('remember').checked") is True)
    keep_js = """(async () => {
      const root = await navigator.storage.getDirectory();
      const fh = await root.getFileHandle('remembered.iso', { create: true });
      const w = await fh.createWritable(); await w.write(new Uint8Array(4096)); await w.close();
      localStorage.setItem('eqoa.rememberDiscs', '1');
      await new Promise((ok) => { const o = indexedDB.open('eqoa-discs', 1);
        o.onupgradeneeded = () => o.result.createObjectStore('handles');
        o.onsuccess = () => { const t = o.result.transaction('handles', 'readwrite'); t.objectStore('handles').put({ handle: fh, name: 'remembered.iso' }, 'disc'); t.oncomplete = ok; }; });
      return true; })()"""
    kept_js = """new Promise((ok) => { const o = indexedDB.open('eqoa-discs', 1);
      o.onupgradeneeded = () => o.result.createObjectStore('handles');
      o.onsuccess = () => { const q = o.result.transaction('handles').objectStore('handles').get('disc'); q.onsuccess = () => ok(!!q.result); q.onerror = () => ok(false); }; })"""
    picked = "(document.getElementById('iso').files[0] || {}).name || ''"
    await page.js(keep_js)
    await page.open(base + q)
    name = await page.until(f"{picked} === 'remembered.iso' && document.getElementById('iso').dataset.again === '1' && document.getElementById('iso-what').textContent && {picked}", 10)
    check("a kept disc is back in its input next visit, and named", name == "remembered.iso", str(await page.js(picked)))
    # the control: unticked, it is forgotten and the next visit has none
    await page.gesture("document.getElementById('remember').click()")
    await asyncio.sleep(0.3)
    gone = await page.js(kept_js) is False and await page.js("localStorage.getItem('eqoa.rememberDiscs')") is None
    await page.open(base + q)
    await asyncio.sleep(1.0)
    check("unticked, the disc is forgotten (control)", gone and await page.js(picked) == "", str(await page.js(picked)))
    # moved or deleted since: said, and forgotten
    await page.js(keep_js)
    await page.js("navigator.storage.getDirectory().then((r) => r.removeEntry('remembered.iso')).then(() => true)")
    await page.open(base + q)
    line = await page.until("document.getElementById('iso-what').textContent.includes('no longer where it was') && document.getElementById('iso-what').textContent", 10)
    check("a kept disc moved since is said so and forgotten", bool(line) and "remembered.iso" in line and await page.js(kept_js) is False, str(line))
    # not allowed without asking: offered on a button, taken on its click
    await page.js(keep_js)
    hook = await page.call("Page.addScriptToEvaluateOnNewDocument", source="""
      FileSystemHandle.prototype.queryPermission = async function () { return 'prompt'; };
      FileSystemHandle.prototype.requestPermission = async function () { return navigator.userActivation.isActive ? 'granted' : 'denied'; };""")
    await page.open(base + q)
    offer = await page.until("!document.getElementById('iso-again-row').hidden && document.getElementById('iso-again').textContent", 10)
    check("a disc the browser must ask for is offered, not taken", offer == "Use remembered.iso again" and await page.js(picked) == "", str(offer))
    await page.gesture("document.getElementById('iso-again').click()")
    name = await page.until(f"{picked} === 'remembered.iso' && document.getElementById('iso-again-row').hidden && {picked}", 10)
    check("its button takes it", name == "remembered.iso", str(await page.js(picked)))
    await page.call("Page.removeScriptToEvaluateOnNewDocument", identifier=hook["identifier"])
    # ticked, a click opens the picker that can be remembered, not the input's
    await page.call("Page.setInterceptFileChooserDialog", enabled=True)
    n = len(page.choosers)
    await page.gesture("document.getElementById('iso').click()")
    await asyncio.sleep(0.8)
    own = [c for c in page.choosers[n:] if c.get("backendNodeId")]
    check("ticked, a click does not open the input's own chooser", not own, str(page.choosers[n:]))
    await page.gesture("document.getElementById('remember').click()")
    await asyncio.sleep(0.3)
    n = len(page.choosers)
    await page.gesture("document.getElementById('iso').click()")
    for _ in range(20):
        if len(page.choosers) > n:
            break
        await asyncio.sleep(0.1)
    check("unticked, the input's own chooser opens (control)", any(c.get("backendNodeId") for c in page.choosers[n:]), str(page.choosers[n:]))
    await page.call("Page.setInterceptFileChooserDialog", enabled=False)
    await page.js("navigator.storage.getDirectory().then((r) => r.removeEntry('remembered.iso')).catch(() => 0).then(() => true)")

    # ---- remember, F3 20261026-014934 ("did not actually remember"). The
    # File System Access picker can't be driven headless: a stand-in for
    # window.showOpenFilePicker hands back a file of the origin's own storage
    # and, as the real one, refuses without an activation; `fail` makes it
    # refuse a start in a kept image's folder ('start') or every call ('all').
    stub_js = """(async () => {
      const root = await navigator.storage.getDirectory();
      for (const n of ['remembered.iso', 'picked-again.iso']) {
        const fh = await root.getFileHandle(n, { create: true });
        const w = await fh.createWritable(); await w.write(new Uint8Array(4096)); await w.close();
      }
      window.__picks = [];
      window.__pickFail = '';
      window.__pickName = 'remembered.iso';
      window.showOpenFilePicker = async (opts) => {
        window.__picks.push({ startIn: !!(opts && opts.startIn), id: opts && opts.id });
        if (!navigator.userActivation.isActive) throw new DOMException('Must be handling a user gesture to show a file picker.', 'SecurityError');
        if (window.__pickFail === 'all' || (window.__pickFail === 'start' && opts.startIn)) throw new DOMException('the picker could not open there', 'NotFoundError');
        return [await (await navigator.storage.getDirectory()).getFileHandle(window.__pickName)];
      };
      return true; })()"""
    facts_js = "eqoaReport.facts().join('\\n')"
    keep_row = "!document.getElementById('iso-again-row').hidden && document.getElementById('iso-again').textContent"
    # a disc picked the browser's own way, then the box ticked: without an
    # activation the picker can't open, so a button that opens it is offered
    await page.open(base + q)
    await page.js(stub_js)
    await page.pick(a.iso)
    await page.until("document.getElementById('iso').files.length > 0", 10)
    await page.js("document.getElementById('remember').click()")
    offer = await page.until(keep_row, 5)
    check("ticked after a pick with no activation, a button to remember it is offered (control)",
          bool(offer) and offer.startswith("Remember ") and await page.js("window.__picks.length") == 0, str(offer))
    await page.js("document.getElementById('remember').click()")
    check("unticked, that button goes", await page.js("document.getElementById('iso-again-row').hidden") is True)
    # ticked after the pick in the tick's own activation: the picker that can
    # keep it opens at once, and the disc is kept
    await page.gesture("document.getElementById('remember').click()")
    name = await page.until(f"({picked}) === 'remembered.iso' && ({picked})", 10)
    check("ticked after a pick, the picker opens at once and the disc is kept",
          name == "remembered.iso" and await page.js("window.__picks.length") == 1 and await page.js(kept_js) is True,
          f"{await page.js(picked)} picks {await page.js('window.__picks.length')}")
    # next visit, the browser must ask: Start asks it in its own click and
    # starts with the kept disc (the form's submit stood in for, to stop short
    # of booting a stand-in file)
    hook = await page.call("Page.addScriptToEvaluateOnNewDocument", source="""
      FileSystemHandle.prototype.queryPermission = async function () { return 'prompt'; };
      FileSystemHandle.prototype.requestPermission = async function () { return navigator.userActivation.isActive ? 'granted' : 'denied'; };""")
    await page.open(base + q)
    offer = await page.until("!document.getElementById('iso-again-row').hidden && document.getElementById('iso-what').textContent", 10)
    check("a kept disc the browser must ask for is said on the disc's line", bool(offer) and "press Start" in offer, str(offer))
    submit_js = """document.getElementById('gate').addEventListener('submit', (e) => {
      window.__submitted = (document.getElementById('iso').files[0] || {}).name || 'none';
      e.preventDefault(); e.stopImmediatePropagation(); }, true); true"""
    await page.js(submit_js)
    await page.js("document.getElementById('go').click()")
    await asyncio.sleep(0.5)
    check("Start without an activation asks nothing and starts nothing (control)",
          await page.js("window.__submitted === undefined") is True and await page.js(picked) == "", str(await page.js("window.__submitted")))
    await page.gesture("document.getElementById('go').click()")
    went = await page.until("window.__submitted", 10)
    check("Start asks the browser for the kept disc in its click and starts with it", went == "remembered.iso", str(went))
    facts = await page.js(facts_js) or ""
    check("a report says what is remembered and what the browser said",
          'remember: on; disc kept "remembered.iso" (checked: prompt, asked: granted)' in facts, facts[-300:])
    await page.call("Page.removeScriptToEvaluateOnNewDocument", identifier=hook["identifier"])
    # a picker that can't start in the kept image's folder opens without it
    await page.open(base + q)
    await page.until(f"({picked}) === 'remembered.iso'", 10)
    await page.js(stub_js)
    await page.js("window.__pickFail = 'all'; true")
    await page.gesture("document.getElementById('iso').click()")
    line = await page.until("document.getElementById('iso-what').textContent.includes('did not open') && document.getElementById('iso-what').textContent", 5)
    check("a picker that opens nowhere says so, and nothing is picked (control)", bool(line) and await page.js(picked) == "remembered.iso", str(line))
    await page.open(base + q)
    await page.until(f"({picked}) === 'remembered.iso'", 10)
    await page.js(stub_js)
    await page.js("window.__pickFail = 'start'; window.__pickName = 'picked-again.iso'; true")
    await page.gesture("document.getElementById('iso').click()")
    name = await page.until(f"({picked}) === 'picked-again.iso' && ({picked})", 10)
    starts = await page.js("JSON.stringify(window.__picks.map((p) => p.startIn))")
    facts = await page.js(facts_js) or ""
    check("a start where the kept image was that fails is tried once more without it",
          name == "picked-again.iso" and starts == "[true,false]" and "starting where it was (opened without it)" in facts, f"{name} {starts} {facts[-160:]}")
    await page.js("document.getElementById('remember').click()")
    facts = await page.js(facts_js) or ""
    check("unticked, a report says so (control)", "remember: off; disc not kept" in facts, facts[-200:])
    await page.js("navigator.storage.getDirectory().then(async (r) => { for (const n of ['remembered.iso', 'picked-again.iso']) await r.removeEntry(n).catch(() => 0); return true; })")

    # ---- spellfx: the community page, its server a stand-in (Site.pick)
    fx = base + "spellfx.html"
    ids = "JSON.stringify([...document.querySelectorAll('#spells li[data-id]')].map((l) => +l.dataset.id))"
    async def open_fx():
        await page.call("Page.navigate", url=fx)
        return await page.until("window.eqoaSpellfx && eqoaSpellfx.spells.length === 4 && document.querySelectorAll('#spells li[data-id]').length", 15)
    check("the community page lists the spells still on a stand-in", await open_fx() == 3 and await page.js(ids) == "[2,3,4]",
          str(await page.js(ids)))
    check("a spell whose visual is set is not listed, though the page has it (control)",
          await page.js("eqoaSpellfx.byId.has(1) && !document.querySelector('#spells li[data-id=\"1\"]')") is True)
    await page.js("(() => { const c = document.getElementById('class'); c.value = '9'; c.dispatchEvent(new Event('change')); return true; })()")
    check("a class shows its own spells alone", await page.js(ids) == "[2,3]", str(await page.js(ids)))
    await page.js("(() => { const c = document.getElementById('class'); c.value = ''; c.dispatchEvent(new Event('change')); return true; })()")
    check("every class shows them all (control)", await page.js(ids) == "[2,3,4]", str(await page.js(ids)))
    # tick every spell the filters show, then untick them (the user's "select all")
    ticked = "document.querySelectorAll('#spells input.pickbox:checked').length"
    await page.js("document.getElementById('tick-shown').click(); true")
    check("Tick all ticks every spell shown", await page.js(ticked) == 3 and "3 spells" in (await page.js("document.getElementById('pick-spell').textContent") or ""),
          str(await page.js("document.getElementById('pick-spell').textContent")))
    await page.js("document.getElementById('tick-shown').click(); true")
    check("pressed again it unticks them (control)", await page.js(ticked) == 0)
    await page.js("document.getElementById('spell-search').value = 'tap'; document.getElementById('spell-search').dispatchEvent(new Event('input')); true")
    check("a search finds a spell by its name", await page.until(f"{ids} === '[4]'", 3) is True)
    check("Tick all counts only the spells shown", "Tick all 1" in (await page.js("document.getElementById('tick-shown').textContent") or ""))
    tagged = await page.js("document.querySelector('#spells li[data-id=\"4\"] .tag.got') && document.querySelector('#spells li[data-id=\"4\"] .tag.got').textContent")
    check("a master class's spell says so", tagged == "master class · Lich", str(tagged))
    await page.js("document.getElementById('spell-search').value = 'lich'; document.getElementById('spell-search').dispatchEvent(new Event('input')); true")
    check("a search finds it by its master class", await page.until(f"{ids} === '[4]'", 3) is True)
    check("a spell got by level carries no such tag (control)", await page.js("!document.querySelector('.tag.got') || document.querySelectorAll('.tag.got').length === 1") is True)
    await page.js("document.getElementById('spell-search').value = ''; document.getElementById('spell-search').dispatchEvent(new Event('input')); true")
    await page.until(f"{ids} === '[2,3,4]'", 3)
    sendable = "!document.getElementById('send').disabled"
    check("no pick, nothing to send (control)", await page.js(sendable) is False)
    await page.js("document.querySelector('#spells li[data-id=\"4\"] button').click(); true")
    check("a spell alone is no pick yet (control)", await page.js(sendable) is False and "Test Tap" in (await page.js("document.getElementById('pick-spell').textContent") or ""))
    # the visual list as the preview sends it when it has read the disc
    await page.until("(() => { const w = document.getElementById('preview').contentWindow; return w.location.pathname.endsWith('fxview.html') && w.document.readyState === 'complete'; })()", 10)
    # what the page asks the preview to play, as the preview's frame gets it
    await page.js("(() => { const w = document.getElementById('preview').contentWindow; w.__asked = []; w.addEventListener('message', (e) => { if (e.data && e.data.type === 'fxview-play') w.__asked.push(e.data.effect + ' ' + e.data.aim); }); return true; })()")
    asked = "document.getElementById('preview').contentWindow.__asked.join()"
    await page.js("document.getElementById('preview').contentWindow.eval(\"parent.postMessage({source: 'eqoa-fxview', type: 'started', visuals: ['0x11111111', '0x55555555']}, location.origin)\")")
    listed = await page.until("document.querySelectorAll('#visuals li[data-visual]').length === 2 && [...document.querySelectorAll('#visuals li[data-visual]')].map((l) => l.dataset.visual).join()", 5)
    check("the visuals the preview read are listed", listed == "0x11111111,0x55555555", str(listed))
    given = await page.js("document.querySelector('#visuals li[data-visual=\"0x11111111\"]').textContent")
    check("a visual says which spell has it", "Test Bolt" in (given or ""), str(given))
    # what a cast lands on: by the spell a harmful one at a second human, never
    # the dummy (the user: its hitbox makes the visual smaller), the caster and a
    # second human at once when asked, and that kept
    played = await page.until(asked, 5)
    check("a harmful spell lands on a second human by itself", played == "0x44444444 human", str(played))
    offered = await page.js("[...document.getElementById('aim').options].map((o) => o.value).join()")
    check("the training dummy is not offered", offered == "auto,both,human,caster", str(offered))
    await page.js("document.querySelector('#spells li[data-id=\"3\"] button').click(); true")
    played = await page.until(f"{asked}.endsWith('0x33333333 caster') && {asked}", 5)
    check("a beneficial one at the caster (control)", bool(played), str(await page.js(asked)))
    await page.js("(() => { const a = document.getElementById('aim'); a.value = 'both'; a.dispatchEvent(new Event('change')); return true; })()")
    played = await page.until(f"{asked}.endsWith('0x33333333 both') && {asked}", 5)
    check("the caster and a second human at once, kept", bool(played) and await page.js("localStorage.getItem('eqoa.spellfxAim')") == "both", str(await page.js(asked)))
    await page.js("document.querySelector('#spells li[data-id=\"4\"] button').click(); true")
    # W and S: the visual before and after, wherever the page's keys are, the preview's too
    visual = "eqoaSpellfx.visual"
    await page.js("document.activeElement && document.activeElement.blur(); true")
    await page.key("s", "KeyS", 83)
    check("S steps to the next visual", await page.until(f"{visual} === '0x11111111'", 3) is True, str(await page.js(visual)))
    await page.js("document.getElementById('spell-search').focus(); true")
    # a key that types: its text with it, as a keyboard's
    await page.call("Input.dispatchKeyEvent", type="keyDown", key="s", code="KeyS", windowsVirtualKeyCode=83, text="s")
    await page.call("Input.dispatchKeyEvent", type="keyUp", key="s", code="KeyS", windowsVirtualKeyCode=83)
    await asyncio.sleep(0.3)
    typed = await page.js(f"{visual} + ' ' + document.getElementById('spell-search').value")
    check("not while a box takes typing (control)", typed == "0x11111111 s", str(typed))
    await page.js("(() => { const s = document.getElementById('spell-search'); s.value = ''; s.dispatchEvent(new Event('input')); s.blur(); return true; })()")
    await page.until(f"{ids} === '[2,3,4]'", 3)
    await page.js("(() => { const f = document.getElementById('preview'); f.focus(); f.contentDocument.getElementById('eqoa').focus(); return true; })()")
    await page.key("w", "KeyW", 87)
    check("W in the preview steps back", await page.until(f"{visual} === '0x55555555'", 3) is True, str(await page.js(visual)))
    await page.js("document.querySelector('#visuals li[data-visual=\"0x55555555\"] button').click(); true")
    check("a spell and a visual make a pick", await page.js(sendable) is True and "Test Tap" in (await page.js("document.getElementById('send').textContent") or ""))
    await page.js("document.getElementById('who').value = 'Tester'; document.getElementById('note').value = 'it rang'; document.getElementById('send').click(); true")
    thanked = await page.until("document.getElementById('sent').textContent.startsWith('Thanks') && document.getElementById('sent').textContent", 5)
    check("the pick is sent as the server takes it", bool(thanked) and site.picks[-1:] == [{"spell": 4, "visual": "0x55555555", "name": "Tester", "note": "it rang"}],
          f"{thanked} {site.picks[-1:]}")
    # several spells, one visual (the user: "a check mark next to multiple spells and can select one spellfx")
    await page.js("for (const id of [2, 3]) document.querySelector('#spells li[data-id=\"' + id + '\"] input.pickbox').click(); true")
    told = await page.js("document.getElementById('pick-spell').textContent + ' | ' + document.getElementById('send').textContent")
    check("ticked spells make one pick for them all", "2 spells" in (told or "") and "these 2 spells" in (told or ""), str(told))
    before = len(site.picks)
    await page.js("document.getElementById('send').click(); true")
    sent2 = await page.until("document.getElementById('sent').textContent.startsWith('Thanks') && document.getElementById('sent').textContent.includes('all 2') && document.getElementById('sent').textContent", 5)
    got = site.picks[before:]
    check("one visual is sent for each ticked spell", bool(sent2) and sorted(p["spell"] for p in got) == [2, 3] and len({p["visual"] for p in got}) == 1, f"{sent2} {got}")
    check("the ticks are cleared after the send", await page.js("document.querySelectorAll('#spells input.pickbox:checked').length") == 0)
    check("no ticks: the pick is the spell picked again (control)", "Test Tap" in (await page.js("document.getElementById('send').textContent") or ""))
    await page.js("document.getElementById('who').value = 'flood'; document.getElementById('send').click(); true")
    refused = await page.until("document.getElementById('sent').textContent.startsWith('Not sent') && document.getElementById('sent').textContent", 5)
    check("a refusal is said, not thanked", "Too many picks" in (refused or "") and len(site.picks) == 3, str(refused))
    await open_fx()
    check("the name is kept for the next visit", await page.js("document.getElementById('who').value") == "flood")
    check("so is what a cast lands on", await page.js("document.getElementById('aim').value") == "both")
    await page.js("localStorage.removeItem('eqoa.spellfxName'); localStorage.removeItem('eqoa.spellfxAim'); true")
    await open_fx()
    check("none kept, the field is empty (control)", await page.js("document.getElementById('who').value") == "")
    check("and a cast lands by the spell (control)", await page.js("document.getElementById('aim').value") == "auto")
    # a spell GMs give its visual while the page is open leaves the list at the next refresh
    await page.until(f"{ids} === '[2,3,4]'", 5)
    SPELLFX_SPELLS[1]["standin"] = False  # Test Heal (id 2) has its visual now
    try:
        await page.js("eqoaSpellfx.refresh(); true")
        gone = await page.until(f"{ids} === '[3,4]'", 5)
        check("a spell given its visual meanwhile leaves the open page", gone is True, str(await page.js(ids)))
    finally:
        SPELLFX_SPELLS[1]["standin"] = True
    await page.js("eqoaSpellfx.refresh(); true")
    check("and comes back if it is put back on a stand-in (control)", await page.until(f"{ids} === '[2,3,4]'", 5) is True)
    check("the game's page links it", 'href="./spellfx.html"' in (HERE / "dist" / "index.html").read_text())
    build = json.loads((HERE / "dist" / "version.json").read_text())["build"]
    icons = [p for p in ("index", "fxview", "spellfx") if f'href="./v/{build}/disc.svg"' in (HERE / "dist" / f"{p}.html").read_text()]
    check("every page carries the program's icon, in its build", icons == ["index", "fxview", "spellfx"] and (HERE / "dist" / "v" / build / "disc.svg").is_file(), str(icons))

    # ---- typed: a phone's or a tablet's own keyboard for the game's edit
    # boxes, the client stood in for by eqoaWantText (main.js)
    await page.open(base + q)
    await page.js("window.__keys = []; document.getElementById('eqoa').addEventListener('keydown', (e) => window.__keys.push(e.key)); true")
    touch_up = "document.getElementById('eqoa').dispatchEvent(new PointerEvent('pointerup', {pointerType: 'touch', bubbles: true})); document.activeElement && document.activeElement.id"
    check("no edit box: a touch puts no keyboard up (control)", await page.js(touch_up) != "eqoatyped")
    await page.js("window.eqoaWantText(true); true")
    check("an edit box: a touch puts the focus in the page's input, its keyboard with it", await page.js(touch_up) == "eqoatyped")
    for ch in "Ab 1":
        await page.call("Input.insertText", text=ch)
    await page.key("Backspace", "Backspace", 8)
    await page.key("Enter", "Enter", 13)
    keys = await page.js("JSON.stringify(window.__keys)")
    check("what is typed there comes to the canvas as its keys", json.loads(keys or "null") == ["A", "b", " ", "1", "Backspace", "Enter"], str(keys))
    check("and none of it stays in the input", await page.js("document.getElementById('eqoatyped').value.trim()") == "")
    await page.js("window.eqoaWantText(false); true")
    check("the box gone, the focus is the canvas's again", await page.js("document.activeElement && document.activeElement.id") == "eqoa")
    check("and a touch puts no keyboard up any more", await page.js(touch_up) != "eqoatyped")

    # ---- fullscreen: Start puts the page full screen in its own click on a
    # computer whose browser keeps Escape for the page (Keyboard Lock) and on
    # a touch device (CDP's touch emulation: its pointer coarse), where a touch
    # on the game puts it back once it is left, but not while an edit box has
    # the keyboard; not where Escape would end it (no Keyboard Lock: a
    # stand-in), not with another window of the game open (multiboxing), not
    # on an iPhone. F11 stays the browser's: Chromium takes it before the page
    # in any full screen (checked headed, through X: README "In a browser").
    fs = "!!document.fullscreenElement"
    fsfact = "(eqoaReport.facts().find((l) => l.startsWith('full screen:')) || '')"
    # the browser's own install offer kept from the page, so the card shows
    # what these checks put there (a stand-in offer is not trusted)
    quiet = await page.call("Page.addScriptToEvaluateOnNewDocument", source="addEventListener('beforeinstallprompt', (e) => { if (e.isTrusted) e.stopImmediatePropagation(); }, true);")

    async def start():
        """Start pressed in a click's activation, the disc picked: whether the
        page went full screen."""
        await page.open(base + q)
        await page.pick(a.iso)
        await page.gesture("document.getElementById('go').click(); true")
        return bool(await page.until(fs, 3))

    async def tap():
        await page.call("Input.dispatchTouchEvent", type="touchStart", touchPoints=[{"x": 300, "y": 300, "id": 1}])
        await page.call("Input.dispatchTouchEvent", type="touchEnd", touchPoints=[])

    async def leave():
        await page.js("document.exitFullscreen().then(() => true, () => true)")
        return bool(await page.until(f"!{fs}", 3))

    check("a computer whose browser keeps Escape for the page: Start goes full screen", await start())
    said = await page.until(f"{fsfact}.includes('Escape the game')", 3)
    check("Escape locked to the game there", bool(said), str(await page.js(fsfact)))
    nolock = await page.call("Page.addScriptToEvaluateOnNewDocument", source="Object.defineProperty(Navigator.prototype, 'keyboard', { configurable: true, get: () => undefined });")
    try:
        check("one whose browser can't (Firefox, Safari: a stand-in): no full screen (control)", not await start())
    finally:
        await page.call("Page.removeScriptToEvaluateOnNewDocument", identifier=nolock["identifier"])
    await page.open(base + q)
    other = devtools("new?" + base, "PUT")
    try:
        devtools(f"activate/{a.target_id}")
        both = await page.until("eqoaWindows.live.length === 2 && document.visibilityState === 'visible'", 5)
        await page.pick(a.iso)
        await page.gesture("document.getElementById('go').click(); true")
        went = await page.until(fs, 2)
        check("another window of the game open (multiboxing): no full screen", bool(both) and not went, f"{both} {await page.js(fsfact)}")
    finally:
        devtools(f"close/{other['id']}")
        devtools(f"activate/{a.target_id}")
    await page.call("Emulation.setUserAgentOverride", userAgent="Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1")
    try:
        check("an iPhone: none (iOS lets no page there)", not await start())
        what = await page.js("document.getElementById('install').hidden ? '' : document.getElementById('install-what').textContent")
        check("its card says how: the Home Screen app", "Add to Home Screen" in (what or ""), str(what))
    finally:
        await page.call("Emulation.setUserAgentOverride", userAgent=ua)

    await page.call("Emulation.setTouchEmulationEnabled", enabled=True, maxTouchPoints=5)
    try:
        check("a touch device: Start goes full screen", await start())
        playing = await page.until("document.body.classList.contains('playing')", 60)
        left = await leave()
        await tap()
        check("left, a touch on the game puts it back", bool(playing) and left and bool(await page.until(fs, 3)), f"playing {playing}, left {left}")
        await page.js("eqoaWantText(true); true")
        await leave()
        await tap()
        await asyncio.sleep(0.5)
        check("not while an edit box has the keyboard (control): the touch puts it up", not await page.js(fs) and await page.js("document.activeElement && document.activeElement.id") == "eqoatyped")
        await page.js("eqoaWantText(false); true")
    finally:
        await page.call("Emulation.setTouchEmulationEnabled", enabled=False)
    check("a computer: Start goes full screen", await start())
    playing = await page.until("document.body.classList.contains('playing')", 60)
    await leave()
    await page.call("Input.dispatchMouseEvent", type="mousePressed", x=300, y=300, button="left", clickCount=1)
    await page.call("Input.dispatchMouseEvent", type="mouseReleased", x=300, y=300, button="left", clickCount=1)
    await asyncio.sleep(0.5)
    check("left, it stays left at a click on the game (control: a computer's is the player's)", bool(playing) and not await page.js(fs))

    # ---- install: the build is an app a browser can install (its manifest,
    # the drawn disc's icons); the card's button asks a Chromium-based
    # browser's own prompt (a stand-in offer: the real one comes when the
    # browser's rules say), elsewhere the card says which menu has it, and
    # started as the app it says nothing
    await page.open(base + q)
    man = await page.call("Page.getAppManifest") or {}
    parsed = json.loads(man.get("data") or "{}")
    check("the page names its build's manifest, which parses", re.search(r"/v/[0-9a-f]{12}/manifest\.json$", man.get("url", "")) is not None and not man.get("errors") and parsed.get("display") == "fullscreen",
          json.dumps(man.get("errors")) + " " + man.get("url", ""))
    installable = ((await page.call("Page.getInstallabilityErrors")) or {}).get("installabilityErrors")
    check("Chromium can install it: no installability error", installable == [], json.dumps(installable))
    await page.call("Page.navigate", url=base + "spellfx.html")
    await page.until("document.readyState === 'complete'", 10)
    bare = ((await page.call("Page.getInstallabilityErrors")) or {}).get("installabilityErrors")
    check("a page with no manifest can't be (control)", bool(bare), json.dumps(bare))
    build = json.loads((HERE / "dist" / "version.json").read_text())["build"]
    made = [i for i in ("icon-192.png", "icon-512.png", "icon-maskable-512.png", "apple-touch-icon.png") if (HERE / "dist" / "v" / build / i).read_bytes()[:8] == b"\x89PNG\r\n\x1a\n"]
    check("its icons are in the build, PNGs", len(made) == 4 and f'href="./v/{build}/apple-touch-icon.png"' in (HERE / "dist" / "index.html").read_text(), str(made))

    card = "JSON.stringify([!document.getElementById('install').hidden, !document.getElementById('install-go').hidden, document.getElementById('install-what').textContent])"
    offer = """(() => { const e = new Event('beforeinstallprompt');
      e.prompt = () => { window.__prompted = (window.__prompted || 0) + 1; return Promise.resolve(); };
      e.userChoice = Promise.resolve({ outcome: %s }); dispatchEvent(e); return true; })()"""
    appfact = "(eqoaReport.facts().find((l) => l.startsWith('app:')) || '')"
    await page.open(base + q)
    shown = json.loads(await page.js(card))
    check("a computer's browser that has not offered it: nothing on the card (control)", shown[:2] == [False, False], str(shown))
    await page.js(offer % "'accepted'")
    shown = json.loads(await page.js(card))
    check("one that offers it: the button", shown[:2] == [True, True] and "full screen" in shown[2], str(shown))
    await page.gesture("document.getElementById('install-go').click(); true")
    said = await page.until(f"{card}.includes('Installed:')", 3)
    check("pressed, the browser's own prompt; accepted, the card says so and the button goes", await page.js("window.__prompted") == 1 and bool(said) and not json.loads(await page.js(card))[1], str(await page.js(card)))
    check("and a report says so", await page.js(appfact) == "app: accepted", str(await page.js(appfact)))
    await page.open(base + q)
    await page.js(offer % "'dismissed'")
    await page.gesture("document.getElementById('install-go').click(); true")
    await asyncio.sleep(0.3)
    shown = json.loads(await page.js(card))
    check("dismissed, the button goes (on a computer the card says nothing more)", shown[:2] == [False, False], str(shown))
    await page.call("Emulation.setTouchEmulationEnabled", enabled=True, maxTouchPoints=5)
    try:
        await page.open(base + q)
        shown = json.loads(await page.js(card))
        check("a phone's browser that has not offered it: its menu", shown[:2] == [True, False] and "Install app or Add to Home screen" in shown[2], str(shown))
        await page.call("Emulation.setUserAgentOverride", userAgent="Mozilla/5.0 (Android 14; Mobile; rv:131.0) Gecko/131.0 Firefox/131.0")
        await page.open(base + q)
        shown = json.loads(await page.js(card))
        check("Firefox on a phone: Firefox's menu", shown[:2] == [True, False] and "Firefox" in shown[2], str(shown))
    finally:
        await page.call("Emulation.setUserAgentOverride", userAgent=ua)
        await page.call("Emulation.setTouchEmulationEnabled", enabled=False)
    standalone = await page.call("Page.addScriptToEvaluateOnNewDocument", source="Object.defineProperty(Navigator.prototype, 'standalone', { configurable: true, get: () => true });")
    try:
        await page.open(base + q)
        await page.js(offer % "'accepted'")
        shown = json.loads(await page.js(card))
        check("started as the installed app: nothing on the card, offered or not", not shown[0] and await page.js(appfact) == "app: started as the installed app", f"{shown} {await page.js(appfact)}")
    finally:
        await page.call("Page.removeScriptToEvaluateOnNewDocument", identifier=standalone["identifier"])
    await page.call("Page.removeScriptToEvaluateOnNewDocument", identifier=quiet["identifier"])

    # ---- software: a browser drawing without the graphics card is told so
    # once, on the card's line, in a notice and on the status line in the
    # world (F3 20261027-003944: Windows' Microsoft Basic Render Driver,
    # which the card's check missed). Stand-ins, before the page loads: no
    # WebGPU and a renderer's name from WebGL2; then the client's own
    # graphics line as that player's client sent it.
    warp = "ANGLE (Microsoft, Microsoft Basic Render Driver (0x0000008C) Direct3D11 vs_5_0 ps_5_0, D3D11)"
    card = "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 (0x00002504) Direct3D11 vs_5_0 ps_5_0, D3D11)"
    gfx = "document.getElementById('gfx-state').textContent"
    soft = f"{notices}.includes('software-graphics')"
    told = "(document.querySelector('#notices .notice[data-key=software-graphics] .text') || {}).textContent || ''"
    status = "document.getElementById('status').textContent"

    async def probed(renderer):
        """The page opened on a stand-in renderer: the card's line once its
        check is done."""
        stub = await page.call("Page.addScriptToEvaluateOnNewDocument", source=f"""
            Object.defineProperty(Navigator.prototype, 'gpu', {{ configurable: true, get: () => undefined }});
            const get = WebGL2RenderingContext.prototype.getParameter;
            WebGL2RenderingContext.prototype.getParameter = function (p) {{
              return p === 0x9246 || p === 0x1F01 ? {json.dumps(renderer)} : get.call(this, p);
            }};""")
        try:
            await page.open(base + q)
            return await page.until(f"!{gfx}.startsWith('Checking') && {gfx}", 10) or ""
        finally:
            await page.call("Page.removeScriptToEvaluateOnNewDocument", identifier=stub["identifier"])

    line = await probed(warp)
    check("the page's probe on Basic Render Driver: the card says so", line.startswith("Graphics: WebGL2, drawn without the graphics card"), line)
    text = await page.until(told, 5) or ""
    check("and a notice says what to do", "without the graphics card" in text and "Use graphics acceleration when available" in text and "driver" in text, text)
    help_ = await page.js("document.getElementById('gfx-help').textContent") or ""
    check("the card's steps are acceleration's", "chrome://settings/system" in help_ and "chrome://flags" not in help_, help_)
    line = await probed(card)
    check("on a graphics card: WebGL2's line, no notice (control)", line.startswith("Graphics: WebGL2. It works") and not await page.js(soft), line)
    await page.js(f"document.body.classList.add('playing'); eqoaStatus({json.dumps('graphics: ' + card + ' (Gl)')}); eqoaStatus('booted · 60 fps · 300 MB'); true")
    check("nor from the client's adapter on one (control)", not await page.js(soft) and await page.js(status) == "booted · 60 fps · 300 MB", str(await page.js(status)))
    await page.js(f"eqoaStatus({json.dumps('graphics: ' + warp + ' (Gl)')}); eqoaStatus('booted · 27 fps · 300 MB'); true")
    check("the client's adapter on Basic Render Driver: the notice", bool(await page.until(soft, 5)))
    check("the card's line (the report's graphics fact) says so", (await page.js(gfx) or "").startswith("Graphics: WebGL2, drawn without the graphics card"), str(await page.js(gfx)))
    check("and the status line in the world", await page.js(status) == "booted · 27 fps · 300 MB · drawn without the graphics card", str(await page.js(status)))
    await page.js("[...document.querySelectorAll('#notices .notice[data-key=software-graphics] button')].find(b => b.textContent === 'Dismiss').click()")
    await page.js(f"eqoaStatus({json.dumps('graphics: ' + warp + ' (Gl)')}); eqoaStatus('booted · 27 fps · 300 MB'); true")
    await asyncio.sleep(0.3)
    check("dismissed, it is not raised again", not await page.js(soft) and "drawn without" in (await page.js(status) or ""))
    await page.js("document.body.classList.remove('playing'); true")

    errors = [l for l in page.lines if l.startswith("EXCEPTION") and "crash_test" not in l and "unreachable" not in l and "a test rejection" not in l]
    check("no exception in the page beyond the crash test's", not errors, "; ".join(errors[:3]))
    return failed


async def main(a):
    dist = HERE / "dist"
    if not (dist / "version.json").exists():
        sys.exit("no web/dist: run web/build.sh first")
    site = Site(dist)
    if a.keep:
        site.keep = Path(a.keep)
        site.keep.mkdir(parents=True, exist_ok=True)
    httpd = site.serve(a.http_port)
    base = f"http://127.0.0.1:{httpd.server_address[1]}/"
    if a.serve:
        print(f"serving {site.root} at {base} (reports at {base}reports, kept in memory)", flush=True)
        while True:
            await asyncio.sleep(3600)
    prof = Path(a.profile).expanduser()
    shutil.rmtree(prof, ignore_errors=True)
    prof.mkdir(parents=True)
    tmp = prof.parent / "tmp"
    tmp.mkdir(exist_ok=True)
    flags = [shutil.which("chromium") or "chromium", "--headless=new", f"--user-data-dir={prof}", f"--remote-debugging-port={a.port}",
             "--no-first-run", "--no-default-browser-check", "--window-size=1280,800", "--use-angle=vulkan", "--enable-features=Vulkan",
             "--ignore-gpu-blocklist", "--enable-gpu", "about:blank"]
    env = dict(os.environ, TMPDIR=str(tmp), DBUS_SESSION_BUS_ADDRESS="disabled:")
    proc = subprocess.Popen(flags, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(100):
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{a.port}/json/version", timeout=1)
                break
            except OSError:
                await asyncio.sleep(0.1)
        req = urllib.request.Request(f"http://127.0.0.1:{a.port}/json/new?about:blank", method="PUT")
        target = json.load(urllib.request.urlopen(req))
        a.target_id = target["id"]
        async with websockets.connect(target["webSocketDebuggerUrl"], max_size=1 << 26) as ws:
            page = Page(ws)
            rt = asyncio.ensure_future(page.reader())
            await page.call("Runtime.enable")
            await page.call("Page.enable")
            failed = await tests(a, site, base, page)
            rt.cancel()
    finally:
        proc.terminate()
        try:
            proc.wait(5)
        except subprocess.TimeoutExpired:
            proc.kill()
        httpd.shutdown()
        shutil.rmtree(site.root, ignore_errors=True)
    print(f"{'all passed' if not failed else f'{len(failed)} failed'}")
    return 1 if failed else 0


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--iso", default=str(HERE.parent / "run" / "eqoa.iso"))
    ap.add_argument("--other-iso", help="a disc image other than Frontiers, for the disc check's other half")
    ap.add_argument("--iso-2002", help="the EverQuest Online Adventures (USA) disc image, for the 2002 checks")
    ap.add_argument("--serve", action="store_true", help="only serve the site and the stand-in report server")
    ap.add_argument("--keep", help="a directory for what the stand-in report server is sent, a JSON file each")
    ap.add_argument("--http-port", type=int, default=0)
    ap.add_argument("--port", type=int, default=9335)
    ap.add_argument("--profile", default="~/.cache/eqoa-rs-web/pagetest/profile")
    sys.exit(asyncio.run(main(ap.parse_args())))
