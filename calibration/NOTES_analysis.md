# Calibration of the analysis: notes

The suite `core/tests/calib_analysis.rs` builds each problem as a Pondus model and solves it through Pondus, so the wrapper is calibrated together with the engine. That means member splitting, load conversion, sign conventions, combinations, slab load transfer, and the seismic, time-history, stability and advanced modules. Each comparison is one row of `calibration/analysis.csv`. A row of kind *published* compares against a number printed in the cited source; a row of kind *closed-form* evaluates a formula of the cited source in the test. No row uses another program's output.

Run it with:

```sh
PONDUS_CALIB=$PWD/calibration cargo test --release -p pondus-core --test calib_analysis -- --test-threads 1
```

The final run gives 20 tests, all passing, and 119 rows: 53 published and 66 closed-form.

## Summary by aspect

| Aspect | Rows | Largest \|error\| | Kinds |
|---|---|---|---|
| Linear statics | 19 | 0.26 % (VM2 deflection, printed to 3 figures) | 4 published, 15 closed-form |
| Supports and loads (spring, settlement, temperature, diaphragm) | 6 | 0.000 % | closed-form |
| Slab load transfer | 7 | 0.000 % | closed-form |
| Plates (convergence study, n = 5 … 33) | 23 | 17.1 % at n = 5; ≤ 0.9 % at n = 33 | 22 published, 1 closed-form |
| Modal | 6 | 0.061 % | 3 published, 3 closed-form |
| Seismic (RSA, ELF) | 8 | 0.155 % | closed-form |
| Time history | 2 | 0.015 % | closed-form |
| Stability (Euler cases, portal, P-Delta) | 6 | 0.051 % | 1 published, 5 closed-form |
| Plastic collapse | 5 | 0.000 % | 1 published, 4 closed-form |
| Moving loads and influence lines | 3 | 0.000 % | closed-form |
| Staged construction | 2 | 0.003 % | closed-form |
| Cables (Ernst modulus) | 2 | 0.013 % | closed-form |
| Foundations (Winkler) | 2 | 0.089 % | closed-form |
| Nonlinear members (tension-only, lift-off) | 4 | 0.000 % | closed-form |
| Section properties | 24 | 0.39 % | 22 published, 2 closed-form |

## Problems, sources and what is compared

### Linear statics

- **ANSYS MAPDL Verification Manual, VM2 (beam stresses and deflections).** The data were taken from the PyMAPDL reproduction of the manual (examples.mapdl.docs.pyansys.com, vm-002): a = 120 in, l = 240 in, h = 30 in, E = 30 × 10⁶ psi, I = 7892 in⁴, w = 10 000/12 lb/in on the overhangs. The targets are σ = 11 400 psi and a mid-span deflection of 0.182 in. Pondus gives 11 404 psi (+0.035 %) and 0.18246 in (+0.25 %). The printed targets are rounded to three figures: Mc/I = 11 404 psi and Ml²/8EI = 0.1825 in by hand.
- **ANSYS VM4 (deflection of a hinged support).** Two bars, l = 15 ft, A = 0.5 in², θ = 30°, F = 5000 lb. The targets are σ = 10 000 psi and δ = −0.12 in, and Pondus reproduces both exactly.
- **VM1 is excluded.** The problem data as reproduced do not give the printed reactions of 900 and 600 lb under any reading I could check, so no row is reported.
- **Roark 9e Table 8.1, Hibbeler 10e Chs. 10–12, Ghali & Neville 7e Ch. 4.** The cases are a propped cantilever under UDL (R_B = 3wL/8, M_A = −wL²/8), a fixed-ended beam with a central load (±PL/8, PL³/192EI), a two-span continuous beam (M_B = −wL²/8, R_B = 5wL/4) and a Gerber beam with an internal hinge. All agree to rounding.
- **Kassimali 6e Ch. 4.** A four-panel Pratt truss: chord and diagonal forces by the method of sections, exact.
- **Fixed-base portal, sway under a lateral load.** The slope-deflection result is k = (24EIc/h³)(1 + 6ρ)/(4 + 6ρ), with ρ = (Ib/L)/(Ic/h). The difference is +0.012 %, from the columns' axial strain, which the formula neglects.
- **3D L-frame (McGuire, Gallagher & Ziemian 2e, Ch. 13).** An out-of-plane tip load, combining beam bending, column bending and column torsion: δ = PL³/3EI + PH³/3EI + PL²H/GJ. Exact.

### Supports and loads

The cases are a cantilever on a tip spring (compatibility), a support settlement (6EIΔ/L²), a temperature gradient across the depth (EIαΔT/h, uniform along the beam), a uniform temperature rise in a restrained bar (EAαΔT), and two columns tied by a rigid diaphragm sharing a storey shear in proportion to stiffness. All are exact. The engine applies α = 12 × 10⁻⁶ /°C to every member, so Pondus scales each thermal load by the member material's α / 12 × 10⁻⁶ (concrete 10 × 10⁻⁶, steel 12 × 10⁻⁶, concrete masonry 8.1 × 10⁻⁶, timber 5 × 10⁻⁶ /°C, or the material's own value), and the temperature rows use the concrete α.

### Slab load transfer

The first case is a two-way 4 × 6 m panel on simply supported edge beams (45° tributary lines). Each long beam takes the trapezoid q(lx·ly/2 − lx²/4) = 80 kN and each short beam the triangle q·lx²/4 = 40 kN. The mid-span moments are w0(3L² − 4a²)/24 = 76.667 kN·m (trapezoid) and w0·lx²/12 = 26.667 kN·m (triangle). The second case is a one-way 3 × 8 m panel (ly/lx > 2): the long beams take half the load each, and the short beams nothing. Both cases are exact, with the reactions balancing the slab load.

### Plates (Timoshenko & Woinowsky-Krieger 2e, Tables 8 and 35, ν = 0.3)

The plates are square, a = 4 m, with h = a/100. The tables are Kirchhoff (thin-plate) solutions, whereas Pondus's plate elements (MITC4) are Mindlin elements and carry transverse shear. The first attempt used h = 0.2 m (a/h = 20) and overshot the clamped deflection by 3.8 % at n = 17. That excess is the shear deformation, roughly (h/a)² relative, and not an error of the elements. For the comparison the plate is therefore thin.

Simply supported edges are rigid edge beams free in torsion; clamped edges are rigid edge beams rigid in torsion. The centre value is the element at the centre (n is odd, so an element sits there). The deflection is the largest element-centre deflection, which approaches the nodal maximum from below. The clamped mid-edge moment is extrapolated linearly to the edge from the two elements next to it.

| n across a | SS w_max | SS Mx centre | Clamped w_max | Clamped Mx centre | Clamped Mx mid-edge |
|---|---|---|---|---|---|
| 5 | −10.29 % | −2.54 % | −17.05 % | +3.09 % | – |
| 9 | −3.09 % | −0.83 % | −4.68 % | +0.35 % | +9.58 % (less hogging) |
| 17 | −0.79 % | −0.26 % | −0.85 % | −0.51 % | +3.11 % |
| 33 | −0.13 % | −0.09 % | +0.23 % | −0.74 % | +0.88 % |

The references are w = 0.00406 qa⁴/D and Mx = 0.0479 qa² for the simply supported plate, and w = 0.00126 qa⁴/D, Mx centre = 0.0231 qa² and Mx edge = −0.0513 qa² for the clamped one. At b/a = 2 with n = 17 (w = 0.01013 qa⁴/D, Mx = 0.1017 qa², My = 0.0464 qa²), the errors are −0.85 %, −0.27 % and −0.53 %. Moments converge faster than deflections. Pondus's default slab mesh is 0.5 m, which on a 4 m panel is n = 8 and gives about 3–5 % low deflections but moments within 1 %. Plate deflections used for serviceability should therefore be checked on a mesh of a/16 or finer.

### Modal and seismic

- **Clough & Penzien 3e, Table 18-1.** A uniform cantilever with βnL = 1.8751, 4.6941 and 7.8548, solved as 32 elements with mass from self-weight. The first three lateral frequencies agree within 0.001 %.
- **Chopra 5e Ch. 10.** A two-storey shear frame (rigid beams, massless columns, equal m and k), with ω² = (k/m)(3 ∓ √5)/2. The frequencies agree within 0.06 % and the first-mode effective-mass ratio, 0.9472, within 0.04 %. The small residual is the columns' axial flexibility.
- **Response spectrum (Chopra 5e Ch. 13; Der Kiureghian 1981).** A flat spectrum Sa = 0.5 g with R = 1: the modal base shears r_i·W·Sa, combined by CQC (ξ = 5 %), agree within 0.16 %. The difference follows from the 0.04 % mass-ratio residual.
- **ASCE 7-22 §12.8 (ELF).** Two frames: a three-storey frame in the velocity branch, and a stiff one-storey frame whose T1 = 0.038 s lies below T0 = 0.12 s. T = min(T1, Cu·Ta), Cs and V = Cs·W are exact after the fix described below. I found no freely available printed ELF worked example with complete input data (FEMA P-751 and the SEAOC manuals were not obtainable), so these rows check the equations, not a printed example. Pondus does not distribute the ELF over the height (Eq. 12.8-11/12, with exponent k); it scales the response spectrum result to the ELF base shear instead. The Fx distribution is therefore not calibrated because it is not computed.

### Time history

An undamped SDOF system is subjected to a step ground acceleration a0 = 0.1 g (Chopra 5e §4.3, dynamic amplification 2). It is modelled as a massless cantilever column with its mass on a slim stiff stub. The peak relative displacement 2a0/ω² agrees within 0.014 % at Δt = Tn/200, and the modal frequency √(k/m) within 0.010 %. The first stub (A = 1, I = 1 m⁴) carried a rotary inertia large enough to lower the y frequency by 13 %. That was a fault of the test model, so the stub was made slim.

### Stability

- **Timoshenko & Gere 2e Ch. 2.** Columns with 8 elements: pinned (π²EI/L²), cantilever (π²EI/4L²), fixed-pinned (20.19 EI/L², printed) and fixed-fixed (4π²EI/L²). All agree within 0.05 %.
- **Fixed-base portal, sway buckling (Ib = Ic, L = h).** The governing equation is x·cot x = −6/G with G = 1, which is Timoshenko & Gere §2.5 and the alignment-chart equation of AISC 360-16 Commentary C-A-7-2 (exact for this frame). Pondus gives 9224.07 kN against 9223.94 kN (+0.001 %) with axially rigid members. With A = 0.01 m² the result is 0.36 % lower, which is the real axial shortening the closed form neglects.
- **P-Delta of a cantilever at 0.4 Pcr.** The exact amplification 3(tan u − u)/u³ (Timoshenko & Gere §1.11) is matched within 0.05 %.

### Plastic collapse (Neal 3e; Horne)

The sections are H 300 × 150, with Mp = 245 MPa × Zx. The cases are a simple beam with a central load (4Mp/L), a propped cantilever with a central load (6Mp/L), a fixed beam under UDL (16Mp/L²), two equal spans under UDL ((6 + 4√2)Mp/L², printed as 11.66), and a fixed-base portal with V = 1.5H, where the combined mechanism governs (6Mp/(h + 1.5L/2)). All are exact. The two-span case shows that the engine finds the span hinge at 0.414L, not at an element end.

### Moving loads, influence lines, staging, cables, foundations, one-way members

- **Moving loads and influence lines (Hibbeler 10e Ch. 6).** Two 100 kN axles 2 m apart on a 10 m span give the absolute maximum moment P(L − s/2)²/2L = 405 kN·m, exact after the fix below. The influence line of M at L/4 peaks at a(L − a)/L = 1.875, and the reaction influence line is 0.5 at mid-span.
- **Staged construction.** A cantilever carries its dead load alone, then a stiff prop is built under its tip and the live load follows. The root moment is −(wD·L²/2 + wL·L²/8) and the prop force 3wL·L/8; both agree within 0.003 %.
- **Cables (Ernst 1965; Irvine 1981).** The engine's cables are small-displacement, tension-only trusses with Ernst's equivalent modulus. A horizontal 50 m stay under 100 kN stretches HL/(E_eq·A) with E_eq/E = 1/[1 + (wL)²EA/12H³]; the result is within 0.014 %.
- **Large-displacement wire not modelled.** A taut wire with no pretension under a central load cannot be represented, because the engine has no large-displacement cable formulation. A 50 mm initial sag gave 12.6 m of sag in the small-displacement solution, so that case was replaced and the limitation is recorded below.
- **Hetényi (1946).** An "infinite" beam (βL = 10, 64 elements) on a Winkler foundation under a point load: w0 = Pβ/2k within 0.015 % and M0 = P/4β within 0.09 %, after the fix below.
- **Tension-only X brace and lift-off support.** Exact by statics.

### Section properties

- **Timoshenko & Goodier 3e §109.** The torsion coefficient k1 = J/(bt³) of rectangles with b/t = 1 to 10 (printed to three figures). The section finite elements are within 0.39 %, and Pondus's frame formula for rectangles (Roark's approximation, used for every rectangular member in the analysis) is within 0.19 %.
- **Warping constant of an I without fillets.** Cw = tf·bf³·h0²/24, within 0.07 %.
- **Shear centre of a thin channel.** e = 3b²·tf/(6b·tf + h·tw), within 0.05 %.
- **JIS G 3192.** A and Ix of five rolled H shapes, from Pondus's plate-and-fillet geometry: A within 0.02 % and Ix within 0.25 %.

## Discrepancies found, their root causes and the fixes

1. **ELF Cs below T0 (bug, `core/src/seismic.rs`).** Pondus took Cs = Sa(T)/(R/Ie) from the design spectrum, so a period below T0 = 0.2·SD1/SDS fell on the spectrum's ascending branch. ASCE 7-22 Eq. 12.8-2 gives Cs = SDS/(R/Ie) on the whole short-period plateau, with no ascending branch. For the stiff one-storey frame (T = 0.038 s) Cs was 0.0738 instead of 0.125, so the ELF base shear and the response spectrum scaling were 41 % low. Fixed: Cs = SDS/(R/Ie) for T ≤ Ts with the code spectrum; after the fix it is exact.
2. **Moving-load envelope misses the peak under an axle (Pondus-side fix in `core/src/advanced.rs`).** The engine's `solve_moving_loads_3d` envelopes each element from its end forces only (`engine/src/solver/moving_loads.rs:416`), so a peak under an axle standing inside an element was lost. The 10 m span gave 400.0 kN·m against 405.0 kN·m (−1.23 %). Pondus now runs the train over a load-free model of its own whose path members are cut at the load step, so every stop is an element end, and reads that envelope at the main stations. After the fix the result is 405.000 kN·m. The engine itself was not changed.
3. **Winkler [kz, ky] order (bug, `core/src/analysis.rs`).** The MCP tool, the Excel sheet and (now) the model documentation give `winkler` as [kz, ky], but the analysis passed k[0] to the engine as ky and k[1] as kz. A beam given only the usual vertical subgrade modulus therefore had no vertical foundation: the solve was singular, or with 8 elements returned 1.4 × 10¹¹ mm. The existing tests used [k, k] and hid it. Fixed: kz = k[0], ky = k[1]. After the fix the Hetényi result is within 0.015 %.

The engine was not changed by this calibration.

## Limitations and what is not calibrated

- **ELF vertical distribution.** No printed ELF worked example was obtained, and Pondus does not compute the vertical ELF distribution (it scales the response spectrum result), so Fx with exponent k is not calibrated.
- **Time history.** Only an SDOF case is calibrated; no MDOF time history is checked against a printed example.
- **Cables.** The engine's cables are small-displacement trusses with the Ernst modulus. Slack or initially straight cables under large transverse loads, which need a large-displacement formulation, are outside what Pondus solves.
- **Thermal α (fixed after this pass).** The engine applies α = 12 × 10⁻⁶ /°C for every material, 20 % high for concrete. Pondus now scales the temperatures by the material's α (`Material::alpha`), so the restrained-bar and gradient rows use α = 10 × 10⁻⁶ /°C for the concrete test members and still agree exactly.
- **Plate theory.** Plate results are Mindlin results. For thick slabs (a/h ≈ 20) the deflection exceeds the Kirchhoff tables by a few per cent through shear deformation, which is physical. At the default 0.5 m mesh, deflections run 3–5 % low on 4 m panels.
- **Axial shortening.** Buckling loads include it (0.36 % on the portal with A = 0.01 m²), whereas textbook values neglect it.
- **Stabileo's reproduced cases.** Stabileo's "VM-inspired" and commercial-program cases (SAP2000, STAAD, Robot) are reproduced analytically in its tests rather than taken from the printed targets, so they were not used as published references here.
