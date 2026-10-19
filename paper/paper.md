---
title: "Calibrating an agentic structural analysis and design program against published benchmarks"
subtitle: "Pondus: 396 comparisons with textbooks, verification manuals and code worked examples"
author:
  - 'Sompote Youwai`\\`{=latex} `\small`{=latex} King Mongkut''s University of Technology Thonburi, Bangkok, Thailand`\\`{=latex} `\small`{=latex} sompote.you@kmutt.ac.th'
date: "October 2026"
abstract: |
  Language-model agents can now build, analyse and design building structures through the tools a program exposes, so the correctness of those tools sets the correctness of everything the agent produces. This paper reports the calibration of Pondus, an open-source program in which an agent and an engineer share one structural model, against reference values from textbooks, software verification manuals and published worked examples of the design codes. Every comparison passes through the program's own model, analysis wrapper and design code rather than through the analysis engine it links, because the engine had already been verified by its authors and the risk sits in the layer above it. The calibration covers linear and nonlinear frame analysis, slab load transfer, plates, modal and seismic analysis, time history, stability, plastic collapse, moving loads, staged construction, section properties, AISC 360-16 and EN 1993-1-1 steel design, the EN 1992-1-1 cross-check, ACI 318-19 beams, slabs, columns and footings, the ACI 318-63 two-way slab coefficients, Wood–Armer reinforcement, timber and masonry. Of 396 comparisons over 219 cases, 367 agree within 0.5 % and 379 within 1 %, and each of the remaining 17 is explained by mesh convergence, by the conservative load-contour method for biaxial bending or by the rounded SI constants of ACI 318 itself. The calibration, with the closed-form tests that preceded it, found twelve defects, six of them unconservative, including a base shear coefficient 41 % low for stiff structures, a stirrup shear resistance 66 % high and a tension rupture capacity 42 % high. Eleven of the twelve sat in the code that turns the model into the inputs of a kernel or a check, and only one in a numerical kernel, which suggests where verification effort for agent-driven engineering software should go.
keywords: [structural analysis software, verification, benchmark, design codes, ACI 318, AISC 360, Eurocode, language-model agents]
geometry: margin=2.2cm
fontsize: 10pt
linestretch: 1.15
---

# Introduction

Structural design software has always needed evidence that it computes what its documentation claims, and the profession has built that evidence from benchmark problems with known answers. Verification manuals of commercial programs, the NAFEMS benchmarks and the worked examples that code bodies publish alongside their standards give an engineer a way to check a program before trusting it with a real building. The arrival of language-model agents that drive engineering programs through tool interfaces such as the Model Context Protocol [@mcp2024] raises the stakes of that check. An agent can assemble a model, run an analysis and size every member in seconds, but it accepts the numbers the tools return, so an error in a tool propagates through the whole design without the pause in which an engineer would have noticed that a result looked wrong.

Pondus is a native program in which an agent and an engineer work on one structural model. The agent calls the same commands as the engineer's user interface, the analysis runs on the Stabileo engine [@stabileo], and the design follows ACI 318-19 [@aci318], AISC 360-16 [@aisc360] and EN 1993-1-1 [@en1993] among other codes. Stabileo ships its own verification suite against textbooks and design examples, so the engine's kernels were not the main concern. The concern was the layer that Pondus adds above the engine: the code that splits members, converts loads, carries slab loads to beams, assembles combinations, maps sign conventions and turns analysis results into the inputs of each design check. That layer is ordinary application code, written quickly, and it is exactly the code an agent depends on without seeing.

This paper reports a calibration of that layer against published reference values. The word calibration is used here in its everyday engineering sense of checking an instrument against known standards, and it corresponds to code and solution verification in the vocabulary of ASME V&V 10 [@asme2006vv10] and @oberkampf2010. No parameter was fitted to the references, so agreement means that the program reproduces the reference and disagreement means a defect, a deliberate simplification or a difference between editions of a code. The paper describes the method in Section 3, reports the results by aspect in Sections 4 to 7, analyses the defects the calibration found in Section 8 and states what was not calibrated in Section 9.

# The program and what needed calibration

Pondus is organised in four layers. The engine is Stabileo's `dedaliano-engine` at commit 7985e8c, vendored into the workspace with two marked source changes: a sign correction in the vertical Winkler foundation matrix and a sparse Cholesky test placed before the dense rank test that the plastic solver runs at every hinge event, which changes speed only. The core library holds the model, the analysis wrapper, the design codes, the advanced analyses, the calculation report and the interchange formats. The MCP server exposes the core's commands to agents, and the desktop application exposes the same commands to the engineer through a ribbon, tables and a chat.

The analysis wrapper does more than pass the model to the engine. It splits every member at the nodes that lie on it, at every load break point and into at least eight elements, so each element carries a load that is linear along its length. It transfers slab loads to the edge beams along the 45° lines of each panel, or one way when the aspect ratio exceeds 2, or it meshes the slabs into MITC4 shell elements [@dvorkin1984] joined to the frame. It builds the seismic mass from the model's own load cases, scales the response spectrum result to the equivalent lateral force base shear, routes nonlinear members and supports to the engine's contact, cable and Winkler solvers, and computes pushover, moving-load envelopes, influence lines and construction stages on models of its own. The design code is Pondus's own for ACI 318 beams, slabs, columns and footings, for AISC 360 members and for the slab coefficient methods, and it wraps the engine's checkers for EN 1993-1-1, the EN 1992-1-1 cross-check, the shear tab bolt and weld groups, timber and masonry.

Each of these steps can be wrong while the engine underneath is right, because each step decides what the engine or the checker is asked. A wrong effective length, a wrong axis order or a missing coefficient produces a confident and plausible number. The calibration was therefore designed so that every comparison enters through the same functions the program uses when it designs a building.

# Method

## Reference values

Each comparison is recorded with its source, the quantity, its unit, the reference value, the Pondus value, the relative error, the tolerance and the kind of reference. A reference of kind *published* is a number printed in the cited source, such as the $\phi_c P_n = 892$ kips of AISC Design Example E.1C [@aisc2023examples] or the coefficient $w_{max} = 0.00406\,qa^4/D$ of Timoshenko and Woinowsky-Krieger's Table 8 [@timoshenko1959]. A reference of kind *closed-form* evaluates, in the test, a formula of the cited source, such as $6EI\Delta/L^2$ for a support settlement [@ghali] or the CQC combination of @derkiureghian1981, and the formula is named in the record. A reference of kind *independent* is an exact solution by another method, used twice for biaxial column capacity. Of the 396 comparisons, 288 are published, 106 closed-form and 2 independent.

The published values come from four groups of sources. The first is the set of inputs and printed answers that Stabileo records, with page numbers, from the AISC Design Examples v16 [@aisc2023examples], the SCI/CTICM Steel Buildings in Europe examples [@sbe2010], SCI P364 [@p364], the JRC Eurocode 2 worked examples [@jrc2014] and the INTI-CIRSOC examples [@inti2005; @inti301]. The second is the classical textbook literature of structural analysis, dynamics, stability and plates [@timoshenko1959; @timoshenko1961; @timoshenko1970; @roark; @hibbeler; @kassimali; @ghali; @mcguire; @chopra; @clough; @neal; @horne; @hetenyi; @ernst1965; @irvine]. The third is the ANSYS Verification Manual as reproduced by PyMAPDL [@ansysvm]. The fourth is a set of ACI 318 worked examples published by StructurePoint after @wight and other textbooks [@structurepoint], a scan of the ACI 318-63 Method 2 coefficient table in lecture notes of the University of Basrah [@hussain; @aci31863], and a timber column example by Dlubal [@dlubal; @nds2018].

## The rule that everything passes through Pondus

No comparison calls the engine's checkers directly. A frame problem is built as a Pondus model and solved by the Pondus analysis, so the member splitting, the load conversion and the reading of results are part of what is tested. A steel member is checked by the Pondus function that the design loop calls, `steel::check` for AISC 360 and `codes::check_ec3` for EN 1993-1-1, with the section properties the example prints converted to SI units. Where Pondus computes a property itself, such as $C_b$ from the moment diagram or $U$ from the end connection, the computed property is compared with the printed one as a separate row, so that an error in a property cannot hide inside an agreeing resistance. Where a design function bundles several clauses, the clause-level functions were exposed without changing any design result, so that the shear terms, the cracked and effective inertia, the torsion terms and the column section points can each be compared with the value the example prints.

## Units and tolerances

US examples were converted exactly (1 in = 25.4 mm, 1 ksi = 6.894757 MPa, 1 kip = 4.448222 kN) and the results were converted back to the example's units before comparison. The default tolerance is 0.5 %, which is half a unit in the third significant figure that published examples print. A wider band was allowed only for three documented reasons. Some sources print fewer figures, such as a buckling reduction factor of 0.65. The plate rows are a mesh convergence study whose coarse meshes are expected to differ. The SI form of ACI 318, which Pondus implements, rounds several constants of the US form, so $0.17\sqrt{f'_c}$ MPa replaces $2\sqrt{f'_c}$ psi and gives a concrete shear strength 2.4 % higher, and each such row states the offset its constant implies.

## Recording and treatment of discrepancies

The comparisons are Rust integration tests (`core/tests/calib_analysis.rs`, `calib_steel.rs` and `calib_concrete.rs`) that assert each row against its tolerance and, when an environment variable names a directory, append the row to a CSV file. The tables and the figure in this paper are generated from those files, so the paper cannot quote a number that the tests did not produce. When a comparison failed, the cause was traced before anything was changed. A defect in Pondus was fixed, the whole test suite was run again, and the values before and after were recorded. A deliberate simplification of Pondus was kept, documented and, where it is conservative, asserted as such. A failure caused by the test model itself, such as a plate too thick for Kirchhoff tables, was corrected in the test and recorded as such.

# Results overview

Table 1 summarises the 396 comparisons by aspect, and Figure 1 shows the distribution of the absolute errors. Taken together, 319 comparisons agree within 0.1 %, 367 within 0.5 % and 379 within 1 %. The median absolute error is below 0.001 % because many comparisons, such as the 107 slab coefficients, the statically determinate frames and the Wood–Armer moments, are reproduced exactly.

<!--include:generated/summary.md-->

Table: Comparisons by suite and aspect. The pass column counts rows within their stated tolerance, and the error columns give the absolute relative error against the reference.

![Absolute relative error of every comparison, by aspect, on a logarithmic scale (exact agreements are drawn at the left edge). The dashed and dotted lines mark 0.5 % and 1 %.](figures/errors.pdf){width=95%}

Seventeen comparisons differ from their reference by more than 1 %, and they fall into three groups whose causes are understood. Nine are the coarse meshes of the plate convergence study, which converge to within 0.9 % on the finest mesh (Section 5.2). Two are the biaxial column capacities by Bresler's load contour, which lie 8.2 % and 8.4 % below the exact skewed-axis capacity, so the method Pondus uses is conservative by that margin (Section 7.3). Six are concrete quantities that depend on the rounded SI constants of ACI 318, such as the 2.4 % in $\phi V_c$ and the 6.4 % that this offset becomes in the required stirrup strength $V_s = V_u/\phi - V_c$, because the latter is a difference of two similar numbers. None of the seventeen is a defect of Pondus.

# Analysis

## Statics, supports and slab load transfer

The linear static cases reproduce their references to rounding. The ANSYS VM2 overhanging beam gives a bending stress of 11 404 psi and a mid-span deflection of 0.18246 in against the printed 11 400 psi and 0.182 in, which are themselves rounded values of $Mc/I$ and $Ml^2/8EI$. VM4 gives the bar stress and the apex deflection exactly. A propped cantilever, a fixed-ended beam, a two-span continuous beam, a Gerber beam with an internal hinge, a Pratt truss and a three-dimensional L-frame loaded out of plane agree exactly with Roark, Hibbeler, Ghali and Neville, Kassimali and McGuire, Gallagher and Ziemian [@roark; @hibbeler; @ghali; @kassimali; @mcguire]. A fixed-base portal under lateral load sways 0.012 % more than the slope-deflection formula because the formula ignores the axial strain of the columns. Springs, settlements, temperature changes and a rigid diaphragm reproduce their closed forms exactly. A two-way slab panel of 4 m × 6 m under 10 kPa sends 80 kN to each long beam and 40 kN to each short beam, as the 45° tributary lines require, and the beams' moments match $w_0(3L^2 - 4a^2)/24$ for the trapezoid and $w_0 l_x^2/12$ for the triangle.

## Plates

Table 2 reports the convergence of Pondus's plate slabs, meshed with MITC4 elements, towards the Kirchhoff solutions of @timoshenko1959 for square plates of side $a = 4$ m and thickness $a/100$ with $\nu = 0.3$. The edges are rigid beams that are free in torsion for the simply supported plate and rigid in torsion for the clamped plate. Moments converge faster than deflections, and on the finest mesh every quantity lies within 0.9 % of the table. A first attempt with $a/h = 20$ overshot the clamped deflection by 3.8 % because the Mindlin elements carry transverse shear deformation, which is physical and of order $(h/a)^2$, so the plates were made thin for the comparison. At the default slab mesh of 0.5 m, which on a 4 m panel is close to the $n = 9$ row, deflections run 3–5 % low while moments are within 1 %. Plate deflections used for serviceability should therefore be checked on a mesh of $a/16$ or finer.

| Elements across $a$ | SS $w_{max}$ | SS $M_x$ centre | Clamped $w_{max}$ | Clamped $M_x$ centre | Clamped $M_x$ mid-edge |
|---:|---:|---:|---:|---:|---:|
| 5 | −10.29 % | −2.54 % | −17.05 % | +3.09 % | |
| 9 | −3.09 % | −0.83 % | −4.68 % | +0.35 % | +9.58 % |
| 17 | −0.79 % | −0.26 % | −0.85 % | −0.51 % | +3.11 % |
| 33 | −0.13 % | −0.09 % | +0.23 % | −0.74 % | +0.88 % |

Table: Error of Pondus's plate slabs against Timoshenko and Woinowsky-Krieger (Tables 8 and 35): $w = 0.00406\,qa^4/D$ and $M_x = 0.0479\,qa^2$ for the simply supported plate, and $w = 0.00126\,qa^4/D$, $M_x = 0.0231\,qa^2$ at the centre and $-0.0513\,qa^2$ at the mid-edge for the clamped plate. The mid-edge moment is extrapolated linearly from the two elements next to the edge. A rectangular plate with $b/a = 2$ at $n = 17$ gives −0.85 %, −0.27 % and −0.53 % for $w$, $M_x$ and $M_y$.

## Modal, seismic and time-history analysis

A uniform cantilever of 32 elements gives its first three lateral frequencies within 0.001 % of the $\beta_n L$ values tabulated by @clough, and a two-storey shear frame gives both frequencies within 0.06 % and the first-mode effective mass ratio of 0.9472 within 0.04 % of the closed form in @chopra, the small residual being the axial flexibility of the columns. A flat spectrum on that frame gives a CQC base shear within 0.16 % of the hand combination. The equivalent lateral force procedure of ASCE 7-22 [@asce722] was checked on two frames, one in the velocity branch of the spectrum and one stiff enough that its period of 0.038 s lies below $T_0$, and the period, $C_s$ and the base shear are exact after the defect described in Section 8 was fixed. No freely available printed ELF example with complete input data was obtained, so these rows check the equations rather than a printed example. An undamped single-degree-of-freedom system under a step ground acceleration reaches the dynamic amplification of 2 within 0.015 % at a time step of $T_n/200$.

## Stability, plastic collapse and the other advanced analyses

Euler columns with pinned, cantilever, fixed-pinned and fixed-fixed ends give their critical loads within 0.05 % of @timoshenko1961, including the printed coefficient 20.19 for the fixed-pinned case. A fixed-base portal with equal beam and column stiffness buckles in sway within 0.001 % of the root of $x\cot x = -6/G$, the alignment-chart equation, when its members are axially rigid, and 0.36 % lower when their real axial stiffness is kept. The P-Delta amplification of a cantilever at 0.4 $P_{cr}$ matches $3(\tan u - u)/u^3$ within 0.0001 %. Five plastic collapse cases of @neal and @horne, from a simple beam to a fixed-base portal whose combined mechanism governs, are reproduced exactly by the pushover analysis, and the two-span case shows that the hinge forms at $0.414L$ inside the span rather than at an element end. A moving pair of axles gives the absolute maximum moment $P(L - s/2)^2/2L = 405$ kN·m exactly, influence lines peak where @hibbeler says they should, a cantilever propped after its dead load carries the staged moments within 0.003 %, a cable stay elongates within 0.014 % of Ernst's equivalent modulus [@ernst1965], and a beam on a Winkler foundation deflects within 0.015 % and bends within 0.09 % of @hetenyi. The tension-only brace and the lift-off support reproduce their statics exactly.

## Section properties

The torsion constants of rectangles with $b/t$ from 1 to 10 from Pondus's section finite elements lie within 0.39 % of the coefficients that @timoshenko1970 print to three figures, and the frame formula Pondus uses for rectangular members lies within 0.19 %. The warping constant of an I section and the shear centre of a thin channel are within 0.07 % of their closed forms, and the area and second moment of five rolled H shapes computed from their plates and fillets agree with JIS G 3192 [@jis3192] within 0.02 % and 0.25 %.

# Steel and Eurocode design

Table 3 lists the governing quantities of the steel examples, and the full set of 72 rows, including the intermediate values, is in the appendix. Every AISC 360 resistance agrees with the AISC Design Examples within 0.35 %, and the largest deviation of the suite, −0.60 %, is the nominal moment at the printed $L_r$ of example H.3, where the Manual's tabulated $r_{ts}$, $h_o$ and $C_w$ are not mutually exact. Pondus computes $C_b$ from the moment diagram within 0.35 % of the printed values of F.1-2B and F.1-3B. The EN 1993-1-1 classification, buckling reduction factors, elastic critical moment under a top-flange load and Annex B interaction factors agree with the SCI/CTICM and P364 examples within 0.22 %. The EN 1992-1-1 cross-check of concrete beams reproduces the JRC flexural and shear resistances within 0.14 %.

| Example | Quantity | Printed | Pondus | Error |
|------------------------------|--------------------------------------|------------:|------------:|------------:|
| AISC D.1, W8×21 | $\phi_t P_n$, yielding (kips) | 277 | 277.2 | +0.07 % |
| AISC D.1, W8×21 | $A_n$ (in²), $U$ | 4.76, 0.908 | 4.760, 0.907 | 0.00 %, −0.10 % |
| AISC D.1, W8×21 | $\phi_t P_n$, rupture (kips) | 211 | 210.5 | −0.24 % |
| AISC E.1C, W14×132 | $\phi_c P_n$ (kips) | 892 | 893.4 | +0.15 % |
| AISC E.1D, W14×90 | $\phi_c P_n$ (kips) | 927 | 927.5 | +0.06 % |
| AISC F.1-2B, W18×50 | $\phi_b M_n$ (kip-ft) | 304 | 305.0 | +0.33 % |
| AISC F.1-3B, W18×50 | $\phi_b M_n$ (kip-ft) | 288 | 287.9 | −0.03 % |
| AISC F.3B, W21×48 | $\phi_b M_n$, flange local buckling (kip-ft) | 398 | 398.0 | −0.01 % |
| AISC G.1B, W24×62 | $\phi_v V_n$ (kips) | 306 | 305.7 | −0.09 % |
| AISC H.3, W14×82 | H1-1b | 0.873 | 0.874 | +0.17 % |
| SBE A.1, IPE 330 | $M_{cr}$, top-flange load (kN·m) | 113.9 | 113.86 | −0.04 % |
| SBE A.5, HE 300 B | $\chi_z$ | 0.671 | 0.6705 | −0.07 % |
| P364 Ex. 9, UKC 356×368×129 | $N_{b,z,Rd}$ (kN) | 3678 | 3672 | −0.16 % |
| SBE-MS8 Ex. 2, IPE 450 | class under N + M, $\chi_z$ | 1, 0.865 | 1, 0.8660 | 0, +0.11 % |
| JRC EC2 §3.2.1.4 | $V_{Rd,s}$ (kN) | 115.52 | 115.36 | −0.14 % |

Table: Governing quantities of the steel and Eurocode examples [@aisc2023examples; @sbe2010; @p364; @jrc2014].

The tension rupture rows deserve a comment because they changed during the calibration. Pondus first took the effective net area equal to the gross area, so it reproduced the yielding capacity of D.1 but reported a rupture capacity of 300 kips where the example's governing value is 211 kips. It now describes the end connection of a tension member by its kind, the connected elements, the bolt diameter, the holes in each flange and in the web, the bolts per line and the connection length. From these it computes $A_n$ with holes of width $d + 1/8$ in and the shear lag factor $U$ as the larger of case 2 ($1 - \bar{x}/l$, with $\bar{x}$ of the split tee) and case 7 of AISC Table D3.1. With the connection of example D.1 it gives $A_n = 4.760$ in² and $U = 0.907$ against the printed 4.76 in² and 0.908, the small difference in $U$ coming from the fillets that the tabulated $\bar{x} = 0.831$ in includes. The same description gives the EN 1993-1-1 net section resistance $0.9 A_{net} f_u/\gamma_{M2}$.

# Concrete, timber and masonry design

## Slab coefficients

The two-way slab design of Pondus follows Method 2 of ACI 318-63 [@aci31863], the method that the Thai standard EIT 1008 adopts, and the coefficient table had been written from a secondary reproduction with only its interior-panel row confirmed. The calibration found a scan of the original Table I in lecture notes of the University of Basrah [@hussain], and every one of its 91 coefficients, for the five edge conditions, six span ratios and the long span, equals Pondus's table exactly. A further 13 rows confirm that span ratios below 0.5 take the column for 0.5 and less. Both worked examples of the notes are reproduced through Pondus's slab design within 0.06 %. The notes themselves swap two row labels and transpose one coefficient in an example, which the comparison exposed and which the calibration records. The one-way coefficients of ACI 318-19 Table 6.5.2 agree within 0.16 % with a StructurePoint reproduction of @wight Example 5-7, except where Pondus is deliberately or newly conservative, as described in Section 8.

## Beams

Flexure, shear, torsion and deflection were compared clause by clause. The INTI singly reinforced beam gives $\phi M_n$ within 0.01 %, and the required tension steel of a simply supported beam by @wight agrees within 0.02 %. The torsion terms of Wight Example 7-2, a 14 × 24 in cantilever under 28 kip-ft, agree within 0.05 % for $A_{oh}$, $p_h$, $A_o$, $\phi T_{th}$, $\phi T_{cr}$, $A_t/s$, $A_l$, the spacing limit and the cross-section limit of 22.7.7.1. The quantities that contain the rounded SI constants, $\phi V_c$, $V_s$, $A_{l,min}$ and the minimum transverse steel, differ by the offsets those constants imply and by no more. For deflection, the cracked section and cracked inertia agree within 0.05 %, while the effective inertia is the expression of ACI 318-19 Table 24.2.3.5, which the 318-14 example replaces with Branson's equation, so the effective inertia was evaluated independently and agrees exactly, and the 7.6 % gap to the example's Branson value is the intended change between editions.

## Columns and footings

The interaction diagram of a 16 × 16 in tied column with twelve #9 bars reproduces $P_0$ and $\phi P_{n,max}$ exactly and the axial forces and moments at the four printed neutral-axis depths within 0.74 %, the residue of $\beta_1 = 0.804$ in the SI expression against 0.80 in the psi expression. The search that Pondus's column design uses returns $\phi M_n$ at each printed $\phi P_n$ within 0.37 %. For biaxial bending Pondus uses Bresler's load contour [@bresler1960] with $\alpha = 1.15$ on its own uniaxial capacities, and on the example of Pincheira and co-authors reproduced by StructurePoint it gives a capacity 8.2–8.4 % below the exact skewed-axis point, which quantifies the conservatism of the method rather than an error of the code. The non-sway moment magnifier, the critical load by option (a) of 6.6.4.4.4 and the minimum moment agree within 0.85 %. The isolated footing of @wight Example 15-2 gives the moment at the column face and the reinforcement within 0.03 %.

## Plate reinforcement, timber and masonry

The Wood–Armer moments [@wood1968; @armer1968] were evaluated on seven moment fields that cover every branch of the normal and special rules, and all 28 design moments agree exactly. The NDS column stability factor of a timber column [@nds2018] agrees with the Dlubal example within 0.08 %, after the calibration noticed that the example's printed adjusted compressive strength does not follow from its own stability factor. No accessible published masonry example was obtained, so the TMS 402 flexural check [@tms402] was compared with its closed form only.

# Defects found by the calibration

Table 4 lists twelve defects with their effect on a design before the fix. Ten were found by the calibration, and two, the staged member forces and the sign of the Winkler coupling, by the closed-form tests that preceded it during development, which are included because they change design forces in the same way. Six were unconservative, three were conservative, two returned wrong results without a clear direction, and one, the engine's single coefficient of thermal expansion, overestimated restraint forces in concrete members by 20 %. Every defect was fixed and every fix was followed by a run of the whole test suite. The development tests also corrected two reporting errors that do not enter a design, a pushover capacity curve that plotted the displacement increment of each event as if it were the total and an influence line sampled beside its section, whose peak was 3.6 % low.

| No. | Defect | Where | Effect before the fix | After |
|---:|----------------------------------------|--------------|----------------------------------|--------------|
| 1 | ELF $C_s$ read from the ascending branch of the spectrum below $T_0$ | seismic | $C_s$ 0.0738 instead of 0.125, base shear **41 % low** | exact |
| 2 | moving-load envelope taken from element ends only | engine, wrapped | absolute maximum moment 400 instead of 405 kN·m, **1.2 % low** | exact |
| 3 | Winkler moduli passed to the engine as $[k_y, k_z]$ instead of $[k_z, k_y]$ | analysis wrapper | a beam on a vertical foundation had none: singular or $1.4\times10^{11}$ mm | within 0.015 % |
| 4 | one effective length for both axes | AISC and EC3 | E.1D $\phi_c P_n$ 597 instead of 927 kips, 36 % low | +0.06 % |
| 5 | EC3 class from the pure-compression web limit for any member ever compressed | EC3 wrapper | IPE 450 class 4 instead of class 1 | as printed |
| 6 | EC3 $C_2$ left equal to $C_1$ for a top-flange load | EC3 wrapper | $M_{cr}$ 78.6 instead of 113.9 kN·m, 31 % low | −0.04 % |
| 7 | EC2 cross-check took the stirrups at the main-bar grade | EC2 wrapper | $V_{Rd,s}$ **66 % high** for SR24 stirrups | exact ratio |
| 8 | tension rupture on the gross area | AISC | D.1 rupture 300 instead of 211 kips, **42 % high** | −0.24 % |
| 9 | first interior support of a one-way end span at $wl^2/10$, the value for three or more spans | slab coefficients | **10 % low** for two-span slabs | $wl^2/9$, safe for any number of spans |
| 10 | one coefficient of thermal expansion, $12\times10^{-6}$/°C, for every material | engine default | restraint forces in concrete 20 % high | per material |
| 11 | staged construction member forces from $K\cdot u$ without fixed-end forces | engine | end moment of a loaded beam −30.60 instead of −31.08 kN·m, **1.6 % low** | exact |
| 12 | sign of the $w$–$\theta_y$ coupling in the vertical Winkler foundation matrix | engine kernel | a uniformly loaded beam on a uniform foundation settled unevenly | uniform, 1.500 mm |

Table: Defects found by the calibration (1–10) and by the closed-form tests that preceded it (11, 12), with their effect before the fix. Unconservative effects are in bold.

The location of the defects is the main lesson. Only one of the twelve, the sign in the Winkler foundation matrix, was an error of a numerical kernel. The frame stiffness matrices, the solvers, the eigenvalue analysis, the plastic event-to-event algorithm and the code formulas of the engine's checkers were correct wherever they were reached. Eight defects sat in Pondus's own code that decides what the kernel is asked: which axis a modulus belongs to, which branch of a spectrum a period falls on, which length the minor axis buckles over, which web limit a beam-column is classed by, which coefficient a destabilising load takes, which strength a stirrup has, which area a bolted member ruptures across and which moment coefficient a two-span slab needs. Three of the defects, the moving-load envelope, the staged member forces and the thermal coefficient, are limitations of the vendored engine that its own suite did not expose because they only matter for the inputs a design program sends. That pattern agrees with the general experience of verification that interfaces and defaults, not algorithms, carry most of the risk [@oberkampf2010], and it matters more when an agent is the user, because an agent has no habit of distrusting a number that looks plausible.

A second lesson concerns the tests that existed before the calibration. Pondus already had tests against closed-form results, and they passed. They did not catch defect 3 because they gave the Winkler modulus as $[k, k]$, they did not catch defect 1 because no test structure was stiff enough to fall below $T_0$, and they did not catch defect 8 because no test had a bolted tension member. Published examples do better than self-written tests because their authors chose cases for engineering reasons rather than to exercise the code, and because the printed intermediate values pin each clause separately.

# Limitations of the calibration

The calibration has limits that a user of Pondus should know. The ELF vertical force distribution is not calibrated because Pondus does not compute it: it scales the response spectrum result to the ELF base shear instead. Only a single-degree-of-freedom time history was calibrated. The cables are small-displacement trusses with Ernst's modulus, so slack cables under large transverse loads are outside what Pondus solves. The plate results are Mindlin results, which exceed the Kirchhoff tables for thick slabs by a physical shear deformation and, at the default mesh, run 3–5 % low in deflection. In steel design, the AISC shear tab and the AISI cold-formed checks were not compared with printed examples because none was obtained in text form, and block shear, the tension increase of $C_b$ in H1.2, the thickness reduction of the yield strength in EN 10025 and the lateral-torsional buckling method of 6.3.2.3 are not implemented. In concrete design, Pondus has no capacity function for a given doubly reinforced section, assumes a sustained load ratio of 0.6 in the slender column stiffness, and does not design punching shear or slab deflection, and the masonry interaction check is the engine's linear sum.

The sources also have limits. Some published values reach the calibration through reproductions, such as Stabileo's records of the AISC and Eurocode examples, StructurePoint's versions of textbook examples and a lecture-note scan of the 1963 coefficient table, so a transcription error in a reproduction would pass into the reference. Where a reproduction contradicted itself, as in a misprinted timber strength, a swapped label in the slab notes and an inconsistent load table in a slenderness example, the calibration followed the calculation that the source actually performs and recorded the inconsistency. Most clauses are checked by one or two examples, which is enough to expose a wrong formula but not to prove that every branch of a long clause is right. The tolerance of 0.5 % was chosen to match the precision of printed examples, and it cannot detect an error smaller than that.

# Conclusions

Pondus reproduces 367 of 396 published and closed-form reference values within 0.5 % and 379 within 1 %, across analysis, steel, concrete, timber and masonry, and the remaining 17 differences are explained by mesh convergence, a conservative method for biaxial bending and the rounded SI constants of ACI 318. The ACI 318-63 Method 2 coefficient table, previously confirmed for one row, now agrees with the published table in all 91 coefficients. The calibration and the closed-form tests before it found and fixed twelve defects, six of them unconservative and three of them capable of changing a design by 40 % or more, and eleven of the twelve lay in the code between the model and the numerical kernels rather than in the kernels themselves.

For software that agents will drive, these results suggest a practical rule. Verification effort should follow the data across every interface, from the model to the inputs of each check, and should use published examples whose intermediate values pin each clause, because that is where errors are made and where neither the agent nor a self-written test is likely to notice them. The calibration suite is part of the program's test suite, so every future change to Pondus is checked against the same 396 references.

# Data availability

Pondus is free software under the GNU Affero General Public License, version 3 only, and its complete source, including the vendored Stabileo engine with its two marked changes, is available at https://github.com/Sompote/pondus. The calibration is reproduced by `cargo test --release -p pondus-core --test calib_analysis --test calib_steel --test calib_concrete` with `PONDUS_CALIB` set to an output directory, which writes the CSV files from which `paper/make_tables.py` generates every table and figure of this paper. The notes in `calibration/NOTES_*.md` give each source, edition and page, every tolerance and the before-and-after values of every fix.

# References

::: {#refs}
:::

\newpage

# Appendix A. All comparisons

The tables below list every comparison in the order the tests record them. The kind column distinguishes printed values (*published*), formulas of the cited source evaluated in the test (*closed-form*) and exact solutions by another method (*independent*).

## A.1 Analysis

<!--include:generated/detail_analysis.md-->

## A.2 Steel and Eurocode design

<!--include:generated/detail_steel.md-->

## A.3 Concrete, timber and masonry design

<!--include:generated/detail_concrete.md-->
