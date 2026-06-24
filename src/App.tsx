import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import {
  Activity,
  BookOpenCheck,
  Bot,
  BrainCircuit,
  CheckCircle2,
  CircleDashed,
  ClipboardList,
  FileText,
  GitBranch,
  HelpCircle,
  Info,
  ListChecks,
  Play,
  ShieldCheck,
  Terminal,
} from "lucide-react";
import {
  draftPatch,
  getDevelopmentBrief,
  getDevelopmentContextPack,
  getDevelopmentReadiness,
  getDevelopmentWorkPackage,
  getImplementationDecision,
  getOllamaStatus,
  inspectRepository,
  listRuns,
  planMicrocycle,
  prepareApplyReadiness,
  reviewPatch,
  runGate,
  runMicrocycle,
  runMicrocycleReport,
} from "./lib/api";
import type {
  AgentRun,
  AgentRunReport,
  AgentRunSummary,
  ApplyReadiness,
  DevelopmentBrief,
  DevelopmentContextPack,
  DevelopmentReadiness,
  DevelopmentWorkPackage,
  GateResult,
  ImplementationDecision,
  MicroPlan,
  OllamaStatus,
  PatchDraft,
  PatchReview,
  RepoInspection,
} from "./lib/types";
import { Badge } from "./components/ui/badge";
import { Button } from "./components/ui/button";
import { Card } from "./components/ui/card";
import { buildAgentNarrative, explainStatus } from "./lib/narrative";
import type { AgentNarrative, NarrativeTone } from "./lib/narrative";
import { cn } from "./lib/utils";

const defaultRepo = "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis";
const tabs = ["repo", "preparacion", "paquete", "contexto", "brief", "decision", "microproceso", "reporte", "plan", "patch", "gates", "bitacora"] as const;
type Tab = (typeof tabs)[number];
type MicroStepStatus = "pending" | "running" | "completed" | "blocked" | "failed";
type MicroStep = {
  id: string;
  label: string;
  status: MicroStepStatus;
  detail: string;
};

const initialMicroSteps: MicroStep[] = [
  { id: "inspect", label: "Inspeccion", status: "pending", detail: "Sin ejecutar." },
  { id: "package", label: "Paquete", status: "pending", detail: "Sin ejecutar." },
  { id: "context", label: "Contexto", status: "pending", detail: "Sin ejecutar." },
  { id: "brief", label: "Brief", status: "pending", detail: "Sin ejecutar." },
  { id: "decision", label: "Decision", status: "pending", detail: "Sin ejecutar." },
  { id: "plan", label: "Plan", status: "pending", detail: "Sin ejecutar." },
  { id: "draft", label: "PatchDraft", status: "pending", detail: "Sin ejecutar." },
  { id: "run", label: "Dry-run", status: "pending", detail: "Sin ejecutar." },
  { id: "gate", label: "Gate", status: "pending", detail: "Sin ejecutar." },
];

function App() {
  const [repoPath, setRepoPath] = useState(defaultRepo);
  const [objective, setObjective] = useState("Auditar el repo y proponer el microciclo mas pequeno gobernado.");
  const [activeTab, setActiveTab] = useState<Tab>("repo");
  const [inspection, setInspection] = useState<RepoInspection | null>(null);
  const [ollama, setOllama] = useState<OllamaStatus | null>(null);
  const [readiness, setReadiness] = useState<DevelopmentReadiness | null>(null);
  const [workPackage, setWorkPackage] = useState<DevelopmentWorkPackage | null>(null);
  const [contextPack, setContextPack] = useState<DevelopmentContextPack | null>(null);
  const [brief, setBrief] = useState<DevelopmentBrief | null>(null);
  const [decision, setDecision] = useState<ImplementationDecision | null>(null);
  const [plan, setPlan] = useState<MicroPlan | null>(null);
  const [draft, setDraft] = useState<PatchDraft | null>(null);
  const [review, setReview] = useState<PatchReview | null>(null);
  const [applyReadiness, setApplyReadiness] = useState<ApplyReadiness | null>(null);
  const [gateResult, setGateResult] = useState<GateResult | null>(null);
  const [run, setRun] = useState<AgentRun | null>(null);
  const [report, setReport] = useState<AgentRunReport | null>(null);
  const [runs, setRuns] = useState<AgentRunSummary[]>([]);
  const [microSteps, setMicroSteps] = useState<MicroStep[]>(initialMicroSteps);
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
  const narrative = useMemo(
    () => buildAgentNarrative({ inspection, ollama, plan, contextPack, brief, draft, review, gateResult, run, blockers, busy }),
    [inspection, ollama, plan, contextPack, brief, draft, review, gateResult, run, blockers, busy],
  );

  async function loadAll() {
    setBusy("inspect");
    setError(null);
    try {
      const [repo, ai, history, ready] = await Promise.all([
        inspectRepository(repoPath),
        getOllamaStatus(),
        listRuns(20),
        getDevelopmentReadiness(repoPath),
      ]);
      setInspection(repo);
      setOllama(ai);
      setRuns(history);
      setReadiness(ready);
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
      setApplyReadiness(null);
      setPlan(nextDraft.plan);
      setActiveTab("patch");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  async function createWorkPackage() {
    setBusy("workPackage");
    setError(null);
    try {
      const nextPackage = await getDevelopmentWorkPackage(repoPath, objective);
      setWorkPackage(nextPackage);
      setActiveTab("paquete");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  async function createContextPack() {
    setBusy("contextPack");
    setError(null);
    try {
      const nextContext = await getDevelopmentContextPack(repoPath, objective);
      setContextPack(nextContext);
      setActiveTab("contexto");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  async function createBrief() {
    setBusy("brief");
    setError(null);
    try {
      const nextBrief = await getDevelopmentBrief(repoPath, objective, true);
      setBrief(nextBrief);
      setActiveTab("brief");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  async function createDecision() {
    setBusy("decision");
    setError(null);
    try {
      const nextDecision = await getImplementationDecision(repoPath, objective, true);
      setDecision(nextDecision);
      setActiveTab("decision");
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

  async function createReport() {
    setBusy("report");
    setError(null);
    try {
      const nextReport = await runMicrocycleReport(repoPath, objective);
      setReport(nextReport);
      setActiveTab("reporte");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  async function prepareApply() {
    if (!draft) {
      setError("No hay PatchDraft para prevalidar.");
      return;
    }
    setBusy("applyReadiness");
    setError(null);
    try {
      const readiness = await prepareApplyReadiness(draft);
      setApplyReadiness(readiness);
      setActiveTab("patch");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  async function runMicroProcess() {
    setBusy("microprocess");
    setError(null);
    setActiveTab("microproceso");
    setMicroSteps(initialMicroSteps);
    try {
      markMicroStep("inspect", "running", "Leyendo repo, modelos y bitacora.");
      const [repo, ai, history, ready] = await Promise.all([
        inspectRepository(repoPath),
        getOllamaStatus(),
        listRuns(20),
        getDevelopmentReadiness(repoPath),
      ]);
      setInspection(repo);
      setOllama(ai);
      setRuns(history);
      setReadiness(ready);
      markMicroStep("inspect", repo.blocks.length > 0 ? "blocked" : "completed", repo.blocks[0] ?? `${repo.projectName} en ${repo.currentBranch}.`);

      markMicroStep("package", "running", "Preparando paquete de trabajo.");
      const nextPackage = await getDevelopmentWorkPackage(repoPath, objective);
      setWorkPackage(nextPackage);
      markMicroStep("package", nextPackage.status === "blocked" ? "blocked" : "completed", `${nextPackage.filesToInspect.length} rutas; gates ${nextPackage.gates.join(", ") || "sin_gate"}.`);

      markMicroStep("context", "running", "Preparando contexto local sanitizado.");
      const nextContext = await getDevelopmentContextPack(repoPath, objective);
      setContextPack(nextContext);
      markMicroStep("context", nextContext.status === "blocked" ? "blocked" : "completed", `${nextContext.files.length} entradas; ${nextContext.totalBytes}/${nextContext.maxBytes} bytes.`);

      markMicroStep("brief", "running", "Preparando brief para modelo local.");
      const nextBrief = await getDevelopmentBrief(repoPath, objective, true);
      setBrief(nextBrief);
      markMicroStep("brief", nextBrief.status === "blocked" ? "blocked" : "completed", nextBrief.proposal?.summary ?? nextBrief.summary);

      markMicroStep("decision", "running", "Cerrando propuesta local como decision revisable.");
      const nextDecision = await getImplementationDecision(repoPath, objective, true);
      setDecision(nextDecision);
      markMicroStep(
        "decision",
        nextDecision.status === "ready_to_draft" ? "completed" : "blocked",
        nextDecision.blockers[0] ?? nextDecision.summary,
      );

      markMicroStep("plan", "running", "Generando microplan gobernado.");
      const nextPlan = await planMicrocycle(repoPath, objective);
      setPlan(nextPlan);
      markMicroStep("plan", nextPlan.blocked ? "blocked" : "completed", `Modelo ${nextPlan.modelUsed}; gate ${nextPlan.recommendedGate}.`);

      markMicroStep("draft", "running", "Preparando PatchDraft revisable.");
      const nextDraft = await draftPatch(repoPath, objective);
      const nextReview = await reviewPatch(nextDraft);
      setDraft(nextDraft);
      setReview(nextReview);
      markMicroStep("draft", nextReview.approved ? "completed" : "blocked", nextReview.blocks[0] ?? nextDraft.summary);

      markMicroStep("run", "running", "Ejecutando dry-run sin escritura.");
      const nextRun = await runMicrocycle(repoPath, objective, 1);
      setRun(nextRun);
      setPlan(nextRun.plan);
      markMicroStep("run", nextRun.status === "completed" ? "completed" : "blocked", `Run ${nextRun.id}: ${nextRun.status}.`);

      const selectedGate = selectSmallGate(repo.declaredGates, nextRun.plan.recommendedGate);
      if (selectedGate) {
        markMicroStep("gate", "running", `Ejecutando ${selectedGate}.`);
        const nextGate = await runGate(repoPath, selectedGate);
        setGateResult(nextGate);
        markMicroStep("gate", gateStatus(nextGate.status), nextGate.summary);
      } else {
        markMicroStep("gate", "blocked", "Sin gate declarado.");
      }
      setRuns(await listRuns(20));
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      markRunningMicroStepFailed(message);
    } finally {
      setBusy(null);
    }
  }

  function markMicroStep(id: string, status: MicroStepStatus, detail: string) {
    setMicroSteps((current) =>
      current.map((step) => (step.id === id ? { ...step, status, detail } : step)),
    );
  }

  function markRunningMicroStepFailed(message: string) {
    setMicroSteps((current) =>
      current.map((step) => (step.status === "running" ? { ...step, status: "failed", detail: message } : step)),
    );
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
              <Badge tone={readinessTone(readiness?.status)}>{readiness ? `Preparacion ${readinessLabel(readiness.status)}` : "Preparacion pendiente"}</Badge>
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
          <div className="rounded-md border border-danger/30 bg-danger/10 px-4 py-3 text-sm text-danger break-words">{error}</div>
        )}

        <AgentNarrativePanel narrative={narrative} />

        <section className="grid gap-4 lg:grid-cols-[minmax(0,1.1fr)_minmax(340px,0.9fr)]">
          <Card
            title="Objetivo"
            description="El agente trabaja en ciclos cerrados: inspecciona, planifica, propone diff, revisa safety, ejecuta gate y se detiene."
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
                  className="h-10 min-w-0 rounded-md border border-border bg-background px-3 text-sm outline-none focus:border-primary"
                />
              </label>
              <label className="grid gap-1 text-sm">
                <span className="text-xs font-medium text-muted-foreground">Microciclo</span>
                <textarea
                  value={objective}
                  onChange={(event) => setObjective(event.target.value)}
                  rows={3}
                  className="min-w-0 rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary"
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
                <Button variant="secondary" onClick={createWorkPackage} disabled={busy !== null}>
                  <ClipboardList className="mr-2 h-4 w-4" />
                  {busy === "workPackage" ? "Paquete..." : "Paquete"}
                </Button>
                <Button variant="secondary" onClick={createContextPack} disabled={busy !== null}>
                  <BookOpenCheck className="mr-2 h-4 w-4" />
                  {busy === "contextPack" ? "Contexto..." : "Contexto"}
                </Button>
                <Button variant="secondary" onClick={createBrief} disabled={busy !== null}>
                  <Bot className="mr-2 h-4 w-4" />
                  {busy === "brief" ? "Brief..." : "Brief IA"}
                </Button>
                <Button variant="secondary" onClick={createDecision} disabled={busy !== null}>
                  <ListChecks className="mr-2 h-4 w-4" />
                  {busy === "decision" ? "Decidiendo..." : "Decision"}
                </Button>
                <Button variant="secondary" onClick={runDryCycle} disabled={busy !== null}>
                  <Play className="mr-2 h-4 w-4" />
                  {busy === "run" ? "Ejecutando..." : "Dry-run"}
                </Button>
                <Button variant="secondary" onClick={createReport} disabled={busy !== null}>
                  <ClipboardList className="mr-2 h-4 w-4" />
                  {busy === "report" ? "Reporte..." : "Reporte PR"}
                </Button>
                <Button variant="secondary" onClick={runMicroProcess} disabled={busy !== null}>
                  <CircleDashed className="mr-2 h-4 w-4" />
                  {busy === "microprocess" ? "Micro..." : "Microproceso"}
                </Button>
                <Button variant="secondary" onClick={runSelectedGate} disabled={busy !== null || !primaryGate}>
                  <Terminal className="mr-2 h-4 w-4" />
                  {busy === "gate" ? "Gate..." : primaryGate || "Sin gate"}
                </Button>
              </div>
            </div>
          </Card>

          <Card title="Bloqueos" description="Si aparece algo aqui, el agente no avanza a escritura real hasta resolverlo.">
            {blockers.length > 0 ? <List items={blockers} tone="warning" /> : <Empty text="Sin bloqueos activos." />}
          </Card>
        </section>

        {activeTab === "repo" && <RepoTab inspection={inspection} ollama={ollama} />}
        {activeTab === "preparacion" && <ReadinessTab readiness={readiness} />}
        {activeTab === "paquete" && <WorkPackageTab workPackage={workPackage} />}
        {activeTab === "contexto" && <ContextPackTab contextPack={contextPack} />}
        {activeTab === "brief" && <BriefTab brief={brief} />}
        {activeTab === "decision" && <DecisionTab decision={decision} />}
        {activeTab === "microproceso" && <MicroProcessTab steps={microSteps} run={run} gateResult={gateResult} />}
        {activeTab === "reporte" && <ReportTab report={report} />}
        {activeTab === "plan" && <PlanTab plan={plan} />}
        {activeTab === "patch" && (
          <PatchTab
            draft={draft}
            review={review}
            readiness={applyReadiness}
            onPrepareApply={prepareApply}
            preparing={busy === "applyReadiness"}
          />
        )}
        {activeTab === "gates" && <GateTab inspection={inspection} plan={plan} gateResult={gateResult} />}
        {activeTab === "bitacora" && <HistoryTab run={run} runs={runs} />}
      </div>
    </main>
  );
}

function AgentNarrativePanel({ narrative }: { narrative: AgentNarrative }) {
  return (
    <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_380px]">
      <Card
        title="Lenguaje natural"
        description="Traduccion operativa del estado actual del agente."
        className={tonePanel(narrative.tone)}
      >
        <div className="grid gap-3">
          <div className="flex min-w-0 items-start gap-3">
            <span className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border bg-background text-muted-foreground">
              <Info className="h-4 w-4" />
            </span>
            <div className="min-w-0">
              <h2 className="text-base font-semibold leading-6 break-words">{narrative.headline}</h2>
              <p className="mt-1 text-sm leading-6 text-muted-foreground break-words">{narrative.body}</p>
            </div>
          </div>
          <div className="grid gap-2 md:grid-cols-2">
            <HelpText label="Siguiente accion" value={narrative.nextAction} />
            <HelpText label="Limite de gobernanza" value={narrative.guardrail} />
          </div>
        </div>
      </Card>

      <Card title="Autonomia gobernada" description="Poder local, con barandas de OneEpis.">
        <div className="grid gap-3">
          <HelpText label="Puede hacer ahora" value={narrative.power} />
          <List items={narrative.checklist.slice(0, 5)} empty="Sin pasos todavia." />
        </div>
      </Card>
    </section>
  );
}

function HelpText({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 rounded border border-border bg-background px-3 py-2">
      <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
        <HelpCircle className="h-3.5 w-3.5 shrink-0" />
        {label}
      </div>
      <p className="mt-1 text-sm leading-5 break-words">{value}</p>
    </div>
  );
}

function MicroProcessTab({
  steps,
  run,
  gateResult,
}: {
  steps: MicroStep[];
  run: AgentRun | null;
  gateResult: GateResult | null;
}) {
  return (
    <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_420px]">
      <Card title="Microproceso">
        <div className="grid gap-2">
          {steps.map((step) => (
            <div key={step.id} className="grid grid-cols-[minmax(0,120px)_minmax(88px,104px)_minmax(0,1fr)] items-start gap-3 rounded border border-border px-3 py-2 text-sm">
              <span className="font-medium break-words">{step.label}</span>
              <Badge tone={stepTone(step.status)}>{explainStatus(step.status)}</Badge>
              <span className="min-w-0 text-muted-foreground break-words">{step.detail}</span>
            </div>
          ))}
        </div>
      </Card>
      <Card title="Resultado">
        {run ? (
          <div className="grid gap-3">
            <div className="flex flex-wrap gap-2">
              <Badge tone={run.status === "completed" ? "success" : "warning"}>{explainStatus(run.status)}</Badge>
              <Badge>{run.modelUsed}</Badge>
              <Badge>{run.plan.recommendedGate}</Badge>
            </div>
            <p className="text-sm text-muted-foreground break-words">{run.objective}</p>
            {gateResult && (
              <p className="rounded border border-border bg-background px-3 py-2 text-xs text-muted-foreground break-words">
                {gateResult.command}: {explainStatus(gateResult.status)}
              </p>
            )}
          </div>
        ) : (
          <Empty text="Sin microproceso reciente." />
        )}
      </Card>
    </section>
  );
}

function ReportTab({ report }: { report: AgentRunReport | null }) {
  if (!report) {
    return <Empty text="Sin reporte. Presiona Reporte PR para ejecutar dry-run y preparar Markdown revisable." />;
  }
  return (
    <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_420px]">
      <Card title="Reporte PR" description="Markdown de microproceso cerrado para revision humana.">
        <div className="flex flex-wrap gap-2">
          <Badge tone={report.verdict === "ready_for_review" ? "success" : "warning"}>{report.verdict}</Badge>
          <Badge>{report.status}</Badge>
          <Badge>{report.recommendedGate}</Badge>
          <Badge>{report.modelUsed}</Badge>
        </div>
        <p className="mt-3 text-sm leading-6 break-words">{report.objective}</p>
        <pre className="mt-3 max-h-[520px] overflow-auto whitespace-pre-wrap break-words rounded border border-border bg-background p-3 text-xs text-muted-foreground">
          {report.markdown}
        </pre>
      </Card>

      <div className="grid gap-4">
        <Card title="Checklist" description="Condiciones que el reporte deja visibles.">
          <List items={report.checklist} empty="Sin checklist." />
        </Card>
        <Card title="Siguientes Pasos" description="Accion concreta antes de cambiar codigo.">
          <List items={report.nextActions} empty="Sin acciones pendientes." />
          <List items={report.warnings} tone="warning" empty="Sin warnings." />
        </Card>
      </div>
    </section>
  );
}

function RepoTab({ inspection, ollama }: { inspection: RepoInspection | null; ollama: OllamaStatus | null }) {
  const isMissingModel = (value?: string) => (value ? ollama?.missingPolicyModels.includes(value) : false);
  return (
    <section className="grid gap-4 lg:grid-cols-3">
      <Card title="Gobernanza" description="Reglas detectadas antes de planificar.">
        <PanelIcon icon={<ShieldCheck className="h-4 w-4" />} label={inspection?.projectName ?? "Sin repo"} />
        <List items={inspection?.detectedRules ?? []} empty="Sin reglas detectadas." />
      </Card>

      <Card title="Git" description="Estado de rama y limpieza del worktree objetivo.">
        <PanelIcon icon={<GitBranch className="h-4 w-4" />} label={inspection?.currentBranch || "Sin rama"} />
        <pre className="mt-3 max-h-44 overflow-auto whitespace-pre-wrap break-words rounded border border-border bg-background p-3 text-xs text-muted-foreground">
          {inspection?.statusText || "Sin estado Git."}
        </pre>
      </Card>

      <Card title="Ollama" description="Modelos locales usados para planificar y revisar.">
        <div className="grid gap-2 text-sm sm:grid-cols-2">
          <ModelSlot label="Gobernanza" value={ollama?.policy.governance} missing={isMissingModel(ollama?.policy.governance)} />
          <ModelSlot label="Codigo" value={ollama?.policy.primaryCode} missing={isMissingModel(ollama?.policy.primaryCode)} />
          <ModelSlot label="Rapido" value={ollama?.policy.fastCode} missing={isMissingModel(ollama?.policy.fastCode)} />
          <ModelSlot label="Fallback" value={ollama?.policy.fallback} missing={isMissingModel(ollama?.policy.fallback)} />
        </div>
        <p className="mt-3 text-xs text-muted-foreground break-words">{ollama?.models.length ?? 0} modelos en {ollama?.baseUrl ?? "Ollama"}</p>
      </Card>
    </section>
  );
}

function ReadinessTab({ readiness }: { readiness: DevelopmentReadiness | null }) {
  if (!readiness) return <Empty text="Sin diagnostico de preparacion. Ejecuta Inspeccionar." />;
  return (
    <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_420px]">
      <Card title="Preparacion OneEpis" description="Diagnostico para programacion asistida con modelos locales.">
        <div className="flex flex-wrap gap-2">
          <Badge tone={readinessTone(readiness.status)}>{readinessLabel(readiness.status)}</Badge>
          <Badge>{readiness.profile}</Badge>
          <Badge>{readiness.requiredGates.length} gates utiles</Badge>
        </div>
        <p className="mt-3 text-sm leading-6 break-words">{readiness.summary}</p>
        <HelpText label="Modelos locales" value={readiness.localModelSummary} />
        <div className="mt-3 grid gap-2">
          {readiness.checks.map((check) => (
            <div key={check.name} className="min-w-0 rounded border border-border px-3 py-2 text-sm">
              <div className="flex flex-wrap items-center gap-2">
                <Badge tone={check.status === "ready" ? "success" : "warning"}>{readinessLabel(check.status)}</Badge>
                <span className="font-medium break-words">{check.name}</span>
              </div>
              <p className="mt-1 text-muted-foreground break-words">{check.detail}</p>
              {check.status !== "ready" && <p className="mt-1 text-xs text-warning break-words">{check.action}</p>}
            </div>
          ))}
        </div>
      </Card>

      <div className="grid gap-4">
        <Card title="Siguiente Accion" description="Orden sugerido para cerrar el ciclo.">
          <List items={readiness.nextActions} empty="Sin acciones pendientes." />
          <List items={readiness.blockers} tone="warning" />
          <List items={readiness.warnings} tone="warning" />
        </Card>
        <Card title="Microciclos Sugeridos" description="Planes pequenos alineados con OneEpis.">
          <div className="grid gap-3">
            {readiness.suggestedMicrocycles.map((item) => (
              <div key={item.title} className="min-w-0 rounded border border-border bg-background px-3 py-2">
                <div className="flex flex-wrap gap-2">
                  <Badge tone={riskTone(item.riskLevel)}>Riesgo {item.riskLevel}</Badge>
                  {item.gates.map((gate) => (
                    <Badge key={gate}>{gate}</Badge>
                  ))}
                </div>
                <h3 className="mt-2 text-sm font-semibold break-words">{item.title}</h3>
                <p className="mt-1 text-sm text-muted-foreground break-words">{item.objective}</p>
                <p className="mt-1 text-xs text-muted-foreground break-words">{item.reason}</p>
              </div>
            ))}
          </div>
        </Card>
      </div>
    </section>
  );
}

function WorkPackageTab({ workPackage }: { workPackage: DevelopmentWorkPackage | null }) {
  if (!workPackage) return <Empty text="Sin paquete de trabajo. Elige un objetivo y presiona Paquete." />;
  return (
    <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_420px]">
      <Card title="Paquete De Trabajo" description="Plan de programacion local con pruebas y barandas.">
        <div className="flex flex-wrap gap-2">
          <Badge tone={workPackageStatusTone(workPackage.status)}>{workPackageStatusLabel(workPackage.status)}</Badge>
          <Badge>{workPackage.branchStrategy}</Badge>
          <Badge>{workPackage.gates.length} gates</Badge>
        </div>
        <h3 className="mt-3 text-base font-semibold break-words">{workPackage.title}</h3>
        <p className="mt-1 text-sm text-muted-foreground break-words">{workPackage.summary}</p>
        <HelpText label="Objetivo" value={workPackage.objective} />
        <div className="mt-3 grid gap-4 md:grid-cols-2">
          <div>
            <h4 className="text-sm font-semibold">Archivos a inspeccionar</h4>
            <List items={workPackage.filesToInspect} />
          </div>
          <div>
            <h4 className="text-sm font-semibold">Pasos de implementacion</h4>
            <List items={workPackage.implementationSteps} />
          </div>
        </div>
      </Card>

      <div className="grid gap-4">
        <Card title="Plan De Pruebas" description="Gates obligatorios para cerrar el microciclo.">
          <div className="grid gap-3">
            {workPackage.testPlan.map((test) => (
              <div key={test.gate} className="min-w-0 rounded border border-border bg-background px-3 py-2 text-sm">
                <div className="flex flex-wrap gap-2">
                  <Badge>{test.gate}</Badge>
                  <Badge tone={test.required ? "warning" : "neutral"}>{test.required ? "obligatorio" : "opcional"}</Badge>
                </div>
                <p className="mt-1 font-medium break-words">{test.command}</p>
                <p className="mt-1 text-muted-foreground break-words">{test.purpose}</p>
              </div>
            ))}
          </div>
        </Card>
        <Card title="Aceptacion Y Parada" description="Criterios para decidir si el ciclo termina.">
          <List items={workPackage.acceptanceCriteria} />
          <List items={workPackage.stopConditions} tone="warning" />
          <List items={workPackage.warnings} tone="warning" />
        </Card>
      </div>
    </section>
  );
}

function ContextPackTab({ contextPack }: { contextPack: DevelopmentContextPack | null }) {
  if (!contextPack) return <Empty text="Sin contexto local. Presiona Contexto para preparar memoria de trabajo sanitizada." />;
  return (
    <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_420px]">
      <Card title="Contexto Local" description="Memoria de trabajo acotada para modelos Ollama, sin escritura sobre OneEpis.">
        <div className="flex flex-wrap gap-2">
          <Badge tone={contextPackTone(contextPack.status)}>{contextPackStatusLabel(contextPack.status)}</Badge>
          <Badge>{contextPack.files.length} entradas</Badge>
          <Badge>{contextPack.totalBytes}/{contextPack.maxBytes} bytes</Badge>
        </div>
        <p className="mt-3 text-sm leading-6 break-words">{contextPack.summary}</p>
        <HelpText label="Objetivo" value={contextPack.objective} />
        <div className="mt-3 grid gap-3">
          {contextPack.files.map((file) => (
            <div key={`${file.path}-${file.kind}`} className="min-w-0 rounded border border-border bg-background px-3 py-2 text-sm">
              <div className="flex flex-wrap items-center gap-2">
                <Badge tone={file.kind === "file" ? "success" : file.kind === "skipped" ? "warning" : "neutral"}>{file.kind}</Badge>
                <span className="min-w-0 font-medium break-words">{file.path}</span>
              </div>
              <p className="mt-1 text-xs text-muted-foreground break-words">{file.summary}</p>
              <div className="mt-2 flex flex-wrap gap-2 text-xs text-muted-foreground">
                <span>{file.bytes} bytes</span>
                <span>{file.lines} lineas</span>
                {file.sha256 && <span className="min-w-0 break-all">sha {file.sha256.slice(0, 12)}</span>}
              </div>
              {file.excerpt && (
                <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap break-words rounded border border-border bg-surface p-3 text-xs text-muted-foreground">
                  {file.excerpt}
                </pre>
              )}
            </div>
          ))}
        </div>
      </Card>

      <div className="grid gap-4">
        <Card title="Notas Para El Modelo" description="Instrucciones de uso para el contexto local.">
          <List items={contextPack.promptNotes} />
          <List items={contextPack.gates.map((gate) => `Gate requerido: ${gate}`)} />
        </Card>
        <Card title="Omisiones Y Riesgos" description="Nada omitido se debe asumir como conocido por el agente.">
          <List items={contextPack.warnings} tone="warning" empty="Sin warnings del contexto." />
        </Card>
      </div>
    </section>
  );
}

function BriefTab({ brief }: { brief: DevelopmentBrief | null }) {
  if (!brief) return <Empty text="Sin brief local. Presiona Brief IA para preparar una orden de trabajo gobernada." />;
  return (
    <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_420px]">
      <Card title="Brief Local" description="Orden de trabajo para Ollama; no aplica cambios ni reemplaza PatchDraft.">
        <div className="flex flex-wrap gap-2">
          <Badge tone={contextPackTone(brief.status)}>{contextPackStatusLabel(brief.status)}</Badge>
          <Badge>{brief.modelUsed}</Badge>
          <Badge>{brief.contextFiles.length} entradas</Badge>
        </div>
        <p className="mt-3 text-sm leading-6 break-words">{brief.summary}</p>
        <HelpText label="Orden de trabajo" value={brief.workOrder} />
        {brief.proposal ? (
          <div className="mt-3 rounded border border-border bg-background px-3 py-2 text-sm">
            <div className="flex flex-wrap gap-2">
              <Badge tone={brief.proposal.status === "proposed" ? "success" : "warning"}>{brief.proposal.status}</Badge>
              <Badge>{brief.proposal.modelUsed}</Badge>
            </div>
            <p className="mt-2 font-medium break-words">{brief.proposal.summary}</p>
            <List items={brief.proposal.filesToChange.map((file) => `Archivo sugerido: ${file}`)} />
            <List items={brief.proposal.implementationNotes} />
            <List items={brief.proposal.risks} tone="warning" />
            <List items={brief.proposal.gates.map((gate) => `Gate sugerido: ${gate}`)} />
          </div>
        ) : (
          <Empty text="Sin propuesta del modelo local; el brief determinista queda disponible." />
        )}
        <div className="mt-3 grid gap-3 md:grid-cols-2">
          <div className="min-w-0">
            <h4 className="text-sm font-semibold">Contrato de respuesta</h4>
            <List items={brief.responseContract} />
          </div>
          <div className="min-w-0">
            <h4 className="text-sm font-semibold">Contexto usado</h4>
            <List items={brief.contextFiles} />
          </div>
        </div>
      </Card>

      <div className="grid gap-4">
        <Card title="Prompts" description="Texto enviado al modelo local cuando se pide propuesta.">
          <h4 className="text-xs font-semibold text-muted-foreground">Sistema</h4>
          <pre className="mt-2 max-h-44 overflow-auto whitespace-pre-wrap break-words rounded border border-border bg-background p-3 text-xs text-muted-foreground">
            {brief.systemPrompt}
          </pre>
          <h4 className="mt-3 text-xs font-semibold text-muted-foreground">Usuario</h4>
          <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap break-words rounded border border-border bg-background p-3 text-xs text-muted-foreground">
            {brief.userPrompt}
          </pre>
        </Card>
        <Card title="Parada Y Siguientes Pasos" description="Nada de esto concede apply.">
          <List items={brief.nextActions} />
          <List items={brief.stopConditions} tone="warning" />
          <List items={brief.warnings} tone="warning" />
        </Card>
      </div>
    </section>
  );
}

function DecisionTab({ decision }: { decision: ImplementationDecision | null }) {
  if (!decision) {
    return <Empty text="Sin decision. Presiona Decision para pedir propuesta local y traducirla a un paso listo para PatchDraft." />;
  }
  return (
    <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_420px]">
      <Card title="Decision De Implementacion" description="Traduce la propuesta local en una decision pequena, revisable y sin escritura.">
        <div className="flex flex-wrap gap-2">
          <Badge tone={decisionTone(decision.status)}>{implementationStatusLabel(decision.status)}</Badge>
          <Badge>{decision.modelUsed}</Badge>
          <Badge>{decision.sourceProposalStatus}</Badge>
        </div>
        <p className="mt-3 text-sm leading-6 break-words">{decision.summary}</p>
        <HelpText label="Objetivo" value={decision.objective} />
        <HelpText label="Intencion de PatchDraft" value={decision.patchIntent} />
        <div className="mt-3 grid gap-4 md:grid-cols-2">
          <div className="min-w-0">
            <h4 className="text-sm font-semibold">Archivos Seleccionados</h4>
            <List items={decision.selectedFiles} empty="Sin archivos aprobados." />
          </div>
          <div className="min-w-0">
            <h4 className="text-sm font-semibold">Gates Requeridos</h4>
            <List items={decision.requiredGates} empty="Sin gates aprobados." />
          </div>
        </div>
        <div className="mt-3">
          <h4 className="text-sm font-semibold">Pasos De Implementacion</h4>
          <List items={decision.implementationSteps} empty="Sin pasos revisables." />
        </div>
      </Card>

      <div className="grid gap-4">
        <Card title="Bloqueos Y Warnings" description="Nada pasa a PatchDraft si queda un bloqueo activo.">
          <List items={decision.blockers} tone="warning" empty="Sin bloqueos." />
          <List items={decision.warnings} tone="warning" empty="Sin warnings." />
        </Card>
        <Card title="Aceptacion Y Siguiente Accion" description="Cierre esperado antes de preparar diff.">
          <List items={decision.acceptanceCriteria} />
          <List items={decision.nextActions} empty="Sin acciones siguientes." />
        </Card>
      </div>
    </section>
  );
}

function PlanTab({ plan }: { plan: MicroPlan | null }) {
  if (!plan) return <Empty text="Sin microplan." />;
  return (
    <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_360px]">
      <Card title="Microplan" description="Decision pequena, verificable y ajustada a gobernanza.">
        <div className="flex flex-wrap gap-2">
          <Badge tone={plan.blocked ? "warning" : "success"}>{plan.blocked ? "Bloqueado" : "Revisable"}</Badge>
          <Badge tone={riskTone(plan.riskLevel)}>Riesgo {plan.riskLevel}</Badge>
          <Badge>{plan.modelUsed || "local_rules"}</Badge>
        </div>
        <p className="mt-3 text-sm leading-6 break-words">{plan.objective}</p>
        <List items={plan.steps} />
      </Card>
      <Card title="Superficies" description="Donde tocaria el cambio y como se validaria.">
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

function PatchTab({
  draft,
  review,
  readiness,
  onPrepareApply,
  preparing,
}: {
  draft: PatchDraft | null;
  review: PatchReview | null;
  readiness: ApplyReadiness | null;
  onPrepareApply: () => void;
  preparing: boolean;
}) {
  if (!draft) return <Empty text="Sin PatchDraft." />;
  return (
    <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_380px]">
      <Card title="PatchDraft" description="Diff revisable; no escribe en el repo objetivo por si solo.">
        <div className="flex flex-wrap gap-2">
          <Badge tone={draft.blocked ? "warning" : "success"}>{draft.blocked ? "Bloqueado" : "Revisable"}</Badge>
          <Badge>{draft.id}</Badge>
          <Badge>{draft.modelUsed || "local_rules"}</Badge>
        </div>
        <p className="mt-3 text-sm leading-6 break-words">{draft.summary}</p>
        <pre className="mt-3 max-h-[420px] overflow-auto whitespace-pre-wrap break-words rounded border border-border bg-background p-3 text-xs text-muted-foreground">
          {draft.unifiedDiff}
        </pre>
      </Card>
      <Card title="Revision" description="Checks deterministas antes de cualquier apply controlado.">
        {review ? (
          <>
            <div className="flex flex-wrap gap-2">
              <Badge tone={review.approved ? "success" : "warning"}>{review.approved ? "Aprobado" : "Bloqueado"}</Badge>
              <Badge>{review.confirmToken}</Badge>
            </div>
            <List items={review.checks.map((item) => `${item.name}: ${explainStatus(item.status)}`)} />
            <List items={review.blocks} tone="warning" />
          </>
        ) : (
          <Empty text="Sin revision." />
        )}
        <div className="mt-4 flex flex-wrap gap-2">
          <Button variant="secondary" onClick={onPrepareApply} disabled={preparing || !review}>
            <ShieldCheck className="mr-2 h-4 w-4" />
            {preparing ? "Prevalidando..." : "Prevalidar Apply"}
          </Button>
        </div>
        {readiness && (
          <div className="mt-4 rounded border border-border bg-background px-3 py-2 text-sm">
            <div className="flex flex-wrap gap-2">
              <Badge tone={applyReadinessTone(readiness.status)}>{explainStatus(readiness.status)}</Badge>
              <Badge>{readiness.targetBranch}</Badge>
              <Badge>{readiness.branchStrategy}</Badge>
            </div>
            <p className="mt-2 break-words text-muted-foreground">{readiness.summary}</p>
            <HelpText label="Token requerido" value={readiness.confirmToken} />
            <HelpText label="Rama actual" value={readiness.currentBranch} />
            <List items={readiness.checks.map((item) => `${item.name}: ${explainStatus(item.status)} - ${item.detail}`)} />
            <List items={readiness.blocks} tone="warning" empty="Sin bloqueos de apply." />
            <List items={readiness.nextActions} empty="Sin acciones siguientes." />
          </div>
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
      <Card title="Gates" description="Solo scripts declarados por el repo objetivo.">
        <List items={inspection?.declaredGates ?? []} empty="Sin gates declarados." />
        {plan?.recommendedGate && <p className="mt-3 text-sm text-muted-foreground break-words">Recomendado: {plan.recommendedGate}</p>}
      </Card>
      <Card title="Resultado" description="Salida sanitizada del gate ejecutado.">
        {gateResult ? (
          <>
            <div className="flex flex-wrap gap-2">
              <Badge tone={gateResult.status === "passed" ? "success" : gateResult.status === "failed" ? "danger" : "warning"}>
                {explainStatus(gateResult.status)}
              </Badge>
              <Badge>{gateResult.command}</Badge>
              <Badge>{gateResult.durationMs} ms</Badge>
            </div>
            <p className="mt-3 text-sm break-words">{gateResult.summary}</p>
            <pre className="mt-3 max-h-72 overflow-auto whitespace-pre-wrap break-words rounded border border-border bg-background p-3 text-xs text-muted-foreground">
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
      <Card title="Ultimo ciclo" description="Estados del microproceso cerrado.">
        {run ? (
          <>
            <div className="flex flex-wrap gap-2">
              <Badge tone={run.status === "completed" ? "success" : "warning"}>{explainStatus(run.status)}</Badge>
              <Badge>{run.mode}</Badge>
              <Badge>{run.persistence}</Badge>
            </div>
            <List items={run.steps.map((step) => `${step.state}: ${explainStatus(step.status)} - ${step.summary}`)} />
          </>
        ) : (
          <Empty text="Sin ciclo reciente." />
        )}
      </Card>
      <Card title="Runs" description="Historial local cuando la persistencia esta configurada.">
        {runs.length > 0 ? (
          <div className="grid gap-2">
            {runs.map((item) => (
              <div key={item.id} className="min-w-0 rounded border border-border px-3 py-2 text-xs">
                <div className="flex items-center justify-between gap-2">
                  <span className="font-medium break-words">{explainStatus(item.status)}</span>
                  <span className="shrink-0 text-muted-foreground">{item.startedAt}</span>
                </div>
                <p className="mt-1 text-muted-foreground break-words">{item.summary}</p>
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
    <div className="min-w-0 rounded border border-border p-2">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-1 flex min-w-0 items-start gap-2 text-xs font-medium">
        {missing ? <Activity className="h-3.5 w-3.5 shrink-0 text-warning" /> : <CheckCircle2 className="h-3.5 w-3.5 shrink-0 text-success" />}
        <span className="min-w-0 break-words">{value ?? "sin modelo"}</span>
      </div>
    </div>
  );
}

function PanelIcon({ icon, label }: { icon: ReactNode; label: string }) {
  return (
    <div className="flex min-w-0 items-center gap-2 text-sm font-medium">
      <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-border bg-background text-muted-foreground">{icon}</span>
      <span className="min-w-0 break-words">{label}</span>
    </div>
  );
}

function List({ items, empty, tone = "neutral" }: { items: string[]; empty?: string; tone?: "neutral" | "warning" }) {
  if (items.length === 0) return empty ? <p className="mt-3 text-sm text-muted-foreground break-words">{empty}</p> : null;
  return (
    <ul className="mt-3 grid gap-2 text-sm">
      {items.map((item, index) => (
        <li key={`${item}-${index}`} className={cn("min-w-0 leading-5 break-words", tone === "warning" ? "text-warning" : "text-muted-foreground")}>
          {item}
        </li>
      ))}
    </ul>
  );
}

function Empty({ text }: { text: string }) {
  return <p className="rounded border border-dashed border-border p-4 text-sm text-muted-foreground break-words">{text}</p>;
}

function tabLabel(tab: Tab) {
  const labels: Record<Tab, string> = {
    repo: "Repo",
    preparacion: "Preparacion",
    paquete: "Paquete",
    contexto: "Contexto",
    brief: "Brief",
    decision: "Decision",
    microproceso: "Microproceso",
    reporte: "Reporte",
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

function applyReadinessTone(status: string) {
  if (status === "ready_to_apply") return "success";
  if (status === "ready_for_confirmation") return "warning";
  if (status === "blocked") return "danger";
  return "neutral";
}

function decisionTone(status: string) {
  if (status === "ready_to_draft") return "success";
  if (status === "needs_model_proposal") return "warning";
  if (status === "blocked") return "danger";
  return "neutral";
}

function implementationStatusLabel(status: string) {
  const labels: Record<string, string> = {
    ready_to_draft: "listo para PatchDraft",
    needs_model_proposal: "requiere propuesta local",
    blocked: "bloqueado",
  };
  return labels[status] ?? status;
}

function readinessTone(status?: string) {
  if (status === "ready") return "success";
  if (status === "blocked") return "danger";
  if (status === "attention") return "warning";
  return "neutral";
}

function readinessLabel(status?: string) {
  const labels: Record<string, string> = {
    ready: "listo",
    attention: "atencion",
    blocked: "bloqueado",
  };
  return status ? labels[status] ?? status : "pendiente";
}

function workPackageStatusTone(status: string) {
  if (status === "ready_to_draft") return "success";
  if (status === "blocked") return "danger";
  return "warning";
}

function workPackageStatusLabel(status: string) {
  const labels: Record<string, string> = {
    ready_to_draft: "listo para PatchDraft",
    blocked: "bloqueado",
    needs_gate: "requiere gate",
  };
  return labels[status] ?? status;
}

function contextPackTone(status: string) {
  if (status === "ready") return "success";
  if (status === "blocked") return "danger";
  return "warning";
}

function contextPackStatusLabel(status: string) {
  const labels: Record<string, string> = {
    ready: "listo",
    partial: "parcial",
    blocked: "bloqueado",
  };
  return labels[status] ?? status;
}

function tonePanel(tone: NarrativeTone) {
  const tones: Record<NarrativeTone, string> = {
    neutral: "",
    success: "border-success/40",
    warning: "border-warning/50",
    danger: "border-danger/50",
  };
  return tones[tone];
}

function stepTone(status: MicroStepStatus) {
  if (status === "completed") return "success";
  if (status === "failed") return "danger";
  if (status === "blocked" || status === "running") return "warning";
  return "neutral";
}

function gateStatus(status: string): MicroStepStatus {
  if (status === "passed") return "completed";
  if (status === "failed") return "failed";
  return "blocked";
}

function selectSmallGate(gates: string[], recommendedGate: string) {
  for (const gate of ["check:size", "check:screens", recommendedGate, "test", "build", "check"]) {
    if (gate && gates.includes(gate)) return gate;
  }
  return "";
}

export default App;
