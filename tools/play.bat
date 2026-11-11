@echo off
rem Запуск Open Universe: сервер + игроки.
rem Двойной клик по этому файлу — и игра запустится.
rem Можно передать параметры: play.bat -Players 3 -Seconds 30

rem Папка, где лежит этот .bat (её родитель — корень репозитория).
set "HERE=%~dp0"
set "ROOT=%HERE%.."

powershell -NoProfile -ExecutionPolicy Bypass -File "%HERE%play.ps1" %*

echo.
echo Нажмите любую клавишу, чтобы закрыть окно...
pause >nul
