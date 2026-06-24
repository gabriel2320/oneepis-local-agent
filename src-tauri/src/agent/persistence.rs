use crate::agent::safety::sanitize_log;
use crate::agent::types::{AgentRun, AgentRunSummary, GateResult, PatchDraft};
use tokio_postgres::{Client, NoTls};

pub async fn record_run(database_url: Option<String>, run: &AgentRun) -> Result<String, String> {
    let Some(client) = connect(database_url).await? else {
        return Ok("not_configured".to_string());
    };
    prepare_schema(&client).await?;

    client
        .execute(
            "
            INSERT INTO agent_runs
              (id, repo_path, branch, model, objective, status, mode, started_at, completed_at, summary)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
            ON CONFLICT (id) DO NOTHING
            ",
            &[
                &run.id,
                &run.repo_path,
                &run.branch,
                &run.model_used,
                &sanitize_log(&run.objective),
                &run.status,
                &run.mode,
                &run.started_at,
                &run.completed_at,
                &sanitize_log(&run.plan.objective),
            ],
        )
        .await
        .map_err(|err| format!("No se pudo guardar agent_run: {err}"))?;

    for step in &run.steps {
        let order = step.order as i32;
        client
            .execute(
                "
                INSERT INTO agent_steps (run_id, step_order, state, status, summary)
                VALUES ($1,$2,$3,$4,$5)
                ",
                &[
                    &run.id,
                    &order,
                    &step.state,
                    &step.status,
                    &sanitize_log(&step.summary),
                ],
            )
            .await
            .map_err(|err| format!("No se pudo guardar agent_step: {err}"))?;
    }

    for lesson in &run.lessons {
        client
            .execute(
                "INSERT INTO agent_lessons (run_id, lesson) VALUES ($1,$2)",
                &[&run.id, &sanitize_log(lesson)],
            )
            .await
            .map_err(|err| format!("No se pudo guardar agent_lesson: {err}"))?;
    }

    for warning in &run.plan.warnings {
        client
            .execute(
                "INSERT INTO agent_blocks (run_id, reason) VALUES ($1,$2)",
                &[&run.id, &sanitize_log(warning)],
            )
            .await
            .map_err(|err| format!("No se pudo guardar agent_block: {err}"))?;
    }

    Ok("recorded_postgresql".to_string())
}

pub async fn record_patch_draft(
    database_url: Option<String>,
    draft: &PatchDraft,
) -> Result<String, String> {
    let Some(client) = connect(database_url).await? else {
        return Ok("not_configured".to_string());
    };
    prepare_schema(&client).await?;
    let files_json = serde_json::to_string(&draft.files).unwrap_or_else(|_| "[]".to_string());
    let diff_sha256 = crate::agent::safety::sha256_hex(draft.unified_diff.as_bytes());
    client
        .execute(
            "
            INSERT INTO agent_patch_drafts
              (id, repo_path, objective, summary, rationale, files_json, diff_sha256, blocked, model, created_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
            ON CONFLICT (id) DO NOTHING
            ",
            &[
                &draft.id,
                &draft.repo_path,
                &sanitize_log(&draft.objective),
                &sanitize_log(&draft.summary),
                &sanitize_log(&draft.rationale),
                &files_json,
                &diff_sha256,
                &draft.blocked,
                &draft.model_used,
                &draft.created_at,
            ],
        )
        .await
        .map_err(|err| format!("No se pudo guardar patch_draft: {err}"))?;
    Ok("recorded_postgresql".to_string())
}

pub async fn record_decision(
    database_url: Option<String>,
    target_id: &str,
    action: &str,
    decision: &str,
) -> Result<String, String> {
    let Some(client) = connect(database_url).await? else {
        return Ok("not_configured".to_string());
    };
    prepare_schema(&client).await?;
    client
        .execute(
            "
            INSERT INTO agent_decisions (target_id, action, decision, decided_at)
            VALUES ($1,$2,$3,now()::text)
            ",
            &[&target_id, &action, &sanitize_log(decision)],
        )
        .await
        .map_err(|err| format!("No se pudo guardar decision: {err}"))?;
    Ok("recorded_postgresql".to_string())
}

pub async fn record_gate_result(
    database_url: Option<String>,
    run_id: &str,
    result: &GateResult,
) -> Result<String, String> {
    let Some(client) = connect(database_url).await? else {
        return Ok("not_configured".to_string());
    };
    prepare_schema(&client).await?;
    let duration_ms = result.duration_ms as i32;
    client
        .execute(
            "
            INSERT INTO agent_gate_results (run_id, command, exit_code, duration_ms, summary)
            VALUES ($1,$2,$3,$4,$5)
            ",
            &[
                &run_id,
                &result.command,
                &result.exit_code,
                &duration_ms,
                &sanitize_log(&result.summary),
            ],
        )
        .await
        .map_err(|err| format!("No se pudo guardar gate_result: {err}"))?;
    Ok("recorded_postgresql".to_string())
}

pub async fn list_runs(
    database_url: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<AgentRunSummary>, String> {
    let Some(client) = connect(database_url).await? else {
        return Ok(Vec::new());
    };
    prepare_schema(&client).await?;
    let limit = limit.unwrap_or(20).clamp(1, 100);
    let rows = client
        .query(
            "
            SELECT id, repo_path, branch, model, objective, status, mode, started_at, completed_at, summary
            FROM agent_runs
            ORDER BY started_at DESC
            LIMIT $1
            ",
            &[&limit],
        )
        .await
        .map_err(|err| format!("No se pudo listar agent_runs: {err}"))?;

    Ok(rows
        .into_iter()
        .map(|row| AgentRunSummary {
            id: row.get("id"),
            repo_path: row.get("repo_path"),
            branch: row.get("branch"),
            model_used: row.get("model"),
            objective: row.get("objective"),
            status: row.get("status"),
            mode: row.get("mode"),
            started_at: row.get("started_at"),
            completed_at: row.get("completed_at"),
            summary: row.get("summary"),
        })
        .collect())
}

async fn connect(database_url: Option<String>) -> Result<Option<Client>, String> {
    let Some(database_url) = database_url.or_else(|| std::env::var("AGENT_DATABASE_URL").ok())
    else {
        return Ok(None);
    };

    let (client, connection) = tokio_postgres::connect(&database_url, NoTls)
        .await
        .map_err(|err| format!("No se pudo conectar a PostgreSQL del agente: {err}"))?;
    tokio::spawn(async move {
        if let Err(err) = connection.await {
            eprintln!("agent postgres connection error: {err}");
        }
    });
    Ok(Some(client))
}

async fn prepare_schema(client: &Client) -> Result<(), String> {
    client
        .batch_execute(
            "
            CREATE TABLE IF NOT EXISTS agent_runs (
              id TEXT PRIMARY KEY,
              repo_path TEXT NOT NULL,
              branch TEXT NOT NULL,
              model TEXT NOT NULL,
              objective TEXT NOT NULL,
              status TEXT NOT NULL,
              mode TEXT NOT NULL,
              started_at TEXT NOT NULL,
              completed_at TEXT NOT NULL,
              summary TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS agent_steps (
              id BIGSERIAL PRIMARY KEY,
              run_id TEXT NOT NULL REFERENCES agent_runs(id),
              step_order INTEGER NOT NULL,
              state TEXT NOT NULL,
              status TEXT NOT NULL,
              summary TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS agent_lessons (
              id BIGSERIAL PRIMARY KEY,
              run_id TEXT NOT NULL REFERENCES agent_runs(id),
              lesson TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS agent_gate_results (
              id BIGSERIAL PRIMARY KEY,
              run_id TEXT NOT NULL REFERENCES agent_runs(id),
              command TEXT NOT NULL,
              exit_code INTEGER NOT NULL,
              duration_ms INTEGER NOT NULL,
              summary TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS agent_blocks (
              id BIGSERIAL PRIMARY KEY,
              run_id TEXT NOT NULL REFERENCES agent_runs(id),
              reason TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS agent_patch_drafts (
              id TEXT PRIMARY KEY,
              repo_path TEXT NOT NULL,
              objective TEXT NOT NULL,
              summary TEXT NOT NULL,
              rationale TEXT NOT NULL,
              files_json TEXT NOT NULL,
              diff_sha256 TEXT NOT NULL,
              blocked BOOLEAN NOT NULL,
              model TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS agent_decisions (
              id BIGSERIAL PRIMARY KEY,
              target_id TEXT NOT NULL,
              action TEXT NOT NULL,
              decision TEXT NOT NULL,
              decided_at TEXT NOT NULL
            );
            ",
        )
        .await
        .map_err(|err| format!("No se pudo preparar schema del agente: {err}"))?;
    Ok(())
}
