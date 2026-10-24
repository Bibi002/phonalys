import { useEffect, useState, type FormEvent } from "react";
import { api } from "../api";
import { ErrorBox, Field, Select, TextArea, useAction, useLoad } from "../components";
import type { Bilan } from "../model";

export type AiKind = "ollama" | "open_ai_compatible_local";

export interface AiSettings {
  enabled: boolean;
  kind: AiKind;
  endpoint: string;
  model: string;
  cert_sha256?: string | null;
  allow_plaintext_lan: boolean;
  style: string;
  timeout_secs: number;
}

const KINDS: readonly (readonly [AiKind, string])[] = [
  ["ollama", "Ollama"],
  ["open_ai_compatible_local", "Compatible OpenAI, local (llama.cpp, LM Studio, vLLM, Jan…)"],
];

const DEFAULT_ENDPOINT: Record<AiKind, string> = {
  ollama: "http://127.0.0.1:11434",
  open_ai_compatible_local: "http://127.0.0.1:1234/v1",
};

/** Réglages de l'assistant de rédaction : un modèle hébergé par le praticien, uniquement. */
export function AssistantSettings() {
  const loaded = useLoad(() => api<AiSettings>("GET", "/api/ai/settings"));
  const [s, setS] = useState<AiSettings>();
  const [models, setModels] = useState<string[]>();
  const [notice, setNotice] = useState<string>();
  const { busy, error, run } = useAction();

  useEffect(() => {
    if (loaded.data) setS(loaded.data);
  }, [loaded.data]);

  if (!s) return <section className="card"><ErrorBox error={loaded.error} /></section>;
  const https = s.endpoint.trim().toLowerCase().startsWith("https:");
  const set = <K extends keyof AiSettings>(k: K, v: AiSettings[K]) => {
    setNotice(undefined);
    setS({ ...s, [k]: v });
  };

  function save(e?: FormEvent) {
    e?.preventDefault();
    if (!s) return;
    setNotice(undefined);
    void run(async () => {
      setS(await api<AiSettings>("PUT", "/api/ai/settings", s));
      setNotice("Réglages de l'assistant enregistrés.");
    });
  }

  function test() {
    if (!s) return;
    setNotice(undefined);
    void run(async () => {
      await api<AiSettings>("PUT", "/api/ai/settings", s);
      const r = await api<{ models: string[] }>("POST", "/api/ai/models");
      setModels(r.models);
      setNotice(r.models.length ? `Connexion réussie : ${r.models.length} modèle(s) installé(s).` : "Connexion réussie, mais aucun modèle n'est installé.");
    });
  }

  return (
    <form className="card" onSubmit={save}>
      <h2>Assistant de rédaction (IA locale)</h2>
      <p className="muted">
        Un modèle installé <strong>chez vous</strong> (Ollama, llama.cpp, LM Studio…) propose une conclusion et un projet
        thérapeutique à partir des résultats saisis. Les IA en ligne (ChatGPT, Claude, Gemini, Mistral…) et toute adresse
        hors de votre réseau sont refusées. Le modèle ne reçoit ni nom, ni école, ni médecin, ni date de naissance : ils
        sont remplacés par des jetons, puis restitués. Le texte proposé est un projet que vous relisez et dont vous
        restez l'auteur.
      </p>
      <label className="check">
        <input type="checkbox" checked={s.enabled} onChange={(e) => set("enabled", e.target.checked)} />
        Activer l'assistant de rédaction
      </label>
      <div className="grid2">
        <Select
          label="Moteur"
          value={s.kind}
          onChange={(k) => setS({ ...s, kind: k, endpoint: s.endpoint === DEFAULT_ENDPOINT[s.kind] ? DEFAULT_ENDPOINT[k] : s.endpoint })}
          options={KINDS}
        />
        <Field label="Adresse du moteur" value={s.endpoint} onChange={(v) => set("endpoint", v)} hint="Sur cet ordinateur : http://127.0.0.1:… ; sur le réseau du cabinet : https:// avec l'empreinte du certificat." />
        {models && models.length > 0 ? (
          <Select label="Modèle" value={s.model} onChange={(v) => set("model", v)} options={[["", "— choisir —"] as const, ...models.map((m) => [m, m] as const)]} />
        ) : (
          <Field label="Modèle" value={s.model} onChange={(v) => set("model", v)} hint="Par exemple mistral:7b ou qwen2.5:14b. « Tester la connexion » liste les modèles installés." />
        )}
        <Field
          label="Délai de réponse (secondes)"
          value={String(s.timeout_secs)}
          inputMode="numeric"
          onChange={(v) => set("timeout_secs", Number(v.replace(/\D/g, "")) || 0)}
          hint="Entre 30 et 900. Un modèle local sans carte graphique peut mettre plusieurs minutes."
        />
      </div>
      {https && (
        <Field
          label="Empreinte SHA-256 du certificat du moteur"
          value={s.cert_sha256 ?? ""}
          onChange={(v) => set("cert_sha256", v)}
          hint="Format AA:BB:… (64 chiffres hexadécimaux). Seul ce certificat sera accepté."
        />
      )}
      {!https && !/^http:\/\/(127\.|localhost|\[::1\])/i.test(s.endpoint.trim()) && (
        <label className="check">
          <input type="checkbox" checked={s.allow_plaintext_lan} onChange={(e) => set("allow_plaintext_lan", e.target.checked)} />
          Autoriser HTTP non chiffré vers une autre machine du réseau (déconseillé : les données circulent en clair)
        </label>
      )}
      <TextArea
        label="Consignes de style"
        value={s.style}
        onChange={(v) => set("style", v)}
        rows={5}
        hint="Votre plan habituel, vos tournures, un extrait de compte rendu anonymisé… Ces consignes accompagnent chaque demande."
      />
      <ErrorBox error={error} />
      {notice && <p className="notice" role="status">{notice}</p>}
      <div className="actions">
        <button type="submit" disabled={busy}>Enregistrer</button>
        <button type="button" className="ghost" disabled={busy} onClick={test}>
          {busy ? "Test en cours…" : "Tester la connexion"}
        </button>
      </div>
    </form>
  );
}

interface Draft {
  conclusion: string;
  projet: string;
  model: string;
}

/** Dans la saisie d'un bilan : demande un projet de rédaction au modèle local. */
export function DraftAssistant(props: { bilan: Bilan; onUse: (conclusion: string, projet: string, mode: "replace" | "append") => void }) {
  const settings = useLoad(() => api<AiSettings>("GET", "/api/ai/settings"));
  const [instructions, setInstructions] = useState("");
  const [draft, setDraft] = useState<Draft>();
  const { busy, error, run } = useAction();

  if (!settings.data?.enabled || !settings.data.model) return null;

  function ask() {
    setDraft(undefined);
    void run(async () => {
      setDraft(
        await api<Draft>("POST", "/api/ai/draft", {
          patient_id: props.bilan.patient_id,
          bilan: { ...props.bilan, resultats: props.bilan.resultats.filter((r) => r.epreuve.trim()) },
          instructions,
        }),
      );
    });
  }

  return (
    <section className="assistant" aria-labelledby="assistant-title">
      <h2 id="assistant-title">Assistant de rédaction</h2>
      <p className="muted small">
        Modèle local « {settings.data.model} ». Les données d'identité sont remplacées par des jetons avant l'envoi.
      </p>
      <Field label="Demande particulière (facultatif)" value={instructions} onChange={setInstructions} hint="Ex. : insister sur la conscience phonologique, ton destiné aux parents…" />
      <button type="button" className="ghost" disabled={busy} onClick={ask}>
        {busy ? "Rédaction en cours… (cela peut prendre quelques minutes)" : "Proposer une conclusion et un projet"}
      </button>
      <ErrorBox error={error} />
      {draft && (
        <div className="draft" role="region" aria-label="Proposition du modèle">
          <p className="warn-text">
            Proposition générée par {draft.model} : relisez chaque phrase, vérifiez chaque score et corrigez avant
            d'enregistrer. Vous restez l'auteur du bilan.
          </p>
          <h3>Conclusion proposée</h3>
          <p className="pre">{draft.conclusion}</p>
          {draft.projet && (
            <>
              <h3>Projet proposé</h3>
              <p className="pre">{draft.projet}</p>
            </>
          )}
          <div className="actions">
            <button type="button" onClick={() => props.onUse(draft.conclusion, draft.projet, "replace")}>
              Utiliser (remplace le texte actuel)
            </button>
            <button type="button" className="ghost" onClick={() => props.onUse(draft.conclusion, draft.projet, "append")}>
              Ajouter à la suite
            </button>
            <button type="button" className="ghost" onClick={() => setDraft(undefined)}>
              Écarter
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
