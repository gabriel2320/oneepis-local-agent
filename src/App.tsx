import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import {
  Activity,
  Bot,
  BrainCircuit,
  CheckCircle2,
  FileText,
  GitBranch,
  ListChecks,
  Play,
  ShieldCheck,
  Terminal,
} from "lucide-react";
import {
  draftPatch,
  getOllamaStatus,
  inspectRepository,
  listRuns,
  planMicrocycle,
  reviewPatch,
  runGate,
  runMicrocycle,
} from "./lib/api";
import type {
  AgentRun,
  AgentRunSummary,
  GateResult,
  MicroPlan,
  OllamaStatus,
  PatchDraft,
  PatchReview,
  RepoInspection,
} from "./lib/types";
import { Badge } from "./components/ui/badge";
import { Button } from "./components/ui/button";
import { Card } from "./components/ui/card";
import { cn } from "./lib/utils";

const defaultRepo = "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis";
const tabs = ["repo", "plan", "patch", "gates", "bitacora"] as const;
type Tab = (typeof tabs)[number];

function App() {
  const [repoPath, setRepoPath] = useState(defaultRepo);
  const [objective, setObjective] = useState("Auditar el repo y proponer el microciclo mas pequeno gobernado.");
  const [activeTab, setActiveTab] = useState<Tab>("repo");
  const [inspection, setInspection] = useState<RepoInspection | null>(null);
  const [ollama, setOllama] = useState<OllamaStatus | null>(null);
  const [plan, setPlan] = useState<MicroPlan | null>(null);
  const [draft, setDraft] = useState<PatchDraft | null>(null);
  const [review, setReview] = useState<PatchReview | null>(null);
  const [gateResult, setGateResult] = useState<GateResult | null>(null);
  const [run, setRun] = useState<AgentRun | null>(null);
  const [runs, setRuns] = useState<AgentRunSummary[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const repoTone = useMemo(() => {
    if (!inspection) return "neutral";
    if (inspection.blocks.length > 0) return "warning";
    return "success";
  }, [inspection]);
  const primaryGate = plan?.recommendedGate && plan.recommendedGate !== "sin_gate"
    ? plan.recommendedGate
    : inspection?.declaredGates[0] ?? "";
  const blockers = [
    ...(inspection?.blocks ?? []),
    ...(ollama && !ollama.available ? [ollama.message] : []),
    ...(ollama?.missingPolicyModels.length ? [`Faltan modelos: ${ollama.missingPolicyModels.join(", ")}`] : []),
  ];

  async function loadAll() {
    setBusy("inspect");
    setError(null);
    try {
      const [repo, ai, history] = await Promise.all([
        inspectRepository(repoPath),
        getOllamaStatus(),
        listRuns(20),
      ]);
      setInspection(repo);
      setOllama(ai);
      setRuns(history);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  async function createPlan() {
    setBusy("plan");
    setError(null);
    try {
      const nextPlan = await planMicrocycle(repoPath, objective);
      setPlan(nextPlan);
      setActiveTab("plan");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  async function createDraft() {
    setBusy("draft");
    setError(null);
    try {
      const nextDraft = await draftPatch(repoPath, objective);
      const nextReview = await reviewPatch(nextDraft);
      setDraft(nextDraft);
      setReview(nextReview);
      setPlan(nextDraft.plan);
      setActiveTab("patch");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  async function runDryCycle() {
    setBusy("run");
    setError(null);
    try {
      const nextRun = await runMicrocycle(repoPath, objective, 1);
      setRun(nextRun);
      setPlan(nextRun.plan);
      setRuns(await listRuns(20));
      setActiveTab("bitacora");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  async function runSelectedGate() {
    if (!primaryGate) {
      setError("No hay gate declarado para ejecutar.");
      return;
    }
    setBusy("gate");
    setError(null);
    try {
      setGateResult(await runGate(repoPath, primaryGate));
      setActiveTab("gates");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  useEffect(() => {
    void loadAll();
  }, []);

  return (
    <main className="min-h-screen bg-background text-foreground">
      <div className="mx-auto flex max-w-7xl flex-col gap-5 px-6 py-5">
        <header className="flex flex-col gap-3 border-b border-border pb-4">
          <div className="flex flex-col gap-3 lg:flex-row lg:items-end lg:justify-between">
            <div>
              <div className="flex items-center gap-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                <Bot className="h-4 w-4" />
                OneEpis Local Agent
              </div>
              <h1 className="mt-2 text-2xl font-semibold">Asistente local gobernado</h1>
            </div>
            <div className="flex flex-wrap gap-2">
              <Badge tone={ollama?.available ? "success" : "warning"}>{ollama?.available ? "Ollama activo" : "Ollama pendiente"}</Badge>
              <Badge tone={repoTone}>{inspection?.isOneEpis ? "OneEpis" : "Repo generico"}</Badge>
              <Badge tone={plan?.riskLevel === "red" ? "danger" : plan?.riskLevel === "yellow" ? "warning" : "neutral"}>
                Riesgo {plan?.riskLevel ?? "sin plan"}
              </Badge>
            </div>
          </div>
          <nav className="flex flex-wrap gap-2">
            {tabs.map((tab) => (
              <button
                key={tab}
                type="button"
                onClick={() => setActiveTab(tab)}
                className={cn(
                  "h-9 rounded-md border px-3 text-sm font-medium transition",
                  activeTab === tab ? "border-primary bg-primary text-primary-foreground" : "border-border bg-surface text-muted-foreground hover:bg-muted",
                )}
              >
                {tabLabel(tab)}
              </button>
            ))}
          </nav>
        </header>

        {error && (
          <div className="rounded-md border border-danger/30 bg-danger/10 px-4 py-3 text-sm text-danger">{error}</div>
        )}

        <section className="grid gap-4 lg:grid-cols-[minmax(0,1.1fr)_minmax(340px,0.9fr)]">
          <Card
            title="Objetivo"
            actions={
              <Button variant="secondary" onClick={loadAll} disabled={busy !== null}>
                {busy === "inspect" ? "Inspeccionando..." : "Inspeccionar"}
              </Button>
            }
          >
            <div className="grid gap-3">
              <label className="grid gap-1 text-sm">
                <span className="text-xs font-medium text-muted-foreground">Repo</span>
                <input
                  value={repoPath}
                  onChange={(event) => setRepoPath(event.target.value)}
                  className="h-10 rounded-md border border-border bg-background px-3 text-sm outline-none focus:border-primary"
                />
              </label>
              <label className="grid gap-1 text-sm">
                <span className="text-xs font-medium text-muted-foreground">Microciclo</span>
                <textarea
                  value={objective}
                  onChange={(event) => setObjective(event.target.value)}
                  rows={3}
                  className="rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary"
                />
              </label>
              <div className="flex flex-wrap gap-2">
                <Button onClick={createPlan} disabled={busy !== null}>
                  <BrainCircuit className="mr-2 h-4 w-4" />
                  {busy === "plan" ? "Planificando..." : "Plan"}
                </Button>
                <Button variant="secondary" onClick={createDraft} disabled={busy !== null}>
                  <FileText className="mr-2 h-4 w-4" />
                  {busy === "draft" ? "Generando..." : "PatchDraft"}
                </Button>
                <Button variant="secondary" onClick={runDryCycle} disabled={busy !== null}>
                  <Play className="mr-2 h-4 w-4" />
                  {busy === "run" ? "Ejecutando..." : "Dry-run"}
                </Button>
                <Button variant="secondary" onClick={runSelectedGate} disabled={busy !== null || !primaryGate}>
                  <Terminal className="mr-2 h-4 w-4" />
                  {busy === "gate" ? "Gate..." : primaryGate || "Sin gate"}
                </Button>
              </div>
            </div>
          </Card>

          <Card title="Bloqueos">
            {blockers.length > 0 ? <List items={blockers} tone="warning" /> : <Empty text="Sin bloqueos activos." />}
          </Card>
        </section>

        {activeTab === "repo" && <RepoTab inspection={inspection} ollama={ollama} />}
        {activeTab === "plan" && <PlanTab plan={plan} />}
        {activeTab === "patch" && <PatchTab draft={draft} review={review} />}
        {activeTab === "gates" && <GateTab inspection={inspection} plan={plan} gateResult={gateResult} />}
        {activeTab === "bitacora" && <HistoryTab run={run} runs={runs} />}
      </div>
    </main>
  );
}

function RepoTab({ inspection, ollama }: { inspection: RepoInspection | null; ollama: OllamaStatus | null }) {
  const isMissingModel = (value?: string) => (value ? ollama?.missingPolicyModels.includes(value) : false);
  return (
    <section className="grid gap-4 lg:grid-cols-3">
      <Card title="Gobernanza">
        <PanelIcon icon={<ShieldCheck className="h-4 w-4" />} label={inspection?.projectName ?? "Sin repo"} />
        <List items={inspection?.detectedRules ?? []} empty="Sin reglas detectadas." />
      </Card>

      <Card title="Git">
        <PanelIcon icon={<GitBranch className="h-4 w-4" />} label={inspection?.currentBranch || "Sin rama"} />
        <pre className="mt-3 max-h-44 overflow-auto rounded border border-border bg-background p-3 text-xs text-muted-foreground">
          {inspection?.statusText || "Sin estado Git."}
        </pre>
      </Card>

      <Card title="Ollama">
        <div className="grid grid-cols-2 gap-2 text-sm">
          <ModelSlot label="Codigo" value={ollama?.policy.primaryCode} missing={isMissingModel(ollama?.policy.primaryCode)} />
          <ModelSlot label="Rapido" value={ollama?.policy.fastCode} missing={isMissingModel(ollama?.policy.fastCode)} />
          <ModelSlot label="Gobierno" value={ollama?.policy.governance} missing={isMissingModel(ollama?.policy.governance)} />
          <ModelSlot label="Fallback" value={ollama?.policy.fallback} missing={isMissingModel(ollama?.policy.fallback)} />
        </div>
        <p className="mt-3 text-xs text-muted-foreground">{ollama?.models.length ?? 0} modelos en {ollama?.baseUrl ?? "Ollama"}</p>
      </Card>
    </section>
  );
}

function PlanTab({ plan }: { plan: MicroPlan | null }) {
  if (!plan) return <Empty text="Sin microplan." />;
  return (
    <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_360px]">
      <Card title="Microplan">
        <div className="flex flex-wrap gap-2">
          <Badge tone={plan.blocked ? "warning" : "success"}>{plan.blocked ? "Bloqueado" : "Revisable"}</Badge>
          <Badge tone={riskTone(plan.riskLevel)}>Riesgo {plan.riskLevel}</Badge>
          <Badge>{plan.modelUsed || "local_rules"}</Badge>
        </div>
        <p className="mt-3 text-sm leading-6">{plan.objective}</p>
        <List items={plan.steps} />
      </Card>
      <Card title="Superficies">
        <List items={plan.touchedSurfaces} empty="Sin superficies." />
        <div className="mt-4">
          <PanelIcon icon={<ListChecks className="h-4 w-4" />} label={plan.recommendedGate || "sin_gate"} />
          <List items={plan.requiredGates} empty="Sin gates requeridos." />
        </div>
        <List items={plan.warnings} tone="warning" />
      </Card>
    </section>
  );
}

function PatchTab({ draft, review }: { draft: PatchDraft | null; review: PatchReview | null }) {
  if (!draft) return <Empty text="Sin PatchDraft." />;
  return (
    <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_380px]">
      <Card title="PatchDraft">
        <div className="flex flex-wrap gap-2">
          <Badge tone={draft.blocked ? "warning" : "success"}>{draft.blocked ? "Bloqueado" : "Aplicable"}</Badge>
          <Badge>{draft.id}</Badge>
          <Badge>{draft.modelUsed || "local_rules"}</Badge>
        </div>
        <p className="mt-3 text-sm leading-6">{draft.summary}</p>
        <pre className="mt-3 max-h-[420px] overflow-auto rounded border border-border bg-background p-3 text-xs text-muted-foreground">
          {draft.unifiedDiff}
        </pre>
      </Card>
      <Card title="Revision">
        {review ? (
          <>
            <div className="flex flex-wrap gap-2">
              <Badge tone={review.approved ? "success" : "warning"}>{review.approved ? "Aprobado" : "Bloqueado"}</Badge>
              <Badge>{review.confirmToken}</Badge>
            </div>
            <List items={review.checks.map((item) => `${item.name}: ${item.status}`)} />
            <List items={review.blocks} tone="warning" />
          </>
        ) : (
          <Empty text="Sin revision." />
        )}
        <List items={draft.risks} tone="warning" />
      </Card>
    </section>
  );
}

function GateTab({
  inspection,
  plan,
  gateResult,
}: {
  inspection: RepoInspection | null;
  plan: MicroPlan | null;
  gateResult: GateResult | null;
}) {
  return (
    <section className="grid gap-4 lg:grid-cols-[360px_minmax(0,1fr)]">
      <Card title="Gates">
        <List items={inspection?.declaredGates ?? []} empty="Sin gates declarados." />
        {plan?.recommendedGate && <p className="mt-3 text-sm text-muted-foreground">Recomendado: {plan.recommendedGate}</p>}
      </Card>
      <Card title="Resultado">
        {gateResult ? (
          <>
            <div className="flex flex-wrap gap-2">
              <Badge tone={gateResult.status === "passed" ? "success" : gateResult.status === "failed" ? "danger" : "warning"}>
                {gateResult.status}
              </Badge>
              <Badge>{gateResult.command}</Badge>
              <Badge>{gateResult.durationMs} ms</Badge>
            </div>
            <p className="mt-3 text-sm">{gateResult.summary}</p>
            <pre className="mt-3 max-h-72 overflow-auto rounded border border-border bg-background p-3 text-xs text-muted-foreground">
              {gateResult.stdout || gateResult.stderr || "Sin salida."}
            </pre>
          </>
        ) : (
          <Empty text="Sin resultado de gate." />
        )}
      </Card>
    </section>
  );
}

function HistoryTab({ run, runs }: { run: AgentRun | null; runs: AgentRunSummary[] }) {
  return (
    <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_420px]">
      <Card title="Ultimo ciclo">
        {run ? (
          <>
            <div className="flex flex-wrap gap-2">
              <Badge tone={run.status === "completed" ? "success" : "warning"}>{run.status}</Badge>
              <Badge>{run.mode}</Badge>
              <Badge>{run.persistence}</Badge>
            </div>
            <List items={run.steps.map((step) => `${step.state}: ${step.summary}`)} />
          </>
        ) : (
          <Empty text="Sin ciclo reciente." />
        )}
      </Card>
      <Card title="Runs">
        {runs.length > 0 ? (
          <div className="grid gap-2">
            {runs.map((item) => (
              <div key={item.id} className="rounded border border-border px-3 py-2 text-xs">
                <div className="flex items-center justify-between gap-2">
                  <span className="font-medium">{item.status}</span>
                  <span className="text-muted-foreground">{item.startedAt}</span>
                </div>
                <p className="mt-1 text-muted-foreground">{item.summary}</p>
              </div>
            ))}
          </div>
        ) : (
          <Empty text="Sin runs persistidos." />
        )}
      </Card>
    </section>
  );
}

function ModelSlot({ label, value, missing }: { label: string; value?: string; missing?: boolean }) {
  return (
    <div className="rounded border border-border p-2">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-1 flex items-center gap-2 text-xs font-medium">
        {missing ? <Activity className="h-3.5 w-3.5 text-warning" /> : <CheckCircle2 className="h-3.5 w-3.5 text-success" />}
        {value ?? "sin modelo"}
      </div>
    </div>
  );
}

function PanelIcon({ icon, label }: { icon: ReactNode; label: string }) {
  return (
    <div className="flex items-center gap-2 text-sm font-medium">
      <span className="flex h-7 w-7 items-center justify-center rounded-md border border-border bg-background text-muted-foreground">{icon}</span>
      {label}
    </div>
  );
}

function List({ items, empty, tone = "neutral" }: { items: string[]; empty?: string; tone?: "neutral" | "warning" }) {
  if (items.length === 0) return empty ? <p className="mt-3 text-sm text-muted-foreground">{empty}</p> : null;
  return (
    <ul className="mt-3 grid gap-2 text-sm">
      {items.map((item) => (
        <li key={item} className={tone === "warning" ? "text-warning" : "text-muted-foreground"}>
          {item}
        </li>
      ))}
    </ul>
  );
}

function Empty({ text }: { text: string }) {
  return <p className="rounded border border-dashed border-border p-4 text-sm text-muted-foreground">{text}</p>;
}

function tabLabel(tab: Tab) {
  const labels: Record<Tab, string> = {
    repo: "Repo",
    plan: "Plan",
    patch: "Patch",
    gates: "Gates",
    bitacora: "Bitacora",
  };
  return labels[tab];
}

function riskTone(risk: string) {
  if (risk === "red") return "danger";
  if (risk === "yellow") return "warning";
  return "success";
}

export default App;
