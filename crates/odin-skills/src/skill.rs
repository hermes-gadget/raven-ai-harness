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
            enabled: true,
            source_path: None,
        });
        registry.register(Skill {
            name: "disabled-skill".into(),
            description: "".into(),
            content: "".into(),
            required_tools: vec![],
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
            enabled: true,
            source_path: None,
        });

        assert!(registry.has("temp"));
        let removed = registry.remove("temp");
        assert!(removed.is_some());
        assert!(!registry.has("temp"));
        assert!(registry.is_empty());
    }
}
