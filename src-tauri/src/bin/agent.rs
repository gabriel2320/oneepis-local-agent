#[path = "../agent/mod.rs"]
mod agent;

use agent::brief;
use agent::context_pack;
use agent::gates;
use agent::ollama;
use agent::patch;
use agent::persistence;
use agent::readiness;
use agent::repo;
use agent::runner;
use agent::types::{ApplyPatchRequest, PatchDraft, RunRequest};
use agent::work_package;
use std::fs;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        return usage();
    };

    match command {
        "inspect" => {
            let repo_path = required_repo(&args)?;
            print_json(&repo::inspect_repository(repo_path)?)?;
        }
        "ollama" => {
            print_json(&ollama::get_ollama_status(None).await?)?;
        }
        "readiness" => {
            let repo_path = required_repo(&args)?;
            print_json(&readiness::development_readiness(repo_path, None).await?)?;
        }
        "work-package" => {
            let repo_path = required_repo(&args)?;
            let objective = option_value(&args, "--objective")
                .unwrap_or("Preparar paquete de trabajo gobernado para OneEpis.");
            print_json(&work_package::development_work_package(repo_path, objective, None).await?)?;
        }
        "context-pack" => {
            let repo_path = required_repo(&args)?;
            let objective = option_value(&args, "--objective")
                .unwrap_or("Preparar contexto local gobernado para OneEpis.");
            print_json(&context_pack::development_context_pack(repo_path, objective, None).await?)?;
        }
        "brief" => {
            let repo_path = required_repo(&args)?;
            let objective = option_value(&args, "--objective")
                .unwrap_or("Preparar brief local gobernado para OneEpis.");
            let ask_model = args.iter().any(|arg| arg == "--ask-model");
            print_json(&brief::development_brief(repo_path, objective, ask_model, None).await?)?;
        }
        "plan" => {
            let repo_path = required_repo(&args)?;
            let objective = option_value(&args, "--objective")
                .unwrap_or("Auditar y proponer el microciclo gobernado mas pequeno.");
            print_json(&runner::plan_microcycle(repo_path, objective, None).await?)?;
        }
        "draft" => {
            let repo_path = required_repo(&args)?;
            let objective = option_value(&args, "--objective")
                .unwrap_or("Generar PatchDraft gobernado sin escribir archivos.");
            print_json(&patch::draft_patch(repo_path, objective, None, None).await?)?;
        }
        "review" => {
            let draft = required_draft_file(&args)?;
            print_json(&patch::review_patch(&draft)?)?;
        }
        "apply" => {
            let draft = required_draft_file(&args)?;
            let confirm_token = option_value(&args, "--confirm-token").map(ToString::to_string);
            let branch_strategy = option_value(&args, "--branch-strategy")
                .unwrap_or("create_safe_branch")
                .to_string();
            let request = ApplyPatchRequest {
                draft,
                allow_apply: true,
                confirm_token,
                branch_strategy,
                database_url: None,
            };
            print_json(&patch::apply_approved_patch(request).await?)?;
        }
        "gate" => {
            let repo_path = required_repo(&args)?;
            let gate = option_value(&args, "--gate").unwrap_or("check");
            print_json(&gates::run_gate(repo_path, gate, None, None).await?)?;
        }
        "list-runs" => {
            let limit = option_value(&args, "--limit").and_then(|value| value.parse::<i64>().ok());
            print_json(&persistence::list_runs(None, limit).await?)?;
        }
        "run" => {
            let repo_path = required_repo(&args)?;
            let max_cycles = option_value(&args, "--max-cycles")
                .and_then(|value| value.parse::<u8>().ok())
                .unwrap_or(1);
            let objective = option_value(&args, "--objective")
                .unwrap_or("Ejecutar dry-run gobernado y registrar aprendizaje.");
            let request = RunRequest {
                repo_path: repo_path.to_string(),
                objective: objective.to_string(),
                max_cycles: Some(max_cycles),
                mode: Some("dry_run".to_string()),
                database_url: None,
                allow_apply: false,
                confirm_token: None,
                branch_strategy: "reuse".to_string(),
            };
            print_json(&runner::run_microcycle(request).await?)?;
        }
        "report" => {
            let repo_path = required_repo(&args)?;
            let objective = option_value(&args, "--objective")
                .unwrap_or("Ejecutar dry-run gobernado y preparar reporte PR.");
            let request = RunRequest {
                repo_path: repo_path.to_string(),
                objective: objective.to_string(),
                max_cycles: Some(1),
                mode: Some("dry_run".to_string()),
                database_url: None,
                allow_apply: false,
                confirm_token: None,
                branch_strategy: "reuse".to_string(),
            };
            print_json(&runner::run_microcycle_report(request).await?)?;
        }
        "stop" => {
            println!(
                "{}",
                serde_json::json!({
                    "status": "noop",
                    "message": "No hay runner persistente en v0.1; cada corrida termina sola."
                })
            );
        }
        _ => return usage(),
    }

    Ok(())
}

fn required_repo(args: &[String]) -> Result<&str, String> {
    args.get(1)
        .map(String::as_str)
        .ok_or_else(|| "Falta ruta del repo objetivo.".to_string())
}

fn required_draft_file(args: &[String]) -> Result<PatchDraft, String> {
    let path = args
        .get(1)
        .ok_or_else(|| "Falta ruta del archivo PatchDraft JSON.".to_string())?;
    let text = fs::read_to_string(path)
        .map_err(|err| format!("No se pudo leer PatchDraft JSON: {err}"))?;
    serde_json::from_str::<PatchDraft>(&text)
        .map_err(|err| format!("No se pudo parsear PatchDraft JSON: {err}"))
}

fn option_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window[0] == name)
        .map(|window| window[1].as_str())
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|err| format!("No se pudo serializar JSON: {err}"))?;
    println!("{text}");
    Ok(())
}

fn usage() -> Result<(), String> {
    Err(
        "Uso: agent inspect <repo> | agent readiness <repo> | agent work-package <repo> [--objective texto] | agent context-pack <repo> [--objective texto] | agent brief <repo> [--objective texto] [--ask-model] | agent plan <repo> [--objective texto] | agent draft <repo> [--objective texto] | agent review <draft.json> | agent apply <draft.json> --confirm-token token | agent gate <repo> --gate check:size | agent list-runs [--limit 20] | agent run <repo> [--max-cycles 1] | agent report <repo> [--objective texto] | agent ollama | agent stop"
            .to_string(),
    )
}
