mod agent;

use agent::gates;
use agent::ollama;
use agent::patch;
use agent::persistence;
use agent::readiness;
use agent::repo;
use agent::runner;
use agent::types::{
    AgentRun, AgentRunSummary, ApplyPatchRequest, ApplyPatchResult, DevelopmentReadiness,
    GateResult, MicroPlan, OllamaStatus, PatchDraft, PatchReview, RepoInspection, RunRequest,
};

#[tauri::command(rename_all = "camelCase")]
fn inspect_repository(repo_path: String) -> Result<RepoInspection, String> {
    repo::inspect_repository(&repo_path)
}

#[tauri::command(rename_all = "camelCase")]
async fn get_ollama_status(base_url: Option<String>) -> Result<OllamaStatus, String> {
    ollama::get_ollama_status(base_url).await
}

#[tauri::command(rename_all = "camelCase")]
async fn development_readiness(
    repo_path: String,
    base_url: Option<String>,
) -> Result<DevelopmentReadiness, String> {
    readiness::development_readiness(&repo_path, base_url).await
}

#[tauri::command(rename_all = "camelCase")]
async fn plan_microcycle(
    repo_path: String,
    objective: String,
    base_url: Option<String>,
) -> Result<MicroPlan, String> {
    runner::plan_microcycle(&repo_path, &objective, base_url).await
}

#[tauri::command]
async fn run_microcycle(request: RunRequest) -> Result<AgentRun, String> {
    runner::run_microcycle(request).await
}

#[tauri::command(rename_all = "camelCase")]
async fn draft_patch(
    repo_path: String,
    objective: String,
    base_url: Option<String>,
    database_url: Option<String>,
) -> Result<PatchDraft, String> {
    patch::draft_patch(&repo_path, &objective, base_url, database_url).await
}

#[tauri::command(rename_all = "camelCase")]
fn review_patch(draft: PatchDraft) -> Result<PatchReview, String> {
    patch::review_patch(&draft)
}

#[tauri::command]
async fn apply_approved_patch(request: ApplyPatchRequest) -> Result<ApplyPatchResult, String> {
    patch::apply_approved_patch(request).await
}

#[tauri::command(rename_all = "camelCase")]
async fn run_gate(
    repo_path: String,
    gate: String,
    database_url: Option<String>,
    run_id: Option<String>,
) -> Result<GateResult, String> {
    gates::run_gate(&repo_path, &gate, database_url, run_id).await
}

#[tauri::command(rename_all = "camelCase")]
async fn list_runs(
    database_url: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<AgentRunSummary>, String> {
    persistence::list_runs(database_url, limit).await
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            inspect_repository,
            get_ollama_status,
            development_readiness,
            plan_microcycle,
            run_microcycle,
            draft_patch,
            review_patch,
            apply_approved_patch,
            run_gate,
            list_runs
        ])
        .run(tauri::generate_context!())
        .expect("error while running OneEpis Local Agent");
}
