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
- [x] Journal d'audit persistant, vérifié à chaque ouverture, avec ancrage de la dernière entrée ; un journal altéré déclenche une alerte sans bloquer l'accès aux dossiers
- [x] Phrase de passe : 14 caractères minimum, robustesse évaluée par zxcvbn, informations personnelles refusées
- [x] TOTP (RFC 6238) avec protection contre le rejeu, 10 codes de récupération à usage unique
- [x] Blocage de 15 min après 4 échecs (phrase de passe **et** second facteur)
- [x] Sessions : jeton de 256 bits haché, inactivité réglable (15 min par défaut), durée maximale 8 h, verrouillage automatique
- [x] Serveur : jeton d'installation à usage unique, cookie `__Host-`, protection CSRF (en-tête et origine)
- [x] TLS 1.3 uniquement (rustls), certificat auto-signé, empreinte affichée
- [x] **WebAuthn / FIDO2 biométrique** : vérification de l'utilisateur obligatoire, ES256 et Ed25519, détection des clones, politique « clé obligatoire »
- [x] Extension **PRF** : mode « lié au matériel », clé du coffre dépendant de la phrase de passe **et** de l'authentificateur ; codes de récupération cryptographiques (format de coffre v2, migration automatique depuis v1)
- [ ] Option « biométrie stricte » : liste blanche d'AAGUID (YubiKey Bio…)
- [x] Sauvegardes chiffrées, test de restauration automatique, copie 3-2-1, restauration sur une nouvelle installation
- [ ] Migrations de schéma
- [x] Appairage des appareils par QR code, autorité locale à contrainte de noms, profil iOS et certificat Android
- [x] Changement de phrase de passe et régénération des codes de récupération
- [x] Écritures tout ou rien (données et journal), instance unique par coffre, saisie conservée après un verrouillage automatique

## Phase 3 — Interfaces
- [x] Interface React/TypeScript servie par le serveur : installation (QR code TOTP généré localement), connexion (clé biométrique avec PRF, TOTP, récupération), patients, sécurité (clés, liaison, politique, mode lié), compte à rebours du verrouillage, verrouillage immédiat
- [x] Compatible avec la CSP stricte (aucun script ni style en ligne, contrôlé à chaque compilation)
- [x] PWA : manifest et service worker qui ne met **jamais** en cache les données de l'API
- [x] Traduction de l'interface (français, néerlandais, allemand) : accès, installation, appairage, structure, réglages ; contrôle automatique des clés manquantes
- [x] Accessibilité de base : lien d'évitement, focus déplacé à chaque page, onglets au clavier, aides reliées aux champs, mouvement réduit, compte à rebours annoncé une seule fois
- [ ] Traduction des écrans cliniques (dossier, bilans, agenda, documents) et des messages d'erreur du serveur ; relecture par des logopèdes néerlandophones et germanophones
- [ ] Exercices et lexiques néerlandais et allemands (contenus propres à chaque langue, pas des traductions)
- [ ] Audit d'accessibilité WCAG 2.2 AA externe (lecteurs d'écran NVDA, VoiceOver, TalkBack)
- [x] Icônes PNG pour iOS (apple-touch-icon)
- [x] Application de bureau Tauri 2 : serveur embarqué sur la boucle locale (port aléatoire), navigation verrouillée, aucune commande native exposée, jeton d'installation transmis hors réseau
- [ ] Installateurs **signés** : MSI (Windows), .deb, .rpm et AppImage (Linux) ; mises à jour signées
- [x] Verrouillage à la sortie de veille, déconnexion après un passage en arrière-plan (réglable)
- [ ] Bureau : détection du verrouillage de session du système (Win+L) sans mise en veille
- [ ] Bureau sous Linux : WebKitGTK ne gère pas encore WebAuthn (connexion par code TOTP en attendant)
- [ ] Synchronisation en temps réel (WebSocket), fonctionnement hors ligne

## Phase 4 — Clinique
- [x] Dossiers patients, anamnèse structurée, prise en charge, séances, objectifs, agenda
- [x] Coffre documentaire chiffré, ouverture soumise à une nouvelle authentification
- [ ] Consentements signés
- [ ] Mode passation : chronomètre, enregistrement audio local (avec consentement), cotation item par item, affichage pour l'enfant sur un second écran
- [ ] Profils graphiques (z-scores par domaine), courbes d'évolution, RCI
- [ ] Éditeur de batteries, import et export JSON signés
- [ ] Premières épreuves originales Phonalys, avec un module de collecte normative anonymisée

## Phase 5 — IA personnelle
- [x] Connecteurs Ollama et API compatible OpenAI locale (llama.cpp, LM Studio, vLLM), test de connexion, épinglage TLS
- [x] Projet de conclusion et de projet thérapeutique à partir des résultats, pseudonymisé, avec les consignes de style du praticien
- [ ] Bibliothèque de modèles de documents (bilans, rapports, courriers, demandes INAMI), apprentissage du style à partir des exemples du praticien (RAG local, sans réentraînement)
- [ ] Génération de bilans complets : texte, tableaux, graphiques et diagrammes, export DOCX, ODT et PDF
- [ ] Assistant conversationnel (« rédige le bilan des patients d'aujourd'hui »), avec des outils internes limités et audités
- [ ] Serveur MCP local optionnel, pour brancher les agents IA du praticien
- [ ] Base de veille juridique intégrée et mise à jour

## Phase 6 — Mise sur le marché
- [ ] Audit de sécurité externe et test d'intrusion
- [x] Écran « Conformité » : registre des traitements, notice patients, trame d'AIPD, procédure et registre des violations (72 h)
- [x] Copie complète du dossier patient (RGPD art. 15 et 20, droits du patient art. 9), après réauthentification
- [ ] Alerte à l'échéance de conservation (30 ans après le dernier contact)
- [ ] Validation juridique (CLUF, registre, notice, AIPD), dépôt de la marque
- [ ] Builds reproductibles, signature des installateurs, mises à jour signées
