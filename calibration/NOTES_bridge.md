# Bridge under moving load: calibration notes

Suite `core/tests/calib_bridge.rs`, written to `bridge.csv` when `PONDUS_CALIB` names a directory.

## Source

FHWA Steel Bridge Design Handbook, Design Example 2A: Two-Span Continuous Straight Composite Steel
I-Girder Bridge, FHWA-HIF-16-002 Vol. 21, December 2015 (AASHTO LRFD 7th edition, 2014). Obtained
from the copy at user.eng.umd.edu/~ccfu/ref717/designexample02.pdf (114 pages).

- Two spans of 90 ft, four girders at 10 ft; Section 1 (positive bending) from the abutments to
  63 ft, Section 2 from 63 to 117 ft (27 ft either side of the pier), Sec. 3.0 and Figure 3.
- The moment envelopes use the short-term composite section with the deck effective over the whole
  length (Sec. 6.1); p. 53 gives those moments of inertia, 48,806 in.4 (positive region) and
  50,027 in.4 (pier region). Only their ratio enters the moments.
- Table 6 (p. 27 of the example, PDF page 36): unfactored and undistributed live load moments in
  kip-ft at the tenth points of span 1: vehicle positive and negative, special negative, standard
  negative, 1.33 vehicle + lane positive. The distribution factor (0.86) is not used here.

## How Pondus is used

- The girder is 20 members of 9 ft, so every tenth point and both section changes are nodes; pinned
  at the left abutment, rollers at the pier and the right abutment; E = 29,000 ksi (immaterial).
- Vehicle envelopes: Pondus's moving-load analysis (`advanced::moving_loads`) at a 0.5 ft step for
  the design truck with rear spacing 14, 16, ..., 30 ft in both directions and the tandem (25 kip
  axles 4 ft apart); the envelope is the extreme over those 19 trains.
- Special negative loading (Art. 3.6.1.3.1): two design trucks, rear spacing 14 ft, gaps of 50 to
  100 ft, both directions, then 0.9 (1.33 M_trucks + M_lane).
- Lane load 0.64 klf: on the parts of Pondus's influence line (`advanced::influence`) with the sign
  wanted, as Art. 3.6.1.2.4 allows ("need not be applied continuously").
- Tolerance: 0.5 %, or the rounding of a value printed to the nearest kip-ft.

## Results

40 comparisons, all within tolerance: median |error| 0.045 %, largest 0.65 %. Every vehicle
envelope value is within 0.16 %, and the pier moment under the special loading is -2020.1 against
-2020 kip-ft.

An independent check (a beam finite-element influence line in Python at 0.1-0.25 ft resolution,
the same two moments of inertia, every truck position) reproduces the table's vehicle envelope
(1082.8, 1059.5, 950.9, 743.3, 463.3 kip-ft at 0.4L-0.8L against 1083, 1059, 951, 743, 463), the
two-truck effects at 0.9L and the pier (-755.6, -1196.7), and the lane on the positive part of the
influence line at 0.9L (31.53 kip-ft).

## Values of the table not compared

- Special negative at 0.1L-0.7L (-29 rising linearly to -206): the special loading applies only
  between the points of dead-load contraflexure, and these values are no two-truck placement.
- Special negative at 0.8L (-967): it would need a two-truck effect of -611 kip-ft, but no
  placement of two trucks (gaps 50 to 250 ft, both directions, independent search) gives more
  than -479.5, the single-truck value. Left out as an apparent error of the table.
- 1.33 vehicle + lane at 0.9L (285) is compared at 1 %: the lane on the positive part of the
  influence line carries 31.53 kip-ft, the table implies 29.6, a coarser lane placement.
- The pier rotation of p. 53 (32.88 mrad with the spans hinged and 5365 kip-ft each side) was not
  reproduced: the two moments of inertia give 32.22 mrad and a prismatic 48,806 in.4 girder
  32.75 mrad, so the example used assumptions it does not state.

## Defect found: the moving-load envelope one element away (defect 14)

Before the fix, Pondus read the envelope at a station as the extreme of the engine's per-element
envelope over the elements touching the station. The engine takes each element's extreme over both
its ends, so a station carried the value one step away. Where the envelope changes fast this
overestimated it: vehicle M+ at 0.6L 958.7 instead of 951 kip-ft (+0.81 %), at 0.5L 1061.4
instead of 1059 (+0.23 %). It was conservative (the envelope too wide), but it was an error.

Fix: the engine keeps the envelope at each end of an element as well (`[Pondus]` change in
`engine/src/solver/moving_loads.rs`), Pondus reads the station's value exactly, interpolates
between the ends of the element a station lies inside, and gives an extreme that lies between two
stations to the nearer of them, so that the absolute maximum moment of a simple span (Hibbeler,
405 kN·m at L/2 - s/4) is still found. After the fix: 950.87 at 0.6L (-0.013 %).
