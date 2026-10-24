/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Développement : lancer le serveur Phonalys avec
//   PHONALYS_PUBLIC_ORIGIN=http://localhost:5173
// puis `npm run dev`. Les appels /api sont relayés sans changer l'en-tête Host, pour
// que le contrôle d'origine du serveur reste actif.
export default defineConfig({
  plugins: [react()],
  build: {
    outDir: "dist",
    // Rien d'incorporé en data: ou en ligne : compatible avec la CSP stricte du serveur.
    assetsInlineLimit: 0,
    sourcemap: false,
  },
  server: {
    proxy: {
      "/api": { target: "https://localhost:8443", secure: false, changeOrigin: false },
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    restoreMocks: true,
  },
});
