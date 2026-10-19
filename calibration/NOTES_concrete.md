# Calibration notes: concrete, timber and masonry design

Suite `concrete` (`core/tests/calib_concrete.rs`, rows in `calibration/concrete.csv`). Run with

```sh
rm -f calibration/concrete.csv
PONDUS_CALIB=$PWD/calibration cargo test --release -p pondus-core --test calib_concrete -- --nocapture --test-threads 1
```

The last run gave 205 rows in 13 tests, all within tolerance: 170 against published values, 33 against closed-form expressions of the cited source evaluated independently in the test, and 2 against an independent exact solution. Every row compares a Pondus function (`pondus_core::rc`, `footing`, `slabfe::wood_armer`, `othermat`) with the source, never the engine's own checkers, so the rows calibrate the code Pondus designs with.

## Units and tolerances

Pondus works in the SI form of ACI 318 (ACI 318M), whose constants are rounded versions of the psi ones. Most examples are printed in US units and were converted exactly (in = 25.4 mm, ksi = 6.894757 MPa, kip = 4.448222 kN). The default tolerance is 0.5 %, half a unit in the third printed digit. A row gets a wider band only where an SI constant enters, and the band is the offset that constant implies:

| SI constant | psi constant | Offset | Rows |
|---|---|---|---|
| 0.17√f'c | 2√f'c | +2.4 % | φVc (2.5 %); Vs = Vu/φ − Vc inherits −6.4 % (7 %) |
| 0.42√f'c | 5√f'c | +1.1 % on a difference | Al,min, +4.0 % (5 %) |
| 1.4/fy, 0.35 bw/fyt | 200/fy, 50 bw/fyt | +1.5 % | As,min, (Av/s)min (2 %) |
| 0.62√f'c | 7.5√f'c | −0.5 % | Mcr (1 %) |
| 15 mm | 0.6 in | −1.6 % on one term | M2,min, −0.85 % (1 %) |
| β1 = 0.85 − 0.05(f'c − 28)/7 (MPa) | 0.85 − 0.05(f'c − 4000)/1000 (psi) | up to +0.7 % | column points Pn, Mn, φMn (1 %); β1 itself (1 %) |

The largest error of the 145 published rows held to 0.5 % or less is 0.16 % (one-way slab, Vu at the supports, wl/2). The Wood–Armer and closed-form rows agree to rounding (≤ 1e-6 %).

## Sources and results

1. **ACI 318-63 Method 2 Table I (two-way slabs).** ACI 318-63 Appendix A, Method 2, Table I, "Moment coefficients", as a scan in H. K. Hussain, *Analysis and Design of Two way concrete slab*, lecture notes, University of Basrah (faculty.uobasrah.edu.iq/uploads/teaching/1622828460.pdf, image kept locally as `sources/aci318-63_method2_table1_basrah.png` and not in the repository, as it is third-party material: see the URL).
   - **Table.** Every one of the 91 coefficients (13 rows of the five cases × six values of m and the long span) equals Pondus's `METHOD2` table exactly. A further 13 rows confirm that m < 0.5 takes the "0.5 and less" column. The limitation in the README, that only the interior row had been confirmed, is closed for the 1963 table. EIT 1008, which adopts the same table, was not available.
   - **Example 1** (interior panel, 6.0 × 7.2 m, 200 mm, f'c 21 MPa, DL 4.8 + 3 kPa, LL 10 kPa) is reproduced through `design_slab`:
     - wu = 25.36 kPa;
     - interpolated coefficients 0.04536 and 0.03402 at m = 0.833;
     - Mu = 41.41, 31.06, 30.13 and 22.82 kN·m/m, all within 0.06 %.
   - **Example 2** (6.1 m square corner panel, case 3, wu = 11.72 kPa): Mu = 21.37, 10.90 and 16.14 kN·m/m.
   - **Errors in the notes.** The notes label the 0.049 row "discontinuous edge" and the 0.025 row "continuous edge", the reverse of Table I; their values and Pondus's assignment follow Table I. Their Example 1 also writes −C as 0.05436 in the moment line while using 0.04536 (41.41 = 0.04536 × 25.36 × 36), so that is a transposition.
2. **One-way slabs (ACI 318-19 Table 6.5.2, unchanged from 318-14).**
   - **Source.** StructurePoint, *One-Way Slab Analysis and Design (ACI 318-14)*, version October 2025, Tables 1–2, after Wight, *Reinforced Concrete Mechanics and Design*, 7th ed. (2016), Example 5-7.
   - **Matching coefficients.** These agree within 0.16 %: wl²/24 at an exterior support built into a spandrel beam, wl²/16 and wl²/11 for interior spans, and shears of 1.15 wl/2 and wl/2.
   - **Conservative choices**, checked as Pondus ≥ published:
     - the end span's midspan takes wl²/11 (the unrestrained-end value) where the example uses wl²/14 for an end integral with its support;
     - the first interior support takes wl²/9 (see the fix below), against the example's wl²/10 with ln the mean of the adjacent clear spans.
3. **Beam flexure.**
   - **INTI 2.I.6.** INTI-CIRSOC, Hernández Balat, Bissio and Ortega, *Ejemplos de Aplicación del Reglamento CIRSOC 201-2005* (2005), Ejemplo 2.I.6, as recorded with its printed values in Stabileo's `engine/tests/design_worked_examples.rs`. φMn = 0.9 × 56.20 kN·m (+0.01 %) and kc = 0.213 (−0.08 %).
   - **Chaallal beam.** StructurePoint, *Simply Supported Reinforced Concrete Beam Analysis and Design (ACI 318-14)* (July 2025), after Chaallal, *Reinforced Concrete Structures*, 2nd ed. (2018). As required = 2.872 in² (+0.02 %); As,min and β1 are within the SI bands above.
   - **Not compared.** Example 2.I.7 (doubly reinforced capacity) is not compared, because Pondus has no capacity function for a given doubly reinforced section: it sizes compression steel inside `flexure_steel`. Example 3.I (shear) uses CIRSOC's Vc = √f'c bw d/6, which is not ACI 318-19.
4. **Beam shear** (the same Chaallal example). φVc and Vs are within the SI bands. (Av/s)min and s,max = d/2 are exact.
5. **Deflection** (the same Chaallal example).
   - **Exact quantities.** kd = 6.37 in and Icr = 3 759 in⁴ are exact. Mcr = 32.98 kip-ft is −0.45 % (the SI constant).
   - **Ie.** Pondus implements ACI 318-19 Table 24.2.3.5, Ie = Icr/(1 − ((2/3)Mcr/Ma)²(1 − Icr/Ig)). The example is ACI 318-14 (Branson), so the 318-19 expression was evaluated independently on the example's Icr, Ig, Mcr and Ma: 4 009 and 3 807 in⁴, where the example's printed Branson values are 4 337 and 3 812 in⁴. The gap at the dead-load level (−7.6 %) is the intended change between the editions, not an error.
6. **Torsion.** StructurePoint, *Equilibrium Torsion in Beams (ACI 318-14)* (February 2026), after Wight, 7th ed., Example 7-2, a 14 × 24 in cantilever under Tu = 28 kip-ft. The 22.7 clauses used are the same in ACI 318-19.
   - **Matching quantities.** These agree within 0.01 %: Aoh, ph, Ao, φTth = 5.87 kip-ft, φTcr = 23.49 kip-ft, At/s = 0.0204 in²/in, Al = 1.265 in² and s,max = ph/8.
   - **Constant-dependent quantities.** Al,min, φVc and (Av + 2At)/s min are within their SI bands. The cross-section ratio of 22.7.7.1 is +0.05 %.
7. **Tied column interaction diagram.** StructurePoint, *Interaction Diagram – Tied Reinforced Concrete Column Design Strength (ACI 318-19)* (2022), 16 × 16 in, 4-#9 each face.
   - **Exact points.** P0 = 1 534 kips and φPn,max = 797.7 kips are exact.
   - **Control points.** Pn and Mn at the four printed neutral-axis depths are within 0.74 %, the residue of β1 = 0.804 (SI) against 0.80 (psi). Pure bending gives Mn = 237.7 kip-ft with φ = 0.90.
   - **Design search.** `column_phi_mn`, the search that design uses, returns φMn at each printed φPn within 0.37 %.
8. **Biaxial bending.** StructurePoint, *Biaxial Bending Interaction Diagrams for Rectangular RC Column Design (ACI 318-19)* (2022), after Pincheira et al., *Reinforced Concrete Design*, 9th ed. (2021), Example 10.20.1.
   - **The comparison.** The exact skewed-axis capacity at Pn = 426 kips with Mnx/Mny = 1.60 is (321, 201) kip-ft. Pondus designs biaxial bending by Bresler's load contour, (Mux/φMnx)^α + (Muy/φMny)^α ≤ 1 with α = 1.15, on its own uniaxial capacities (Mnx0 = 477.1 and Mny0 = 387.0 kip-ft at the same Pn). This gives (294.6, 184.1), 8.2–8.4 % below the exact point.
   - **Interpretation.** The load contour is therefore conservative by about 8 % for this section. The rows are kind "independent" with a 15 % band, and the test asserts the contour does not exceed the exact value. This measures the method Pondus chose, not a coding error.
9. **Slender columns, non-sway.** StructurePoint, *Slenderness Effects for Columns in Non-Sway Frame – Moment Magnification Method (ACI 318-19)* (January 2020), after Wang et al., *Reinforced Concrete Design*, 8th ed., Example 13.17.3.
   - **Results.** Mc = 105 kip-ft and δns = 1.00 are exact. M2,min = 48.56 kip-ft is −0.85 % (15 mm for 0.6 in).
   - **Inconsistency in the example.** Its Table 1 gives the axial load as 1 776 kips while every calculation uses Pu = 525 kips; the calculations were followed.
   - **(EI)eff option.** The example takes (EI)eff by 6.6.4.4.4(b) with βdns = 0.4 (Pc = 7 871 kips). Pondus has option (a), 0.4 EcIg/(1 + βdns), and in design assumes βdns = 0.6, because the sustained share of the load is not known. Pc by (a) is checked closed-form (4 627 kips at βdns = 0.4), as is the magnifier where it applies (δ = Cm/(1 − Pu/0.75Pc) at 1.8 klu). At 3 klu, Pu > 0.75 Pc and Pondus reports the column unstable (δ = ∞), as it should.
   - **Radius of gyration.** Pondus takes r = 0.3h (6.2.5.1(b)) where the example uses √(Ig/Ag) (6.2.5.1(a)); both are permitted.
10. **Spread footing.** StructurePoint, *Reinforced Concrete Spread Footing (Isolated Footing) Analysis and Design* (ACI 318-14, September 2018), after Wight, 7th ed., Example 15-2.
    - **Results.** Mu at the column face = 954 kip-ft (−0.03 %, `footing::face_moment`), As = 7.76 in² (−0.02 %) and As,min = 0.0018bh = 7.72 in².
    - **Not compared.** The printed shear capacities are ACI 318-14. ACI 318-19 adds the size factor λs (about 0.65 at d = 28 in), and Pondus's footing shear checks are the Stabileo engine's (calibrated in Stabileo), so they are not compared.
11. **Wood–Armer.** Wood (1968), *Concrete* 2(2), and Armer's discussion (1968), normal and special rules, evaluated independently on seven moment fields that cover every branch (28 rows, exact).
12. **Timber column (NDS 2018).** Dlubal knowledge base article 001674, *Timber Column Design as per the 2018 NDS Standard*: Alaska cedar 7.5 × 7.5 in, L = 10 ft pinned, P = 30 kips.
    - **Inputs.** Pondus applies CD = CM = Ct = CF = Ci = 1, so the example's adjusted values were given as the grade values: Fc* = 673.40 psi and E'min = 418 ksi.
    - **Results.** CP = 0.8667 against the printed 0.866 (+0.08 %), and η = 0.9139 against the printed 0.913.
    - **Misprint.** The example's printed F'c = 585.86 psi does not follow from its own CP (673.40 × 0.866 = 583.2 psi), so CP and η were compared instead.
13. **Masonry (TMS 402-16 9.3.2).** No accessible published worked example was obtained: the Masonry Society handouts and the University of Michigan notes that the search returned could not be downloaded. The flexure ratio was therefore checked closed-form against Mn = As fy (d − a/2) with a = As fy/(0.80 f'm b), φ = 0.90 and Pondus's mapping d = t/2 (exact). The masonry interaction is the engine's simplified linear sum Pu/φPn + Mu/φMn, not a P–M diagram. That is a conservative limitation of the check, not calibrated here.

## Changes to Pondus

- **One-way slab coefficient fix.** The negative moment at the first interior support of an end span was wl²/10, the ACI value for more than two spans. With two spans ACI requires wl²/9, so Pondus was up to 10 % unconservative for two-span slabs. A panel does not know how many spans follow, so Pondus now uses wl²/9:
  - before: wl²/10 with the panel's ln = 13.08 ft, 19.56 kN·m/m (4.40 kip-ft/ft) on the Wight 5-7 end span;
  - after: wl²/9, 21.73 kN·m/m, against the example's 20.68 kN·m/m (4.65 kip-ft/ft, wl²/10 with ln = 13.46 ft).
  This is safe for any number of spans.
- **Refactor for calibration only, design results unchanged.** Pure functions were taken out of `design_beam`, `design_column` and the footing design so that each code quantity can be compared by itself: `torsion_terms`, `torsion_section_ratio`, `shear_terms`, `cracked_inertia`, `effective_inertia`, `column_p0`, `section_at`, `bar_rows`, `column_mn_nominal`, `critical_load`, `moment_magnifier` (`rc.rs`) and `face_moment` (`footing.rs`). The full core suite passes unchanged afterwards, including `torsion_by_hand` and the design flows.

## Not calibrated, and why

- Doubly reinforced section capacity (no Pondus function), the CIRSOC shear example (a different code) and the footing shear capacities (318-14 printed; the checks are the engine's).
- The column design's βdns = 0.6 assumption and the load-contour exponent α = 1.15. Their effect is quantified (biaxial −8 %), but the published examples use other choices.
- Torsion and deflection of a whole designed beam through `design_beam`: its effective depth assumes a 20 mm bar and its demands come from the analysis, so the clause-level functions were compared instead.
- Slab deflection and two-way shear (punching is flagged, not designed). Timber bending (CL) and masonry axial capacity were not compared with a published example.
