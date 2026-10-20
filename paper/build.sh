#!/bin/sh
# Builds paper.pdf in arXiv format: tables and figures from the calibration CSVs, then
# pandoc -> pure-ASCII LaTeX (arxiv/template.latex, arxiv/arxiv.sty, natbib) -> tectonic.
# Also writes arxiv/submission.tar.gz (main.tex, main.bbl, arxiv.sty, figures) for arXiv.
set -e
cd "$(dirname "$0")"
python3 make_tables.py
python3 arxiv/build_arxiv.py
