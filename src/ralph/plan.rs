use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserStory {
    pub id: String,
    pub title: String,
    pub description: String,
    #[serde(rename = "acceptanceCriteria")]
    pub acceptance_criteria: Vec<String>,
    pub priority: u32,
    pub passes: bool,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrdJson {
    pub project: String,
    #[serde(rename = "branchName")]
    pub branch_name: String,
    pub description: String,
    #[serde(rename = "validationCommands")]
    pub validation_commands: Vec<String>,
    #[serde(rename = "userStories")]
    pub user_stories: Vec<UserStory>,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub prd: PrdJson,
}

impl Plan {
    /// Reads `prd.json` from `dir` and returns `Ok(Plan)`.
    /// Returns `Err` if the file is missing or contains invalid JSON.
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("prd.json");
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let prd: PrdJson = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(Plan { prd })
    }

    /// Writes `self.prd` back to `prd.json` in `dir`.
    pub fn save(&self, dir: &Path) -> Result<()> {
        let path = dir.join("prd.json");
        let json = serde_json::to_string_pretty(&self.prd)?;
        std::fs::write(&path, json)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    /// Returns the count of stories where `passes == true`.
    pub fn done_count(&self) -> usize {
        self.prd.user_stories.iter().filter(|s| s.passes).count()
    }

    /// Returns the total number of stories.
    pub fn total_count(&self) -> usize {
        self.prd.user_stories.len()
    }

    /// Returns the first story where `passes == false`, sorted by `priority` ascending.
    pub fn next_story(&self) -> Option<&UserStory> {
        self.prd
            .user_stories
            .iter()
            .filter(|s| !s.passes)
            .min_by_key(|s| s.priority)
    }

    /// Returns `true` if all stories have `passes == true`.
    pub fn is_complete(&self) -> bool {
        self.prd.user_stories.iter().all(|s| s.passes)
    }
}
