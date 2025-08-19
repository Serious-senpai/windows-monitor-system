@echo off

for %%f in ("%~dp0..") do set root=%%~ff
echo Got root of repository: %root%

echo Recreating all certificates...

openssl req -x509 -newkey rsa:4096 -sha512 -days 3650 -noenc -keyout %root%\cert\server.rsa -out %root%\cert\server.pem -subj "/CN=localhost" -addext "subjectAltName=DNS:localhost,DNS:*.localhost"
openssl req -new -newkey rsa:4096 -sha512 -nodes -keyout %root%\cert\client.rsa -out %root%\cert\client.csr -subj "/CN=client"
openssl x509 -req -days 3650 -sha512 -in %root%\cert\client.csr -CA %root%\cert\server.pem -CAkey %root%\cert\server.rsa -CAcreateserial -out %root%\cert\client.pem
openssl pkcs12 -export -out %root%\cert\client.pfx -inkey %root%\cert\client.rsa -in %root%\cert\client.pem
del %root%\cert\client.rsa %root%\cert\client.pem %root%\cert\client.csr

echo Completed
