# Publier une version

Le workflow « Publication » (`.github/workflows/release.yml`) construit :

| Fichier | Contenu |
|---|---|
| `.msi`, `-setup.exe` | Application de bureau Windows (serveur embarqué) |
| `.deb`, `.rpm`, `.AppImage` | Application de bureau Linux |
| `phonalys-server-X.Y.Z-windows-x64.zip`, `…-linux-x64.tar.gz` | Serveur autonome du cabinet, pour les téléphones et tablettes |
| `SHA256SUMS.txt` | Empreintes de tous les fichiers |
| `*.sig`, `latest.json` | Signatures et manifeste des mises à jour (si la clé est configurée) |

Il crée un **brouillon** de release : rien n'est visible tant que vous ne l'avez pas relu et publié. Si la clé de mise à jour est configurée, il produit aussi les signatures `.sig` et le manifeste `latest.json`.

## Étapes

1. Mettre la même version dans `Cargo.toml` (`[workspace.package]`) et `apps/desktop/tauri.conf.json`. Le workflow refuse une étiquette qui ne correspond pas.
2. Committer, puis créer et pousser l'étiquette :
   ```
   git tag v0.1.0
   git push origin v0.1.0
   ```
3. Suivre le workflow dans l'onglet *Actions*, puis relire le brouillon dans *Releases* et le publier.

Pour un essai sans rien publier : *Actions › Publication › Run workflow*. Les fichiers sont alors téléchargeables comme artefacts du workflow.

## Diffusion publique, signature et mises à jour

Le code source reste dans le dépôt **privé** `Bibi002/phonalys`. Les installateurs sont publiés dans le dépôt **public** `Bibi002/phonalys-releases` (présentation, licence, guide, politique de sécurité, aucun code source). Lien à donner aux praticiens : https://github.com/Bibi002/phonalys-releases/releases/latest

Une fois la version publiée dans le dépôt privé, tout s'enchaîne automatiquement :

1. **Diffusion publique** (`.github/workflows/diffusion.yml`) : copie de la version vers le dépôt public, après vérification des empreintes.
2. **Signature** (workflow `signer.yml` du dépôt public) : signature Sigstore de `SHA256SUMS.txt` et attestation de provenance GitHub pour chaque fichier. C'est gratuit, sans limite, et aucun secret n'est à conserver.
3. **Mises à jour** : le fichier `latest.json` de la version est lu au démarrage par les applications installées, qui proposent d'installer la nouvelle version.

**Console** : ouvrez `outils/console.html` dans un navigateur. Elle affiche les versions, les téléchargements par version et par système, l'état des signatures et des mises à jour, ainsi que le nombre de vérifications de mise à jour (compteur anonyme de GitHub, sans aucun pistage).

### Mise en place (une seule fois, par le titulaire)

Ces trois secrets ne doivent être vus par personne d'autre que vous. Tapez les commandes dans **votre** terminal PowerShell (pas dans une conversation).

**1. Clé de signature des mises à jour.** Choisissez une phrase de passe quand elle est demandée et conservez-la dans votre gestionnaire de mots de passe :

```powershell
New-Item -ItemType Directory -Force "$env:USERPROFILE\.phonalys" | Out-Null
npx --yes @tauri-apps/cli@2.12.1 signer generate -w "$env:USERPROFILE\.phonalys\mises-a-jour.key"
Get-Content "$env:USERPROFILE\.phonalys\mises-a-jour.key" -Raw | gh secret set TAURI_SIGNING_PRIVATE_KEY -R Bibi002/phonalys
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD -R Bibi002/phonalys
```

Gardez une copie de `mises-a-jour.key` **hors ligne** (clé USB rangée en lieu sûr). Si elle est perdue, les applications installées ne pourront plus être mises à jour automatiquement. Si elle est volée, quelqu'un pourrait signer une fausse mise à jour : changez-la et publiez une nouvelle version au plus vite. La clé **publique** (`mises-a-jour.key.pub`) n'est pas secrète : elle va dans `apps/desktop/tauri.conf.json` (`plugins.updater.pubkey`).

**2. Jeton de diffusion vers le dépôt public.** Créez un jeton sur https://github.com/settings/personal-access-tokens/new :
- *Repository access* : **Only select repositories** › `phonalys-releases` ;
- *Permissions* › *Repository permissions* › **Contents : Read and write** (rien d'autre) ;
- une date d'expiration, par exemple un an, à renouveler.

```powershell
gh secret set PUBLIC_RELEASES_TOKEN -R Bibi002/phonalys
```

Si le jeton est absent ou a expiré, le workflow « Diffusion publique » échoue sans rien publier ; il se relance à la main (*Actions › Diffusion publique › Run workflow*).

### Copie manuelle (secours)

```powershell
$v = "0.1.3"; $d = "$env:TEMP\phonalys-$v"
gh release download "v$v" -R Bibi002/phonalys -D $d
Get-Content "$d\SHA256SUMS.txt" | % { $h, $n = $_ -split '\s+', 2; if ((Get-FileHash "$d\$n").Hash -ne $h) { throw "Empreinte différente : $n" } }
Copy-Item docs\INSTALLATION.md $d
gh release create "v$v" -R Bibi002/phonalys-releases --title "Phonalys $v" --notes-file notes.md --latest (Get-ChildItem $d -File).FullName
```

### Microsoft Store (facultatif)

Le Store signe lui-même l'application (paquet MSIX) : c'est la seule façon gratuite de supprimer l'avertissement SmartScreen. Il faut un compte développeur Microsoft (gratuit pour un particulier, avec vérification d'identité) : https://storedeveloper.microsoft.com. Une fois le compte créé, relevez dans *Partner Center › Gestion du produit › Identité du produit* les valeurs **Package/Identity/Name** et **Package/Identity/Publisher** : elles sont nécessaires pour produire le paquet MSIX.

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

- **Mises à jour des paquets `.deb` et `.rpm` et du serveur du cabinet** : manuelles (installer la nouvelle version par-dessus). L'AppImage et les installateurs Windows se mettent à jour depuis l'application.
- **Paquets Linux** : non signés. Vérifier l'empreinte avec `sha256sum -c SHA256SUMS.txt`.
- **Attestation de provenance (SLSA)** : elle n'est produite que si le dépôt qui construit est public (ou avec GitHub Enterprise Cloud). Le code restant privé, les empreintes SHA-256 font foi.
