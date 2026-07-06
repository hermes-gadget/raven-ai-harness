//! Odin Skills — Reusable workflows loaded from markdown files.
//!
//! Skills are structured markdown files with YAML frontmatter that describe
//! reusable workflows. The `SkillRegistry` loads all skills from a directory
//! and indexes them by name for fast lookup.

pub mod skill;

pub use skill::{Skill, SkillFrontmatter, SkillRegistry};
