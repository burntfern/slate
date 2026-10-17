@echo off
cd /d "%~dp0"
:: slate.exe sits in cargo's target-dir (default ..\target relative to this
:: repo). Adjust the path if your cargo config points somewhere else.
"%~dp0..\target\debug\slate.exe" > "%~dp0slate.log" 2>&1
