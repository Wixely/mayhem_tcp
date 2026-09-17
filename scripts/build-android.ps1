param(
    [ValidateSet('arm64', 'x64')][string]$Architecture = 'arm64',
    [string]$AndroidSdk = ${env:ANDROID_HOME},
    [string]$AndroidNdk = '',
    [string]$JavaSdk = ${env:JAVA_HOME},
    [ValidateSet('Debug', 'Release')][string]$Configuration = 'Debug'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = Split-Path $PSScriptRoot -Parent
if (!$AndroidSdk) { $AndroidSdk = Join-Path ${env:ProgramFiles(x86)} 'Android/android-sdk' }
if (!$JavaSdk) {
    $JavaSdk = Get-ChildItem (Join-Path $env:ProgramFiles 'Android/openjdk') -Directory |
        Sort-Object Name -Descending | Select-Object -First 1 -ExpandProperty FullName
}
$ndk = if ($AndroidNdk) { $AndroidNdk } else { Join-Path $AndroidSdk 'ndk-bundle' }
if (!(Test-Path $ndk)) {
    $ndk = Get-ChildItem (Join-Path $AndroidSdk 'ndk') -Directory |
        Sort-Object Name -Descending | Select-Object -First 1 -ExpandProperty FullName
}
$target = if ($Architecture -eq 'arm64') { 'aarch64-linux-android' } else { 'x86_64-linux-android' }
$abi = if ($Architecture -eq 'arm64') { 'arm64-v8a' } else { 'x86_64' }
$linker = Join-Path $ndk "toolchains/llvm/prebuilt/windows-x86_64/bin/${target}28-clang.cmd"
if (!(Test-Path $linker)) { throw "Android NDK linker not found: $linker" }
$linkerVariable = 'CARGO_TARGET_' + $target.Replace('-', '_').ToUpperInvariant() + '_LINKER'
$oldLinker = [Environment]::GetEnvironmentVariable($linkerVariable, 'Process')
$oldFlags = $env:RUSTFLAGS
Push-Location $root
try {
    [Environment]::SetEnvironmentVariable($linkerVariable, $linker, 'Process')
    $env:RUSTFLAGS = '-C link-arg=-Wl,-z,max-page-size=16384 --remap-path-prefix=' + $root + '=/src --remap-path-prefix=' + $env:USERPROFILE + '=/build-user'
    cargo build --release --locked --manifest-path android/native/Cargo.toml --target $target
    if ($LASTEXITCODE -ne 0) { throw 'Android Rust build failed; install the target with rustup target add first' }
    $nativeDirectory = Join-Path $root "android/build/jni/$abi"
    New-Item -ItemType Directory -Force -Path $nativeDirectory | Out-Null
    Copy-Item "android/native/target/$target/release/libmayhem_android.so" $nativeDirectory
    dotnet build android/app/MayhemTcp.Android.csproj -c $Configuration "-p:RuntimeIdentifier=android-$Architecture" "-p:AndroidSdkDirectory=$AndroidSdk" "-p:JavaSdkDirectory=$JavaSdk"
    if ($LASTEXITCODE -ne 0) { throw 'Android APK build failed' }
    $apk = Get-ChildItem "android/app/bin/$Configuration" -Recurse -Filter '*-Signed.apk' |
        Where-Object { $_.FullName -match "android-$Architecture" } | Select-Object -First 1
    if (!$apk) { throw 'Signed APK not found' }
    $output = Join-Path $root '.local/android'
    New-Item -ItemType Directory -Force -Path $output | Out-Null
    $destination = Join-Path $output "mayhem_tcp-android-$Architecture-test.apk"
    Copy-Item $apk.FullName $destination -Force
    # Ship dependency/runtime notices beside the APK. No signing keys or symbols.
    $noticeStage = Join-Path $output ([Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $noticeStage | Out-Null
    Copy-Item LICENSE $noticeStage
    $metadata = cargo metadata --locked --format-version 1 --filter-platform $target --manifest-path android/native/Cargo.toml | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Dependency metadata failed' }
    foreach ($dependency in $metadata.packages | Where-Object { $_.name -notin @('mayhem_tcp', 'mayhem_android') }) {
        $files = @(Get-ChildItem (Split-Path $dependency.manifest_path -Parent) -File |
            Where-Object { $_.Name -match '^(LICENSE|COPYING|NOTICE)' })
        if ($files.Count -eq 0) { throw "Missing license files for $($dependency.name)" }
        $folder = Join-Path $noticeStage ($dependency.name + '-' + $dependency.version)
        New-Item -ItemType Directory -Path $folder | Out-Null
        $files | Copy-Item -Destination $folder
    }
    $sysroot = rustc --print sysroot
    Copy-Item (Join-Path $sysroot 'share/doc/rust/licenses') (Join-Path $noticeStage 'rust') -Recurse
    Copy-Item (Join-Path $sysroot 'share/doc/rust/COPYRIGHT-library.html') (Join-Path $noticeStage 'rust')
    $dotnetRoot = if ($env:DOTNET_ROOT) { $env:DOTNET_ROOT } else { Split-Path (Get-Command dotnet).Source -Parent }
    foreach ($packName in @("Microsoft.NETCore.App.Runtime.Mono.android-$Architecture", "Microsoft.Android.Runtime.Mono.36.android-$Architecture", 'Microsoft.Android.Runtime.36.android')) {
        $pack = Get-ChildItem (Join-Path $dotnetRoot "packs/$packName") -Directory |
            Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
        $folder = Join-Path $noticeStage ($packName + '-' + $pack.Name)
        New-Item -ItemType Directory -Path $folder | Out-Null
        $files = @(Get-ChildItem $pack.FullName -File | Where-Object { $_.Name -match 'LICENSE|NOTICE' })
        if ($files.Count -eq 0) { throw "Missing runtime notices: $packName" }
        $files | Copy-Item -Destination $folder
    }
    $noticeArchive = Join-Path $output "mayhem_tcp-android-$Architecture-licenses.zip"
    Compress-Archive -Path "$noticeStage/*" -DestinationPath $noticeArchive -Force
    $noticeHash = (Get-FileHash $noticeArchive -Algorithm SHA256).Hash.ToLowerInvariant()
    "$noticeHash  $([IO.Path]::GetFileName($noticeArchive))" | Set-Content "$noticeArchive.sha256" -Encoding ASCII
    $hash = (Get-FileHash $destination -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($destination))" | Set-Content "$destination.sha256" -Encoding ASCII
    Write-Host "Test APK: $destination"
} finally {
    [Environment]::SetEnvironmentVariable($linkerVariable, $oldLinker, 'Process')
    $env:RUSTFLAGS = $oldFlags
    Pop-Location
}
