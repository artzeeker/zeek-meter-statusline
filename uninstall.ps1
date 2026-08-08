<#
Uninstalls zeek-meter-statusline on Windows: locates the installed binary and
delegates the actual work to its `uninstall` subcommand (which owns all the
JSON edits - settings.json, this tool's own config file, VS Code's
settings.json - the same way install.ps1 delegates JSON work to
`init --merge-settings` rather than doing it here).

Usage (no params passed through irm|iex need a bare pipe):
    irm https://raw.githubusercontent.com/artzeeker/zeek-meter-statusline/main/uninstall.ps1 | iex

To pass params, download into a scriptblock and invoke it directly:
    & ([scriptblock]::Create((irm .../uninstall.ps1))) -Yes
    & ([scriptblock]::Create((irm .../uninstall.ps1))) -KeepConfig
    & ([scriptblock]::Create((irm .../uninstall.ps1))) -RemoveFont
    & ([scriptblock]::Create((irm .../uninstall.ps1))) -DryRun
#>
[CmdletBinding()]
param(
    [switch]$Yes,
    [switch]$DryRun,
    [switch]$KeepConfig,
    [switch]$KeepVscode,
    [switch]$RemoveFont
)

$ErrorActionPreference = "Stop"

function Write-Log([string]$Message) { Write-Host $Message }
function Write-Warn([string]$Message) { Write-Warning $Message }

$BinName = "zeek-meter-statusline"
$ClaudeDir = if ($env:CLAUDE_STATUSLINE_CLAUDE_DIR) { $env:CLAUDE_STATUSLINE_CLAUDE_DIR } else { Join-Path $HOME ".claude" }
$InstalledBin = Join-Path $ClaudeDir "$BinName.exe"

$BinArgs = @()
if ($Yes) { $BinArgs += "--yes" }
if ($DryRun) { $BinArgs += "--dry-run" }
if ($KeepConfig) { $BinArgs += "--keep-config" }
if ($KeepVscode) { $BinArgs += "--keep-vscode" }
if ($RemoveFont) { $BinArgs += "--remove-font" }

if (Test-Path $InstalledBin) {
    & $InstalledBin uninstall @BinArgs
    exit $LASTEXITCODE
}

# Fallback: the binary is missing (already removed, or never installed here).
# There's nothing to delegate JSON-editing work to, so do the parts that
# don't need JSON parsing and tell the user the rest.
Write-Warn "$InstalledBin not found - doing minimal cleanup without it."

$ConfigFile = Join-Path $ClaudeDir "$BinName.json"
if (Test-Path $ConfigFile) {
    if ($DryRun) {
        Write-Log "Would remove $ConfigFile"
    } else {
        Remove-Item -Path $ConfigFile -Force
        Write-Log "Removed $ConfigFile"
    }
} else {
    Write-Log "No config file found at $ConfigFile"
}

Write-Log ""
Write-Log "Could not update settings.json automatically (the binary that does that JSON edit is gone)."
Write-Log "If $ClaudeDir\settings.json still has a `"statusLine`" entry pointing at $BinName, remove it by hand."
Write-Log "If VS Code's settings.json still lists 'Symbols Nerd Font Mono' in terminal.integrated.fontFamily, you can leave it - it's harmless if the font stays installed, or remove it by hand."
