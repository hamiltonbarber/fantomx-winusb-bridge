@echo off
title Roland Fantom-X Windows MIDI Services Bridge

if exist "%~dp0bin\x64\Release\fantomx-bridge.exe" (
    "%~dp0bin\x64\Release\fantomx-bridge.exe"
    goto :done
)

if exist "%~dp0target\release\loopback-endpoints-cpp.exe" (
    "%~dp0target\release\loopback-endpoints-cpp.exe"
    goto :done
)

echo [INFO] Bridge executable not found. Building from source...
call "%~dp0build.bat"
if exist "%~dp0bin\x64\Release\fantomx-bridge.exe" (
    "%~dp0bin\x64\Release\fantomx-bridge.exe"
) else (
    echo [ERROR] Could not build or find bridge executable.
    pause
)

:done
pause
