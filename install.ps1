<#
.SYNOPSIS
    Installer di Pixel Studio per Windows (per-utente, senza privilegi admin).

.DESCRIPTION
    Scarica e installa Rust (rustup, toolchain stable) se non e' presente,
    compila il workspace in modalita release e installa l'editor e il server
    MCP in %LOCALAPPDATA%\Programs\Pixel Studio.

    Segue le convenzioni degli installer Windows per-utente:
      * nessuna elevazione (HKCU + %LOCALAPPDATA%)
      * scorciatoia nel menu Start e (opzionale) sul desktop
      * voce in "App e funzionalita" con UninstallString / QuietUninstallString
      * disinstallatore generato accanto ai binari
      * log completo in %TEMP%\pixel-studio-install-*.log

.PARAMETER InstallDir
    Cartella di installazione. Default: %LOCALAPPDATA%\Programs\Pixel Studio

.PARAMETER NoDesktopShortcut
    Non crea la scorciatoia sul desktop (quella nel menu Start resta).

.PARAMETER SkipRust
    Non scarica Rust: usa il cargo gia' installato e fallisce se manca.

.EXAMPLE
    .\install.cmd

.EXAMPLE
    .\install.cmd -InstallDir "D:\Apps\Pixel Studio" -NoDesktopShortcut
#>
[CmdletBinding()]
param(
    [string]$InstallDir = '',
    [switch]$NoDesktopShortcut,
    [switch]$SkipRust
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$AppName = 'Pixel Studio'
$AppKey = 'PixelStudio'
$Publisher = 'Leon'
$RepoUrl = 'https://github.com/Lexy0825/Pixealrt-Sprite-generator'
$RepoRoot = $PSScriptRoot

function Write-Step([string]$Text) {
    Write-Host ''
    Write-Host "==> $Text" -ForegroundColor Cyan
}

function Write-Ok([string]$Text) {
    Write-Host "    [ok] $Text" -ForegroundColor Green
}

function Write-Info([string]$Text) {
    Write-Host "    $Text" -ForegroundColor Gray
}

function Write-Warn([string]$Text) {
    Write-Host "    [avviso] $Text" -ForegroundColor Yellow
}

function Fail([string]$Text) {
    Write-Host ''
    Write-Host "Errore: $Text" -ForegroundColor Red
    if ($script:LogPath) {
        Write-Host "Log completo: $script:LogPath" -ForegroundColor DarkGray
    }
    exit 1
}

# Trova cargo.exe: CARGO_HOME, installazione rustup standard, PATH, infine rustup.
function Find-Cargo {
    $candidates = @()
    if ($env:CARGO_HOME) { $candidates += (Join-Path $env:CARGO_HOME 'bin\cargo.exe') }
    $candidates += (Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe')
    foreach ($candidate in $candidates) {
        if ($candidate -and (Test-Path $candidate)) { return $candidate }
    }
    $onPath = Get-Command cargo -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }
    $rustup = Get-Command rustup -ErrorAction SilentlyContinue
    if ($rustup) {
        $fromRustup = & $rustup.Source which cargo 2>$null
        if ($LASTEXITCODE -eq 0 -and $fromRustup) {
            $resolved = "$fromRustup".Trim()
            if (Test-Path $resolved) { return $resolved }
        }
    }
    return $null
}

# --- log -------------------------------------------------------------------
$script:LogPath = Join-Path $env:TEMP ('pixel-studio-install-{0:yyyyMMdd-HHmmss}.log' -f (Get-Date))
try { Start-Transcript -Path $script:LogPath -Force | Out-Null } catch { $script:LogPath = $null }

Write-Host ''
Write-Host "  $AppName - installer per Windows" -ForegroundColor White
Write-Host '  ---------------------------------------------------' -ForegroundColor DarkGray

# --- 1. ambiente -----------------------------------------------------------
Write-Step 'Verifica ambiente'

if ([Environment]::OSVersion.Platform -ne 'Win32NT') {
    Fail 'questo installer funziona solo su Windows'
}
if (-not (Test-Path (Join-Path $RepoRoot 'Cargo.toml'))) {
    Fail "Cargo.toml non trovato: esegui l'installer dalla cartella del progetto"
}

$windowsBuild = [Environment]::OSVersion.Version.Build
if ($windowsBuild -lt 10240) {
    Write-Warn "Windows build ${windowsBuild}: sono supportate solo versioni da Windows 10 in poi"
} else {
    Write-Ok "Windows build $windowsBuild"
}

$processorArch = if ($env:PROCESSOR_ARCHITECTURE) { $env:PROCESSOR_ARCHITECTURE } else { 'AMD64' }
$rustTarget = if ($processorArch -eq 'ARM64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
Write-Ok "architettura $processorArch (target Rust $rustTarget)"

if (-not $InstallDir) {
    $localData = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { Join-Path $env:USERPROFILE 'AppData\Local' }
    $InstallDir = Join-Path $localData 'Programs\Pixel Studio'
}
$InstallDir = $InstallDir.TrimEnd('\')

if ($InstallDir -like '*\Program Files*') {
    Write-Warn 'la cartella scelta richiede i privilegi di amministratore: apri il prompt come amministratore'
}
Write-Ok "cartella di installazione: $InstallDir"

# --- 2. Rust ---------------------------------------------------------------
Write-Step 'Toolchain Rust'

$cargo = Find-Cargo

if ($cargo) {
    Write-Ok "cargo trovato: $cargo"
} else {
    if ($SkipRust) {
        Fail 'Rust non e'' installato e -SkipRust e'' attivo. Installa Rust da https://rustup.rs'
    }

    Write-Info 'Rust non trovato: scarico rustup (installazione per-utente, senza admin)'
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $rustupUrl = "https://static.rust-lang.org/rustup/dist/$rustTarget/rustup-init.exe"
    $rustupExe = Join-Path $env:TEMP 'rustup-init.exe'
    try {
        Invoke-WebRequest -Uri $rustupUrl -OutFile $rustupExe -UseBasicParsing
        $expected = (Invoke-WebRequest -Uri "$rustupUrl.sha256" -UseBasicParsing).Content
    } catch {
        Fail "download di rustup non riuscito ($($_.Exception.Message)). Controlla la connessione o installa Rust da https://rustup.rs"
    }

    if ($expected -match '([0-9a-fA-F]{64})') { $expected = $Matches[1].ToLower() } else { $expected = $null }
    $actual = (Get-FileHash -Path $rustupExe -Algorithm SHA256).Hash.ToLower()
    if ($expected -and $expected -ne $actual) {
        Fail 'checksum di rustup-init non valido: download interrotto o manomesso'
    }
    Write-Ok 'rustup-init scaricato e verificato (SHA256)'

    & $rustupExe -y --default-toolchain stable --profile minimal
    if ($LASTEXITCODE -ne 0) {
        Fail 'installazione di Rust non riuscita (vedi il log)'
    }

    # rustup aggiorna il PATH permanente: ricaricalo in questa sessione.
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
                [Environment]::GetEnvironmentVariable('Path', 'User')

    $cargo = Find-Cargo
    if (-not $cargo) {
        Fail "cargo non trovato dopo l'installazione di Rust: apri un nuovo terminale e rilancia install.cmd"
    }
    Write-Ok "Rust installato: $cargo"
}

# --- 3. linker MSVC --------------------------------------------------------
Write-Step 'Strumenti di compilazione C++ (MSVC)'

$vcToolsPath = $null
$programFilesX86 = ${env:ProgramFiles(x86)}
if ($programFilesX86) {
    $vswhere = Join-Path $programFilesX86 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (Test-Path $vswhere) {
        $vcToolsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    }
}

if ($vcToolsPath) {
    Write-Ok "Build Tools trovati: $vcToolsPath"
} else {
    Write-Warn 'Build Tools MSVC non rilevati: la compilazione potrebbe fallire'
    Write-Info 'Installa con:'
    Write-Info '  winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"'
}

# --- 4. compilazione -------------------------------------------------------
Write-Step 'Compilazione (cargo build --release)'
Write-Info "cartella: $RepoRoot"
Write-Info 'la prima compilazione puo'' richiedere alcuni minuti'

Push-Location $RepoRoot
& $cargo build --release -p pixel-ui -p pixel-mcp
$buildExit = $LASTEXITCODE
Pop-Location

if ($buildExit -ne 0) {
    Fail 'compilazione fallita (vedi log). Se manca MSVC installa i Build Tools come indicato sopra.'
}

$uiExe = Join-Path $RepoRoot 'target\release\pixel-ui.exe'
$mcpExe = Join-Path $RepoRoot 'target\release\pixel-mcp.exe'
foreach ($binary in @($uiExe, $mcpExe)) {
    if (-not (Test-Path $binary)) { Fail "binario mancante dopo la compilazione: $binary" }
}
Write-Ok 'pixel-ui.exe e pixel-mcp.exe compilati'

# --- 5. chiusura istanze ---------------------------------------------------
Write-Step 'Chiusura eventuali istanze in esecuzione'

$running = Get-Process -Name 'pixel-ui' -ErrorAction SilentlyContinue
if ($running) {
    foreach ($process in $running) {
        Write-Info "chiudo il processo $($process.Id)"
        $null = $process.CloseMainWindow()
        Start-Sleep -Milliseconds 800
        if (-not $process.HasExited) { $process.Kill() }
    }
} else {
    Write-Ok 'nessuna istanza aperta'
}

# --- 6. copia dei file -----------------------------------------------------
Write-Step "Installazione in $InstallDir"

New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
Copy-Item -LiteralPath $uiExe -Destination $InstallDir -Force
Copy-Item -LiteralPath $mcpExe -Destination $InstallDir -Force

$iconSource = Join-Path $RepoRoot 'crates\pixel-ui\assets\icon.ico'
if (Test-Path $iconSource) {
    Copy-Item -LiteralPath $iconSource -Destination (Join-Path $InstallDir 'icon.ico') -Force
}

$installedUi = Join-Path $InstallDir 'pixel-ui.exe'
$installedMcp = Join-Path $InstallDir 'pixel-mcp.exe'
Write-Ok 'binari copiati'

# --- 7. disinstallatore ----------------------------------------------------
$uninstallPath = Join-Path $InstallDir 'uninstall.ps1'
$uninstallScript = @'
# Disinstallatore di Pixel Studio (generato dall'installer).
# Si puo' usare da "App e funzionalita" oppure a mano:
#   powershell -ExecutionPolicy Bypass -File uninstall.ps1 [-KeepData] [-Quiet]
param([switch]$KeepData, [switch]$Quiet)

$ErrorActionPreference = 'SilentlyContinue'
$dir = $PSScriptRoot

if (-not $Quiet) {
    Write-Host ''
    Write-Host "Rimozione di Pixel Studio da $dir" -ForegroundColor Cyan
}

Get-Process -Name 'pixel-ui' -ErrorAction SilentlyContinue | ForEach-Object {
    $null = $_.CloseMainWindow()
    Start-Sleep -Milliseconds 800
    if (-not $_.HasExited) { $_.Kill() }
}

$dataDir = Join-Path $env:APPDATA 'PixelStudio'
$removeData = $false
if (-not $KeepData) {
    if ($Quiet) {
        $removeData = $true
    } elseif (Test-Path $dataDir) {
        $answer = Read-Host "Eliminare anche progetti recenti e anteprime in $dataDir? [s/N]"
        $removeData = ($answer -match '^(s|si|y|yes)$')
    }
}

$programs = [Environment]::GetFolderPath('Programs')
$desktop = [Environment]::GetFolderPath('Desktop')
Remove-Item -LiteralPath (Join-Path $programs 'Pixel Studio') -Recurse -Force
Remove-Item -LiteralPath (Join-Path $desktop 'Pixel Studio.lnk') -Force
Remove-Item -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\PixelStudio' -Recurse -Force
if ($removeData) { Remove-Item -LiteralPath $dataDir -Recurse -Force }

if (-not $Quiet) {
    Write-Host 'Rimozione completata.' -ForegroundColor Green
}

Remove-Item -LiteralPath $dir -Recurse -Force
if (Test-Path $dir) {
    # Cartella ancora in uso: rimozione differita tramite cmd.
    Start-Process -FilePath (Join-Path $env:SystemRoot 'System32\cmd.exe') `
        -ArgumentList @('/c', "ping -n 3 127.0.0.1 >nul & rmdir /s /q `"$dir`"") `
        -WindowStyle Hidden
}
'@
Set-Content -LiteralPath $uninstallPath -Value $uninstallScript -Encoding ASCII
Write-Ok 'disinstallatore scritto (uninstall.ps1)'

# --- 8. scorciatoie --------------------------------------------------------
Write-Step 'Scorciatoie'

$shell = New-Object -ComObject WScript.Shell
$startMenuDir = Join-Path ([Environment]::GetFolderPath('Programs')) $AppName
New-Item -ItemType Directory -Path $startMenuDir -Force | Out-Null

$startLink = $shell.CreateShortcut((Join-Path $startMenuDir "$AppName.lnk"))
$startLink.TargetPath = $installedUi
$startLink.WorkingDirectory = $InstallDir
$startLink.IconLocation = "$installedUi,0"
$startLink.Description = "$AppName - editor pixel-art e tileset"
$startLink.Save()
Write-Ok "menu Start: $startMenuDir\$AppName.lnk"

if (-not $NoDesktopShortcut) {
    $desktopLink = $shell.CreateShortcut((Join-Path ([Environment]::GetFolderPath('Desktop')) "$AppName.lnk"))
    $desktopLink.TargetPath = $installedUi
    $desktopLink.WorkingDirectory = $InstallDir
    $desktopLink.IconLocation = "$installedUi,0"
    $desktopLink.Description = "$AppName - editor pixel-art e tileset"
    $desktopLink.Save()
    Write-Ok 'scorciatoia sul desktop'
} else {
    Write-Info 'scorciatoia sul desktop non richiesta'
}

# --- 9. registrazione in "App e funzionalita" ------------------------------
Write-Step 'Registrazione in App e funzionalita'

$appVersion = '0.1.0'
$manifest = Get-Content -LiteralPath (Join-Path $RepoRoot 'Cargo.toml') -Raw
if ($manifest -match '(?m)^\s*version\s*=\s*"([^"]+)"') { $appVersion = $Matches[1] }

$files = Get-ChildItem -LiteralPath $InstallDir -Recurse -File
$sizeKb = if ($files) { [int](($files | Measure-Object -Property Length -Sum).Sum / 1KB) } else { 0 }

$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\' + $AppKey
New-Item -Path $uninstallKey -Force | Out-Null

$stringValues = [ordered]@{
    DisplayName          = $AppName
    DisplayVersion       = $appVersion
    Publisher            = $Publisher
    DisplayIcon          = $installedUi
    InstallLocation      = $InstallDir
    InstallSource        = $RepoRoot
    UninstallString      = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$uninstallPath`""
    QuietUninstallString = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$uninstallPath`" -KeepData -Quiet"
    InstallDate          = (Get-Date -Format 'yyyyMMdd')
    URLInfoAbout         = $RepoUrl
    HelpLink             = $RepoUrl
}
foreach ($name in $stringValues.Keys) {
    New-ItemProperty -Path $uninstallKey -Name $name -Value $stringValues[$name] -PropertyType String -Force | Out-Null
}
New-ItemProperty -Path $uninstallKey -Name 'NoModify' -Value 1 -PropertyType DWord -Force | Out-Null
New-ItemProperty -Path $uninstallKey -Name 'NoRepair' -Value 1 -PropertyType DWord -Force | Out-Null
New-ItemProperty -Path $uninstallKey -Name 'EstimatedSize' -Value $sizeKb -PropertyType DWord -Force | Out-Null
Write-Ok "$AppName $appVersion registrato (HKCU, $sizeKb KB)"

# --- 10. riepilogo ---------------------------------------------------------
$mcpJsonPath = $installedMcp -replace '\\', '\\'

Write-Host ''
Write-Host '  Installazione completata' -ForegroundColor Green
Write-Host '  ---------------------------------------------------' -ForegroundColor DarkGray
Write-Host "  Editor:  $installedUi"
Write-Host "  MCP:     $installedMcp"
Write-Host "  Avvio:   menu Start > $AppName"
Write-Host "  Rimozione: App e funzionalita > $AppName"
Write-Host ''
Write-Host '  Configurazione client MCP (Claude Desktop / Cursor):'
Write-Host "  { `"mcpServers`": { `"pixel-studio`": { `"command`": `"$mcpJsonPath`", `"args`": [] } } }"
Write-Host ''

if ($script:LogPath) {
    Write-Host "  Log: $script:LogPath" -ForegroundColor DarkGray
    try { Stop-Transcript | Out-Null } catch { }
}
exit 0
