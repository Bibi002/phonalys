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

- Une **phrase de passe longue** (5 mots ou plus) est obligatoire, et vérifiée contre une liste de mots de passe compromis embarquée hors ligne.
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
