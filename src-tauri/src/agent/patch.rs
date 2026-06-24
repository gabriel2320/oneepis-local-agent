use crate::agent::persistence::{record_decision, record_patch_draft};
use crate::agent::repo::{canonical_repo, declared_gates, git, inspect_repository};
use crate::agent::runner::plan_microcycle;
use crate::agent::safety::{sanitize_log, sha256_hex};
use crate::agent::types::{
    ApplyPatchRequest, ApplyPatchResult, ApplyReadiness, PatchDraft, PatchReview, RepoInspection,
    ReviewCheck,
};
use chrono::Utc;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const MAX_DIFF_BYTES: usize = 40_000;
const MAX_PATCH_FILES: usize = 8;
const ONEEPIS_PATCH_TARGETS: &[&str] = &[
    "docs/CODEX_PLAN.md",
    "CODEX_PLAN.md",
    "docs/CURRENT_STATE.md",
    "CURRENT_STATE.md",
    "README.md",
    "docs/GOVERNANCE.md",
    "docs/SCREEN_TREE.md",
    "AGENTS.md",
];

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
    let summary = format!("Draft gobernado para: {}", sanitize_log(objective));
    let rationale = "Separar plan, revision y aplicacion evita que un modelo local escriba sin aprobacion humana.".to_string();
    let repo = Path::new(&inspection.repo_path);
    let (unified_diff, patch_file) =
        advisory_diff(repo, &inspection, &id, &plan.objective, &plan.steps, &gates);
    let blocked = plan.blocked || plan.risk_level == "red" || !inspection.blocks.is_empty();

    let draft = PatchDraft {
        id,
        repo_path: inspection.repo_path,
        objective: sanitize_log(objective),
        summary,
        rationale,
        files: vec![patch_file],
        unified_diff,
        risks,
        gates,
        blocked,
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
        "PatchDraft debe estar desbloqueado.",
    );
    check(
        &mut checks,
        &mut blocks,
        "risk-level",
        draft.plan.risk_level != "red",
        "Riesgo debe ser verde o amarillo para apply controlado.",
    );
    check(
        &mut checks,
        &mut blocks,
        "diff-present",
        !draft.unified_diff.trim().is_empty(),
        "Unified diff debe estar presente.",
    );
    check(
        &mut checks,
        &mut blocks,
        "diff-size",
        draft.unified_diff.len() <= MAX_DIFF_BYTES,
        "Diff debe quedar bajo el limite de seguridad.",
    );
    check(
        &mut checks,
        &mut blocks,
        "gates-present",
        !draft.gates.is_empty(),
        "PatchDraft debe declarar al menos un gate.",
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
        "Diff debe usar rutas relativas seguras.",
    );

    for gate in &draft.gates {
        check(
            &mut checks,
            &mut blocks,
            &format!("gate:{gate}"),
            gates.contains(gate),
            &format!("Gate debe estar declarado por el repo: {gate}."),
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

pub fn prepare_apply_readiness(request: ApplyPatchRequest) -> Result<ApplyReadiness, String> {
    let review = review_patch(&request.draft)?;
    let repo = canonical_repo(&request.draft.repo_path)?;
    let inspection = inspect_repository(&request.draft.repo_path)?;
    let target_branch = safe_branch_name(&request.draft.objective);
    let mut checks = review.checks.clone();
    let mut blocks = review.blocks.clone();

    check(
        &mut checks,
        &mut blocks,
        "review-approved",
        review.approved,
        "PatchReview debe estar aprobado.",
    );
    check(
        &mut checks,
        &mut blocks,
        "repo-git",
        inspection.is_git_repo,
        "Repo objetivo debe ser Git.",
    );
    check(
        &mut checks,
        &mut blocks,
        "worktree-clean",
        !inspection.dirty,
        "Worktree debe estar limpio antes de apply.",
    );
    check(
        &mut checks,
        &mut blocks,
        "branch-strategy-supported",
        matches!(
            request.branch_strategy.as_str(),
            "create_safe_branch" | "reuse"
        ),
        "branchStrategy debe ser create_safe_branch o reuse.",
    );
    let safe_branch_ready = request.branch_strategy == "create_safe_branch"
        || inspection.current_branch == target_branch;
    check(
        &mut checks,
        &mut blocks,
        "safe-branch",
        safe_branch_ready,
        &format!("Apply requiere rama segura {target_branch} o branchStrategy=create_safe_branch."),
    );

    if review.approved
        && inspection.is_git_repo
        && !inspection.dirty
        && matches!(
            request.branch_strategy.as_str(),
            "create_safe_branch" | "reuse"
        )
        && safe_branch_ready
    {
        match git_apply(&repo, &request.draft.unified_diff, true) {
            Ok(()) => push_status(
                &mut checks,
                "git-apply-check",
                "passed",
                "git apply --check acepto el diff.",
            ),
            Err(err) => {
                let detail = format!("git apply --check bloqueo el diff: {err}");
                push_status(&mut checks, "git-apply-check", "blocked", &detail);
                blocks.push(detail);
            }
        }
    } else {
        push_status(
            &mut checks,
            "git-apply-check",
            "skipped",
            "Se omite hasta resolver revision, Git, limpieza y rama segura.",
        );
    }

    match request.confirm_token.as_deref() {
        Some(token) if token == review.confirm_token => push_status(
            &mut checks,
            "human-confirmation",
            "passed",
            "Token humano coincide con PatchReview.",
        ),
        Some(_) => {
            let detail = "Token de confirmacion invalido.".to_string();
            push_status(&mut checks, "human-confirmation", "blocked", &detail);
            blocks.push(detail);
        }
        None => push_status(
            &mut checks,
            "human-confirmation",
            "pending",
            &format!("Confirmar manualmente con {}.", review.confirm_token),
        ),
    }

    let has_confirmation = request.confirm_token.as_deref() == Some(review.confirm_token.as_str());
    let status = if !blocks.is_empty() {
        "blocked"
    } else if has_confirmation {
        "ready_to_apply"
    } else {
        "ready_for_confirmation"
    };
    let can_apply = status == "ready_to_apply";
    let summary = match status {
        "ready_to_apply" => format!(
            "PatchDraft {} listo para apply controlado en {target_branch}.",
            request.draft.id
        ),
        "ready_for_confirmation" => format!(
            "PatchDraft {} listo para confirmacion humana; aun no aplica cambios.",
            request.draft.id
        ),
        _ => format!(
            "PatchDraft {} bloqueado antes de apply controlado.",
            request.draft.id
        ),
    };
    let next_actions = apply_next_actions(status, &review.confirm_token, &target_branch, &blocks);

    Ok(ApplyReadiness {
        draft_id: request.draft.id,
        status: status.to_string(),
        summary,
        can_apply,
        current_branch: inspection.current_branch,
        target_branch,
        branch_strategy: request.branch_strategy,
        confirm_token: review.confirm_token,
        checks,
        blocks,
        next_actions,
    })
}

pub async fn apply_approved_patch(request: ApplyPatchRequest) -> Result<ApplyPatchResult, String> {
    let readiness = prepare_apply_readiness(request.clone())?;
    if !request.allow_apply {
        return Ok(blocked_readiness_result(
            &request.draft,
            "allowApply=false; aplicacion real bloqueada.",
            &readiness,
        ));
    }
    if !readiness.can_apply {
        return Ok(blocked_readiness_result(
            &request.draft,
            &readiness.summary,
            &readiness,
        ));
    }

    let repo = canonical_repo(&request.draft.repo_path)?;
    let inspection = inspect_repository(&request.draft.repo_path)?;
    let branch = ensure_branch(&repo, &inspection.current_branch, &request)?;
    git_apply(&repo, &request.draft.unified_diff, true)?;
    git_apply(&repo, &request.draft.unified_diff, false)?;
    let mut messages = Vec::new();
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

fn blocked_readiness_result(
    draft: &PatchDraft,
    summary: &str,
    readiness: &ApplyReadiness,
) -> ApplyPatchResult {
    let mut messages = vec![summary.to_string()];
    messages.extend(readiness.blocks.clone());
    messages.extend(readiness.next_actions.clone());
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
    let branch = safe_branch_name(&request.draft.objective);
    if request.branch_strategy == "reuse" {
        if current_branch == branch {
            return Ok(current_branch.to_string());
        }
        return Err(format!(
            "Rama actual no es segura para apply controlado: usar {branch}."
        ));
    }
    if request.branch_strategy != "create_safe_branch" {
        return Err("branchStrategy debe ser create_safe_branch o reuse.".to_string());
    }
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

fn push_status(checks: &mut Vec<ReviewCheck>, name: &str, status: &str, detail: &str) {
    checks.push(ReviewCheck {
        name: name.to_string(),
        status: status.to_string(),
        detail: detail.to_string(),
    });
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

fn apply_next_actions(
    status: &str,
    confirm_token: &str,
    target_branch: &str,
    blocks: &[String],
) -> Vec<String> {
    match status {
        "ready_to_apply" => vec![
            format!("Aplicar con branchStrategy=create_safe_branch en {target_branch}."),
            "Ejecutar el gate declarado inmediatamente despues del apply.".to_string(),
            "Registrar decision humana y resultado del gate en el PR.".to_string(),
        ],
        "ready_for_confirmation" => vec![
            format!("Confirmar manualmente con token {confirm_token}."),
            format!("Aplicar solo en rama segura {target_branch}."),
            "Mantener el PR en modo revisable hasta que el gate pase.".to_string(),
        ],
        _ => blocks
            .iter()
            .take(3)
            .cloned()
            .chain(["No aplicar cambios reales hasta resolver bloqueos.".to_string()])
            .collect(),
    }
}

fn advisory_diff(
    repo: &Path,
    inspection: &RepoInspection,
    id: &str,
    objective: &str,
    steps: &[String],
    gates: &[String],
) -> (String, String) {
    if inspection.is_one_epis {
        if let Some(path) = oneepis_patch_target(repo) {
            let lines = advisory_append_lines(id, objective, steps, gates);
            return (append_diff(repo, &path, lines), path);
        }
    }

    let path = format!("agent-runs/{id}.md");
    let lines = advisory_file_lines(objective, steps, gates);
    (new_file_diff(&path, lines), path)
}

fn advisory_file_lines(objective: &str, steps: &[String], gates: &[String]) -> Vec<String> {
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
    lines
}

fn advisory_append_lines(
    id: &str,
    objective: &str,
    steps: &[String],
    gates: &[String],
) -> Vec<String> {
    let mut lines = vec![
        String::new(),
        format!("## OneEpis Local Agent Draft {id}"),
        String::new(),
        format!("- Objective: {}", sanitize_log(objective)),
        format!("- Gates: {}", gates.join(", ")),
        "- Decision: registrar el microciclo en una fuente canonica existente antes de aplicar cambios.".to_string(),
        String::new(),
    ];
    for step in steps {
        lines.push(format!("- {step}"));
    }
    lines
}

fn new_file_diff(path: &str, lines: Vec<String>) -> String {
    let body: Vec<String> = lines.into_iter().map(|line| format!("+{line}")).collect();
    format!(
        "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{} @@\n{}\n",
        body.len(),
        body.join("\n")
    )
}

fn append_diff(repo: &Path, path: &str, mut lines: Vec<String>) -> String {
    let full_path = repo.join(path);
    let text = fs::read_to_string(full_path).unwrap_or_default();
    let line_count = text.lines().count();
    if line_count == 0 && lines.first().is_some_and(|line| line.is_empty()) {
        lines.remove(0);
    }
    let body: Vec<String> = lines.into_iter().map(|line| format!("+{line}")).collect();
    let old_start = line_count;
    let new_start = line_count + 1;
    format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -{old_start},0 +{new_start},{} @@\n{}\n",
        body.len(),
        body.join("\n")
    )
}

fn oneepis_patch_target(repo: &Path) -> Option<String> {
    ONEEPIS_PATCH_TARGETS
        .iter()
        .find_map(|relative| repo.join(relative).is_file().then(|| relative.to_string()))
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

fn safe_branch_name(objective: &str) -> String {
    format!("agent/{}", slug(objective))
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
        commit_all(&repo);
        let before = fs::read_dir(&repo).expect("before").count();
        let draft = draft_patch(
            repo.to_str().expect("utf8 repo"),
            "Auditar sin escribir",
            Some("http://127.0.0.1:9".to_string()),
            None,
        )
        .await
        .expect("draft");
        let after = fs::read_dir(&repo).expect("after").count();
        assert_eq!(before, after);
        assert!(!draft.blocked);
        let review = review_patch(&draft).expect("review");
        assert!(review.approved);
        assert!(draft.unified_diff.contains("agent-runs/"));
        let _ = fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn oneepis_draft_uses_existing_canonical_document() {
        let repo = std::env::temp_dir().join(format!(
            "oneepis-agent-oneepis-draft-test-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(repo.join("docs")).expect("docs");
        Command::new("git")
            .arg("init")
            .current_dir(&repo)
            .output()
            .expect("git init");
        fs::write(
            repo.join("package.json"),
            r#"{"scripts":{"check:size":"echo size","check:api":"echo api"}}"#,
        )
        .expect("package");
        fs::write(repo.join("AGENTS.md"), "# Agents\n").expect("agents");
        fs::write(repo.join("docs").join("GOVERNANCE.md"), "# Governance\n").expect("gov");
        fs::write(repo.join("docs").join("CODEX_PLAN.md"), "# Codex Plan\n").expect("plan");
        commit_all(&repo);

        let draft = draft_patch(
            repo.to_str().expect("utf8 repo"),
            "Auditar API y proponer microciclo pequeno",
            Some("http://127.0.0.1:9".to_string()),
            None,
        )
        .await
        .expect("draft");

        assert!(!draft.blocked);
        assert_eq!(draft.files, vec!["docs/CODEX_PLAN.md".to_string()]);
        assert!(draft
            .unified_diff
            .contains("diff --git a/docs/CODEX_PLAN.md b/docs/CODEX_PLAN.md"));
        assert!(!draft.unified_diff.contains("agent-runs/"));
        assert!(draft.gates.contains(&"check:api".to_string()));
        let _ = fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn prepare_apply_readiness_requires_confirmation_without_writing() {
        let repo = std::env::temp_dir().join(format!(
            "oneepis-agent-apply-ready-test-{}",
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
            r#"{"scripts":{"check":"echo ok"}}"#,
        )
        .expect("package");
        commit_all(&repo);
        let branch_before = current_branch(&repo);
        let entries_before = fs::read_dir(&repo).expect("before").count();
        let draft = draft_patch(
            repo.to_str().expect("utf8 repo"),
            "Preparar apply seguro",
            Some("http://127.0.0.1:9".to_string()),
            None,
        )
        .await
        .expect("draft");

        let readiness = prepare_apply_readiness(ApplyPatchRequest {
            draft,
            allow_apply: true,
            confirm_token: None,
            branch_strategy: "create_safe_branch".to_string(),
            database_url: None,
        })
        .expect("readiness");

        assert_eq!(readiness.status, "ready_for_confirmation");
        assert!(!readiness.can_apply);
        assert!(readiness.target_branch.starts_with("agent/"));
        assert_eq!(current_branch(&repo), branch_before);
        assert_eq!(fs::read_dir(&repo).expect("after").count(), entries_before);
        assert!(readiness
            .checks
            .iter()
            .any(|check| check.name == "git-apply-check" && check.status == "passed"));
        assert!(readiness
            .checks
            .iter()
            .any(|check| check.name == "human-confirmation" && check.status == "pending"));
        let _ = fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn prepare_apply_readiness_blocks_dirty_repo() {
        let repo = std::env::temp_dir().join(format!(
            "oneepis-agent-apply-dirty-test-{}",
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
            r#"{"scripts":{"check":"echo ok"}}"#,
        )
        .expect("package");
        commit_all(&repo);
        let draft = draft_patch(
            repo.to_str().expect("utf8 repo"),
            "Preparar apply seguro",
            Some("http://127.0.0.1:9".to_string()),
            None,
        )
        .await
        .expect("draft");
        let token = review_patch(&draft).expect("review").confirm_token;
        fs::write(repo.join("dirty.txt"), "cambio sin commit").expect("dirty");

        let readiness = prepare_apply_readiness(ApplyPatchRequest {
            draft,
            allow_apply: true,
            confirm_token: Some(token),
            branch_strategy: "create_safe_branch".to_string(),
            database_url: None,
        })
        .expect("readiness");

        assert_eq!(readiness.status, "blocked");
        assert!(!readiness.can_apply);
        assert!(readiness
            .blocks
            .iter()
            .any(|block| block.contains("Worktree debe estar limpio")));
        assert!(readiness
            .checks
            .iter()
            .any(|check| check.name == "git-apply-check" && check.status == "skipped"));
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn unsafe_patch_paths_are_rejected() {
        assert!(!is_safe_relative_path("../outside.rs"));
        assert!(!is_safe_relative_path("C:\\outside.rs"));
        assert!(is_safe_relative_path("src/main.rs"));
    }

    fn commit_all(repo: &std::path::Path) {
        let add = Command::new("git")
            .arg("add")
            .arg(".")
            .current_dir(repo)
            .output()
            .expect("git add");
        assert!(add.status.success(), "git add failed");
        let commit = Command::new("git")
            .arg("-c")
            .arg("user.name=OneEpis Agent Test")
            .arg("-c")
            .arg("user.email=oneepis-agent-test@example.invalid")
            .arg("commit")
            .arg("-m")
            .arg("test fixture")
            .current_dir(repo)
            .output()
            .expect("git commit");
        assert!(commit.status.success(), "git commit failed");
    }

    fn current_branch(repo: &std::path::Path) -> String {
        let output = Command::new("git")
            .arg("branch")
            .arg("--show-current")
            .current_dir(repo)
            .output()
            .expect("git branch");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }
}
