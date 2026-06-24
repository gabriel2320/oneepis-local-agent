@echo off
setlocal

set "SCRIPT_DIR=%~dp0"
set "LAUNCHER=%SCRIPT_DIR%launch-dev.ps1"

if not exist "%LAUNCHER%" (
  echo [OneEpis Local Agent] No se encontro "%LAUNCHER%"
  pause
  exit /b 1
)

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%LAUNCHER%" %*
set "EXIT_CODE=%ERRORLEVEL%"

if not "%EXIT_CODE%"=="0" (
  echo.
  echo [OneEpis Local Agent] El launcher termino con codigo %EXIT_CODE%.
  pause
)

exit /b %EXIT_CODE%
