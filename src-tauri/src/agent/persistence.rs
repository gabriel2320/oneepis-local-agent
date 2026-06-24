use crate::agent::safety::sanitize_log;
use crate::agent::types::AgentRun;
use tokio_postgres::NoTls;

pub async fn record_run(database_url: Option<String>, run: &AgentRun) -> Result<String, String> {
    let Some(database_url) = database_url.or_else(|| std::env::var("AGENT_DATABASE_URL").ok()) else {
        return Ok("not_configured".to_string());
    };

    let (client, connection) = tokio_postgres::connect(&database_url, NoTls)
        .await
        .map_err(|err| format!("No se pudo conectar a PostgreSQL del agente: {err}"))?;
    tokio::spawn(async move {
        if let Err(err) = connection.await {
            eprintln!("agent postgres connection error: {err}");
        }
    });

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
            ",
        )
        .await
        .map_err(|err| format!("No se pudo preparar schema del agente: {err}"))?;

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
                &[&run.id, &order, &step.state, &step.status, &sanitize_log(&step.summary)],
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

