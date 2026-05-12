$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)

Write-Host "[*] Preparing Rainmeter packaging..."

# Create Skins folder with copy
if (Test-Path "rainmeter-widget\Skins\DeepSeekBalanceMonitor") {
    Remove-Item -Recurse -Force "rainmeter-widget\Skins\DeepSeekBalanceMonitor"
}
New-Item -ItemType Directory -Force -Path "rainmeter-widget\Skins" | Out-Null
Copy-Item -Recurse "rainmeter-widget\DeepSeekBalanceMonitor" "rainmeter-widget\Skins\DeepSeekBalanceMonitor"

# Convert .ini to UTF-16 LE BOM
Get-ChildItem "rainmeter-widget\Skins\DeepSeekBalanceMonitor\*.ini" | ForEach-Object {
    $content = Get-Content $_.FullName -Raw -Encoding UTF8
    $utf16 = New-Object System.Text.UnicodeEncoding($false, $true)
    [System.IO.File]::WriteAllText($_.FullName, $content, $utf16)
}

# Create RMSKIN.ini
@"
[rmskin]
Name=DeepSeek Balance Monitor
Version=1.2.0
Author=SrtaEstrella
Description=Desktop widget showing DeepSeek API balance and status.
License=MIT
LoadType=Skin
VariableFiles=
"@ | Out-File -Encoding utf8 "rainmeter-widget\RMSKIN.ini"

Write-Host "[*] Building .rmskin..."
pip install rmskin-builder --quiet
rmskin-builder --path rainmeter-widget --dir-out dist

Write-Host "[*] Cleanup..."
Remove-Item "rainmeter-widget\RMSKIN.ini"
Remove-Item -Recurse -Force "rainmeter-widget\Skins"

Write-Host ""
Write-Host "Done. Check dist\ for .rmskin file."
