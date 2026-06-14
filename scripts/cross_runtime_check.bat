@echo off
REM Roda cada fixture em tests/cross-runtime/ via Bun + Node + RTS,
REM compara stdouts. Retorna 0 se todos batem; 1 se houve divergencia.

setlocal enabledelayedexpansion

set FIXTURES_DIR=tests\cross-runtime
set REPORT_FILE=cross_runtime_report.json

REM Auto-detect RTS bin
if "%RTS_BIN%"=="" (
    if exist "target\release\rts.exe" (
        set RTS_BIN=target\release\rts.exe
    ) else if exist "target\debug\rts.exe" (
        set RTS_BIN=target\debug\rts.exe
    ) else (
        echo error: nao encontrei binario rts.exe
        exit /b 2
    )
)

REM Check dependencies
where bun >nul 2>&1
if errorlevel 1 (
    echo error: bun nao instalado
    exit /b 2
)

where node >nul 2>&1
if errorlevel 1 (
    echo error: node nao instalado
    exit /b 2
)

set total=0
set pass=0
set diverge_rts=0
set diverge_bun_node=0
set errors=0

echo Running cross-runtime tests...
echo.

for %%f in (%FIXTURES_DIR%\*.ts) do (
    set /a total+=1
    set name=%%~nf
    
    REM Run on each runtime
    bun "%%f" 2>nul > bun_out.tmp
    set bun_exit=!errorlevel!
    
    node "%%f" 2>nul > node_out.tmp
    set node_exit=!errorlevel!
    
    "%RTS_BIN%" run-new "%%f" 2>nul > rts_out.tmp
    set rts_exit=!errorlevel!
    
    REM Compare outputs
    fc /b bun_out.tmp node_out.tmp >nul 2>&1
    set bun_node_same=!errorlevel!
    
    fc /b bun_out.tmp rts_out.tmp >nul 2>&1
    set bun_rts_same=!errorlevel!
    
    if !bun_node_same! neq 0 (
        echo [~] !name! - Bun != Node ^(skip^)
        set /a diverge_bun_node+=1
    ) else if !rts_exit! neq 0 (
        echo [X] !name! - RTS error
        set /a errors+=1
    ) else if !bun_rts_same! neq 0 (
        echo [X] !name! - RTS difere de Bun/Node
        set /a diverge_rts+=1
    ) else (
        echo [OK] !name!
        set /a pass+=1
    )
)

REM Cleanup
del bun_out.tmp node_out.tmp rts_out.tmp 2>nul

echo.
echo ========================================
echo Summary:
echo   Total:               !total!
echo   Pass:                !pass!
echo   RTS divergente:      !diverge_rts!
echo   Bun/Node divergem:   !diverge_bun_node!
echo   RTS runtime error:   !errors!
echo ========================================

if !diverge_rts! gtr 0 exit /b 1
if !errors! gtr 0 exit /b 1
exit /b 0
