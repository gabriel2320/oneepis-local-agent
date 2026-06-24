use crate::agent::gates::run_gate;
use crate::agent::repo::{canonical_repo, git, inspect_repository};
use crate::agent::safety::{sanitize_log, sha256_hex};
use crate::agent::types::{
    LocalProblemPlan, LocalProblemRequest, LocalProblemRun, LocalProblemSpec,
};
use chrono::Utc;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

const GLOBAL_FORBIDDEN_SIGNALS: &[&str] = &[
    "endpoint",
    "new route",
    "ruta nueva",
    "permission",
    "permiso",
    "migration",
    "migracion",
    "alembic",
    "rag",
    "dashboard",
    "receta",
    "prescription",
    "firma clinica",
    "signature",
    "external ai",
    "openai",
    "anthropic",
];

pub fn list_local_problems() -> Vec<LocalProblemSpec> {
    local_problem_specs()
}

pub fn local_problem_plan(request: LocalProblemRequest) -> Result<LocalProblemPlan, String> {
    let problem = local_problem_spec(&request.problem_id)?;
    let inspection = inspect_repository(&request.repo_path)?;
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();

    if !inspection.is_git_repo {
        blockers.push("El proyecto objetivo debe ser un repo Git.".to_string());
    }
    if !inspection.is_one_epis {
        blockers
            .push("El adaptador OneEpis debe estar activo antes de ejecutar LOCAL-*.".to_string());
    }
    for gate in &problem.gates {
        if !inspection.declared_gates.contains(gate) {
            blockers.push(format!("Falta gate declarado en package.json: {gate}."));
        }
    }
    if inspection.dirty && inspection.current_branch != problem.branch {
        blockers.push(format!(
            "Hay cambios pendientes fuera de la rama segura {}; primero guarda o descarta esos cambios.",
            problem.branch
        ));
    } else if inspection.dirty {
        warnings.push(
            "Hay cambios pendientes en la rama segura; solo se podran commitear si pasan validacion LOCAL."
                .to_string(),
        );
    }

    let status = if blockers.is_empty() {
        "ready"
    } else {
        "blocked"
    };
    let next_actions = if blockers.is_empty() {
        vec![
            format!("Preparar rama local con LOCAL {}.", problem.id),
            "Ejecutar el cambio pequeno permitido por la ficha LOCAL.".to_string(),
            "Correr gates y crear commit local sin push automatico.".to_string(),
        ]
    } else {
        blockers
            .iter()
            .take(3)
            .cloned()
            .chain(["No ejecutar cambios hasta resolver bloqueos.".to_string()])
            .collect()
    };

    Ok(LocalProblemPlan {
        repo_path: inspection.repo_path,
        problem,
        status: status.to_string(),
        blockers,
        warnings,
        next_actions,
        no_push: true,
    })
}

pub async fn prepare_local_problem(
    request: LocalProblemRequest,
) -> Result<LocalProblemRun, String> {
    let problem = local_problem_spec(&request.problem_id)?;
    let inspection = inspect_repository(&request.repo_path)?;
    let repo = canonical_repo(&request.repo_path)?;
    let mut blockers = common_blocks(&inspection, &problem);

    if inspection.dirty {
        blockers.push("La preparacion de rama requiere proyecto limpio.".to_string());
    }
    if !blockers.is_empty() {
        return Ok(blocked_run(&inspection.repo_path, &problem, blockers));
    }

    let branch = ensure_problem_branch(&repo, &inspection.current_branch, &problem.branch)?;
    Ok(LocalProblemRun {
        id: run_id(&inspection.repo_path, &problem.id),
        problem_id: problem.id.clone(),
        status: "ready_for_changes".to_string(),
        repo_path: inspection.repo_path,
        branch,
        commit_sha: None,
        changed_files: Vec::new(),
        gate_results: Vec::new(),
        blockers: Vec::new(),
        warnings: vec!["No hay push automatico; el ciclo termina en commit local.".to_string()],
        next_actions: vec![
            "Realizar solo el refactor permitido por esta ficha LOCAL.".to_string(),
            "Ejecutar local-problem-commit para validar gates y crear commit local.".to_string(),
        ],
        no_push: true,
        summary: format!("Rama segura preparada para {}.", problem.id),
    })
}

pub async fn commit_local_problem(request: LocalProblemRequest) -> Result<LocalProblemRun, String> {
    let problem = local_problem_spec(&request.problem_id)?;
    let inspection = inspect_repository(&request.repo_path)?;
    let repo = canonical_repo(&request.repo_path)?;
    let mut blockers = common_blocks(&inspection, &problem);

    if inspection.current_branch != problem.branch {
        blockers.push(format!(
            "El commit LOCAL debe hacerse en rama segura {}; rama actual: {}.",
            problem.branch, inspection.current_branch
        ));
    }

    let changed_files = changed_files(&repo)?;
    if changed_files.is_empty() {
        blockers.push("No hay cambios para commitear.".to_string());
    }

    blockers.extend(validate_changed_files(&problem, &changed_files));
    blockers.extend(validate_diff_content(&repo, &problem, &changed_files)?);

    if !blockers.is_empty() {
        return Ok(LocalProblemRun {
            id: run_id(&inspection.repo_path, &problem.id),
            problem_id: problem.id.clone(),
            status: "blocked".to_string(),
            repo_path: inspection.repo_path,
            branch: inspection.current_branch,
            commit_sha: None,
            changed_files,
            gate_results: Vec::new(),
            blockers,
            warnings: Vec::new(),
            next_actions: vec![
                "Reducir el cambio a la ficha LOCAL y volver a validar.".to_string(),
                "No crear commit hasta que no haya bloqueos.".to_string(),
            ],
            no_push: true,
            summary: format!("{} bloqueado antes de gates o commit.", problem.id),
        });
    }

    let mut gate_results = Vec::new();
    for gate in &problem.gates {
        let result = run_gate(&inspection.repo_path, gate, None, None).await?;
        let passed = result.status == "passed";
        gate_results.push(result);
        if !passed {
            return Ok(LocalProblemRun {
                id: run_id(&inspection.repo_path, &problem.id),
                problem_id: problem.id.clone(),
                status: "blocked".to_string(),
                repo_path: inspection.repo_path,
                branch: inspection.current_branch,
                commit_sha: None,
                changed_files,
                gate_results,
                blockers: vec![format!("Gate {gate} no paso; commit local bloqueado.")],
                warnings: Vec::new(),
                next_actions: vec![
                    "Leer salida del gate, reducir el cambio y volver a correr commit LOCAL."
                        .to_string(),
                ],
                no_push: true,
                summary: format!("{} bloqueado por gate.", problem.id),
            });
        }
    }
    restore_gate_artifacts(&repo, &changed_files)?;

    git_add_files(&repo, &changed_files)?;
    git_commit(&repo, &problem.commit_message)?;
    let commit_sha = git(&repo, &["rev-parse", "HEAD"])?;

    Ok(LocalProblemRun {
        id: run_id(&inspection.repo_path, &problem.id),
        problem_id: problem.id.clone(),
        status: "committed".to_string(),
        repo_path: inspection.repo_path,
        branch: inspection.current_branch,
        commit_sha: Some(commit_sha),
        changed_files,
        gate_results,
        blockers: Vec::new(),
        warnings: vec!["Commit local creado; no se hizo push automatico.".to_string()],
        next_actions: vec![
            "Revisar diff y commit local.".to_string(),
            "Crear PR manual solo si el resultado clinico-tecnico es correcto.".to_string(),
        ],
        no_push: true,
        summary: format!("{} resuelto con commit local.", problem.id),
    })
}

pub async fn solve_local_problem(request: LocalProblemRequest) -> Result<LocalProblemRun, String> {
    let problem = local_problem_spec(&request.problem_id)?;
    let inspection = inspect_repository(&request.repo_path)?;
    let repo = canonical_repo(&request.repo_path)?;
    let mut blockers = common_blocks(&inspection, &problem);

    if inspection.dirty {
        blockers.push(
            "El solver LOCAL requiere proyecto limpio antes de preparar la rama.".to_string(),
        );
    }
    if !blockers.is_empty() {
        return Ok(blocked_run(&inspection.repo_path, &problem, blockers));
    }

    ensure_problem_branch(&repo, &inspection.current_branch, &problem.branch)?;
    match problem.id.as_str() {
        "LOCAL-001" => solve_local_001(&repo)?,
        "LOCAL-002" => solve_local_002(&repo)?,
        "LOCAL-003" => solve_local_003(&repo)?,
        "LOCAL-004" => solve_local_004(&repo)?,
        "LOCAL-005" => solve_local_005(&repo)?,
        "LOCAL-006" => solve_local_006(&repo)?,
        "LOCAL-007" => solve_local_007(&repo)?,
        "LOCAL-008" => solve_local_008(&repo)?,
        "LOCAL-009" => solve_local_009(&repo)?,
        "LOCAL-010" => solve_local_010(&repo)?,
        _ => {
            return Ok(LocalProblemRun {
                id: run_id(&inspection.repo_path, &problem.id),
                problem_id: problem.id.clone(),
                status: "blocked".to_string(),
                repo_path: inspection.repo_path,
                branch: problem.branch,
                commit_sha: None,
                changed_files: Vec::new(),
                gate_results: Vec::new(),
                blockers: vec![format!(
                    "Solver autonomo aun no implementado para {}.",
                    problem.id
                )],
                warnings: Vec::new(),
                next_actions: vec![
                    "Implementar una receta deterministica especifica antes de ejecutar este LOCAL."
                        .to_string(),
                ],
                no_push: true,
                summary: "Solver LOCAL detenido antes de editar.".to_string(),
            });
        }
    }

    commit_local_problem(request).await
}

fn local_problem_specs() -> Vec<LocalProblemSpec> {
    vec![
        spec(
            "LOCAL-001",
            "dieta clinical_intent.py fase 3",
            "Extraer builders o helpers deterministicos restantes sin cambiar API ni prompts.",
            "agent/local-001-dieta-clinical-intent-py-fase-3",
            "LOCAL-001 diet clinical_intent helpers",
            &["clinical_intent"],
            &["clinical_intent"],
            &["check:api", "check:contract"],
            &["prompt", "endpoint nuevo", "openapi"],
        ),
        spec(
            "LOCAL-002",
            "adelgazar clinical_record.py",
            "Mover enums/tipos auxiliares a modulo de dominio pequeno sin cambiar SQLAlchemy ni migraciones.",
            "agent/local-002-adelgazar-clinical-record-py",
            "LOCAL-002 slim clinical_record domain types",
            &["clinical_record"],
            &["clinical_record"],
            &["check:api"],
            &["migration", "alembic"],
        ),
        spec(
            "LOCAL-003",
            "dividir clinical-intent-result-panel.tsx",
            "Extraer subpaneles visuales pequenos: faltantes, decisiones, evidencia. Sin cambiar textos clinicos.",
            "agent/local-003-dividir-clinical-intent-result-panel",
            "LOCAL-003 split clinical intent result panel",
            &["clinical-intent-result-panel.tsx"],
            &["clinical-intent-result"],
            &["check:web"],
            &["texto clinico", "clinical copy", "endpoint"],
        ),
        spec(
            "LOCAL-004",
            "adelgazar patient-ai-chart-pages.tsx",
            "Mantenerlo como orquestador; extraer cabecera/estado operativo si aplica.",
            "agent/local-004-adelgazar-patient-ai-chart-pages",
            "LOCAL-004 slim patient ai chart pages",
            &["patient-ai-chart"],
            &["patient-ai-chart"],
            &["check:web", "check:size"],
            &["endpoint", "dashboard", "rag"],
        ),
        spec(
            "LOCAL-005",
            "dividir assistant-read-sections.tsx",
            "Separar timeline/search/series/correlation en archivos de dominio, sin crear carpeta generica utils.",
            "agent/local-005-dividir-assistant-read-sections",
            "LOCAL-005 split assistant read sections",
            &["assistant-read-sections.tsx"],
            &["assistant-read", "timeline", "correlation", "series"],
            &["check:web", "check:e2e"],
            &["utils", "rag", "endpoint"],
        ),
        spec(
            "LOCAL-006",
            "adelgazar patient-record-workspaces.tsx",
            "Extraer solo un workspace claro, por ejemplo auditoria o sugerencias IA. Refactor puro.",
            "agent/local-006-adelgazar-patient-record-workspaces",
            "LOCAL-006 slim patient record workspaces",
            &["patient-record-workspace"],
            &["patient-record-workspace", "patient-record-audit-workspace"],
            &["check:web"],
            &["endpoint", "permission", "route"],
        ),
        spec(
            "LOCAL-007",
            "dieta ambulatory-appointment-pages.tsx",
            "Separar lista y formulario de cita sin tocar permisos ni endpoints.",
            "agent/local-007-dieta-ambulatory-appointment-pages",
            "LOCAL-007 split ambulatory appointment pages",
            &["ambulatory-appointment"],
            &["ambulatory-appointment"],
            &["check:web"],
            &["endpoint", "permission", "permiso"],
        ),
        spec(
            "LOCAL-008",
            "dieta ambulatory-visit-pages.tsx",
            "Extraer panel de preconsulta o cierre ambulatorio, sin cambiar enfermeria/permisos.",
            "agent/local-008-dieta-ambulatory-visit-pages",
            "LOCAL-008 split ambulatory visit pages",
            &["ambulatory-visit"],
            &["ambulatory-visit"],
            &["check:web", "check:e2e"],
            &["endpoint", "permission", "enfermeria"],
        ),
        spec(
            "LOCAL-009",
            "revisar demo-record.ts",
            "Verificar que no haya contaminacion de nombres/proyectos externos y dividir datos demo si el archivo sigue creciendo.",
            "agent/local-009-revisar-demo-record",
            "LOCAL-009 review demo record",
            &["demo-record"],
            &["demo-record", "demo-hospital-record"],
            &["check:web"],
            &["real patient", "phi", "externo"],
        ),
        spec(
            "LOCAL-010",
            "robustecer smoke de ficha",
            "Mejorar selectores Playwright ambiguos en ficha/papel sin agregar cobertura pesada.",
            "agent/local-010-robustecer-smoke-ficha",
            "LOCAL-010 harden patient chart smoke",
            &["clinical-smoke.spec.ts", "print-smoke.spec.ts"],
            &["clinical-smoke.spec.ts", "print-smoke.spec.ts"],
            &["check:e2e"],
            &["coverage pesada", "new feature", "dashboard"],
        ),
    ]
}

fn solve_local_001(repo: &Path) -> Result<(), String> {
    let source_path = repo.join("apps/api/src/oneepis_api/services/clinical_intent.py");
    let extracted_path =
        repo.join("apps/api/src/oneepis_api/services/clinical_intent_payload_rules.py");
    ensure_file_missing(&extracted_path, "LOCAL-001")?;

    let mut source = read_text(&source_path, "clinical_intent.py LOCAL-001")?;
    let mut blocks = Vec::new();
    for item in [
        "_exam_results",
        "_exam_marker",
        "_exam_value",
        "_exam_delta_finding",
        "_medication_payload_finding",
        "_medication_detail",
    ] {
        blocks.push(extract_python_block(&mut source, &format!("def {item}"))?);
    }
    remove_exact(
        &mut source,
        "from oneepis_api.services.clinical_intent_text import (\n    payload_text as _payload_text,\n)\n",
        "import payload_text LOCAL-001",
    )?;
    insert_after_once(
        &mut source,
        "from oneepis_api.services.clinical_intent_context import (\n    clinical_intent_context_payload as _context_payload,\n)\n",
        "from oneepis_api.services.clinical_intent_payload_rules import (\n    _exam_delta_finding,\n    _exam_marker,\n    _exam_results,\n    _exam_value,\n    _medication_payload_finding,\n)\n",
    )?;
    write_text(
        &source_path,
        normalize_python_blank_lines(&source),
        "clinical_intent.py LOCAL-001",
    )?;

    let mut body = String::from(
        "from __future__ import annotations\n\nfrom oneepis_api.services.clinical_intent_text import (\n    normalize_text as _normalize_text,\n)\nfrom oneepis_api.services.clinical_intent_text import (\n    payload_text as _payload_text,\n)\n\n\n",
    );
    body.push_str(&blocks.join("\n\n\n"));
    body.push('\n');
    write_text(
        &extracted_path,
        body,
        "clinical_intent_payload_rules.py LOCAL-001",
    )?;
    Ok(())
}

fn solve_local_002(repo: &Path) -> Result<(), String> {
    let source_path = repo.join("apps/api/src/oneepis_api/models/clinical_record.py");
    let extracted_path = repo.join("apps/api/src/oneepis_api/models/clinical_record_enums.py");
    ensure_file_missing(&extracted_path, "LOCAL-002")?;

    let mut source = read_text(&source_path, "clinical_record.py LOCAL-002")?;
    let mut blocks = Vec::new();
    for item in [
        "ClinicalEntryKind",
        "ClinicalEntryStatus",
        "ClinicalEventType",
        "ClinicalEventSourceType",
        "AllergySeverity",
        "RecordStatus",
        "EncounterType",
        "EncounterStatus",
        "AppointmentStatus",
    ] {
        blocks.push(extract_python_block(&mut source, &format!("class {item}"))?);
    }
    remove_exact(&mut source, "import enum\n", "import enum LOCAL-002")?;
    insert_after_once(
        &mut source,
        "from oneepis_api.models.base import IdMixin, TimestampMixin\n",
        "from oneepis_api.models.clinical_record_enums import (\n    AllergySeverity,\n    AppointmentStatus,\n    ClinicalEntryKind,\n    ClinicalEntryStatus,\n    ClinicalEventSourceType,\n    ClinicalEventType,\n    EncounterStatus,\n    EncounterType,\n    RecordStatus,\n)\n",
    )?;
    write_text(
        &source_path,
        normalize_python_blank_lines(&source),
        "clinical_record.py LOCAL-002",
    )?;

    let mut body = String::from("from __future__ import annotations\n\nimport enum\n\n\n");
    body.push_str(&blocks.join("\n\n\n"));
    body.push('\n');
    write_text(&extracted_path, body, "clinical_record_enums.py LOCAL-002")?;
    Ok(())
}

fn solve_local_003(repo: &Path) -> Result<(), String> {
    let panel_path =
        repo.join("apps/web/src/components/clinical/ai-chart/clinical-intent-result-panel.tsx");
    let extracted_path = repo.join(
        "apps/web/src/components/clinical/ai-chart/clinical-intent-result-evidence-panels.tsx",
    );
    if !panel_path.is_file() {
        return Err("No se encontro clinical-intent-result-panel.tsx.".to_string());
    }
    if extracted_path.exists() {
        return Err("El archivo extraido LOCAL-003 ya existe; no se sobrescribe.".to_string());
    }

    let mut panel = fs::read_to_string(&panel_path)
        .map_err(|err| format!("No se pudo leer panel LOCAL-003: {err}"))?;
    let mut extracted = Vec::new();
    for function in [
        "ProblemsEvidencePanel",
        "shortSourceId",
        "EvidenceMarksPanel",
        "MissingDataPanel",
    ] {
        let block = extract_function_block(&mut panel, function)?;
        extracted.push(export_function(block));
    }
    let import = "import {\n  EvidenceMarksPanel,\n  MissingDataPanel,\n  ProblemsEvidencePanel,\n} from \"./clinical-intent-result-evidence-panels\";\n";
    if !panel.contains("clinical-intent-result-evidence-panels") {
        let anchor = "import { ReviewItemsPanel } from \"./review-items-panel\";\n";
        panel = panel.replacen(anchor, &format!("{anchor}{import}"), 1);
    }
    fs::write(&panel_path, panel)
        .map_err(|err| format!("No se pudo escribir panel LOCAL-003: {err}"))?;

    let mut body = String::from(
        "\"use client\";\n\nimport type { ClinicalIntentResponse } from \"@/lib/types\";\n\n",
    );
    body.push_str(&extracted.join("\n\n"));
    body.push('\n');
    fs::write(&extracted_path, body)
        .map_err(|err| format!("No se pudo escribir subpaneles LOCAL-003: {err}"))?;
    Ok(())
}

fn solve_local_004(repo: &Path) -> Result<(), String> {
    let source_path = repo.join("apps/web/src/components/clinical/patient-ai-chart-pages.tsx");
    let extracted_path =
        repo.join("apps/web/src/components/clinical/patient-ai-chart-shell-sections.tsx");
    ensure_file_missing(&extracted_path, "LOCAL-004")?;

    let mut source = read_text(&source_path, "patient-ai-chart-pages.tsx LOCAL-004")?;
    remove_exact(
        &mut source,
        "import { BackLink, PageTitle, usePatientId, usePatientRecordQuery } from \"./patient-page-shared\";\n",
        "patient-page-shared import LOCAL-004",
    )?;
    insert_after_once(
        &mut source,
        "import { DraftSoapPaper } from \"@/components/clinical/ai-chart/draft-soap-paper\";\n",
        "import { AiChartPageHeader } from \"@/components/clinical/patient-ai-chart-shell-sections\";\n",
    )?;
    insert_after_once(
        &mut source,
        "import type {\n  ClinicalIntentAction,\n  ClinicalIntentResponse,\n  ClinicalIntentRouteResponse,\n  ClinicalIntentType,\n  ClinicalReviewItem,\n  DraftSoapFromEventsResponse,\n  AIStreamEvent,\n} from \"@/lib/types\";\n",
        "\nimport { usePatientId, usePatientRecordQuery } from \"./patient-page-shared\";\n",
    )?;
    let header_block = "        <BackLink href={`/pacientes/${patientId}/ficha`} label=\"Ficha\" />\n        <PageTitle\n          title=\"AI-Chart Core\"\n          description=\"Eventos clinicos -> contexto -> borrador SOAP editable -> confirmacion humana.\"\n        />\n        {DEMO_MODE ? <ErrorState description=\"El modo demo no permite generar borradores reales.\" /> : null}\n        {!DEMO_MODE && !userLoading && !canUseAi ? (\n          <ErrorState description=\"Tu rol actual no permite usar IA clinica.\" />\n        ) : null}\n";
    replace_exact(
        &mut source,
        header_block,
        "        <AiChartPageHeader patientId={patientId} userLoading={userLoading} canUseAi={canUseAi} />\n",
        "header block LOCAL-004",
    )?;
    write_text(
        &source_path,
        normalize_blank_lines(&source),
        "patient-ai-chart-pages.tsx LOCAL-004",
    )?;

    let body = "\"use client\";\n\nimport { ErrorState } from \"@/components/clinical/states\";\nimport { DEMO_MODE } from \"@/lib/api/client\";\n\nimport { BackLink, PageTitle } from \"./patient-page-shared\";\n\nexport function AiChartPageHeader({\n  patientId,\n  userLoading,\n  canUseAi,\n}: {\n  patientId: string;\n  userLoading: boolean;\n  canUseAi: boolean;\n}) {\n  return (\n    <>\n      <BackLink href={`/pacientes/${patientId}/ficha`} label=\"Ficha\" />\n      <PageTitle\n        title=\"AI-Chart Core\"\n        description=\"Eventos clinicos -> contexto -> borrador SOAP editable -> confirmacion humana.\"\n      />\n      {DEMO_MODE ? <ErrorState description=\"El modo demo no permite generar borradores reales.\" /> : null}\n      {!DEMO_MODE && !userLoading && !canUseAi ? (\n        <ErrorState description=\"Tu rol actual no permite usar IA clinica.\" />\n      ) : null}\n    </>\n  );\n}\n";
    write_text(
        &extracted_path,
        body.to_string(),
        "patient-ai-chart-shell-sections.tsx LOCAL-004",
    )?;
    Ok(())
}

fn solve_local_005(repo: &Path) -> Result<(), String> {
    let source_path =
        repo.join("apps/web/src/components/clinical/ai-chart/assistant-read-sections.tsx");
    let dir = source_path
        .parent()
        .ok_or_else(|| "Ruta Assistant Read invalida.".to_string())?;
    let module_paths = [
        "assistant-read-state.tsx",
        "assistant-read-source-line.tsx",
        "assistant-read-footnotes.tsx",
        "assistant-read-timeline-section.tsx",
        "assistant-read-search-section.tsx",
        "assistant-read-series-section.tsx",
        "assistant-read-correlation-section.tsx",
    ];
    for file in module_paths {
        ensure_file_missing(&dir.join(file), "LOCAL-005")?;
    }

    let mut source = read_text(&source_path, "assistant-read-sections.tsx LOCAL-005")?;
    let panel_state = export_function(extract_function_block(&mut source, "PanelState")?);
    let timeline = export_function(extract_function_block(&mut source, "TimelineList")?);
    let search = export_function(extract_function_block(&mut source, "SearchList")?);
    let series_chart = export_function(extract_function_block(&mut source, "SeriesChart")?);
    let series_list = export_function(extract_function_block(&mut source, "SeriesList")?);
    let lab_panels = export_function(extract_function_block(&mut source, "LabPanelList")?);
    let correlation = export_function(extract_function_block(&mut source, "CorrelationList")?);
    let evidence = export_function(extract_function_block(&mut source, "EvidenceList")?);
    let source_line = export_function(extract_function_block(&mut source, "SourceLine")?);
    let source_text = export_function(extract_function_block(&mut source, "sourceText")?);
    let source_href = export_function(extract_function_block(&mut source, "sourceHref")?);
    let footnotes = export_function(extract_function_block(&mut source, "DataFootnotes")?);

    write_text(
        &dir.join("assistant-read-state.tsx"),
        format!(
            "\"use client\";\n\nimport type {{ ReactNode }} from \"react\";\n\nimport {{ EmptyState, ErrorState }} from \"@/components/clinical/states\";\n\n{panel_state}\n"
        ),
        "assistant-read-state.tsx LOCAL-005",
    )?;
    write_text(
        &dir.join("assistant-read-source-line.tsx"),
        format!(
            "\"use client\";\n\nimport {{ API_BASE_URL }} from \"@/lib/api/client\";\n\n{source_line}\n\n{source_text}\n\n{source_href}\n"
        ),
        "assistant-read-source-line.tsx LOCAL-005",
    )?;
    write_text(
        &dir.join("assistant-read-footnotes.tsx"),
        format!("\"use client\";\n\n{footnotes}\n"),
        "assistant-read-footnotes.tsx LOCAL-005",
    )?;
    write_text(
        &dir.join("assistant-read-timeline-section.tsx"),
        format!(
            "\"use client\";\n\nimport {{ formatDateTime }} from \"@/components/clinical/date-format\";\nimport {{ Badge }} from \"@/components/ui/badge\";\nimport type {{ AssistantTimelineItem }} from \"@/lib/types\";\n\nimport {{ SourceLine }} from \"./assistant-read-source-line\";\n\n{timeline}\n"
        ),
        "assistant-read-timeline-section.tsx LOCAL-005",
    )?;
    write_text(
        &dir.join("assistant-read-search-section.tsx"),
        format!(
            "\"use client\";\n\nimport {{ formatDateTime }} from \"@/components/clinical/date-format\";\nimport {{ Badge }} from \"@/components/ui/badge\";\nimport type {{ AssistantSearchResult }} from \"@/lib/types\";\n\nimport {{ SourceLine }} from \"./assistant-read-source-line\";\n\n{search}\n"
        ),
        "assistant-read-search-section.tsx LOCAL-005",
    )?;
    write_text(
        &dir.join("assistant-read-series-section.tsx"),
        format!(
            "\"use client\";\n\nimport {{ Line, LineChart, Tooltip, XAxis, YAxis }} from \"recharts\";\n\nimport {{ formatDateTime }} from \"@/components/clinical/date-format\";\nimport {{ EmptyState }} from \"@/components/clinical/states\";\nimport {{ Badge }} from \"@/components/ui/badge\";\nimport type {{ AssistantChartSeries, LabPanel }} from \"@/lib/types\";\n\nimport {{ SourceLine }} from \"./assistant-read-source-line\";\n\n{series_chart}\n\n{series_list}\n\n{lab_panels}\n"
        ),
        "assistant-read-series-section.tsx LOCAL-005",
    )?;
    write_text(
        &dir.join("assistant-read-correlation-section.tsx"),
        format!(
            "\"use client\";\n\nimport {{ formatDateTime }} from \"@/components/clinical/date-format\";\nimport {{ Badge }} from \"@/components/ui/badge\";\nimport type {{ AssistantCorrelationEvidence, AssistantCorrelationResult }} from \"@/lib/types\";\n\nimport {{ DataFootnotes }} from \"./assistant-read-footnotes\";\nimport {{ sourceText }} from \"./assistant-read-source-line\";\n\n{correlation}\n\n{evidence}\n"
        ),
        "assistant-read-correlation-section.tsx LOCAL-005",
    )?;
    write_text(
        &source_path,
        "\"use client\";\n\nexport { DataFootnotes } from \"./assistant-read-footnotes\";\nexport { CorrelationList } from \"./assistant-read-correlation-section\";\nexport { SearchList } from \"./assistant-read-search-section\";\nexport { SeriesChart, SeriesList, LabPanelList } from \"./assistant-read-series-section\";\nexport { PanelState } from \"./assistant-read-state\";\nexport { TimelineList } from \"./assistant-read-timeline-section\";\n",
        "assistant-read-sections.tsx LOCAL-005",
    )?;
    Ok(())
}

fn solve_local_006(repo: &Path) -> Result<(), String> {
    let source_path = repo.join("apps/web/src/components/clinical/patient-record-workspaces.tsx");
    let extracted_path =
        repo.join("apps/web/src/components/clinical/patient-record-audit-workspace.tsx");
    ensure_file_missing(&extracted_path, "LOCAL-006")?;

    let mut source = read_text(&source_path, "patient-record-workspaces.tsx LOCAL-006")?;
    let audit_workspace = export_function(extract_function_block(&mut source, "AuditWorkspace")?);
    remove_exact(
        &mut source,
        "import { AuditTimeline } from \"@/components/clinical/audit-widgets\";\n",
        "AuditTimeline import LOCAL-006",
    )?;
    replace_exact(
        &mut source,
        "import { listAuditEvents, listClinicalEncounters, listVitalSigns } from \"@/lib/api/clinical-record\";\n",
        "import { listClinicalEncounters, listVitalSigns } from \"@/lib/api/clinical-record\";\n",
        "clinical-record import LOCAL-006",
    )?;
    insert_after_once(
        &mut source,
        "import { NoPermissionButton } from \"./patient-page-shared\";\n",
        "export { AuditWorkspace } from \"./patient-record-audit-workspace\";\n",
    )?;
    write_text(
        &source_path,
        normalize_blank_lines(&source),
        "patient-record-workspaces.tsx LOCAL-006",
    )?;

    let body = format!(
        "\"use client\";\n\nimport {{ useQuery }} from \"@tanstack/react-query\";\n\nimport {{ AuditTimeline }} from \"@/components/clinical/audit-widgets\";\nimport {{ ClinicalSectionCard }} from \"@/components/clinical/cards\";\nimport {{ EmptyState, ErrorState, LoadingRows }} from \"@/components/clinical/states\";\nimport {{ listAuditEvents }} from \"@/lib/api/clinical-record\";\nimport {{ DEMO_MODE }} from \"@/lib/api/client\";\n\n{audit_workspace}\n"
    );
    write_text(
        &extracted_path,
        body,
        "patient-record-audit-workspace.tsx LOCAL-006",
    )?;
    Ok(())
}

fn solve_local_007(repo: &Path) -> Result<(), String> {
    let source_path =
        repo.join("apps/web/src/components/clinical/ambulatory-appointment-pages.tsx");
    let list_path = repo.join("apps/web/src/components/clinical/ambulatory-appointment-list.tsx");
    let form_path = repo.join("apps/web/src/components/clinical/ambulatory-appointment-form.tsx");
    ensure_file_missing(&list_path, "LOCAL-007")?;
    ensure_file_missing(&form_path, "LOCAL-007")?;

    let mut source = read_text(&source_path, "ambulatory-appointment-pages.tsx LOCAL-007")?;
    let form_type = extract_ts_type_block(&mut source, "AppointmentFormState")?;
    let status_label = extract_ts_const_object(&mut source, "statusLabel")?;
    let appointment_list = export_function(extract_function_block(&mut source, "AppointmentList")?);
    let create_panel = export_function(extract_function_block(
        &mut source,
        "AppointmentCreatePanel",
    )?);
    let appointment_input = extract_function_block(&mut source, "AppointmentInput")?;
    let empty_form = extract_function_block(&mut source, "emptyAppointmentForm")?;
    let patient_name_map = extract_function_block(&mut source, "patientNameMap")?;

    replace_exact(
        &mut source,
        "import Link from \"next/link\";\n",
        "",
        "Link import LOCAL-007",
    )?;
    replace_exact(
        &mut source,
        "import { useMemo, useState } from \"react\";\n",
        "import { useState } from \"react\";\n",
        "react import LOCAL-007",
    )?;
    replace_exact(
        &mut source,
        "import { useMutation, useQuery, useQueryClient } from \"@tanstack/react-query\";\n",
        "import { useQuery } from \"@tanstack/react-query\";\n",
        "query import LOCAL-007",
    )?;
    replace_exact(
        &mut source,
        "import { CalendarPlus, Save } from \"lucide-react\";\n\n",
        "",
        "icons import LOCAL-007",
    )?;
    replace_exact(
        &mut source,
        "import { useCurrentUser } from \"@/components/auth/use-current-user\";\n",
        "",
        "current user import LOCAL-007",
    )?;
    replace_exact(
        &mut source,
        "import { formatDateTime } from \"@/components/clinical/date-format\";\n",
        "",
        "formatDateTime import LOCAL-007",
    )?;
    replace_exact(
        &mut source,
        "import { EmptyState, ErrorState, LoadingRows } from \"@/components/clinical/states\";\n",
        "import { ErrorState, LoadingRows } from \"@/components/clinical/states\";\n",
        "states import LOCAL-007",
    )?;
    replace_exact(
        &mut source,
        "import { Badge } from \"@/components/ui/badge\";\n",
        "",
        "badge import LOCAL-007",
    )?;
    replace_exact(
        &mut source,
        "import { Button } from \"@/components/ui/button\";\n",
        "",
        "button import LOCAL-007",
    )?;
    replace_exact(
        &mut source,
        "import { Textarea } from \"@/components/ui/textarea\";\n",
        "",
        "textarea import LOCAL-007",
    )?;
    replace_exact(
        &mut source,
        "import { listAppointments, createPatientAppointment } from \"@/lib/api/appointments\";\n",
        "import { listAppointments } from \"@/lib/api/appointments\";\n",
        "appointments import LOCAL-007",
    )?;
    replace_exact(
        &mut source,
        "import { canManageEncounters } from \"@/lib/permissions\";\n",
        "",
        "permissions import LOCAL-007",
    )?;
    replace_exact(
        &mut source,
        "import type { ClinicalAppointment, Patient } from \"@/lib/types\";\n",
        "",
        "types import LOCAL-007",
    )?;
    replace_exact(
        &mut source,
        "import { Field, emptyToNull, toDatetimeLocal } from \"./patient-page-shared\";\n",
        "import { AppointmentCreatePanel } from \"./ambulatory-appointment-form\";\nimport { AppointmentList } from \"./ambulatory-appointment-list\";\nimport { Field } from \"./patient-page-shared\";\n",
        "shared import LOCAL-007",
    )?;
    write_text(
        &source_path,
        normalize_blank_lines(&source),
        "ambulatory-appointment-pages.tsx LOCAL-007",
    )?;

    write_text(
        &list_path,
        format!(
            "\"use client\";\n\nimport Link from \"next/link\";\nimport {{ useMemo }} from \"react\";\n\nimport {{ formatDateTime }} from \"@/components/clinical/date-format\";\nimport {{ EmptyState }} from \"@/components/clinical/states\";\nimport {{ Badge }} from \"@/components/ui/badge\";\nimport {{ Button }} from \"@/components/ui/button\";\nimport type {{ ClinicalAppointment, Patient }} from \"@/lib/types\";\n\n{status_label}\n\n{appointment_list}\n\n{patient_name_map}\n"
        ),
        "ambulatory-appointment-list.tsx LOCAL-007",
    )?;
    write_text(
        &form_path,
        format!(
            "\"use client\";\n\nimport {{ useState }} from \"react\";\nimport {{ useMutation, useQueryClient }} from \"@tanstack/react-query\";\nimport {{ CalendarPlus, Save }} from \"lucide-react\";\n\nimport {{ useCurrentUser }} from \"@/components/auth/use-current-user\";\nimport {{ ClinicalSectionCard }} from \"@/components/clinical/cards\";\nimport {{ EmptyState, ErrorState }} from \"@/components/clinical/states\";\nimport {{ Button }} from \"@/components/ui/button\";\nimport {{ Input }} from \"@/components/ui/input\";\nimport {{ Textarea }} from \"@/components/ui/textarea\";\nimport {{ createPatientAppointment }} from \"@/lib/api/appointments\";\nimport {{ DEMO_MODE }} from \"@/lib/api/client\";\nimport {{ canManageEncounters }} from \"@/lib/permissions\";\nimport type {{ Patient }} from \"@/lib/types\";\n\nimport {{ Field, emptyToNull, toDatetimeLocal }} from \"./patient-page-shared\";\n\n{form_type}\n\n{create_panel}\n\n{appointment_input}\n\n{empty_form}\n"
        ),
        "ambulatory-appointment-form.tsx LOCAL-007",
    )?;
    Ok(())
}

fn solve_local_008(repo: &Path) -> Result<(), String> {
    let source_path = repo.join("apps/web/src/components/clinical/ambulatory-visit-pages.tsx");
    let form_path = repo.join("apps/web/src/components/clinical/ambulatory-visit-form.tsx");
    ensure_file_missing(&form_path, "LOCAL-008")?;

    let mut source = read_text(&source_path, "ambulatory-visit-pages.tsx LOCAL-008")?;
    let form_type = extract_ts_type_block(&mut source, "AmbulatoryVisitFormState")?;
    let form = export_function(extract_function_block(&mut source, "AmbulatoryVisitForm")?);
    let soap_field = extract_function_block(&mut source, "SoapField")?;
    replace_exact(
        &mut source,
        "import { Save } from \"lucide-react\";\n\n",
        "",
        "Save import LOCAL-008",
    )?;
    replace_exact(
        &mut source,
        "import { Button } from \"@/components/ui/button\";\n",
        "",
        "Button import LOCAL-008",
    )?;
    replace_exact(
        &mut source,
        "import { Input } from \"@/components/ui/input\";\n",
        "",
        "Input import LOCAL-008",
    )?;
    replace_exact(
        &mut source,
        "import { Textarea } from \"@/components/ui/textarea\";\n",
        "",
        "Textarea import LOCAL-008",
    )?;
    replace_exact(&mut source, "  Field,\n", "", "Field import LOCAL-008")?;
    insert_after_once(
        &mut source,
        "import type { ClinicalEntry, PatientRecordSnapshot } from \"@/lib/types\";\n\n",
        "import { AmbulatoryVisitForm, type AmbulatoryVisitFormState } from \"./ambulatory-visit-form\";\n",
    )?;
    write_text(
        &source_path,
        normalize_blank_lines(&source),
        "ambulatory-visit-pages.tsx LOCAL-008",
    )?;

    write_text(
        &form_path,
        format!(
            "\"use client\";\n\nimport {{ Save }} from \"lucide-react\";\n\nimport {{ Button }} from \"@/components/ui/button\";\nimport {{ Input }} from \"@/components/ui/input\";\nimport {{ Textarea }} from \"@/components/ui/textarea\";\n\nimport {{ Field }} from \"./patient-page-shared\";\n\nexport {form_type}\n\n{form}\n\n{soap_field}\n"
        ),
        "ambulatory-visit-form.tsx LOCAL-008",
    )?;
    Ok(())
}

fn solve_local_009(repo: &Path) -> Result<(), String> {
    let source_path = repo.join("apps/web/src/lib/demo-record.ts");
    let extracted_path = repo.join("apps/web/src/lib/demo-hospital-record.ts");
    ensure_file_missing(&extracted_path, "LOCAL-009")?;

    let mut source = read_text(&source_path, "demo-record.ts LOCAL-009")?;
    let beds = extract_ts_const_array(&mut source, "demoHospitalBeds")?;
    let sheets = extract_ts_const_array(&mut source, "demoHospitalDailySheets")?;
    let indications = extract_ts_const_array(&mut source, "demoHospitalIndications")?;
    replace_exact(
        &mut source,
        "  HospitalBed,\n  HospitalDailySheet,\n  HospitalIndication,\n",
        "",
        "hospital types LOCAL-009",
    )?;
    insert_after_once(
        &mut source,
        "} from \"@/lib/types\";\n",
        "\nexport { demoHospitalBeds, demoHospitalDailySheets, demoHospitalIndications } from \"./demo-hospital-record\";\n",
    )?;
    write_text(
        &source_path,
        normalize_blank_lines(&source),
        "demo-record.ts LOCAL-009",
    )?;

    write_text(
        &extracted_path,
        format!(
            "import type {{ HospitalBed, HospitalDailySheet, HospitalIndication }} from \"@/lib/types\";\n\n{beds}\n\n{sheets}\n\n{indications}\n"
        ),
        "demo-hospital-record.ts LOCAL-009",
    )?;
    Ok(())
}

fn solve_local_010(repo: &Path) -> Result<(), String> {
    let clinical_smoke = repo.join("apps/web/tests/e2e/clinical-smoke.spec.ts");
    let print_smoke = repo.join("apps/web/tests/e2e/print-smoke.spec.ts");
    let mut clinical = read_text(&clinical_smoke, "clinical-smoke.spec.ts LOCAL-010")?;
    for (from, to) in [
        (
            "page.getByText(\"Fuentes usadas\")",
            "page.getByText(\"Fuentes usadas\", { exact: true })",
        ),
        (
            "page.getByText(\"Riesgos clinicos\")",
            "page.getByText(\"Riesgos clinicos\", { exact: true })",
        ),
        (
            "page.getByText(\"Limites visibles y faltantes\")",
            "page.getByText(\"Limites visibles y faltantes\", { exact: true })",
        ),
    ] {
        replace_exact(&mut clinical, from, to, "clinical smoke selector LOCAL-010")?;
    }
    write_text(
        &clinical_smoke,
        clinical,
        "clinical-smoke.spec.ts LOCAL-010",
    )?;

    let mut print = read_text(&print_smoke, "print-smoke.spec.ts LOCAL-010")?;
    for (from, to) in [
        (
            "page.getByText(\"Vista papel\")",
            "page.getByText(\"Vista papel\", { exact: true })",
        ),
        (
            "page.getByText(\"Documento de desarrollo / no uso clinico real.\")",
            "page.getByText(\"Documento de desarrollo / no uso clinico real.\", { exact: true })",
        ),
    ] {
        replace_exact(&mut print, from, to, "print smoke selector LOCAL-010")?;
    }
    write_text(&print_smoke, print, "print-smoke.spec.ts LOCAL-010")?;
    Ok(())
}

fn export_function(block: String) -> String {
    if block.starts_with("function ") {
        block.replacen("function ", "export function ", 1)
    } else {
        block
    }
}

fn extract_function_block(source: &mut String, function_name: &str) -> Result<String, String> {
    let signature = format!("function {function_name}");
    let mut start = source
        .find(&signature)
        .ok_or_else(|| format!("No se encontro funcion {function_name} para extraer."))?;
    if start >= "export ".len() && &source[start - "export ".len()..start] == "export " {
        start -= "export ".len();
    }
    let params_open = source[start..]
        .find('(')
        .map(|offset| start + offset)
        .ok_or_else(|| format!("Funcion {function_name} no tiene parametros."))?;
    let params_close = matching_paren(source, params_open)
        .ok_or_else(|| format!("Funcion {function_name} no cierra parametros."))?;
    let open = source[params_close..]
        .find('{')
        .map(|offset| params_close + offset)
        .ok_or_else(|| format!("Funcion {function_name} no tiene cuerpo."))?;
    let mut depth = 0i32;
    let mut end = None;
    for (index, ch) in source[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(open + index + ch.len_utf8());
                    break;
                }
            }
            _ => {}
        }
    }
    let mut end = end.ok_or_else(|| format!("Funcion {function_name} no cierra."))?;
    while source[end..].starts_with('\n') || source[end..].starts_with('\r') {
        end += source[end..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(0);
    }
    let block = source[start..end].trim_end().to_string();
    source.replace_range(start..end, "");
    Ok(block)
}

fn matching_paren(source: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (index, ch) in source[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + index);
                }
            }
            _ => {}
        }
    }
    None
}

fn matching_delimiter(source: &str, open: usize, start: char, end: char) -> Option<usize> {
    let mut depth = 0i32;
    for (index, ch) in source[open..].char_indices() {
        if ch == start {
            depth += 1;
        } else if ch == end {
            depth -= 1;
            if depth == 0 {
                return Some(open + index);
            }
        }
    }
    None
}

fn extract_python_block(source: &mut String, header: &str) -> Result<String, String> {
    let start = source
        .find(header)
        .ok_or_else(|| format!("No se encontro bloque Python {header}."))?;
    let mut end = start;
    let mut depth = 0i32;
    let mut header_complete = false;
    while end < source.len() {
        let next = source[end..]
            .find('\n')
            .map(|offset| end + offset + 1)
            .unwrap_or(source.len());
        let line = &source[end..next];
        for ch in line.chars() {
            match ch {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            }
        }
        end = next;
        if depth <= 0 && line.trim_end().ends_with(':') {
            header_complete = true;
            break;
        }
    }
    if !header_complete {
        return Err(format!("Bloque Python {header} no cierra encabezado."));
    }
    while end < source.len() {
        let next = source[end..]
            .find('\n')
            .map(|offset| end + offset + 1)
            .unwrap_or(source.len());
        let line = &source[end..next];
        let trimmed = line.trim();
        if !trimmed.is_empty() && !line.starts_with(' ') && !line.starts_with('\t') {
            break;
        }
        end = next;
    }
    let block = source[start..end].trim_end().to_string();
    source.replace_range(start..end, "");
    Ok(block)
}

fn extract_ts_type_block(source: &mut String, type_name: &str) -> Result<String, String> {
    let marker = format!("type {type_name}");
    let start = source
        .find(&marker)
        .ok_or_else(|| format!("No se encontro tipo TS {type_name}."))?;
    let end = source[start..]
        .find("};")
        .map(|offset| start + offset + 2)
        .ok_or_else(|| format!("Tipo TS {type_name} no cierra."))?;
    let block = source[start..end].trim_end().to_string();
    let mut remove_end = end;
    while source[remove_end..].starts_with('\n') || source[remove_end..].starts_with('\r') {
        remove_end += source[remove_end..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(0);
    }
    source.replace_range(start..remove_end, "");
    Ok(block)
}

fn extract_ts_const_object(source: &mut String, const_name: &str) -> Result<String, String> {
    extract_ts_const_delimited(source, const_name, '{', '}')
}

fn extract_ts_const_array(source: &mut String, const_name: &str) -> Result<String, String> {
    extract_ts_const_delimited(source, const_name, '[', ']')
}

fn extract_ts_const_delimited(
    source: &mut String,
    const_name: &str,
    start_char: char,
    end_char: char,
) -> Result<String, String> {
    let marker = format!("export const {const_name}");
    let start = source
        .find(&marker)
        .or_else(|| source.find(&format!("const {const_name}")))
        .ok_or_else(|| format!("No se encontro const TS {const_name}."))?;
    let equal = source[start..]
        .find('=')
        .map(|offset| start + offset)
        .ok_or_else(|| format!("Const TS {const_name} no tiene asignacion."))?;
    let open = source[equal..]
        .find(start_char)
        .map(|offset| equal + offset)
        .ok_or_else(|| format!("Const TS {const_name} no abre bloque."))?;
    let close = matching_delimiter(source, open, start_char, end_char)
        .ok_or_else(|| format!("Const TS {const_name} no cierra bloque."))?;
    let end = if source[close..].starts_with(&format!("{end_char};")) {
        close + end_char.len_utf8() + 1
    } else {
        close + end_char.len_utf8()
    };
    let block = source[start..end].trim_end().to_string();
    let mut remove_end = end;
    while source[remove_end..].starts_with('\n') || source[remove_end..].starts_with('\r') {
        remove_end += source[remove_end..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(0);
    }
    source.replace_range(start..remove_end, "");
    Ok(block)
}

fn ensure_file_missing(path: &Path, label: &str) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "El archivo {} ya existe; {label} no sobrescribe.",
            path.display()
        ));
    }
    Ok(())
}

fn read_text(path: &Path, label: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|err| format!("No se pudo leer {label}: {err}"))
}

fn write_text(path: &Path, content: impl AsRef<str>, label: &str) -> Result<(), String> {
    fs::write(path, content.as_ref()).map_err(|err| format!("No se pudo escribir {label}: {err}"))
}

fn remove_exact(source: &mut String, needle: &str, label: &str) -> Result<(), String> {
    replace_exact(source, needle, "", label)
}

fn replace_exact(
    source: &mut String,
    needle: &str,
    replacement: &str,
    label: &str,
) -> Result<(), String> {
    if !source.contains(needle) {
        return Err(format!("No se encontro bloque esperado para {label}."));
    }
    *source = source.replacen(needle, replacement, 1);
    Ok(())
}

fn insert_after_once(source: &mut String, anchor: &str, insertion: &str) -> Result<(), String> {
    if source.contains(insertion) {
        return Ok(());
    }
    let replacement = format!("{anchor}{insertion}");
    replace_exact(source, anchor, &replacement, "insert_after_once")
}

fn normalize_blank_lines(source: &str) -> String {
    let mut out = source.replace("\r\n", "\n").replace('\r', "\n");
    out = out.replace("\n\n\n", "\n\n");
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out
}

fn normalize_python_blank_lines(source: &str) -> String {
    let mut out = source.replace("\r\n", "\n").replace('\r', "\n");
    while out.contains("\n\n\n\n") {
        out = out.replace("\n\n\n\n", "\n\n\n");
    }
    out
}

fn spec(
    id: &str,
    title: &str,
    objective: &str,
    branch: &str,
    commit_message: &str,
    primary_files: &[&str],
    allowed_path_markers: &[&str],
    gates: &[&str],
    forbidden_signals: &[&str],
) -> LocalProblemSpec {
    LocalProblemSpec {
        id: id.to_string(),
        title: title.to_string(),
        objective: objective.to_string(),
        branch: branch.to_string(),
        commit_message: commit_message.to_string(),
        primary_files: primary_files.iter().map(|item| item.to_string()).collect(),
        allowed_path_markers: allowed_path_markers
            .iter()
            .map(|item| item.to_string())
            .collect(),
        gates: gates.iter().map(|item| item.to_string()).collect(),
        forbidden_signals: GLOBAL_FORBIDDEN_SIGNALS
            .iter()
            .chain(forbidden_signals.iter())
            .map(|item| item.to_string())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        instructions: vec![
            "Prioridad: dieta y claridad antes de clinica nueva.".to_string(),
            "No crear endpoint, tabla, ruta, permisos, IA nueva, RAG, receta, firma ni dashboard."
                .to_string(),
            "Cada problema se resuelve en una rama agent/local-* y un commit local.".to_string(),
            "No hacer push automatico.".to_string(),
        ],
    }
}

fn local_problem_spec(problem_id: &str) -> Result<LocalProblemSpec, String> {
    local_problem_specs()
        .into_iter()
        .find(|problem| problem.id.eq_ignore_ascii_case(problem_id))
        .ok_or_else(|| format!("Problema LOCAL no registrado: {problem_id}."))
}

fn common_blocks(
    inspection: &crate::agent::types::RepoInspection,
    problem: &LocalProblemSpec,
) -> Vec<String> {
    let mut blockers = Vec::new();
    if !inspection.is_git_repo {
        blockers.push("El proyecto objetivo debe ser Git.".to_string());
    }
    if !inspection.is_one_epis {
        blockers.push("LOCAL-* solo se ejecuta con adaptador OneEpis activo.".to_string());
    }
    for gate in &problem.gates {
        if !inspection.declared_gates.contains(gate) {
            blockers.push(format!("Gate requerido no declarado: {gate}."));
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
    problem: &LocalProblemSpec,
    blockers: Vec<String>,
) -> LocalProblemRun {
    LocalProblemRun {
        id: run_id(repo_path, &problem.id),
        problem_id: problem.id.clone(),
        status: "blocked".to_string(),
        repo_path: repo_path.to_string(),
        branch: problem.branch.clone(),
        commit_sha: None,
        changed_files: Vec::new(),
        gate_results: Vec::new(),
        blockers,
        warnings: Vec::new(),
        next_actions: vec![
            "Resolver bloqueos antes de preparar la rama local.".to_string(),
            "No aplicar ni commitear cambios fuera de LOCAL-*.".to_string(),
        ],
        no_push: true,
        summary: format!("{} bloqueado.", problem.id),
    }
}

fn ensure_problem_branch(
    repo: &Path,
    current_branch: &str,
    target_branch: &str,
) -> Result<String, String> {
    if current_branch == target_branch {
        return Ok(target_branch.to_string());
    }
    if git(repo, &["rev-parse", "--verify", target_branch]).is_ok() {
        git(repo, &["switch", target_branch])?;
    } else {
        let base_branch = local_problem_base_branch(repo, current_branch);
        if current_branch != base_branch {
            git(repo, &["switch", &base_branch])?;
        }
        git(repo, &["switch", "-c", target_branch])?;
    }
    Ok(target_branch.to_string())
}

fn local_problem_base_branch(repo: &Path, current_branch: &str) -> String {
    for candidate in ["main", "master"] {
        if git(repo, &["rev-parse", "--verify", candidate]).is_ok() {
            return candidate.to_string();
        }
    }
    current_branch.to_string()
}

fn changed_files(repo: &Path) -> Result<Vec<String>, String> {
    let status = git(repo, &["status", "--porcelain", "--untracked-files=all"])?;
    let mut files = Vec::new();
    for line in status.lines() {
        if line.len() < 4 {
            continue;
        }
        let raw = line[3..].trim();
        let path = raw
            .split(" -> ")
            .last()
            .unwrap_or(raw)
            .trim_matches('"')
            .replace('\\', "/");
        if !path.is_empty() {
            files.push(path);
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn validate_changed_files(problem: &LocalProblemSpec, files: &[String]) -> Vec<String> {
    let mut blockers = Vec::new();
    for file in files {
        let lower = file.to_ascii_lowercase();
        if problem
            .forbidden_signals
            .iter()
            .any(|signal| lower.contains(signal))
        {
            blockers.push(format!("Archivo fuera de gobernanza LOCAL: {file}."));
            continue;
        }
        if !problem
            .allowed_path_markers
            .iter()
            .any(|marker| lower.contains(&marker.to_ascii_lowercase()))
        {
            blockers.push(format!("Archivo no permitido para {}: {file}.", problem.id));
        }
    }
    blockers
}

fn validate_diff_content(
    repo: &Path,
    problem: &LocalProblemSpec,
    _files: &[String],
) -> Result<Vec<String>, String> {
    let mut content = diff_signal_lines(&git(repo, &["diff", "--unified=0", "--"])?);
    content.push_str(&diff_signal_lines(&git(
        repo,
        &["diff", "--cached", "--unified=0", "--"],
    )?));
    let lower = content.to_ascii_lowercase();
    Ok(problem
        .forbidden_signals
        .iter()
        .filter(|signal| lower.contains(&signal.to_ascii_lowercase()))
        .map(|signal| format!("Senal prohibida para {}: {signal}.", problem.id))
        .collect())
}

fn diff_signal_lines(diff: &str) -> String {
    diff.lines()
        .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
        .map(|line| line.trim_start_matches('+'))
        .collect::<Vec<_>>()
        .join("\n")
}

fn git_add_files(repo: &Path, files: &[String]) -> Result<(), String> {
    let mut args = vec!["add", "--"];
    args.extend(files.iter().map(String::as_str));
    git(repo, &args)?;
    Ok(())
}

fn restore_gate_artifacts(repo: &Path, allowed_files: &[String]) -> Result<(), String> {
    let allowed = allowed_files
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let generated = changed_files(repo)?
        .into_iter()
        .filter(|file| !allowed.contains(file.as_str()))
        .collect::<Vec<_>>();
    if generated.is_empty() {
        return Ok(());
    }
    let mut args = vec!["restore", "--"];
    args.extend(generated.iter().map(String::as_str));
    git(repo, &args)?;
    Ok(())
}

fn git_commit(repo: &Path, message: &str) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("-c")
        .arg("user.name=OneEpis Local Agent")
        .arg("-c")
        .arg("user.email=oneepis-local-agent@example.invalid")
        .arg("commit")
        .arg("-m")
        .arg(message)
        .output()
        .map_err(|err| format!("No se pudo ejecutar git commit: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = sanitize_log(&String::from_utf8_lossy(&output.stderr));
    Err(format!("git commit fallo: {stderr}"))
}

fn run_id(repo_path: &str, problem_id: &str) -> String {
    let digest = sha256_hex(format!("{repo_path}:{problem_id}:{}", Utc::now()).as_bytes());
    format!("local-{}", &digest[..16])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_all_requested_local_problems() {
        let specs = list_local_problems();
        let ids = specs
            .iter()
            .map(|spec| spec.id.as_str())
            .collect::<BTreeSet<_>>();

        for index in 1..=10 {
            assert!(ids.contains(format!("LOCAL-{index:03}").as_str()));
        }
        assert!(specs
            .iter()
            .all(|spec| spec.branch.starts_with("agent/local-")));
        assert!(specs.iter().all(|spec| !spec.gates.is_empty()));
        assert!(specs.iter().all(|spec| spec
            .instructions
            .iter()
            .any(|item| item.contains("No hacer push"))));
    }

    #[tokio::test]
    async fn prepare_creates_safe_branch_without_commit() {
        let repo = temp_oneepis_repo("prepare");
        let before = git(&repo, &["rev-parse", "HEAD"]).expect("head");
        let result = prepare_local_problem(LocalProblemRequest {
            repo_path: repo.display().to_string(),
            problem_id: "LOCAL-003".to_string(),
        })
        .await
        .expect("prepare");

        assert_eq!(result.status, "ready_for_changes");
        assert_eq!(
            result.branch,
            "agent/local-003-dividir-clinical-intent-result-panel"
        );
        assert!(result.no_push);
        assert_eq!(git(&repo, &["rev-parse", "HEAD"]).expect("head"), before);
        let _ = fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn commit_blocks_changes_outside_problem_surface() {
        let repo = temp_oneepis_repo("forbidden");
        prepare_local_problem(LocalProblemRequest {
            repo_path: repo.display().to_string(),
            problem_id: "LOCAL-003".to_string(),
        })
        .await
        .expect("prepare");
        fs::create_dir_all(repo.join("migrations")).expect("migrations");
        fs::write(repo.join("migrations").join("001.sql"), "select 1;").expect("migration");

        let result = commit_local_problem(LocalProblemRequest {
            repo_path: repo.display().to_string(),
            problem_id: "LOCAL-003".to_string(),
        })
        .await
        .expect("commit");

        assert_eq!(result.status, "blocked");
        assert!(result.commit_sha.is_none());
        assert!(result
            .blockers
            .iter()
            .any(|block| block.contains("Archivo fuera de gobernanza LOCAL")));
        let _ = fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn commit_runs_gates_and_creates_local_commit_without_push() {
        let repo = temp_oneepis_repo("commit");
        prepare_local_problem(LocalProblemRequest {
            repo_path: repo.display().to_string(),
            problem_id: "LOCAL-003".to_string(),
        })
        .await
        .expect("prepare");
        fs::create_dir_all(repo.join("web")).expect("web");
        fs::write(
            repo.join("web")
                .join("clinical-intent-result-panel-parts.tsx"),
            "export const MissingEvidencePanel = () => null;\n",
        )
        .expect("component");

        let result = commit_local_problem(LocalProblemRequest {
            repo_path: repo.display().to_string(),
            problem_id: "LOCAL-003".to_string(),
        })
        .await
        .expect("commit");

        assert_eq!(result.status, "committed");
        assert!(result.no_push);
        assert!(result.commit_sha.is_some());
        assert!(result
            .gate_results
            .iter()
            .any(|gate| gate.gate == "check:web" && gate.status == "passed"));
        assert!(git(&repo, &["status", "--short"])
            .expect("status")
            .is_empty());
        let _ = fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn solve_local_003_extracts_subpanels_and_commits() {
        let repo = temp_oneepis_repo("solve-local-003");
        write_local_003_fixture(&repo);
        commit_all(&repo);

        let result = solve_local_problem(LocalProblemRequest {
            repo_path: repo.display().to_string(),
            problem_id: "LOCAL-003".to_string(),
        })
        .await
        .expect("solve");

        assert_eq!(result.status, "committed");
        assert_eq!(
            result.branch,
            "agent/local-003-dividir-clinical-intent-result-panel"
        );
        assert!(result.commit_sha.is_some());
        assert!(result
            .changed_files
            .iter()
            .any(|file| file.ends_with("clinical-intent-result-evidence-panels.tsx")));
        let panel = fs::read_to_string(
            repo.join("apps/web/src/components/clinical/ai-chart/clinical-intent-result-panel.tsx"),
        )
        .expect("panel");
        let extracted = fs::read_to_string(repo.join(
            "apps/web/src/components/clinical/ai-chart/clinical-intent-result-evidence-panels.tsx",
        ))
        .expect("extracted");
        assert!(panel.contains("clinical-intent-result-evidence-panels"));
        assert!(!panel.contains("function MissingDataPanel"));
        assert!(extracted.contains("export function ProblemsEvidencePanel"));
        assert!(extracted.contains("export function EvidenceMarksPanel"));
        assert!(extracted.contains("export function MissingDataPanel"));
        assert!(git(&repo, &["status", "--short"])
            .expect("status")
            .is_empty());
        let _ = fs::remove_dir_all(repo);
    }

    fn temp_oneepis_repo(label: &str) -> std::path::PathBuf {
        let repo = std::env::temp_dir().join(format!(
            "oneepis-agent-local-problem-{label}-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(repo.join("docs")).expect("docs");
        Command::new("git")
            .arg("init")
            .current_dir(&repo)
            .output()
            .expect("git init");
        fs::write(repo.join("AGENTS.md"), "# Agents\n").expect("agents");
        fs::write(repo.join("docs").join("GOVERNANCE.md"), "# Governance\n").expect("gov");
        fs::write(
            repo.join("package.json"),
            r#"{"scripts":{"check:api":"echo api","check:contract":"echo contract","check:web":"echo web","check:e2e":"echo e2e","check:size":"echo size"}}"#,
        )
        .expect("package");
        commit_all(&repo);
        repo
    }

    fn write_local_003_fixture(repo: &Path) {
        let dir = repo.join("apps/web/src/components/clinical/ai-chart");
        fs::create_dir_all(&dir).expect("component dir");
        fs::write(
            dir.join("review-items-panel.tsx"),
            "export function ReviewItemsPanel() { return null; }\n",
        )
        .expect("review panel");
        fs::write(
            dir.join("clinical-intent-result-panel.tsx"),
            r#""use client";

import type { ClinicalIntentResponse } from "@/lib/types";

import { ReviewItemsPanel } from "./review-items-panel";

export function ClinicalIntentResultPanel({ intent }: { intent: ClinicalIntentResponse }) {
  return (
    <div>
      <ProblemsEvidencePanel intent={intent} />
      <ReviewItemsPanel />
      <EvidenceMarksPanel intent={intent} />
      <MissingDataPanel intent={intent} />
    </div>
  );
}

function ProblemsEvidencePanel({ intent }: { intent: ClinicalIntentResponse }) {
  return (
    <div>
      <p>Problemas y evidencia</p>
      {intent.problem_contexts.map((context) => (
        <p key={context.title}>Fuente: registro {shortSourceId(context.title)}</p>
      ))}
    </div>
  );
}

function shortSourceId(sourceId: string) {
  return sourceId.slice(0, 8);
}

function EvidenceMarksPanel({ intent }: { intent: ClinicalIntentResponse }) {
  return (
    <ul>
      {intent.evidence_marks.map((mark) => (
        <li key={mark.label}>{mark.label}</li>
      ))}
    </ul>
  );
}

function MissingDataPanel({ intent }: { intent: ClinicalIntentResponse }) {
  return (
    <ul>
      {intent.missing_data.map((item) => <li key={item}>{item}</li>)}
    </ul>
  );
}
"#,
        )
        .expect("clinical intent panel");
    }

    fn commit_all(repo: &Path) {
        git(repo, &["add", "."]).expect("git add");
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .arg("-c")
            .arg("user.name=OneEpis Agent Test")
            .arg("-c")
            .arg("user.email=oneepis-agent-test@example.invalid")
            .arg("commit")
            .arg("-m")
            .arg("fixture")
            .output()
            .expect("git commit");
        assert!(output.status.success(), "git commit failed");
    }
}
