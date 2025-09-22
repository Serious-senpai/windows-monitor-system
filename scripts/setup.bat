@echo off

for %%f in ("%~dp0..") do set root=%%~ff
echo Got root of repository: %root%

if not exist "%root%\.vscode\" (
    mkdir "%root%\.vscode\"
)

echo @echo off> "%root%\.vscode\__run.bat"
echo cd /d %root%>> "%root%\.vscode\__run.bat"
echo set OPENSSL_DIR=%root%\extern\OpenSSL\3.4.1>> "%root%\.vscode\__run.bat"
echo set OPENSSL_CONF=%root%\extern\OpenSSL\3.4.1\ssl\openssl.cnf>> "%root%\.vscode\__run.bat"
echo set PATH=%root%\extern\OpenSSL\3.4.1\bin;%%PATH%%>> "%root%\.vscode\__run.bat"
echo set WINDOWS_MONITOR_PASSWORD=password>> "%root%\.vscode\__run.bat"
echo cmd>> "%root%\.vscode\__run.bat"

copy /-y "%root%\.vscode\__run.bat" "%root%\run.bat"
copy /-y "%root%\vscode.json" "%root%\.vscode\settings.json"

if not exist "%root%\cert\" (
    mkdir "%root%\cert\"
)
openssl req -x509 -newkey rsa:4096 -sha512 -days 3650 -noenc -keyout %root%\cert\server.rsa -out %root%\cert\server.pem -subj "/CN=localhost" -addext "subjectAltName=DNS:localhost,DNS:*.localhost"
