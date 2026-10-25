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

Each check has its negative control. Needs the `websockets` package.

    web/build.sh && web/pagetest.py --iso run/eqoa.iso
    web/pagetest.py --serve --http-port 8794   # the site alone, for drive.py --base
"""
import argparse, asyncio, functools, http.server, json, os, shutil, socketserver, subprocess, sys, tempfile, threading, time, urllib.request
from pathlib import Path

import websockets

HERE = Path(__file__).resolve().parent


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

            def do_POST(self):
                route = self.path.split("?")[0]
                body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
                code, answer = 404, {"ok": False, "error": "no such route"}
                if route in ("/reports/report", "/reports/crash"):
                    code, answer = site.take(route.rsplit("/", 1)[1], body)
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
    ticks = "window.eqoaHiddenTicks"
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
