# Sobe o servidor do solver.
#
# O toolchain Rust GNU instalado nesta maquina nao traz o dlltool.exe, que o
# crate windows-sys precisa. O MSYS2 tem uma copia, entao basta coloca-lo no
# PATH antes de compilar. Depois de compilado, o .exe roda sozinho.

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

if (Test-Path "C:\msys64\ucrt64\bin\dlltool.exe") {
    $env:PATH = "C:\msys64\ucrt64\bin;$env:PATH"
}

$porta = if ($args.Count -gt 0) { $args[0] } else { "8080" }

cargo run --release -- --port $porta
