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
pub mod small_model;
pub mod summarizer;

pub use confidence::ConfidenceScorer;
pub use decomposer::GoalDecomposer;
pub use engine::Engine as LoopEngine;
pub use phases::{
    ActPhase, CritiquePhase, DecidePhase, InspectPhase, Phase, PhaseContext, PlanPhase,
    RevisePhase, VerifyPhase,
};
pub use small_model::{
    AdaptiveExecutionPolicy, DistilledContext, EvidenceCheck, ExecutionMode, FailureKind,
    PromptStyle, SmallModelProfile, TaskComplexity, ToolArgRepair, ToolComplexity,
    classify_tool_failure, distill_context, parse_plan_response, repair_tool_argument_value,
    repair_tool_arguments_once, verify_evidence,
};
pub use summarizer::StateSummarizer;
