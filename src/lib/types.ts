export type GovernanceDocument = {
  path: string;
  title: string;
  sha256: string;
  bytes: number;
  present: boolean;
};

export type RepoInspection = {
  repoPath: string;
  projectName: string;
  isGitRepo: boolean;
  isOneEpis: boolean;
  currentBranch: string;
  dirty: boolean;
  statusText: string;
  governanceDocuments: GovernanceDocument[];
  declaredGates: string[];
  detectedRules: string[];
  blocks: string[];
};

export type OllamaModel = {
  name: string;
  size: number;
  family: string;
  parameters: string;
  quantization: string;
};

export type ModelPolicy = {
  primaryCode: string;
  fastCode: string;
  governance: string;
  fallback: string;
  embeddings: string;
};

export type OllamaStatus = {
  baseUrl: string;
  available: boolean;
  message: string;
  models: OllamaModel[];
  policy: ModelPolicy;
  missingPolicyModels: string[];
};

export type MicroPlan = {
  objective: string;
  recommendedGate: string;
  riskLevel: "green" | "yellow" | "red" | string;
  touchedSurfaces: string[];
  requiredGates: string[];
  steps: string[];
  warnings: string[];
  blocked: boolean;
  modelUsed: string;
};

export type PatchDraft = {
  id: string;
  repoPath: string;
  objective: string;
  summary: string;
  rationale: string;
  files: string[];
  unifiedDiff: string;
  risks: string[];
  gates: string[];
  blocked: boolean;
  modelUsed: string;
  createdAt: string;
  plan: MicroPlan;
};

export type ReviewCheck = {
  name: string;
  status: "passed" | "blocked" | string;
  detail: string;
};

export type PatchReview = {
  draftId: string;
  approved: boolean;
  confirmToken: string;
  checks: ReviewCheck[];
  blocks: string[];
};

export type ApplyPatchRequest = {
  draft: PatchDraft;
  allowApply: boolean;
  confirmToken?: string | null;
  branchStrategy: "reuse" | "create_safe_branch";
  databaseUrl?: string | null;
};

export type ApplyPatchResult = {
  draftId: string;
  status: "applied" | "blocked" | string;
  branch: string;
  applied: boolean;
  messages: string[];
};

export type GateResult = {
  gate: string;
  command: string;
  status: "passed" | "failed" | "blocked" | string;
  exitCode: number;
  durationMs: number;
  summary: string;
  stdout: string;
  stderr: string;
};

export type AgentRunSummary = {
  id: string;
  repoPath: string;
  branch: string;
  modelUsed: string;
  objective: string;
  status: string;
  mode: string;
  startedAt: string;
  completedAt: string;
  summary: string;
};

export type AgentStep = {
  order: number;
  state: string;
  status: "completed" | "blocked" | "skipped" | "failed";
  summary: string;
};

export type AgentRun = {
  id: string;
  repoPath: string;
  objective: string;
  branch: string;
  status: "completed" | "blocked" | "failed";
  mode: "dry_run" | "controlled";
  modelUsed: string;
  startedAt: string;
  completedAt: string;
  steps: AgentStep[];
  plan: MicroPlan;
  lessons: string[];
  persistence: string;
};
