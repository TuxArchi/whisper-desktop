import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { ipc, isTauri } from "../lib/ipc";
import type {
  AppState,
  DataInfo,
  HistoryEntry,
  IntegrationReport,
  JobEvent,
  MediaFile,
  StudioState,
} from "../lib/types";

const LANGUAGES: Array<[string, string]> = [
  ["auto", "Detectar automáticamente"],
  ["es", "Español"],
  ["en", "Inglés"],
  ["fr", "Francés"],
  ["de", "Alemán"],
  ["pt", "Portugués"],
  ["it", "Italiano"],
  ["ja", "Japonés"],
  ["zh", "Chino"],
  ["ru", "Ruso"],
];

const AUDIO_EXTENSIONS = [
  "mp3",
  "wav",
  "m4a",
  "flac",
  "ogg",
  "opus",
  "mp4",
  "mkv",
  "mov",
  "webm",
];

interface JobRow {
  index: number;
  source: string;
  percent: number;
  status: "running" | "ok" | "error";
  outputs: string[];
  error: string | null;
}

export default function Studio({ state }: { state: AppState }) {
  const [studio, setStudio] = useState<StudioState | null>(null);
  const [files, setFiles] = useState<MediaFile[]>([]);
  const [preserveTree, setPreserveTree] = useState(true);
  const [task, setTask] = useState("transcribe");
  const [language, setLanguage] = useState("auto");
  const [model, setModel] = useState("");
  const [formats, setFormats] = useState<string[]>(["txt", "srt"]);
  const [pattern, setPattern] = useState("{nombre}_{fecha}");
  const [outputDir, setOutputDir] = useState("");
  const [rows, setRows] = useState<JobRow[]>([]);
  const [running, setRunning] = useState(false);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [preview, setPreview] = useState<string[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [dataInfo, setDataInfo] = useState<DataInfo | null>(null);
  const [keepCopy, setKeepCopy] = useState(false);
  const [dataMessage, setDataMessage] = useState<string | null>(null);
  const [dataProgress, setDataProgress] = useState<number | null>(null);
  const [repairMessage, setRepairMessage] = useState<string | null>(null);
  const [repairing, setRepairing] = useState(false);

  const addFiles = useCallback((paths: string[]) => {
    const usable = paths
      .filter((path) =>
        AUDIO_EXTENSIONS.some((extension) =>
          path.toLowerCase().endsWith(`.${extension}`),
        ),
      )
      .map((path) => ({ path, relative: "" }));
    setFiles((prev) => {
      const seen = new Set(prev.map((file) => file.path));
      return [...prev, ...usable.filter((file) => !seen.has(file.path))];
    });
    if (usable.length !== paths.length) {
      setMessage("Algunos archivos se ignoraron: no parecen audio o vídeo.");
    }
  }, []);

  useEffect(() => {
    ipc<StudioState>("studio_state")
      .then((value) => {
        setStudio(value);
        setModel(value.defaultModel);
        setLanguage(value.language);
        setOutputDir(value.outputDir);
        setPattern(value.pattern);
        setFormats(value.formats.length > 0 ? value.formats : ["txt"]);
        setTask(value.task === "translateEn" ? "translateEn" : value.task);
      })
      .catch((cause: unknown) => setMessage(String(cause)));

    ipc<HistoryEntry[]>("list_history", { limit: 20 })
      .then(setHistory)
      .catch(() => undefined);

    // "Open with Whisper Desktop": files passed on the command line.
    ipc<DataInfo>("data_info")
      .then(setDataInfo)
      .catch(() => undefined);

    ipc<string[]>("initial_files")
      .then((paths) => {
        if (paths.length > 0) addFiles(paths);
      })
      .catch(() => undefined);
  }, [addFiles]);

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "drop") {
          addFiles(event.payload.paths);
        }
      })
      .then((value) => {
        unlisten = value;
      })
      .catch(() => undefined);
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    listen<[number, number]>("data:progress", (event) => {
      const [done, total] = event.payload;
      setDataProgress(total > 0 ? Math.round((done / total) * 100) : 0);
    })
      .then((value) => {
        unlisten = value;
      })
      .catch(() => undefined);
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    listen<string[]>("open-files", (event) => addFiles(event.payload))
      .then((value) => {
        unlisten = value;
      })
      .catch(() => undefined);
    return () => unlisten?.();
  }, [addFiles]);

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    listen<JobEvent>("job:event", (event) => {
      const payload = event.payload;
      if (payload.kind === "started") {
        setRows((prev) => {
          const next = [...prev];
          next[payload.index] = {
            index: payload.index,
            source: payload.source,
            percent: 0,
            status: "running",
            outputs: [],
            error: null,
          };
          return next;
        });
      } else if (payload.kind === "progress") {
        setRows((prev) =>
          prev.map((row) =>
            row && row.index === payload.index ? { ...row, percent: payload.percent } : row,
          ),
        );
      } else if (payload.kind === "finished") {
        setRows((prev) =>
          prev.map((row) =>
            row && row.index === payload.index
              ? {
                  ...row,
                  percent: 100,
                  status: payload.ok ? "ok" : "error",
                  outputs: payload.outputs,
                  error: payload.error,
                }
              : row,
          ),
        );
      } else if (payload.kind === "done") {
        setRunning(false);
        ipc<HistoryEntry[]>("list_history", { limit: 20 })
          .then(setHistory)
          .catch(() => undefined);
      }
    })
      .then((value) => {
        unlisten = value;
      })
      .catch(() => undefined);
    return () => unlisten?.();
  }, []);


  const pickFiles = useCallback(async () => {
    if (!isTauri()) {
      setMessage("Selección de archivos disponible solo en la app de escritorio.");
      return;
    }
    const selected = await open({
      multiple: true,
      filters: [{ name: "Audio y vídeo", extensions: AUDIO_EXTENSIONS }],
    });
    if (Array.isArray(selected)) addFiles(selected);
    else if (typeof selected === "string") addFiles([selected]);
  }, [addFiles]);

  const pickFolder = useCallback(async () => {
    if (!isTauri()) return;
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") setOutputDir(selected);
  }, []);

  // Picking a folder scans it in Rust: the webview cannot walk the filesystem.
  const pickInputFolder = useCallback(async () => {
    if (!isTauri()) {
      setMessage("Escanear carpetas solo está disponible en la app de escritorio.");
      return;
    }
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== "string") return;
    try {
      const found = await ipc<MediaFile[]>("scan_folder", { path: selected });
      if (found.length === 0) {
        setMessage("No se encontró audio o vídeo en esa carpeta.");
        return;
      }
      setFiles((prev) => {
        const seen = new Set(prev.map((file) => file.path));
        return [...prev, ...found.filter((file) => !seen.has(file.path))];
      });
    } catch (cause) {
      setMessage(String(cause));
    }
  }, []);

  const previewName = useCallback(async () => {
    if (files.length === 0 || !model) {
      setPreview([]);
      return;
    }
    try {
      const result = await ipc<string[]>("preview_output", {
        source: files[0].path,
        model,
        language,
        pattern,
        outputDir,
        formats,
      });
      setPreview(result);
    } catch {
      setPreview([]);
    }
  }, [files, model, language, pattern, outputDir, formats]);

  useEffect(() => {
    const timer = setTimeout(previewName, 300);
    return () => clearTimeout(timer);
  }, [previewName]);

  async function start() {
    if (files.length === 0 || !studio?.canRun) return;
    setRunning(true);
    setMessage(null);
    setRows([]);
    try {
      await ipc("start_job", {
        request: {
          entries: files.map((file) => ({
            path: file.path,
            relative: preserveTree ? file.relative : "",
          })),
          task,
          language,
          model,
          formats,
          pattern,
          outputDir,
        },
      });
    } catch (cause) {
      setMessage(String(cause));
      setRunning(false);
    }
  }

  async function changeDataLocation() {
    if (!isTauri()) {
      setDataMessage("Mover datos solo está disponible en la app de escritorio.");
      return;
    }
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== "string") return;

    setDataMessage("Moviendo datos…");
    setDataProgress(0);
    try {
      const info = await ipc<DataInfo>("relocate_data", {
        target: selected,
        keepCopy,
      });
      setDataInfo(info);
      setDataMessage(
        "Datos movidos y verificados. Reinicia Whisper Desktop para usar la nueva ubicación.",
      );
    } catch (cause) {
      setDataMessage(String(cause));
    } finally {
      setDataProgress(null);
    }
  }

  async function repairEngine() {
    setRepairing(true);
    setRepairMessage("Descargando el motor de nuevo…");
    try {
      await ipc("repair_engine");
      setRepairMessage(
        "Motor reinstalado y verificado. Vuelve a intentar la transcripción.",
      );
    } catch (cause) {
      setRepairMessage(String(cause));
    } finally {
      setRepairing(false);
    }
  }

  async function runIntegration(command: "integrate_system" | "unintegrate_system") {
    try {
      const report = await ipc<IntegrationReport>(command);
      setMessage(
        report.errors.length > 0
          ? report.errors.join(" · ")
          : command === "integrate_system"
            ? `${report.actions.length} elementos integrados en el sistema`
            : "Accesos directos eliminados",
      );
    } catch (cause) {
      setMessage(String(cause));
    }
  }

  function toggleFormat(value: string) {
    setFormats((prev) =>
      prev.includes(value) ? prev.filter((item) => item !== value) : [...prev, value],
    );
  }

  return (
    <div className="studio">
      <header className="wizard-head">
        <div className="brand">
          <span className="dot" />
          Whisper Desktop
        </div>
        <span className="muted small">
          {studio?.backend ? `Motor: ${studio.backend}` : state.dataDir}
        </span>
      </header>

      {studio?.problem && <div className="banner warn">{studio.problem}</div>}

      <section className="card">
        <h2>Archivos</h2>
        <button className="dropzone" onClick={pickFiles}>
          <strong>Arrastra audios o vídeos aquí</strong>
          <span className="muted small">
            mp3 · wav · m4a · flac · ogg · mp4 · mkv · mov
          </span>
          <span className="muted small">o haz clic para elegirlos</span>
        </button>

        <div className="actions">
          <button className="ghost" onClick={pickInputFolder}>
            Añadir carpeta completa
          </button>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={preserveTree}
              onChange={(event) => setPreserveTree(event.target.checked)}
            />
            Mantener estructura de carpetas
          </label>
        </div>

        {files.length > 0 && (
          <ul className="file-list">
            {files.map((file) => (
              <li key={file.path}>
                <span className="mono small">
                  {file.relative ? `${file.relative}/` : ""}
                  {file.path.split(/[\\/]/).pop()}
                </span>
                <button
                  className="icon"
                  title="Quitar"
                  onClick={() =>
                    setFiles((prev) => prev.filter((item) => item.path !== file.path))
                  }
                >
                  ✕
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="card">
        <h2>Opciones</h2>

        <div className="field">
          <span className="label">Tarea</span>
          <div className="segmented">
            {[
              ["transcribe", "Transcribir"],
              ["translateEn", "Traducir al inglés"],
              ["both", "Ambas"],
            ].map(([value, label]) => (
              <button
                key={value}
                className={task === value ? "chip active" : "chip"}
                onClick={() => setTask(value)}
              >
                {label}
              </button>
            ))}
          </div>
          {task === "both" && (
            <p className="muted small">
              Se ejecuta dos veces (transcripción y traducción): tarda el doble.
            </p>
          )}
        </div>

        <div className="grid">
          <label className="field">
            <span className="label">Idioma del audio</span>
            <select value={language} onChange={(event) => setLanguage(event.target.value)}>
              {LANGUAGES.map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>

          <label className="field">
            <span className="label">Modelo</span>
            <select value={model} onChange={(event) => setModel(event.target.value)}>
              {(studio?.models ?? []).map((file) => (
                <option key={file} value={file}>
                  {file.replace("ggml-", "").replace(".bin", "")}
                </option>
              ))}
            </select>
          </label>
        </div>

        <div className="field">
          <span className="label">Carpeta de salida</span>
          <div className="row">
            <input
              value={outputDir}
              onChange={(event) => setOutputDir(event.target.value)}
              className="mono small"
            />
            <button className="ghost" onClick={pickFolder}>
              Cambiar…
            </button>
          </div>
        </div>

        <div className="field">
          <span className="label">Formatos</span>
          <div className="segmented">
            {["txt", "srt", "vtt", "json"].map((value) => (
              <button
                key={value}
                className={formats.includes(value) ? "chip active" : "chip"}
                onClick={() => toggleFormat(value)}
              >
                {value.toUpperCase()}
              </button>
            ))}
          </div>
        </div>

        <div className="field">
          <span className="label">Nombre del archivo</span>
          <input value={pattern} onChange={(event) => setPattern(event.target.value)} />
          <p className="muted small">
            Variables: {"{nombre} {modelo} {idioma} {tarea} {fecha} {hora}"}
          </p>
          {preview.length > 0 && (
            <p className="muted small mono">
              {preview.map((path) => path.split(/[\\/]/).pop()).join(" · ")}
            </p>
          )}
        </div>
      </section>

      {rows.length > 0 && (
        <section className="card">
          <h2>Cola</h2>
          <ul className="steps">
            {rows.filter(Boolean).map((row) => (
              <li key={row.index}>
                <div className="step-head">
                  <span className="mono small">{row.source.split(/[\\/]/).pop()}</span>
                  <span className={row.status === "error" ? "muted small danger" : "muted small"}>
                    {row.status === "error"
                      ? row.error ?? "error"
                      : row.status === "ok"
                        ? "listo"
                        : `${row.percent}%`}
                  </span>
                </div>
                <div className="bar">
                  <div
                    className="bar-fill"
                    style={{
                      width: `${row.percent}%`,
                      background: row.status === "error" ? "var(--danger)" : undefined,
                    }}
                  />
                </div>
              </li>
            ))}
          </ul>
        </section>
      )}

      <section className="card highlight">
        <div className="actions">
          <button
            className="ghost"
            disabled={!running}
            onClick={() => ipc("cancel_job").catch(() => undefined)}
          >
            Cancelar
          </button>
          <button
            className="primary"
            disabled={running || files.length === 0 || !studio?.canRun}
            onClick={start}
          >
            {files.length > 1 ? `Transcribir ${files.length} archivos` : "Iniciar"}
          </button>
        </div>
        {message && <p className="muted small">{message}</p>}
      </section>

      <section className="card">
        <h2>Historial</h2>
        {history.length === 0 && <p className="muted small">Todavía no hay trabajos.</p>}
        <ul className="file-list">
          {history.map((entry) => (
            <li key={entry.id}>
              <span className="mono small">
                {entry.ok ? "✔" : "✕"} {entry.source.split(/[\\/]/).pop()} · {entry.model}
              </span>
              <span className="muted small">
                {entry.ok && entry.outputs.length > 0 ? (
                  <button
                    className="link"
                    onClick={() =>
                      ipc("open_path", {
                        path: entry.outputs[0].replace(/[\\/][^\\/]+$/, ""),
                      }).catch(() => undefined)
                    }
                  >
                    Abrir carpeta
                  </button>
                ) : (
                  entry.error ?? ""
                )}
              </span>
            </li>
          ))}
        </ul>
      </section>

      <section className="card">
        <h2>Datos y almacenamiento</h2>
        {dataInfo && (
          <div className="grid">
            <Info label="Ubicación" value={dataInfo.dir} />
            <Info
              label="Contenido"
              value={`${dataInfo.files} archivos · ${formatSize(dataInfo.bytes)}`}
            />
          </div>
        )}
        <div className="actions">
          <label className="checkbox">
            <input
              type="checkbox"
              checked={keepCopy}
              onChange={(event) => setKeepCopy(event.target.checked)}
            />
            Conservar copia en el origen
          </label>
          <button className="ghost" disabled={dataProgress !== null} onClick={changeDataLocation}>
            Cambiar ubicación…
          </button>
        </div>
        {dataProgress !== null && (
          <div className="bar">
            <div className="bar-fill" style={{ width: `${dataProgress}%` }} />
          </div>
        )}
        {dataMessage && <p className="muted small">{dataMessage}</p>}
      </section>

      <section className="card">
        <h2>Mantenimiento del motor</h2>
        <p className="muted small">
          Si la transcripción falla al arrancar (por ejemplo, tras una actualización
          del motor), descarga de nuevo los binarios del backend que usas. No toca
          los modelos ni tus transcripciones.
        </p>
        <div className="actions">
          <span className="muted small">
            Motor actual: <strong>{state.platform}</strong>
          </span>
          <button className="ghost" disabled={repairing} onClick={repairEngine}>
            {repairing ? "Reparando…" : "Reparar motor"}
          </button>
        </div>
        {repairMessage && <p className="muted small">{repairMessage}</p>}
      </section>

      <section className="card">
        <h2>Integración con el sistema</h2>
        <div className="actions">
          <button className="ghost" onClick={() => runIntegration("unintegrate_system")}>
            Quitar accesos directos
          </button>
          <button className="ghost" onClick={() => runIntegration("integrate_system")}>
            Reintegrar en el menú
          </button>
        </div>
      </section>
    </div>
  );
}

function formatSize(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  if (bytes >= 1024 ** 2) return `${Math.round(bytes / 1024 ** 2)} MB`;
  return `${bytes} B`;
}

function Info({ label, value }: { label: string; value: string }) {
  return (
    <div className="info">
      <span className="label">{label}</span>
      <span className="mono small">{value}</span>
    </div>
  );
}
