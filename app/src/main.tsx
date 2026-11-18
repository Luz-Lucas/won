import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { windowReady } from "./api";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

// A janela nasce escondida; mostra-a depois do primeiro desenho para evitar o flash branco.
requestAnimationFrame(() => {
  windowReady().catch(() => {
    /* sem backend (navegador): nada a fazer */
  });
});
