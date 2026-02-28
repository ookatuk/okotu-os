@echo off

:: by ai.
:: if broken, please tell ookatuk.

setlocal enabledelayedexpansion

set "SRC_DIR=ucode"
set "DEST_DIR=..\contents\ucode"

if not exist "%DEST_DIR%" mkdir "%DEST_DIR%"

for /r "%SRC_DIR%" %%f in (*) do (
    set "full_path=%%f"

    set "rel_path=%%f"
    set "rel_path=!rel_path:*%cd%\%SRC_DIR%\=!"

    set "out_file=%DEST_DIR%\!rel_path!.z"

    for %%d in ("!out_file!") do if not exist "%%~dpd" mkdir "%%~dpd"

    echo Compressing: !rel_path!

    python -c "import zlib, sys, os; f_in_path=r'%%f'; f_out_path=r'!out_file!'; size=os.path.getsize(f_in_path); data=open(f_in_path, 'rb').read(); compressor=zlib.compressobj(9, zlib.DEFLATED, -15); compressed=compressor.compress(data)+compressor.flush(); f_out=open(f_out_path, 'wb'); f_out.write(size.to_bytes(4, 'little')); f_out.write(compressed); f_out.close()"
)

echo Done.
pause