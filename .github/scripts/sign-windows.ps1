# Signature Authenticode d'un fichier produit par `cargo tauri build` (appelé par
# Tauri pour l'exécutable et pour chaque installateur). Le certificat et son mot de
# passe viennent des secrets du dépôt, jamais du code.
param([Parameter(Mandatory = $true)][string]$File)

$ErrorActionPreference = "Stop"
if (-not $env:PHONALYS_PFX -or -not (Test-Path $env:PHONALYS_PFX)) { throw "certificat de signature absent" }

$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" |
  Sort-Object FullName | Select-Object -Last 1
if (-not $signtool) { throw "signtool introuvable" }

& $signtool.FullName sign /fd sha256 /td sha256 /tr http://timestamp.digicert.com `
  /f $env:PHONALYS_PFX /p $env:WINDOWS_CERTIFICATE_PASSWORD $File
if ($LASTEXITCODE -ne 0) { throw "échec de la signature de $File" }
