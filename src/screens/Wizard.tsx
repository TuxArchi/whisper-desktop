import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ipc, isTauri } from "../lib/ipc";
import { formatBytes, formatReason } from "../lib/i18n";
import type {
  AppState,
  CatalogStatus,
  HardwareReport,
  InstallEvent,
  InstallPlan,
  IntegrationReport,
  VerifyView,
} from "../lib/types";

type Step = "welcome" | "detecting" | "recommend" | "installing" | "done";

const BACKEND_LABEL: Record<string, string> = {
  cpu: "CPU",
  cuda: "NVIDIA CUDA",
  vulkan: "Vulkan",
  rocm: "AMD ROCm",
};

interface StepProgress {
  label: string;
  done: number;
  total: number;
}

export default function Wizard({
  state,
  onInstalled,
}: {
  state: AppState;
  onInstalled: () => void;
}) {
  const [step, setStep] = useState<Step>("welcome");
  const [report, setReport] = useState<HardwareReport | null>(null);
  const [plan, setPlan] = useState<InstallPlan | null>(null);
  const [catalog, setCatalog] = useState<CatalogStatus | null>(null);
  const [selectedModel, setSelectedModel] = useState<string>("");
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<Record<string, StepProgress>>({});
  const [currentStep, setCurrentStep] = useState<string>("");
  const [installError, setInstallError] = useState<string | null>(null);
  const [verification, setVerification] = useState<VerifyView | null>(null);
  const [integration, setIntegration] = useState<IntegrationReport | null>(null);

  // The progress event carries no step id: attach it to the active one.
  const currentStepRef = useRef<string>("");
  currentStepRef.current = currentStep;

  useEffect(() => {
    if (step !== "recommend") return;
    ipc<CatalogStatus>("catalog_status")
      .then((value) => {
        setCatalog(value);
        setSelectedModel((current) => current || value.recommended);
      })
      .catch(() => setCatalog(null));
  }, [step]);

  const detect = useCallback(async () => {
    setStep("detecting");
    setError(null);
    try {
      const hardware = await ipc<HardwareReport>("detect_hardware");
      const decision = await ipc<InstallPlan>("build_plan");
      setReport(hardware);
      setPlan(decision);
      setStep("recommend");
    } catch (cause) {
      setError(String(cause));
      setStep("welcome");
    }
  }, []);

  const gpuVram = useCallback(() => {
    const vram = (report?.gpus ?? [])
      .map((gpu) => gpu.vramMb ?? 0)
      .reduce((max, value) => Math.max(max, value), 0);
    return vram;
  }, [report]);

  const selectedVram = useCallback(() => {
    const model = catalog?.models.find((item) => item.id === selectedModel);
    return model?.vramMb ?? 0;
  }, [catalog, selectedModel]);

  const modelOverBudget = useCallback(() => {
    const vram = gpuVram();
    return vram > 0 && selectedVram() > vram;
  }, [gpuVram, selectedVram]);

  // `selectedModel` must be a dependency: without it the callback keeps the
  // value from the first render and always installs the recommended model.
  const startInstall = useCallback(async () => {
    setStep("installing");
    setInstallError(null);
    setProgress({});
    setCurrentStep("");

    let unlisten: (() => void) | undefined;
    if (isTauri()) {
      unlisten = await listen<InstallEvent>("install:event", (event) => {
        const payload = event.payload;
        if (payload.kind === "step") {
          setCurrentStep(payload.id);
          setProgress((prev) => ({
            ...prev,
            [payload.id]: { label: payload.label, done: 0, total: 0 },
          }));
        } else if (payload.kind === "progress") {
          setProgress((prev) => {
            const key = currentStepRef.current;
            if (!key || !prev[key]) return prev;
            return {
              ...prev,
              [key]: { ...prev[key], done: payload.done, total: payload.total },
            };
          });
        }
      });
    }

    try {
      await ipc("start_install", { model: selectedModel || null });

      // Shortcuts, PATH and "Open with". Best effort: a failure here must not
      // invalidate an otherwise correct installation.
      try {
        setIntegration(await ipc<IntegrationReport>("integrate_system"));
      } catch {
        setIntegration(null);
      }

      // The install only counts when the engine actually runs. This launches
      // whisper-cli on a generated clip and reads which backend it loaded.
      setCurrentStep("verify");
      setProgress((prev) => ({
        ...prev,
        verify: { label: "Comprobando que la GPU responde", done: 0, total: 0 },
      }));
      try {
        setVerification(await ipc<VerifyView>("verify_installation"));
      } catch {
        setVerification(null);
      }
      setStep("done");
    } catch (cause) {
      setInstallError(String(cause));
      setStep("recommend");
    } finally {
      unlisten?.();
    }
  }, [selectedModel]);

  return (
    <div className="wizard">
      <header className="wizard-head">
        <div className="brand">
          <span className="dot" />
          Whisper Desktop
        </div>
        <span className="muted small">v{state.appVersion}</span>
      </header>

      {step === "welcome" && (
        <section className="card hero">
          <h1>Vamos a preparar tu equipo</h1>
          <p className="muted">
            Voy a leer tu CPU, memoria y tarjeta gráfica para elegir el motor más rápido que
            tu equipo pueda usar de verdad, y el modelo con mejor calidad sin quedarse corto
            de memoria.
          </p>
          <ul className="bullets">
            <li>Sin instalar Python ni configurar CUDA a mano.</li>
            <li>NVIDIA, AMD e Intel: cada una por su mejor camino.</li>
            <li>Todo se queda en tu equipo. Nada se envía a internet.</li>
          </ul>
          {error && <div className="banner danger">{error}</div>}
          <button className="primary" onClick={detect}>
            Analizar mi equipo
          </button>
        </section>
      )}

      {step === "detecting" && (
        <section className="card hero centered">
          <div className="spinner" />
          <h2>Leyendo el hardware…</h2>
          <p className="muted">CPU, memoria, GPU, driver y espacio libre.</p>
        </section>
      )}

      {step === "recommend" && report && plan && (
        <>
          <section className="card">
            <h2>Tu equipo</h2>
            <div className="grid">
              <Info label="Sistema" value={osLabel(report)} />
              <Info
                label="CPU"
                value={`${report.cpu.model ?? "CPU"} · ${report.cpu.physicalCores} núcleos`}
              />
              <Info label="Memoria" value={`${(report.ramMb / 1024).toFixed(0)} GB`} />
            </div>

            {report.gpus.length === 0 && (
              <p className="muted">No se detectó GPU compatible: se usará el procesador.</p>
            )}

            {report.gpus.map((gpu) => (
              <div className="gpu" key={gpu.pciId ?? gpu.name}>
                <div>
                  <strong>{gpu.name ?? "GPU desconocida"}</strong>
                  <span className="muted small"> · {gpu.vendor}</span>
                </div>
                <div className="muted small">
                  {gpu.vramMb ? `${gpu.vramMb} MB VRAM` : "VRAM desconocida"} · {gpu.family}
                </div>
              </div>
            ))}

            {report.vulkan && (
              <p className="muted small">
                Vulkan {report.vulkan.apiVersion ?? "?"} · {report.vulkan.deviceCount}{" "}
                dispositivo(s)
              </p>
            )}
          </section>

          <section className="card highlight">
            <h2>Recomendación</h2>
            <div className="plan">
              <div>
                <span className="label">Motor</span>
                <strong>{BACKEND_LABEL[plan.backend] ?? plan.backend}</strong>
              </div>
              <div>
                <span className="label">Modelo</span>
                <strong>
                  {selectedModel || plan.model} · {plan.variant}
                </strong>
              </div>
              <div>
                <span className="label">Hilos</span>
                <strong>{plan.threads}</strong>
              </div>
            </div>

            <ul className="reasons">
              {plan.reasons.map((reason) => (
                <li key={reason.code + JSON.stringify(reason.args)}>{formatReason(reason)}</li>
              ))}
            </ul>

            {plan.fallbacks.length > 0 && (
              <p className="muted small">
                Si la GPU no responde, se probará:{" "}
                {plan.fallbacks.map((b) => BACKEND_LABEL[b] ?? b).join(" → ")}
              </p>
            )}

            {catalog && catalog.models.length > 0 && (
              <div className="models">
                <span className="label">Modelo a descargar</span>
                <div className="model-list">
                  {catalog.models.map((model) => {
                    const active = (selectedModel || plan.model) === model.id;
                    return (
                      <label key={model.id} className={active ? "model active" : "model"}>
                        <input
                          type="radio"
                          name="model"
                          checked={active}
                          onChange={() => setSelectedModel(model.id)}
                        />
                        <span className="model-name">{model.id}</span>
                        {model.id === catalog.recommended && (
                          <span className="badge">recomendado</span>
                        )}
                        <span className="model-size">
                          {formatBytes(model.size)} · ~{model.vramMb} MB VRAM
                        </span>
                      </label>
                    );
                  })}
                </div>
                {modelOverBudget() && (
                  <p className="warn-line">
                    Ese modelo pide ~{selectedVram()} MB de VRAM y tienes {gpuVram()} MB. Puede
                    que termine usando la CPU, que es mucho más lento.
                  </p>
                )}
              </div>
            )}
          </section>

          {plan.warnings.length > 0 && (
            <section className="card warn">
              <h3>Avisos</h3>
              <ul>
                {plan.warnings.map((warning) => (
                  <li key={warning.code + JSON.stringify(warning.args)}>
                    {formatReason(warning)}
                  </li>
                ))}
              </ul>
            </section>
          )}

          {catalog && catalog.notes.length > 0 && (
            <section className="card warn">
              <h3>El catálogo publica algo distinto</h3>
              <ul>
                {catalog.notes.map((note) => (
                  <li key={note.code + JSON.stringify(note.args)}>{formatReason(note)}</li>
                ))}
              </ul>
            </section>
          )}

          {installError && <div className="banner danger pre">{installError}</div>}

          <section className="actions">
            <button className="ghost" onClick={() => setStep("welcome")}>
              Volver
            </button>
            <button className="primary" disabled={!catalog?.ok} onClick={startInstall}>
              Descargar e instalar
            </button>
          </section>

          {catalog && !catalog.ok && (
            <section className="card warn">
              <h3>El catálogo de descargas aún no está publicado</h3>
              <p className="muted small">
                La aplicación ya sabe qué descargar, pero los binarios del motor todavía no
                existen en Releases. Los hashes de seguridad deben ser reales antes de
                permitir una instalación.
              </p>
            </section>
          )}
        </>
      )}

      {step === "installing" && (
        <section className="card">
          <h2>Instalando</h2>
          <ul className="steps">
            {Object.entries(progress).map(([id, item]) => (
              <li key={id} className={id === currentStep ? "active" : ""}>
                <div className="step-head">
                  <span>{item.label}</span>
                  <span className="muted small">
                    {item.total > 0
                      ? `${formatBytes(item.done)} / ${formatBytes(item.total)}`
                      : ""}
                  </span>
                </div>
                <div className="bar">
                  <div
                    className="bar-fill"
                    style={{
                      width:
                        item.total > 0
                          ? `${Math.min(100, (item.done / item.total) * 100)}%`
                          : "0%",
                    }}
                  />
                </div>
              </li>
            ))}
          </ul>
          {Object.keys(progress).length === 0 && <p className="muted">Preparando…</p>}
        </section>
      )}

      {step === "done" && (
        <section className="card hero centered">
          <h1>Instalado y comprobado</h1>

          {verification?.working ? (
            <p className="muted">
              Backend confirmado:{" "}
              <strong>{BACKEND_LABEL[verification.working] ?? verification.working}</strong>
              {verification.rtf > 0 && (
                <>
                  {" "}
                  · {verification.rtf.toFixed(1)}× tiempo real
                </>
              )}
            </p>
          ) : (
            <p className="muted">
              La instalación terminó, pero no se pudo confirmar la GPU. Se usará el siguiente
              backend de la lista automáticamente.
            </p>
          )}

          {verification && verification.tried.length > 1 && (
            <p className="muted small">
              Se probó: {verification.tried.map((b) => BACKEND_LABEL[b] ?? b).join(" → ")}
            </p>
          )}

          {integration && integration.errors.length > 0 && (
            <div className="banner warn left">
              No se pudieron crear todos los accesos directos:
              <ul>
                {integration.errors.map((error) => (
                  <li key={error}>{error}</li>
                ))}
              </ul>
              Puedes reintentarlo desde Ajustes.
            </div>
          )}

          {integration && integration.errors.length === 0 && integration.actions.length > 0 && (
            <p className="muted small">
              {integration.actions.length} elementos integrados en el sistema (menú de
              aplicaciones y comando en PATH).
            </p>
          )}

          <button className="primary" onClick={onInstalled}>
            Continuar
          </button>
        </section>
      )}
    </div>
  );
}

function Info({ label, value }: { label: string; value: string }) {
  return (
    <div className="info">
      <span className="label">{label}</span>
      <span>{value}</span>
    </div>
  );
}

function osLabel(report: HardwareReport): string {
  const version = report.os.version ?? report.os.family;
  return `${version} (${report.os.arch})`;
}
