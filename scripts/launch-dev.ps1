param(
  [switch]$SmokeTest
)

$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
$PackageJson = Join-Path $RepoRoot "package.json"
$TauriConfig = Join-Path $RepoRoot "src-tauri\tauri.conf.json"

function Write-LauncherLine {
  param([string]$Message)
  Write-Host "[OneEpis Local Agent] $Message"
}

if (-not (Test-Path -LiteralPath $PackageJson)) {
  throw "No se encontro package.json en $RepoRoot"
}

if (-not (Test-Path -LiteralPath $TauriConfig)) {
  throw "No se encontro src-tauri\tauri.conf.json en $RepoRoot"
}

$NpmCommand = Get-Command npm.cmd -ErrorAction SilentlyContinue
if (-not $NpmCommand) {
  $NodeNpm = "C:\Program Files\nodejs\npm.cmd"
  if (Test-Path -LiteralPath $NodeNpm) {
    $NpmCommand = Get-Item -LiteralPath $NodeNpm
  }
}

if (-not $NpmCommand) {
  throw "No se encontro npm.cmd. Instala Node.js o agrega npm al PATH antes de abrir el acceso directo."
}

Set-Location -LiteralPath $RepoRoot
Write-LauncherLine "Repo: $RepoRoot"
Write-LauncherLine "npm: $($NpmCommand.Source)"

if ($SmokeTest) {
  Write-LauncherLine "SmokeTest OK"
  exit 0
}

Write-LauncherLine "Iniciando npm run dev..."
& $NpmCommand.Source run dev
