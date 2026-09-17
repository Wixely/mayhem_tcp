param([string]$OutputDirectory = '.local/releases')

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Push-Location (Split-Path $PSScriptRoot -Parent)
$previousRustflags = $env:RUSTFLAGS
try {
    # Keep host paths out of panic locations and omit PDB/debug-path metadata.
    $env:RUSTFLAGS = '-C target-feature=+crt-static -C strip=debuginfo --remap-path-prefix=' +
        (Get-Location).Path + '=/src --remap-path-prefix=' + $env:USERPROFILE + '=/build-user'
    cargo build --release --locked --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    $metadataText = cargo metadata --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Dependency metadata failed' }
    $metadata = $metadataText | ConvertFrom-Json
    $package = $metadata.packages | Where-Object { $_.name -eq 'mayhem_tcp' }
    $name = 'mayhem_tcp-' + $package.version + '-windows-x86_64'
    $output = [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $OutputDirectory))
    $stage = Join-Path $output ([Guid]::NewGuid().ToString('N'))
    $bundle = Join-Path $stage $name
    New-Item -ItemType Directory -Force -Path $bundle | Out-Null
    Copy-Item target/x86_64-pc-windows-msvc/release/mayhem_tcp.exe $bundle
    Copy-Item README.md,LICENSE -Destination $bundle
    Copy-Item docs -Destination $bundle -Recurse
    $notices = Join-Path $bundle 'licenses'
    New-Item -ItemType Directory -Path $notices | Out-Null
    $summary = @('Third-party dependency licenses', '')
    foreach ($dependency in $metadata.packages | Where-Object { $_.name -ne 'mayhem_tcp' } | Sort-Object name) {
        $source = Split-Path $dependency.manifest_path -Parent
        $files = @(Get-ChildItem -LiteralPath $source -File | Where-Object { $_.Name -match '^(LICENSE|COPYING|NOTICE)' })
        if ($files.Count -eq 0) { throw "No license file found for $($dependency.name)" }
        $destination = Join-Path $notices ($dependency.name + '-' + $dependency.version)
        New-Item -ItemType Directory -Path $destination | Out-Null
        $files | Copy-Item -Destination $destination
        $summary += "$($dependency.name) $($dependency.version): $($dependency.license)"
    }
    $sysroot = rustc --print sysroot
    if ($LASTEXITCODE -ne 0) { throw 'Cannot find Rust license notices' }
    Copy-Item (Join-Path $sysroot 'share/doc/rust/licenses') (Join-Path $notices 'rust') -Recurse
    Copy-Item (Join-Path $sysroot 'share/doc/rust/COPYRIGHT-library.html') (Join-Path $notices 'rust')
    $summary | Set-Content (Join-Path $bundle 'THIRD-PARTY-NOTICES.txt') -Encoding UTF8
    $exe = Join-Path $bundle 'mayhem_tcp.exe'
    & $exe --help
    if ($LASTEXITCODE -ne 0) { throw 'Packaged executable smoke test failed' }
    $binaryText = [System.Text.Encoding]::ASCII.GetString([System.IO.File]::ReadAllBytes($exe))
    if ($binaryText -match '(?i)[A-Z]:[\\/]Users[\\/]' -or $binaryText.Contains((Get-Location).Path)) {
        throw 'Executable contains an unremapped host path'
    }
    $archive = Join-Path $output ($name + '.zip')
    Compress-Archive -LiteralPath $bundle -DestinationPath $archive -Force
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $name.zip" | Set-Content (Join-Path $output ($name + '.sha256')) -Encoding ASCII
    Write-Host "Release package: $archive"
} finally {
    $env:RUSTFLAGS = $previousRustflags
    Pop-Location
}
