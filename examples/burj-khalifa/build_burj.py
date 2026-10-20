"""A simplified structural model of the Burj Khalifa (Dubai, SOM 2004-2010), built in Pondus through
its MCP server exactly as an agent would build it, then analysed for its modes.

What is taken from the published record (see README.md in this folder for the sources):
- the buttressed core: a hexagonal central core, three wings at 120 degrees, corridor walls from the
  core to near each wing tip ending in hammerhead walls, perimeter (wall-like) columns at 9 m
  spacing and nose columns, outriggers at the mechanical floors, flat-plate floors;
- 154 storeys + mechanical levels to the top of concrete at about 586 m, the steel spire above
  (about 4,000 t), wings about 25 m wide reaching about 50 m from the centre at the base (the raft
  wings are 50 m x 25 m), setbacks spiralling up the three wings;
- core walls 1300 to 500 mm, corridor walls and perimeter columns 600 mm thick, C80 concrete with a
  specified modulus of 43,800 MPa;
- a superstructure weight of about 340,000 t (450,000 t with the 110,000 t foundation).

What is assumed (not published in the sources found) is marked ASSUMED below: the corridor width,
the core size, the hammerhead and outrigger proportions, the setback heights (12 setbacks of one
9 m bay instead of the real 27), and two storeys per model level.

Run:  python3 build_burj.py   (needs target/release/pondus; writes burj_khalifa.pondus here)
"""
import json, math, os, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
PONDUS = os.path.join(HERE, "..", "..", "target", "release", "pondus")
MODEL = os.path.join(HERE, "burj_khalifa.pondus")

# ------------------------------------------------------------------ data
H_TOP = 586.0               # top of the concrete tower, m (top floor 585.4 m)
N_LEVELS = 78               # model levels, two storeys each (156 storeys)
HZ = H_TOP / N_LEVELS
LEVELS = [round(k * HZ, 3) for k in range(N_LEVELS + 1)]
W_WING = 25.0               # wing width, m (raft wings 50 x 25 m)
L_BASE = 50.0               # wing tip from the centre at the base, m
S0 = 10.0                   # ASSUMED: corridor walls start 10 m from the centre (the core)
CORR = 3.5                  # ASSUMED: corridor walls at +-3.5 m from the wing axis
HAMMER = 5.0                # ASSUMED: hammerhead walls reach +-5 m
BAY = 9.0                   # perimeter column spacing, m
SIDE = W_WING / 2           # perimeter columns at +-12.5 m
COL_S = [14.0, 23.0, 32.0, 41.0]
OUTRIGGER_Z = [64.0, 154.0, 274.0, 410.0, 511.0]   # mechanical floors ~17, 41, 73, 109, 136 (ASSUMED heights)
W_TARGET = 340_000 * 9.81   # kN, superstructure incl. spire
SPIRE = 4_000 * 9.81        # kN, steel spire on top of the concrete
GAMMA = 24.0
E_C80 = 43_800.0            # MPa, specified for C80 at 90 days
FC = (E_C80 / 4700.0) ** 2  # Pondus takes E = 4700 sqrt(f'c): this f'c gives 43,800 MPa
ANGLES = [90.0, 210.0, 330.0]

def core_t(z):              # core walls 1300 -> 500 mm (ASSUMED steps)
    return 1.3 if z < 150 else 1.0 if z < 300 else 0.8 if z < 450 else 0.6

CORE_BANDS = [(0.0, 150.0), (150.0, 300.0), (300.0, 450.0), (450.0, H_TOP)]

# setbacks: 12 steps of one bay, wing (i-1) % 3 at 45 m intervals (ASSUMED; the real tower has 27)
SETBACK_Z = [45.0 * i for i in range(1, 13)]
def wing_len(k, z):
    n = sum(1 for i, zs in enumerate(SETBACK_Z) if zs <= z + 1e-6 and i % 3 == k)
    return L_BASE - BAY * n

def snap(z):                # setback heights onto model levels
    return min(LEVELS, key=lambda l: abs(l - z))

def frame(k):
    a = math.radians(ANGLES[k])
    u, v = (math.cos(a), math.sin(a)), (-math.sin(a), math.cos(a))
    return lambda s, t: [round(s * u[0] + t * v[0], 4), round(s * u[1] + t * v[1], 4)]

# ------------------------------------------------------------------ the model as MCP commands
cmds = []
def call(name, args):
    cmds.append((name, args))

call("add_material", {"kind": "concrete", "name": "C80 (E 43.8 GPa)", "fc": round(FC, 2)})
MAT = "C80 (E 43.8 GPa)"

# band edges for every wing: the levels where its length changes
def bands(k):
    zs = sorted({0.0, H_TOP} | {snap(z) for i, z in enumerate(SETBACK_Z) if i % 3 == k})
    return [(a, b) for a, b in zip(zs, zs[1:])]

walls_fixed, walls_rest = [], []
def wall(p, q, t, z0, z1, name):
    w = {"from": p, "to": q, "t": t, "z0": z0, "z1": z1, "name": name, "material": MAT}
    (walls_fixed if z0 < 1e-6 else walls_rest).append(w)

# the hexagonal core: corners at the starts of the corridor walls
corner = []
for k in range(3):
    f = frame(k)
    corner += [f(S0, -CORR), f(S0, CORR)]
for (z0, z1) in CORE_BANDS:
    for j in range(6):
        wall(corner[j], corner[(j + 1) % 6], core_t(z0), z0, z1, f"core {j + 1}")

wing_bands = {}
for k in range(3):
    f = frame(k)
    wing_bands[k] = []
    for (z0, z1) in bands(k):
        L = wing_len(k, z0)
        if L - 3.0 <= S0 + 2.0:
            continue            # the wing has gone: the core alone above
        wing_bands[k].append((z0, z1, L))
        e = L - 3.0             # corridor walls end at the hammerhead
        for side in (-1, 1):
            wall(f(S0, side * CORR), f(e, side * CORR), 0.6, z0, z1, f"wing {k + 1} corridor {'+' if side > 0 else '-'}")
        # hammerhead in three pieces so the corridor walls end on its nodes
        wall(f(e, -HAMMER), f(e, -CORR), 0.8, z0, z1, f"wing {k + 1} hammerhead")
        wall(f(e, -CORR), f(e, CORR), 0.8, z0, z1, f"wing {k + 1} hammerhead")
        wall(f(e, CORR), f(e, HAMMER), 0.8, z0, z1, f"wing {k + 1} hammerhead")

call("add_walls", {"walls": walls_fixed, "levels": LEVELS})
call("add_walls", {"walls": walls_rest, "levels": LEVELS, "base": "none"})

# columns, floor beams (they carry the floor mass) and outriggers, level by level
members, beam_len = [], {}
col_base = []
outrig = {snap(z) for z in OUTRIGGER_Z}
for k in range(3):
    f = frame(k)
    for (z0, z1, L) in wing_bands[k]:
        i0, i1 = LEVELS.index(z0), LEVELS.index(z1)
        for i in range(i0, i1):
            za, zb = LEVELS[i], LEVELS[i + 1]
            for s in COL_S:
                if s < L - 3.0 - 0.5:
                    for side in (-1, 1):
                        p = f(s, side * SIDE)
                        members.append({"from": p + [za], "to": p + [zb], "section": "600 x 2400", "material": MAT, "kind": "column", "roll": ANGLES[k]})
                        if za == 0.0:
                            col_base.append(p + [0.0])
                        sec = "1000 x 4000" if zb in outrig else "1000 x 300"
                        members.append({"from": p + [zb], "to": f(s, side * CORR) + [zb], "section": sec, "material": MAT, "kind": "beam"})
                        beam_len[zb] = beam_len.get(zb, 0.0) + (SIDE - CORR)
            # the nose column on the wing axis, tied to the middle of the hammerhead
            p = f(L, 0.0)
            members.append({"from": p + [za], "to": p + [zb], "section": "1200 x 1200", "material": MAT, "kind": "column"})
            if za == 0.0:
                col_base.append(p + [0.0])
            members.append({"from": p + [zb], "to": f(L - 3.0, 0.0) + [zb], "section": "1000 x 300", "material": MAT, "kind": "beam"})
            beam_len[zb] = beam_len.get(zb, 0.0) + 3.0
# the core alone above the last wing: floor beams across the core carry its floors
core_only = [z for z in LEVELS[1:] if z not in beam_len]
for z in core_only:
    for j in (0, 2, 4):
        members.append({"from": corner[j] + [z], "to": corner[(j + 3) % 6] + [z], "section": "1000 x 300", "material": MAT, "kind": "beam"})
        beam_len[z] = beam_len.get(z, 0.0) + math.dist(corner[j], corner[(j + 3) % 6])
for b in range(0, len(members), 400):
    call("add_members", {"members": members[b:b + 400]})
call("add_supports", {"points": col_base, "type": "fixed"})
call("set_diaphragms", {"all": True})

# ------------------------------------------------------------------ the mass
def hex_area():
    a = 0.0
    for j in range(6):
        (x0, y0), (x1, y1) = corner[j], corner[(j + 1) % 6]
        a += x0 * y1 - x1 * y0
    return abs(a) / 2
def floor_area(z):
    a = hex_area()
    for k in range(3):
        for (z0, z1, L) in wing_bands[k]:
            if z0 < z <= z1 + 1e-6:
                a += W_WING * (L - S0)
    return a
# self-weight of the walls, columns and beams of the model, to give the floors the rest of W
self_w = 0.0
for w in walls_fixed + walls_rest:
    self_w += math.dist(w["from"], w["to"]) * w["t"] * (w["z1"] - w["z0"]) * GAMMA
for m in members:
    b, h = (float(x) / 1000 for x in m["section"].split(" x "))
    self_w += b * h * math.dist(m["from"], m["to"]) * GAMMA
area_sum = sum(floor_area(z) for z in LEVELS[1:])
q = (W_TARGET - SPIRE - self_w) / area_sum    # kN/m2 of model level (two floors)
loads = []
for z in LEVELS[1:]:
    wz = floor_area(z) * q + (SPIRE if z == LEVELS[-1] else 0.0)
    loads.append({"type": "line", "z": z, "w": [0.0, 0.0, -round(wz / beam_len[z], 3)]})
call("add_loads", {"case": "D", "loads": loads})

call("set_seismic", {"enabled": True, "live_mass": 0.0, "spectrum": {"kind": "code", "sds": 0.33, "sd1": 0.19, "tl": 8.0}, "r": 5.0, "cd": 4.5, "ie": 1.0})

# ------------------------------------------------------------------ run Pondus
if os.path.exists(MODEL):
    os.remove(MODEL)
proc = subprocess.Popen([PONDUS, "--mcp", "--autosave", MODEL], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
def rpc(i, method, params=None):
    proc.stdin.write(json.dumps({"jsonrpc": "2.0", "id": i, "method": method, "params": params or {}}) + "\n")
    proc.stdin.flush()
    while True:
        line = proc.stdout.readline()
        if not line:
            sys.exit("pondus exited")
        r = json.loads(line)
        if r.get("id") == i:
            return r
rpc(0, "initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "build_burj", "version": "1"}})
proc.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
proc.stdin.flush()
import base64, shutil
SETTLE = 0.0575        # m: final settlement 55-60 mm under the full D + L (Poulos 2009)

def tool(name, args):
    global n_id
    n_id += 1
    r = rpc(n_id, "tools/call", {"name": name, "arguments": args})
    if "error" in r or r.get("result", {}).get("isError"):
        sys.exit(f"{name} failed: {json.dumps(r)[:2000]}")
    return r["result"]["content"]

def text(c):
    return c[0].get("text", "")

n_id = 0
for name, args in cmds:
    print(f"{name}: {text(tool(name, args))[:140]}", flush=True)

def run_variant(tag, label):
    t = json.loads(text(tool("analyze", {})))
    seis = json.loads(text(tool("get_seismic", {})))
    shutil.copy(MODEL, os.path.join(HERE, f"burj_khalifa_{tag}.pondus"))
    json.dump(seis, open(os.path.join(HERE, f"seismic_{tag}.json"), "w"), indent=1)
    modes = seis.get("modes", [])
    print(f"\n== {label}: seismic weight {seis.get('seismic_weight_kN', seis.get('weight_kN'))} kN", flush=True)
    for md in modes[:8]:
        print("  ", json.dumps(md)[:160], flush=True)
    return seis

res = {}
res["fixed"] = run_variant("fixed", "fixed base, gross sections")
# pictures of the first three modes (fixed base)
for k in (1, 2, 3):
    for c in tool("show_picture", {"view": "3d", "diagram": "mode", "mode": k, "yaw": -40, "pitch": 15, "caption": f"mode {k}"}):
        if c.get("type") == "image":
            open(os.path.join(HERE, f"mode{k}.png"), "wb").write(base64.b64decode(c["data"]))
for c in tool("show_picture", {"view": "3d", "yaw": -40, "pitch": 15}):
    if c.get("type") == "image":
        open(os.path.join(HERE, "model.png"), "wb").write(base64.b64decode(c["data"]))
for c in tool("show_picture", {"view": "plan", "z": LEVELS[1]}):
    if c.get("type") == "image":
        open(os.path.join(HERE, "plan.png"), "wb").write(base64.b64decode(c["data"]))

# the base reactions under D, for the foundation springs
rx = json.loads(text(tool("get_results", {"result": "D", "members": [0], "reactions": True})))["reactions"]
base = [(r["node"], r["F_kN"][2]) for r in rx if abs(r["at"][2]) < 1e-6]
print(f"{len(base)} base nodes carry {sum(f for _, f in base):,.0f} kN", flush=True)

# ACI 318-19 6.6.3.1.1: walls and columns at 0.70 Ig, on the fixed base
tool("add_material", {"kind": "concrete", "name": "C80 0.70Ig", "fc": round(FC * 0.49, 3)})
tool("set_walls", {"all": True, "material": "C80 0.70Ig"})
tool("set_members", {"where": {"kind": "column"}, "material": "C80 0.70Ig"})
res["cracked"] = run_variant("070Ig", "fixed base, walls and columns 0.70 Ig")
tool("set_walls", {"all": True, "material": MAT})
tool("set_members", {"where": {"kind": "column"}, "material": MAT})

# the piled raft as vertical springs, each proportional to its node's gravity reaction so that the
# tower settles uniformly by SETTLE under its weight; the tower rocks on their spread
for factor, tag in ((1.0, "raft_static"), (3.0, "raft_3x")):
    for node, f in base:
        kz = max(f, 1.0) / SETTLE * factor
        tool("add_supports", {"nodes": [node], "type": "spring", "springs": [1e10, 1e10, kz, 1e12, 1e12, 1e12]})
    res[tag] = run_variant(tag, f"piled raft springs x{factor:g} (settlement {SETTLE * 1000 / factor:.1f} mm under the weight)")
json.dump({k: v.get("modes", [])[:10] for k, v in res.items()}, open(os.path.join(HERE, "modes_summary.json"), "w"), indent=1)
proc.stdin.close()
proc.wait(timeout=60)
print(f"\nself-weight of the members {self_w:,.0f} kN, floors {q:.2f} kN/m2 per model level")
