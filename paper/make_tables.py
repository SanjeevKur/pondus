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
    ax.scatter(e, [i] * len(e), s=10, alpha=0.6, color={"steel": "#2663d2", "concrete": "#c47a12", "analysis": "#2a8a4a"}.get(s, "k"))
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
