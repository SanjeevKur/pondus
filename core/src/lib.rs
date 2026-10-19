// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Pondus core: the structural model and everything that works on it without a window
//! (analysis with the vendored Stabileo engine, ACI 318-19 concrete and AISC 360-16 steel
//! design, the Stabileo and BIM exchange, the MCP server), so the agent, the CLI and the app
//! share one implementation.

pub mod about;
pub mod advanced;
pub mod analysis;
pub mod check;
pub mod codes;
pub mod commands;
pub mod connections;
pub mod design;
pub mod dxf;
pub mod excel;
pub mod footing;
pub mod ifc;
pub mod mcp;
pub mod model;
pub mod othermat;
pub mod picture;
pub mod report;
pub mod rc;
pub mod scene;
pub mod seismic;
pub mod secfe;
pub mod sections;
pub mod slabfe;
pub mod stability;
pub mod slabload;
pub mod stabileo;
pub mod steel;
pub mod timehistory;
pub mod transform;

pub use model::*;
