# Burj Khalifa: a simplified model in Pondus

A simplified structural model of the Burj Khalifa (Dubai, SOM, 2010), built in Pondus through its MCP server by `build_burj.py`, as an agent would build it, and analysed for its modes. Open any of the `.pondus` files in Pondus to look at it.

| File | Model |
|---|---|
| `burj_khalifa_fixed.pondus` | fixed base, gross sections |
| `burj_khalifa_070Ig.pondus` | fixed base, walls and columns at 0.70 Ig (ACI 318-19 6.6.3.1.1) |
| `burj_khalifa_raft_static.pondus` | the piled raft as vertical springs that settle 57.5 mm under the weight |

The model has 84 walls (a hexagonal core, two corridor walls and a hammerhead wall in each of the three wings, with 12 setbacks), 2,286 frame members (perimeter and nose columns, floor strips that carry the floor mass, outriggers at five mechanical levels) and rigid diaphragms at 78 levels of two storeys each, up to the top of the concrete at 586 m. Its seismic weight is 3.34 × 10⁶ kN, the 340,000 t of the superstructure with the 4,000 t steel spire lumped at the top.

**From the published record:** the buttressed-core system, the height and storeys, wings about 25 m wide reaching 50 m from the centre (the raft wings are 50 × 25 m), core walls 1300 to 500 mm, corridor walls and perimeter columns 600 mm, C80 concrete with E = 43,800 MPa, the building and spire weights, and the raft settlement. **Assumed** (no published value was found): the core and corridor dimensions, the hammerhead and outrigger proportions and the setback heights (12 setbacks of one 9 m bay instead of 27).

| Model | T1 (s) | T2 (s) |
|---|---:|---:|
| fixed base, gross sections | 6.93 | 6.23 |
| fixed base, 0.70 Ig | 8.24 | 7.42 |
| piled raft springs (static stiffness) | 7.04 | 6.35 |
| SOM's analysis (Baker, Korista and Novak 2007) | 11.3 | 10.2 |

The model is too stiff: its periods are 27 % to 39 % below SOM's. A hand estimate from the same section gives 7.0 s, so the difference lies in the simplifications rather than in the solver. The likely causes are the corridor walls, which are solid here but coupled through link beams in the real tower, and the assumed wall and core dimensions. The foundation springs lengthen the period by only 1.6 %, because each pier keeps its rotation fixed to the raft. This is an illustration of what Pondus can model, not a validation, and it is not used in the paper.

Sources: Baker, Korista and Novak (2007), The Structural Design of Tall and Special Buildings 16(4); the CTBUH and ICC papers by SOM's engineers; Poulos (2009) and the ELPLA case study for the raft and its settlement; Wikipedia for the heights and weights.
