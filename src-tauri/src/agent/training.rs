use crate::agent::repo::{canonical_repo, git, inspect_repository};
use crate::agent::safety::sha256_hex;
use crate::agent::types::{
    TrainingEvaluation, TrainingEvaluationItem, TrainingPlan, TrainingRequest, TrainingRun,
    TrainingScenario,
};

use chrono::Utc;
use std::path::Path;

const MAX_TRAINING_CYCLES: u8 = 3;

const TRAINING_RULES: &[&str] = &[
    "Una tarea por rama agent/train-*.",
    "Maximo 3 ciclos concatenados.",
    "No hay push automatico.",
    "Si necesita tabla, endpoint, permiso nuevo o ruta nueva, detenerse y crear plan.",
    "Si toca escritura clinica: API, PostgreSQL, permisos, auditoria, OpenAPI y tests son obligatorios.",
    "Si toca IA: siempre opcional, lateral, sin diagnostico ni escritura automatica.",
    "Usar solo IA local: Ollama y reglas locales; sin proveedores externos.",
];

pub fn list_training_scenarios() -> Vec<TrainingScenario> {
    training_scenarios()
}

pub fn evaluate_training_scenarios(repo_path: &str) -> Result<TrainingEvaluation, String> {
    let inspection = inspect_repository(repo_path)?;
    let mut items = training_scenarios()
        .into_iter()
        .map(|scenario| evaluate_training_scenario(&inspection, scenario))
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .success_score
            .cmp(&left.success_score)
            .then_with(|| left.scenario.id.cmp(&right.scenario.id))
    });

    let blocked = items
        .iter()
        .filter(|item| item.success_level == "blocked")
        .count();
    let high_confidence = items
        .iter()
        .filter(|item| item.success_level == "high")
        .count();
    let medium_confidence = items
        .iter()
        .filter(|item| item.success_level == "medium")
        .count();
    let low_confidence = items
        .iter()
        .filter(|item| item.success_level == "low")
        .count();
    let recommended_order = items
        .iter()
        .filter(|item| matches!(item.success_level.as_str(), "high" | "medium"))
        .map(|item| item.scenario.id.clone())
        .collect::<Vec<_>>();
    let mut warnings = Vec::new();
    if inspection.dirty {
        warnings.push(
            "Hay cambios pendientes; la evaluacion puede explicar TRAIN, pero no debe preparar ramas."
                .to_string(),
        );
    }
    if !inspection.is_one_epis {
        warnings.push(
            "El adaptador OneEpis no esta activo; TRAIN queda solo como referencia.".to_string(),
        );
    }

    let status = if blocked == items.len() {
        "blocked"
    } else if high_confidence + medium_confidence > 0 {
        "ready"
    } else {
        "review_only"
    };
    let summary = format!(
        "Evaluacion TRAIN: {high_confidence} alta confianza, {medium_confidence} media, {low_confidence} baja y {blocked} bloqueada."
    );

    Ok(TrainingEvaluation {
        repo_path: inspection.repo_path,
        status: status.to_string(),
        summary,
        total: items.len(),
        high_confidence,
        medium_confidence,
        low_confidence,
        blocked,
        recommended_order,
        items,
        warnings,
        no_push: true,
        local_ai_only: true,
    })
}

pub fn training_plan(request: TrainingRequest) -> Result<TrainingPlan, String> {
    let scenario = training_scenario(&request.scenario_id)?;
    let inspection = inspect_repository(&request.repo_path)?;
    let mut blockers = common_training_blocks(&inspection, &scenario, request.cycles);
    let mut warnings = Vec::new();

    if scenario.execution_mode == "plan_only" {
        warnings.push(
            "Este escenario debe terminar en plan/contrato minimo; no debe aplicar UI amplia ni cambios clinicos."
                .to_string(),
        );
    }
    if !scenario.manual_gates.is_empty() {
        warnings.push(format!(
            "Requiere verificacion manual/local adicional: {}.",
            scenario.manual_gates.join(", ")
        ));
    }
    if inspection.dirty && inspection.current_branch == scenario.branch {
        warnings.push(
            "Hay cambios pendientes en la rama de entrenamiento; solo continuar si son del escenario."
                .to_string(),
        );
    } else if inspection.dirty {
        blockers.push(format!(
            "Hay cambios pendientes fuera de {}; guarda o descarta antes de preparar TRAIN.",
            scenario.branch
        ));
    }

    let status = if blockers.is_empty() {
        if scenario.execution_mode == "plan_only" {
            "plan_only_ready"
        } else {
            "ready"
        }
    } else {
        "blocked"
    };
    let next_actions = if blockers.is_empty() {
        vec![
            format!("Preparar rama local {}.", scenario.branch),
            "Crear brief con contexto y, si se usa IA, consultar solo Ollama.".to_string(),
            "Ejecutar como maximo 3 ciclos y detenerse si aparece una condicion de stop."
                .to_string(),
            "Correr gates declarados antes de cualquier commit local manual.".to_string(),
        ]
    } else {
        blockers
            .iter()
            .take(3)
            .cloned()
            .chain(["No iniciar entrenamiento hasta resolver bloqueos.".to_string()])
            .collect()
    };

    Ok(TrainingPlan {
        repo_path: inspection.repo_path,
        scenario,
        status: status.to_string(),
        cycles: request.cycles,
        max_cycles: MAX_TRAINING_CYCLES,
        blockers,
        warnings,
        next_actions,
        no_push: true,
        local_ai_only: true,
        summary: "Plan de entrenamiento local gobernado para OneEpis.".to_string(),
    })
}

fn evaluate_training_scenario(
    inspection: &crate::agent::types::RepoInspection,
    scenario: TrainingScenario,
) -> TrainingEvaluationItem {
    let mut blockers = common_training_blocks(inspection, &scenario, 1);
    let mut warnings = Vec::new();

    if inspection.dirty && inspection.current_branch == scenario.branch {
        warnings.push(
            "Hay cambios pendientes en la rama de entrenamiento; revisar que pertenezcan a este TRAIN."
                .to_string(),
        );
    } else if inspection.dirty {
        blockers.push(format!(
            "Hay cambios pendientes fuera de {}; guarda o descarta antes de preparar TRAIN.",
            scenario.branch
        ));
    }
    if scenario.execution_mode == "plan_only" {
        warnings.push(
            "Este TRAIN se considera exitoso si termina en plan/contrato minimo.".to_string(),
        );
    }
    if !scenario.manual_gates.is_empty() {
        warnings.push(format!(
            "Necesita verificacion manual/local adicional: {}.",
            scenario.manual_gates.join(", ")
        ));
    }

    let risks = training_risks(&scenario);
    let strengths = training_strengths(&scenario);
    let mut score = if blockers.is_empty() { 88 } else { 24 };
    score -= training_mode_penalty(&scenario.execution_mode);
    score -= (scenario.manual_gates.len() as i32 * 8).min(24);
    if scenario.gates.is_empty() && !scenario.manual_gates.is_empty() {
        score -= 12;
    }
    if scenario.gates.iter().any(|gate| gate == "check:e2e") {
        score -= 4;
    }
    if scenario
        .allowed_surfaces
        .iter()
        .any(|surface| surface.contains("permissions") || surface.contains("nursing"))
    {
        score -= 8;
    }
    if !blockers.is_empty() {
        score = score.min(24);
    }
    score = score.clamp(0, 100);

    let success_level = if !blockers.is_empty() {
        "blocked"
    } else if score >= 75 {
        "high"
    } else if score >= 55 {
        "medium"
    } else {
        "low"
    };
    let readiness_status = if !blockers.is_empty() {
        "blocked"
    } else if scenario.execution_mode == "plan_only" {
        "plan_only_ready"
    } else {
        "ready"
    };
    let verdict = training_verdict(success_level, &scenario);
    let next_actions = training_evaluation_next_actions(&scenario, success_level, &blockers);

    TrainingEvaluationItem {
        scenario: scenario.clone(),
        readiness_status: readiness_status.to_string(),
        success_level: success_level.to_string(),
        success_score: score,
        verdict,
        official_gates: scenario.gates.clone(),
        manual_gates: scenario.manual_gates.clone(),
        blockers,
        warnings,
        strengths,
        risks,
        next_actions,
    }
}

fn training_strengths(scenario: &TrainingScenario) -> Vec<String> {
    let mut strengths = Vec::new();
    if !scenario.gates.is_empty() {
        strengths.push(format!(
            "Tiene gates oficiales declarados: {}.",
            scenario.gates.join(", ")
        ));
    }
    if scenario.manual_gates.is_empty() {
        strengths.push("No depende de verificacion manual adicional.".to_string());
    }
    if scenario.allowed_surfaces.len() <= 3 {
        strengths.push("Superficie acotada para revisar diff pequeno.".to_string());
    }
    if scenario.execution_mode == "plan_only" || scenario.execution_mode == "docs_only" {
        strengths.push("Puede cerrarse sin tocar conducta clinica.".to_string());
    }
    strengths
}

fn training_risks(scenario: &TrainingScenario) -> Vec<String> {
    let mut risks = Vec::new();
    if !scenario.manual_gates.is_empty() {
        risks.push(format!(
            "Falta convertir a comando tipado: {}.",
            scenario.manual_gates.join(", ")
        ));
    }
    if scenario.gates.iter().any(|gate| gate == "check:e2e") {
        risks.push("Depende de E2E; puede fallar por selectores o datos de prueba.".to_string());
    }
    if scenario.execution_mode == "manual_gate_required" {
        risks.push("Requiere entorno local adicional antes de declarar exito.".to_string());
    }
    if scenario.execution_mode == "failure_drill" {
        risks.push("Debe simular fallo sin romper guardado clinico.".to_string());
    }
    if scenario.execution_mode == "experiment_local" {
        risks.push(
            "Debe medir con dataset sintetico acotado y no commitear datos pesados.".to_string(),
        );
    }
    if scenario
        .allowed_surfaces
        .iter()
        .any(|surface| surface.contains("permissions") || surface.contains("nursing"))
    {
        risks.push("Toca permisos o roles clinicos; exige revision humana cuidadosa.".to_string());
    }
    risks
}

fn training_mode_penalty(mode: &str) -> i32 {
    match mode {
        "guided_refactor" => 4,
        "audit_then_patch" => 8,
        "test_refactor" => 8,
        "manual_gate_required" => 18,
        "failure_drill" => 18,
        "plan_only" => 0,
        "audit_only" => 14,
        "experiment_local" => 24,
        "docs_only" => 4,
        _ => 10,
    }
}

fn training_verdict(level: &str, scenario: &TrainingScenario) -> String {
    match level {
        "high" => format!(
            "Alta probabilidad de cierre gobernado para {}.",
            scenario.id
        ),
        "medium" => format!(
            "Probabilidad media: {} necesita control humano o verificacion adicional.",
            scenario.id
        ),
        "low" => format!(
            "Probabilidad baja: {} debe mejorar gates o alcance antes de resolver.",
            scenario.id
        ),
        _ => format!("{} bloqueado antes de iniciar entrenamiento.", scenario.id),
    }
}

fn training_evaluation_next_actions(
    scenario: &TrainingScenario,
    success_level: &str,
    blockers: &[String],
) -> Vec<String> {
    if !blockers.is_empty() {
        return blockers
            .iter()
            .take(2)
            .cloned()
            .chain(["Resolver bloqueos antes de preparar rama TRAIN.".to_string()])
            .collect();
    }
    let mut actions = Vec::new();
    if scenario.execution_mode == "plan_only" {
        actions.push("Cerrar como plan/contrato minimo, sin aplicar UI amplia.".to_string());
    } else {
        actions.push(format!("Preparar rama local {}.", scenario.branch));
    }
    if !scenario.manual_gates.is_empty() {
        actions.push(format!(
            "Convertir o ejecutar verificacion manual/local: {}.",
            scenario.manual_gates.join(", ")
        ));
    }
    if success_level == "low" {
        actions.push("Antes de resolver, crear gate tipado o reducir alcance.".to_string());
    } else {
        actions.push(
            "Mantener maximo 3 ciclos, solo IA local y detenerse ante condicion de stop."
                .to_string(),
        );
    }
    actions
}

pub async fn prepare_training_scenario(request: TrainingRequest) -> Result<TrainingRun, String> {
    let scenario = training_scenario(&request.scenario_id)?;
    let plan = training_plan(request.clone())?;
    if !plan.blockers.is_empty() {
        return Ok(blocked_run(
            &plan.repo_path,
            &scenario,
            request.cycles,
            plan.blockers,
        ));
    }
    let repo = canonical_repo(&request.repo_path)?;
    let branch = ensure_training_branch(&repo, &scenario.branch)?;
    Ok(TrainingRun {
        id: run_id(&plan.repo_path, &scenario.id),
        scenario_id: scenario.id,
        status: "ready_for_training".to_string(),
        repo_path: plan.repo_path,
        branch,
        cycles: request.cycles,
        blockers: Vec::new(),
        warnings: plan
            .warnings
            .into_iter()
            .chain(["Rama preparada; no se hicieron cambios ni push.".to_string()])
            .collect(),
        next_actions: vec![
            "Pedir brief local gobernado con el objetivo TRAIN seleccionado.".to_string(),
            "Usar solo Ollama si se consulta modelo; no usar IA externa.".to_string(),
            "Si aparece una condicion de stop, documentar plan y detenerse.".to_string(),
        ],
        no_push: true,
        local_ai_only: true,
        summary: "Escenario TRAIN preparado en rama local.".to_string(),
    })
}

fn training_scenarios() -> Vec<TrainingScenario> {
    vec![
        scenario(
            "TRAIN-001",
            "Reducir clinical_intent.py sin cambiar conducta",
            "Extraer otra familia de reglas deterministicas manteniendo respuestas identicas.",
            "agent/train-001-reducir-clinical-intent",
            &[
                "Refactor backend con tests.",
                "Contrato estable.",
                "No tocar prompts ni endpoints.",
            ],
            &["check:api", "check:contract", "check:size"],
            &[],
            &["clinical_intent.py", "clinical_intent_*"],
            "guided_refactor",
        ),
        scenario(
            "TRAIN-002",
            "Detectar drift entre OpenAPI y tipos frontend",
            "Auditar tipos manuales Assistant Read / Clinical Record contra openapi.json.",
            "agent/train-002-openapi-frontend-drift",
            &[
                "Distinguir drift real de ruido.",
                "Proponer patch minimo.",
            ],
            &["check:contract", "check:web"],
            &[],
            &["packages/contracts/openapi.json", "assistant-read", "clinical-record", "types"],
            "audit_then_patch",
        ),
        scenario(
            "TRAIN-003",
            "Mejorar una pantalla near-limit sin cambiar UX",
            "Tomar un archivo React sobre 300 lineas y extraer componentes de dominio.",
            "agent/train-003-react-near-limit",
            &[
                "Refactor UI puro.",
                "Preservar textos, rutas y permisos.",
            ],
            &["check:web", "check:e2e"],
            &[],
            &["apps/web/src/components/clinical"],
            "guided_refactor",
        ),
        scenario(
            "TRAIN-004",
            "Endurecer un flujo E2E fragil",
            "Encontrar selectores ambiguos en Playwright y hacerlos mas robustos.",
            "agent/train-004-e2e-fragil",
            &[
                "No agregar snapshots pesados.",
                "Usar roles, scopes y texto exacto.",
            ],
            &["check:e2e"],
            &[],
            &["apps/web/tests/e2e"],
            "test_refactor",
        ),
        scenario(
            "TRAIN-005",
            "Auditoria de permisos enfermeria/preconsulta",
            "Verificar que enfermeria solo pueda completar preconsulta minima y nada mas.",
            "agent/train-005-permisos-enfermeria",
            &[
                "Revisar backend, frontend y tests.",
                "No abrir permisos generales.",
            ],
            &["check:api", "check:web"],
            &[],
            &["permissions", "preconsult", "ambulatory", "nursing"],
            "audit_then_patch",
        ),
        scenario(
            "TRAIN-006",
            "Papel serio, sin documento nuevo",
            "Mejorar jerarquia visual de una ruta print existente.",
            "agent/train-006-papel-print",
            &[
                "CSS print.",
                "No habilitar receta, firma ni folio legal.",
            ],
            &["check:web", "check:e2e"],
            &[],
            &["print", "paper", "css"],
            "guided_refactor",
        ),
        scenario(
            "TRAIN-007",
            "Migracion limpia desde PostgreSQL vacio",
            "Crear o verificar una prueba de bootstrap Alembic desde base temporal limpia.",
            "agent/train-007-bootstrap-alembic",
            &[
                "Migraciones reales.",
                "No confiar solo en SQLite/tests API.",
            ],
            &["check:api"],
            &["alembic:bootstrap-postgresql-local"],
            &["alembic", "migrations", "postgresql"],
            "manual_gate_required",
        ),
        scenario(
            "TRAIN-008",
            "Simular fallo de Ollama sin romper ficha",
            "Probar que la UI y API siguen operativas si Ollama no responde.",
            "agent/train-008-ollama-fallback",
            &[
                "IA opcional.",
                "Fallback controlado.",
                "No bloquear guardado clinico.",
            ],
            &["check:web"],
            &["backend:mocked-ollama-tests"],
            &["ollama", "ai", "fallback"],
            "failure_drill",
        ),
        scenario(
            "TRAIN-009",
            "Corregir contaminacion demo",
            "Auditar fixtures/demo para nombres externos, datos realistas o residuos de otros proyectos.",
            "agent/train-009-contaminacion-demo",
            &[
                "Higiene clinica.",
                "Datos sinteticos obvios.",
                "No PHI.",
            ],
            &["check:web"],
            &["text-search:documented"],
            &["demo", "fixtures"],
            "audit_then_patch",
        ),
        scenario(
            "TRAIN-010",
            "Preparar contrato antes de UI amplia",
            "Tomar una idea futura y producir solo contrato/plan minimo.",
            "agent/train-010-contrato-antes-ui",
            &[
                "Detenerse antes de implementar si falta API/PostgreSQL/permisos/auditoria.",
                "Docs minimas sin documentos nuevos innecesarios.",
            ],
            &["check:size"],
            &["git:diff-check"],
            &["docs", "contracts"],
            "plan_only",
        ),
        scenario(
            "TRAIN-011",
            "Refactor de cliente API por dominio",
            "Separar funciones de un cliente frontend grande sin cambiar imports publicos.",
            "agent/train-011-cliente-api-dominio",
            &[
                "Compatibilidad temporal.",
                "Evitar barrels eternos.",
            ],
            &["check:web"],
            &[],
            &["apps/web/src/lib/api"],
            "guided_refactor",
        ),
        scenario(
            "TRAIN-012",
            "Revisar seguridad CI",
            "Confirmar que gitleaks bloquea y que el resto sigue report-only documentado.",
            "agent/train-012-seguridad-ci",
            &[
                "No endurecer senales ruidosas sin politica.",
                "Distinguir bloqueante de report-only.",
            ],
            &[],
            &["git:diff-check", "ci:remote-if-pr"],
            &[".github", "security", "gitleaks"],
            "audit_only",
        ),
        scenario(
            "TRAIN-013",
            "Performance con dataset sintetico",
            "Crear una prueba local controlada para ficha con muchos eventos ficticios.",
            "agent/train-013-performance-sintetica",
            &[
                "No commitear datos pesados.",
                "Medir antes de optimizar.",
            ],
            &[],
            &["script-temporal-or-bounded-test"],
            &["performance", "synthetic", "events"],
            "experiment_local",
        ),
        scenario(
            "TRAIN-014",
            "Revisar ausencia de fallback silencioso por ID",
            "Auditar rutas print o clinicas con [id] para confirmar que no muestran otro registro si falta el solicitado.",
            "agent/train-014-identidad-estricta-id",
            &[
                "Seguridad clinica por identidad estricta.",
                "No mostrar registros sustitutos.",
            ],
            &["check:e2e"],
            &[],
            &["print", "routes", "[id]", "not-found"],
            "test_refactor",
        ),
        scenario(
            "TRAIN-015",
            "Actualizar documentacion viva despues de merge real",
            "Detectar contradicciones entre CURRENT_STATE, GOVERNANCE, CODEX_PLAN y codigo.",
            "agent/train-015-documentacion-viva",
            &[
                "Docs minimas.",
                "No crear documentos nuevos.",
            ],
            &["check:size"],
            &["git:diff-check"],
            &["CURRENT_STATE", "GOVERNANCE", "CODEX_PLAN", "docs"],
            "docs_only",
        ),
    ]
}

fn scenario(
    id: &str,
    title: &str,
    objective: &str,
    branch: &str,
    teaches: &[&str],
    gates: &[&str],
    manual_gates: &[&str],
    allowed_surfaces: &[&str],
    execution_mode: &str,
) -> TrainingScenario {
    TrainingScenario {
        id: id.to_string(),
        title: title.to_string(),
        objective: objective.to_string(),
        branch: branch.to_string(),
        teaches: teaches.iter().map(|item| item.to_string()).collect(),
        gates: gates.iter().map(|item| item.to_string()).collect(),
        manual_gates: manual_gates.iter().map(|item| item.to_string()).collect(),
        allowed_surfaces: allowed_surfaces
            .iter()
            .map(|item| item.to_string())
            .collect(),
        stop_conditions: vec![
            "tabla nueva".to_string(),
            "endpoint nuevo".to_string(),
            "permiso nuevo".to_string(),
            "ruta nueva".to_string(),
            "IA diagnostica o protagonista".to_string(),
            "escritura clinica automatica".to_string(),
        ],
        execution_mode: execution_mode.to_string(),
        instructions: TRAINING_RULES.iter().map(|item| item.to_string()).collect(),
    }
}

fn training_scenario(id: &str) -> Result<TrainingScenario, String> {
    training_scenarios()
        .into_iter()
        .find(|scenario| scenario.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("Escenario TRAIN no registrado: {id}."))
}

fn common_training_blocks(
    inspection: &crate::agent::types::RepoInspection,
    scenario: &TrainingScenario,
    cycles: u8,
) -> Vec<String> {
    let mut blockers = Vec::new();
    if !inspection.is_git_repo {
        blockers.push("El proyecto objetivo debe ser un repo Git.".to_string());
    }
    if !inspection.is_one_epis {
        blockers.push("TRAIN solo se ejecuta con adaptador OneEpis activo.".to_string());
    }
    if cycles == 0 || cycles > MAX_TRAINING_CYCLES {
        blockers.push(format!(
            "TRAIN permite entre 1 y {MAX_TRAINING_CYCLES} ciclos concatenados; solicitado: {cycles}."
        ));
    }
    for gate in &scenario.gates {
        if gate.starts_with("check:") && !inspection.declared_gates.contains(gate) {
            blockers.push(format!(
                "Gate requerido no declarado en package.json: {gate}."
            ));
        }
    }
    blockers.extend(
        inspection
            .blocks
            .iter()
            .filter(|block| !block.to_ascii_lowercase().contains("worktree sucio"))
            .cloned(),
    );
    blockers
}

fn blocked_run(
    repo_path: &str,
    scenario: &TrainingScenario,
    cycles: u8,
    blockers: Vec<String>,
) -> TrainingRun {
    TrainingRun {
        id: run_id(repo_path, &scenario.id),
        scenario_id: scenario.id.clone(),
        status: "blocked".to_string(),
        repo_path: repo_path.to_string(),
        branch: scenario.branch.clone(),
        cycles,
        blockers,
        warnings: Vec::new(),
        next_actions: vec![
            "Resolver bloqueos antes de preparar rama TRAIN.".to_string(),
            "No ejecutar entrenamiento ni consultar IA hasta que el plan este listo.".to_string(),
        ],
        no_push: true,
        local_ai_only: true,
        summary: format!("{} bloqueado.", scenario.id),
    }
}

fn ensure_training_branch(repo: &Path, branch: &str) -> Result<String, String> {
    let current_branch = git(repo, &["branch", "--show-current"])?;
    if current_branch == branch {
        return Ok(branch.to_string());
    }
    if git(repo, &["rev-parse", "--verify", branch]).is_ok() {
        git(repo, &["switch", branch])?;
    } else {
        let base = training_base_branch(repo, &current_branch);
        if current_branch != base {
            git(repo, &["switch", &base])?;
        }
        git(repo, &["switch", "-c", branch])?;
    }
    Ok(branch.to_string())
}

fn training_base_branch(repo: &Path, current_branch: &str) -> String {
    for candidate in ["main", "master"] {
        if git(repo, &["rev-parse", "--verify", candidate]).is_ok() {
            return candidate.to_string();
        }
    }
    current_branch.to_string()
}

fn run_id(repo_path: &str, scenario_id: &str) -> String {
    let seed = format!("{}:{}:{}", repo_path, scenario_id, Utc::now());
    let digest = sha256_hex(seed.as_bytes());
    format!("train-{}", &digest[..16])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    #[test]
    fn registry_contains_all_training_scenarios() {
        let scenarios = list_training_scenarios();
        assert_eq!(scenarios.len(), 15);
        let ids = scenarios
            .iter()
            .map(|scenario| scenario.id.as_str())
            .collect::<Vec<_>>();
        for index in 1..=15 {
            assert!(ids.contains(&format!("TRAIN-{index:03}").as_str()));
        }
        assert!(scenarios.iter().all(|scenario| scenario.local_ai_rule()));
    }

    #[test]
    fn plan_blocks_more_than_three_cycles() {
        let repo = temp_repo("cycles");
        let result = training_plan(TrainingRequest {
            repo_path: repo.display().to_string(),
            scenario_id: "TRAIN-001".to_string(),
            cycles: 4,
        })
        .expect("plan");
        assert_eq!(result.status, "blocked");
        assert!(result
            .blockers
            .iter()
            .any(|block| block.contains("3 ciclos")));
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn plan_only_scenario_stays_plan_only() {
        let repo = temp_repo("plan-only");
        let result = training_plan(TrainingRequest {
            repo_path: repo.display().to_string(),
            scenario_id: "TRAIN-010".to_string(),
            cycles: 1,
        })
        .expect("plan");
        assert_eq!(result.status, "plan_only_ready");
        assert!(result.local_ai_only);
        assert!(result.no_push);
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("plan/contrato minimo")));
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn evaluation_scores_all_training_scenarios() {
        let repo = temp_repo("evaluation");
        let result = evaluate_training_scenarios(&repo.display().to_string()).expect("evaluation");
        assert_eq!(result.total, 15);
        assert_eq!(result.blocked, 0);
        assert!(result.high_confidence > 0);
        assert!(result.recommended_order.contains(&"TRAIN-001".to_string()));
        let train_007 = result
            .items
            .iter()
            .find(|item| item.scenario.id == "TRAIN-007")
            .expect("TRAIN-007");
        assert_eq!(train_007.success_level, "medium");
        assert!(train_007
            .risks
            .iter()
            .any(|risk| risk.contains("comando tipado")));
        let train_013 = result
            .items
            .iter()
            .find(|item| item.scenario.id == "TRAIN-013")
            .expect("TRAIN-013");
        assert_eq!(train_013.success_level, "low");
        assert!(result.no_push);
        assert!(result.local_ai_only);
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn evaluation_blocks_dirty_repo() {
        let repo = temp_repo("evaluation-dirty");
        fs::write(repo.join("dirty.txt"), "pending").expect("dirty");
        let result = evaluate_training_scenarios(&repo.display().to_string()).expect("evaluation");
        assert_eq!(result.status, "blocked");
        assert_eq!(result.blocked, 15);
        assert!(result
            .items
            .iter()
            .all(|item| item.success_level == "blocked"));
        let _ = fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn prepare_creates_training_branch_without_changes() {
        let repo = temp_repo("prepare");
        let result = prepare_training_scenario(TrainingRequest {
            repo_path: repo.display().to_string(),
            scenario_id: "TRAIN-004".to_string(),
            cycles: 2,
        })
        .await
        .expect("prepare");
        assert_eq!(result.status, "ready_for_training");
        assert_eq!(result.branch, "agent/train-004-e2e-fragil");
        assert!(git(&repo, &["status", "--short"])
            .expect("status")
            .is_empty());
        let _ = fs::remove_dir_all(repo);
    }

    trait TrainingScenarioExt {
        fn local_ai_rule(&self) -> bool;
    }

    impl TrainingScenarioExt for TrainingScenario {
        fn local_ai_rule(&self) -> bool {
            self.instructions
                .iter()
                .any(|item| item.contains("Usar solo IA local"))
        }
    }

    fn temp_repo(label: &str) -> std::path::PathBuf {
        let repo = std::env::temp_dir().join(format!(
            "oneepis-agent-training-{label}-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&repo).expect("repo dir");
        git(&repo, &["init", "-b", "main"]).expect("git init");
        git(&repo, &["config", "user.email", "agent@example.local"]).expect("git email");
        git(&repo, &["config", "user.name", "OneEpis Agent"]).expect("git name");
        fs::write(
            repo.join("package.json"),
            r#"{"scripts":{"check:api":"echo api","check:contract":"echo contract","check:size":"echo size","check:web":"echo web","check:e2e":"echo e2e"}}"#,
        )
        .expect("package");
        fs::write(repo.join("AGENTS.md"), "OneEpis local training fixture").expect("agents");
        fs::create_dir_all(repo.join("docs")).expect("docs");
        fs::write(repo.join("docs/GOVERNANCE.md"), "OneEpis governance").expect("gov");
        git(&repo, &["add", "."]).expect("add");
        let output = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .arg("commit")
            .arg("-m")
            .arg("init")
            .output()
            .expect("commit");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        repo
    }
}
