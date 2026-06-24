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

export type ReadinessCheck = {
  name: string;
  status: "ready" | "blocked" | string;
  detail: string;
  action: string;
};

export type SuggestedMicrocycle = {
  title: string;
  objective: string;
  riskLevel: "green" | "yellow" | "red" | string;
  gates: string[];
  reason: string;
};

export type DevelopmentReadiness = {
  repoPath: string;
  profile: "oneepis" | "generic" | string;
  status: "ready" | "attention" | "blocked" | string;
  summary: string;
  checks: ReadinessCheck[];
  blockers: string[];
  warnings: string[];
  nextActions: string[];
  suggestedMicrocycles: SuggestedMicrocycle[];
  requiredGates: string[];
  localModelSummary: string;
};

export type WorkPackageTest = {
  gate: string;
  command: string;
  purpose: string;
  required: boolean;
};

export type DevelopmentWorkPackage = {
  repoPath: string;
  title: string;
  objective: string;
  status: "ready_to_draft" | "blocked" | "needs_gate" | string;
  summary: string;
  branchStrategy: string;
  filesToInspect: string[];
  implementationSteps: string[];
  testPlan: WorkPackageTest[];
  acceptanceCriteria: string[];
  stopConditions: string[];
  gates: string[];
  warnings: string[];
  canDraft: boolean;
  canApply: boolean;
};

export type ContextPackFile = {
  path: string;
  kind: "file" | "directory" | "missing" | "skipped" | string;
  bytes: number;
  lines: number;
  sha256: string;
  summary: string;
  excerpt: string;
};

export type DevelopmentContextPack = {
  repoPath: string;
  objective: string;
  status: "ready" | "partial" | "blocked" | string;
  summary: string;
  files: ContextPackFile[];
  warnings: string[];
  promptNotes: string[];
  gates: string[];
  totalBytes: number;
  maxBytes: number;
};

export type LocalModelProposal = {
  status: string;
  modelUsed: string;
  summary: string;
  filesToChange: string[];
  implementationNotes: string[];
  risks: string[];
  gates: string[];
  rawResponse: string;
};

export type DevelopmentBrief = {
  repoPath: string;
  objective: string;
  status: "ready" | "partial" | "blocked" | string;
  summary: string;
  modelUsed: string;
  workOrder: string;
  systemPrompt: string;
  userPrompt: string;
  responseContract: string[];
  contextFiles: string[];
  gates: string[];
  warnings: string[];
  stopConditions: string[];
  nextActions: string[];
  proposal?: LocalModelProposal | null;
};

export type ImplementationDecision = {
  repoPath: string;
  objective: string;
  status: "ready_to_draft" | "needs_model_proposal" | "blocked" | string;
  summary: string;
  modelUsed: string;
  sourceProposalStatus: string;
  selectedFiles: string[];
  implementationSteps: string[];
  requiredGates: string[];
  acceptanceCriteria: string[];
  blockers: string[];
  warnings: string[];
  patchIntent: string;
  nextActions: string[];
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

export type ApplyReadiness = {
  draftId: string;
  status: "ready_for_confirmation" | "ready_to_apply" | "blocked" | string;
  summary: string;
  canApply: boolean;
  currentBranch: string;
  targetBranch: string;
  branchStrategy: "reuse" | "create_safe_branch" | string;
  confirmToken: string;
  checks: ReviewCheck[];
  blocks: string[];
  nextActions: string[];
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

export type AgentRunReport = {
  runId: string;
  status: string;
  verdict: string;
  objective: string;
  branch: string;
  modelUsed: string;
  recommendedGate: string;
  markdown: string;
  checklist: string[];
  warnings: string[];
  nextActions: string[];
};
