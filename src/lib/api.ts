import { invoke } from "@tauri-apps/api/core";
import type { AgentRun, MicroPlan, OllamaStatus, RepoInspection } from "./types";

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
      repo_path: repoPath,
      objective,
      max_cycles: maxCycles,
      mode: "dry_run",
      database_url: null,
    },
  });
}

export function runOneEpisAutopilot(workspacePath: string, objective: string, maxCycles: number) {
  return invoke<AgentRun>("run_oneepis_autopilot", {
    request: {
      workspace_path: workspacePath || null,
      repo_url: null,
      objective,
      max_cycles: maxCycles,
      mode: "controlled",
      database_url: null,
    },
  });
}
