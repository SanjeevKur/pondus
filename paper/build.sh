#!/bin/sh
# Builds paper.pdf: tables and figures from the calibration CSVs, then pandoc → LaTeX → tectonic.
set -e
cd "$(dirname "$0")"
python3 make_tables.py
FONTS="$(cd ../app/assets/fonts && pwd)/"
pandoc paper_full.md --from markdown+pipe_tables+tex_math_dollars+raw_tex --citeproc --bibliography refs.bib -s \
  --pdf-engine=xelatex -V documentclass=article -V papersize=a4 \
  -V mainfont=Inter-Regular.ttf -V mainfontoptions="Path=$FONTS,BoldFont=Inter-SemiBold.ttf,ItalicFont=Inter-Regular.ttf,ItalicFeatures={FakeSlant=0.2}" \
  -V monofont=JetBrainsMono-Regular.ttf -V monofontoptions="Path=$FONTS,Scale=0.85" \
  -V colorlinks=true -V linkcolor=blue -V urlcolor=blue -V citecolor=black \
  -o paper.tex
tectonic -X compile paper.tex --keep-logs >build.log 2>&1 || { tail -40 build.log; exit 1; }
echo "built paper.pdf"
