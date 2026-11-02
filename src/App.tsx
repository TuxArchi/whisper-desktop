import { useCallback, useEffect, useState } from "react";
import { ipc } from "./lib/ipc";
import type { AppState } from "./lib/types";
import Studio from "./screens/Studio";
import Wizard from "./screens/Wizard";

export default function App() {
  const [state, setState] = useState<AppState | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    ipc<AppState>("get_state")
      .then(setState)
      .catch((cause: unknown) => setError(String(cause)));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  if (error) {
    return (
      <main className="screen centered">
        <h1>Algo falló al arrancar</h1>
        <p className="muted">{error}</p>
      </main>
    );
  }

  if (!state) {
    return (
      <main className="screen centered">
        <div className="spinner" aria-label="Cargando" />
      </main>
    );
  }

  return (
    <main className="screen">
      {state.zipPreview && (
        <div className="banner danger">
          Estás ejecutando el programa desde dentro del ZIP. Extrae la carpeta antes de
          continuar: si no, los modelos se guardarán en una carpeta temporal.
        </div>
      )}
      {state.installed ? (
        <Studio state={state} />
      ) : (
        <Wizard state={state} onInstalled={refresh} />
      )}
    </main>
  );
}
