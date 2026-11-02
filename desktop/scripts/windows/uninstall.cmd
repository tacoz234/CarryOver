@echo off
rem Stops CopyBlip and removes it. Keeps the pairing in %APPDATA%\CopyBlip.
taskkill /im copyblip.exe /f >nul 2>&1
reg delete "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v CopyBlip /f >nul 2>&1
rmdir /s /q "%LOCALAPPDATA%\CopyBlip" 2>nul
echo CopyBlip is stopped and removed.
pause
