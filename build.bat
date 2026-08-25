@echo off
setlocal
echo ================================================================================
echo  Building Roland Fantom-X Windows MIDI Services Bridge
echo ================================================================================
echo.

set "MSBUILD_EXE="

:: 1. Try finding MSBuild via vswhere (standard Visual Studio locator)
if exist "%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" (
    for /f "usebackq tokens=*" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -prerelease -products * -requires Microsoft.Component.MSBuild -find MSBuild\**\Bin\MSBuild.exe`) do (
        set "MSBUILD_EXE=%%i"
    )
)

:: 2. Try standard Community path
if not defined MSBUILD_EXE (
    if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Community\Msbuild\Current\Bin\MSBuild.exe" (
        set "MSBUILD_EXE=%ProgramFiles%\Microsoft Visual Studio\2022\Community\Msbuild\Current\Bin\MSBuild.exe"
    )
)

:: 3. Try standard Professional / Enterprise paths
if not defined MSBUILD_EXE (
    if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Professional\Msbuild\Current\Bin\MSBuild.exe" (
        set "MSBUILD_EXE=%ProgramFiles%\Microsoft Visual Studio\2022\Professional\Msbuild\Current\Bin\MSBuild.exe"
    )
    if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Enterprise\Msbuild\Current\Bin\MSBuild.exe" (
        set "MSBUILD_EXE=%ProgramFiles%\Microsoft Visual Studio\2022\Enterprise\Msbuild\Current\Bin\MSBuild.exe"
    )
)

:: 4. Try system PATH
if not defined MSBUILD_EXE (
    where msbuild >nul 2>&1
    if %errorlevel% equ 0 (
        set "MSBUILD_EXE=msbuild"
    )
)

if not defined MSBUILD_EXE (
    echo [ERROR] MSBuild not found. Please install Visual Studio 2022 (with Desktop development with C++).
    pause
    exit /b 1
)

if not exist "%~dp0packages\Windows.Devices.Midi2.0.99.57-devpreview.5" (
    echo [*] Restoring NuGet dependencies...
    if exist "%~dp0scripts\nuget.exe" (
        "%~dp0scripts\nuget.exe" restore "%~dp0packages.config" -ConfigFile "%~dp0NuGet.Config" -Source "%~dp0scripts" -Source "https://api.nuget.org/v3/index.json" -Source "https://pkgs.dev.azure.com/livemidi/WindowsMIDI/_packaging/WindowsMIDIServices/nuget/v3/index.json"
    ) else (
        nuget restore "%~dp0packages.config" -ConfigFile "%~dp0NuGet.Config"
    )
)

echo [*] Compiling release binary...
"%MSBUILD_EXE%" "%~dp0fantomx-bridge.vcxproj" /p:Configuration=Release /p:Platform=x64 /p:PlatformToolset=v143
if %errorlevel% neq 0 (
    echo.
    echo [ERROR] Build failed.
    pause
    exit /b 1
)

echo [*] Deploying Windows MIDI Services native runtime dependencies...
if exist "%~dp0packages\Windows.Devices.Midi2.0.99.57-devpreview.5\runtimes\win-x64\native\Windows.Devices.Midi2.dll" (
    copy /y "%~dp0packages\Windows.Devices.Midi2.0.99.57-devpreview.5\runtimes\win-x64\native\Windows.Devices.Midi2.dll" "%~dp0bin\x64\Release\" >nul
)
if exist "%~dp0packages\Windows.Devices.Midi2.0.99.57-devpreview.5\runtimes\win-x64\native\Windows.Devices.Midi2.pri" (
    copy /y "%~dp0packages\Windows.Devices.Midi2.0.99.57-devpreview.5\runtimes\win-x64\native\Windows.Devices.Midi2.pri" "%~dp0bin\x64\Release\" >nul
)

echo.
echo [SUCCESS] Build completed successfully! Binary: bin\x64\Release\fantomx-bridge.exe
endlocal
