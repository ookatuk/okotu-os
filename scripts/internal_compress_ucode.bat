@echo off
setlocal enabledelayedexpansion

if "%SRC_DIR%"=="" (
    echo Error: SRC_DIR is not set.
    exit /b 1
)

set "DEST_DIR=..\contents\ucode"

pushd "%SRC_DIR%"
set "ABS_SRC_DIR=%cd%"
popd

for /r "%ABS_SRC_DIR%" %%f in (*) do (
    set "full_path=%%f"
    
    set "rel_path=!full_path:%ABS_SRC_DIR%\=!"
    
    set "out_file=%DEST_DIR%\!rel_path!.z"
    
    for %%d in ("!out_file!") do if not exist "%%~dpd" mkdir "%%~dpd" (

    echo Compressing: !rel_path!

    )
    python -c "import zlib, sys, os; f_in, f_out = sys.argv[1], sys.argv[2]; size=os.path.getsize(f_in); data=open(f_in, 'rb').read(); comp=zlib.compressobj(9, zlib.DEFLATED, -15); payload=comp.compress(data)+comp.flush(); out=open(f_out, 'wb'); out.write(size.to_bytes(4, 'little')); out.write(payload); out.close()" "%%f" "!out_file!"
)

echo Done.
pause
