use crate::agent::persistence::record_gate_result;
use crate::agent::repo::{canonical_repo, declared_gate_script, declared_gates};
use crate::agent::safety::{blocked_command_reason, sanitize_log};
use crate::agent::types::GateResult;
use std::time::Instant;
use tokio::process::Command;

pub async fn run_gate(
    repo_path: &str,
    gate: &str,
    database_url: Option<String>,
    run_id: Option<String>,
) -> Result<GateResult, String> {
    let repo = canonical_repo(repo_path)?;
    let gates = declared_gates(&repo);
    if !gates.iter().any(|candidate| candidate == gate) {
        return Ok(blocked_gate(
            gate,
            &format!("Gate no declarado por package.json: {gate}."),
        ));
    }

    let script = declared_gate_script(&repo, gate).unwrap_or_default();
    if let Some(reason) = blocked_command_reason(&script) {
        return Ok(blocked_gate(gate, reason));
    }

    let command = format!("npm run {gate}");
    let started = Instant::now();
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let output = Command::new(npm)
        .arg("run")
        .arg(gate)
        .current_dir(&repo)
        .output()
        .await
        .map_err(|err| format!("No se pudo ejecutar gate {gate}: {err}"))?;
    let duration_ms = started.elapsed().as_millis();
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = sanitize_log(&String::from_utf8_lossy(&output.stdout));
    let stderr = sanitize_log(&String::from_utf8_lossy(&output.stderr));
    let status = if output.status.success() {
        "passed"
    } else {
        "failed"
    };
    let summary = if output.status.success() {
        format!("Gate {gate} paso en {duration_ms} ms.")
    } else {
        format!("Gate {gate} fallo con codigo {exit_code}.")
    };

    let result = GateResult {
        gate: gate.to_string(),
        command,
        status: status.to_string(),
        exit_code,
        duration_ms,
        summary,
        stdout,
        stderr,
    };
    if let Some(run_id) = run_id {
        let _ = record_gate_result(database_url, &run_id, &result).await;
    }
    Ok(result)
}

fn blocked_gate(gate: &str, reason: &str) -> GateResult {
    GateResult {
        gate: gate.to_string(),
        command: format!("npm run {gate}"),
        status: "blocked".to_string(),
        exit_code: -1,
        duration_ms: 0,
        summary: reason.to_string(),
        stdout: String::new(),
        stderr: String::new(),
    }
}
