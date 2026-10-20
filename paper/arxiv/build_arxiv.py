"""Builds the arXiv version of the paper: paper_full.md -> arxiv/main.tex (pure ASCII, so that
arXiv's pdfLaTeX compiles it), main.pdf with tectonic, and arxiv/submission.tar.gz holding
main.tex, main.bbl, refs.bib, arxiv.sty and the figure. arXiv compiles with pdfLaTeX and uses the
.bbl as given when one is present; refs.bib is included so that the package also builds standalone."""
import os, re, shutil, subprocess, sys, tarfile, unicodedata

HERE = os.path.dirname(os.path.abspath(__file__))
PAPER = os.path.dirname(HERE)
BUILD = os.path.join(HERE, "build")

# ------------------------------------------------------------------ markdown preparation
src = open(os.path.join(PAPER, "paper_full.md")).read()
m = re.match(r"---\n(.*?)\n---\n(.*)", src, re.S)
assert m, "paper_full.md has no YAML front matter"
front, body = m.groups()

# author block: plain fields for the template instead of the raw-LaTeX author of paper.md
front = re.sub(r"author:\n(  - .*\n)+", "", front + "\n")
AUTHORS = (r"\textbf{Sompote Youwai}\\ AI Research Group, Department of Civil Engineering\\"
           r" King Mongkut's University of Technology Thonburi, Bangkok, Thailand\\ \texttt{sompote.you@kmutt.ac.th}")
front += ("author: 'Sompote Youwai'\n"
          "shorttitle: 'Calibrating an agentic structural program'\n")
front = re.sub(r"^(geometry|fontsize|linestretch):.*\n", "", front, flags=re.M)

# references where natbib puts them; the appendix as a LaTeX appendix (A, A.1, ...)
body = re.sub(r"# References\s*\n+::: \{#refs\}\s*\n:::\s*\n", "\\\\bibliography{refs}\n", body)
assert "\\bibliography{refs}" in body, "references block not found"
body = body.replace("# Appendix A. All comparisons", "\\appendix\n\n# All comparisons")
body = re.sub(r"^## A\.\d+ ", "## ", body, flags=re.M)

md = os.path.join(BUILD, "main.md")
os.makedirs(os.path.join(BUILD, "figures"), exist_ok=True)
open(md, "w").write("---\n" + front.strip() + "\n---\n" + body)

# ------------------------------------------------------------------ pandoc
tex = os.path.join(BUILD, "main.tex")
subprocess.run(["pandoc", md, "--from", "markdown+pipe_tables+tex_math_dollars+raw_tex", "--natbib",
                "--template", os.path.join(HERE, "template.latex"), "-V", "authorblock=" + AUTHORS, "-t", "latex", "-o", tex], check=True)

# ------------------------------------------------------------------ to ASCII
M = lambda s: "\\ensuremath{" + s + "}"
MATH = {  # characters that are symbols, usable in text and in math
    "−": "-", "×": "\\times", "·": "\\cdot", "≤": "\\le", "≥": "\\ge", "≈": "\\approx", "±": "\\pm",
    "√": "\\surd", "∫": "\\int", "∝": "\\propto", "→": "\\rightarrow", "°": "^{\\circ}",
    "²": "^{2}", "³": "^{3}", "⁴": "^{4}", "⁶": "^{6}", "⁻": "^{-}", "¹": "^{1}",
    "α": "\\alpha", "β": "\\beta", "γ": "\\gamma", "δ": "\\delta", "ε": "\\varepsilon", "η": "\\eta",
    "θ": "\\theta", "κ": "\\kappa", "λ": "\\lambda", "μ": "\\mu", "ν": "\\nu", "ξ": "\\xi", "π": "\\pi",
    "ρ": "\\rho", "σ": "\\sigma", "τ": "\\tau", "φ": "\\phi", "χ": "\\chi", "ψ": "\\psi", "ω": "\\omega",
    "Δ": "\\Delta", "Σ": "\\Sigma", "Φ": "\\Phi", "Ω": "\\Omega", "Γ": "\\Gamma", "Λ": "\\Lambda",
}
TEXT = {"–": "--", "—": "---", "‘": "`", "’": "'", "“": "``", "”": "''", "§": "\\S{}", "é": "\\'e",
        "è": "\\`e", "ü": '\\"u', "ö": '\\"o', "ä": '\\"a', "ñ": "\\~n", "\u00a0": "~", "\u2009": "\\,",
        "\u202f": "\\,", "…": "\\ldots{}", "º": "\\textordmasculine{}", "µ": M("\\mu")}

t = open(tex).read()
# a letter with a combining macron (x̄, λ̄): \bar of the letter
t = re.sub("(.)\u0304", lambda m: M("\\bar{" + MATH.get(m.group(1), m.group(1)) + "}"), t)
t = "".join(M(MATH[c]) if c in MATH else TEXT.get(c, c) for c in t)
left = sorted({c for c in t if ord(c) > 127})
if left:
    sys.exit("non-ASCII characters left: " + ", ".join(f"{c!r} {unicodedata.name(c, '?')}" for c in left))
open(tex, "w").write(t)

# ------------------------------------------------------------------ compile
for f in ["arxiv.sty"]:
    shutil.copy(os.path.join(HERE, f), BUILD)
shutil.copy(os.path.join(PAPER, "refs.bib"), BUILD)
FIGS = sorted(f for f in os.listdir(os.path.join(PAPER, "figures")) if f.endswith(".pdf"))
for f in FIGS:
    shutil.copy(os.path.join(PAPER, "figures", f), os.path.join(BUILD, "figures"))
r = subprocess.run(["tectonic", "-X", "compile", "main.tex", "--keep-intermediates", "--keep-logs"], cwd=BUILD, capture_output=True, text=True)
if r.returncode != 0:
    print(r.stdout[-3000:], r.stderr[-3000:])
    sys.exit("tectonic failed")
log = open(os.path.join(BUILD, "main.log"), errors="replace").read()
undef = sorted(set(re.findall(r"Citation `([^']+)' .*undefined", log)) | set(re.findall(r"Reference `([^']+)' .*undefined", log)))
if undef:
    sys.exit("undefined citations or references: " + ", ".join(undef))

# ------------------------------------------------------------------ outputs
shutil.copy(os.path.join(BUILD, "main.pdf"), os.path.join(PAPER, "paper.pdf"))
tar = os.path.join(HERE, "submission.tar.gz")
with tarfile.open(tar, "w:gz") as z:
    for f in ["main.tex", "main.bbl", "refs.bib", "arxiv.sty"] + [f"figures/{g}" for g in FIGS]:
        z.add(os.path.join(BUILD, f), arcname=f)
print(f"built paper.pdf (arXiv format) and {os.path.relpath(tar, PAPER)}")
