@echo off
setlocal
rem Installs Carryover for this Windows user, joins or starts a group, starts it
rem at sign-in with an icon in the notification area, and starts it now.
set "app=%LOCALAPPDATA%\CopyBlip"
set "conf=%APPDATA%\CopyBlip"
taskkill /im copyblip.exe /f >nul 2>&1
if not exist "%app%" mkdir "%app%"
if not exist "%conf%" mkdir "%conf%"
if not exist "%~dp0copyblip.exe" goto blocked
copy /y "%~dp0copyblip.exe" "%app%\copyblip.exe" >nul || goto failed
rem copyblip.exe won't start without this, which its window uses.
copy /y "%~dp0WebView2Loader.dll" "%app%\WebView2Loader.dll" >nul || goto failed
copy /y "%~dp0invite.cmd" "%app%\invite.cmd" >nul || goto failed
rem Antivirus can remove the copy right after it lands.
if not exist "%app%\copyblip.exe" goto blocked
rem A package built with --with-group carries its group. Otherwise keep this
rem PC's existing group, or ask for one.
if exist "%~dp0config.json" (
  copy /y "%~dp0config.json" "%conf%\config.json" >nul || goto failed
) else if not exist "%conf%\config.json" (
  call :pair || goto failed
)
call :webview
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v CopyBlip /t REG_SZ /d "\"%app%\copyblip.exe\" run --background --tray" /f >nul || goto failed
start "" "%app%\copyblip.exe" run --background --tray
echo.
echo Carryover is installed and running. Look for its icon in the notification
echo area. Windows may tuck it under the ^^ arrow next to the clock.
echo If Windows asks about the firewall, allow Carryover on private networks.
echo.
echo To add another computer, click the icon, choose Add a Device, then Show
echo Link, and paste the link into setup on that computer.
echo.
pause
exit /b 0

:pair
echo.
echo This PC isn't in a Carryover group yet.
echo.
echo   J  Join an existing group. On a computer already in it, open Carryover,
echo      choose Add a Device, then Show Link, and copy the link. If its window
echo      won't open: on a PC, double-click invite.cmd in
echo      %%LOCALAPPDATA%%\CopyBlip; on a Mac, run "copyblip invite".
echo   N  Start a new group, with this PC as the first device.
echo.
choice /c JN /n /m "Join or New? [J/N] "
if errorlevel 2 (
  "%app%\copyblip.exe" init
  exit /b
)
"%app%\copyblip.exe" join && exit /b 0
echo That didn't work. Try again.
goto pair

rem Carryover's window needs Microsoft's WebView2 runtime. Windows 11 usually
rem has it, but not always. Syncing works without it.
:webview
set "client=Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
reg query "HKLM\SOFTWARE\WOW6432Node\%client%" /v pv >nul 2>&1 && exit /b 0
reg query "HKLM\SOFTWARE\%client%" /v pv >nul 2>&1 && exit /b 0
reg query "HKCU\Software\%client%" /v pv >nul 2>&1 && exit /b 0
echo.
echo Installing Microsoft's WebView2 runtime, which Carryover's window needs...
set "bootstrap=%TEMP%\MicrosoftEdgeWebview2Setup.exe"
curl.exe -sSfL -o "%bootstrap%" "https://go.microsoft.com/fwlink/p/?LinkId=2124703" && "%bootstrap%" /silent /install
if errorlevel 1 (
  echo Couldn't install WebView2. Carryover still syncs, but its window won't
  echo open until you install the WebView2 Runtime from Microsoft's website.
)
del "%bootstrap%" >nul 2>&1
exit /b 0

:blocked
echo.
echo copyblip.exe is missing. Windows Security probably removed it. Open
echo Windows Security, Virus and threat protection, Protection history, and
echo allow Carryover, or add an exclusion for this folder and
echo %app%
echo then extract the zip again and rerun setup.
pause
exit /b 1

:failed
echo.
echo Setup failed. See the message above.
pause
exit /b 1
