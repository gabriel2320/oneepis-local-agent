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
  MicroPlan,
  OllamaStatus,
  PatchDraft,
  PatchReview,
  RepoInspection,
} from "./types";

export function inspectRepository(repoPath: string) {
  return invoke<RepoInspection>("inspect_repository", { repoPath });
}

export function getOllamaStatus(baseUrl?: string) {
  return invoke<OllamaStatus>("get_ollama_status", { baseUrl });
}

export function getDevelopmentReadiness(repoPath: string, baseUrl?: string) {
  return invoke<DevelopmentReadiness>("development_readiness", { repoPath, baseUrl });
}

export function getDevelopmentWorkPackage(repoPath: string, objective: string, baseUrl?: string) {
  return invoke<DevelopmentWorkPackage>("development_work_package", { repoPath, objective, baseUrl });
}

export function getDevelopmentContextPack(repoPath: string, objective: string, baseUrl?: string) {
  return invoke<DevelopmentContextPack>("development_context_pack", { repoPath, objective, baseUrl });
}

export function getDevelopmentBrief(repoPath: string, objective: string, askModel = false, baseUrl?: string) {
  return invoke<DevelopmentBrief>("development_brief", { repoPath, objective, askModel, baseUrl });
}

export function getImplementationDecision(repoPath: string, objective: string, askModel = true, baseUrl?: string) {
  return invoke<ImplementationDecision>("implementation_decision", { repoPath, objective, askModel, baseUrl });
}

export function getEvolutionPlan(repoPath: string, objective: string, baseUrl?: string) {
  return invoke<EvolutionPlan>("evolution_plan", { repoPath, objective, baseUrl });
}

export function planMicrocycle(repoPath: string, objective: string, baseUrl?: string) {
  return invoke<MicroPlan>("plan_microcycle", { repoPath, objective, baseUrl });
}

export function runMicrocycle(repoPath: string, objective: string, maxCycles: number, askModel = false) {
  return invoke<AgentRun>("run_microcycle", {
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
  return invoke<AgentRunReport>("run_microcycle_report", {
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
  return invoke<PatchDraft>("draft_patch", { repoPath, objective, baseUrl, databaseUrl: null });
}

export function reviewPatch(draft: PatchDraft) {
  return invoke<PatchReview>("review_patch", { draft });
}

export function applyApprovedPatch(request: ApplyPatchRequest) {
  return invoke<ApplyPatchResult>("apply_approved_patch", { request });
}

export function prepareApplyReadiness(draft: PatchDraft, confirmToken?: string | null) {
  return invoke<ApplyReadiness>("prepare_apply_readiness", {
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
  return invoke<GateResult>("run_gate", { repoPath, gate, databaseUrl: null, runId: null });
}

export function listRuns(limit = 20) {
  return invoke<AgentRunSummary[]>("list_runs", { databaseUrl: null, limit });
}
