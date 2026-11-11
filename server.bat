@echo off
chcp 866 >nul
rem ============================================================
rem  Open Universe - СЕРВЕР (игровой мир).
rem
rem  Запустите ЭТОТ файл первым, затем game.bat в другом окне.
rem
rem  Работает, пока окно открыто. Остановить: Ctrl+C.
rem
rem  Параметры (необязательно), например:
rem    server.bat -Port 7800 -Seconds 600
rem    server.bat -World world        (мир сохраняется между запусками)
rem ============================================================

set "HERE=%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%HERE%tools\server.ps1" %*

echo.
echo Сервер остановлен. Нажмите любую клавишу, чтобы закрыть...
pause >nul
