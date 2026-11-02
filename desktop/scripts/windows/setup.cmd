@echo off
setlocal
rem Installs CopyBlip for this Windows user, starts it at sign-in with an icon
rem in the notification area, and starts it now.
set "app=%LOCALAPPDATA%\CopyBlip"
set "conf=%APPDATA%\CopyBlip"
taskkill /im copyblip.exe /f >nul 2>&1
if not exist "%app%" mkdir "%app%"
if not exist "%conf%" mkdir "%conf%"
copy /y "%~dp0copyblip.exe" "%app%\copyblip.exe" >nul || goto failed
rem The package's config carries the current group and relay.
copy /y "%~dp0config.json" "%conf%\config.json" >nul || goto failed
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v CopyBlip /t REG_SZ /d "\"%app%\copyblip.exe\" run --background --tray" /f >nul || goto failed
start "" "%app%\copyblip.exe" run --background --tray
echo.
echo CopyBlip is installed and running. Look for its icon in the notification
echo area. Windows may tuck it under the ^^ arrow next to the clock.
echo If Windows asks about the firewall, allow CopyBlip on private networks.
echo.
pause
exit /b 0
:failed
echo.
echo Setup failed. See the message above.
pause
exit /b 1
