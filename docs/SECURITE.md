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
| Récupération | 10 codes à usage unique de 80 bits. Seules leurs empreintes sont conservées. |
| Échecs | Délai exponentiel à partir du 4e échec, plafonné à 15 min. Un second facteur faux compte comme un échec. Les essais sont traités un par un. |
| Sessions | Jeton de 256 bits, dont seule l'empreinte reste en mémoire. Cookie `__Host-`, `Secure`, `HttpOnly`, `SameSite=Strict`. Expiration après 5 min d'inactivité ou 8 h au total. |
| Verrouillage | Sans session active, les clés sont effacées de la mémoire. `/api/lock` verrouille immédiatement tous les appareils. |
| CSRF | Les requêtes qui modifient l'état exigent un en-tête dédié et une origine identique à celle du serveur. |
| TLS | TLS 1.3 uniquement. Certificat auto-signé dont l'empreinte SHA-256 est à épingler. |

### Biométrie et RGPD

L'empreinte ou le visage sont vérifiés **par l'appareil lui-même**. Phonalys ne reçoit qu'une signature cryptographique et le drapeau « utilisateur vérifié ». **Aucune donnée biométrique n'est transmise ni stockée.** On évite ainsi un traitement de données biométriques au sens de l'article 9 du RGPD.

Ce drapeau signifie « vérifié par biométrie **ou** par le code PIN de l'appareil » : le protocole ne permet pas de distinguer les deux. Pour imposer une biométrie stricte, il faudra restreindre les modèles acceptés (AAGUID), par exemple à une YubiKey Bio. C'est une option prévue.

**Contrainte technique :** WebAuthn exige un nom de domaine (pas une adresse IP) et un certificat reconnu par l'appareil. Sur le réseau local, il faut donc un nom comme `phonalys.lan` et l'installation de l'autorité de certification Phonalys sur chaque appareil. Ce sera géré par l'appairage en phase 3. `localhost` fonctionne directement.

### Limites, à connaître honnêtement

- **Le second facteur (TOTP ou biométrie) est un contrôle d'accès, pas encore un facteur cryptographique.** Quelqu'un qui volerait *le fichier du coffre* **et** la phrase de passe pourrait le déchiffrer hors ligne. La parade, prochaine étape, est l'**extension PRF de WebAuthn** : la clé du coffre dépendra alors aussi de l'authentificateur physique, et le vol du fichier et de la phrase de passe ne suffira plus.
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
