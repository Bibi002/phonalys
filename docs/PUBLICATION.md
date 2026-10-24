# Publier une version

Le workflow « Publication » (`.github/workflows/release.yml`) construit :

| Fichier | Contenu |
|---|---|
| `.msi`, `-setup.exe` | Application de bureau Windows (serveur embarqué) |
| `.deb`, `.rpm`, `.AppImage` | Application de bureau Linux |
| `phonalys-server-X.Y.Z-windows-x64.zip`, `…-linux-x64.tar.gz` | Serveur autonome du cabinet, pour les téléphones et tablettes |
| `SHA256SUMS.txt` | Empreintes de tous les fichiers |

Il crée un **brouillon** de release : rien n'est visible tant que vous ne l'avez pas relu et publié.

## Étapes

1. Mettre la même version dans `Cargo.toml` (`[workspace.package]`) et `apps/desktop/tauri.conf.json`. Le workflow refuse une étiquette qui ne correspond pas.
2. Committer, puis créer et pousser l'étiquette :
   ```
   git tag v0.1.0
   git push origin v0.1.0
   ```
3. Suivre le workflow dans l'onglet *Actions*, puis relire le brouillon dans *Releases* et le publier.

Pour un essai sans rien publier : *Actions › Publication › Run workflow*. Les fichiers sont alors téléchargeables comme artefacts du workflow.

## Signature des installateurs Windows (à faire par le titulaire)

Sans signature, Windows SmartScreen affiche « Windows a protégé votre ordinateur ». Pour signer :

1. Acheter un certificat de signature de code (OV ou EV) au nom du titulaire. Le fournisseur vérifie l'identité ; Phonalys ne peut pas le faire à votre place.
2. Exporter le certificat en `.pfx`, protégé par un mot de passe fort. Ne jamais le committer : `.gitignore` refuse déjà les fichiers `*.pfx`.
3. Déposer vous-même les deux secrets du dépôt, sans que personne d'autre ne voie le fichier ni le mot de passe :
   ```
   [Convert]::ToBase64String([IO.File]::ReadAllBytes("C:\chemin\certificat.pfx")) | gh secret set WINDOWS_CERTIFICATE
   gh secret set WINDOWS_CERTIFICATE_PASSWORD
   ```
   (la seconde commande demande le mot de passe sans l'afficher).
4. Supprimer la copie locale du `.pfx` si elle n'est pas conservée dans un coffre.

L'exécutable et les deux installateurs sont alors signés et horodatés à chaque publication. Le certificat est effacé de la machine de construction à la fin du travail.

Les certificats EV récents sont souvent livrés sur une clé matérielle ou dans un service de signature en ligne, et ne s'exportent pas en `.pfx`. Dans ce cas, il faut adapter `.github/scripts/sign-windows.ps1` à l'outil du fournisseur.

## Ce qui reste à faire

- **Mises à jour automatiques** : pas encore intégrées, volontairement. Il vous faut d'abord trancher deux points :
  1. **Où publier les mises à jour.** Le dépôt est privé : les postes des clients ne peuvent pas lire ses releases. Il faut une adresse publique en HTTPS (un site à vous, ou un second dépôt public ne contenant que les installateurs et le fichier `latest.json`).
  2. **Automatique ou manuel.** Pour un logiciel qui garde des dossiers de santé, une mise à jour silencieuse est un risque autant qu'une protection : si votre compte de publication est compromis, tous les cabinets reçoivent le code de l'attaquant. Proposition : l'application **signale** la nouvelle version et le praticien choisit quand l'installer.

  Une fois ces choix faits, il restera à générer la paire de clés de signature des mises à jour (`cargo tauri signer generate`) : vous la générez, la clé privée va seulement dans un secret du dépôt, la clé publique dans `tauri.conf.json`. Ensuite, le plugin `tauri-plugin-updater` peut être branché.
- **Paquets Linux** : non signés. Vérifier l'empreinte avec `sha256sum -c SHA256SUMS.txt`.
- **Attestation de provenance (SLSA)** : produite automatiquement si le dépôt devient public. Pour un dépôt privé, elle exige GitHub Enterprise Cloud.
