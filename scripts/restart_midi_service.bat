@echo off
echo ================================================================================
echo  Resetting Windows MIDI Services (Clearing Dev Test Endpoints)
echo ================================================================================
echo.
net stop midisrv
net start midisrv
echo.
echo [SUCCESS] Windows MIDI Services has been reset. All test endpoints cleared!
echo.
pause
