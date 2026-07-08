//! Odin Loop — The core agent loop engine.
//!
//! Implements the 7-phase agent loop:
//!   PLAN → ACT → INSPECT → CRITIQUE → REVISE → VERIFY → CONTINUE/STOP
//!
//! Designed to help smaller/cheaper/local models succeed through
//! decomposition, self-checking, retry logic, and escalation.

pub mod confidence;
pub mod decomposer;
pub mod engine;
pub mod phases;
pub mod summarizer;

pub use confidence::ConfidenceScorer;
pub use decomposer::GoalDecomposer;
pub use engine::Engine as LoopEngine;
pub use phases::{
    ActPhase, CritiquePhase, DecidePhase, InspectPhase, Phase, PhaseContext, PlanPhase,
    RevisePhase, VerifyPhase,
};
pub use summarizer::StateSummarizer;
