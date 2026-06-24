#[path = "../agent/mod.rs"]
mod agent;

use agent::ollama;
use agent::repo;
use agent::runner;
use agent::types::RunRequest;

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
        "plan" => {
            let repo_path = required_repo(&args)?;
            let objective = option_value(&args, "--objective")
                .unwrap_or("Auditar y proponer el microciclo gobernado mas pequeno.");
            print_json(&runner::plan_microcycle(repo_path, objective, None).await?)?;
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
            };
            print_json(&runner::run_microcycle(request).await?)?;
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
        "Uso: agent inspect <repo> | agent plan <repo> [--objective texto] | agent run <repo> [--max-cycles 1] | agent ollama | agent stop"
            .to_string(),
    )
}

