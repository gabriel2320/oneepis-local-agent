import { invoke } from "@tauri-apps/api/core";
import type {
  AgentRun,
  AgentRunSummary,
  ApplyPatchRequest,
  ApplyPatchResult,
  GateResult,
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

export function planMicrocycle(repoPath: string, objective: string, baseUrl?: string) {
  return invoke<MicroPlan>("plan_microcycle", { repoPath, objective, baseUrl });
}

export function runMicrocycle(repoPath: string, objective: string, maxCycles: number) {
  return invoke<AgentRun>("run_microcycle", {
    request: {
      repoPath,
      objective,
      maxCycles,
      mode: "dry_run",
      databaseUrl: null,
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

export function runGate(repoPath: string, gate: string) {
  return invoke<GateResult>("run_gate", { repoPath, gate, databaseUrl: null, runId: null });
}

export function listRuns(limit = 20) {
  return invoke<AgentRunSummary[]>("list_runs", { databaseUrl: null, limit });
}
