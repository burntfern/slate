@echo off
cd /d "%~dp0"
if exist "%~dp0target\debug\slate.exe" (
  "%~dp0target\debug\slate.exe" > "%~dp0slate.log" 2>&1
) else if exist "%~dp0..\target\debug\slate.exe" (
  "%~dp0..\target\debug\slate.exe" > "%~dp0slate.log" 2>&1
) else (
  echo slate.exe not found
  exit /b 1
)
