# Phonalys

**Plateforme d'évaluation et de rédaction de bilans logopédiques, auto-hébergée, avec IA personnelle locale.**

Phonalys s'adresse aux logopèdes indépendants. Le praticien fait passer ses épreuves, l'outil cote les résultats selon les normes, puis l'IA, qui tourne sur le matériel du praticien, rédige les bilans et documents à partir de ses propres modèles. Les données patients ne quittent jamais l'infrastructure du praticien.

> Logiciel propriétaire : voir [LICENSE](LICENSE). Toute reproduction est interdite.

## Principes

1. **Les données restent chez le praticien.** Aucun serveur central ni compte hébergé par l'éditeur, et aucune télémétrie.
2. **IA locale uniquement.** Ollama, llama.cpp, LM Studio, vLLM… sur la machine ou le réseau privé du praticien. Les IA grand public (ChatGPT, Claude, Gemini, Mistral cloud…) sont **bloquées par construction**, voir [`phonalys-ai`](crates/phonalys-ai).
3. **Chiffrement de bout en bout au repos.** Le chiffrement utilise XChaCha20-Poly1305, les clés sont dérivées par Argon2id et organisées en enveloppes (une clé par dossier patient).
4. **Double authentification obligatoire.** Une clé matérielle FIDO2 (YubiKey, Nitrokey) est recommandée, et TOTP sert de méthode de secours.
5. **Le praticien décide.** L'IA propose, le logopède valide. Aucun document ne sort sans relecture humaine.

## Composants

| Élément | Rôle | État |
|---|---|---|
| `crates/phonalys-crypto` | Dérivation de clés, chiffrement authentifié, enveloppes, hachage des mots de passe | ✅ testé |
| `crates/phonalys-psychometrics` | Âge, normes, z, percentiles, notes standard/échelle/T, IC 95 %, RCI | ✅ testé |
| `crates/phonalys-core` | Batteries déclaratives (JSON), cotation, journal d'audit chaîné par HMAC | ✅ testé |
| `crates/phonalys-ai` | Politique réseau IA (local uniquement), pseudonymisation | ✅ testé |
| `crates/phonalys-vault` | Coffre SQLite chiffré par enregistrement, délai après échecs, audit persistant | ✅ testé |
| `crates/phonalys-auth` | Politique de phrase de passe, TOTP (RFC 6238), codes de récupération, sessions | ✅ testé |
| `crates/phonalys-server` | Serveur TLS 1.3 : installation, connexion 2FA, verrouillage auto, API patients | ✅ testé (web/PWA en phase 3) |
| `apps/desktop` | Application Tauri (Windows, Linux) | ⏳ phase 2 |
| `apps/web` | Interface React, installable en PWA (iPhone, Android) | ⏳ phase 2 |

## Développement

Prérequis : [Rust stable](https://rustup.rs). Sous Windows, il faut aussi les Build Tools MSVC.

```sh
cargo test --workspace          # tous les tests
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p phonalys-server    # écoute sur https://127.0.0.1:8443 (TLS 1.3)
```

Au premier démarrage, la console affiche un **jeton d'installation à usage unique** et l'**empreinte SHA-256 du certificat**. Notez l'empreinte, puis vérifiez-la sur chaque appareil que vous connectez.

| Variable | Défaut | Rôle |
|---|---|---|
| `PHONALYS_DATA_DIR` | `%LOCALAPPDATA%\Phonalys` / `~/.local/share/Phonalys` | Coffre et certificat |
| `PHONALYS_BIND` | `127.0.0.1:8443` | Adresse d'écoute (`0.0.0.0:8443` pour le réseau local) |
| `PHONALYS_TLS_NAMES` | — | Noms et adresses IP supplémentaires du certificat, par exemple `phonalys.lan,192.168.1.20` |
| `PHONALYS_INSECURE_HTTP` | — | `1` = HTTP en clair, **refusé hors de 127.0.0.1** (développement) |

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [Sécurité et modèle de menace](docs/SECURITE.md)
- [Conformité légale (RGPD, droit belge, MDR, AI Act)](docs/CONFORMITE.md)
- [Feuille de route](docs/ROADMAP.md)
