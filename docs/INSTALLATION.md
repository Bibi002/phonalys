# Installer Phonalys

Guide destiné au praticien. Phonalys fonctionne entièrement chez vous : aucun compte en ligne, aucune donnée envoyée à l'éditeur.

Remplacez `<version>` par le numéro de la version téléchargée (par exemple `0.1.2`).

## 1. Choisir son installation

| Situation | À télécharger |
|---|---|
| **Un seul ordinateur** (Windows) | `Phonalys_<version>_x64-setup.exe` (ou `Phonalys_<version>_x64_en-US.msi` pour un déploiement géré) |
| **Un seul ordinateur** (Linux) | `.deb` (Debian, Ubuntu), `.rpm` (Fedora, openSUSE) ou `.AppImage` (toutes distributions) |
| **Ordinateur et téléphones ou tablettes** | En plus : le serveur du cabinet, `phonalys-server-<version>-windows-x64.zip` ou `phonalys-server-<version>-linux-x64.tar.gz`, sur la machine qui reste allumée au cabinet |

L'application de bureau seule n'est joignable que depuis l'ordinateur où elle est installée. Pour utiliser un téléphone ou une tablette, il faut le serveur du cabinet (section 4).

## 2. Vérifier le fichier téléchargé

Avant d'installer, comparez l'empreinte du fichier avec celle de `SHA256SUMS.txt`, publié avec la version.

- Windows (PowerShell) : `Get-FileHash .\Phonalys_<version>_x64-setup.exe`
- Linux : `sha256sum -c SHA256SUMS.txt --ignore-missing`

Si l'empreinte diffère, **n'installez pas** le fichier.

> **Versions actuelles non signées.** Windows affichera « Windows a protégé votre ordinateur » : cliquez sur « Informations complémentaires », puis « Exécuter quand même », **seulement** si l'empreinte est correcte. Une version signée suivra.

## 3. Premier démarrage

1. Lancez Phonalys. L'écran « Installation de Phonalys » s'ouvre ; dans l'application de bureau, le jeton d'installation est déjà rempli.
2. Choisissez une **phrase de passe** d'au moins 14 caractères (idéalement 5 mots) et **notez-la sur papier**, rangée en lieu sûr. **Sans elle, vos données sont définitivement illisibles** : il n'existe aucune réinitialisation, et les codes de récupération ne la remplacent pas.
3. Scannez le QR code avec une application d'authentification hors ligne (Aegis, 2FAS, Ente Auth, Bitwarden, KeePassXC).
4. **Imprimez ou notez les codes de récupération** et rangez-les hors ligne, loin de l'ordinateur. Ils remplacent le code à 6 chiffres ou un appareil perdu, toujours **avec** la phrase de passe.
5. Connectez-vous, puis dans « Sécurité », activez la connexion biométrique (empreinte, visage, Windows Hello).
6. Dans « Réglages › Cabinet », renseignez vos coordonnées : elles apparaissent sur les bilans et dans les documents de l'écran « Conformité ».

Vous avez déjà une sauvegarde (nouvel ordinateur) ? À l'écran d'installation, choisissez « Restaurer cette sauvegarde » au lieu de créer un coffre.

## 4. Serveur du cabinet (téléphones et tablettes)

Le serveur s'installe sur la machine qui reste allumée au cabinet (PC fixe, mini-PC, serveur domestique). Sur cette machine, utilisez Phonalys **dans le navigateur** (`https://phonalys.lan:8443`) et non l'application de bureau : un même coffre ne peut être ouvert que par un seul programme à la fois, et Phonalys refuse un second lancement.

1. Dans la box ou le routeur, donnez un nom à cette machine, par exemple `phonalys.lan`, et fixez son adresse IP.
2. Décompressez l'archive dans un dossier, par exemple `C:\Phonalys` ou `/opt/phonalys`.
3. Démarrez le serveur :

   **Windows** (PowerShell) :
   ```powershell
   $env:PHONALYS_BIND = "0.0.0.0:8443"
   $env:PHONALYS_PUBLIC_ORIGIN = "https://phonalys.lan:8443"
   C:\Phonalys\phonalys-server.exe
   ```
   Autorisez le programme dans le pare-feu, **pour les réseaux privés uniquement**.

   **Linux** : un service systemd, par exemple `/etc/systemd/system/phonalys.service` :
   ```ini
   [Unit]
   Description=Phonalys
   After=network-online.target

   [Service]
   User=phonalys
   Environment=PHONALYS_BIND=0.0.0.0:8443
   Environment=PHONALYS_PUBLIC_ORIGIN=https://phonalys.lan:8443
   Environment=PHONALYS_DATA_DIR=/var/lib/phonalys
   ExecStart=/opt/phonalys/phonalys-server
   Restart=on-failure
   NoNewPrivileges=true
   ProtectSystem=strict
   ReadWritePaths=/var/lib/phonalys

   [Install]
   WantedBy=multi-user.target
   ```
   puis `sudo useradd -r phonalys && sudo mkdir -p /var/lib/phonalys && sudo chown phonalys /var/lib/phonalys && sudo systemctl enable --now phonalys`.
4. Au premier démarrage, la console (ou `journalctl -u phonalys`) affiche le **jeton d'installation** et l'**empreinte de l'autorité du cabinet**. Ouvrez `https://phonalys.lan:8443` depuis l'ordinateur et suivez la section 3.
5. Pour chaque téléphone : « Sécurité › Téléphones et tablettes », scannez le QR code, **vérifiez que l'empreinte est identique des deux côtés**, installez le profil (iPhone, iPad) ou le certificat (Android), puis ajoutez Phonalys à l'écran d'accueil.

N'ouvrez jamais le port 8443 vers Internet. Pour un accès à distance, utilisez un VPN (WireGuard, par exemple).

## Mises à jour

Au démarrage, l'application de bureau vérifie s'il existe une nouvelle version sur la page publique des versions. Si c'est le cas, elle le signale : **rien ne s'installe sans votre accord** (« Installer maintenant » ou « Plus tard »). La mise à jour est vérifiée par signature avant l'installation, puis Phonalys redémarre ; vos données ne sont pas touchées.

Cette vérification n'envoie aucune information sur le cabinet. Pour la désactiver (poste géré par un informaticien), créez un fichier vide nommé `mises-a-jour-desactivees` dans le dossier de données (`%LOCALAPPDATA%\Phonalys` ou `~/.local/share/Phonalys`).

Les paquets `.deb` et `.rpm` et le serveur du cabinet se mettent à jour en installant la nouvelle version par-dessus l'ancienne. Les versions 0.1.0 et 0.1.1 ne savent pas encore se mettre à jour : installez une fois la version suivante à la main.

## 5. Sauvegardes

Une sauvegarde chiffrée est créée automatiquement chaque jour d'utilisation et testée aussitôt. Par défaut, elle est sur **le même disque** que le coffre : elle ne protège ni d'une panne du disque, ni d'un vol. Dans « Sauvegardes », indiquez un **second dossier** (clé USB, disque externe, NAS) ; tant que ce n'est pas fait, l'accueil l'affiche en alerte. Une sauvegarde s'ouvre avec la phrase de passe en vigueur au moment où elle a été faite.

Faites au moins une fois l'essai complet : restaurer une sauvegarde sur un autre ordinateur, avant d'en avoir besoin.

## 6. Assistant de rédaction (facultatif)

Installez [Ollama](https://ollama.com) sur l'ordinateur, téléchargez un modèle (par exemple `ollama pull qwen2.5:7b`), puis activez l'assistant dans « Réglages ». Les IA en ligne sont bloquées par Phonalys.

## 7. Conformité

L'écran « Conformité » génère le registre des traitements, la notice à remettre aux patients, une trame d'analyse d'impact et le registre des violations. Ce sont des modèles à relire et à faire valider.

## En cas de problème

- **« Serveur injoignable »** : vérifiez que le serveur tourne et que l'appareil est sur le réseau du cabinet.
- **Code à 6 chiffres ou téléphone perdu** : connectez-vous avec la phrase de passe et un code de récupération, puis générez-en de nouveaux (« Sécurité »).
- **Phrase de passe oubliée** : aucune récupération n'est possible, ni par les codes, ni par l'éditeur. C'est le prix d'un chiffrement que personne d'autre ne peut ouvrir : d'où la phrase notée sur papier, en lieu sûr.
- **Phrase de passe peut-être connue d'un tiers** : « Sécurité › Changer la phrase de passe ».
- **« Phonalys est déjà ouvert »** : une autre fenêtre de Phonalys ou le serveur du cabinet tourne déjà sur cet ordinateur ; fermez-le, ou utilisez le navigateur.
- **Ordinateur ou téléphone perdu** : suivez « Conformité › Violation de données ».
- **Faille de sécurité ou comportement suspect** : écrivez à phonalys@proton.me, sans publier les détails avant le correctif.
