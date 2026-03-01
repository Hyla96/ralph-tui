use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
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
    #[serde(rename = "tasks")]
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone)]
pub struct Workflow {
    pub prd: PrdJson,
}

impl Workflow {
    /// Reads `prd.json` from `dir` and returns `Ok(Workflow)`.
    /// Returns `Err` if the file is missing or contains invalid JSON.
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("prd.json");
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let prd: PrdJson = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(Workflow { prd })
    }

    /// Writes `self.prd` back to `prd.json` in `dir`.
    pub fn save(&self, dir: &Path) -> Result<()> {
        let path = dir.join("prd.json");
        let json = serde_json::to_string_pretty(&self.prd)?;
        std::fs::write(&path, json)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    /// Returns the count of tasks where `passes == true`.
    pub fn done_count(&self) -> usize {
        self.prd.tasks.iter().filter(|t| t.passes).count()
    }

    /// Returns the total number of tasks.
    pub fn total_count(&self) -> usize {
        self.prd.tasks.len()
    }

    /// Returns the first task where `passes == false`, sorted by `priority` ascending.
    pub fn next_task(&self) -> Option<&Task> {
        self.prd
            .tasks
            .iter()
            .filter(|t| !t.passes)
            .min_by_key(|t| t.priority)
    }

    /// Returns `true` if all tasks have `passes == true`.
    pub fn is_complete(&self) -> bool {
        self.prd.tasks.iter().all(|t| t.passes)
    }
}
