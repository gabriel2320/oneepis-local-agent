use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceDocument {
    pub path: String,
    pub title: String,
    pub sha256: String,
    pub bytes: usize,
    pub present: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInspection {
    pub repo_path: String,
    pub project_name: String,
    pub is_git_repo: bool,
    pub is_one_epis: bool,
    pub current_branch: String,
    pub dirty: bool,
    pub status_text: String,
    pub governance_documents: Vec<GovernanceDocument>,
    pub declared_gates: Vec<String>,
    pub detected_rules: Vec<String>,
    pub blocks: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoCheckout {
    pub repo_url: String,
    pub workspace_path: String,
    pub repo_path: String,
    pub action: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaModel {
    pub name: String,
    pub size: u64,
    pub family: String,
    pub parameters: String,
    pub quantization: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPolicy {
    pub primary_code: String,
    pub fast_code: String,
    pub governance: String,
    pub fallback: String,
    pub embeddings: String,
}

impl Default for ModelPolicy {
    fn default() -> Self {
        Self {
            primary_code: std::env::var("AGENT_PRIMARY_CODE_MODEL")
                .unwrap_or_else(|_| "qwen2.5-coder:14b".to_string()),
            fast_code: std::env::var("AGENT_FAST_CODE_MODEL")
                .unwrap_or_else(|_| "qwen2.5-coder:7b".to_string()),
            governance: std::env::var("AGENT_GOVERNANCE_MODEL")
                .unwrap_or_else(|_| "qwen3:8b".to_string()),
            fallback: std::env::var("AGENT_FALLBACK_MODEL")
                .unwrap_or_else(|_| "llama3.2:latest".to_string()),
            embeddings: std::env::var("AGENT_EMBEDDINGS_MODEL")
                .unwrap_or_else(|_| "bge-m3:latest".to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaStatus {
    pub base_url: String,
    pub available: bool,
    pub message: String,
    pub models: Vec<OllamaModel>,
    pub policy: ModelPolicy,
    pub missing_policy_models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroPlan {
    #[serde(default)]
    pub objective: String,
    #[serde(default, alias = "recommended_gate")]
    pub recommended_gate: String,
    #[serde(default)]
    pub steps: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub blocked: bool,
    #[serde(default, alias = "model_used")]
    pub model_used: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStep {
    pub order: usize,
    pub state: String,
    pub status: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextWork {
    pub kind: String,
    pub title: String,
    pub rationale: String,
    pub gate: String,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentTask {
    pub id: String,
    pub title: String,
    pub surface: String,
    pub risk: String,
    pub rationale: String,
    pub files: Vec<String>,
    pub required_gate: String,
    pub allowed_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchEdit {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub original: String,
    #[serde(default)]
    pub replacement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchPlan {
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub branch_name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub edits: Vec<PatchEdit>,
    #[serde(default)]
    pub forbidden_edits: Vec<String>,
    #[serde(default)]
    pub expected_gate: String,
    #[serde(default)]
    pub model_used: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalCommitResult {
    pub branch: String,
    pub commit_sha: String,
    pub status: String,
    pub gate_command: Vec<String>,
    pub gate_output_summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRun {
    pub id: String,
    pub repo_path: String,
    pub objective: String,
    pub branch: String,
    pub status: String,
    pub mode: String,
    pub model_used: String,
    pub started_at: String,
    pub completed_at: String,
    pub steps: Vec<AgentStep>,
    pub plan: MicroPlan,
    pub checkout: Option<RepoCheckout>,
    pub next_work: Option<NextWork>,
    pub task: Option<DevelopmentTask>,
    pub patch_plan: Option<PatchPlan>,
    pub commit_result: Option<LocalCommitResult>,
    pub changed_files: Vec<String>,
    pub lessons: Vec<String>,
    pub persistence: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunRequest {
    pub repo_path: String,
    pub objective: String,
    pub max_cycles: Option<u8>,
    pub mode: Option<String>,
    pub database_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AutopilotRequest {
    pub workspace_path: Option<String>,
    pub repo_url: Option<String>,
    pub objective: Option<String>,
    pub max_cycles: Option<u8>,
    pub mode: Option<String>,
    pub database_url: Option<String>,
}
