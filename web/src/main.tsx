import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

// Le service worker ne met en cache que l'interface, jamais les données (voir public/sw.js).
if ("serviceWorker" in navigator && window.isSecureContext && import.meta.env.PROD) {
  void navigator.serviceWorker.register("/sw.js");
}
