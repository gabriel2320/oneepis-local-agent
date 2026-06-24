mod agent;

use agent::ollama;
use agent::repo;
use agent::runner;
use agent::types::{AgentRun, MicroPlan, OllamaStatus, RepoInspection, RunRequest};

#[tauri::command(rename_all = "camelCase")]
fn inspect_repository(repo_path: String) -> Result<RepoInspection, String> {
    repo::inspect_repository(&repo_path)
}

#[tauri::command(rename_all = "camelCase")]
async fn get_ollama_status(base_url: Option<String>) -> Result<OllamaStatus, String> {
    ollama::get_ollama_status(base_url).await
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

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            inspect_repository,
            get_ollama_status,
            plan_microcycle,
            run_microcycle
        ])
        .run(tauri::generate_context!())
        .expect("error while running OneEpis Local Agent");
}

