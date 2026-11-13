<#
.SYNOPSIS
    Pubblica on_doc/ su GitHub Pages usando il branch gh-pages.

.DESCRIPTION
    on_doc/ e' ignorata da git nel branch main (vedi .gitignore), quindi GitHub
    Pages non puo' servire la guida dal main: la guida vive solo in locale.

    Questo script crea un commit "orfano" (senza storia) con il solo contenuto
    di on_doc/ - index.html, style.css, img/, .nojekyll - e lo invia con un
    push forzato al branch gh-pages. main resta intatto: nessun file della
    documentazione entra nella storia del codice.

.PARAMETER Branch
    Branch di pubblicazione. Default: gh-pages

.PARAMETER Remote
    Remote git da usare. Default: origin

.PARAMETER NoPush
    Prepara il commit e mostra i file, senza eseguire il push.

.EXAMPLE
    .\deploy-docs.cmd

.EXAMPLE
    .\deploy-docs.cmd -NoPush
#>
[CmdletBinding()]
param(
    [string]$Branch = 'gh-pages',
    [string]$Remote = 'origin',
    [switch]$NoPush
)

$ErrorActionPreference = 'Stop'

$repo = $PSScriptRoot
$docs = Join-Path $repo 'on_doc'
$gitDir = Join-Path $repo '.git'

if (-not (Test-Path $gitDir)) { throw "Non e' un repository git: $repo" }
if (-not (Test-Path (Join-Path $docs 'index.html'))) { throw "Guida non trovata: $docs\index.html" }

function Invoke-GitDocs([string[]]$GitArgs) {
    Push-Location $docs
    try {
        $output = & git "--git-dir=$gitDir" "--work-tree=$docs" @GitArgs
    } finally {
        Pop-Location
    }
    if ($LASTEXITCODE -ne 0) {
        throw ("git " + ($GitArgs -join ' ') + " non riuscito")
    }
    return $output
}

$indexFile = Join-Path $gitDir ('docs-index-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
Write-Host ''
Write-Host "==> preparo il contenuto di on_doc per $Branch" -ForegroundColor Cyan

$commit = $null
try {
    $env:GIT_INDEX_FILE = $indexFile
    Invoke-GitDocs @('read-tree', '--empty') | Out-Null
    Invoke-GitDocs @('add', '-A', '-f', '.') | Out-Null

    # .nojekyll: evita l'elaborazione Jekyll di GitHub Pages.
    $nojekyllBlob = ('pagina statica' | & git "--git-dir=$gitDir" hash-object -w --stdin)
    Invoke-GitDocs @('update-index', '--add', '--cacheinfo', "100644,$($nojekyllBlob.Trim()),.nojekyll") | Out-Null

    $tree = ((Invoke-GitDocs @('write-tree')) -join '').Trim()
    $commit = ((Invoke-GitDocs @('commit-tree', $tree, '-m', 'docs: guida utente Pixel Studio')) -join '').Trim()
} finally {
    $env:GIT_INDEX_FILE = $null
    Remove-Item -LiteralPath $indexFile -Force -ErrorAction SilentlyContinue
}

$files = Invoke-GitDocs @('ls-tree', '-r', '--name-only', $commit)
Write-Host "    commit $commit" -ForegroundColor Gray
Write-Host '    file pubblicati:' -ForegroundColor Gray
foreach ($file in $files) { Write-Host "      $file" -ForegroundColor Gray }

if ($NoPush) {
    Write-Host ''
    Write-Host 'Anteprima: nessun push eseguito.' -ForegroundColor Yellow
    Write-Host 'Per pubblicare davvero: .\deploy-docs.cmd' -ForegroundColor Yellow
    exit 0
}

Write-Host ''
Write-Host "==> push verso $Remote/$Branch" -ForegroundColor Cyan
& git "--git-dir=$gitDir" --work-tree=$repo push --force $Remote "$($commit):refs/heads/$Branch"
if ($LASTEXITCODE -ne 0) {
    throw "push rifiutato: verifica di avere accesso in scrittura a '$Remote'"
}

$remoteUrl = (& git "--git-dir=$gitDir" --work-tree=$repo remote get-url $Remote).Trim()
if ($remoteUrl -match 'github\.com[:/](?<owner>[^/]+)/(?<name>[^/.]+)') {
    $owner = $Matches['owner'].ToLower()
    $name = $Matches['name']
    Write-Host ''
    Write-Host "==> Fatto. Guida online: https://$owner.github.io/$name/" -ForegroundColor Green
    Write-Host '    (la prima volta abilita Settings > Pages > Deploy from a branch > gh-pages / root)' -ForegroundColor DarkGray
}
