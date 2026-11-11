@echo off
rem ============================================================
rem  Open Universe - запуск игры одним двойным кликом.
rem
rem  Что произойдёт:
rem    1. Соберётся игровое ядро (Rust).
rem    2. Запустится сервер (мир).
rem    3. Подключатся два игрока.
rem    4. Покажется статистика.
rem
rem  Можно передать параметры, например:
rem    play.bat -Players 3 -Seconds 30
rem ============================================================

set "HERE=%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%HERE%tools\play.ps1" %*

echo.
echo Нажмите любую клавишу, чтобы закрыть...
pause >nul
