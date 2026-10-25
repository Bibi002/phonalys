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
4. **Altération des traces.** Le journal d'audit est chaîné par HMAC-SHA256, et un ancrage scellé de la dernière entrée est mis à jour avec chacune : modification, insertion, permutation et suppression (y compris des dernières entrées) sont détectées. Limite : le remplacement du fichier entier par une copie plus ancienne (sauvegarde) ne peut pas l'être. Un journal altéré **n'empêche pas** d'ouvrir le coffre (les dossiers des patients doivent rester accessibles) : une alerte s'affiche à l'accueil et dans le journal.
5. **Chaîne d'approvisionnement.** `cargo-deny` vérifie les avis de sécurité, les licences et les sources ; OpenSSL est banni (rustls uniquement) ; `Cargo.lock` est versionné ; les actions GitHub sont figées par empreinte ; chaque version publiée est signée par Sigstore avec une attestation de provenance ; les mises à jour sont signées (minisign). **Pas encore** : builds reproductibles, signature Authenticode des installateurs Windows (avertissement SmartScreen).

**Hors périmètre, à dire honnêtement :** un poste déjà compromis *pendant* une session déverrouillée (un enregistreur de frappe ou une capture d'écran) peut voir ce que voit le praticien. On réduit ce risque avec le verrouillage automatique, des sessions courtes et des recommandations de durcissement de l'OS, mais on ne l'élimine pas.

## Cryptographie

| Usage | Algorithme | Notes |
|---|---|---|
| Chiffrement des données | **XChaCha20-Poly1305** (AEAD, clé de 256 bits, nonce aléatoire de 192 bits) | Niveau de sécurité équivalent à AES-256-GCM, mais sans risque de réutilisation de nonce, et rapide sans accélération AES matérielle. |
| Dérivation depuis la phrase de passe | **Argon2id** : 256 MiB, 3 passes, 4 voies | Résiste aux attaques par GPU ou ASIC. |
| Mots de passe de connexion | Argon2id, format PHC | |
| Intégrité de l'audit | HMAC-SHA256 chaîné | |
| Transport | TLS 1.3 (rustls) | Uniquement TLS 1.3 hors de la boucle locale |

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
| **Connexion en un geste** (option, par clé) | La sortie PRF d'une clé biométrique ouvre seule une enveloppe de la clé du coffre ; la signature du défi, avec vérification de l'utilisateur, est ensuite exigée. Une telle clé réunit deux facteurs (l'appareil possédé et l'empreinte ou le code de l'appareil). L'activer exige la phrase de passe, pour qu'une personne devant une session restée ouverte ne puisse pas s'ajouter un accès. **Après 3 échecs consécutifs, le geste seul est refusé** : phrase de passe et second facteur sont redemandés. |
| Phrase de passe : changement | « Sécurité › Changer la phrase de passe » : la phrase actuelle est exigée, la nouvelle doit respecter la même politique. Clés liées et codes de récupération restent valables (seules les enveloppes extérieures sont refaites). Toutes les sessions sont fermées et une sauvegarde est faite aussitôt : les sauvegardes antérieures s'ouvrent avec l'**ancienne** phrase de passe. |
| Récupération | 10 codes à usage unique de 80 bits, régénérables (« Sécurité › Nouveaux codes de récupération », les anciens sont alors révoqués). **Un code remplace un second facteur ou un appareil perdu, jamais la phrase de passe** : sans elle, rien ne peut être déchiffré. En mode lié, chaque code est lui-même une enveloppe cryptographique de la clé du coffre, détruite après usage. |
| Échecs | **4 essais**, puis connexion bloquée 15 min (et de nouveau 15 min à chaque échec suivant). Un second facteur faux compte comme un échec. Les essais restants sont affichés. Un **code de récupération débloque immédiatement** : avec ses 80 bits aléatoires, il ne peut pas être deviné, et un échec ne révèle pas si c'est la phrase de passe ou le code qui est faux. Les essais sont traités un par un. |
| Sessions | Jeton de 256 bits, dont seule l'empreinte reste en mémoire. Cookie `__Host-`, `Secure`, `HttpOnly`, `SameSite=Strict`. Expiration après 15 min d'inactivité (réglable : 5, 10, 15, 30 ou 60 min) ou 8 h au total. Clavier, souris et écran tactile comptent comme activité (signal au serveur au plus une fois par minute). |
| Verrouillage | Sans session active, les clés sont effacées de la mémoire. `/api/lock` verrouille immédiatement tous les appareils. **Sortie de veille ou d'hibernation** de la machine qui héberge le serveur (ordinateur de bureau compris) : tout est verrouillé. **Arrière-plan** : fenêtre réduite, autre onglet ou téléphone en veille plus de 5 min (réglable : 1, 5, 15 min ou jamais), l'appareil ferme sa session. Limite connue : le verrouillage de session Windows (Win+L) sans mise en veille n'est pas détecté ; le délai d'inactivité s'applique alors. |
| CSRF | Les requêtes qui modifient l'état exigent un en-tête dédié et une origine identique à celle du serveur (en HTTP/1.1 comme en HTTP/2). |
| DNS rebinding | Le serveur ne répond qu'aux requêtes adressées à son nom public (`PHONALYS_PUBLIC_ORIGIN`). Un site malveillant dont le domaine pointerait vers le serveur ou vers 127.0.0.1 reçoit un refus (421). |
| Dossier clinique | Séances, bilans, objectifs et résultats d'exercices : chaque enregistrement est chiffré avec sa propre clé. Types fixés par liste blanche. Effacer un patient efface tout son dossier clinique (RGPD art. 17). Le journal d'audit est consultable et ne contient que des identifiants pseudonymes. |
| **Sauvegardes** | Copie cohérente du coffre (SQLite `VACUUM INTO`), donc chiffrée comme lui. Chaque sauvegarde subit aussitôt un **test de restauration** : tous les enregistrements sont déchiffrés et la chaîne d'audit est vérifiée ; une sauvegarde en échec est supprimée et signalée. Automatique toutes les 24 h pendant l'utilisation, copie facultative vers un second dossier (règle 3-2-1), conservation des 14 dernières. Noms horodatés sans aucune donnée patient. Rotation dans l'ordre réel des sauvegardes (jamais d'après l'horloge, qui peut être déréglée), fichiers écrits sur le support avant d'être annoncés. **Alerte à l'accueil** si une sauvegarde échoue, si aucune n'a réussi depuis 3 jours, s'il n'y a pas de second dossier ou plus de copie depuis 7 jours. Restauration uniquement sur une installation vierge, avec le jeton d'installation (ou `phonalys-server restore <fichier>`). Une sauvegarde s'ouvre avec la phrase de passe en vigueur **au moment où elle a été faite**. |
| **Coffre documentaire** | Prescriptions, comptes rendus, courriers : chiffrés dans le coffre (une clé par document), inclus dans les sauvegardes, effacés avec le dossier du patient. Le type est déterminé par le serveur d'après le contenu (PDF, PNG, JPEG, texte, Word, OpenDocument, Excel) ; HTML, SVG et exécutables sont refusés. Taille maximale : 20 Mo. **Ouvrir, télécharger ou supprimer un document exige de confirmer son identité** (clé biométrique, ou phrase de passe et code à 6 chiffres), confirmation valable 5 min pour la seule session concernée : une session restée ouverte ne suffit pas. Les échecs comptent dans le blocage de la connexion. Le document est servi sous une CSP sans aucun script, `nosniff`, `no-store`, et ne peut être encadré que par l'application. Chaque ouverture est inscrite au journal. |
| **Copie du dossier** | Export complet d'un patient (identité, dossier clinique, documents) pour le droit d'accès et de portabilité. Même exigence que les documents : **réauthentification récente** de la session. Chaque export est inscrit au journal. Le nom du fichier ne contient aucune donnée d'identité. Le fichier produit n'est **pas chiffré** : l'interface le rappelle. |
| **Assistant de rédaction (IA)** | Uniquement un modèle hébergé par le praticien (Ollama ou API compatible OpenAI locale). Adresse vérifiée à l'enregistrement **et** à chaque appel, après résolution DNS : les noms des fournisseurs d'IA en ligne sont refusés, et toute adresse hors des plages privées (boucle locale, 10/8, 172.16/12, 192.168/16, 100.64/10, fc00::/7) aussi. La connexion se fait aux adresses validées, sans nouvelle résolution (pas de DNS rebinding). Sur le réseau du cabinet, HTTPS obligatoire (TLS 1.3) avec **épinglage de l'empreinte du certificat** du moteur ; HTTP en clair seulement sur la boucle locale, sauf choix explicite. **Pseudonymisation** : nom, prénom, école, médecin et contacts sont remplacés par des jetons, la date de naissance par l'âge ; aucun identifiant interne n'est transmis. Chaque requête est inscrite au journal. Le texte produit est un projet que le praticien relit et valide (aucun diagnostic automatique). |
| **Mises à jour** | Seule connexion sortante de l'application de bureau : la lecture de `latest.json` puis de l'installateur sur la page publique des versions (GitHub, HTTPS). **Aucun identifiant ni aucune donnée du cabinet n'est envoyé.** Installation **uniquement avec l'accord du praticien**, après vérification de la signature (clé minisign du titulaire, clé publique compilée dans l'application) **et** de la version signée (`requireSignedVersion`) : une réponse falsifiée ne peut ni installer un autre fichier ni faire revenir à une ancienne version. La vérification est faite côté Rust ; la page web n'a aucune permission native et ne peut pas déclencher d'installation. Désactivable par un fichier `mises-a-jour-desactivees`. |
| Synthèse vocale | Exercices : seules les voix installées sur l'appareil sont utilisées (`localService`) ; une voix « en ligne » enverrait le texte à un tiers. |
| Application web | CSP stricte (aucun script ni style en ligne, vérifié à chaque compilation). Le service worker ne met en cache que l'interface, **jamais** les réponses de l'API. Aucun stockage local des données dans le navigateur. |
| Instance unique | Un coffre n'est ouvert que par un seul processus à la fois (verrou du système sur `cabinet.phonalys-lock`) : un second lancement, ou l'application de bureau et le serveur du cabinet sur le même ordinateur, est refusé avec un message, au lieu de corrompre le journal. |
| Écritures | Chaque modification de données et son entrée d'audit sont écrites ensemble ou pas du tout (point de sauvegarde SQLite) : un disque plein ou une coupure ne laisse ni dossier à moitié effacé ni modification hors journal. |
| Application de bureau | Serveur embarqué à l'écoute de la seule boucle locale, port aléatoire. La fenêtre ne peut naviguer vers aucune autre adresse et n'a accès à aucune commande native. |
| TLS | TLS 1.3 uniquement. **Autorité locale du cabinet** créée au premier démarrage : elle émet le certificat du serveur (397 jours, renouvelé automatiquement 30 jours avant l'échéance, sans rien réinstaller sur les appareils). Elle porte une **contrainte de noms** (RFC 5280) qui la limite aux noms du réseau local (`.lan`, `.local`, `.home.arpa`, `.internal`, `localhost` et les noms configurés) et aux plages privées : même volée, sa clé ne permettrait d'usurper aucun site public (vérifié par les tests). L'appairage se fait par QR code ; l'utilisateur compare l'empreinte de l'autorité sur les deux écrans avant d'installer le profil (iOS) ou le certificat (Android). |

### Biométrie et RGPD

L'empreinte ou le visage sont vérifiés **par l'appareil lui-même**. Phonalys ne reçoit qu'une signature cryptographique et le drapeau « utilisateur vérifié ». **Aucune donnée biométrique n'est transmise ni stockée.** On évite ainsi un traitement de données biométriques au sens de l'article 9 du RGPD.

Ce drapeau signifie « vérifié par biométrie **ou** par le code PIN de l'appareil » : le protocole ne permet pas de distinguer les deux. Pour imposer une biométrie stricte, il faudra restreindre les modèles acceptés (AAGUID), par exemple à une YubiKey Bio. C'est une option prévue.

**Contrainte technique :** WebAuthn exige un nom de domaine (pas une adresse IP) et un certificat reconnu par l'appareil. Sur le réseau local, il faut donc un nom comme `phonalys.lan` et l'installation de l'autorité de certification Phonalys sur chaque appareil, ce que fait l'appairage par QR code. `localhost` fonctionne directement.

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
- **Verrouillage automatique** après inactivité (15 min par défaut, réglable) et à la sortie de veille. Après un verrouillage automatique, l'écran en cours reste en mémoire, masqué, 30 min au plus, pour ne pas perdre une saisie ; « Verrouiller » efface tout immédiatement.
- Le **coffre documentaire** exige une nouvelle authentification forte, valable 5 min pour la session concernée.
- Après 4 échecs de connexion, la connexion est bloquée 15 min (puis 15 min à chaque nouvel échec), et chaque tentative est inscrite à l'audit.

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

Écrire à **phonalys@proton.me** (Proton Mail : chiffrement de bout en bout avec une autre adresse Proton). Merci de ne pas publier la faille avant son correctif. Nous nous engageons à répondre sous 72 h.
