"""Builds the paper's tables and figures from calibration/*.csv (one row per comparison)."""
import glob, os, sys
import pandas as pd
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

HERE = os.path.dirname(os.path.abspath(__file__))
CAL = os.path.join(HERE, "..", "calibration")
OUT = os.path.join(HERE, "generated")
FIG = os.path.join(HERE, "figures")
os.makedirs(OUT, exist_ok=True)
os.makedirs(FIG, exist_ok=True)

frames = []
for f in sorted(glob.glob(os.path.join(CAL, "*.csv"))):
    d = pd.read_csv(f)
    d["suite"] = os.path.splitext(os.path.basename(f))[0]
    frames.append(d)
if not frames:
    sys.exit("no calibration CSVs")
df = pd.concat(frames, ignore_index=True)
df["abs_err"] = df["error_pct"].abs()
df["pass"] = df["abs_err"] <= df["tol_pct"] + 1e-12

def fmt(x):
    if pd.isna(x):
        return ""
    a = abs(x)
    if a == 0:
        return "0"
    if a >= 1e5 or a < 1e-3:
        return f"{x:.4g}"
    return f"{x:.4g}"

# summary by suite and aspect
summ = df.groupby(["suite", "aspect"]).agg(rows=("case", "size"), cases=("case", "nunique"), max_err=("abs_err", "max"), median_err=("abs_err", "median"), passed=("pass", "sum")).reset_index()
with open(os.path.join(OUT, "summary.md"), "w") as f:
    f.write("| Suite | Aspect | Cases | Comparisons | Passed | Median abs. error (%) | Max abs. error (%) |\n|" + "-" * 8 + "|" + "-" * 30 + "|" + "-" * 6 + ":|" + "-" * 10 + ":|" + "-" * 6 + ":|" + "-" * 10 + ":|" + "-" * 10 + ":|\n")
    for _, r in summ.iterrows():
        f.write(f"| {r.suite} | {r.aspect} | {r.cases} | {r.rows} | {int(r.passed)} | {r.median_err:.3f} | {r.max_err:.3f} |\n")
    t = df
    f.write(f"| **All** | | **{t.case.nunique()}** | **{len(t)}** | **{int(t['pass'].sum())}** | **{t.abs_err.median():.3f}** | **{t.abs_err.max():.3f}** |\n")

# one detailed table per suite
for s, d in df.groupby("suite"):
    with open(os.path.join(OUT, f"detail_{s}.md"), "w") as f:
        widths = [10, 18, 34, 22, 8, 8, 7, 8]
        f.write("| Aspect | Case | Source | Quantity (unit) | Reference | Pondus | Error (%) | Kind |\n|" + "|".join("-" * w + (":" if k in (4, 5, 6) else "") for k, w in enumerate(widths)) + "|\n")
        for _, r in d.iterrows():
            u = "" if str(r.unit) in ("-", "–", "nan") else f" ({r.unit})"
            f.write(f"| {r.aspect} | {r.case} | {r.source} | {r.quantity}{u} | {fmt(r.reference)} | {fmt(r.pondus)} | {r.error_pct:+.3f} | {r.kind} |\n")

# figure: error distribution by aspect
asp = summ.sort_values(["suite", "aspect"])
labels = [f"{a}" for a in asp.aspect]
fig, ax = plt.subplots(figsize=(7.0, 0.28 * len(labels) + 1.2))
for i, (s, a) in enumerate(zip(asp.suite, asp.aspect)):
    e = df[(df.suite == s) & (df.aspect == a)].abs_err.clip(lower=1e-6)
    ax.scatter(e, [i] * len(e), s=10, alpha=0.6, color={"steel": "#2663d2", "concrete": "#c47a12", "analysis": "#2a8a4a", "walls": "#8a3fa8", "bridge": "#b8860b"}.get(s, "k"))
ax.set_yticks(range(len(labels)))
ax.set_yticklabels(labels, fontsize=7)
ax.set_xscale("log")
ax.set_xlabel("absolute relative error (%)")
ax.axvline(0.5, color="grey", lw=0.8, ls="--")
ax.axvline(1.0, color="grey", lw=0.8, ls=":")
ax.invert_yaxis()
fig.tight_layout()
fig.savefig(os.path.join(FIG, "errors.pdf"))
fig.savefig(os.path.join(FIG, "errors.png"), dpi=200)
print(f"{len(df)} comparisons, {df.case.nunique()} cases, {int(df['pass'].sum())} passed, max {df.abs_err.max():.3f} %")

# validation against measured buildings (calibration/validation/*.csv): not counted in the
# calibration totals, one compact table per building
for f in sorted(glob.glob(os.path.join(CAL, "validation", "*.csv"))):
    v = pd.read_csv(f)
    if "kind" not in v.columns:
        continue
    name = os.path.splitext(os.path.basename(f))[0]
    v = v[v.kind != "input"]
    with open(os.path.join(OUT, f"validation_{name}.md"), "w") as out:
        out.write("| Pondus model | Quantity | Reference | Value | Pondus | Diff. (%) |\n|" + "|".join(["-" * 22, "-" * 34, "-" * 24, "-" * 6 + ":", "-" * 6 + ":", "-" * 6 + ":"]) + "|\n")
        last = None
        for _, r in v.iterrows():
            u = "" if str(r.unit) in ("-", "–", "nan") else f" ({r.unit})"
            model = r.case.replace("web wall, ", "").replace("sqrt(", "√(") if r.case != last else ""
            last = r.case
            src = r.source.split(",")[0]
            out.write(f"| {model} | {r.quantity}{u} | {src}, {r.kind} | {fmt(r.reference)} | {fmt(r.pondus)} | {r.error_pct:+.1f} |\n")

# figure: the UCSD-NEES building slice (Pondus's picture, its mode shapes, measured periods)
img_p = os.path.join(CAL, "validation", "ucsd7_model.png")
modes_p = os.path.join(CAL, "validation", "ucsd7_modes.csv")
val_p = os.path.join(CAL, "validation", "ucsd7.csv")
if all(os.path.exists(q) for q in (img_p, modes_p, val_p)):
    import numpy as np
    from matplotlib import image as mpimg
    im = mpimg.imread(img_p)[..., :3]
    bg = im[0, 0]
    mask = np.abs(im - bg).sum(axis=2) > 0.05
    ys, xs = np.where(mask)
    sl = (slice(max(ys.min() - 10, 0), ys.max() + 10), slice(max(xs.min() - 40, 0), xs.max() + 40))
    im = 1.0 - im[sl]
    im[~mask[sl]] = 1.0
    md = pd.read_csv(modes_p)
    v = pd.read_csv(val_p)
    def pondus(case_start, qty):
        r = v[v.case.str.startswith(case_start) & v.quantity.str.startswith(qty)]
        return float(r.pondus.iloc[0])
    fig = plt.figure(figsize=(7.0, 3.6))
    gs = fig.add_gridspec(1, 3, width_ratios=[0.55, 1.0, 2.1], wspace=0.35)
    a0 = fig.add_subplot(gs[0]); a0.imshow(im); a0.axis("off"); a0.set_title("(a) Pondus model", fontsize=8)
    a1 = fig.add_subplot(gs[1])
    for col, lab, c in (("mode1_ux", f"mode 1, T = {md.T1[0]:.3f} s", "#2663d2"), ("mode2_ux", f"mode 2, T = {md.T2[0]:.3f} s", "#c47a12")):
        u = md[col] / md[col].abs().max()
        if u.iloc[-1] < 0: u = -u
        a1.plot(u, md.z, "-o", ms=3, lw=1.2, color=c, label=lab)
    a1.axvline(0, color="grey", lw=0.6)
    a1.set_xlabel("normalised $u_x$", fontsize=7); a1.set_ylabel("height (m)", fontsize=7)
    a1.tick_params(labelsize=6.5); a1.legend(fontsize=5.8, loc="upper center", bbox_to_anchor=(0.5, -0.16), frameon=False)
    a1.set_title("(b) in-plane modes", fontsize=8)
    a2 = fig.add_subplot(gs[2])
    states = ["ambient,\nbefore tests", "first\nWN", "before\nEQ1", "after\nEQ1", "after\nEQ2", "after\nEQ3", "after\nEQ4"]
    meas = [1 / 2.23, 0.51, 0.59, 0.65, 0.82, 0.88, 1.16]
    a2.plot(range(len(meas)), meas, "ks-", ms=4, lw=1.0, label="measured (Moaveni et al. 2011; Panagiotou et al. 2010)")
    lines = [("web wall, gross section, E = 4700 sqrt(41.4)", "Pondus, gross section", "#2663d2", "-"),
             ("web wall, gross section, E = 4700 sqrt(27.6)", "Pondus, gross, specified $f'_c$", "#2663d2", ":"),
             ("web wall, 0.70 Ig", "Pondus, $0.70 I_g$ (ACI uncracked)", "#2a8a4a", "--"),
             ("web wall, 0.35 Ig", "Pondus, $0.35 I_g$ (ACI cracked)", "#c0392b", "--")]
    for key, lab, c, ls in lines:
        t = pondus(key, "T1, first")
        a2.axhline(t, color=c, ls=ls, lw=1.0, label=f"{lab}: {t:.3f} s")
    a2.plot([1], [0.50], "v", color="#8a3fa8", ms=5, label="designers' uncracked 3-D model: 0.50 s")
    a2.plot([1], [0.557], "^", color="#8a3fa8", ms=5, mfc="none", label="blind-prediction model (Kelly 2007): 0.557 s")
    a2.set_xticks(range(len(states))); a2.set_xticklabels(states, fontsize=5.8)
    a2.set_ylabel("first period $T_1$ (s)", fontsize=7); a2.tick_params(axis="y", labelsize=6.5)
    a2.set_ylim(0.3, 1.25); a2.legend(fontsize=5.3, loc="upper left", frameon=False)
    a2.set_title("(c) first period through the test sequence", fontsize=8)
    fig.savefig(os.path.join(FIG, "ucsd7.pdf"), bbox_inches="tight")
    fig.savefig(os.path.join(FIG, "ucsd7.png"), dpi=200, bbox_inches="tight")
    plt.close(fig)

# figure: the FHWA two-span bridge, Pondus against Table 6 of the example
bp = os.path.join(CAL, "bridge.csv")
if os.path.exists(bp):
    b = pd.read_csv(bp)
    def series(prefix):
        r = b[b.quantity.str.startswith(prefix)].copy()
        r["x"] = r.quantity.str.extract(r"at ([0-9.]+)L").astype(float)
        return r.sort_values("x")
    fig, ax = plt.subplots(figsize=(7.0, 3.3))
    for prefix, lab, c in (("vehicle M+", "vehicle, positive", "#2663d2"), ("1.33 vehicle + lane M+", "1.33 vehicle + lane, positive", "#2a8a4a"),
                           ("vehicle M-", "vehicle, negative", "#7a9fe0"), ("standard M-", "standard negative (1.33 vehicle + lane)", "#c47a12"),
                           ("special M-", "special negative (90 % two trucks + lane)", "#c0392b")):
        r = series(prefix)
        xs = [0.0] + list(r.x) if prefix.endswith("M+") else list(r.x)
        ys = [0.0] + list(r.pondus) if prefix.endswith("M+") else list(r.pondus)
        if prefix.endswith("M+"):
            xs.append(1.0); ys.append(0.0)
        ax.plot(xs, ys, "-", color=c, lw=1.2, label=f"Pondus: {lab}")
        ax.plot(r.x, r.reference, "o", mfc="none", color=c, ms=5)
    ax.plot([], [], "o", mfc="none", color="k", label="FHWA Design Example 2A, Table 6")
    ax.axhline(0, color="grey", lw=0.6)
    ax.set_xlabel("position in span 1, $x/L$ (pier at 1.0)", fontsize=7.5)
    ax.set_ylabel("moment per lane (kip-ft)", fontsize=7.5)
    ax.tick_params(labelsize=7)
    ax.legend(fontsize=6, loc="lower left", frameon=False, ncol=2)
    ax.set_xlim(0, 1.0)
    fig.tight_layout()
    fig.savefig(os.path.join(FIG, "bridge.pdf"))
    fig.savefig(os.path.join(FIG, "bridge.png"), dpi=200)
    plt.close(fig)

# the paper with its includes expanded: <!--include:path--> lines
import re
src = open(os.path.join(HERE, "paper.md")).read()
def inc(m):
    path = m.group(1)
    body = open(os.path.join(HERE, path)).read()
    if "detail_" in path:
        return "\\begingroup\\scriptsize\n\n" + body + "\n\\endgroup\n"
    return body
full = re.sub(r"<!--include:([^>]+)-->", inc, src).replace("∝", "$\\propto$")
open(os.path.join(HERE, "paper_full.md"), "w").write(full)
