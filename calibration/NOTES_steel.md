# Calibration of steel and Eurocode design

Suite `core/tests/calib_steel.rs`, results `calibration/steel.csv` (one row per compared quantity).
Run with `PONDUS_CALIB=$PWD/calibration cargo test --release -p pondus-core --test calib_steel -- --test-threads 1`
after removing the old CSV.

Every comparison goes through Pondus's own code: `steel::capacities`, `steel::cb`, `steel::check` (AISC 360-16),
`codes::ec3_member`, `codes::ec3_c1`, `codes::ec3_ltb`, `codes::ec3_class_nm` and the engine's EN 1993-1-1
checker as Pondus calls it, `codes::check_ec2_beam` (EN 1992-1-1 cross-check) and `connections::shear_tab`.
The published inputs and printed answers are those recorded, with page numbers, in Stabileo's
`engine/tests/design_worked_examples.rs` and `docs/verification/DESIGN_EXAMPLES.md`. US examples were converted to SI
(1 in = 25.4 mm, 1 ksi = 6.894757 MPa, 1 kip = 4.448222 kN), and the results were converted back to the example's
units. The tolerance is 0.5 % (half a unit in the third printed figure). It is wider only where the source prints
fewer figures, and each such case says so.

## Result

All 68 rows pass: 62 compare against printed values and 6 against closed-form checks. The largest deviation is
−0.60 %, for AISC H.3, where Mn at the printed Lr was compared with a 1 % band. The Manual's rts, ho and Cw are not
mutually exact, so Iy·ho²/4 differs from the tabulated Cw.

| Code | Examples | Quantities |
|---|---|---|
| AISC 360 tension | D.1 W8×21 | φtPn (D2-1 yielding) |
| AISC 360 compression | E.1C W14×132, E.1D W14×90, INTI301 Ej. 10(a) IPE 300 | φcPn, φcFn / Fn, Fcr (E3-2, E3-3), Pn |
| AISC 360 flexure | F.1-1B, F.1-2B, F.1-3B W18×50; F.3B W21×48; F.5 W12×58; H.3 W14×82 | Mp, Cb (F1-1), Lp, Lr, Mn (F2-2, F2-3, F3-1, F6-1), Fcr, φbMn |
| AISC 360 shear | G.1B W24×62 | φvVn (G2-1, φv = 1.0) |
| AISC 360 interaction | H.3 W14×82 through `steel::check` | Pr/Pc, H1-1b |
| EN 1993-1-1 | SBE A.1 IPE 330, A.2 IPE 400, A.5 HE 300 B, P364 Ex. 9 UKC, SBE-MS8 Ex. 2 IPE 450 | Av, Mc,Rd, Vpl,Rd, C1, Mcr (top flange), λ̄LT, class, χy, χz, Nb,Rd, Nc,Rd, kyy, kzy |
| EN 1992-1-1 | JRC-EC2 §3.2.1.3 and §3.2.1.4 | MRd (mid-span T-beam and support), VRd,c, VRd,s |
| Connections | shear tab by hand (closed form) | bolt-group ratio, bearing, plate shear |

## Discrepancies found and fixed

Each case below failed before its fix and passes after it. The full core test suite passes after all of them.

1. **One effective length for both axes (AISC and EC3; conservative).**
   - Pondus took Lc = k·L about both axes, so a column braced about its weak axis at mid-height could not be
     represented.
   - AISC E.1D (Lcx = 30 ft, Lcy = 15 ft) gave φcPn = 596.9 kips against the printed 927, which is −35.6 %.
   - SBE A.5 (Lcr,y = 8.0 m, Lcr,z = 5.6 m) gave χz = 0.472 against the printed 0.671, which is −29.7 %.
   - **Fix:** the minor-axis buckling length is now k·Lb (`steel.rs` and `codes.rs`). The bracing that holds the
     compression flange is taken to hold the weak axis too. Lb defaults to the member length, so models that never
     set Lb are unchanged.
   - **After:** E.1D gives 927.5 kips (+0.06 %) and A.5 gives χz = 0.6705 (−0.07 %).
2. **EC3 classification of beam-columns (very conservative).**
   - Pondus applied Table 5.2's web limits for pure compression (33ε, 38ε, 42ε) to every station of any member that
     was ever in compression.
   - The IPE 450 S355 of SBE-MS8 Ex. 2 (NEd = 127 kN with My = 356 kNm, printed Class 1) became Class 4, with an
     effective area, and χz = 0.872 against 0.865.
   - **Fix:** `codes::ec3_class_nm` classifies each station from its own N and M. The web uses α for Classes 1 and 2
     and ψ for Class 3, and the flange is treated as uniformly compressed.
   - **After:** Class 1 as printed, and χz = 0.8660 (+0.11 %).
3. **EC3 Mcr for a top-flange load (conservative).**
   - Pondus left C2 to the engine, which takes C2 = C1 for a destabilising load.
   - For SBE A.1 (simply supported, uniform load, zg = +165 mm, SN003 C1 = 1.127, C2 = 0.454) this gave
     Mcr = 78.6 kNm against the printed 113.9, which is −31 %.
   - **Fix:** `codes::ec3_ltb` and `codes::ec3_c1` recognise the two SN003 diagrams the sources print.
     - A linear diagram (end moments only) gets C2 = 0.
     - A parabola with zero end moments (a uniformly loaded simply supported span) gets C1 = 1.127 and C2 = 0.454.
     - Every other diagram keeps the conservative C2 = C1.
   - **After:** Mcr = 113.86 kNm (−0.04 %) and λ̄LT = 1.2884 (+0.03 %).
4. **EC2 cross-check ignored the stirrup grade (unconservative).**
   - `check_ec2_beam` received fyt but discarded it. The engine has a single fyk, so SR24 stirrups (fyt = 235 MPa)
     under SD40 bars were checked as fy = 390 MPa, and VRd,s was 66 % too high.
   - **Fix:** the stirrup area is scaled by fyt/fy before it is passed in, which makes Asw·fyk the stirrups' own yield
     force in (6.8).
   - **After:** the ratio VRd,s(235)/VRd,s(390) = 0.6026, exactly fyt ratio.

## Deliberate Pondus choices, documented (no fix)

- **AISC tension rupture (fixed after this pass).** Pondus first took Ae = Ag, which overestimated the D.1 rupture
  capacity by 42 % (300 against 211 kips). It now computes the net section from the member's end connection
  (`TensionConnection`: bolted or welded, the connected elements, bolt diameter, holes per flange and web, bolts per
  line, connection length; a design-wide default with a per-member override). An subtracts holes of width d + 1/8 in
  (B4.3b), and U is the larger of Table D3.1 case 2 (1 − x̄/l, x̄ of the split tee or half channel without fillets) and
  case 7 (0.90 or 0.85 for flange-connected W shapes, 0.70 web-connected), or 1.0 when every element is connected.
  With the D.1 connection (3/4 in bolts, two holes per flange, l = 9 in), Pondus gives An = 4.760 in² (printed 4.76),
  U = 0.907 (printed 0.908 from the tabulated x̄ = 0.831 in, which includes the fillets) and φtPn = 210.5 kips
  (printed 211, −0.24 %). H.3, whose tension capacity is yielding on Ag, is run without a connection. EC3 checks the
  net section too, Nu,Rd = 0.9·Anet·fu/γM2 with d0 = d + 2 mm (closed-form row). Block shear (J4.3) is still not
  checked: that belongs to the connection design.
- **AISC Cb.** Pondus computes Cb from F1-1 over the member, and divides it by 1.4 for beams loaded on the top flange
  (Commentary F1); the examples load at the shear centre, so the test sets `top_flange_load = false`. Pondus's Cb is
  1.0135 on the F.1-2B centre third and 1.2987 on the F.1-3B half span, against the printed 1.01 and 1.30, both within
  0.35 %. The Mn rows use the example's own rounded Cb, so that the F2-2 and F2-3 equations are compared alone.
- **AISC H1.2.** Pondus does not apply H1.2's Cb increase for axial tension. In H.3 the printed Cb = 1.41 (Table
  3-1's 1.14 times 1.24) was produced by constructing a moment diagram whose F1-1 value is 1.41. Without that
  increase Pondus is conservative.
- **EC3 χLT.** Pondus uses the general case 6.3.2.2 (through the engine), whereas the examples use 6.3.2.3 with the
  f modification. Pondus's Mb,Rd is therefore lower, and χLT and Mb,Rd are not compared. Mcr and λ̄LT are compared
  and match.
- **EC3 fy for thick plates.** Pondus takes the material's fy as given. P364 Ex. 9 reduces S355 to 345 MPa for
  16 < t ≤ 40 mm, and the test passes 345. Pondus does not apply EN 10025's thickness steps itself.
- **E.** AISC is run with E = 200 000 MPa against AISC's 29 000 ksi (199 948 MPa), a 0.03 % difference that is
  inside every band. EC3 uses 210 000 MPa.

## Section properties used as inputs

The examples print some section properties and not others, and the treatment differs:
- **Printed properties are used as printed:** A, Ix, Sx, Zx, rx, ry, J, Sy, Zy, rts and ho where given.
- **W18×50 and W14×82:** Iy and Cw are derived from the printed rts and ho (Iy = 2·Sx·rts²/ho, Cw = Iy·ho²/4), so
  that Pondus's rts = (√(Iy·Cw)/Sx)^½ and ho = d − tf reproduce the printed values exactly.
- **Plate thicknesses and fillet radii** come from the Manual's Table 1-1 or the European tables. They set only the
  element slenderness, so they decide whether an element is compact or slender, and each example's class or
  compactness agrees with the printed one (F.3B: bf/2tf = 9.47 is noncompact, as printed).
- **Properties a check does not read** (for example Iy in a flexure-only case) are nominal and are not compared.

## Not calibrated, and why

- **AISC shear tab against a published example.** AISC Design Examples Part II (e.g. II.A-17) was not available as
  text in this session. The tab is checked clause by clause in closed form instead (elastic bolt group, J3.10
  bearing, J4.2 plate shear), and all three ratios match to 1e-6.
- **Weld groups.** Pondus calls the engine's `check_weld_groups` without a wrapper of its own. The engine's
  published-example coverage (INTI301 Ej. 3, AISC J.1) applies to it, and nothing Pondus-specific is left to
  calibrate.
- **Cold-formed steel (AISI S100), timber and masonry.** As in Stabileo's own pass, no free published example
  collection (AISI D100, AWC, TMS) was located, so these are not compared with printed values. Pondus's existing
  hand-derived tests in `core/tests/othercodes.rs` remain their only check.
- **AISC H1-1a, E4, F4/F5, G2.2, and EC3 kyz/kzz with Mz ≠ 0.** Either Pondus does not implement the clause, or no
  example with printed inputs was at hand.
