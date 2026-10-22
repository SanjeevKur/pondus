# Calibration against the SAP2000 and ETABS verification manuals: notes

The suites `core/tests/calib_csi_analysis.rs` and `core/tests/calib_csi_design.rs` rebuild the verification problems that Computers and Structures, Inc. (CSI) publishes for SAP2000 and ETABS as Pondus models and design calls, and compare Pondus with the independent value each problem prints. CSI's own SAP2000 or ETABS result is recorded beside it in `calibration/programs.csv` and is not asserted. Each comparison is one row of `calibration/csi.csv`.

```sh
PONDUS_CALIB=$PWD/calibration cargo test --release -p pondus-core --test calib_csi_analysis --test calib_csi_design -- --test-threads 1
```

The run gives 28 tests (20 analysis, 8 design), all passing, and 199 rows over 64 cases: 177 within 0.5 % and 180 within 1 %. Of the rows, 186 compare with a printed independent value, 9 with SAP2000's refined plane-stress mesh (walls) and 4 with closed forms that the examples imply.

## Sources

- **SAP2000:** *SAP2000 Software Verification*, one PDF per problem, downloaded on 22 October 2026 from `docs.csiamerica.com/manuals/sap2000/Verification/Analysis/Frames/Problem 1-0xx.pdf` and `.../Shells/Problem 2-0xx.pdf`. The copies are in `calibration/sources/csi/sap_frames/` (30 frame problems) and `sap_shells/` (20 shell problems).
- **ETABS:** *Software Verification Examples for ETABS 2016* (859 pp.), from `docs.csiamerica.com/manuals/etabs/Software Verification.pdf`, in `calibration/sources/csi/etabs/`.
- **Extracted data:** the geometry, properties, loads, printed results and a feasibility assessment of each problem are in `calibration/sources/csi/specs/*.md`.
- **STAAD.Pro:** Bentley's STAAD.Pro verification manual is available only as unofficial uploads (Scribd, PDFCoffee) and was not used.

## Summary by aspect

| Aspect | Cases | Rows | ≤ 0.5 % | ≤ 1 % | Median \|error\| (%) | Max \|error\| (%) |
|---|---|---|---|---|---|---|
| Frame statics (SAP 1-001, 1-002, 1-005, 1-007, 1-018, 1-022 static, ETABS 1) | 25 | 50 | 50 | 50 | 0.000 | 0.06 |
| Nonlinear members (SAP 1-012) | 3 | 15 | 15 | 15 | 0.000 | 0.00 |
| Foundations (SAP 1-013) | 1 | 2 | 2 | 2 | 0.002 | 0.00 |
| Second order (SAP 1-016) | 2 | 5 | 4 | 4 | 0.001 | 1.75 |
| Stability (SAP 1-019) | 1 | 1 | 1 | 1 | 0.002 | 0.00 |
| Staged construction (SAP 1-027) | 2 | 4 | 4 | 4 | 0.000 | 0.00 |
| Modal (SAP 1-014, 1-021, 1-022, 1-024, 1-025, ETABS 2, 4) | 7 | 36 | 35 | 35 | 0.036 | 1.10 |
| Response spectrum (SAP 1-020, 1-022, 1-024, 1-025) | 6 | 27 | 27 | 27 | 0.068 | 0.26 |
| Shear walls (ETABS 15a, against SAP2000's refined mesh) | 9 | 9 | 0 | 1 | 2.76 | 6.63 |
| AISC 360 flexure (ETABS AISC 360-10 Ex. 001) | 1 | 6 | 6 | 6 | 0.008 | 0.06 |
| ACI 318 beam (ETABS ACI 318-14 Ex. 001) | 1 | 9 | 4 | 5 | 0.64 | 2.37 |
| ACI 318 column (ETABS ACI 318-14 Ex. 002) | 1 | 6 | 6 | 6 | 0.055 | 0.18 |
| ACI 318 wall (ETABS ACI 318-14 Wall-001) | 1 | 6 | 0 | 1 | 2.07 | 2.23 |
| EN 1993-1-1 (ETABS EN 3-2005 Ex. 001-003) | 3 | 20 | 20 | 20 | 0.016 | 0.28 |
| EN 1992-1-1 (ETABS EN 2-2004 Ex. 001) | 1 | 3 | 3 | 3 | 0.034 | 0.06 |

The 19 rows beyond 1 % fall into five groups, none of them a defect of Pondus:

- **Shear walls (9 rows).** The reference is SAP2000's refined plane-stress mesh, and Pondus's wall is a Timoshenko beam (wide column, shear area 5A/6). Pondus is 0.8 % to 3.7 % more flexible on walls of 3 and 6 storeys and 3.3 % to 6.6 % on one-storey squat walls, where plane-stress effects at the fixed base stiffen the mesh. ETABS's own one-panel-per-storey walls lie 1.5 % to 22 % *below* the same reference. Pondus is closer to the reference than ETABS in all nine cases (`programs.csv`).
- **SI constants of ACI 318 (4 rows).** These are φVc, (Av/s)min, As,min and the required Av/s, with the known offsets of 0.17√f'c, 0.35 bw/fyt and 1.4/fy against the psi forms.
- **ACI 318-14 Wall-001 (5 rows).** The hand iteration is not converged. At its c = 30.1 in, equilibrium gives Pn = 1035 kips but moments about the extreme bar give 1026 kips, and the section's own moment there is 2077 kip-ft, not the Pn·e = 2116 the example prints. On the line e = 24.54 in Pondus finds c = 29.47 in, Pn = 1012 kips and a demand/capacity of 1.009. ETABS prints 1.007 against the hand calculation's 1.00, so ETABS and Pondus agree.
- **P-Delta moment at the default mesh (1 row).** See finding F1 below.
- **Bathe–Wilson third eigenvalue (1 row).** The references lump the mass at the joints of one element per member. Pondus's consistent mass is converged: going from one to eight elements per beam moves ω²2 by 0.05 %. The two part increasingly with the mode number: +0.05 %, +0.42 % and +1.10 %.

## Problems, and what is compared

### Frame statics

- **SAP 1-001** (three-member frame, seven load cases): self-weight, span and joint loads, loads on a member's projection and in local axes, trapezoids and a point load. Uz of joint 3 and Ux of joint 2 are exact. The joint moment of case 3 (counter-clockwise in the x-z view) is a negative moment about +y in Pondus, and this sign reproduces the printed equality of cases 2 and 3.
- **SAP 1-002** (temperature): a uniform rise, a rise varying along the length and a gradient through the depth, on a cantilever and a propped cantilever. All six are exact.
  - α is set per material.
  - The rise varying along the length is modelled as two halves at their mean temperatures. That is exact for the free elongation, which depends on the integral of the rise, and for the restrained force, which depends on its mean.
  - The bar is 10 in long, so it cannot be cut into ten pieces: they would fall under Pondus's 1 mm geometric tolerance.
- **SAP 1-005** (support displacements): settlement of a roller (A), the grounded end of a spring displaced (B, applied as the spring force k·δ at the joint), a 0.01 rad rotation of the fixed support (C), and the grounded end of a rotational spring rotated (D, applied as k·θ). All 8 values are exact.
  - The source's figure for D shows only the spring. A joint with no translational restraint reproduces both printed values, whereas a pin gives −426 instead of −473 kip-in.
  - The skewed supports of models E and F are not a Pondus feature.
- **SAP 1-007** (end releases): model C, a moment release, is exact. Shear and axial releases (A, B) are not Pondus features.
- **SAP 1-018** (bending, shear and axial deformation of a portal): all four property sets are exact to 1e-6. Models A and C need frame shear deformation, the new option `design.shear_deformation`.
- **SAP 1-022 / ETABS 7, static:** the seven-storey frame under lateral loads. The roof displacement, column axial force and base moment are exact. The section W24X110 prints A = 2.5 in², which is irrelevant under the diaphragm; 32.5 in² was used.
- **ETABS 1:** moments and shears at the ends and quarter points of a propped beam under point loads and a uniform load, 20 rows, all exact. Pondus's shear sign is fixed once from the first station.

### Nonlinear members, foundations, second order, stability, stages

- **SAP 1-012:** a braced bent, linear (A), with one brace that takes no compression (B) and with the other that takes no tension (C). All 15 values are exact, and the three zero reactions are checked with an absolute tolerance.
- **SAP 1-013:** a beam on an elastic foundation with 64 elements per member. Uz and My at mid-span agree within 0.003 %.
- **SAP 1-016:** tension stiffening by P-Delta. Without tension both values are exact. With the tension (91 % of the Euler load), Uz is exact at the default mesh, but My is 1.76 % high at the default 8 elements per member and within 0.05 % at 128 (finding F1).
- **SAP 1-019:** a pinned portal buckles at 280.19 k, as kl tan kl = 6 gives (Timoshenko & Gere). The property box prints E = 29 900 ksi, but the hand calculation and SAP2000 use 29 000, and so does the test.
- **SAP 1-027:** construction sequence with shear deformation on. Stages 1 and 2 are exact. Stage 3 removes a member, which Pondus's staged analysis cannot do.

### Modal and response spectrum

- **SAP 1-014:** the five bending periods of a cantilever with 32 elements, within 0.08 % of Clough & Penzien.
- **SAP 1-020:** a two-storey frame with lumped masses at its beam mid-spans and SRSS. Both periods and all 10 response values are within 0.22 %.
- **SAP 1-021 / ETABS 6:** Bathe and Wilson's frame, with the three in-plane eigenvalues within 1.1 % (see above).
- **SAP 1-022 / ETABS 7:** the seven periods within 0.03 %, and the El Centro response spectrum (SRSS) within 0.1 %. Pondus's CQC values are recorded beside SAP2000's CQC values.
- **SAP 1-024 / ETABS 8:** a two-storey 3D frame with its storey masses at the centres of mass. T1–T4 are within 0.1 %, and the CQC and SRSS roof displacements within 0.04 %. The torsion constant is not printed; J = 2.5 ft⁴ (columns) and 2.0 ft⁴ (beams) was taken.
- **SAP 1-025 / ETABS 5:** the L-shaped building braced by four X-braced frames, all members axial only, with storey mass and rotational inertia at the centres of mass and two modes. f1 and f2 agree within 0.01 %. The CQC and SRSS displacements, the rotation, the column force and both brace forces agree within 0.26 %.
  - The reference models each frame with its own joints, so the corner column at (0, 80) ft is two columns linked only through the diaphragm. Shared joints made one frame in each direction 40 % stiffer and f2 5.6 % high. Frame 3 is placed 10 mm off its line so that Pondus keeps its joints separate.
  - The brace the manual numbers 4 (ETABS D1) is the bay 1 diagonal rising from (0, 0), and 6 (D3) is its parallel in bay 2.
- **ETABS 2:** three periods and nine mass-normalised mode-shape values, within 0.1 %. The 24 in rigid zone is a short stiff member.
- **ETABS 4:** a one-storey frame with torsional coupling, with the mass and polar inertia of a uniform square at the plan centre. T1–T3 are within 0.18 %.
  - Column torsion is left out, as in the reference's 3-degree-of-freedom system.
  - The modes are picked by participation, because near-massless columns with tiny torsion constants have twisting modes of no mass.

Two modelling traps surfaced in these models. A weightless member still gets a density of 1 kg/m³, so a member made "axially rigid" by an area of 100 m² or more adds noticeable mass (it made ETABS 2 1.5 % long and SAP 1-020 4 % long until the areas were reduced). Beams meant to be rigid need an I at least 1e4 times the columns', because 240 times left a 1.5 % error.

### Shear walls (ETABS 15a)

Nine planar walls 12 in thick, 120, 360 and 720 in long, of 1, 3 and 6 storeys, under 100 k at the top: see the summary above. Examples 15b to 15f (walls on columns, spandrels, C-, E- and flanged walls) need wall sections that Pondus does not have, or loads the manual does not print.

### Design

- **AISC 360-10 Ex. 001** (W18X50, Lb = 5, 11.667 and 35 ft): φbMn agrees within 0.06 % at each Cb printed, and Pondus's whole-span Cb equals 1.136. Mn at the printed Lp and Lr equals Mp and 0.7 Fy Sx. Example 002 (built-up column) uses the Q factor of 360-10, which 360-16 replaced by effective widths, so it is not comparable.
- **ACI 318-14 Ex. 001** (beam): Mu and Vu at d through Pondus's analysis are exact; As and a within 0.03 %; the shear terms carry the SI offsets.
- **ACI 318-14 Ex. 002** (column): Pn, Mn, εt and φ at c = 13.45 in within 0.2 %; the demand/capacity on the line e = 10 in is 0.999 and φMn at Pu 332.3 against 332.0 kip-ft.
- **ACI 318-14 Wall-001:** see above.
- **EN 3-2005 Ex. 001** (IPE 200 beam-column): the section properties, class, χy and the axial ratio agree within 0.06 %.
  - The example's bending ratio uses Annex A (Method 1), and Pondus uses Annex B, so the Annex B interaction is compared with its closed form instead. With Table B.3's Cmy = 0.6 that is 0.873 (finding F2).
- **EN 3-2005 Ex. 002** (406x178x74 UB, V and M):
  - The example takes η = 1.2 for the shear area, and Pondus takes the conservative η = 1.0 that 6.2.6(3) allows, giving Vpl,Rd 664.5 against 689.2 kN.
  - The example prints Av = 4021.2 mm², an arithmetic slip, because its formula gives 4184.4 mm². This does not matter there, because η hw tw = 4341 mm² governs.
  - The 6.2.8 moment reduced for shear is compared with its closed form at Pondus's Vpl,Rd (finding F3).
- **EN 3-2005 Ex. 003** (457x191x98 UB, N + M): class 2, Npl,Rd, Mpl,y,Rd, and MN,y,Rd = 343.2 against 342.2 kN·m, through the new 6.2.9.1 check (finding F4). The header says S275, but fy = 235 MPa is used throughout.
- **EN 2-2004 Ex. 001:** given the example's As and Asw/s, Pondus's EN 1992-1-1 cross-check returns MRd = MEd and VRd,s = VEd within 0.02 %, and VRd,c,min within 0.06 %.

## Findings and changes to Pondus

| No. | Finding | Effect before | Change |
|---|---|---|---|
| F1 | P-Delta member moments are recovered from cubic elements with their geometric end forces, which converges with the square of the element length | SAP 1-016 (N = 0.91 Pe): My +1.76 % at the default 8 elements per member, +0.44 % at 16, 0.05 % at 128 | Not changed; documented. Refine the members carrying large axial force. |
| F2 | EN 1993-1-1 Annex B interaction took Cmy = Cmz = CmLT = 1.0 (the engine's default) instead of Table B.3 | conservative: EN3-001 interaction 1.233 instead of 0.873 (+41 %) | `codes::ec3_cm`: Table B.3 from each combination's moment diagrams (linear: 0.6 + 0.4ψ ≥ 0.4; span loads: the uniform-load column; sway members (k > 1): 0.9) |
| F3 | EN 1993-1-1 6.2.8, the moment resistance reduced for shear when VEd > 0.5 Vpl,Rd, was not applied | **unconservative:** EN3-002 My,Rd 412.8 instead of 380.8 kN·m (+8.4 %) | the moduli are reduced by ρ Aw²/4tw (plastic) and ρ tw hw²/6 (elastic) at every station where it applies |
| F4 | EN 1993-1-1 6.2.9.1, the plastic cross-section check under N + M for Class 1 and 2 I sections, was not made | can be unconservative for short, heavily loaded members where the section check governs | a new check "cross-section N + M (6.2.9.1)", MN,y,Rd, MN,z,Rd and the biaxial criterion (6.41) |
| F5 | no lumped masses: every example with storey masses at a joint or a centre of mass was impossible | – | `Model::masses` (`NodalMass`: t and t·m², global axes), the MCP tool `add_masses`, added to the seismic weight and the modal mass; engine `[Pondus]` change: `solve_modal_3d_with_masses` |
| F6 | only CQC | – | `seismic.combination = "srss"` |
| F7 | no frame shear deformation | SAP 1-018 A and C, 1-027 not reproducible | `design.shear_deformation` (off by default): 5A/6 for rectangles, d·tw and 5/3 bf tf for H shapes, given areas for general sections |
| F8 | a diaphragm model was refused when a node gets its in-plane stiffness only through the diaphragm (a centre-of-mass node), because Pondus factorised the unconstrained K for every model | robustness | the unconstrained factorisation is skipped for diaphragm models, which are solved case by case on the constrained system anyway (also saves its cost) |
| F9 | a node joined only by axial members had free rotations (singular K) in a model with frames, and an all-truss model with diaphragms was numbered by the engine with 3 DOFs per node, so the diaphragm could not act | robustness | such nodes have their rotations held (not the rotation about z on a diaphragm level), and an all-truss model with diaphragms uses frames pinned at both ends |
| F10 | the engine's diaphragm ties a slave's translations but not its rotation about z, so a slave node carrying only a mass had a free rotation and its rotational inertia was not tied to the floor; for the same reason the result depends slightly on which node is the master (0.02 % in a test) | robustness | Pondus makes the node that carries only a mass the master of its level (engine unchanged) |
| F11 | when the sparse shift-invert iteration does not converge, the engine fell back to a dense solver that needs M positive definite, which fails with lumped masses only | robustness | engine `[Pondus]` change: the semidefinite dense solver as the fallback up to 3000 DOFs |

Two Pondus conventions also showed up. `rect_section` rounds a rectangle to whole millimetres, because sections are named "b x h" in mm (3 in = 76.2 mm becomes 76 mm, so I is 1.06 % low), and the tests therefore use general sections for inch-sized bars. The EN 1993-1-1 shear area takes η = 1.0.
