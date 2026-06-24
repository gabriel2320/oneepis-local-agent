use crate::agent::context_pack::build_development_context_pack;
use crate::agent::ollama::get_ollama_status;
use crate::agent::readiness::build_development_readiness;
use crate::agent::repo::inspect_repository;
use crate::agent::safety::sanitize_log;
use crate::agent::types::{
    DevelopmentContextPack, DevelopmentReadiness, DevelopmentWorkPackage, EvolutionCandidate,
    EvolutionDimensionScore, EvolutionPlan, EvolutionScore, RankedEvolutionCandidate,
    RepoInspection,
};
use crate::agent::work_package::build_development_work_package;
use std::path::Path;

const LOCAL_ONLY_BOUNDARY: &str = "Evolucion supervisada local: solo Ollama/local_rules, sin IA externa, sin PHI, sin datos reales, sin escritura automatica y sin reemplazar gobernanza OneEpis.";

pub async fn evolution_plan(
    repo_path: &str,
    objective: &str,
    base_url: Option<String>,
) -> Result<EvolutionPlan, String> {
    let inspection = inspect_repository(repo_path)?;
    let ollama = get_ollama_status(base_url).await?;
    let readiness = build_development_readiness(&inspection, &ollama);
    let package = build_development_work_package(&inspection, &readiness, objective);
    let context = build_development_context_pack(Path::new(repo_path), &package);
    Ok(build_evolution_plan(
        &inspection,
        &readiness,
        &package,
        &context,
        objective,
    ))
}

pub fn build_evolution_plan(
    inspection: &RepoInspection,
    readiness: &DevelopmentReadiness,
    package: &DevelopmentWorkPackage,
    context: &DevelopmentContextPack,
    objective: &str,
) -> EvolutionPlan {
    let objective = sanitize_log(objective);
    let mut candidates = generate_candidates(inspection, readiness, package, context, &objective);
    if candidates.is_empty() {
        candidates.push(user_objective_candidate(package, context, &objective));
    }

    let mut ranked_candidates = candidates
        .into_iter()
        .map(|candidate| {
            let score = score_candidate(&candidate, inspection, &objective);
            RankedEvolutionCandidate { candidate, score }
        })
        .collect::<Vec<_>>();

    ranked_candidates.sort_by(|a, b| {
        verdict_rank(&b.score.verdict)
            .cmp(&verdict_rank(&a.score.verdict))
            .then_with(|| b.score.net_score.cmp(&a.score.net_score))
            .then_with(|| a.candidate.id.cmp(&b.candidate.id))
    });

    let blockers = hard_blockers(inspection);
    let warnings = warnings(inspection, readiness, context);
    let selected_candidate = if blockers.is_empty() {
        ranked_candidates
            .iter()
            .find(|item| item.score.verdict == "executable")
            .map(|item| item.candidate.clone())
    } else {
        None
    };
    let status = status_for(&blockers, &selected_candidate, &ranked_candidates);
    let summary = summary_for(status, inspection, &selected_candidate, &ranked_candidates);
    let next_actions = next_actions(status, &selected_candidate, &blockers, &warnings);

    EvolutionPlan {
        repo_path: inspection.repo_path.clone(),
        status: status.to_string(),
        summary,
        selected_candidate,
        ranked_candidates,
        blockers,
        warnings,
        next_actions,
        local_only_boundary: LOCAL_ONLY_BOUNDARY.to_string(),
    }
}

fn generate_candidates(
    inspection: &RepoInspection,
    readiness: &DevelopmentReadiness,
    package: &DevelopmentWorkPackage,
    context: &DevelopmentContextPack,
    objective: &str,
) -> Vec<EvolutionCandidate> {
    let mut candidates = Vec::new();
    candidates.push(user_objective_candidate(package, context, objective));

    for suggested in &readiness.suggested_microcycles {
        candidates.push(EvolutionCandidate {
            id: format!("suggested-{}", slug(&suggested.title)),
            title: suggested.title.clone(),
            objective: suggested.objective.clone(),
            dimension: infer_dimension(&suggested.objective),
            risk_level: suggested.risk_level.clone(),
            files_to_inspect: candidate_files(
                package,
                context,
                &["clinical", "api", "contract", "screen", "size"],
            ),
            gates: normalize_gates(&suggested.gates, inspection),
            expected_improvement: suggested.reason.clone(),
            forbidden_flags: forbidden_flags_for(&format!(
                "{} {} {}",
                objective, suggested.title, suggested.objective
            )),
            requires_human_review: false,
            source: "readiness".to_string(),
        });
    }

    candidates.extend(policy_candidates(inspection, package, context, objective));
    dedupe_candidates(candidates)
}

fn user_objective_candidate(
    package: &DevelopmentWorkPackage,
    context: &DevelopmentContextPack,
    objective: &str,
) -> EvolutionCandidate {
    let forbidden_flags = forbidden_flags_for(objective);
    EvolutionCandidate {
        id: "user-objective".to_string(),
        title: "Objetivo ingresado acotado".to_string(),
        objective: objective.to_string(),
        dimension: infer_dimension(objective),
        risk_level: if forbidden_flags.is_empty() {
            infer_risk(objective)
        } else {
            "red".to_string()
        },
        files_to_inspect: candidate_files(
            package,
            context,
            &["agents", "governance", "api", "web", "clinical"],
        ),
        gates: package.gates.clone(),
        expected_improvement:
            "Convertir el objetivo actual en un microproceso pequeno, puntuado y revisable."
                .to_string(),
        forbidden_flags,
        requires_human_review: false,
        source: "user_objective".to_string(),
    }
}

fn policy_candidates(
    inspection: &RepoInspection,
    package: &DevelopmentWorkPackage,
    context: &DevelopmentContextPack,
    objective: &str,
) -> Vec<EvolutionCandidate> {
    vec![
        EvolutionCandidate {
            id: "policy-local-ai-boundary".to_string(),
            title: "Blindaje de IA local".to_string(),
            objective: "Agregar o verificar aprendizaje ejecutable que impida proveedores IA externos y confirme Ollama local como unico proveedor.".to_string(),
            dimension: "ai_local".to_string(),
            risk_level: "green".to_string(),
            files_to_inspect: candidate_files(package, context, &["package", "script", "agents", "governance"]),
            gates: gate_subset(inspection, &["check:no-external-ai", "check:size", "check:api"]),
            expected_improvement: "Reduce riesgo de rechazo por gobernanza y evita que el agente proponga IA protagonista o externa.".to_string(),
            forbidden_flags: forbidden_flags_for(objective),
            requires_human_review: false,
            source: "policy".to_string(),
        },
        EvolutionCandidate {
            id: "policy-no-phi".to_string(),
            title: "Blindaje sin PHI".to_string(),
            objective: "Agregar o verificar aprendizaje ejecutable para que fixtures, prompts y diffs no incluyan PHI, secretos ni identificadores reales.".to_string(),
            dimension: "security".to_string(),
            risk_level: "green".to_string(),
            files_to_inspect: candidate_files(package, context, &["test", "fixture", "agents", "governance", "script"]),
            gates: gate_subset(inspection, &["check:no-phi", "check:size", "check:api"]),
            expected_improvement: "Hace medible una regla central: programar OneEpis sin datos sensibles.".to_string(),
            forbidden_flags: forbidden_flags_for(objective),
            requires_human_review: false,
            source: "policy".to_string(),
        },
        EvolutionCandidate {
            id: "policy-clinical-contract".to_string(),
            title: "Contrato clinico minimo".to_string(),
            objective: "Auditar una superficie paciente/ficha/API y dejar aprendizaje ejecutable en contrato, test o tipo sin cambiar verdad clinica.".to_string(),
            dimension: "clinical_truth".to_string(),
            risk_level: "yellow".to_string(),
            files_to_inspect: candidate_files(package, context, &["api", "openapi", "contract", "clinical", "test"]),
            gates: gate_subset(inspection, &["check:contract", "check:api", "check:size"]),
            expected_improvement: "Sigue la escalera paciente -> ficha -> papel -> API -> PostgreSQL -> auditoria -> permisos -> OpenAPI.".to_string(),
            forbidden_flags: forbidden_flags_for(objective),
            requires_human_review: false,
            source: "policy".to_string(),
        },
        EvolutionCandidate {
            id: "policy-anti-bloat".to_string(),
            title: "Dieta anti-bloat near-limit".to_string(),
            objective: "Reducir o aislar una pieza tecnica near-limit sin comportamiento nuevo y validar con check:size.".to_string(),
            dimension: "anti_bloat".to_string(),
            risk_level: "green".to_string(),
            files_to_inspect: candidate_files(package, context, &["size", "clinical", "component", "service", "script"]),
            gates: gate_subset(inspection, &["check:size", "check:api", "check:web"]),
            expected_improvement: "Mejora mantenibilidad antes de agregar superficie nueva.".to_string(),
            forbidden_flags: forbidden_flags_for(objective),
            requires_human_review: false,
            source: "policy".to_string(),
        },
        EvolutionCandidate {
            id: "policy-executable-learning".to_string(),
            title: "Aprendizaje ejecutable".to_string(),
            objective: "Convertir una regla ya documentada en test, gate, contrato o tipo para que el siguiente ciclo la pueda verificar.".to_string(),
            dimension: "executable_learning".to_string(),
            risk_level: "green".to_string(),
            files_to_inspect: candidate_files(package, context, &["test", "contract", "type", "governance", "screen"]),
            gates: gate_subset(inspection, &["check:size", "check:api", "check:web", "check:contract"]),
            expected_improvement: "Evita que el aprendizaje quede solo en texto y lo vuelve verificable.".to_string(),
            forbidden_flags: forbidden_flags_for(objective),
            requires_human_review: false,
            source: "policy".to_string(),
        },
    ]
}

fn score_candidate(
    candidate: &EvolutionCandidate,
    inspection: &RepoInspection,
    user_objective: &str,
) -> EvolutionScore {
    let text = normalized_text(&format!(
        "{} {} {}",
        candidate.title, candidate.objective, candidate.expected_improvement
    ));
    let mut flags = candidate.forbidden_flags.clone();
    flags.extend(forbidden_flags_for(&format!("{user_objective} {text}")));
    flags = dedupe(flags);

    let dimension_scores = vec![
        score_objective_alignment(candidate, user_objective),
        score_governance(inspection, candidate, &flags, &text),
        score_security(&text),
        score_clinical_truth(&text),
        score_executable_learning(candidate, &text),
        score_anti_bloat(&text),
        score_local_ai(&text),
    ];
    let risk_penalty = risk_penalty(candidate, !flags.is_empty());
    let bloat_penalty = bloat_penalty(&text);
    let net_score =
        dimension_scores.iter().map(|item| item.score).sum::<i32>() - risk_penalty - bloat_penalty;
    let verdict = verdict_for(candidate, &flags, net_score);
    let mut reasons = Vec::new();
    reasons.push(format!(
        "Puntaje neto {net_score}; minimo ejecutable: 3 con gates y sin riesgo rojo."
    ));
    if candidate.gates.is_empty() {
        reasons.push("No declara gate disponible en el repo objetivo.".to_string());
    }
    if !flags.is_empty() {
        reasons.extend(flags.iter().map(|flag| format!("Bloqueo: {flag}.")));
    }
    if candidate.requires_human_review {
        reasons.push("Requiere revision humana antes de convertirse en PatchDraft.".to_string());
    }
    reasons.push(format!(
        "Mejora esperada: {}",
        candidate.expected_improvement
    ));

    EvolutionScore {
        candidate_id: candidate.id.clone(),
        dimension_scores,
        risk_penalty,
        bloat_penalty,
        net_score,
        verdict,
        reasons: dedupe(reasons),
    }
}

fn score_objective_alignment(
    candidate: &EvolutionCandidate,
    user_objective: &str,
) -> EvolutionDimensionScore {
    let objective_dimension = infer_dimension(user_objective);
    let score = if candidate.dimension == objective_dimension {
        4
    } else {
        0
    };
    EvolutionDimensionScore {
        dimension: "objective_alignment".to_string(),
        score,
        reason: if score > 0 {
            "El candidato responde directamente a la dimension del objetivo ingresado.".to_string()
        } else {
            "El candidato es util, pero no es la dimension principal del objetivo actual."
                .to_string()
        },
    }
}

fn score_governance(
    inspection: &RepoInspection,
    candidate: &EvolutionCandidate,
    flags: &[String],
    text: &str,
) -> EvolutionDimensionScore {
    let mut score = 0;
    if inspection.is_one_epis {
        score += 2;
    }
    if !candidate.gates.is_empty() {
        score += 1;
    }
    if text.contains("gobernanza") || text.contains("agents") || text.contains("screen_tree") {
        score += 1;
    }
    if !flags.is_empty() {
        score = 0;
    }
    EvolutionDimensionScore {
        dimension: "governance_fit".to_string(),
        score,
        reason: if score > 0 {
            "Perfil OneEpis, gates y reglas visibles sostienen el microproceso.".to_string()
        } else {
            "No hay ajuste suficiente a gobernanza o existen banderas prohibidas.".to_string()
        },
    }
}

fn score_security(text: &str) -> EvolutionDimensionScore {
    let score = if contains_any(
        text,
        &[
            "seguridad",
            "secret",
            "secreto",
            "no phi",
            "sin phi",
            "identificador",
            "permiso",
            "auditoria",
        ],
    ) {
        3
    } else if text.contains("clin") || text.contains("paciente") {
        1
    } else {
        0
    };
    EvolutionDimensionScore {
        dimension: "security".to_string(),
        score,
        reason: "Premia reglas que reducen secretos, PHI, permisos o auditoria faltante."
            .to_string(),
    }
}

fn score_clinical_truth(text: &str) -> EvolutionDimensionScore {
    let score = if contains_any(
        text,
        &[
            "paciente",
            "ficha",
            "papel",
            "api",
            "postgresql",
            "auditoria",
            "permisos",
            "openapi",
            "contrato",
        ],
    ) {
        3
    } else if text.contains("clin") {
        2
    } else {
        0
    };
    EvolutionDimensionScore {
        dimension: "clinical_truth".to_string(),
        score,
        reason: "Premia la escalera paciente -> ficha -> papel -> API -> PostgreSQL -> auditoria -> permisos -> OpenAPI.".to_string(),
    }
}

fn score_executable_learning(
    candidate: &EvolutionCandidate,
    text: &str,
) -> EvolutionDimensionScore {
    let score = if contains_any(text, &["test", "gate", "contrato", "tipo", "ejecutable"]) {
        3
    } else if !candidate.gates.is_empty() {
        2
    } else {
        0
    };
    EvolutionDimensionScore {
        dimension: "executable_learning".to_string(),
        score,
        reason: "Premia cambios que dejan test, gate, contrato o tipo verificable.".to_string(),
    }
}

fn score_anti_bloat(text: &str) -> EvolutionDimensionScore {
    let score = if contains_any(
        text,
        &[
            "near-limit",
            "tamano",
            "size",
            "reducir",
            "archivo",
            "anti-bloat",
        ],
    ) {
        3
    } else if contains_any(text, &["refactor", "manteni"]) {
        2
    } else {
        0
    };
    EvolutionDimensionScore {
        dimension: "anti_bloat".to_string(),
        score,
        reason: "Premia reducir superficie o tamano antes de agregar comportamiento.".to_string(),
    }
}

fn score_local_ai(text: &str) -> EvolutionDimensionScore {
    let score = if contains_any(
        text,
        &[
            "ollama",
            "local",
            "no external",
            "sin ia externa",
            "no ia externa",
        ],
    ) {
        3
    } else {
        0
    };
    EvolutionDimensionScore {
        dimension: "ai_local".to_string(),
        score,
        reason: "Premia que el agente siga siendo local y no protagonista clinico.".to_string(),
    }
}

fn verdict_for(candidate: &EvolutionCandidate, flags: &[String], net_score: i32) -> String {
    if !flags.is_empty() || candidate.risk_level == "red" {
        return "blocked".to_string();
    }
    if candidate.gates.is_empty() || candidate.requires_human_review {
        return "review_only".to_string();
    }
    if net_score >= 3 {
        "executable".to_string()
    } else {
        "rejected".to_string()
    }
}

fn risk_penalty(candidate: &EvolutionCandidate, has_flags: bool) -> i32 {
    if has_flags {
        return 8;
    }
    match candidate.risk_level.as_str() {
        "green" => 0,
        "yellow" => 1,
        "red" => 6,
        _ => 2,
    }
}

fn bloat_penalty(text: &str) -> i32 {
    let mut penalty = 0;
    if contains_any(text, &["dashboard", "chat", "rag", "workbench"]) {
        penalty += 4;
    }
    if contains_any(
        text,
        &[
            "pantalla nueva",
            "feature nueva",
            "modulo nuevo",
            "sistema amplio",
        ],
    ) {
        penalty += 3;
    }
    penalty
}

fn forbidden_flags_for(input: &str) -> Vec<String> {
    let text = normalized_text(input);
    let mut flags = Vec::new();
    if text.contains("dashboard") {
        flags.push("dashboard generico no gobernado".to_string());
    }
    if text.contains("chat libre") || text.contains("chatbot") || text.contains("chat ia") {
        flags.push("chat libre o IA protagonista".to_string());
    }
    if text.contains("rag") {
        flags.push("RAG amplio sin contrato".to_string());
    }
    if contains_external_ai(&text) {
        flags.push("IA externa prohibida por politica local".to_string());
    }
    if contains_any(&text, &["receta", "prescripcion", "farmaco", "medicamento"]) {
        flags.push("prescripcion o farmaco sin plan clinico especifico".to_string());
    }
    if contains_any(&text, &["firma clinica", "firmar clin", "signed"]) {
        flags.push("firma clinica falsa o no verificable".to_string());
    }
    if contains_phi(&text) {
        flags.push("PHI o identificadores reales".to_string());
    }
    dedupe(flags)
}

fn contains_external_ai(text: &str) -> bool {
    if contains_any(
        text,
        &[
            "sin ia externa",
            "no ia externa",
            "no external ai",
            "no external-ai",
        ],
    ) {
        return false;
    }
    contains_any(
        text,
        &[
            "ia externa",
            "external ai",
            "openai",
            "chatgpt",
            "claude",
            "gemini",
        ],
    )
}

fn contains_phi(text: &str) -> bool {
    if contains_any(text, &["sin phi", "no phi", "no-phi"]) {
        return false;
    }
    contains_any(
        text,
        &[
            " phi",
            "paciente real",
            "dato real",
            "rut real",
            "identificador real",
        ],
    )
}

fn gate_subset(inspection: &RepoInspection, preferred: &[&str]) -> Vec<String> {
    let mut gates = Vec::new();
    for gate in preferred {
        if inspection
            .declared_gates
            .iter()
            .any(|declared| declared == gate)
        {
            push_unique(&mut gates, gate);
        }
    }
    gates
}

fn normalize_gates(gates: &[String], inspection: &RepoInspection) -> Vec<String> {
    gates
        .iter()
        .filter(|gate| {
            inspection
                .declared_gates
                .iter()
                .any(|declared| declared == *gate)
        })
        .cloned()
        .collect()
}

fn candidate_files(
    package: &DevelopmentWorkPackage,
    context: &DevelopmentContextPack,
    needles: &[&str],
) -> Vec<String> {
    let mut files = Vec::new();
    for file in &context.files {
        let lower = file.path.to_ascii_lowercase();
        if file.kind == "file" && needles.iter().any(|needle| lower.contains(needle)) {
            push_unique(&mut files, &file.path);
        }
    }
    for path in &package.files_to_inspect {
        let lower = path.to_ascii_lowercase();
        if needles.iter().any(|needle| lower.contains(needle)) {
            push_unique(&mut files, path);
        }
    }
    if files.is_empty() {
        for file in &context.files {
            if file.kind == "file" {
                push_unique(&mut files, &file.path);
            }
            if files.len() >= 3 {
                break;
            }
        }
    }
    if files.is_empty() {
        for path in &package.files_to_inspect {
            push_unique(&mut files, path);
            if files.len() >= 3 {
                break;
            }
        }
    }
    files.truncate(3);
    files
}

fn hard_blockers(inspection: &RepoInspection) -> Vec<String> {
    let mut blockers = inspection.blocks.clone();
    if !inspection.is_git_repo {
        blockers.push(
            "El repo objetivo no es Git; la evolucion supervisada queda bloqueada.".to_string(),
        );
    }
    dedupe(blockers)
}

fn warnings(
    inspection: &RepoInspection,
    readiness: &DevelopmentReadiness,
    context: &DevelopmentContextPack,
) -> Vec<String> {
    let mut warnings = readiness.warnings.clone();
    warnings.extend(readiness.blockers.clone());
    warnings.extend(context.warnings.clone());
    if inspection.dirty {
        warnings.push(
            "Worktree sucio: se puede diagnosticar, pero no se debe preparar apply hasta limpiar cambios."
                .to_string(),
        );
    }
    if !inspection.is_one_epis {
        warnings
            .push("Repo sin perfil OneEpis completo; autonomia reducida a revision.".to_string());
    }
    dedupe(warnings)
}

fn status_for(
    blockers: &[String],
    selected_candidate: &Option<EvolutionCandidate>,
    ranked_candidates: &[RankedEvolutionCandidate],
) -> &'static str {
    if !blockers.is_empty() {
        return "blocked";
    }
    if selected_candidate.is_some() {
        return "ready";
    }
    if ranked_candidates
        .iter()
        .any(|item| item.score.verdict == "review_only")
    {
        "review_only"
    } else {
        "blocked"
    }
}

fn summary_for(
    status: &str,
    inspection: &RepoInspection,
    selected_candidate: &Option<EvolutionCandidate>,
    ranked_candidates: &[RankedEvolutionCandidate],
) -> String {
    match (status, selected_candidate) {
        ("ready", Some(candidate)) => format!(
            "{} tiene un microproceso recomendado: {}.",
            inspection.project_name, candidate.title
        ),
        ("review_only", _) => format!(
            "{} solo tiene candidatos para revision humana; falta gate o hay alcance ambiguo.",
            inspection.project_name
        ),
        ("blocked", _)
            if ranked_candidates
                .iter()
                .any(|item| item.score.verdict == "blocked") =>
        {
            format!(
                "{} bloqueo la evolucion supervisada por una bandera prohibida de gobernanza.",
                inspection.project_name
            )
        }
        _ => format!(
            "{} no tiene un microproceso ejecutable con la informacion actual.",
            inspection.project_name
        ),
    }
}

fn next_actions(
    status: &str,
    selected_candidate: &Option<EvolutionCandidate>,
    blockers: &[String],
    warnings: &[String],
) -> Vec<String> {
    if status == "blocked" && !blockers.is_empty() {
        return blockers
            .iter()
            .take(3)
            .cloned()
            .chain(["Resolver bloqueo y volver a pedir Evolucion.".to_string()])
            .collect();
    }
    if let Some(candidate) = selected_candidate {
        return vec![
            format!("Usar este objetivo: {}", candidate.objective),
            format!(
                "Preparar paquete/contexto y PatchDraft solo con gates: {}.",
                join_or_empty(&candidate.gates)
            ),
            "No aplicar cambios hasta tener PatchDraft revisado, repo limpio y confirmacion humana."
                .to_string(),
        ];
    }
    if !warnings.is_empty() {
        return warnings
            .iter()
            .take(2)
            .cloned()
            .chain(["Reducir objetivo o agregar gate verificable antes de PatchDraft.".to_string()])
            .collect();
    }
    vec!["Reducir objetivo y volver a calcular evolucion supervisada.".to_string()]
}

fn infer_dimension(input: &str) -> String {
    let text = normalized_text(input);
    if contains_any(&text, &["ollama", "ia local", "no external", "ia externa"]) {
        "ai_local".to_string()
    } else if contains_any(
        &text,
        &["secret", "phi", "permiso", "auditoria", "seguridad"],
    ) {
        "security".to_string()
    } else if contains_any(
        &text,
        &["near-limit", "tamano", "size", "archivo", "reducir"],
    ) {
        "anti_bloat".to_string()
    } else if contains_any(&text, &["test", "gate", "contrato", "tipo"]) {
        "executable_learning".to_string()
    } else if contains_any(&text, &["paciente", "ficha", "api", "openapi", "clin"]) {
        "clinical_truth".to_string()
    } else {
        "governance_fit".to_string()
    }
}

fn infer_risk(input: &str) -> String {
    let text = normalized_text(input);
    if contains_any(
        &text,
        &["nuevo", "pantalla", "api", "postgres", "firma", "receta"],
    ) {
        "yellow".to_string()
    } else {
        "green".to_string()
    }
}

fn verdict_rank(verdict: &str) -> i32 {
    match verdict {
        "executable" => 4,
        "review_only" => 3,
        "rejected" => 2,
        "blocked" => 1,
        _ => 0,
    }
}

fn normalized_text(input: &str) -> String {
    input.to_ascii_lowercase()
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
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
        "microciclo".to_string()
    } else {
        slug.chars().take(48).collect()
    }
}

fn join_or_empty(items: &[String]) -> String {
    if items.is_empty() {
        "sin_gate".to_string()
    } else {
        items.join(", ")
    }
}

fn push_unique(items: &mut Vec<String>, item: &str) {
    if !items.iter().any(|candidate| candidate == item) {
        items.push(item.to_string());
    }
}

fn dedupe(items: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for item in items {
        push_unique(&mut deduped, &item);
    }
    deduped
}

fn dedupe_candidates(candidates: Vec<EvolutionCandidate>) -> Vec<EvolutionCandidate> {
    let mut deduped = Vec::new();
    for candidate in candidates {
        if !deduped
            .iter()
            .any(|item: &EvolutionCandidate| item.id == candidate.id)
        {
            deduped.push(candidate);
        }
    }
    deduped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{
        ContextPackFile, DevelopmentReadiness, GovernanceDocument, ReadinessCheck,
        SuggestedMicrocycle, WorkPackageTest,
    };

    #[test]
    fn evolution_plan_is_read_only_and_selects_local_ai_boundary() {
        let inspection = inspection(false);
        let readiness = readiness("ready");
        let package = package("Evitar IA externa y confirmar Ollama local");
        let context = context();

        let plan = build_evolution_plan(
            &inspection,
            &readiness,
            &package,
            &context,
            "Agregar check:no-external-ai para mantener Ollama como unico proveedor",
        );

        assert_eq!(plan.status, "ready");
        assert!(plan
            .local_only_boundary
            .contains("sin escritura automatica"));
        assert!(plan
            .selected_candidate
            .as_ref()
            .is_some_and(|candidate| candidate.dimension == "ai_local"));
        assert!(plan
            .ranked_candidates
            .iter()
            .any(|item| item.score.verdict == "executable"));
    }

    #[test]
    fn forbidden_external_ai_dashboard_is_blocked() {
        let inspection = inspection(false);
        let readiness = readiness("ready");
        let package = package("Crear dashboard con ChatGPT");
        let context = context();

        let plan = build_evolution_plan(
            &inspection,
            &readiness,
            &package,
            &context,
            "Crear dashboard con ChatGPT y RAG para decidir acciones clinicas",
        );

        assert_eq!(plan.status, "blocked");
        assert!(plan.selected_candidate.is_none());
        assert!(plan.ranked_candidates.iter().any(|item| {
            item.score.verdict == "blocked"
                && item
                    .score
                    .reasons
                    .iter()
                    .any(|reason| reason.contains("IA externa"))
        }));
    }

    #[test]
    fn anti_bloat_candidate_wins_near_limit_objective() {
        let inspection = inspection(false);
        let readiness = readiness("ready");
        let package = package("Reducir archivo clinico near-limit");
        let context = context();

        let plan = build_evolution_plan(
            &inspection,
            &readiness,
            &package,
            &context,
            "Reducir archivo clinico near-limit sin cambiar comportamiento",
        );

        assert_eq!(plan.status, "ready");
        let selected = plan.selected_candidate.expect("selected candidate");
        assert_eq!(selected.dimension, "anti_bloat");
        assert!(selected.gates.contains(&"check:size".to_string()));
    }

    #[test]
    fn missing_gates_keeps_candidate_review_only() {
        let mut inspection = inspection(false);
        inspection.declared_gates.clear();
        let readiness = readiness("ready");
        let mut package = package("Convertir regla documentada en test");
        package.gates.clear();
        let context = context();

        let plan = build_evolution_plan(
            &inspection,
            &readiness,
            &package,
            &context,
            "Convertir regla documentada en test",
        );

        assert_eq!(plan.status, "review_only");
        assert!(plan.selected_candidate.is_none());
        assert!(plan
            .ranked_candidates
            .iter()
            .any(|item| item.score.verdict == "review_only"));
    }

    fn inspection(dirty: bool) -> RepoInspection {
        RepoInspection {
            repo_path: "C:\\OneEpis".to_string(),
            project_name: "OneEpis".to_string(),
            is_git_repo: true,
            is_one_epis: true,
            current_branch: "codex/test".to_string(),
            dirty,
            status_text: String::new(),
            governance_documents: vec![GovernanceDocument {
                path: "docs/GOVERNANCE.md".to_string(),
                title: "Gobernanza".to_string(),
                sha256: "test".to_string(),
                bytes: 1,
                present: true,
            }],
            declared_gates: vec![
                "check:api".to_string(),
                "check:web".to_string(),
                "check:contract".to_string(),
                "check:size".to_string(),
            ],
            detected_rules: Vec::new(),
            blocks: Vec::new(),
        }
    }

    fn readiness(status: &str) -> DevelopmentReadiness {
        DevelopmentReadiness {
            repo_path: "C:\\OneEpis".to_string(),
            profile: "oneepis".to_string(),
            status: status.to_string(),
            summary: "ready".to_string(),
            checks: vec![ReadinessCheck {
                name: "worktree-clean".to_string(),
                status: status.to_string(),
                detail: String::new(),
                action: String::new(),
            }],
            blockers: Vec::new(),
            warnings: Vec::new(),
            next_actions: Vec::new(),
            suggested_microcycles: vec![SuggestedMicrocycle {
                title: "Dieta de archivo clinico near-limit".to_string(),
                objective: "Reducir un archivo clinico near-limit sin cambiar comportamiento y validar con check:size.".to_string(),
                risk_level: "green".to_string(),
                gates: vec!["check:size".to_string()],
                reason: "Mejora mantenibilidad.".to_string(),
            }],
            required_gates: vec![
                "check:api".to_string(),
                "check:web".to_string(),
                "check:contract".to_string(),
                "check:size".to_string(),
            ],
            local_model_summary: String::new(),
        }
    }

    fn package(objective: &str) -> DevelopmentWorkPackage {
        DevelopmentWorkPackage {
            repo_path: "C:\\OneEpis".to_string(),
            title: "Paquete".to_string(),
            objective: objective.to_string(),
            status: "ready_to_draft".to_string(),
            summary: "Listo".to_string(),
            branch_strategy: "agent/test".to_string(),
            files_to_inspect: vec![
                "AGENTS.md".to_string(),
                "docs/GOVERNANCE.md".to_string(),
                "scripts/check-file-size.mjs".to_string(),
                "apps/api/src/oneepis_api/services/clinical_intent.py".to_string(),
            ],
            implementation_steps: Vec::new(),
            test_plan: vec![WorkPackageTest {
                gate: "check:size".to_string(),
                command: "npm run check:size".to_string(),
                purpose: "Validar tamano".to_string(),
                required: true,
            }],
            acceptance_criteria: Vec::new(),
            stop_conditions: Vec::new(),
            gates: vec!["check:size".to_string()],
            warnings: Vec::new(),
            can_draft: true,
            can_apply: false,
        }
    }

    fn context() -> DevelopmentContextPack {
        DevelopmentContextPack {
            repo_path: "C:\\OneEpis".to_string(),
            objective: "Reducir archivo clinico near-limit".to_string(),
            status: "ready".to_string(),
            summary: "Contexto".to_string(),
            files: vec![
                ContextPackFile {
                    path: "AGENTS.md".to_string(),
                    kind: "file".to_string(),
                    bytes: 20,
                    lines: 2,
                    sha256: "abc".to_string(),
                    summary: "Guia".to_string(),
                    excerpt: "Ollama local, sin PHI.".to_string(),
                },
                ContextPackFile {
                    path: "apps/api/src/oneepis_api/services/clinical_intent.py".to_string(),
                    kind: "file".to_string(),
                    bytes: 200,
                    lines: 20,
                    sha256: "def".to_string(),
                    summary: "Servicio clinico".to_string(),
                    excerpt: "Reglas clinicas".to_string(),
                },
            ],
            warnings: Vec::new(),
            prompt_notes: Vec::new(),
            gates: vec!["check:size".to_string()],
            total_bytes: 220,
            max_bytes: 1024,
        }
    }
}
