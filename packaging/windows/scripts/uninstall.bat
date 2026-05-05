@echo off
REM ccr-rust/packaging/windows/scripts/uninstall.bat

echo Uninstalling Claude Code Router...

REM Stop running services
ccr stop 2>NUL

echo.
echo ✅ Claude Code Router has been removed
echo.
echo Configuration files are preserved at:
echo %USERPROFILE%\.claude-code-router
echo.
echo To completely remove configuration, delete the above directory.
pause
