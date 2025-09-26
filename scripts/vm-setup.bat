:: Copy this script to anywhere in the VM
@echo off

:: The repository folder must be shared first in VMware settings
net use Y: "\\vmware-host\Shared Folders"

set current=%~dp0
cd /d %current%

echo @echo off>migrate.bat
echo set root=Y:\windows-monitor-system>>migrate.bat
echo echo Got root of repository: %%root%%>>migrate.bat

echo echo Copying necessary files from %%root%% to %%cd%%>>migrate.bat

echo @echo on>>migrate.bat
echo copy /y %%root%%\scripts\events.bat events.bat>>migrate.bat
echo copy /y %%root%%\target\release\wm-client.exe wm-client.exe>>migrate.bat
echo copy /-y %%root%%\target\release\client-config.yml client-config.yml>>migrate.bat

echo To copy files from host to VM, use migrate.bat
echo.

cmd
