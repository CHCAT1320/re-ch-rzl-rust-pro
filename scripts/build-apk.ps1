# Local Android APK build for Windows.
# Requires JDK. SDK/NDK are auto-detected; missing NDK is installed via sdkmanager.
#
# Signing:
#   Default: android/debug.keystore (auto-created, password android)
#   Release keystore: pass -Keystore / -Password, or set
#     CARGO_APK_RELEASE_KEYSTORE
#     CARGO_APK_RELEASE_KEYSTORE_PASSWORD
param(
    [switch]$Release,
    [string]$Keystore,
    [string]$Password
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root

function Get-SdkRoot {
    foreach ($name in @("ANDROID_HOME", "ANDROID_SDK_ROOT")) {
        $value = [Environment]::GetEnvironmentVariable($name)
        if ($value -and (Test-Path -LiteralPath $value)) {
            return $value
        }
    }
    foreach ($fallback in @(
            (Join-Path $env:LOCALAPPDATA "Android\Sdk"),
            "D:\Android",
            "C:\Android"
        )) {
        if (Test-Path -LiteralPath $fallback) {
            return $fallback
        }
    }
    throw "Android SDK not found. Set ANDROID_HOME."
}

function Find-Ndk([string]$sdk) {
    foreach ($name in @("ANDROID_NDK_HOME", "NDK_HOME", "ANDROID_NDK_ROOT")) {
        $value = [Environment]::GetEnvironmentVariable($name)
        if ($value -and (Test-Path -LiteralPath (Join-Path $value "ndk-build.cmd"))) {
            return $value
        }
        if ($value -and (Test-Path -LiteralPath (Join-Path $value "ndk-build"))) {
            return $value
        }
    }
    $ndkRoot = Join-Path $sdk "ndk"
    if (Test-Path -LiteralPath $ndkRoot) {
        $found = Get-ChildItem -LiteralPath $ndkRoot -Directory | Sort-Object Name -Descending | Select-Object -First 1
        if ($found) {
            return $found.FullName
        }
    }
    $bundle = Join-Path $sdk "ndk-bundle"
    if (Test-Path -LiteralPath $bundle) {
        return $bundle
    }
    return $null
}

function Find-SdkManager([string]$sdk) {
    $candidates = @(
        (Join-Path $sdk "cmdline-tools\latest\bin\sdkmanager.bat"),
        (Join-Path $sdk "cmdline-tools\bin\sdkmanager.bat"),
        (Join-Path $sdk "tools\bin\sdkmanager.bat")
    )
    foreach ($path in $candidates) {
        if (Test-Path -LiteralPath $path) {
            return $path
        }
    }
    $nested = Get-ChildItem -Path (Join-Path $sdk "cmdline-tools") -Recurse -Filter "sdkmanager.bat" -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($nested) {
        return $nested.FullName
    }
    return $null
}

function Ensure-SdkManager([string]$sdk) {
    $existing = Find-SdkManager $sdk
    if ($existing) {
        return $existing
    }
    Write-Host "cmdline-tools missing; downloading Android commandlinetools..."
    $zip = Join-Path $env:TEMP "commandlinetools-win.zip"
    $url = "https://dl.google.com/android/repository/commandlinetools-win-13114758_latest.zip"
    Invoke-WebRequest -Uri $url -OutFile $zip
    $extract = Join-Path $env:TEMP "commandlinetools-win"
    if (Test-Path -LiteralPath $extract) {
        Remove-Item -LiteralPath $extract -Recurse -Force
    }
    Expand-Archive -LiteralPath $zip -DestinationPath $extract -Force
    $dest = Join-Path $sdk "cmdline-tools\latest"
    New-Item -ItemType Directory -Force -Path $dest | Out-Null
    $inner = Join-Path $extract "cmdline-tools"
    Copy-Item -Path (Join-Path $inner "*") -Destination $dest -Recurse -Force
    $bat = Join-Path $dest "bin\sdkmanager.bat"
    if (-not (Test-Path -LiteralPath $bat)) {
        throw "sdkmanager.bat not found after installing cmdline-tools"
    }
    return $bat
}

function Invoke-SdkManager([string]$sdkmanager, [string]$sdk, [string[]]$packages) {
    cmd /c "echo y| `"$sdkmanager`" --sdk_root=`"$sdk`" --licenses" | Out-Host
    $quoted = ($packages | ForEach-Object { '"{0}"' -f $_ }) -join " "
    cmd /c "echo y| `"$sdkmanager`" --sdk_root=`"$sdk`" $quoted" | Out-Host
}

function Install-Ndk([string]$sdk, [string]$sdkmanager) {
    Write-Host "Installing NDK via sdkmanager..."
    Invoke-SdkManager $sdkmanager $sdk @("ndk;27.2.12479018")
    $ndk = Find-Ndk $sdk
    if (-not $ndk) {
        throw "NDK install failed. Install Android NDK in SDK Manager, or set ANDROID_NDK_HOME."
    }
    return $ndk
}

function Ensure-Platform([string]$sdk, [int]$api) {
    $platform = Join-Path $sdk "platforms\android-$api"
    if (Test-Path -LiteralPath $platform) {
        return
    }
    $sdkmanager = Ensure-SdkManager $sdk
    Write-Host "Installing Android platform $api..."
    Invoke-SdkManager $sdkmanager $sdk @("platforms;android-$api")
    if (-not (Test-Path -LiteralPath $platform)) {
        throw "Failed to install platforms;android-$api"
    }
}

$sdk = Get-SdkRoot
$env:ANDROID_HOME = $sdk
$env:ANDROID_SDK_ROOT = $sdk

$ndk = Find-Ndk $sdk
if (-not $ndk) {
    $sdkmanager = Ensure-SdkManager $sdk
    $ndk = Install-Ndk $sdk $sdkmanager
}
$env:ANDROID_NDK_HOME = $ndk
$env:NDK_HOME = $ndk
Ensure-Platform $sdk 35

Write-Host "ANDROID_HOME=$sdk"
Write-Host "ANDROID_NDK_HOME=$ndk"

function Ensure-DebugKeystore {
    $keystore = Join-Path $root "android\debug.keystore"
    if (Test-Path -LiteralPath $keystore) {
        return
    }
    $keytool = Get-Command keytool -ErrorAction SilentlyContinue
    if (-not $keytool) {
        $javaHome = $env:JAVA_HOME
        if ($javaHome) {
            $candidate = Join-Path $javaHome "bin\keytool.exe"
            if (Test-Path -LiteralPath $candidate) {
                $keytool = Get-Command $candidate
            }
        }
    }
    if (-not $keytool) {
        throw "keytool not found. Install a JDK and ensure it is on PATH."
    }
    New-Item -ItemType Directory -Force -Path (Join-Path $root "android") | Out-Null
    Write-Host "Creating android/debug.keystore..."
    & $keytool.Source -genkeypair -keystore $keystore -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 10000 -storepass android -keypass android -dname "CN=Android Debug,O=Android,C=US"
    if (-not $?) {
        throw "Failed to create debug keystore"
    }
}

if ($Keystore) {
    if (-not (Test-Path -LiteralPath $Keystore)) {
        throw "Keystore not found: $Keystore"
    }
    if (-not $Password) {
        throw "Pass -Password when using -Keystore."
    }
    $env:CARGO_APK_RELEASE_KEYSTORE = (Resolve-Path -LiteralPath $Keystore).Path
    $env:CARGO_APK_RELEASE_KEYSTORE_PASSWORD = $Password
}

if ($env:CARGO_APK_RELEASE_KEYSTORE) {
    Write-Host "Signing with $($env:CARGO_APK_RELEASE_KEYSTORE)"
} else {
    Ensure-DebugKeystore
    Write-Host "Signing with android/debug.keystore (debug). For a real key: -Keystore path -Password secret"
}

rustup target add aarch64-linux-android | Out-Host

if (-not (Get-Command cargo-apk2 -ErrorAction SilentlyContinue)) {
    Write-Host "Installing cargo-apk2..."
    cargo install cargo-apk2
}

$profileArgs = @()
if ($Release) {
    $profileArgs += "--release"
}

cargo apk2 build --lib @profileArgs
if (-not $?) {
    throw "cargo apk2 build failed"
}

$searchRoots = @(
    (Join-Path $root "target\aarch64-linux-android\release\apk"),
    (Join-Path $root "target\aarch64-linux-android\debug\apk"),
    (Join-Path $root "target\release\apk"),
    (Join-Path $root "target\debug\apk"),
    (Join-Path $root "target")
)
$apk = $null
foreach ($dir in $searchRoots) {
    if (Test-Path -LiteralPath $dir) {
        $apk = Get-ChildItem -Path $dir -Filter "*.apk" -Recurse -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending |
            Select-Object -First 1
        if ($apk) {
            break
        }
    }
}
if (-not $apk) {
    throw "APK not found after build"
}

$outDir = Join-Path $root "dist\android"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$dest = Join-Path $outDir "re-ch-rzl-rust.apk"
Copy-Item -LiteralPath $apk.FullName -Destination $dest -Force
Write-Host "APK: $dest"
