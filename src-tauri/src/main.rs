mod agent;

use agent::brief;
use agent::context_pack;
use agent::evolution;
use agent::gates;
use agent::ollama;
use agent::patch;
use agent::persistence;
use agent::readiness;
use agent::repo;
use agent::runner;
use agent::types::{
    AgentRun, AgentRunReport, AgentRunSummary, ApplyPatchRequest, ApplyPatchResult, ApplyReadiness,
    DevelopmentBrief, DevelopmentContextPack, DevelopmentReadiness, DevelopmentWorkPackage,
    EvolutionPlan, GateResult, ImplementationDecision, MicroPlan, OllamaStatus, PatchDraft,
    PatchReview, RepoInspection, RunRequest,
};
use agent::work_package;

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
async fn development_work_package(
    repo_path: String,
    objective: String,
    base_url: Option<String>,
) -> Result<DevelopmentWorkPackage, String> {
    work_package::development_work_package(&repo_path, &objective, base_url).await
}

#[tauri::command(rename_all = "camelCase")]
async fn development_context_pack(
    repo_path: String,
    objective: String,
    base_url: Option<String>,
) -> Result<DevelopmentContextPack, String> {
    context_pack::development_context_pack(&repo_path, &objective, base_url).await
}

#[tauri::command(rename_all = "camelCase")]
async fn development_brief(
    repo_path: String,
    objective: String,
    ask_model: Option<bool>,
    base_url: Option<String>,
) -> Result<DevelopmentBrief, String> {
    brief::development_brief(&repo_path, &objective, ask_model.unwrap_or(false), base_url).await
}

#[tauri::command(rename_all = "camelCase")]
async fn implementation_decision(
    repo_path: String,
    objective: String,
    ask_model: Option<bool>,
    base_url: Option<String>,
) -> Result<ImplementationDecision, String> {
    brief::implementation_decision(&repo_path, &objective, ask_model.unwrap_or(false), base_url)
        .await
}

#[tauri::command(rename_all = "camelCase")]
async fn evolution_plan(
    repo_path: String,
    objective: String,
    base_url: Option<String>,
) -> Result<EvolutionPlan, String> {
    evolution::evolution_plan(&repo_path, &objective, base_url).await
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

#[tauri::command]
async fn run_microcycle_report(request: RunRequest) -> Result<AgentRunReport, String> {
    runner::run_microcycle_report(request).await
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
fn prepare_apply_readiness(request: ApplyPatchRequest) -> Result<ApplyReadiness, String> {
    patch::prepare_apply_readiness(request)
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
            development_work_package,
            development_context_pack,
            development_brief,
            implementation_decision,
            evolution_plan,
            plan_microcycle,
            run_microcycle,
            run_microcycle_report,
            draft_patch,
            review_patch,
            prepare_apply_readiness,
            apply_approved_patch,
            run_gate,
            list_runs
        ])
        .run(tauri::generate_context!())
        .expect("error while running OneEpis Local Agent");
}
