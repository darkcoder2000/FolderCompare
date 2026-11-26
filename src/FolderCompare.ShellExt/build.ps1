# Builds FolderCompare.ShellExt.dll (x64) with the MSVC Build Tools. Called by FolderCompare.App.csproj and
# FolderCompare.Avalonia.csproj; a solution build runs both in parallel, so the compile is serialized by a mutex.
param([string]$OutDir = (Join-Path $PSScriptRoot 'bin\x64'))

$ErrorActionPreference = 'Stop'
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path $vswhere)) { throw 'vswhere.exe not found. Install the Visual Studio Build Tools with the C++ workload.' }
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw 'No Visual Studio installation with the C++ x64 tools found.' }
$vcvars = Join-Path $vs 'VC\Auxiliary\Build\vcvars64.bat'

New-Item -ItemType Directory -Force $OutDir | Out-Null
$objDir = Join-Path $PSScriptRoot 'obj\x64'
New-Item -ItemType Directory -Force $objDir | Out-Null

$src = Join-Path $PSScriptRoot 'ShellExt.cpp'
$def = Join-Path $PSScriptRoot 'ShellExt.def'
$dll = Join-Path $OutDir 'FolderCompare.ShellExt.dll'

$sources = @($src, $def, $PSCommandPath)
$mutex = New-Object System.Threading.Mutex($false, 'Local\FolderCompare.ShellExt.Build')
try { [void]$mutex.WaitOne() } catch [System.Threading.AbandonedMutexException] { }
try {
# The other project's build may have produced the DLL while this one was waiting.
if ((Test-Path $dll) -and -not ($sources | Where-Object { (Get-Item $_).LastWriteTimeUtc -gt (Get-Item $dll).LastWriteTimeUtc })) {
    Write-Host 'FolderCompare.ShellExt.dll is up to date.'
    return
}

$cl = "cl /nologo /O2 /MT /LD /EHsc /std:c++17 /W4 /WX /DUNICODE /D_UNICODE /Fo`"$objDir\\`" `"$src`" " +
      "/link /DEF:`"$def`" /OUT:`"$dll`" /IMPLIB:`"$objDir\FolderCompare.ShellExt.lib`" " +
      "ole32.lib shell32.lib shlwapi.lib advapi32.lib runtimeobject.lib"
$env:PATH = "$(Split-Path $vswhere);$env:PATH" # vcvars64.bat calls vswhere itself
cmd /c "`"$vcvars`" >nul && $cl"
if ($LASTEXITCODE -ne 0) { throw "Building the shell extension failed ($LASTEXITCODE)." }
}
finally { $mutex.ReleaseMutex(); $mutex.Dispose() }
