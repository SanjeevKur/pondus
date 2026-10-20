# Shear walls under earthquake and dynamic loading: calibration notes

Suite `core/tests/calib_walls.rs`, written to `walls.csv` when `PONDUS_CALIB` names a directory.
Every case is built with the `add_walls` command, so the wide-column wall (a pier on the centre
line with the wall section t × lw and shear area 5/6 A, rigid arms to the wall ends at every
level) is what is tested, together with the seismic mass model, the modal and CQC analysis and
the time-history solver.

## Cases and references

| Case | Reference | Kind | Tolerance |
|---|---|---|---|
| Cantilever wall 0.3 × 2 m, H 60 m, 20 storeys, mass its own weight | Clough & Penzien 3e Table 18-1, βnL = 1.875104, 4.694091, 7.854757 | published | 0.3 / 0.6 / 1.2 % |
| Same, effective mass ratios of modes 1–3 | (∫mφ)²/∫mφ² of the flexural cantilever, Chopra 5e Ch. 17 | closed-form | 0.5 / 1 / 2 % |
| Wall + two shear frames, αH = 0.5, 1, 3, 8, 40 storeys | flexural–shear continuum (Heidebrecht & Stafford Smith 1973; Miranda & Taghavi 2005), frequency equation solved in the test | independent | 1.5 % (f), 1 % (mass) |
| Same, αH = 8 at 10, 20, 40 storeys | convergence to the continuum | independent | 3 % |
| One storey, 10 × 10 m floor, wall on one edge, four corner cantilever columns | 3 × 3 rigid-floor eigenproblem (Chopra 5e Ch. 9), Jacobi in the test | closed-form | 0.5 % (f), 1 % (mass) |
| Same, CQC base shear in y, flat Sa 0.4 g, ξ 5 % | Der Kiureghian (1981) | closed-form | 0.5 % |
| Cantilever wall, undamped step 0.1 g | modal superposition of 6 flexural modes (Chopra 5e Ch. 4, 13) | closed-form | 1.5 % |

The continuum solver was checked against its two limits before use: αH = 0 gives the flexural
cantilever (β1 = 1.875104, effective mass 0.6131) and αH = 200 the shear cantilever (λ² → πα/2,
effective mass 8/π²). An independent beam finite-element eigen solve with storey shear springs
(Python, 200 elements) reproduced λ² at αH = 0.3, 1, 3 and 8 to four digits.

## Systematic offsets and their causes

- The continuum and the βnL table are Euler–Bernoulli; Pondus's piers deform in shear as well.
  A Timoshenko version of the independent solve (κGA H²/EI = 3750 for this wall) predicts
  −0.062, −0.43 and −1.01 % in f1–f3 of the cantilever, against −0.077, −0.44 and −1.03 %
  observed, and −0.14, −0.31 and −0.69 % at αH = 8.
- Weightless members (the frames of the benchmark, the rigid arms) get Pondus's minimum density
  of 1 kg/m³, about 0.2 % of the wall's mass in the wall-frame models, which lowers f1 by about
  0.1 %. This accounts for most of the remaining −0.1 to −0.2 % in f1.
- The convergence rows (−0.61, −0.35, −0.33 % at 10, 20, 40 storeys) reach that floor.

## Defect found: rigid arms of absolute stiffness

Before the fix the arms had E = 2.5e7 MPa on a General section with A = I = J = 1 m⁴, the same
for every wall. With a slender 40-storey wall coupled to soft frames this put a stiffness
contrast of order 10⁹ into K on top of the cantilever's own conditioning, and the result lost
precision silently (no warning, global equilibrium still satisfied):

| αH, 40 storeys | f1 before (Hz) | continuum | static top deflection before (PH³/EI) | discrete reference |
|---|---|---|---|---|
| 0.3 | 0.29259 (+4.5 %) | 0.27993 | | |
| 0.5 | 0.32103 (+11.5 %) | 0.28789 | 0.23888 (−21 %) | 0.30307 |
| 0.7 | 0.29871 (−0.2 %) | 0.29931 | 0.27906 (+0.09 %) | 0.27882 |

The same models at 10 and 20 storeys were right, and so were the walls without the diaphragm,
which is what made the error look like a numerical artefact and not a modelling one. Softening
the arm modulus to 2.5e5 MPa removed it (0.30332 at αH 0.5), which confirmed the cause.

Fix (`core/src/walls.rs`): the arms take the wall's own section (A = t·lw, I = J = t·lw³/12)
and a modulus `ARM_FACTOR` = 1000 times the wall material's, so they stay rigid against their
own wall without an absolute stiffness; `sync` rebuilds the arms of models saved earlier and
removes the arm sections and materials no member uses. After the fix: f1 at αH 0.5 is −0.18 %
and the static deflection +0.08 % (the wall's shear deformation).

Two benchmark-model errors were corrected in the tests, not in Pondus: frame beams 10⁹ times
stiffer than their columns (moderated to 10⁴, which removed spurious zero-mass modes), and
the one-storey floor beams, first hinged at both ends (a mechanism with the corner columns
hinged at the top), then too flexible axially and in plan for the 47 Hz torsional mode,
because only the model's nodes join the diaphragm (A raised to 100 m², I to 2000 m⁴; the
static floor stiffness matched the 3 × 3 reference to six digits throughout).
