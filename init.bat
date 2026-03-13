@echo off
setlocal

echo "All bat file is written ai and not runtime checked."
echo "We cannot guarantee against damages caused by the execution."
choice /c yn /m "ok?"

if errorlevel 2 (
    exit /b
)

set "SCRIPT_DIR=%~dp0"
pushd /d "%SCRIPT_DIR%"

echo --- Updating Submodules ---
git submodule update --init --recursive

echo --- Running Get Microcode Script ---
call "scripts\get_microcode.bat"
popd
echo.
echo All processes completed.
pause
