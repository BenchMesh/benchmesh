#!/usr/bin/env python3
"""A strict fake RIGOL MHO954 on a loopback socket, written from the MHO900 Programming Guide.

No MHO900 is on the maintainers' bench: this runs the real server over its real LAN link without
one. It proves the definition sends only what the guide documents, not that a real scope agrees.
A command the guide does not document queues an error and is printed on stderr. It starts with a
deep read window set, as another program may leave it, and refuses the screen trace until the
window is reset.

    python3 scripts/mho900_fake.py [--port 5555] [--idn "NK TECHNOLOGIES,MHO954,FAKE,00.01.03"]
"""
import argparse
import math
import re
import socket
import struct
import sys
import threading

POINTS = 1000
YREF = 2048
NO_ERROR = '0,"No error"'


class Scope:
    def __init__(self, idn):
        self.idn = idn
        self.errors = []
        self.running = True
        self.sweep, self.acq, self.source, self.slope, self.level = "AUTO", "NORM", "CHAN1", "POS", 0.0
        self.timebase, self.tb_offset = 2e-6, 0.0
        self.wav = {"src": 1, "mode": "NORM", "form": "BYTE", "start": 5_000_000, "stop": 9_000_000}
        self.ch = {n: {"disp": n <= 2, "scale": 0.5, "offset": 0.0, "coup": "DC", "imp": "OMEG", "probe": 10.0, "bw": "OFF", "inv": False} for n in range(1, 5)}

    def fail(self, cmd, why):
        self.errors.append(f'-113,"{why}: {cmd}"')
        print(f"REFUSED {cmd!r}: {why}", file=sys.stderr, flush=True)

    def yinc(self, ch):
        return self.ch[ch]["scale"] * 8 / 3000

    def trace(self, ch):
        c, amp = self.ch[ch], 1.2 / ch
        out = []
        for i in range(POINTS):
            v = amp * math.sin(2 * math.pi * 3 * ch * i / POINTS) * (-1 if c["inv"] else 1)
            out.append(max(0, min(4095, round(v / self.yinc(ch)) + YREF)))
        return out

    def measure(self, item, ch):
        amp = 1.2 / ch
        return {"VPP": 2 * amp, "VAVG": 0.0, "VRMS": amp / math.sqrt(2), "FREQuency": 3 * ch / (10 * self.timebase)}[item]

    def block(self, cmd):
        w = self.wav
        ch = w["src"]
        if not self.ch[ch]["disp"]:
            return self.fail(cmd, "source channel is not displayed")
        if w["mode"] == "NORM" and not 1 <= w["start"] <= w["stop"] <= POINTS:
            return self.fail(cmd, "read window outside 1..1000 for the screen")
        codes = self.trace(ch)
        data = struct.pack(f"<{POINTS}H", *codes) if w["form"] == "WORD" else bytes(c >> 4 for c in codes)
        return f"#9{len(data):09d}".encode() + data + b"\n"

    def handle(self, cmd):
        c = cmd.strip()
        if c == "*IDN?":
            return self.idn
        if c in (":SYSTem:ERRor?", ":SYST:ERR?", "SYST:ERR?", ":SYSTem:ERRor:NEXT?"):
            return self.errors.pop(0) if self.errors else NO_ERROR
        if c in (":RUN", ":STOP", ":SINGle", ":CLEar", ":AUToset"):
            self.running = c != ":STOP"
            return None
        m = re.fullmatch(r":CHANnel([1-4]):(DISPlay|SCALe|OFFSet|COUPling|IMPedance|PROBe|BWLimit|INVert)(\?| (.+))", c)
        if m:
            return self.channel(c, int(m[1]), m[2], m[3] == "?", m[4])
        m = re.fullmatch(r":WAVeform:(SOURce CHANnel([1-4])|MODE NORMal|FORMat (WORD|BYTE)|STARt (\d+)|STOP (\d+)|PREamble\?|DATA\?)", c)
        if m:
            if m[2]:
                if not self.ch[int(m[2])]["disp"]:
                    return self.fail(c, "source channel is not displayed")
                self.wav["src"] = int(m[2])
            elif m[1].startswith("MODE"):
                self.wav["mode"] = "NORM"
            elif m[3]:
                self.wav["form"] = m[3]
            elif m[4]:
                self.wav["start"] = int(m[4])
            elif m[5]:
                self.wav["stop"] = int(m[5])
            elif m[1] == "DATA?":
                return self.block(c)
            else:
                word = self.wav["form"] == "WORD"
                dx = self.timebase * 10 / POINTS
                inc, ref = (self.yinc(self.wav["src"]), YREF) if word else (self.yinc(self.wav["src"]) * 16, YREF >> 4)
                return f"{int(word)},0,{POINTS},1,{dx:.6E},{self.tb_offset - self.timebase * 5:.6E},0.000000E-12,{inc:.6E},0,{ref}"
            return None
        m = re.fullmatch(r":MEASure:ITEM\? (VPP|VAVG|VRMS|FREQuency),CHANnel([1-4])", c)
        if m:
            return f"{self.measure(m[1], int(m[2])):.6E}"
        return self.settings(c)

    def channel(self, c, n, what, query, value):
        s = self.ch[n]
        if query:
            return {"DISPlay": lambda: str(int(s["disp"])), "SCALe": lambda: f'{s["scale"]:.6E}', "OFFSet": lambda: f'{s["offset"]:.6E}',
                    "COUPling": lambda: s["coup"], "IMPedance": lambda: s["imp"], "PROBe": lambda: f'{s["probe"]:.6E}',
                    "BWLimit": lambda: s["bw"], "INVert": lambda: str(int(s["inv"]))}[what]()
        legal = {"COUPling": {"AC", "DC", "GND"}, "IMPedance": {"OMEG", "FIFTy"}, "BWLimit": {"OFF", "ON", "20M", "250M"}, "DISPlay": {"0", "1", "ON", "OFF"}, "INVert": {"0", "1", "ON", "OFF"}}
        if what in legal and value not in legal[what]:
            return self.fail(c, "value the guide does not list")
        if what == "DISPlay":
            s["disp"] = value in ("1", "ON")
        elif what == "INVert":
            s["inv"] = value in ("1", "ON")
        elif what == "SCALe":
            s["scale"] = float(value)
        elif what == "OFFSet":
            s["offset"] = float(value)
        elif what == "PROBe":
            s["probe"] = float(value)
        elif what == "COUPling":
            s["coup"] = value
        elif what == "IMPedance":
            s["imp"] = "FIFT" if value == "FIFTy" else value
        elif what == "BWLimit":
            s["bw"] = value
        return None

    def settings(self, c):
        status = "STOP" if not self.running else "AUTO"
        queries = {":TIMebase:MAIN:SCALe?": f"{self.timebase:.6E}", ":TIMebase:MAIN:OFFSet?": f"{self.tb_offset:.6E}", ":TRIGger:STATus?": status,
                   ":TRIGger:SWEep?": self.sweep, ":TRIGger:EDGE:SOURce?": self.source, ":TRIGger:EDGE:SLOPe?": self.slope,
                   ":TRIGger:EDGE:LEVel?": f"{self.level:.6E}", ":ACQuire:TYPE?": self.acq, ":ACQuire:SRATe?": "1.000000E+9"}
        if c in queries:
            return queries[c]
        m = re.fullmatch(r":TIMebase:MAIN:(SCALe|OFFSet) (\S+)", c)
        if m:
            if m[1] == "SCALe":
                self.timebase = float(m[2])
            else:
                self.tb_offset = float(m[2])
            return None
        m = re.fullmatch(r":TRIGger:(SWEep (AUTO|NORMal|SINGle)|EDGE:SOURce (CHANnel[1-4])|EDGE:SLOPe (POSitive|NEGative|RFALl)|EDGE:LEVel (\S+))", c)
        if m:
            if m[2]:
                self.sweep = m[2][:4].upper()
            elif m[3]:
                self.source = "CHAN" + m[3][-1]
            elif m[4]:
                self.slope = {"POSitive": "POS", "NEGative": "NEG", "RFALl": "RFAL"}[m[4]]
            else:
                self.level = float(m[5])
            return None
        m = re.fullmatch(r":ACQuire:TYPE (NORMal|PEAK|AVERages|HRESolution)", c)
        if m:
            self.acq = {"NORMal": "NORM", "PEAK": "PEAK", "AVERages": "AVER", "HRESolution": "HRES"}[m[1]]
            return None
        return self.fail(c, "not in the programming guide")


def serve(conn, scope, lock):
    buf = b""
    with conn:
        while chunk := conn.recv(4096):
            buf += chunk
            while b"\n" in buf:
                line, buf = buf.split(b"\n", 1)
                with lock:
                    reply = scope.handle(line.decode())
                if reply is not None:
                    conn.sendall(reply if isinstance(reply, bytes) else reply.encode() + b"\n")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--port", type=int, default=5555)
    ap.add_argument("--idn", default="RIGOL TECHNOLOGIES,MHO954,FAKE0000000,00.01.00")
    args = ap.parse_args()
    scope, lock = Scope(args.idn), threading.Lock()
    srv = socket.create_server(("127.0.0.1", args.port))
    print(f"fake MHO954 on 127.0.0.1:{args.port}", flush=True)
    while True:
        conn, _ = srv.accept()
        threading.Thread(target=serve, args=(conn, scope, lock), daemon=True).start()


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
