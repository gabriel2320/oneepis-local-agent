import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { Activity, Bot, BrainCircuit, CheckCircle2, FileText, GitBranch, Play, ShieldCheck } from "lucide-react";
import { getOllamaStatus, inspectRepository, planMicrocycle, runOneEpisAutopilot } from "./lib/api";
import type { AgentRun, MicroPlan, OllamaStatus, RepoInspection } from "./lib/types";
import { Badge } from "./components/ui/badge";
import { Button } from "./components/ui/button";
import { Card } from "./components/ui/card";

const defaultRepo = "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis";

function App() {
  const [repoPath, setRepoPath] = useState(defaultRepo);
  const [objective, setObjective] = useState("Auditar el repo y proponer el microciclo mas pequeño gobernado.");
  const [inspection, setInspection] = useState<RepoInspection | null>(null);
  const [ollama, setOllama] = useState<OllamaStatus | null>(null);
  const [plan, setPlan] = useState<MicroPlan | null>(null);
  const [run, setRun] = useState<AgentRun | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const statusTone = useMemo(() => {
    if (!inspection) return "neutral";
    if (inspection.blocks.length > 0) return "warning";
    return "success";
  }, [inspection]);
  const isMissingModel = (value?: string) => (value ? ollama?.missingPolicyModels.includes(value) : false);

  async function loadAll() {
    setBusy("inspect");
    setError(null);
    try {
      const [repo, ai] = await Promise.all([inspectRepository(repoPath), getOllamaStatus()]);
      setInspection(repo);
      setOllama(ai);
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
      setPlan(await planMicrocycle(repoPath, objective));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  async function runAutopilot() {
    setBusy("run");
    setError(null);
    try {
      const result = await runOneEpisAutopilot(repoPath, objective, 1);
      setRun(result);
      setRepoPath(result.repoPath);
      setInspection(await inspectRepository(result.repoPath));
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
        <header className="flex flex-col gap-3 border-b border-border pb-5 md:flex-row md:items-end md:justify-between">
          <div>
            <div className="flex items-center gap-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
              <Bot className="h-4 w-4" />
              Laboratorio local externo
            </div>
            <h1 className="mt-2 text-2xl font-semibold">OneEpis Local Agent</h1>
            <p className="mt-1 max-w-3xl text-sm leading-6 text-muted-foreground">
              Clona OneEpis, lo audita con IA local cuando esta disponible, elige el siguiente trabajo local y ejecuta
              un gate verificable con acciones tipadas.
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            <Badge tone={ollama?.available ? "success" : "warning"}>{ollama?.available ? "Ollama activo" : "Ollama pendiente"}</Badge>
            <Badge tone={statusTone}>{inspection?.isOneEpis ? "Adaptador OneEpis" : "Repo generico"}</Badge>
            <Badge>Sin push automatico</Badge>
          </div>
        </header>

        {error && (
          <div className="rounded-md border border-danger/30 bg-danger/10 px-4 py-3 text-sm text-danger">{error}</div>
        )}

        <section className="grid gap-4 lg:grid-cols-[minmax(0,1.1fr)_minmax(320px,0.9fr)]">
          <Card
            title="Proyecto"
            description="Workspace local de OneEpis y objetivo de la siguiente pasada."
            actions={
              <Button variant="secondary" onClick={loadAll} disabled={busy !== null}>
                {busy === "inspect" ? "Inspeccionando..." : "Inspeccionar"}
              </Button>
            }
          >
            <div className="grid gap-3">
              <label className="grid gap-1 text-sm">
                <span className="text-xs font-medium text-muted-foreground">Workspace o repo OneEpis</span>
                <input
                  value={repoPath}
                  onChange={(event) => setRepoPath(event.target.value)}
                  className="h-10 rounded-md border border-border bg-background px-3 text-sm outline-none focus:border-primary"
                />
              </label>
              <label className="grid gap-1 text-sm">
                <span className="text-xs font-medium text-muted-foreground">Objetivo</span>
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
                  {busy === "plan" ? "Planificando..." : "Planificar"}
                </Button>
                <Button variant="secondary" onClick={runAutopilot} disabled={busy !== null}>
                  <Play className="mr-2 h-4 w-4" />
                  {busy === "run" ? "Ejecutando..." : "Ejecutar local"}
                </Button>
              </div>
            </div>
          </Card>

          <Card title="Ollama" description={ollama?.message ?? "Consultando modelos locales."}>
            <div className="grid gap-3">
              <div className="grid grid-cols-2 gap-2 text-sm">
                <ModelSlot label="Codigo" value={ollama?.policy.primaryCode} missing={isMissingModel(ollama?.policy.primaryCode)} />
                <ModelSlot label="Rapido" value={ollama?.policy.fastCode} missing={isMissingModel(ollama?.policy.fastCode)} />
                <ModelSlot label="Gobierno" value={ollama?.policy.governance} missing={isMissingModel(ollama?.policy.governance)} />
                <ModelSlot label="Fallback" value={ollama?.policy.fallback} missing={isMissingModel(ollama?.policy.fallback)} />
              </div>
              <div className="text-xs text-muted-foreground">{ollama?.models.length ?? 0} modelos detectados en {ollama?.baseUrl ?? "Ollama"}</div>
            </div>
          </Card>
        </section>

        <section className="grid gap-4 lg:grid-cols-3">
          <Card title="Gobernanza" description="Reglas detectadas y bloqueos activos.">
            <PanelIcon icon={<ShieldCheck className="h-4 w-4" />} label={inspection?.projectName ?? "Sin repo"} />
            <List items={inspection?.detectedRules ?? []} empty="Sin reglas detectadas." />
            {inspection && inspection.blocks.length > 0 && <List items={inspection.blocks} tone="warning" />}
          </Card>

          <Card title="Git y gates" description="Estado del repo objetivo.">
            <PanelIcon icon={<GitBranch className="h-4 w-4" />} label={inspection?.currentBranch || "Sin rama"} />
            <pre className="mt-3 max-h-32 overflow-auto rounded border border-border bg-background p-3 text-xs text-muted-foreground">
              {inspection?.statusText || "Sin estado Git."}
            </pre>
            <List items={inspection?.declaredGates ?? []} empty="Sin gates declarados." />
          </Card>

          <Card title="Docs leidos" description="Fuentes de gobierno registradas por hash.">
            <PanelIcon icon={<FileText className="h-4 w-4" />} label={`${inspection?.governanceDocuments.filter((doc) => doc.present).length ?? 0} documentos`} />
            <div className="mt-3 grid gap-2">
              {inspection?.governanceDocuments.map((doc) => (
                <div key={doc.path} className="rounded border border-border px-3 py-2 text-xs">
                  <div className="font-medium">{doc.path}</div>
                  <div className="mt-1 text-muted-foreground">{doc.present ? `${doc.bytes} bytes · ${doc.sha256.slice(0, 12)}` : "No encontrado"}</div>
                </div>
              ))}
            </div>
          </Card>
        </section>

        <section className="grid gap-4 lg:grid-cols-2">
          <Card title="Microplan" description="Propuesta compacta, sin aplicar cambios.">
            {plan ? (
              <div className="grid gap-3">
                <Badge tone={plan.blocked ? "warning" : "success"}>{plan.blocked ? "Bloqueado" : "Listo para revision"}</Badge>
                <p className="text-sm leading-6">{plan.objective}</p>
                <p className="text-xs text-muted-foreground">Gate recomendado: {plan.recommendedGate || "sin gate"}</p>
                <List items={plan.steps} />
                <List items={plan.warnings} tone="warning" />
              </div>
            ) : (
              <Empty text="Crea un microplan para ver pasos, advertencias y gate recomendado." />
            )}
          </Card>

          <Card title="Ultimo ciclo" description="Autopilot local registrado por la maquina de estados.">
            {run ? (
              <div className="grid gap-3">
                <div className="flex flex-wrap gap-2">
                  <Badge tone={run.status === "completed" ? "success" : "warning"}>{run.status}</Badge>
                  <Badge>{run.mode}</Badge>
                  <Badge>{run.persistence}</Badge>
                </div>
                {run.checkout && (
                  <p className="text-xs text-muted-foreground">
                    Checkout: {run.checkout.action} en {run.checkout.repoPath}
                  </p>
                )}
                {run.nextWork && (
                  <div className="rounded border border-border px-3 py-2 text-sm">
                    <div className="font-medium">{run.nextWork.title}</div>
                    <div className="mt-1 text-xs text-muted-foreground">{run.nextWork.rationale}</div>
                    <div className="mt-2 text-xs text-muted-foreground">{run.nextWork.command.join(" ")}</div>
                  </div>
                )}
                <List items={run.steps.map((step) => `${step.state}: ${step.summary}`)} />
                <List items={run.lessons} />
              </div>
            ) : (
              <Empty text="Ejecuta el autopilot local para clonar, auditar y correr el siguiente gate." />
            )}
          </Card>
        </section>
      </div>
    </main>
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

export default App;
