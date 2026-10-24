"""Mat foundations on piles (pile caps and piled rafts), built in Pondus through its MCP server
exactly as an agent would build them with the add_mat tool, then analysed and designed.

Three cases, each a plate panel resting on its supports with no beams:

  A  nine-pile cap: 3 x 3 piles at 1.2 m, cap 3.6 x 3.6 x 0.7 m, one column load of 100 t at
     the centre (the prompt that motivated the feature);
  B  piled raft: 4 x 4 piles at 2.4 m, raft 9 x 9 x 1.2 m, four 500 mm columns at +-3 m each
     carrying 2,500 kN dead and 800 kN live;
  C  the raft of B on soil alone (no piles), a Winkler subgrade of 20,000 kN/m3.

The piles are springs of their axial stiffness EA/L (a pinned pile right under a column load
would take all of it and the cap would not bend), held in plan. Pile loads are the support
reactions, the mat's moments per metre come from get_slab_results, and design gives the bars
of both faces and the punching check at every pile and column (ACI 318-19 22.6).

Run:  python3 build_raft.py   (needs target/release/pondus; writes the .pondus files, the
pictures and results.md in this folder)
"""
import base64, json, os, subprocess

HERE = os.path.dirname(os.path.abspath(__file__))
PONDUS = os.path.join(HERE, "..", "..", "target", "release", "pondus")

CASES = [
    {
        "key": "nine_pile_cap",
        "title": "A. Nine-pile cap, 3 x 3 at 1.2 m, 0.7 m thick, 100 t at the centre",
        "mat": {"pile_grid": {"nx": 3, "ny": 3, "sx": 1.2, "sy": 1.2, "edge": 0.6}, "h": 0.7, "pile_k": 100_000, "pile_size": 0.3, "name": "Pile cap"},
        "loads": [("D", [[0.0, 0.0], 981.0])],
    },
    {
        "key": "piled_raft",
        "title": "B. Piled raft, 4 x 4 at 2.4 m, 9 x 9 x 1.2 m, four columns of 2,500 kN D + 800 kN L",
        "mat": {"pile_grid": {"nx": 4, "ny": 4, "sx": 2.4, "sy": 2.4, "edge": 0.9}, "h": 1.2, "pile_k": 150_000, "pile_size": 0.5, "name": "Piled raft"},
        "loads": [("D", [[x, y], 2500.0]) for x in (-3.0, 3.0) for y in (-3.0, 3.0)] + [("L", [[x, y], 800.0]) for x in (-3.0, 3.0) for y in (-3.0, 3.0)],
    },
    {
        "key": "raft_on_soil",
        "title": "C. The same raft on soil alone, subgrade 20,000 kN/m3, no piles",
        "mat": {"x0": -4.5, "y0": -4.5, "x1": 4.5, "y1": 4.5, "h": 1.2, "subgrade": 20_000, "pile_size": 0.5, "name": "Raft on soil"},
        "loads": [("D", [[x, y], 2500.0]) for x in (-3.0, 3.0) for y in (-3.0, 3.0)] + [("L", [[x, y], 800.0]) for x in (-3.0, 3.0) for y in (-3.0, 3.0)],
    },
]


class Server:
    def __init__(self, model_path):
        self.p = subprocess.Popen([PONDUS, "--mcp", "--autosave", model_path], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.n = 0
        self.rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "build_raft", "version": "1"}})

    def rpc(self, method, params):
        self.n += 1
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": self.n, "method": method, "params": params}) + "\n")
        self.p.stdin.flush()
        while True:
            line = self.p.stdout.readline()
            if not line:
                raise SystemExit("pondus stopped: " + self.p.stderr.read()[-800:])
            try:
                m = json.loads(line)
            except json.JSONDecodeError:
                continue
            if m.get("id") == self.n:
                if "error" in m:
                    raise SystemExit(f"{method}: {m['error']}")
                return m["result"]

    def call(self, name, args):
        c = self.rpc("tools/call", {"name": name, "arguments": args})["content"]
        text = next((x["text"] for x in c if x["type"] == "text"), "")
        images = [x["data"] for x in c if x["type"] == "image"]
        try:
            return json.loads(text), images
        except json.JSONDecodeError:
            return text, images

    def close(self):
        self.p.stdin.close()
        self.p.wait(timeout=30)


def run_case(c):
    model = os.path.join(HERE, c["key"] + ".pondus")
    s = Server(model)
    s.call("new_model", {"name": c["title"]})
    mat, _ = s.call("add_mat", c["mat"])
    for case, (at, f) in c["loads"]:
        s.call("add_loads", {"case": case, "loads": [{"type": "nodal", "at": [at[0], at[1], 0.0], "f": [0, 0, -f, 0, 0, 0]}]})
    an, _ = s.call("analyze", {})
    res, _ = s.call("get_results", {"reactions": True, "result": "SLS D + L"})
    slab, _ = s.call("get_slab_results", {"result": "SLS D + L"})
    des, _ = s.call("design", {})
    sd = des["slabs"][0]
    pics = [("model", {"view": "3d", "diagram": "none", "yaw": 35, "pitch": 30, "show_loads": True}),
            ("mx", {"view": "plan", "z": 0.0, "diagram": "platemx", "result": "ULS 1.2D + 1.6L" if any(k == "L" for k, _ in c["loads"]) else "ULS 1.4D", "values": True}),
            ("qx", {"view": "plan", "z": 0.0, "diagram": "plateqx"}),
            ("utilisation", {"view": "3d", "diagram": "utilisation", "yaw": 35, "pitch": 30})]
    for name, args in pics:
        _, imgs = s.call("show_picture", args)
        if imgs:
            with open(os.path.join(HERE, f"{c['key']}_{name}.png"), "wb") as f:
                f.write(base64.b64decode(imgs[0]))
    s.call("write_report", {"path": os.path.join(HERE, c["key"] + "-report.pdf"), "project": c["title"], "engineer": "Pondus example"})
    s.close()
    return mat, an, res, slab, sd


def main():
    out = ["# Results (generated by build_raft.py)\n"]
    for c in CASES:
        mat, an, res, slab, sd = run_case(c)
        out.append(f"## {c['title']}\n")
        d = next(x for x in an["cases"] if x["case"] == "D")
        out.append(f"Plan {mat['plan']} m, h {mat['h']} m, mesh {mat['slab_mesh']} m, {slab['slabs'][0]['elements']} elements; "
                   f"case D applied {d['applied_kN'][2]:.1f} kN, reactions {d['reactions_kN'][2]:.1f} kN ({'balanced' if d['balanced'] else 'NOT balanced'}).\n")
        piles = set(mat.get("piles", []))
        reactions = [(r["at"][0], r["at"][1], r["F_kN"][2]) for r in res.get("reactions", []) if r.get("node") in piles]
        soil = next((r["F_kN"][2] for r in res.get("reactions", []) if r.get("node") == 0), None)
        if reactions:
            vals = sorted(set(round(v, 1) for _, _, v in reactions))
            out.append("Pile loads under D + L (service), kN:\n")
            out.append("| Pile at (x, y) m | Load (kN) |\n|---|---:|")
            for x, y, v in sorted(reactions):
                out.append(f"| ({x:+.1f}, {y:+.1f}) | {v:.1f} |")
            out.append(f"\nLargest {max(vals):.1f} kN, least {min(vals):.1f} kN, mean {sum(v for *_, v in reactions) / len(reactions):.1f} kN.\n")
        if soil is not None:
            out.append(f"Soil reaction (Winkler springs, node 0): {soil:.1f} kN; settlement up to {slab['slabs'][0]['max_deflection_mm']:.2f} mm.\n")
        sr = slab["slabs"][0]
        out.append(f"Plate moments under D + L: mx {sr['mx [value, x, y]']['min'][0]:.1f} to {sr['mx [value, x, y]']['max'][0]:.1f} kN·m/m, "
                   f"my {sr['my']['min'][0]:.1f} to {sr['my']['max'][0]:.1f} kN·m/m, deflection {sr['max_deflection_mm']:.2f} mm.\n")
        out.append(f"Design: {sd['method']}; ratio {sd['ratio']:.2f}, governed by {sd['governs']}.\n")
        out.append("| Face | Bars |\n|---|---|")
        for b in sd["bars"]:
            out.append(f"| {b.split(':')[0]} | {b.split(':', 1)[1].strip()} |")
        punch = [n for n in sd["notes"] if n.startswith("punching at")]
        if punch:
            worst = max(punch, key=lambda n: float(n.rsplit("ratio", 1)[1].split()[0]))
            out.append(f"\nPunching checked at {len(punch)} points; the worst: {worst}\n")
        out.append(f"![model]({c['key']}_model.png)\n![mx]({c['key']}_mx.png)\n")
    with open(os.path.join(HERE, "results.md"), "w") as f:
        f.write("\n".join(out))
    print("\n".join(out))


if __name__ == "__main__":
    main()
