@echo off

for %%f in ("%~dp0..") do set root=%%~ff
echo Got root of repository: %root%

echo Recreating server certificate...

openssl req -x509 -newkey rsa:4096 -sha512 -days 3650 -noenc -keyout %root%\cert\server.rsa -out %root%\cert\server.pem -subj "/CN=localhost" -addext "subjectAltName=DNS:localhost,DNS:*.localhost"

echo Completed
