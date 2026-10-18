@echo off
cd /d "%~dp0"
"%~dp0..\target\debug\slate.exe" > "%~dp0slate.log" 2>&1
