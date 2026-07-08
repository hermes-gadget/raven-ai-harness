//! Skill definitions and the SkillRegistry loader.
//!
//! Skills are loaded from markdown files with YAML frontmatter:
//! ```markdown
//! ---
//! name: my-skill
//! description: A helpful workflow
//! required_tools:
//!   - file_read
//!   - file_write
//! enabled: true
//! ---
//!
//! ## Instructions
//! Follow these steps...
//! ```

use async_trait::async_trait;
use odin_core::error::{OdinError, OdinResult};
use odin_core::traits::Skill as SkillTrait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A skill loaded from a markdown file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Unique skill name (derived from the file or frontmatter).
    pub name: String,

    /// Human-readable description.
    pub description: String,

    /// The markdown content body (instructions).
    pub content: String,

    /// Required tool names for this skill.
    #[serde(default)]
    pub required_tools: Vec<String>,

    /// Recommended (optional) tool names for this skill.
    #[serde(default)]
    pub recommended_tools: Vec<String>,

    /// Whether this skill is enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Source file path.
    #[serde(skip)]
    pub source_path: Option<PathBuf>,
}

fn default_enabled() -> bool {
    true
}

/// Frontmatter parsed from a skill markdown file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    /// Unique skill name.
    pub name: String,

    /// Human-readable description.
    pub description: String,

    /// Required tool names.
    #[serde(default)]
    pub required_tools: Vec<String>,

    /// Recommended (optional) tool names.
    #[serde(default)]
    pub recommended_tools: Vec<String>,

    /// Whether this skill is enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[async_trait]
impl SkillTrait for Skill {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    async fn load(&self) -> OdinResult<String> {
        Ok(self.content.clone())
    }

    fn required_tools(&self) -> Vec<String> {
        self.required_tools.clone()
    }

    fn recommended_tools(&self) -> Vec<String> {
        self.recommended_tools.clone()
    }

    fn enabled(&self) -> bool {
        self.enabled
    }
}

impl Skill {
    /// Parse a skill from markdown content with YAML frontmatter.
    ///
    /// The expected format is:
    /// ```markdown
    /// ---
    /// name: my-skill
    /// description: ...
    /// ---
    /// Body content...
    /// ```
    pub fn from_markdown(content: &str, source_path: Option<PathBuf>) -> OdinResult<Self> {
        // Split on `---` frontmatter delimiters
        let content = content.trim();

        if let Some(rest) = content.strip_prefix("---") {
            // Find the closing `---`
            if let Some(end_idx) = rest.find("\n---") {
                let frontmatter_str = &rest[..end_idx];
                let body_start = end_idx + 4; // skip the `\n---`
                let body = rest[body_start..].trim();

                let frontmatter: SkillFrontmatter =
                    serde_yaml::from_str(frontmatter_str).map_err(|e| {
                        OdinError::Config(format!("Failed to parse skill frontmatter: {e}"))
                    })?;

                if frontmatter.name.is_empty() {
                    return Err(OdinError::Config(
                        "Skill frontmatter must specify a 'name'".into(),
                    ));
                }

                return Ok(Self {
                    name: frontmatter.name,
                    description: frontmatter.description,
                    content: body.to_string(),
                    required_tools: frontmatter.required_tools,
                    recommended_tools: frontmatter.recommended_tools,
                    enabled: frontmatter.enabled,
                    source_path,
                });
            }
        }

        // No valid frontmatter — derive name from source path
        let name = source_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_string();

        Ok(Self {
            name,
            description: String::new(),
            content: content.to_string(),
            required_tools: vec![],
            recommended_tools: vec![],
            enabled: true,
            source_path,
        })
    }

    /// Render the skill as markdown (frontmatter + body).
    pub fn to_markdown(&self) -> OdinResult<String> {
        let fm = SkillFrontmatter {
            name: self.name.clone(),
            description: self.description.clone(),
            required_tools: self.required_tools.clone(),
            recommended_tools: self.recommended_tools.clone(),
            enabled: self.enabled,
        };
        let fm_str = serde_yaml::to_string(&fm).map_err(|e| {
            OdinError::Serialization(serde_json::Error::io(std::io::Error::other(format!(
                "Failed to serialize frontmatter: {e}"
            ))))
        })?;
        Ok(format!("---\n{}---\n{}", fm_str, self.content))
    }
}

// ── SkillRegistry ────────────────────────────────────────────────────

/// A registry that discovers and indexes skills from a directory.
#[derive(Debug, Clone)]
pub struct SkillRegistry {
    /// Skills indexed by name.
    skills: HashMap<String, Skill>,

    /// The directory from which skills were loaded.
    skills_dir: Option<PathBuf>,
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillRegistry {
    /// Create an empty skill registry.
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
            skills_dir: None,
        }
    }

    /// Load all `.md` and `.skill.md` files from the given directory.
    pub fn load_from_dir(dir: &Path) -> OdinResult<Self> {
        if !dir.exists() {
            return Err(OdinError::Config(format!(
                "Skills directory does not exist: {}",
                dir.display()
            )));
        }
        if !dir.is_dir() {
            return Err(OdinError::Config(format!(
                "Skills path is not a directory: {}",
                dir.display()
            )));
        }

        let mut registry = Self::new();
        registry.skills_dir = Some(dir.to_path_buf());

        let mut read_dir = std::fs::read_dir(dir).map_err(|e| {
            OdinError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to read skills dir {}: {e}", dir.display()),
            ))
        })?;

        while let Some(entry) = read_dir.next().transpose()? {
            let path = entry.path();

            // Only process .md files (or .skill.md files)
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                match Skill::from_file(&path) {
                    Ok(skill) => {
                        tracing::info!("Loaded skill '{}' from {}", skill.name, path.display());
                        registry.skills.insert(skill.name.clone(), skill);
                    }
                    Err(e) => {
                        tracing::warn!("Skipped {}: {e}", path.display());
                    }
                }
            }
        }

        tracing::info!(
            "Loaded {} skill(s) from {}",
            registry.skills.len(),
            dir.display()
        );

        Ok(registry)
    }

    /// Get a skill by name.
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    /// Get all skills.
    pub fn all(&self) -> Vec<&Skill> {
        self.skills.values().collect()
    }

    /// Get all enabled skills.
    pub fn enabled(&self) -> Vec<&Skill> {
        self.skills.values().filter(|s| s.enabled).collect()
    }

    /// Check if a skill exists by name.
    pub fn has(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }

    /// Register a skill programmatically.
    pub fn register(&mut self, skill: Skill) {
        tracing::info!("Registered skill '{}'", skill.name);
        self.skills.insert(skill.name.clone(), skill);
    }

    /// Remove a skill by name.
    pub fn remove(&mut self, name: &str) -> Option<Skill> {
        let skill = self.skills.remove(name);
        if skill.is_some() {
            tracing::info!("Removed skill '{name}'");
        }
        skill
    }

    /// Get the number of loaded skills.
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// Get the source directory.
    pub fn skills_dir(&self) -> Option<&Path> {
        self.skills_dir.as_deref()
    }

    /// Get the required and recommended tools for a skill by name.
    pub fn tools_for_skill(&self, name: &str) -> Option<SkillTools> {
        self.skills.get(name).map(|s| SkillTools {
            skill_name: s.name.clone(),
            required: s.required_tools.clone(),
            recommended: s.recommended_tools.clone(),
        })
    }

    /// Validate all skills against a set of available tool names.
    ///
    /// Returns warnings for skills whose required tools are unavailable
    /// and notes for skills whose recommended tools are unavailable.
    pub fn validate_tools(&self, available_tools: &[String]) -> Vec<SkillValidation> {
        let mut results = Vec::new();
        for skill in self.skills.values() {
            let missing_required: Vec<String> = skill
                .required_tools
                .iter()
                .filter(|t| !available_tools.contains(t))
                .cloned()
                .collect();
            let missing_recommended: Vec<String> = skill
                .recommended_tools
                .iter()
                .filter(|t| !available_tools.contains(t))
                .cloned()
                .collect();
            if !missing_required.is_empty() || !missing_recommended.is_empty() {
                let has_errors = !skill.required_tools.is_empty()
                    && missing_required.len() == skill.required_tools.len();
                results.push(SkillValidation {
                    skill_name: skill.name.clone(),
                    missing_required,
                    missing_recommended,
                    has_errors,
                });
            }
        }
        results
    }
}

// ── File loading helpers ─────────────────────────────────────────────

impl Skill {
    /// Load a skill from a markdown file.
    pub fn from_file(path: &Path) -> OdinResult<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            OdinError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to read skill file {}: {e}", path.display()),
            ))
        })?;
        Self::from_markdown(&content, Some(path.to_path_buf()))
    }
}

// ── Skill-Tool Wiring Types ──────────────────────────────────────────

/// Required and recommended tools for a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTools {
    pub skill_name: String,
    pub required: Vec<String>,
    pub recommended: Vec<String>,
}

/// Validation result for a skill's tool dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillValidation {
    pub skill_name: String,
    pub missing_required: Vec<String>,
    pub missing_recommended: Vec<String>,
    /// True if all required tools are missing (skill effectively unusable).
    pub has_errors: bool,
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_from_markdown_with_frontmatter() {
        let md = r#"---
name: code-review
description: Perform a thorough code review
required_tools:
  - file_read
  - git
enabled: true
---

## Steps

1. Read the diff
2. Check for bugs
3. Verify tests pass
"#;

        let skill = Skill::from_markdown(md, None).unwrap();
        assert_eq!(skill.name, "code-review");
        assert_eq!(skill.description, "Perform a thorough code review");
        assert_eq!(skill.required_tools, vec!["file_read", "git"]);
        assert!(skill.recommended_tools.is_empty());
        assert!(skill.enabled);
        assert!(skill.content.contains("## Steps"));
    }

    #[test]
    fn test_skill_from_markdown_without_frontmatter() {
        let md = "# Just content\n\nNo frontmatter here.";
        let skill = Skill::from_markdown(md, Some(PathBuf::from("my-skill.md"))).unwrap();
        assert_eq!(skill.name, "my-skill");
        assert_eq!(skill.description, "");
        assert!(skill.content.contains("Just content"));
    }

    #[test]
    fn test_skill_round_trip() {
        let skill = Skill {
            name: "test".into(),
            description: "A test skill".into(),
            content: "# Body\n\nDo the thing.".into(),
            required_tools: vec!["shell".into()],
            recommended_tools: vec!["git".into()],
            enabled: true,
            source_path: None,
        };

        let md = skill.to_markdown().unwrap();
        let parsed = Skill::from_markdown(&md, None).unwrap();

        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.description, "A test skill");
        assert_eq!(parsed.required_tools, vec!["shell"]);
        assert!(parsed.content.contains("Do the thing."));
    }

    #[test]
    fn test_registry_load_from_dir() {
        let dir = tempfile::tempdir().unwrap();

        // Create a skill file
        let file1 = dir.path().join("code-review.md");
        std::fs::write(
            &file1,
            r#"---
name: code-review
description: Review code
---

Check the PR.
"#,
        )
        .unwrap();

        // Create a non-skill markdown file
        let file2 = dir.path().join("readme.md");
        std::fs::write(&file2, "# README\n\nJust info.").unwrap();

        let registry = SkillRegistry::load_from_dir(dir.path()).unwrap();
        assert_eq!(registry.len(), 2);
        assert!(registry.has("code-review"));
        assert!(registry.has("readme"));
    }

    #[test]
    fn test_registry_get_and_register() {
        let mut registry = SkillRegistry::new();

        let skill = Skill {
            name: "my-skill".into(),
            description: "Test".into(),
            content: "body".into(),
            required_tools: vec![],
            recommended_tools: vec![],
            enabled: true,
            source_path: None,
        };

        registry.register(skill);
        assert!(registry.has("my-skill"));
        assert_eq!(registry.len(), 1);

        let loaded = registry.get("my-skill").unwrap();
        assert_eq!(loaded.name, "my-skill");
    }

    #[test]
    fn test_registry_enabled_filter() {
        let mut registry = SkillRegistry::new();

        registry.register(Skill {
            name: "enabled-skill".into(),
            description: "".into(),
            content: "".into(),
            required_tools: vec![],
            recommended_tools: vec![],
            enabled: true,
            source_path: None,
        });
        registry.register(Skill {
            name: "disabled-skill".into(),
            description: "".into(),
            content: "".into(),
            required_tools: vec![],
            recommended_tools: vec![],
            enabled: false,
            source_path: None,
        });

        assert_eq!(registry.all().len(), 2);
        assert_eq!(registry.enabled().len(), 1);
        assert_eq!(registry.enabled()[0].name, "enabled-skill");
    }

    #[test]
    fn test_registry_remove() {
        let mut registry = SkillRegistry::new();
        registry.register(Skill {
            name: "temp".into(),
            description: "".into(),
            content: "".into(),
            required_tools: vec![],
            recommended_tools: vec![],
            enabled: true,
            source_path: None,
        });

        assert!(registry.has("temp"));
        let removed = registry.remove("temp");
        assert!(removed.is_some());
        assert!(!registry.has("temp"));
        assert!(registry.is_empty());
    }

    #[test]
    fn test_recommended_tools_in_frontmatter() {
        let md = r#"---
name: deploy
description: Deploy to production
required_tools:
  - shell
  - git
recommended_tools:
  - github_pr_status
  - system_info
enabled: true
---

## Steps
1. Check status
2. Deploy
"#;
        let skill = Skill::from_markdown(md, None).unwrap();
        assert_eq!(skill.required_tools, vec!["shell", "git"]);
        assert_eq!(
            skill.recommended_tools,
            vec!["github_pr_status", "system_info"]
        );
    }

    #[test]
    fn test_tools_for_skill() {
        let mut registry = SkillRegistry::new();
        registry.register(Skill {
            name: "analyze".into(),
            description: "".into(),
            content: "".into(),
            required_tools: vec!["file_read".into(), "shell".into()],
            recommended_tools: vec!["git".into()],
            enabled: true,
            source_path: None,
        });

        let tools = registry.tools_for_skill("analyze").unwrap();
        assert_eq!(tools.skill_name, "analyze");
        assert_eq!(tools.required, vec!["file_read", "shell"]);
        assert_eq!(tools.recommended, vec!["git"]);

        assert!(registry.tools_for_skill("nonexistent").is_none());
    }

    #[test]
    fn test_validate_tools_missing_required() {
        let mut registry = SkillRegistry::new();
        registry.register(Skill {
            name: "needs-shell".into(),
            description: "".into(),
            content: "".into(),
            required_tools: vec!["shell".into(), "missing_tool".into()],
            recommended_tools: vec![],
            enabled: true,
            source_path: None,
        });

        let available = vec!["shell".to_string(), "file_read".to_string()];
        let results = registry.validate_tools(&available);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].skill_name, "needs-shell");
        assert_eq!(results[0].missing_required, vec!["missing_tool"]);
        assert!(!results[0].has_errors); // not all missing — "shell" is available
    }

    #[test]
    fn test_validate_tools_all_required_missing() {
        let mut registry = SkillRegistry::new();
        registry.register(Skill {
            name: "broken".into(),
            description: "".into(),
            content: "".into(),
            required_tools: vec!["nonexistent".into()],
            recommended_tools: vec![],
            enabled: true,
            source_path: None,
        });

        let available = vec!["shell".to_string()];
        let results = registry.validate_tools(&available);
        assert_eq!(results.len(), 1);
        assert!(results[0].has_errors); // all required tools missing
    }

    #[test]
    fn test_validate_tools_missing_recommended_only() {
        let mut registry = SkillRegistry::new();
        registry.register(Skill {
            name: "ok".into(),
            description: "".into(),
            content: "".into(),
            required_tools: vec!["shell".into()],
            recommended_tools: vec!["nice_to_have".into()],
            enabled: true,
            source_path: None,
        });

        let available = vec!["shell".to_string()];
        let results = registry.validate_tools(&available);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].missing_recommended, vec!["nice_to_have"]);
        assert!(!results[0].has_errors); // required tools all present
    }

    #[test]
    fn test_validate_tools_no_issues() {
        let mut registry = SkillRegistry::new();
        registry.register(Skill {
            name: "good".into(),
            description: "".into(),
            content: "".into(),
            required_tools: vec!["shell".into()],
            recommended_tools: vec!["git".into()],
            enabled: true,
            source_path: None,
        });

        let available = vec!["shell".to_string(), "git".to_string()];
        let results = registry.validate_tools(&available);
        assert!(results.is_empty());
    }
}
