#!/usr/bin/env python3
"""drive -- run the browser client in a headless Chromium and watch it, for tests.

Serves web/dist on a local port, opens the page in a Chromium of its own over
the DevTools protocol, puts the disc image into the page's file input (the
same as a player picking it), presses View or Start, prints the page's
console, and screenshots it. After Start the game runs from its boot on the
canvas (the licence, the title, Station's login screen...): drive it with
--keys, --type, --gamepad and --mouse.

    web/drive.py --iso run/eqoa.iso --view 'CHAR.ESF 0x70D898C6' --shot 8=human.png
    web/drive.py --iso run/eqoa.iso --boot --params 'relay=ws://127.0.0.1:8790&server=203.0.113.10' \\
        --keys 12=Enter:100 --type 20=dab --shot 30=station.png

--shot SECONDS=FILE.png screenshots that long after the button (repeatable);
--until TEXT exits 0 once a console line has it; --keys SECONDS=KEYS holds
keys for a while (w, a, s, d, ArrowLeft...: 'w:2000' holds w for 2 s; 'w:down'
and 'w:up' press it and let it go apart, the canvas focused only for the press,
so a --js between them can move the focus: a key let go elsewhere);
--type SECONDS=TEXT types TEXT on the canvas then, a character at a time
(an account's name into Station's login; a password from the environment:
--type 20='$EQOA_PASSWORD'); --insert SECONDS=TEXT puts TEXT into the element
with the focus as a phone's own keyboard does (CDP Input.insertText, a newline
its Enter; the page's input for the game's edit boxes once a touch has put the
focus there), and prints only how many; --js SECONDS=EXPR evaluates EXPR in
the page then, its value printed.
WebGL/WebGPU run on the machine's GPU (--no-gpu: SwiftShader, every core).
--headed opens a real window on $DISPLAY instead (F11 needs one); --resize
SECONDS=WxH sets the page's viewport then, as a resized window would; --scale
is the device's pixel ratio throughout (Windows' display scaling, 1.25 or 1.5;
a phone's, 2 to 3).
--mobile makes the page a touch-first device (CDP's touch emulation: its
pointer is coarse); --touch SECONDS=STEPS touches the canvas then, steps
';'-separated: 'tap X Y', 'hold X Y MS', 'drag X0 Y0 X1 Y1 MS' (CSS pixels);
--mouse SECONDS=STEPS moves the mouse over the canvas, steps ';'-separated:
'click X Y', 'rclick X Y', 'wheel X Y LINES' (up positive), 'move X Y [MS]'
(there over MS, with what is held), 'drag X0 Y0 X1 Y1 [MS]' (the left button
held), 'rdrag X0 Y0 X1 Y1 [MS]' (the right), 'press X Y [left|right]' and
'release X Y [left|right]' (a button held across steps and events);
--gamepad SECONDS=STEPS plays a gamepad the page sees through
navigator.getGamepads (the standard layout's indices: 0 A, 1 B, 2 X, 3 Y,
4 LB, 5 RB, 6 LT, 7 RT, 8 Back, 9 Start, 10 L3, 11 R3, 12-15 the d-pad):
'tap N [MS]', 'press N', 'release N', 'axes LX LY RX RY'.
--hide SECONDS=DURATION puts another tab in front then, for DURATION seconds, so
the page is hidden as a player's is when they switch tabs (no animation
frames, its timers held back; with --headed the browser's own throttling,
which headless may not do), then brings the page's tab back.
--iso-2002 PATH puts the 2002 disc (EverQuest Online Adventures (USA),
SLUS-20470) into the page's second input with the first, for the art on F9;
--ask-2002 PATH answers the picker F9's ask opens in the game (the file
chooser intercepted) with it. --start-when EXPR counts every event's seconds
from when EXPR first holds in the page, or with line:TEXT from the first
console line with TEXT ('line:the HUD is in': SUPPORT's unpack, which the
title waits on and which varies with the machine's load).
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
        return await drive(a, base, target["webSocketDebuggerUrl"], target["id"])
    finally:
        proc.terminate()
        try:
            proc.wait(5)
        except subprocess.TimeoutExpired:
            proc.kill()
        if httpd:
            httpd.shutdown()


async def drive(a, base, ws_url, target_id):
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
                    out(f"log {e.get('level')}: {e.get('text')}" + (f" ({e['url']})" if e.get("url") else ""))
                elif m.get("method") == "Page.fileChooserOpened" and a.ask_2002:
                    # F9's ask for the 2002 disc: answered with --ask-2002
                    node = m["params"]["backendNodeId"]
                    asyncio.ensure_future(call("DOM.setFileInputFiles", backendNodeId=node, files=[str(Path(a.ask_2002).resolve())]))
                    out(f"file chooser: {a.ask_2002}")

        t0 = time.monotonic()
        rt = asyncio.ensure_future(reader())
        await call("Runtime.enable")
        await call("Log.enable")
        await call("Page.enable")
        q = a.params
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
        if a.iso_2002:
            node = await call("DOM.querySelector", nodeId=doc["root"]["nodeId"], selector="#iso2002")
            await call("DOM.setFileInputFiles", nodeId=node["nodeId"], files=[str(Path(a.iso_2002).resolve())])
            for _ in range(100):
                if await js("document.getElementById('iso2002-what').textContent"):
                    break
                await asyncio.sleep(0.1)
            out("2002 disc: " + str(await js("document.getElementById('iso2002-what').textContent")))
        if a.ask_2002:
            await call("Page.setInterceptFileChooserDialog", enabled=True)

        def setv(k, v):
            return js(f"document.getElementById({json.dumps(k)}).value = {json.dumps(v)}")

        if a.view is not None:
            await setv("viewargs", a.view)
            await js("document.getElementById('view').click()")
        else:
            await js("document.getElementById('go').click()")
        t_click = time.monotonic()
        if a.start_when:
            # the events' clock starts when EXPR holds (a stage the client
            # notes it is in, past the boot's own variable wait)
            while time.monotonic() < t_click + a.timeout:
                if a.start_when.startswith("line:"):
                    if any(a.start_when[5:] in l for l in lines):
                        break
                else:
                    try:
                        if await js(a.start_when):
                            break
                    except Exception:
                        pass
                await asyncio.sleep(0.25)
            out(f"start-when held after {time.monotonic() - t_click:.1f} s")
            t_click = time.monotonic()

        async def shoot(path):
            r = await call("Page.captureScreenshot", format="png")
            Path(path).write_bytes(base64.b64decode(r["data"]))
            out(f"screenshot {path}")

        # A character as a US keyboard sends it: its physical key and the
        # Windows key code (ord('#') is 35, VK_END: '#' arrived as End).
        SYMBOLS = {"#": ("Digit3", 51), "!": ("Digit1", 49), "@": ("Digit2", 50), "$": ("Digit4", 52),
                   "/": ("Slash", 191), "?": ("Slash", 191), " ": ("Space", 32), ".": ("Period", 190),
                   "-": ("Minus", 189), "_": ("Minus", 189), "'": ("Quote", 222), "`": ("Backquote", 192),
                   "~": ("Backquote", 192), "=": ("Equal", 187), "+": ("Equal", 187)}

        async def key(k, down):
            if len(k) == 1 and k.isalpha():
                code, vk = "Key" + k.upper(), ord(k.upper())
            elif len(k) == 1 and k.isdigit():
                code, vk = "Digit" + k, ord(k)
            elif k in SYMBOLS:
                code, vk = SYMBOLS[k]
            else:
                code, vk = k, 0
            await call("Input.dispatchKeyEvent", type="keyDown" if down else "keyUp", key=k, code=code,
                       text=k if (down and len(k) == 1) else "", windowsVirtualKeyCode=vk)

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

        # the buttons a 'press' holds (1 left, 2 right) and where the mouse is
        mouse = {"mask": 0, "at": (0.0, 0.0)}

        async def moved(x, y):
            m = mouse["mask"]
            b = "left" if m & 1 else "right" if m & 2 else "none"
            await call("Input.dispatchMouseEvent", type="mouseMoved", x=x, y=y, button=b, buttons=m)
            mouse["at"] = (x, y)

        async def mice(spec):
            for step in spec.split(";"):
                w = step.split()
                if not w:
                    continue
                x, y = float(w[1]), float(w[2])
                if w[0] == "move":
                    # there over MS (one step by default), with what is held
                    ms = int(w[3]) if len(w) > 3 else 16
                    n = max(1, ms // 16)
                    x0, y0 = mouse["at"]
                    for i in range(1, n + 1):
                        await asyncio.sleep(ms / 1000 / n)
                        await moved(x0 + (x - x0) * i / n, y0 + (y - y0) * i / n)
                    out(f"mouse {step}")
                    continue
                await moved(x, y)
                if w[0] in ("press", "release"):
                    b = w[3] if len(w) > 3 else "left"
                    bit = 1 if b == "left" else 2
                    mouse["mask"] = mouse["mask"] | bit if w[0] == "press" else mouse["mask"] & ~bit
                    await call("Input.dispatchMouseEvent", type="mousePressed" if w[0] == "press" else "mouseReleased", x=x, y=y, button=b, buttons=mouse["mask"], clickCount=1)
                elif w[0] in ("click", "rclick"):
                    b = "left" if w[0] == "click" else "right"
                    for kind in ("mousePressed", "mouseReleased"):
                        await call("Input.dispatchMouseEvent", type=kind, x=x, y=y, button=b, clickCount=1)
                        await asyncio.sleep(0.03)
                elif w[0] == "wheel":
                    # a line is a hundred pixels to Chromium; down is positive there
                    await call("Input.dispatchMouseEvent", type="mouseWheel", x=x, y=y, deltaX=0, deltaY=-100 * float(w[3]))
                elif w[0] in ("drag", "rdrag"):
                    # the left (right) button held from (x, y) to (x1, y1) over MS (default 300)
                    x1, y1 = float(w[3]), float(w[4])
                    ms = int(w[5]) if len(w) > 5 else 300
                    b, bit = ("left", 1) if w[0] == "drag" else ("right", 2)
                    await call("Input.dispatchMouseEvent", type="mousePressed", x=x, y=y, button=b, buttons=bit, clickCount=1)
                    n = max(2, ms // 16)
                    for i in range(1, n + 1):
                        await asyncio.sleep(ms / 1000 / n)
                        await call("Input.dispatchMouseEvent", type="mouseMoved", x=x + (x1 - x) * i / n, y=y + (y1 - y) * i / n, button=b, buttons=bit)
                    await call("Input.dispatchMouseEvent", type="mouseReleased", x=x1, y=y1, button=b, buttons=0, clickCount=1)
                    mouse["at"] = (x1, y1)
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
        for s in a.type:
            t, text = s.split("=", 1)
            events.append((float(t), "type", os.environ.get(text[1:], "") if text.startswith("$") else text))
        for s in a.insert:
            t, text = s.split("=", 1)
            events.append((float(t), "insert", os.environ.get(text[1:], "") if text.startswith("$") else text))
        for s in a.hide:
            t, dur = s.split("=", 1)
            events.append((float(t), "hide", ""))
            events.append((float(t) + float(dur), "unhide", ""))
        events.sort()
        other = []  # the tab put in front of the page (--hide)

        def devtools(path, method="GET"):
            req = urllib.request.Request(f"http://127.0.0.1:{a.port}/json/{path}", method=method)
            with urllib.request.urlopen(req) as r:
                body = r.read()
            return json.loads(body) if body.strip().startswith(b"{") else body
        deadline = t_click + a.timeout
        for t, kind, arg in events:
            while time.monotonic() < t_click + t:
                if until.is_set():
                    break
                await asyncio.sleep(0.05)
            if kind == "shot":
                await shoot(arg)
            elif kind == "hide":
                blank = devtools("new?about:blank", "PUT")
                devtools(f"activate/{blank['id']}")
                other.append(blank["id"])
                out(f"hidden: another tab in front (page visibility {await js('document.visibilityState')})")
            elif kind == "unhide":
                devtools(f"activate/{target_id}")
                if other:
                    devtools(f"close/{other.pop()}")
                await asyncio.sleep(0.5)
                out(f"shown again (page visibility {await js('document.visibilityState')})")
            elif kind == "js":
                out(f"js {arg[:60]} -> {await js(arg)}")
            elif kind == "touch":
                await touches(arg)
            elif kind == "gamepad":
                await gamepad(arg)
            elif kind == "mouse":
                await mice(arg)
            elif kind == "insert":
                # a phone's own keyboard: into the element with the focus, the
                # focus left where it is (the page's input for the game's edit
                # boxes); a newline is its Enter. What it typed is not printed.
                for ch in arg:
                    if ch == "\n":
                        for kind_ in ("keyDown", "keyUp"):
                            await call("Input.dispatchKeyEvent", type=kind_, key="Enter", code="Enter", windowsVirtualKeyCode=13)
                    else:
                        await call("Input.insertText", text=ch)
                    await asyncio.sleep(0.08)
                out(f"inserted {len(arg)} characters into #{await js('document.activeElement ? document.activeElement.id : null')}")
            elif kind == "type":
                await js("document.getElementById('eqoa').focus()")
                for ch in arg:
                    await key(ch, True)
                    await asyncio.sleep(0.04)
                    await key(ch, False)
                    await asyncio.sleep(0.06)
                out(f"typed {len(arg)} characters")
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
    ap.add_argument("--iso-2002", help="the 2002 disc, put into the page's second input with the first")
    ap.add_argument("--ask-2002", help="the 2002 disc, given to the picker F9's ask opens")
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--view", help="the viewer's file and arguments")
    g.add_argument("--boot", action="store_true", help="Start: the game from its boot")
    ap.add_argument("--type", action="append", default=[], help="SECONDS=TEXT: typed on the canvas ($VAR: from the environment)")
    ap.add_argument("--insert", action="append", default=[], help="SECONDS=TEXT: put into the element with the focus as a phone's keyboard puts it, a newline its Enter ($VAR: from the environment)")
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
    ap.add_argument("--start-when", help="a page expression, or line:TEXT for a console line: the events' seconds count from when it first holds ('line:the HUD is in')")
    ap.add_argument("--hide", action="append", default=[], help="SECONDS=DURATION: another tab in front for DURATION seconds (the page hidden)")
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
