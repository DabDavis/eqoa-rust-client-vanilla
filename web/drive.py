#!/usr/bin/env python3
"""drive -- run the browser client in a headless Chromium and watch it, for tests.

Serves web/dist on a local port, opens the page in a Chromium of its own over
the DevTools protocol, puts the disc image into the page's file input (the
same as a player picking it), fills the form, presses View or Play, prints the
page's console, and screenshots it.

    web/drive.py --iso run/eqoa.iso --view 'CHAR.ESF 0x70D898C6' --shot 8=human.png
    EQOA_PASSWORD=... web/drive.py --iso run/eqoa.iso --play dab \\
        --params 'relay=ws://127.0.0.1:8790&server=203.0.113.10' --shot 30=world.png

--shot SECONDS=FILE.png screenshots that long after the button (repeatable);
--until TEXT exits 0 once a console line has it; --keys SECONDS=KEYS holds
keys for a while (w, a, s, d, ArrowLeft...: 'w:2000' holds w for 2 s; 'w:down'
and 'w:up' press it and let it go apart, the canvas focused only for the press,
so a --js between them can move the focus: a key let go elsewhere);
--js SECONDS=EXPR evaluates EXPR in the page then (the character screens:
"document.querySelector('#charlist button.primary').click()"), its value
printed. --new-account makes the account first (the page's New account tab).
WebGL/WebGPU run on the machine's GPU (--no-gpu: SwiftShader, every core).
--headed opens a real window on $DISPLAY instead (F11 needs one); --resize
SECONDS=WxH sets the page's viewport then, as a resized window would; --scale
is the device's pixel ratio throughout (Windows' display scaling, 1.25 or 1.5;
a phone's, 2 to 3).
--mobile makes the page a touch-first device (CDP's touch emulation: its
pointer is coarse); --touch SECONDS=STEPS touches the canvas then, steps
';'-separated: 'tap X Y', 'hold X Y MS', 'drag X0 Y0 X1 Y1 MS' (CSS pixels);
--mouse SECONDS=STEPS moves the mouse over the canvas, steps ';'-separated:
'click X Y', 'rclick X Y', 'wheel X Y LINES' (up positive), 'move X Y',
'drag X0 Y0 X1 Y1 [MS]' (the left button held);
--gamepad SECONDS=STEPS plays a gamepad the page sees through
navigator.getGamepads (the standard layout's indices: 0 A, 1 B, 2 X, 3 Y,
4 LB, 5 RB, 6 LT, 7 RT, 8 Back, 9 Start, 10 L3, 11 R3, 12-15 the d-pad):
'tap N [MS]', 'press N', 'release N', 'axes LX LY RX RY'.
Needs the `websockets` package.
"""
import argparse, asyncio, base64, functools, http.server, json, os, shutil, socketserver, subprocess, sys, threading, time, urllib.request
from pathlib import Path

import websockets

HERE = Path(__file__).resolve().parent


def serve(root, port):
    class Quiet(http.server.SimpleHTTPRequestHandler):
        extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, ".wasm": "application/wasm", ".js": "text/javascript"}

        def log_message(self, *a):
            pass

    handler = functools.partial(Quiet, directory=str(root))
    socketserver.ThreadingTCPServer.allow_reuse_address = True
    httpd = socketserver.ThreadingTCPServer(("127.0.0.1", port), handler)
    httpd.daemon_threads = True
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd


async def main(a):
    httpd = serve(HERE / "dist", a.http_port) if not a.base else None
    base = a.base or f"http://127.0.0.1:{httpd.server_address[1]}/"
    prof = Path(a.profile).expanduser()
    prof.mkdir(parents=True, exist_ok=True)
    tmp = prof.parent / "tmp"
    tmp.mkdir(exist_ok=True)
    # sound starts without a gesture: the page's clicks here are not the user's
    flags = [shutil.which("chromium") or "chromium", *([] if a.headed else ["--headless=new"]), f"--user-data-dir={prof}",
             f"--remote-debugging-port={a.port}", "--no-first-run", "--no-default-browser-check",
             f"--window-size={a.size}", "--enable-unsafe-swiftshader", "--autoplay-policy=no-user-gesture-required"]
    if a.gpu:
        flags += ["--use-angle=vulkan", "--enable-features=Vulkan", "--ignore-gpu-blocklist", "--enable-gpu"]
    if a.webgpu:
        flags += ["--enable-unsafe-webgpu", "--enable-features=Vulkan,WebGPU"]
    if a.scale != 1:
        # the display's own scale, as Windows' is: the canvas's device pixels
        # follow it (CDP's emulated ratio leaves them at the CSS size)
        flags.append(f"--force-device-scale-factor={a.scale}")
    flags += a.chrome
    env = dict(os.environ, TMPDIR=str(tmp), DBUS_SESSION_BUS_ADDRESS="disabled:")
    proc = subprocess.Popen(flags + ["about:blank"], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(100):
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{a.port}/json/version", timeout=1)
                break
            except OSError:
                await asyncio.sleep(0.1)
        req = urllib.request.Request(f"http://127.0.0.1:{a.port}/json/new?about:blank", method="PUT")
        target = json.load(urllib.request.urlopen(req))
        return await drive(a, base, target["webSocketDebuggerUrl"])
    finally:
        proc.terminate()
        try:
            proc.wait(5)
        except subprocess.TimeoutExpired:
            proc.kill()
        if httpd:
            httpd.shutdown()


async def drive(a, base, ws_url):
    async with websockets.connect(ws_url, max_size=1 << 28) as ws:
        seq = 0
        pending = {}
        lines = []
        until = asyncio.Event()

        async def call(method, **params):
            nonlocal seq
            seq += 1
            fut = asyncio.get_running_loop().create_future()
            pending[seq] = fut
            await ws.send(json.dumps({"id": seq, "method": method, "params": params}))
            return await fut

        def out(text):
            line = f"[{time.monotonic() - t0:7.2f}] {text}"
            print(line, flush=True)
            lines.append(line)
            if a.until and a.until in text:
                until.set()

        async def reader():
            async for raw in ws:
                m = json.loads(raw)
                if "id" in m and m["id"] in pending:
                    pending.pop(m["id"]).set_result(m.get("result", m.get("error")))
                elif m.get("method") == "Runtime.consoleAPICalled":
                    p = m["params"]
                    out(" ".join(str(x.get("value", x.get("description", ""))) for x in p["args"]))
                elif m.get("method") == "Runtime.exceptionThrown":
                    d = m["params"]["exceptionDetails"]
                    out("EXCEPTION " + d.get("exception", {}).get("description", d.get("text", "")))
                elif m.get("method") == "Log.entryAdded":
                    e = m["params"]["entry"]
                    out(f"log {e.get('level')}: {e.get('text')}")

        t0 = time.monotonic()
        rt = asyncio.ensure_future(reader())
        await call("Runtime.enable")
        await call("Log.enable")
        await call("Page.enable")
        q = "&".join(p for p in [a.params, f"character={a.character}" if a.character else ""] if p)
        if a.mobile:
            await call("Emulation.setTouchEmulationEnabled", enabled=True, maxTouchPoints=5)
        if a.gamepad:
            # one standard-layout pad, played by --gamepad
            await call("Page.addScriptToEvaluateOnNewDocument", source="""
                window.drivePad = { id: 'drive.py pad', index: 0, connected: true, mapping: 'standard', timestamp: 0,
                    buttons: Array.from({ length: 17 }, () => ({ pressed: false, touched: false, value: 0 })),
                    axes: [0, 0, 0, 0], vibrationActuator: null };
                Object.defineProperty(navigator, 'getGamepads', { configurable: true, value: () => [window.drivePad] });""")
        for src in a.init:
            await call("Page.addScriptToEvaluateOnNewDocument", source=src)
        await call("Page.navigate", url=base + ("?" + q if q else ""))
        await asyncio.sleep(2)

        async def js(expr):
            r = await call("Runtime.evaluate", expression=expr, awaitPromise=True)
            return r.get("result", {}).get("value") if r else None

        for _ in range(100):
            if await js("document.getElementById('status').textContent") == "ready":
                break
            await asyncio.sleep(0.1)
        doc = await call("DOM.getDocument")
        node = await call("DOM.querySelector", nodeId=doc["root"]["nodeId"], selector="#iso")
        await call("DOM.setFileInputFiles", nodeId=node["nodeId"], files=[str(Path(a.iso).resolve())])

        def setv(k, v):
            return js(f"document.getElementById({json.dumps(k)}).value = {json.dumps(v)}")

        if a.view is not None:
            await setv("viewargs", a.view)
            await js("document.getElementById('view').click()")
        else:
            if a.new_account:
                await js("document.getElementById('tab-new').click()")
                await setv("pass2", os.environ.get("EQOA_PASSWORD", ""))
            await setv("user", a.play)
            await setv("pass", os.environ.get("EQOA_PASSWORD", ""))
            await js("document.getElementById('go').click()")
        t_click = time.monotonic()

        async def shoot(path):
            r = await call("Page.captureScreenshot", format="png")
            Path(path).write_bytes(base64.b64decode(r["data"]))
            out(f"screenshot {path}")

        async def key(k, down):
            code = {"w": "KeyW", "a": "KeyA", "s": "KeyS", "d": "KeyD"}.get(k, k)
            await call("Input.dispatchKeyEvent", type="keyDown" if down else "keyUp", key=k, code=code,
                       text=k if (down and len(k) == 1) else "", windowsVirtualKeyCode=ord(k.upper()) if len(k) == 1 else 0)

        async def touch(kind, pts):
            await call("Input.dispatchTouchEvent", type=kind, touchPoints=[{"x": x, "y": y, "id": 1} for x, y in pts])

        async def touches(spec):
            for step in spec.split(";"):
                w = step.split()
                if not w:
                    continue
                if w[0] == "tap":
                    x, y = float(w[1]), float(w[2])
                    await touch("touchStart", [(x, y)])
                    await asyncio.sleep(0.08)
                    await touch("touchEnd", [])
                elif w[0] == "hold":
                    x, y, ms = float(w[1]), float(w[2]), int(w[3])
                    await touch("touchStart", [(x, y)])
                    await asyncio.sleep(ms / 1000)
                    await touch("touchEnd", [])
                elif w[0] == "drag":
                    x0, y0, x1, y1, ms = float(w[1]), float(w[2]), float(w[3]), float(w[4]), int(w[5])
                    await touch("touchStart", [(x0, y0)])
                    n = max(2, ms // 16)
                    for k in range(1, n + 1):
                        await asyncio.sleep(ms / 1000 / n)
                        await touch("touchMove", [(x0 + (x1 - x0) * k / n, y0 + (y1 - y0) * k / n)])
                    await touch("touchEnd", [])
                out(f"touch {step}")

        async def mice(spec):
            for step in spec.split(";"):
                w = step.split()
                if not w:
                    continue
                x, y = float(w[1]), float(w[2])
                await call("Input.dispatchMouseEvent", type="mouseMoved", x=x, y=y)
                if w[0] in ("click", "rclick"):
                    b = "left" if w[0] == "click" else "right"
                    for kind in ("mousePressed", "mouseReleased"):
                        await call("Input.dispatchMouseEvent", type=kind, x=x, y=y, button=b, clickCount=1)
                        await asyncio.sleep(0.03)
                elif w[0] == "wheel":
                    # a line is a hundred pixels to Chromium; down is positive there
                    await call("Input.dispatchMouseEvent", type="mouseWheel", x=x, y=y, deltaX=0, deltaY=-100 * float(w[3]))
                elif w[0] == "drag":
                    # the left button held from (x, y) to (x1, y1) over MS (default 300)
                    x1, y1 = float(w[3]), float(w[4])
                    ms = int(w[5]) if len(w) > 5 else 300
                    await call("Input.dispatchMouseEvent", type="mousePressed", x=x, y=y, button="left", buttons=1, clickCount=1)
                    n = max(2, ms // 16)
                    for i in range(1, n + 1):
                        await asyncio.sleep(ms / 1000 / n)
                        await call("Input.dispatchMouseEvent", type="mouseMoved", x=x + (x1 - x) * i / n, y=y + (y1 - y) * i / n, button="left", buttons=1)
                    await call("Input.dispatchMouseEvent", type="mouseReleased", x=x1, y=y1, button="left", buttons=0, clickCount=1)
                out(f"mouse {step}")

        async def gamepad(spec):
            for step in spec.split(";"):
                w = step.split()
                if not w:
                    continue
                b = "window.drivePad.buttons"
                if w[0] in ("tap", "press"):
                    await js(f"{b}[{int(w[1])}].pressed = true, {b}[{int(w[1])}].value = 1")
                if w[0] == "tap":
                    await asyncio.sleep(int(w[2] if len(w) > 2 else 150) / 1000)
                if w[0] in ("tap", "release"):
                    await js(f"{b}[{int(w[1])}].pressed = false, {b}[{int(w[1])}].value = 0")
                if w[0] == "axes":
                    await js(f"window.drivePad.axes = [{','.join(str(float(v)) for v in w[1:5])}]")
                await js("window.drivePad.timestamp = performance.now()")
                out(f"gamepad {step}")

        events = []
        for s in a.touch:
            t, spec = s.split("=", 1)
            events.append((float(t), "touch", spec))
        for s in a.gamepad:
            t, spec = s.split("=", 1)
            events.append((float(t), "gamepad", spec))
        for s in a.mouse:
            t, spec = s.split("=", 1)
            events.append((float(t), "mouse", spec))
        for s in a.shot:
            t, f = s.split("=", 1)
            events.append((float(t), "shot", f))
        for s in a.keys:
            t, spec = s.split("=", 1)
            events.append((float(t), "keys", spec))
        for s in a.js:
            t, expr = s.split("=", 1)
            events.append((float(t), "js", expr))
        for s in a.resize:
            t, size = s.split("=", 1)
            events.append((float(t), "resize", size))
        events.sort()
        deadline = t_click + a.timeout
        for t, kind, arg in events:
            while time.monotonic() < t_click + t:
                if until.is_set():
                    break
                await asyncio.sleep(0.05)
            if kind == "shot":
                await shoot(arg)
            elif kind == "js":
                out(f"js {arg[:60]} -> {await js(arg)}")
            elif kind == "touch":
                await touches(arg)
            elif kind == "gamepad":
                await gamepad(arg)
            elif kind == "mouse":
                await mice(arg)
            elif kind == "resize":
                w, h = (int(v) for v in arg.split("x"))
                await call("Emulation.setDeviceMetricsOverride", width=w, height=h, deviceScaleFactor=a.scale, mobile=False)
                out(f"resized to {w}x{h}")
            else:
                for part in arg.split(","):
                    k, ms = part.split(":")
                    if ms != "up":
                        await js("document.getElementById('eqoa').focus()")
                        await key(k, True)
                    if ms not in ("down", "up"):
                        await asyncio.sleep(int(ms) / 1000)
                    if ms != "down":
                        await key(k, False)
                    out(f"keys {part}")
        while time.monotonic() < deadline and not until.is_set() and a.until:
            await asyncio.sleep(0.1)
        if a.final_shot:
            await shoot(a.final_shot)
        rt.cancel()
        return 0 if (not a.until or until.is_set()) else 2


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--iso", required=True)
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--view", help="the viewer's file and arguments")
    g.add_argument("--play", metavar="USER", help="log in as USER (password in EQOA_PASSWORD)")
    ap.add_argument("--character", help="the page's ?character=: played at once, skipping the select screen")
    ap.add_argument("--new-account", action="store_true")
    ap.add_argument("--js", action="append", default=[])
    ap.add_argument("--init", action="append", default=[], help="a script run before the page's own (wrap a constructor, say)")
    ap.add_argument("--params", default="", help="the page's query string")
    ap.add_argument("--base", help="a page already served (default: web/dist on --http-port)")
    ap.add_argument("--http-port", type=int, default=0, help="0: any free port")
    ap.add_argument("--shot", action="append", default=[])
    ap.add_argument("--keys", action="append", default=[])
    ap.add_argument("--resize", action="append", default=[], help="SECONDS=WxH: the page's viewport then, as a window resized (or put full screen)")
    ap.add_argument("--mobile", action="store_true", help="a touch-first device (touch emulation: the pointer is coarse)")
    ap.add_argument("--touch", action="append", default=[], help="SECONDS=STEPS: touches on the canvas")
    ap.add_argument("--gamepad", action="append", default=[], help="SECONDS=STEPS: a gamepad, through navigator.getGamepads")
    ap.add_argument("--mouse", action="append", default=[], help="SECONDS=STEPS: the mouse over the canvas")
    ap.add_argument("--final-shot")
    ap.add_argument("--until")
    ap.add_argument("--timeout", type=float, default=60)
    ap.add_argument("--profile", default="~/.cache/eqoa-rs-web/profile")
    ap.add_argument("--port", type=int, default=9334)
    ap.add_argument("--size", default="1280,800")
    ap.add_argument("--scale", type=float, default=1, help="the device's pixel ratio (Windows' display scaling: 1.25, 1.5)")
    ap.add_argument("--gpu", action=argparse.BooleanOptionalAction, default=True)
    ap.add_argument("--webgpu", action="store_true", help="ask Chromium for WebGPU too")
    ap.add_argument("--chrome", action="append", default=[])
    ap.add_argument("--headed", action="store_true", help="a window on $DISPLAY (an Xvfb with a window manager) for what needs one: F11")
    sys.exit(asyncio.run(main(ap.parse_args())))
