@echo off
setlocal enabledelayedexpansion

if /i "%~1" == "create" (
    set targets=wm-client wm-server
    set counters=
    for %%t in (!targets!) do (
        set counters=!counters! "\Process(%%t)\%% Processor Time"
        set counters=!counters! "\Process(%%t)\Elapsed Time"
        set counters=!counters! "\Process(%%t)\IO Data Bytes/sec"
        set counters=!counters! "\Process(%%t)\IO Other Bytes/sec"
        set counters=!counters! "\Process(%%t)\Thread Count"
        set counters=!counters! "\Process(%%t)\Working Set - Private"
    )

    call :create-counter "Windows Monitor Counter (blg)" "bin"
    call :create-counter "Windows Monitor Counter (csv)" "csv"
    goto :eof
)

if /i "%~1" == "start" (
    call :start-counter "Windows Monitor Counter (blg)"
    call :start-counter "Windows Monitor Counter (csv)"
    goto :eof
)

if /i "%~1" == "stop" (
    call :stop-counter "Windows Monitor Counter (blg)"
    call :stop-counter "Windows Monitor Counter (csv)"
    goto :eof
)

echo Unrecognized option "%~1"
goto :eof

:create-counter
echo Creating performance counter log %1, output as %2
logman create counter %1 -o "%cd%\Benchmark" -c !counters! -si 00:00:01 -f %2
logman query %1
goto :eof

:start-counter
echo Starting performance counter log %1
logman start %1
goto :eof

:stop-counter
echo Stopping performance counter log %1
logman stop %1
goto :eof
