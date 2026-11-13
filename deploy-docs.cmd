@echo off
title Pixel Studio - pubblicazione guida su GitHub Pages
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0deploy-docs.ps1" %*
set "RC=%ERRORLEVEL%"
echo.
if not "%RC%"=="0" echo Pubblicazione non riuscita (codice %RC%): vedi il messaggio sopra.
echo Premi un tasto per chiudere questa finestra.
pause >nul
exit /b %RC%
