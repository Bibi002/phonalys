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

## Diffusion publique

Le code source reste dans le dépôt **privé** `Bibi002/phonalys`. Les installateurs sont rendus publics dans un dépôt séparé, **`Bibi002/phonalys-releases`** (page de présentation, licence, guide, politique de sécurité, sans aucun code source). Lien à donner aux praticiens : https://github.com/Bibi002/phonalys-releases/releases/latest

Après avoir publié une version dans le dépôt privé, copiez-la vers le dépôt public en vérifiant les empreintes :

```powershell
$v = "0.1.1"; $d = "$env:TEMP\phonalys-$v"
gh release download "v$v" -R Bibi002/phonalys -D $d -p "Phonalys*" -p "phonalys-server*" -p "SHA256SUMS.txt"
# Chaque fichier doit correspondre à SHA256SUMS.txt :
Get-Content "$d\SHA256SUMS.txt" | % { $h, $n = $_ -split '\s+', 2; if ((Get-FileHash "$d\$n").Hash -ne $h) { throw "Empreinte différente : $n" } }
Copy-Item docs\INSTALLATION.md $d
gh release create "v$v" -R Bibi002/phonalys-releases --title "Phonalys $v" --notes-file notes.md --latest (Get-ChildItem $d -File).FullName
```

## Signature des installateurs Windows (à faire par le titulaire)

Sans signature, Windows SmartScreen affiche « Windows a protégé votre ordinateur ». La signature prouve que l'installateur vient bien de vous et n'a pas été modifié. Le certificat est délivré **au nom d'une personne ou d'une entreprise vérifiée** : seul le titulaire peut le demander.

**Depuis juin 2023, la clé privée d'un certificat de signature de code doit rester dans un module matériel** (clé USB cryptographique ou service de signature en ligne). Un fichier `.pfx` téléchargeable n'est donc plus délivré par les autorités reconnues. Pour signer depuis GitHub Actions, il faut un **service de signature en ligne**. Options à comparer (prix et conditions à vérifier au moment de l'achat) :

| Service | Principe | À vérifier |
|---|---|---|
| **Azure Artifact Signing** (Microsoft, ex-Trusted Signing) | Abonnement mensuel ; Microsoft vérifie l'identité ; action GitHub officielle | Ouverture aux **personnes physiques** et aux entreprises **en Belgique** |
| **Certum** (autorité polonaise, UE) avec **SimplySign** | Certificat OV, clé dans le nuage Certum | Outil de signature utilisable sur un agent GitHub |
| **SSL.com** avec **eSigner** | Certificat OV ou EV, signature en ligne, action GitHub | Coût annuel et coût par signature |

Une fois le service choisi :

1. Demandez le certificat à votre nom (« M. Baron ») ou au nom de votre entreprise, et passez la vérification d'identité.
2. Déposez **vous-même** les identifiants du service dans les secrets du dépôt (`gh secret set NOM`, la valeur est demandée sans être affichée). Ne les transmettez à personne, Claude compris.
3. Demandez ensuite l'adaptation de `.github/scripts/sign-windows.ps1` au service choisi. Le workflow appelle déjà ce script pour l'exécutable et les deux installateurs, ou saute la signature si aucun secret n'est présent.

Le mode actuel par fichier `.pfx` (secrets `WINDOWS_CERTIFICATE` et `WINDOWS_CERTIFICATE_PASSWORD`) ne sert que pour un certificat dont la clé est exportable, par exemple un certificat interne d'entreprise.

**À savoir :** même signé, un nouvel éditeur peut voir l'avertissement SmartScreen pendant quelques semaines, le temps que la réputation du certificat s'établisse auprès de Microsoft.

## Ce qui reste à faire

- **Mises à jour automatiques** : pas encore intégrées, volontairement. Il vous faut d'abord trancher deux points :
  1. **Où publier les mises à jour.** Le dépôt public `Bibi002/phonalys-releases` peut servir d'adresse : il faudra y publier aussi le fichier `latest.json` produit par Tauri.
  2. **Automatique ou manuel.** Pour un logiciel qui garde des dossiers de santé, une mise à jour silencieuse est un risque autant qu'une protection : si votre compte de publication est compromis, tous les cabinets reçoivent le code de l'attaquant. Proposition : l'application **signale** la nouvelle version et le praticien choisit quand l'installer.

  Une fois ces choix faits, il restera à générer la paire de clés de signature des mises à jour (`cargo tauri signer generate`) : vous la générez, la clé privée va seulement dans un secret du dépôt, la clé publique dans `tauri.conf.json`. Ensuite, le plugin `tauri-plugin-updater` peut être branché.
- **Paquets Linux** : non signés. Vérifier l'empreinte avec `sha256sum -c SHA256SUMS.txt`.
- **Attestation de provenance (SLSA)** : elle n'est produite que si le dépôt qui construit est public (ou avec GitHub Enterprise Cloud). Le code restant privé, les empreintes SHA-256 font foi.
