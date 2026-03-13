@echo off
setlocal enabledelayedexpansion

:: 設定
set "TMP_BASE=%TEMP%\ucode_update_%RANDOM%"
set "SCRIPT_DIR=%~dp0"
set "DEST_UCODE=%TMP_BASE%\ucode"

echo Working in: %TMP_BASE%

mkdir "%DEST_UCODE%\GenuineIntel" 2>nul
mkdir "%DEST_UCODE%\AuthenticAMD" 2>nul

echo --- Fetching Intel ---
call :fetch_git "https://github.com/intel/Intel-Linux-Processor-Microcode-Data-Files.git" "intel-ucode" "%DEST_UCODE%\GenuineIntel"

echo --- Fetching AMD ---
call :fetch_git "https://kernel.googlesource.com/pub/scm/linux/kernel/git/firmware/linux-firmware.git" "amd-ucode" "%DEST_UCODE%\AuthenticAMD"

echo --- Compressing ---
set "SRC_DIR=%DEST_UCODE%"
call "%SCRIPT_DIR%internal_compress_ucode.bat"

echo --- Cleaning up ---
rd /s /q "%TMP_BASE%"

echo Done!
pause
exit /b

:fetch_git
set "URL=%~1"
set "SPARSE_PATH=%~2"
set "TARGET_DIR=%~3"
set "WORK_DIR=%TMP_BASE%\fetch_%RANDOM%"

mkdir "%WORK_DIR%"
pushd "%WORK_DIR%"
    git init -q
    git remote add origin "%URL%"
    git config core.sparseCheckout true
    echo %SPARSE_PATH%/ >> .git/info/sparse-checkout
    
    git pull --depth 1 origin main 2>nul || git pull --depth 1 origin master 2>nul
    
    robocopy "%WORK_DIR%\%SPARSE_PATH%" "%TARGET_DIR%" /E /XF README* >nul
popd
exit /b
