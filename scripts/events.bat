@echo off
setlocal enabledelayedexpansion

echo Starting event spam for testing capture callbacks...
echo Press Ctrl+C to stop

:loop
    :: Generate various system events

    :: File system events
    echo test > temp_file_!random!.txt
    del temp_file_*.txt 2>nul

    :: Process events
    start /b cmd /c "timeout /t 1 >nul"

    :: Network events (ping)
    ping -n 1 127.0.0.1 >nul 2>&1

    echo Event batch completed at !time!

goto loop
