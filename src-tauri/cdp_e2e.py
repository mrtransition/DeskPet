"""Drive the real DeskPet app over WebView2 CDP (raw WebSocket, no deps)."""
import base64, json, os, socket, struct, sys, time, urllib.request

CDP = "http://127.0.0.1:19222"


class WS:
    def __init__(self, url):
        assert url.startswith("ws://")
        rest = url[5:]
        hostport, path = rest.split("/", 1)
        host, port = hostport.split(":")
        self.s = socket.create_connection((host, int(port)), timeout=60)
        key = base64.b64encode(os.urandom(16)).decode()
        req = (f"GET /{path} HTTP/1.1\r\nHost: {hostport}\r\nUpgrade: websocket\r\n"
               f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n")
        self.s.sendall(req.encode())
        buf = b""
        while b"\r\n\r\n" not in buf:
            buf += self.s.recv(4096)
        assert b"101" in buf.split(b"\r\n")[0], buf[:200]

    def _recv_exact(self, n):
        b = b""
        while len(b) < n:
            c = self.s.recv(n - len(b))
            if not c:
                raise ConnectionError("closed")
            b += c
        return b

    def send(self, text):
        data = text.encode()
        hdr = bytearray([0x81])
        n = len(data)
        if n < 126:
            hdr.append(0x80 | n)
        elif n < 65536:
            hdr.append(0x80 | 126); hdr += struct.pack(">H", n)
        else:
            hdr.append(0x80 | 127); hdr += struct.pack(">Q", n)
        mask = os.urandom(4)
        hdr += mask
        self.s.sendall(bytes(hdr) + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))

    def recv(self):
        while True:
            b0, b1 = self._recv_exact(2)
            op = b0 & 0x0F
            n = b1 & 0x7F
            if n == 126:
                n = struct.unpack(">H", self._recv_exact(2))[0]
            elif n == 127:
                n = struct.unpack(">Q", self._recv_exact(8))[0]
            payload = self._recv_exact(n)
            if op == 1:
                return json.loads(payload)
            if op == 8:
                raise ConnectionError("ws closed")


def page_url(part):
    with urllib.request.urlopen(CDP + "/json/list") as r:
        for p in json.load(r):
            if part in p["url"]:
                return p["webSocketDebuggerUrl"]
    raise SystemExit("no page " + part)


class Tab:
    def __init__(self, part):
        self.ws = WS(page_url(part))
        self.n = 0

    def eval(self, expr):
        self.n += 1
        self.ws.send(json.dumps({"id": self.n, "method": "Runtime.evaluate",
                                 "params": {"expression": expr, "awaitPromise": True, "returnByValue": True}}))
        while True:
            m = self.ws.recv()
            if m.get("id") == self.n:
                r = m.get("result", {})
                if "exceptionDetails" in r:
                    d = r["exceptionDetails"]
                    return {"__error": d.get("text"), "detail": str(d.get("exception", {}).get("description"))[:500]}
                return r.get("result", {}).get("value")


def main():
    t = Tab("bubble.html")
    print("open:", t.eval("""(async () => {
      await loadState();
      newSession();
      const p = S.providers.find(x => x.id === 'p-local');
      const s = curSession();
      s.providerId = p.id; s.personaId = S.personas[0].id;
      await saveStateNow();
      return {prov: p.name, base: p.baseUrl, auth: S.settings.toolSettings};
    })()"""))
    print("send:", t.eval("send('帮我看看剪贴板')"))
    last = None
    for _ in range(60):
        time.sleep(0.5)
        last = t.eval("({streaming: streaming, roles: curSession().msgs.map(m=>m.role), "
                      "cards: [...document.querySelectorAll('.msg.tool')].map(e=>e.textContent.trim().slice(0,50))})")
        if isinstance(last, dict) and last.get("streaming") is False and (last.get("roles") or [])[-1:] == ["assistant"]:
            break
    print("state:", json.dumps(last, ensure_ascii=False))
    print("msgs:", json.dumps(t.eval("curSession().msgs.map(m=>({r:m.role,c:(m.content||'').slice(0,60)}))"), ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
