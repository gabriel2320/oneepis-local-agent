import { invoke } from "@tauri-apps/api/core";
import type {
  AgentRun,
  AgentRunReport,
  AgentRunSummary,
  ApplyPatchRequest,
  ApplyPatchResult,
  ApplyReadiness,
  DevelopmentBrief,
  DevelopmentContextPack,
  DevelopmentReadiness,
  DevelopmentWorkPackage,
  EvolutionPlan,
  GateResult,
  ImplementationDecision,
  LocalProblemPlan,
  LocalProblemRun,
  LocalProblemSpec,
  MicroPlan,
  OllamaStatus,
  PatchDraft,
  PatchReview,
  RepoInspection,
} from "./types";

type CommandArgs = Record<string, unknown>;

const PREVIEW_NOTICE = "Vista previa web: abre la app de escritorio para inspeccionar OneEpis real.";

export function isBrowserPreview() {
  return !hasTauriBridge();
}

export function previewNotice() {
  return PREVIEW_NOTICE;
}

export function inspectRepository(repoPath: string) {
  return safeInvoke<RepoInspection>("inspect_repository", { repoPath });
}

export function getOllamaStatus(baseUrl?: string) {
  return safeInvoke<OllamaStatus>("get_ollama_status", { baseUrl });
}

export function getDevelopmentReadiness(repoPath: string, baseUrl?: string) {
  return safeInvoke<DevelopmentReadiness>("development_readiness", { repoPath, baseUrl });
}

export function getDevelopmentWorkPackage(repoPath: string, objective: string, baseUrl?: string) {
  return safeInvoke<DevelopmentWorkPackage>("development_work_package", { repoPath, objective, baseUrl });
}

export function getDevelopmentContextPack(repoPath: string, objective: string, baseUrl?: string) {
  return safeInvoke<DevelopmentContextPack>("development_context_pack", { repoPath, objective, baseUrl });
}

export function getDevelopmentBrief(repoPath: string, objective: string, askModel = false, baseUrl?: string) {
  return safeInvoke<DevelopmentBrief>("development_brief", { repoPath, objective, askModel, baseUrl });
}

export function getImplementationDecision(repoPath: string, objective: string, askModel = true, baseUrl?: string) {
  return safeInvoke<ImplementationDecision>("implementation_decision", { repoPath, objective, askModel, baseUrl });
}

export function getEvolutionPlan(repoPath: string, objective: string, baseUrl?: string) {
  return safeInvoke<EvolutionPlan>("evolution_plan", { repoPath, objective, baseUrl });
}

export function planMicrocycle(repoPath: string, objective: string, baseUrl?: string) {
  return safeInvoke<MicroPlan>("plan_microcycle", { repoPath, objective, baseUrl });
}

export function runMicrocycle(repoPath: string, objective: string, maxCycles: number, askModel = false) {
  return safeInvoke<AgentRun>("run_microcycle", {
    request: {
      repoPath,
      objective,
      maxCycles,
      mode: "dry_run",
      databaseUrl: null,
      askModel,
      allowApply: false,
      confirmToken: null,
      branchStrategy: "reuse",
    },
  });
}

export function runMicrocycleReport(repoPath: string, objective: string, askModel = true) {
  return safeInvoke<AgentRunReport>("run_microcycle_report", {
    request: {
      repoPath,
      objective,
      maxCycles: 1,
      mode: "dry_run",
      databaseUrl: null,
      askModel,
      allowApply: false,
      confirmToken: null,
      branchStrategy: "reuse",
    },
  });
}

export function draftPatch(repoPath: string, objective: string, baseUrl?: string) {
  return safeInvoke<PatchDraft>("draft_patch", { repoPath, objective, baseUrl, databaseUrl: null });
}

export function reviewPatch(draft: PatchDraft) {
  return safeInvoke<PatchReview>("review_patch", { draft });
}

export function applyApprovedPatch(request: ApplyPatchRequest) {
  return safeInvoke<ApplyPatchResult>("apply_approved_patch", { request });
}

export function prepareApplyReadiness(draft: PatchDraft, confirmToken?: string | null) {
  return safeInvoke<ApplyReadiness>("prepare_apply_readiness", {
    request: {
      draft,
      allowApply: true,
      confirmToken: confirmToken ?? null,
      branchStrategy: "create_safe_branch",
      databaseUrl: null,
    },
  });
}

export function runGate(repoPath: string, gate: string) {
  return safeInvoke<GateResult>("run_gate", { repoPath, gate, databaseUrl: null, runId: null });
}

export function listRuns(limit = 20) {
  return safeInvoke<AgentRunSummary[]>("list_runs", { databaseUrl: null, limit });
}

export function listLocalProblems() {
  return safeInvoke<LocalProblemSpec[]>("list_local_problems", {});
}

export function localProblemPlan(repoPath: string, problemId: string) {
  return safeInvoke<LocalProblemPlan>("local_problem_plan", { request: { repoPath, problemId } });
}

export function prepareLocalProblem(repoPath: string, problemId: string) {
  return safeInvoke<LocalProblemRun>("prepare_local_problem", { request: { repoPath, problemId } });
}

export function commitLocalProblem(repoPath: string, problemId: string) {
  return safeInvoke<LocalProblemRun>("commit_local_problem", { request: { repoPath, problemId } });
}

async function safeInvoke<T>(command: string, args: CommandArgs): Promise<T> {
  if (hasTauriBridge()) {
    try {
      return await invoke<T>(command, args);
    } catch (error) {
      if (!isMissingInvokeError(error)) {
        throw error;
      }
    }
  }
  return previewResponse(command, args) as T;
}

function hasTauriBridge() {
  if (typeof window === "undefined") return false;
  const tauriWindow = window as Window & {
    __TAURI__?: unknown;
    __TAURI_INTERNALS__?: unknown;
  };
  return Boolean(tauriWindow.__TAURI__ || tauriWindow.__TAURI_INTERNALS__);
}

function isMissingInvokeError(error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  return message.includes("invoke") || message.includes("__TAURI");
}

function previewResponse(command: string, args: CommandArgs) {
  const repoPath = String(args.repoPath ?? requestField(args, "repoPath") ?? "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis");
  const objective = String(args.objective ?? requestField(args, "objective") ?? "Elegir el siguiente paso pequeño y seguro.");
  switch (command) {
    case "inspect_repository":
      return previewInspection(repoPath);
    case "get_ollama_status":
      return previewOllama();
    case "development_readiness":
      return previewReadiness(repoPath);
    case "development_work_package":
      return previewWorkPackage(repoPath, objective);
    case "development_context_pack":
      return previewContextPack(repoPath, objective);
    case "development_brief":
      return previewBrief(repoPath, objective);
    case "implementation_decision":
      return previewDecision(repoPath, objective);
    case "evolution_plan":
      return previewEvolutionPlan(repoPath, objective);
    case "plan_microcycle":
      return previewMicroPlan(objective);
    case "draft_patch":
      return previewDraft(repoPath, objective);
    case "review_patch":
      return previewReview(args.draft as PatchDraft | undefined);
    case "prepare_apply_readiness":
      return previewApplyReadiness((args.request as ApplyPatchRequest | undefined)?.draft);
    case "apply_approved_patch":
      return previewApplyResult((args.request as ApplyPatchRequest | undefined)?.draft);
    case "run_gate":
      return previewGate(String(args.gate ?? "check:size"));
    case "run_microcycle":
      return previewRun(repoPath, objective);
    case "run_microcycle_report":
      return previewReport(repoPath, objective);
    case "list_runs":
      return [];
    case "list_local_problems":
      return previewLocalProblems();
    case "local_problem_plan":
      return previewLocalProblemPlan(repoPath, String(requestField(args, "problemId") ?? "LOCAL-001"));
    case "prepare_local_problem":
      return previewLocalProblemRun(repoPath, String(requestField(args, "problemId") ?? "LOCAL-001"), "ready_for_changes");
    case "commit_local_problem":
      return previewLocalProblemRun(repoPath, String(requestField(args, "problemId") ?? "LOCAL-001"), "blocked");
    default:
      throw new Error(PREVIEW_NOTICE);
  }
}

function requestField(args: CommandArgs, field: string) {
  const request = args.request as Record<string, unknown> | undefined;
  return request?.[field];
}

function previewInspection(repoPath: string): RepoInspection {
  return {
    repoPath,
    projectName: "OneEpis",
    isGitRepo: true,
    isOneEpis: true,
    currentBranch: "vista-previa",
    dirty: false,
    statusText: PREVIEW_NOTICE,
    governanceDocuments: [
      {
        path: "docs/GOVERNANCE.md",
        title: "Reglas de OneEpis",
        sha256: "preview",
        bytes: 0,
        present: true,
      },
    ],
    declaredGates: ["check:size", "check:api", "check:web"],
    detectedRules: [
      "Solo modelos locales.",
      "No usar datos reales de pacientes.",
      "Revisar antes de aplicar cambios.",
    ],
    blocks: [],
  };
}

function previewOllama(): OllamaStatus {
  return {
    baseUrl: "vista-previa",
    available: false,
    message: PREVIEW_NOTICE,
    models: [],
    policy: {
      primaryCode: "modelo local",
      fastCode: "modelo local rapido",
      governance: "modelo local para reglas",
      fallback: "reglas internas sin IA",
      embeddings: "busqueda local",
    },
    missingPolicyModels: [],
  };
}

function previewReadiness(repoPath: string): DevelopmentReadiness {
  return {
    repoPath,
    profile: "oneepis",
    status: "attention",
    summary: `${PREVIEW_NOTICE} La pantalla muestra un ejemplo seguro de como se vera la revision.`,
    checks: [
      {
        name: "Proyecto",
        status: "ready",
        detail: "La carpeta de ejemplo parece un proyecto OneEpis.",
        action: "Abre la app de escritorio para revisar el proyecto real.",
      },
      {
        name: "Modelo local",
        status: "blocked",
        detail: "El navegador web no puede hablar con Ollama ni con Rust.",
        action: "Usa la app de escritorio para conectar con el modelo local.",
      },
    ],
    blockers: [],
    warnings: [PREVIEW_NOTICE],
    nextActions: [
      "Abre OneEpis Local Agent como app de escritorio.",
      "Presiona Revisar proyecto para leer el estado real.",
    ],
    suggestedMicrocycles: [
      {
        title: "Dieta de archivo grande",
        objective: "Reducir un archivo grande sin cambiar comportamiento.",
        riskLevel: "green",
        gates: ["check:size"],
        reason: "Es un paso pequeño y fácil de comprobar.",
      },
    ],
    requiredGates: ["check:size"],
    localModelSummary: "Vista previa web: el modelo local se revisa dentro de la app de escritorio.",
  };
}

function previewWorkPackage(repoPath: string, objective: string): DevelopmentWorkPackage {
  return {
    repoPath,
    title: "Trabajo sugerido",
    objective,
    status: "ready_to_draft",
    summary: "Ejemplo de un trabajo pequeño, revisable y sin cambios automáticos.",
    branchStrategy: "crear rama segura",
    filesToInspect: ["AGENTS.md", "docs/GOVERNANCE.md"],
    implementationSteps: [
      "Leer las reglas de OneEpis.",
      "Elegir un solo cambio pequeño.",
      "Preparar un borrador antes de tocar archivos reales.",
    ],
    testPlan: [
      {
        gate: "check:size",
        command: "npm run check:size",
        purpose: "Comprobar que el cambio no agranda demasiado los archivos.",
        required: true,
      },
    ],
    acceptanceCriteria: ["El cambio queda como borrador revisable.", "No se usan datos reales."],
    stopConditions: ["Hay cambios pendientes.", "Falta una prueba declarada."],
    gates: ["check:size"],
    warnings: [PREVIEW_NOTICE],
    canDraft: true,
    canApply: false,
  };
}

function previewContextPack(repoPath: string, objective: string): DevelopmentContextPack {
  return {
    repoPath,
    objective,
    status: "partial",
    summary: "Ejemplo de contexto reducido para explicar la pantalla.",
    files: [
      {
        path: "AGENTS.md",
        kind: "file",
        bytes: 0,
        lines: 0,
        sha256: "preview",
        summary: "Reglas principales del proyecto.",
        excerpt: PREVIEW_NOTICE,
      },
    ],
    warnings: [PREVIEW_NOTICE],
    promptNotes: ["No enviar datos reales.", "Pedir cambios pequeños y comprobables."],
    gates: ["check:size"],
    totalBytes: 0,
    maxBytes: 24576,
  };
}

function previewBrief(repoPath: string, objective: string): DevelopmentBrief {
  return {
    repoPath,
    objective,
    status: "partial",
    summary: "Ejemplo de instrucciones para el modelo local.",
    modelUsed: "vista-previa",
    workOrder: "Preparar una sugerencia pequeña y revisable.",
    systemPrompt: "Vista previa web. Sin conexión al backend real.",
    userPrompt: objective,
    responseContract: ["Resumen claro.", "Archivos sugeridos.", "Prueba recomendada."],
    contextFiles: ["AGENTS.md (file)"],
    gates: ["check:size"],
    warnings: [PREVIEW_NOTICE],
    stopConditions: ["No aplicar cambios desde la vista previa web."],
    nextActions: ["Abrir la app de escritorio para pedir una propuesta real."],
    proposal: null,
  };
}

function previewDecision(repoPath: string, objective: string): ImplementationDecision {
  return {
    repoPath,
    objective,
    status: "needs_model_proposal",
    summary: "La vista previa no decide cambios reales.",
    modelUsed: "vista-previa",
    sourceProposalStatus: "missing",
    selectedFiles: [],
    implementationSteps: [],
    requiredGates: ["check:size"],
    acceptanceCriteria: ["Abrir la app de escritorio antes de aplicar."],
    blockers: [PREVIEW_NOTICE],
    warnings: [],
    patchIntent: "Sin cambios reales en navegador web.",
    nextActions: ["Abrir la app de escritorio."],
  };
}

function previewEvolutionPlan(repoPath: string, objective: string): EvolutionPlan {
  const candidate = {
    id: "preview-next-step",
    title: "Reducir un archivo grande",
    objective: objective || "Reducir un archivo grande sin cambiar comportamiento.",
    dimension: "anti_bloat",
    riskLevel: "green",
    filesToInspect: ["AGENTS.md", "docs/GOVERNANCE.md"],
    gates: ["check:size"],
    expectedImprovement: "Un cambio pequeño, fácil de revisar y comprobar.",
    forbiddenFlags: [],
    requiresHumanReview: false,
    source: "vista-previa",
  };
  return {
    repoPath,
    status: "ready",
    summary: "Ejemplo: el próximo paso recomendado sería pequeño y comprobable.",
    selectedCandidate: candidate,
    rankedCandidates: [
      {
        candidate,
        score: {
          candidateId: candidate.id,
          dimensionScores: [
            {
              dimension: "objective_alignment",
              score: 4,
              reason: "Responde al objetivo escrito por el usuario.",
            },
            {
              dimension: "anti_bloat",
              score: 3,
              reason: "Evita agrandar el proyecto.",
            },
          ],
          riskPenalty: 0,
          bloatPenalty: 0,
          netScore: 7,
          verdict: "executable",
          reasons: ["Es pequeño.", "Tiene una prueba clara.", PREVIEW_NOTICE],
        },
      },
    ],
    blockers: [],
    warnings: [PREVIEW_NOTICE],
    nextActions: ["Abrir la app de escritorio.", "Revisar el proyecto real.", "Crear un borrador real."],
    localOnlyBoundary: "Solo modelos locales, sin datos reales y sin aplicar cambios automáticos.",
  };
}

function previewMicroPlan(objective: string): MicroPlan {
  return {
    objective,
    recommendedGate: "check:size",
    riskLevel: "green",
    touchedSurfaces: ["proyecto", "reglas"],
    requiredGates: ["check:size"],
    steps: ["Revisar reglas.", "Elegir un cambio pequeño.", "Crear borrador.", "Ejecutar prueba."],
    warnings: [PREVIEW_NOTICE],
    blocked: false,
    modelUsed: "vista-previa",
  };
}

function previewDraft(repoPath: string, objective: string): PatchDraft {
  return {
    id: "preview-draft",
    repoPath,
    objective,
    summary: "Borrador de ejemplo. No cambia archivos reales.",
    rationale: PREVIEW_NOTICE,
    files: ["AGENTS.md"],
    unifiedDiff: "Vista previa web: la lista de cambios real aparece en la app de escritorio.",
    risks: [PREVIEW_NOTICE],
    gates: ["check:size"],
    blocked: true,
    modelUsed: "vista-previa",
    createdAt: new Date().toISOString(),
    plan: previewMicroPlan(objective),
  };
}

function previewReview(draft?: PatchDraft): PatchReview {
  return {
    draftId: draft?.id ?? "preview-draft",
    approved: false,
    confirmToken: "PREVIEW",
    checks: [
      {
        name: "vista-previa",
        status: "blocked",
        detail: PREVIEW_NOTICE,
      },
    ],
    blocks: [PREVIEW_NOTICE],
  };
}

function previewApplyReadiness(draft?: PatchDraft): ApplyReadiness {
  return {
    draftId: draft?.id ?? "preview-draft",
    status: "blocked",
    summary: "La vista previa web no puede aplicar cambios.",
    canApply: false,
    currentBranch: "vista-previa",
    targetBranch: "agent/vista-previa",
    branchStrategy: "create_safe_branch",
    confirmToken: "PREVIEW",
    checks: [
      {
        name: "app-escritorio",
        status: "blocked",
        detail: PREVIEW_NOTICE,
      },
    ],
    blocks: [PREVIEW_NOTICE],
    nextActions: ["Abrir la app de escritorio para aplicar cambios reales."],
  };
}

function previewApplyResult(draft?: PatchDraft): ApplyPatchResult {
  return {
    draftId: draft?.id ?? "preview-draft",
    status: "blocked",
    branch: "vista-previa",
    applied: false,
    messages: [PREVIEW_NOTICE],
  };
}

function previewGate(gate: string): GateResult {
  return {
    gate,
    command: `npm run ${gate}`,
    status: "blocked",
    exitCode: 0,
    durationMs: 0,
    summary: "La prueba real se ejecuta desde la app de escritorio.",
    stdout: PREVIEW_NOTICE,
    stderr: "",
  };
}

function previewRun(repoPath: string, objective: string): AgentRun {
  return {
    id: "preview-run",
    repoPath,
    objective,
    branch: "vista-previa",
    status: "blocked",
    mode: "dry_run",
    modelUsed: "vista-previa",
    startedAt: new Date().toISOString(),
    completedAt: new Date().toISOString(),
    steps: [
      {
        order: 1,
        state: "vista-previa",
        status: "blocked",
        summary: PREVIEW_NOTICE,
      },
    ],
    plan: previewMicroPlan(objective),
    lessons: ["La vista previa sirve para revisar la interfaz, no para cambiar OneEpis."],
    persistence: "not_configured",
  };
}

function previewReport(repoPath: string, objective: string): AgentRunReport {
  return {
    runId: "preview-run",
    status: "blocked",
    verdict: "vista_previa",
    objective,
    branch: "vista-previa",
    modelUsed: "vista-previa",
    recommendedGate: "check:size",
    markdown: `# Vista previa\n\n${PREVIEW_NOTICE}`,
    checklist: ["Abrir la app de escritorio.", "Revisar el proyecto real."],
    warnings: [PREVIEW_NOTICE],
    nextActions: ["Abrir la app de escritorio para ejecutar el proceso real."],
  };
}

function previewLocalProblems(): LocalProblemSpec[] {
  return [
    {
      id: "LOCAL-001",
      title: "dieta clinical_intent.py fase 3",
      objective: "Extraer helpers deterministicos sin cambiar API ni prompts.",
      branch: "agent/local-001-dieta-clinical-intent-py-fase-3",
      commitMessage: "LOCAL-001 diet clinical_intent helpers",
      primaryFiles: ["clinical_intent.py"],
      allowedPathMarkers: ["clinical_intent"],
      gates: ["check:api", "check:contract"],
      forbiddenSignals: ["endpoint", "tabla", "ruta", "permisos", "IA nueva", "RAG", "receta", "firma", "dashboard"],
      instructions: [
        "Prioridad: dieta y claridad antes de clinica nueva.",
        "Un problema LOCAL es una rama y un commit local, sin push automatico.",
      ],
    },
    {
      id: "LOCAL-003",
      title: "dividir clinical-intent-result-panel.tsx",
      objective: "Extraer subpaneles visuales pequenos sin cambiar textos clinicos.",
      branch: "agent/local-003-dividir-clinical-intent-result-panel",
      commitMessage: "LOCAL-003 split clinical intent result panel",
      primaryFiles: ["clinical-intent-result-panel.tsx"],
      allowedPathMarkers: ["clinical-intent-result"],
      gates: ["check:web"],
      forbiddenSignals: ["endpoint", "dashboard", "RAG", "receta", "firma"],
      instructions: [
        "Mantener textos clinicos.",
        "Crear commit local solo si check:web pasa.",
      ],
    },
  ];
}

function previewLocalProblemPlan(repoPath: string, problemId: string): LocalProblemPlan {
  const problem = previewLocalProblems().find((item) => item.id === problemId) ?? previewLocalProblems()[0];
  return {
    repoPath,
    problem,
    status: "blocked",
    blockers: [PREVIEW_NOTICE],
    warnings: ["La vista web no puede inspeccionar Git real."],
    nextActions: ["Abrir la app de escritorio para preparar rama y commit local."],
    noPush: true,
  };
}

function previewLocalProblemRun(repoPath: string, problemId: string, status: string): LocalProblemRun {
  const problem = previewLocalProblems().find((item) => item.id === problemId) ?? previewLocalProblems()[0];
  return {
    id: "preview-local-run",
    problemId: problem.id,
    status,
    repoPath,
    branch: problem.branch,
    commitSha: null,
    changedFiles: [],
    gateResults: [],
    blockers: [PREVIEW_NOTICE],
    warnings: ["No hay push automatico."],
    nextActions: ["Usar la app de escritorio para ejecutar el ciclo LOCAL real."],
    noPush: true,
    summary: "Vista previa web del ciclo LOCAL.",
  };
}
