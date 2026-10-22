#!/usr/bin/env python3
"""pagetest -- the page's own behaviour, in a headless Chromium: updates,
its notices and what the page catches.

Serves a copy of web/dist (build.sh's output), then plays deploys against an
open page by writing new builds and version.json under it:

  update    an untouched page reloads into a new build at once; one with a
            disc picked is told, and Later holds until a newer build; Reload
            brings the newest; a reload that still brings the old page is a
            problem, not a second reload
  crash     a panic says the client stopped, with a Reload; F3 is no
            report dialog's, and nothing is posted anywhere
  missing   a page whose build is gone says so in its status line
  disc      the disc is named when picked (with a second image in --other-iso)

Each check has its negative control. Needs the `websockets` package.

    web/build.sh && web/pagetest.py --iso run/eqoa.iso
    web/pagetest.py --serve --http-port 8794   # the site alone, for drive.py --base
"""
import argparse, asyncio, functools, http.server, json, os, shutil, socketserver, subprocess, sys, tempfile, threading, time, urllib.request
from pathlib import Path

import websockets

HERE = Path(__file__).resolve().parent


class Site:
    """A copy of web/dist to play deploys on, and what the page posted (it
    should post nothing)."""

    def __init__(self, dist):
        tmp = Path("~/.cache/eqoa-rs-web").expanduser()
        tmp.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix="site-", dir=tmp))
        shutil.copytree(dist, self.root, dirs_exist_ok=True)
        self.first = json.loads((self.root / "version.json").read_text())["build"]
        self.page = (dist / "index.html").read_text()
        self.posts = []

    def deploy(self, build, page=True, version=True):
        """A build named `build` (a copy of the first's files): its directory,
        then the page naming it, then version.json, as deploy.sh goes."""
        d = self.root / "v" / build
        if not d.exists():
            shutil.copytree(self.root / "v" / self.first, d)
            (d / "build.json").write_text(json.dumps({"build": build, "commit": "test"}))
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
                # the page posts nothing: anything that comes is counted
                site.posts.append((self.path, self.rfile.read(int(self.headers.get("Content-Length", 0)))))
                data = json.dumps({"ok": False, "error": "no such route"}).encode()
                self.send_response(404)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

        socketserver.ThreadingTCPServer.allow_reuse_address = True
        httpd = socketserver.ThreadingTCPServer(("127.0.0.1", port), functools.partial(Handler, directory=str(self.root)))
        httpd.daemon_threads = True
        threading.Thread(target=httpd.serve_forever, daemon=True).start()
        return httpd


class Page:
    """One Chromium tab over the DevTools protocol."""

    def __init__(self, ws):
        self.ws, self.seq, self.pending, self.lines = ws, 0, {}, []

    async def reader(self):
        async for raw in self.ws:
            m = json.loads(raw)
            if "id" in m and m["id"] in self.pending:
                self.pending.pop(m["id"]).set_result(m.get("result", m.get("error")))
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

    async def pick(self, iso):
        doc = await self.call("DOM.getDocument")
        node = await self.call("DOM.querySelector", nodeId=doc["root"]["nodeId"], selector="#iso")
        await self.call("DOM.setFileInputFiles", nodeId=node["nodeId"], files=[str(Path(iso).resolve())])


async def tests(a, site, base, page):
    failed = []

    def check(name, ok, detail=""):
        print(f"{'ok  ' if ok else 'FAIL'} {name}{': ' + detail if detail and not ok else ''}", flush=True)
        if not ok:
            failed.append(name)

    notices = "JSON.stringify([...document.querySelectorAll('#notices .notice')].map(n => n.dataset.key))"
    q = ""

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
    await page.js("[...document.querySelectorAll('#notices .notice button')].find(b => b.textContent === 'Later').click()")
    await page.js("eqoaCheckUpdate()")
    check("Later holds for that build", "update" not in await page.js(notices))
    site.deploy("d" * 12)
    await page.js("eqoaCheckUpdate()")
    check("a newer build is told again after Later", bool(await page.until(f"{notices}.includes('update')", 5)))
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

    # ---- crash: the client stopped (a panic), and the page says so
    await page.open(base + q)
    check("no crash notice before a panic (control)", "crash" not in await page.js(notices))
    await page.js("document.getElementById('eqoa').focus()")
    await page.key("F3", "F3", 114)
    await asyncio.sleep(0.5)
    check("F3 opens no report dialog", await page.js("!document.getElementById('report') && !document.querySelector('.modal')") is True)
    await page.js("try { eqoaClient.crashTest() } catch (e) { window.__trap = String(e) }")
    said = await page.until(f"{notices}.includes('crash') && document.querySelector('#notices .notice[data-key=crash] .text').textContent", 5)
    check("a panic says the client stopped", bool(said) and said.startswith("The client stopped") and "crash_test: the client stopped on purpose" in said, str(said))
    reload_button = await page.js("[...document.querySelectorAll('#notices .notice[data-key=crash] button')].map(b => b.textContent).join(',')")
    check("and offers a reload, nothing else", reload_button == "Reload", str(reload_button))
    await page.stay()
    await page.js("[...document.querySelectorAll('#notices .notice[data-key=crash] button')].find(b => b.textContent === 'Reload').click()")
    check("Reload starts the page again", bool(await page.reloaded()) and "crash" not in await page.js(notices))

    # ---- missing: a page naming a build that is not there
    (site.root / "gone.html").write_text(site.page.replace(f"v/{site.first}/", "v/" + "f" * 12 + "/"))
    await page.call("Page.navigate", url=base + "gone.html" + q)
    said = await page.until("document.getElementById('status').textContent.includes('did not load')", 10)
    check("a page whose build is gone says so", bool(said))

    # ---- disc
    await page.open(base + q)
    await page.pick(a.iso)
    line = await page.until("document.getElementById('iso-what').className.includes('ok') && document.getElementById('iso-what').textContent", 10)
    check("the Frontiers disc is named", bool(line) and "SLUS-20744" in line, str(line))
    if a.other_iso:
        await page.pick(a.other_iso)
        line = await page.until("document.getElementById('iso-what').className.includes('warn') && document.getElementById('iso-what').textContent", 10)
        check("another disc is named, and what it lacks", bool(line) and "Frontiers" in line, str(line))

    errors = [l for l in page.lines if l.startswith("EXCEPTION") and "crash_test" not in l and "unreachable" not in l]
    check("no exception in the page beyond the crash test's", not errors, "; ".join(errors[:3]))
    check("the page posted nothing", not site.posts, str(site.posts[:3]))
    return failed


async def main(a):
    dist = HERE / "dist"
    if not (dist / "version.json").exists():
        sys.exit("no web/dist: run web/build.sh first")
    site = Site(dist)
    httpd = site.serve(a.http_port)
    base = f"http://127.0.0.1:{httpd.server_address[1]}/"
    if a.serve:
        print(f"serving {site.root} at {base}", flush=True)
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
    ap.add_argument("--serve", action="store_true", help="only serve the site")
    ap.add_argument("--http-port", type=int, default=0)
    ap.add_argument("--port", type=int, default=9335)
    ap.add_argument("--profile", default="~/.cache/eqoa-rs-web/pagetest/profile")
    sys.exit(asyncio.run(main(ap.parse_args())))
