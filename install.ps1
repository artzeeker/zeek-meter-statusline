<#
Installs zeek-meter-statusline on Windows: downloads the release binary,
merges its statusLine entry into Claude Code's settings.json, and
(optionally, interactively) sets up Nerd Font glyphs.

Usage (no params passed through irm|iex need a bare pipe):
    irm https://raw.githubusercontent.com/artzeeker/zeek-meter-statusline/main/install.ps1 | iex

To pass params, download into a scriptblock and invoke it directly:
    & ([scriptblock]::Create((irm https://raw.githubusercontent.com/artzeeker/zeek-meter-statusline/main/install.ps1))) -Yes
    & ([scriptblock]::Create((irm .../install.ps1))) -Version v1.2.0
    & ([scriptblock]::Create((irm .../install.ps1))) -NoFont
    & ([scriptblock]::Create((irm .../install.ps1))) -NoWizard

No dependency beyond Invoke-WebRequest/Invoke-RestMethod (built into
PowerShell) and the downloaded binary itself: settings.json and VS Code
config edits are delegated to `zeek-meter-statusline init ...` rather than
done here, so this script never needs jq.

To uninstall: irm https://raw.githubusercontent.com/artzeeker/zeek-meter-statusline/main/uninstall.ps1 | iex
#>
[CmdletBinding()]
param(
    [string]$Version = "",
    [switch]$Yes,
    [switch]$NoFont,
    [switch]$NoTerminalConfig,
    [switch]$NoWizard
)

$ErrorActionPreference = "Stop"
# Windows PowerShell 5.1 defaults to TLS 1.0/1.1, which GitHub rejects.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

if ($PSVersionTable.PSVersion.Major -ge 6 -and -not $IsWindows) {
    Write-Error "This installer is for Windows. On macOS/Linux/WSL, use: curl -fsSL https://raw.githubusercontent.com/artzeeker/zeek-meter-statusline/main/install.sh | bash"
    exit 1
}

$Repo = "artzeeker/zeek-meter-statusline"
$GitHubUrl = "https://github.com/$Repo"
$Api = "https://api.github.com/repos/$Repo"
$NerdFontsRepo = "ryanoasis/nerd-fonts"
$NerdFontsAsset = "NerdFontsSymbolsOnly"
$BinName = "zeek-meter-statusline"

function Write-Log([string]$Message) { Write-Host $Message }
function Write-Warn([string]$Message) { Write-Warning $Message }
function Die([string]$Message) { Write-Error "Error: $Message"; exit 1 }

function Invoke-Retry {
    param([scriptblock]$Action, [int]$Tries = 3)
    for ($attempt = 1; $attempt -le $Tries; $attempt++) {
        try {
            return & $Action
        } catch {
            if ($attempt -ge $Tries) { throw }
            Start-Sleep -Seconds 1
        }
    }
}

function Get-FileWithRetry([string]$Url, [string]$OutFile) {
    Invoke-Retry { Invoke-WebRequest -Uri $Url -OutFile $OutFile -UseBasicParsing }
}

function Confirm-Action([string]$Question, [string]$Default = "y") {
    if ($Yes) { return $Default -eq "y" }
    $suffix = if ($Default -eq "n") { "[y/N]" } else { "[Y/n]" }
    $reply = Read-Host "$Question $suffix"
    if ([string]::IsNullOrWhiteSpace($reply)) { return $Default -eq "y" }
    return $reply -match "^(y|yes)$"
}

# ---------------------------------------------------------------------------
# Platform detection (Windows only for this script; only x86_64 has a build)
# ---------------------------------------------------------------------------

# PROCESSOR_ARCHITEW6432 is set only when this process is 32-bit running on a
# 64-bit OS (WOW64); it reflects the real OS arch in that case, unlike
# PROCESSOR_ARCHITECTURE which would report x86. Fall back to
# PROCESSOR_ARCHITECTURE for a native (non-WOW64) process.
$arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
if ($arch -ne "AMD64") {
    Die "no Windows build for $arch (only x86_64)"
}
$Target = "x86_64-pc-windows-msvc"

$ClaudeDir = if ($env:CLAUDE_STATUSLINE_CLAUDE_DIR) { $env:CLAUDE_STATUSLINE_CLAUDE_DIR } else { Join-Path $HOME ".claude" }
$InstalledBin = Join-Path $ClaudeDir "$BinName.exe"
New-Item -ItemType Directory -Path $ClaudeDir -Force | Out-Null

$WorkDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Path $WorkDir -Force | Out-Null

try {
    # -----------------------------------------------------------------------
    # Version resolution + download
    # -----------------------------------------------------------------------

    function Resolve-LatestVersion([string]$ApiUrl) {
        $release = Invoke-Retry { Invoke-RestMethod -Uri $ApiUrl -Headers @{ "User-Agent" = "zeek-meter-statusline-installer" } }
        return $release.tag_name
    }

    if (-not $Version) {
        $Version = Resolve-LatestVersion "$Api/releases/latest"
        if (-not $Version) { Die "could not resolve the latest release version" }
    }

    Write-Log "Installing zeek-meter-statusline $Version for $Target..."

    $ArchiveName = "$BinName-$Target.zip"
    $DownloadUrl = "$GitHubUrl/releases/download/$Version/$ArchiveName"
    $ChecksumsUrl = "$GitHubUrl/releases/download/$Version/SHA256SUMS"
    $ArchivePath = Join-Path $WorkDir $ArchiveName
    $ChecksumsPath = Join-Path $WorkDir "SHA256SUMS"

    try {
        Get-FileWithRetry $DownloadUrl $ArchivePath
    } catch {
        Die "failed to download $DownloadUrl (check the version exists: $GitHubUrl/releases)"
    }
    try {
        Get-FileWithRetry $ChecksumsUrl $ChecksumsPath
    } catch {
        Die "failed to download SHA256SUMS for $Version"
    }

    $sumsLine = Get-Content $ChecksumsPath | Where-Object { $_ -match [regex]::Escape($ArchiveName) + '$' } | Select-Object -First 1
    if (-not $sumsLine) { Die "no checksum entry for $ArchiveName in SHA256SUMS" }
    $expected = ($sumsLine -split '\s+')[0]
    $actual = (Get-FileHash -Path $ArchivePath -Algorithm SHA256).Hash
    if ($expected -ne $actual) { Die "checksum mismatch for $ArchiveName (expected $expected, got $actual)" }

    Write-Log "Checksum verified."

    $ExtractDir = Join-Path $WorkDir "extracted"
    Expand-Archive -Path $ArchivePath -DestinationPath $ExtractDir -Force

    $foundBin = Get-ChildItem -Path $ExtractDir -Recurse -Filter "$BinName.exe" | Select-Object -First 1 -ExpandProperty FullName
    if (-not $foundBin) { Die "archive didn't contain $BinName.exe" }

    # -----------------------------------------------------------------------
    # Install the binary
    # -----------------------------------------------------------------------

    Copy-Item -Path $foundBin -Destination $InstalledBin -Force
    Write-Log "Installed $InstalledBin"

    # -----------------------------------------------------------------------
    # settings.json (delegated to the binary - no jq needed here)
    # -----------------------------------------------------------------------

    & $InstalledBin init --merge-settings

    # -------------------------------------------------------------------
    # Nerd Font (symbols-only, ~2.85MB, user-scope, no admin)
    # -------------------------------------------------------------------

    # Checks whether the font is actually resolvable by applications, not just
    # whether a file with a guessed name exists on disk. This is the only
    # check that can't produce a false positive: a bare-filename HKCU
    # registration (the bug this installer used to have - see CHANGELOG)
    # copies the file successfully but never becomes enumerable, and a
    # filesystem-only check would happily report success anyway.
    function Test-NerdFontUsable {
        try {
            Add-Type -AssemblyName System.Drawing -ErrorAction Stop
            $families = (New-Object System.Drawing.Text.InstalledFontCollection).Families | ForEach-Object { $_.Name }
            return [bool]($families | Where-Object { $_ -match "(?i)symbols nerd font" })
        } catch {
            return $false
        }
    }

    # Loads newly-registered fonts into the current session so they render
    # without requiring a sign-out. Best-effort: if this fails, the fonts are
    # still correctly registered and will pick up at next logon.
    function Enable-FontNow([string[]]$FontPaths) {
        try {
            Add-Type -MemberDefinition @"
[DllImport("gdi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
public static extern int AddFontResourceW(string lpFileName);
[DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
public static extern IntPtr SendMessageTimeoutW(IntPtr hWnd, uint Msg, UIntPtr wParam, UIntPtr lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
"@ -Name "FontApi" -Namespace "ZeekMeterStatusline" -ErrorAction Stop
            foreach ($p in $FontPaths) {
                [ZeekMeterStatusline.FontApi]::AddFontResourceW($p) | Out-Null
            }
            $result = [UIntPtr]::Zero
            [ZeekMeterStatusline.FontApi]::SendMessageTimeoutW([IntPtr]0xFFFF, 0x001D, [UIntPtr]::Zero, [UIntPtr]::Zero, 0x0002, 1000, [ref]$result) | Out-Null
        } catch {
            # Non-fatal: registration on disk/registry already succeeded.
        }
    }

    function Install-NerdFont([string]$ZipPath) {
        $dest = Join-Path $env:LOCALAPPDATA "Microsoft\Windows\Fonts"
        New-Item -ItemType Directory -Path $dest -Force | Out-Null
        $extract = Join-Path $WorkDir "font-extract"
        Expand-Archive -Path $ZipPath -DestinationPath $extract -Force
        $regKey = "HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Fonts"
        $copied = $false
        $enabledPaths = @()
        Get-ChildItem -Path $extract -Include "*.ttf", "*.otf" -Recurse | ForEach-Object {
            $destFile = Join-Path $dest $_.Name
            if (-not (Test-Path $destFile)) {
                Copy-Item $_.FullName $destFile
                $copied = $true
            }
            # Always (re-)ensure the registry value, even if the file already
            # existed: this is what lets a re-run repair a machine stuck with
            # a bare-filename registration from an older/broken install.
            $displayName = [System.IO.Path]::GetFileNameWithoutExtension($_.Name)
            $valueName = "$displayName (TrueType)"
            $existing = (Get-ItemProperty -Path $regKey -Name $valueName -ErrorAction SilentlyContinue).$valueName
            if ($existing -ne $destFile) {
                New-ItemProperty -Path $regKey -Name $valueName -Value $destFile -PropertyType String -Force | Out-Null
            }
            $enabledPaths += $destFile
        }
        if ($enabledPaths.Count -gt 0) {
            Enable-FontNow $enabledPaths
        }
        if ($copied) {
            Write-Log "Installed Nerd Font symbols to $dest."
        } else {
            Write-Log "Nerd Font symbols already present on disk; verified/repaired registration."
        }
    }

    $fontInstalledOk = $false
    if (-not $NoFont) {
        $doFont = $true
        if (-not $Yes -and -not (Test-NerdFontUsable)) {
            $doFont = Confirm-Action "Install the Nerd Font symbols pack (~2.85MB, user-scope, no admin needed) so icons render?" "y"
        }
        if ($doFont) {
            if (Test-NerdFontUsable) {
                Write-Log "Nerd Font symbols already installed."
            } else {
                $nfLatest = Resolve-LatestVersion "https://api.github.com/repos/$NerdFontsRepo/releases/latest"
                if ($nfLatest) {
                    $nfUrl = "https://github.com/$NerdFontsRepo/releases/download/$nfLatest/$NerdFontsAsset.zip"
                    $nfZip = Join-Path $WorkDir "nerd-fonts-symbols.zip"
                    try {
                        Get-FileWithRetry $nfUrl $nfZip
                        Install-NerdFont $nfZip
                    } catch {
                        Write-Warn "could not download the Nerd Font symbols pack; continuing without it"
                    }
                } else {
                    Write-Warn "could not resolve the latest Nerd Fonts release; continuing without font install"
                }
            }
            # Verify rather than assume: registration can silently fail to
            # become enumerable (this is exactly how the tofu-box bug
            # shipped previously), so trust InstalledFontCollection, not
            # "the copy/registry-write didn't throw".
            $fontInstalledOk = Test-NerdFontUsable
        }
    }

    if (-not $fontInstalledOk) {
        # Persist the "no Nerd Font" choice so the statusline defaults to
        # plain ASCII bars instead of showing tofu boxes for missing glyphs.
        # Only write this if the user hasn't already made an explicit choice
        # in this file - a re-run repairing a previously-broken install
        # should not clobber a deliberate `"nerd_font": true`.
        $cfg = Join-Path $ClaudeDir "zeek-meter-statusline.json"
        $hasExplicitChoice = $false
        if (Test-Path $cfg) {
            try {
                $existingCfg = Get-Content $cfg -Raw | ConvertFrom-Json -ErrorAction Stop
                $hasExplicitChoice = $null -ne $existingCfg.nerd_font
            } catch { }
        }
        if (-not $hasExplicitChoice) {
            Set-Content -Path $cfg -Value '{"nerd_font": false}' -NoNewline
            Add-Content -Path $cfg -Value ""
        }
        Write-Log "Nerd Font icons disabled (set CLAUDE_STATUSLINE_NERDFONT=1 or edit $cfg once a Nerd Font is available)."
        if (-not $NoFont) {
            Write-Log "The font pack was installed but isn't active yet in this session. Sign out and back in (or reboot), then re-run this installer to re-verify - or set CLAUDE_STATUSLINE_NERDFONT=1 once you've confirmed icons render."
        }
    }

    # -------------------------------------------------------------------
    # Terminal config (delegated to the binary for the JSON edit itself)
    # -------------------------------------------------------------------

    $vscodeConfigured = $false
    if (-not $NoTerminalConfig -and $fontInstalledOk) {
        & $InstalledBin init --detect-terminals | ForEach-Object {
            $parts = $_ -split '\|'
            $name = $parts[0]; $path = $parts[1]; $needsEdit = $parts[2]; $note = $parts[3]
            if (-not $name) { return }
            if ($name -eq "VS Code" -and $needsEdit -eq "true") {
                $doEdit = $true
                if (-not $Yes) {
                    $doEdit = Confirm-Action "VS Code's integrated terminal needs an explicit font-fallback entry to show icons - add it to $path?" "y"
                }
                if ($doEdit) {
                    & $InstalledBin init --configure-vscode --apply
                    $vscodeConfigured = $true
                }
            } elseif ($name -eq "Windows Terminal" -and $path) {
                Write-Log "Windows Terminal: $note"
            }
        }
    }

    # -------------------------------------------------------------------
    # Config wizard (optional, interactive only)
    # -------------------------------------------------------------------

    if (-not $NoWizard -and -not $Yes) {
        if (Confirm-Action "Run the interactive config wizard now (theme, layout, extra segments)?" "n") {
            & $InstalledBin config
        }
    }

    # -------------------------------------------------------------------
    # Done
    # -------------------------------------------------------------------

    Write-Log ""
    Write-Log "Done. Start a new Claude Code session to see the status line."
    if ($fontInstalledOk) {
        $glyphs = @(0xF2DB, 0xF418, 0xF0E4, 0xF017, 0xF133) | ForEach-Object { [char]::ConvertFromUtf32($_) }
        Write-Log ("Glyph test (should show 5 distinct icons, not boxes): " + ($glyphs -join " "))
        if ($vscodeConfigured) {
            Write-Log "Fully quit and reopen VS Code (not just reload window) - it only picks up fonts at process start."
        }
    }
    Write-Log "Run '$InstalledBin config' any time to change the theme, layout, or which segments show."
    Write-Log "Run 'irm https://raw.githubusercontent.com/$Repo/main/uninstall.ps1 | iex' to uninstall."
    Write-Log "Options: -Version vX.Y.Z to pin, -NoFont, -NoTerminalConfig, -NoWizard, -Yes for non-interactive (see script header for how to pass these through irm|iex)."
} finally {
    Remove-Item -Path $WorkDir -Recurse -Force -ErrorAction SilentlyContinue
}
