# Sécurité et modèle de menace

La sécurité est le cœur de Phonalys, pas une option. Ce document dit ce qui est protégé, contre qui, et comment. Il dit aussi ce qui **ne peut pas** être garanti : aucun logiciel n'est infaillible, et le prétendre serait déjà une faille.

## Actifs protégés

| Actif | Sensibilité |
|---|---|
| Dossiers patients (identité, anamnèse, résultats, bilans) | Données de santé de mineurs, RGPD art. 9 |
| Documents professionnels importés dans le coffre | Élevée |
| Clés (KEK, clé maîtresse, clés de dossiers, clé d'audit) | Critique |
| Journal d'audit | Preuve légale |

## Adversaires considérés

1. **Vol ou perte de l'ordinateur ou du disque.** Tout est chiffré au repos, et sans la phrase de passe et le second facteur, les données sont illisibles.
2. **Logiciel malveillant ou attaquant sur le réseau local.** TLS est obligatoire hors de la boucle locale, l'API est authentifiée, le serveur écoute par défaut sur 127.0.0.1 seulement, et les en-têtes HTTP sont durcis.
3. **Exfiltration via l'IA.** L'IA ne joint que des IP privées, des fournisseurs cloud sont sur liste noire, la connexion est figée sur les adresses vérifiées, et les invites sont pseudonymisées.
4. **Altération des traces.** Le journal d'audit est chaîné par HMAC-SHA256 : toute modification, suppression ou permutation est détectée.
5. **Chaîne d'approvisionnement.** `cargo-deny` vérifie les avis de sécurité, les licences et les sources ; OpenSSL est banni (rustls uniquement) ; `Cargo.lock` est versionné ; les builds sont reproductibles ; les installateurs sont signés.

**Hors périmètre, à dire honnêtement :** un poste déjà compromis *pendant* une session déverrouillée (un enregistreur de frappe ou une capture d'écran) peut voir ce que voit le praticien. On réduit ce risque avec le verrouillage automatique, des sessions courtes et des recommandations de durcissement de l'OS, mais on ne l'élimine pas.

## Cryptographie

| Usage | Algorithme | Notes |
|---|---|---|
| Chiffrement des données | **XChaCha20-Poly1305** (AEAD, clé de 256 bits, nonce aléatoire de 192 bits) | Niveau de sécurité équivalent à AES-256-GCM, mais sans risque de réutilisation de nonce, et rapide sans accélération AES matérielle. |
| Dérivation depuis la phrase de passe | **Argon2id** : 256 MiB, 3 passes, 4 voies | Résiste aux attaques par GPU ou ASIC. |
| Mots de passe de connexion | Argon2id, format PHC | |
| Intégrité de l'audit | HMAC-SHA256 chaîné | |
| Transport | TLS 1.3 (rustls) | Prévu |

Les blobs sont liés à leur contexte par AAD (`patient:<id>`), ce qui empêche de déplacer un résultat d'un dossier à un autre. Les clés en mémoire sont effacées à leur destruction (`zeroize`). Le code Rust interdit `unsafe` dans tout l'espace de travail.

On parle souvent de « chiffrement de niveau militaire » : concrètement, ce sont des algorithmes symétriques de 256 bits, la même catégorie que ceux validés pour les informations classifiées. Ce qui fait réellement la différence, c'est la gestion des clés et l'authentification, pas l'étiquette.

## Authentification

### Ce qui est en place (phase 2)

| Mesure | Détail |
|---|---|
| Installation | Jeton à usage unique affiché sur la console du serveur. Sans lui, personne sur le réseau ne peut s'approprier une installation vierge. |
| Phrase de passe | Au moins 14 caractères et un score zxcvbn de 4/4 (dictionnaires, claviers, dates). Le nom du praticien est refusé. Elle n'est **jamais stockée**, même hachée : elle sert uniquement à dériver la clé du coffre. |
| Second facteur | TOTP (RFC 6238) dont le secret est scellé *dans* le coffre. Un code ne peut pas être rejoué, et la fenêtre de tolérance est de ±30 s. |
| **Biométrie / clé de sécurité** | WebAuthn/FIDO2 avec **vérification de l'utilisateur obligatoire** : Windows Hello, Touch ID, Face ID, empreinte Android, YubiKey Bio. Les signatures ES256 et Ed25519 sont acceptées. L'origine et le RP ID sont vérifiés, ce qui rend la méthode résistante à l'hameçonnage. Les défis sont à usage unique et valables 2 min. Un authentificateur cloné est détecté par son compteur de signatures. |
| Politique « clé obligatoire » | Le TOTP est alors refusé. Les codes de récupération restent valables en cas de perte de l'appareil. La dernière clé ne peut pas être retirée tant que la politique est active. |
| **Mode lié au matériel** (option) | Grâce à l'extension **PRF** de WebAuthn, la clé du coffre dépend à la fois de la phrase de passe **et** d'un secret qui ne quitte jamais l'authentificateur. Chaque clé liée garde une double enveloppe : `chiffrement(clé de la phrase de passe, chiffrement(HKDF(sortie PRF), clé maîtresse))`. L'enveloppe « phrase de passe seule » est supprimée : voler le fichier du coffre **et** la phrase de passe ne suffit plus. À l'activation, de nouveaux codes de récupération sont générés et les anciens sont révoqués. La dernière clé liée ne peut pas être retirée. |
| Récupération | 10 codes à usage unique de 80 bits. En mode lié, chaque code est lui-même une enveloppe cryptographique de la clé du coffre, détruite après usage. |
| Échecs | **4 essais**, puis connexion bloquée 15 min (et de nouveau 15 min à chaque échec suivant). Un second facteur faux compte comme un échec. Les essais restants sont affichés. Un **code de récupération débloque immédiatement** : avec ses 80 bits aléatoires, il ne peut pas être deviné, et un échec ne révèle pas si c'est la phrase de passe ou le code qui est faux. Les essais sont traités un par un. |
| Sessions | Jeton de 256 bits, dont seule l'empreinte reste en mémoire. Cookie `__Host-`, `Secure`, `HttpOnly`, `SameSite=Strict`. Expiration après 5 min d'inactivité ou 8 h au total. |
| Verrouillage | Sans session active, les clés sont effacées de la mémoire. `/api/lock` verrouille immédiatement tous les appareils. |
| CSRF | Les requêtes qui modifient l'état exigent un en-tête dédié et une origine identique à celle du serveur (en HTTP/1.1 comme en HTTP/2). |
| DNS rebinding | Le serveur ne répond qu'aux requêtes adressées à son nom public (`PHONALYS_PUBLIC_ORIGIN`). Un site malveillant dont le domaine pointerait vers le serveur ou vers 127.0.0.1 reçoit un refus (421). |
| Application web | CSP stricte (aucun script ni style en ligne, vérifié à chaque compilation). Le service worker ne met en cache que l'interface, **jamais** les réponses de l'API. Aucun stockage local des données dans le navigateur. |
| Application de bureau | Serveur embarqué à l'écoute de la seule boucle locale, port aléatoire. La fenêtre ne peut naviguer vers aucune autre adresse et n'a accès à aucune commande native. |
| TLS | TLS 1.3 uniquement. Certificat auto-signé dont l'empreinte SHA-256 est à épingler. |

### Biométrie et RGPD

L'empreinte ou le visage sont vérifiés **par l'appareil lui-même**. Phonalys ne reçoit qu'une signature cryptographique et le drapeau « utilisateur vérifié ». **Aucune donnée biométrique n'est transmise ni stockée.** On évite ainsi un traitement de données biométriques au sens de l'article 9 du RGPD.

Ce drapeau signifie « vérifié par biométrie **ou** par le code PIN de l'appareil » : le protocole ne permet pas de distinguer les deux. Pour imposer une biométrie stricte, il faudra restreindre les modèles acceptés (AAGUID), par exemple à une YubiKey Bio. C'est une option prévue.

**Contrainte technique :** WebAuthn exige un nom de domaine (pas une adresse IP) et un certificat reconnu par l'appareil. Sur le réseau local, il faut donc un nom comme `phonalys.lan` et l'installation de l'autorité de certification Phonalys sur chaque appareil. Ce sera géré par l'appairage en phase 3. `localhost` fonctionne directement.

### Limites, à connaître honnêtement

- **Hors mode lié, le second facteur (TOTP ou biométrie) reste un contrôle d'accès, pas un facteur cryptographique.** Quelqu'un qui volerait *le fichier du coffre* **et** la phrase de passe pourrait le déchiffrer hors ligne. Le **mode lié au matériel** supprime ce risque ; il est fortement recommandé dès qu'au moins deux clés compatibles PRF sont enregistrées.
- **Mode lié : la perte de toutes les clés liées et de tous les codes de récupération rend le coffre définitivement illisible.** Personne, pas même l'éditeur, ne peut le rouvrir. Il faut lier au moins deux clés (par exemple Windows Hello et une YubiKey rangée à part) et conserver les codes imprimés hors ligne.
- L'extension PRF n'existe pas partout : elle fonctionne avec Windows Hello (Windows 11 récent), les passkeys iCloud (iOS/macOS 18+), Android avec Google Password Manager et les YubiKey 5 / Bio (hmac-secret). Un appareil sans PRF peut toujours servir de clé de connexion, mais pas de clé liée.
- Une sortie PRF reste en mémoire du serveur le temps d'une connexion, puis elle est effacée (`zeroize`).
- Le délai après échecs ne freine que les essais *via l'application*. Hors ligne, c'est Argon2id (256 MiB par essai) qui protège, d'où l'exigence d'une phrase de passe forte.

### Recommandations

- Une **phrase de passe longue** (5 mots ou plus) est obligatoire. Une vérification contre une liste de mots de passe compromis, embarquée hors ligne, est prévue.
- Le **second facteur est obligatoire** :
  - recommandé : **clé FIDO2 / WebAuthn** (YubiKey 5, Nitrokey 3, Token2), résistante à l'hameçonnage ;
  - secours : TOTP (Aegis, 2FAS, Ente Auth) et codes de récupération imprimés.
- **Verrouillage automatique** après inactivité (5 min par défaut, réglable), à la mise en veille et à la fermeture du capot.
- Le **coffre documentaire** exige une nouvelle authentification forte à chaque ouverture.
- L'échec répété d'une connexion déclenche un délai exponentiel, et chaque tentative est inscrite à l'audit.

## Recommandations au praticien

- **Gestionnaire de mots de passe :**
  - **KeePassXC**, hors ligne, base locale chiffrée : c'est le choix le plus cohérent avec Phonalys, rien n'est dans le cloud ;
  - **Bitwarden auto-hébergé** (ou Vaultwarden) si une synchronisation entre appareils est nécessaire ;
  - **Proton Pass** ou **1Password** comme alternatives hébergées réputées.
- Chiffrement complet du disque : BitLocker (Windows Pro), LUKS (Linux).
- Système et Phonalys toujours à jour, et un compte utilisateur non administrateur pour le travail quotidien.
- **Sauvegardes chiffrées 3-2-1** : 3 copies, sur 2 supports différents, dont 1 hors site. Phonalys exporte des sauvegardes déjà chiffrées.
- Accès distant **uniquement via VPN**, jamais par redirection de port.

## Signaler une vulnérabilité

Écrire à [ADRESSE SÉCURITÉ À DÉFINIR]. Merci de ne pas publier la faille avant son correctif. Nous nous engageons à répondre sous 72 h.
