@echo off
title Pixel Studio - installazione
rem Avvia l'installer bypassando la ExecutionPolicy solo per questo processo.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" %*
set "RC=%ERRORLEVEL%"
echo.
if not "%RC%"=="0" echo Installazione non riuscita (codice %RC%): vedi il messaggio e il log sopra.
echo Premi un tasto per chiudere questa finestra.
pause >nul
exit /b %RC%
