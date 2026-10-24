# Feuille de route

## Phase 1 — Socle sécurisé ✅ (en cours)
- [x] Espace de travail Rust, lints stricts, `unsafe` interdit
- [x] Cryptographie : Argon2id, XChaCha20-Poly1305, enveloppes de clés, tests par propriétés
- [x] Moteur psychométrique : âge, normes, z, percentiles empiriques et normaux, notes dérivées, IC, RCI
- [x] Batteries déclaratives JSON avec validation
- [x] Journal d'audit chaîné par HMAC
- [x] Politique réseau IA « local uniquement », pseudonymisation
- [x] Serveur : en-têtes de durcissement, écoute locale par défaut
- [x] CI Windows et Linux, cargo-deny

## Phase 2 — Coffre et comptes ✅ (en cours)
- [x] Coffre SQLite chiffré : une clé par enregistrement, AAD, `secure_delete`, permissions restreintes
- [x] Journal d'audit persistant, vérifié à chaque ouverture (un journal altéré bloque l'ouverture)
- [x] Phrase de passe : 14 caractères minimum, robustesse évaluée par zxcvbn, informations personnelles refusées
- [x] TOTP (RFC 6238) avec protection contre le rejeu, 10 codes de récupération à usage unique
- [x] Délai exponentiel après échecs (phrase de passe **et** second facteur)
- [x] Sessions : jeton de 256 bits haché, inactivité 5 min, durée maximale 8 h, verrouillage automatique
- [x] Serveur : jeton d'installation à usage unique, cookie `__Host-`, protection CSRF (en-tête et origine)
- [x] TLS 1.3 uniquement (rustls), certificat auto-signé, empreinte affichée
- [ ] **WebAuthn / FIDO2** (clé matérielle) avec l'extension PRF, pour lier la clé du coffre au jeton physique
- [ ] Sauvegardes chiffrées, restauration testée, migrations de schéma
- [ ] Appairage des appareils par QR code (avec l'empreinte du certificat)
- [ ] Changement de phrase de passe et régénération des codes de récupération via l'API

## Phase 3 — Interfaces
- [ ] Interface React/TypeScript, accessible (WCAG 2.2 AA), en français (néerlandais et allemand prévus pour la Belgique)
- [ ] Application de bureau Tauri 2 : installateurs MSI signés (Windows), .deb, .rpm et AppImage (Linux)
- [ ] PWA installable sur iPhone et Android
- [ ] Synchronisation en temps réel (WebSocket), fonctionnement hors ligne

## Phase 4 — Clinique
- [ ] Dossiers patients, anamnèse structurée, consentements
- [ ] Mode passation : chronomètre, enregistrement audio local (avec consentement), cotation item par item, affichage pour l'enfant sur un second écran
- [ ] Profils graphiques (z-scores par domaine), courbes d'évolution, RCI
- [ ] Éditeur de batteries, import et export JSON signés
- [ ] Premières épreuves originales Phonalys, avec un module de collecte normative anonymisée

## Phase 5 — IA personnelle
- [ ] Connecteurs Ollama et API compatible OpenAI locale (llama.cpp, LM Studio, vLLM), assistant de configuration
- [ ] Bibliothèque de modèles de documents (bilans, rapports, courriers, demandes INAMI), apprentissage du style à partir des exemples du praticien (RAG local, sans réentraînement)
- [ ] Génération de bilans complets : texte, tableaux, graphiques et diagrammes, export DOCX, ODT et PDF
- [ ] Assistant conversationnel (« rédige le bilan des patients d'aujourd'hui »), avec des outils internes limités et audités
- [ ] Serveur MCP local optionnel, pour brancher les agents IA du praticien
- [ ] Base de veille juridique intégrée et mise à jour

## Phase 6 — Mise sur le marché
- [ ] Audit de sécurité externe et test d'intrusion
- [ ] Validation juridique (CLUF, AIPD type), dépôt de la marque
- [ ] Builds reproductibles, signature des installateurs, mises à jour signées
