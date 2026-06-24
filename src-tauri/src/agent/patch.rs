use crate::agent::persistence::{record_decision, record_patch_draft};
use crate::agent::repo::{canonical_repo, declared_gates, git, inspect_repository};
use crate::agent::runner::plan_microcycle;
use crate::agent::safety::{sanitize_log, sha256_hex};
use crate::agent::types::{
    ApplyPatchRequest, ApplyPatchResult, PatchDraft, PatchReview, ReviewCheck,
};
use chrono::Utc;
use std::io::Write;
use std::process::{Command, Stdio};

const MAX_DIFF_BYTES: usize = 40_000;
const MAX_PATCH_FILES: usize = 8;

pub async fn draft_patch(
    repo_path: &str,
    objective: &str,
    base_url: Option<String>,
    database_url: Option<String>,
) -> Result<PatchDraft, String> {
    let inspection = inspect_repository(repo_path)?;
    let plan = plan_microcycle(repo_path, objective, base_url).await?;
    let created_at = Utc::now().to_rfc3339();
    let id = draft_id(&inspection.repo_path, objective, &created_at);
    let mut risks = plan.warnings.clone();
    risks.extend(inspection.blocks.clone());
    risks.push("v0.2 genera un draft revisable; la escritura real requiere un diff concreto aprobado en v0.3.".to_string());

    let gates = if plan.required_gates.is_empty() {
        if plan.recommended_gate == "sin_gate" {
            Vec::new()
        } else {
            vec![plan.recommended_gate.clone()]
        }
    } else {
        plan.required_gates.clone()
    };
    let files = if plan.touched_surfaces.is_empty() {
        vec!["repo".to_string()]
    } else {
        plan.touched_surfaces.clone()
    };
    let summary = format!("Draft gobernado para: {}", sanitize_log(objective));
    let rationale = "Separar plan, revision y aplicacion evita que un modelo local escriba sin aprobacion humana.".to_string();
    let unified_diff = advisory_diff(&id, &plan.objective, &plan.steps, &gates);

    let draft = PatchDraft {
        id,
        repo_path: inspection.repo_path,
        objective: sanitize_log(objective),
        summary,
        rationale,
        files,
        unified_diff,
        risks,
        gates,
        blocked: true,
        model_used: plan.model_used.clone(),
        created_at,
        plan,
    };
    let _ = record_patch_draft(database_url, &draft).await;
    Ok(draft)
}

pub fn review_patch(draft: &PatchDraft) -> Result<PatchReview, String> {
    let repo = canonical_repo(&draft.repo_path)?;
    let gates = declared_gates(&repo);
    let mut checks = Vec::new();
    let mut blocks = Vec::new();

    check(
        &mut checks,
        &mut blocks,
        "draft-state",
        !draft.blocked,
        "PatchDraft no debe estar bloqueado.",
    );
    check(
        &mut checks,
        &mut blocks,
        "risk-level",
        draft.plan.risk_level != "red",
        "Riesgo rojo requiere contrato/manual review antes de aplicar.",
    );
    check(
        &mut checks,
        &mut blocks,
        "diff-present",
        !draft.unified_diff.trim().is_empty(),
        "Falta unified diff aplicable.",
    );
    check(
        &mut checks,
        &mut blocks,
        "diff-size",
        draft.unified_diff.len() <= MAX_DIFF_BYTES,
        "Diff excede el limite de seguridad.",
    );

    let patch_files = patch_files(&draft.unified_diff);
    check(
        &mut checks,
        &mut blocks,
        "diff-files",
        !patch_files.is_empty() && patch_files.len() <= MAX_PATCH_FILES,
        "Diff debe tocar entre 1 y 8 archivos.",
    );
    let safe_paths = patch_files.iter().all(|path| is_safe_relative_path(path));
    check(
        &mut checks,
        &mut blocks,
        "diff-paths",
        safe_paths,
        "Diff contiene rutas absolutas o parent traversal.",
    );

    for gate in &draft.gates {
        check(
            &mut checks,
            &mut blocks,
            &format!("gate:{gate}"),
            gates.contains(gate),
            &format!("Gate no declarado por el repo: {gate}."),
        );
    }

    let approved = blocks.is_empty();
    Ok(PatchReview {
        draft_id: draft.id.clone(),
        approved,
        confirm_token: confirm_token(&draft.id),
        checks,
        blocks,
    })
}

pub async fn apply_approved_patch(request: ApplyPatchRequest) -> Result<ApplyPatchResult, String> {
    let review = review_patch(&request.draft)?;
    let mut messages = Vec::new();
    if !request.allow_apply {
        return Ok(blocked_result(
            &request.draft,
            "allowApply=false; aplicacion real bloqueada.",
            review.blocks,
        ));
    }
    if request.confirm_token.as_deref() != Some(review.confirm_token.as_str()) {
        return Ok(blocked_result(
            &request.draft,
            "Token de confirmacion invalido.",
            review.blocks,
        ));
    }
    if !review.approved {
        return Ok(blocked_result(
            &request.draft,
            "Revision de patch no aprobada.",
            review.blocks,
        ));
    }

    let repo = canonical_repo(&request.draft.repo_path)?;
    let inspection = inspect_repository(&request.draft.repo_path)?;
    if !inspection.is_git_repo || inspection.dirty {
        return Ok(blocked_result(
            &request.draft,
            "Repo no Git o worktree sucio.",
            inspection.blocks,
        ));
    }

    let branch = ensure_branch(&repo, &inspection.current_branch, &request)?;
    git_apply(&repo, &request.draft.unified_diff, true)?;
    git_apply(&repo, &request.draft.unified_diff, false)?;
    messages.push("Patch aplicado con git apply.".to_string());
    messages.push(format!("Rama activa: {branch}."));
    let _ = record_decision(
        request.database_url,
        &request.draft.id,
        "apply_approved_patch",
        "applied",
    )
    .await;

    Ok(ApplyPatchResult {
        draft_id: request.draft.id,
        status: "applied".to_string(),
        branch,
        applied: true,
        messages,
    })
}

fn blocked_result(draft: &PatchDraft, summary: &str, blocks: Vec<String>) -> ApplyPatchResult {
    let mut messages = vec![summary.to_string()];
    messages.extend(blocks);
    ApplyPatchResult {
        draft_id: draft.id.clone(),
        status: "blocked".to_string(),
        branch: "unchanged".to_string(),
        applied: false,
        messages,
    }
}

fn ensure_branch(
    repo: &std::path::Path,
    current_branch: &str,
    request: &ApplyPatchRequest,
) -> Result<String, String> {
    if request.branch_strategy != "create_safe_branch" {
        return Ok(current_branch.to_string());
    }
    let branch = format!("agent/{}", slug(&request.draft.objective));
    if current_branch == branch {
        return Ok(branch);
    }
    if git(repo, &["rev-parse", "--verify", &branch]).is_ok() {
        git(repo, &["switch", &branch])?;
    } else {
        git(repo, &["switch", "-c", &branch])?;
    }
    Ok(branch)
}

fn git_apply(repo: &std::path::Path, diff: &str, check_only: bool) -> Result<(), String> {
    let mut command = Command::new("git");
    command.arg("-C").arg(repo).arg("apply");
    if check_only {
        command.arg("--check");
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("No se pudo iniciar git apply: {err}"))?;
    let Some(mut stdin) = child.stdin.take() else {
        return Err("No se pudo abrir stdin para git apply.".to_string());
    };
    stdin
        .write_all(diff.as_bytes())
        .map_err(|err| format!("No se pudo escribir diff en git apply: {err}"))?;
    drop(stdin);
    let output = child
        .wait_with_output()
        .map_err(|err| format!("No se pudo esperar git apply: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = sanitize_log(&String::from_utf8_lossy(&output.stderr));
    Err(format!("git apply fallo: {stderr}"))
}

fn check(
    checks: &mut Vec<ReviewCheck>,
    blocks: &mut Vec<String>,
    name: &str,
    ok: bool,
    detail: &str,
) {
    checks.push(ReviewCheck {
        name: name.to_string(),
        status: if ok { "passed" } else { "blocked" }.to_string(),
        detail: detail.to_string(),
    });
    if !ok {
        blocks.push(detail.to_string());
    }
}

fn advisory_diff(id: &str, objective: &str, steps: &[String], gates: &[String]) -> String {
    let path = format!("agent-runs/{id}.md");
    let mut lines = vec![
        "# OneEpis Local Agent PatchDraft".to_string(),
        String::new(),
        format!("- Objective: {}", sanitize_log(objective)),
        format!("- Gates: {}", gates.join(", ")),
        String::new(),
    ];
    for step in steps {
        lines.push(format!("- {step}"));
    }
    let body: Vec<String> = lines.into_iter().map(|line| format!("+{line}")).collect();
    format!(
        "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{} @@\n{}\n",
        body.len(),
        body.join("\n")
    )
}

fn patch_files(diff: &str) -> Vec<String> {
    diff.lines()
        .filter_map(|line| line.strip_prefix("diff --git a/"))
        .filter_map(|rest| rest.split_once(" b/").map(|(_, path)| path.to_string()))
        .collect()
}

fn is_safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && !path.contains("..")
        && !path.contains(':')
}

fn draft_id(repo_path: &str, objective: &str, created_at: &str) -> String {
    let digest = sha256_hex(format!("{repo_path}:{objective}:{created_at}").as_bytes());
    format!("draft-{}", &digest[..16])
}

fn confirm_token(draft_id: &str) -> String {
    format!("APPLY:{draft_id}")
}

fn slug(input: &str) -> String {
    let mut slug = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if matches!(ch, ' ' | '-' | '_' | '/' | '\\') && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "microcycle".to_string()
    } else {
        slug.chars().take(48).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    #[tokio::test]
    async fn draft_patch_does_not_write_files() {
        let repo = std::env::temp_dir().join(format!(
            "oneepis-agent-draft-test-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&repo).expect("temp repo");
        Command::new("git")
            .arg("init")
            .current_dir(&repo)
            .output()
            .expect("git init");
        fs::write(
            repo.join("package.json"),
            r#"{"scripts":{"check":"echo ok","check:api":"echo api"}}"#,
        )
        .expect("package");
        let before = fs::read_dir(&repo).expect("before").count();
        let draft = draft_patch(
            repo.to_str().expect("utf8 repo"),
            "Auditar sin escribir",
            None,
            None,
        )
        .await
        .expect("draft");
        let after = fs::read_dir(&repo).expect("after").count();
        assert_eq!(before, after);
        assert!(draft.blocked);
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn unsafe_patch_paths_are_rejected() {
        assert!(!is_safe_relative_path("../outside.rs"));
        assert!(!is_safe_relative_path("C:\\outside.rs"));
        assert!(is_safe_relative_path("src/main.rs"));
    }
}
