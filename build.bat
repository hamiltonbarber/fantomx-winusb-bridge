@echo off
setlocal
echo ================================================================================
echo  Building Roland Fantom-X Windows MIDI Services Bridge
echo ================================================================================
echo.

set "PROJ_DIR=%~dp0"
set "MSBUILD_EXE="

if exist "%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" (
    for /f "usebackq tokens=*" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -prerelease -products * -requires Microsoft.Component.MSBuild -find MSBuild\**\Bin\MSBuild.exe`) do set "MSBUILD_EXE=%%i"
)

if "%MSBUILD_EXE%"=="" if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Community\Msbuild\Current\Bin\MSBuild.exe" set "MSBUILD_EXE=%ProgramFiles%\Microsoft Visual Studio\2022\Community\Msbuild\Current\Bin\MSBuild.exe"
if "%MSBUILD_EXE%"=="" if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Professional\Msbuild\Current\Bin\MSBuild.exe" set "MSBUILD_EXE=%ProgramFiles%\Microsoft Visual Studio\2022\Professional\Msbuild\Current\Bin\MSBuild.exe"
if "%MSBUILD_EXE%"=="" if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Enterprise\Msbuild\Current\Bin\MSBuild.exe" set "MSBUILD_EXE=%ProgramFiles%\Microsoft Visual Studio\2022\Enterprise\Msbuild\Current\Bin\MSBuild.exe"
if "%MSBUILD_EXE%"=="" set "MSBUILD_EXE=msbuild"

echo [*] Restoring NuGet dependencies...
if exist "%PROJ_DIR%scripts\nuget.exe" (
    "%PROJ_DIR%scripts\nuget.exe" restore "%PROJ_DIR%packages.config" -ConfigFile "%PROJ_DIR%NuGet.Config"
) else (
    nuget restore "%PROJ_DIR%packages.config" -ConfigFile "%PROJ_DIR%NuGet.Config"
)

echo [*] Compiling release binary...
"%MSBUILD_EXE%" "%PROJ_DIR%fantomx-bridge.vcxproj" /p:Configuration=Release /p:Platform=x64 /p:PlatformToolset=v143
if %errorlevel% neq 0 (
    echo.
    echo [ERROR] Build failed.
    pause
    exit /b 1
)

echo [*] Deploying Windows MIDI Services native runtime dependencies...
if exist "%PROJ_DIR%packages\Windows.Devices.Midi2.0.99.57-devpreview.5\runtimes\win-x64\native\Windows.Devices.Midi2.dll" (
    copy /y "%PROJ_DIR%packages\Windows.Devices.Midi2.0.99.57-devpreview.5\runtimes\win-x64\native\Windows.Devices.Midi2.dll" "%PROJ_DIR%bin\x64\Release\" >nul
)
if exist "%PROJ_DIR%packages\Windows.Devices.Midi2.0.99.57-devpreview.5\runtimes\win-x64\native\Windows.Devices.Midi2.pri" (
    copy /y "%PROJ_DIR%packages\Windows.Devices.Midi2.0.99.57-devpreview.5\runtimes\win-x64\native\Windows.Devices.Midi2.pri" "%PROJ_DIR%bin\x64\Release\" >nul
)

echo.
echo [SUCCESS] Build completed successfully! Binary: bin\x64\Release\fantomx-bridge.exe
endlocal
