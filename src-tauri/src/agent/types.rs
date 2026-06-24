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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessCheck {
    pub name: String,
    pub status: String,
    pub detail: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedMicrocycle {
    pub title: String,
    pub objective: String,
    pub risk_level: String,
    pub gates: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentReadiness {
    pub repo_path: String,
    pub profile: String,
    pub status: String,
    pub summary: String,
    pub checks: Vec<ReadinessCheck>,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub next_actions: Vec<String>,
    pub suggested_microcycles: Vec<SuggestedMicrocycle>,
    pub required_gates: Vec<String>,
    pub local_model_summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkPackageTest {
    pub gate: String,
    pub command: String,
    pub purpose: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentWorkPackage {
    pub repo_path: String,
    pub title: String,
    pub objective: String,
    pub status: String,
    pub summary: String,
    pub branch_strategy: String,
    pub files_to_inspect: Vec<String>,
    pub implementation_steps: Vec<String>,
    pub test_plan: Vec<WorkPackageTest>,
    pub acceptance_criteria: Vec<String>,
    pub stop_conditions: Vec<String>,
    pub gates: Vec<String>,
    pub warnings: Vec<String>,
    pub can_draft: bool,
    pub can_apply: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextPackFile {
    pub path: String,
    pub kind: String,
    pub bytes: usize,
    pub lines: usize,
    pub sha256: String,
    pub summary: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentContextPack {
    pub repo_path: String,
    pub objective: String,
    pub status: String,
    pub summary: String,
    pub files: Vec<ContextPackFile>,
    pub warnings: Vec<String>,
    pub prompt_notes: Vec<String>,
    pub gates: Vec<String>,
    pub total_bytes: usize,
    pub max_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelProposal {
    pub status: String,
    pub model_used: String,
    pub summary: String,
    pub files_to_change: Vec<String>,
    pub implementation_notes: Vec<String>,
    pub risks: Vec<String>,
    pub gates: Vec<String>,
    pub raw_response: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentBrief {
    pub repo_path: String,
    pub objective: String,
    pub status: String,
    pub summary: String,
    pub model_used: String,
    pub work_order: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub response_contract: Vec<String>,
    pub context_files: Vec<String>,
    pub gates: Vec<String>,
    pub warnings: Vec<String>,
    pub stop_conditions: Vec<String>,
    pub next_actions: Vec<String>,
    pub proposal: Option<LocalModelProposal>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImplementationDecision {
    pub repo_path: String,
    pub objective: String,
    pub status: String,
    pub summary: String,
    pub model_used: String,
    pub source_proposal_status: String,
    pub selected_files: Vec<String>,
    pub implementation_steps: Vec<String>,
    pub required_gates: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub patch_intent: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvolutionCandidate {
    pub id: String,
    pub title: String,
    pub objective: String,
    pub dimension: String,
    pub risk_level: String,
    pub files_to_inspect: Vec<String>,
    pub gates: Vec<String>,
    pub expected_improvement: String,
    pub forbidden_flags: Vec<String>,
    pub requires_human_review: bool,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvolutionDimensionScore {
    pub dimension: String,
    pub score: i32,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvolutionScore {
    pub candidate_id: String,
    pub dimension_scores: Vec<EvolutionDimensionScore>,
    pub risk_penalty: i32,
    pub bloat_penalty: i32,
    pub net_score: i32,
    pub verdict: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedEvolutionCandidate {
    pub candidate: EvolutionCandidate,
    pub score: EvolutionScore,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvolutionPlan {
    pub repo_path: String,
    pub status: String,
    pub summary: String,
    pub selected_candidate: Option<EvolutionCandidate>,
    pub ranked_candidates: Vec<RankedEvolutionCandidate>,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub next_actions: Vec<String>,
    pub local_only_boundary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroPlan {
    #[serde(default)]
    pub objective: String,
    #[serde(default, alias = "recommended_gate")]
    pub recommended_gate: String,
    #[serde(default)]
    pub risk_level: String,
    #[serde(default)]
    pub touched_surfaces: Vec<String>,
    #[serde(default)]
    pub required_gates: Vec<String>,
    #[serde(default)]
    pub steps: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub blocked: bool,
    #[serde(default, alias = "model_used")]
    pub model_used: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchDraft {
    pub id: String,
    pub repo_path: String,
    pub objective: String,
    pub summary: String,
    pub rationale: String,
    pub files: Vec<String>,
    pub unified_diff: String,
    pub risks: Vec<String>,
    pub gates: Vec<String>,
    pub blocked: bool,
    pub model_used: String,
    pub created_at: String,
    pub plan: MicroPlan,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCheck {
    pub name: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchReview {
    pub draft_id: String,
    pub approved: bool,
    pub confirm_token: String,
    pub checks: Vec<ReviewCheck>,
    pub blocks: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyPatchRequest {
    pub draft: PatchDraft,
    #[serde(default)]
    pub allow_apply: bool,
    pub confirm_token: Option<String>,
    #[serde(default = "default_branch_strategy")]
    pub branch_strategy: String,
    pub database_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyPatchResult {
    pub draft_id: String,
    pub status: String,
    pub branch: String,
    pub applied: bool,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReadiness {
    pub draft_id: String,
    pub status: String,
    pub summary: String,
    pub can_apply: bool,
    pub current_branch: String,
    pub target_branch: String,
    pub branch_strategy: String,
    pub confirm_token: String,
    pub checks: Vec<ReviewCheck>,
    pub blocks: Vec<String>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GateResult {
    pub gate: String,
    pub command: String,
    pub status: String,
    pub exit_code: i32,
    pub duration_ms: u128,
    pub summary: String,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunSummary {
    pub id: String,
    pub repo_path: String,
    pub branch: String,
    pub model_used: String,
    pub objective: String,
    pub status: String,
    pub mode: String,
    pub started_at: String,
    pub completed_at: String,
    pub summary: String,
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
    pub lessons: Vec<String>,
    pub persistence: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunReport {
    pub run_id: String,
    pub status: String,
    pub verdict: String,
    pub objective: String,
    pub branch: String,
    pub model_used: String,
    pub recommended_gate: String,
    pub markdown: String,
    pub checklist: Vec<String>,
    pub warnings: Vec<String>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRequest {
    pub repo_path: String,
    pub objective: String,
    pub max_cycles: Option<u8>,
    pub mode: Option<String>,
    pub database_url: Option<String>,
    #[serde(default)]
    pub ask_model: bool,
    #[serde(default)]
    pub allow_apply: bool,
    pub confirm_token: Option<String>,
    #[serde(default = "default_branch_strategy")]
    pub branch_strategy: String,
}

fn default_branch_strategy() -> String {
    "reuse".to_string()
}
