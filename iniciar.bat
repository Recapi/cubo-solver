@echo off
REM Sobe o servidor local do solver e abre no navegador.
REM Uso:  iniciar.bat          (porta 8080)
REM       iniciar.bat 3000     (outra porta)

setlocal
cd /d "%~dp0"

REM O toolchain GNU desta maquina nao traz dlltool.exe; a copia do MSYS2 resolve.
if exist "C:\msys64\ucrt64\bin" set "PATH=C:\msys64\ucrt64\bin;%PATH%"

set "PORTA=%~1"
if "%PORTA%"=="" set "PORTA=8080"

REM Encerra uma instancia anterior, senao o linker nao consegue regravar o .exe
taskkill /IM cubo-solver.exe /F >nul 2>&1

echo Compilando (a primeira vez demora alguns minutos)...
cargo build --release
if errorlevel 1 (
  echo.
  echo Falhou ao compilar. Veja as mensagens acima.
  pause
  exit /b 1
)

echo.
echo Servidor em http://localhost:%PORTA%
echo Feche esta janela para encerrar.
echo.
start "" "http://localhost:%PORTA%"
set "PORT=%PORTA%"
target\release\cubo-solver.exe
