@echo off
chcp 866 >nul
rem ============================================================
rem  Open Universe - ИГРА (графический клиент на Unreal Engine 5).
rem
rem  Сначала запустите server.bat в другом окне, потом этот файл.
rem  В открывшемся редакторе нажмите Play (Alt+P).
rem
rem  Параметры (необязательно), например:
rem    game.bat -EngineRoot D:\UE_5.5
rem ============================================================

set "HERE=%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%HERE%tools\game.ps1" %*

echo.
echo Нажмите любую клавишу, чтобы закрыть...
pause >nul
