use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Walk up the directory tree from `path` until a `.git` directory is found.
    /// Returns `Ok(Store)` with the repo root, or `Err` if no git root is found.
    pub fn find(path: &Path) -> Result<Self> {
        let mut current = path.to_path_buf();
        loop {
            if current.join(".git").exists() {
                return Ok(Store { root: current });
            }
            if !current.pop() {
                return Err(anyhow!("not inside a git repository"));
            }
        }
    }

    /// Returns `<repo_root>/.ralph/workflows/` as a `PathBuf`.
    pub fn workflows_dir(&self) -> PathBuf {
        self.root.join(".ralph").join("workflows")
    }

    /// Returns `<repo_root>/.ralph/workflows/<name>/` as a `PathBuf`.
    pub fn workflow_dir(&self, name: &str) -> PathBuf {
        self.workflows_dir().join(name)
    }

    /// Scans `.ralph/workflows/` and returns subdirectory names that contain a `prd.json`.
    /// Returns an empty vec if the directory does not exist.
    pub fn list_workflows(&self) -> Vec<String> {
        let dir = self.workflows_dir();
        if !dir.exists() {
            return vec![];
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return vec![];
        };
        let mut workflows: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .filter(|e| e.path().join("prd.json").exists())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        workflows.sort();
        workflows
    }

    /// Creates `.ralph/workflows/<name>/` and writes a starter `prd.json`.
    /// Returns `Err` if the workflow already exists or if name is invalid.
    pub fn create_workflow(&self, name: &str) -> Result<()> {
        let dir = self.workflow_dir(name);
        if dir.exists() {
            return Err(anyhow!("workflow '{}' already exists", name));
        }
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create workflow directory: {}", dir.display()))?;

        let starter = serde_json::json!({
            "project": name,
            "branchName": "",
            "description": "",
            "validationCommands": [],
            "tasks": []
        });
        let json = serde_json::to_string_pretty(&starter)?;
        std::fs::write(dir.join("prd.json"), json)
            .with_context(|| "failed to write starter prd.json")?;
        Ok(())
    }

    /// Returns `true` only if `name` matches `[a-z0-9-]{3,64}`.
    pub fn is_valid_name(name: &str) -> bool {
        let len = name.len();
        if !(3..=64).contains(&len) {
            return false;
        }
        name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    }

    /// Returns the repo root path.
    pub fn root(&self) -> &Path {
        &self.root
    }
}
