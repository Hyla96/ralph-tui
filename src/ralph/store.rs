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

    /// Returns `<repo_root>/.ralph/plans/` as a `PathBuf`.
    pub fn plans_dir(&self) -> PathBuf {
        self.root.join(".ralph").join("plans")
    }

    /// Returns `<repo_root>/.ralph/plans/<name>/` as a `PathBuf`.
    pub fn plan_dir(&self, name: &str) -> PathBuf {
        self.plans_dir().join(name)
    }

    /// Scans `.ralph/plans/` and returns subdirectory names that contain a `prd.json`.
    /// Returns an empty vec if the directory does not exist.
    pub fn list_plans(&self) -> Vec<String> {
        let dir = self.plans_dir();
        if !dir.exists() {
            return vec![];
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return vec![];
        };
        let mut plans: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .filter(|e| e.path().join("prd.json").exists())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        plans.sort();
        plans
    }

    /// Creates `.ralph/plans/<name>/` and writes a starter `prd.json`.
    /// Returns `Err` if the plan already exists or if name is invalid.
    pub fn create_plan(&self, name: &str) -> Result<()> {
        let dir = self.plan_dir(name);
        if dir.exists() {
            return Err(anyhow!("plan '{}' already exists", name));
        }
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create plan directory: {}", dir.display()))?;

        let starter = serde_json::json!({
            "project": name,
            "branchName": "",
            "description": "",
            "validationCommands": [],
            "userStories": []
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
