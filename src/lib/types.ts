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
  steps: string[];
  warnings: string[];
  blocked: boolean;
  modelUsed: string;
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

