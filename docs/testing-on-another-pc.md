# Тесты на втором компьютере

Как поставить окружение на другой ПК и прогонять проверки Open Universe.
Есть **единый скрипт** `tools/test-all.ps1`: он сам определяет, что установлено,
запускает доступные тесты и печатает сводку — что прошло, что упало, что
пропущено и почему.

---

## Пошагово с нуля (чек-лист)

### Шаг 1. Установить Git
```powershell
winget install --id Git.Git -e
```
Проверка (в **новом** окне терминала):
```powershell
git --version
```
Если `winget` незнаком — скачайте Git с https://git-scm.com/download/win.

### Шаг 2. Установить Rust
```powershell
winget install --id Rustlang.Rustup -e
```
Проверка (в новом окне):
```powershell
cargo --version
rustc --version
```
Без Rust тесты ядра не запустятся.

### Шаг 3. Установить Visual Studio 2022 с C++
Нужен для C++-тестов провода и рельефа (компилятор `cl.exe`).
```powershell
winget install --id Microsoft.VisualStudio.2022.Community -e
```
После установки откройте **Visual Studio Installer → Изменить (Modify)** и
включите рабочую нагрузку **«Разработка классических приложений на C++»**
(*Desktop development with C++*), затем нажмите «Изменить».

Проверка:
```powershell
& "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
```
Должен вывестись путь; пусто — компонент C++ не отмечен.

> Без VS с C++ шаги `test-client` и `test-terrain` будут **пропущены**.

### Шаг 4. Получить код проекта
```powershell
cd C:\            # или любой рабочий диск
git clone https://github.com/andreyshpak2016-lgtm/open-universe.git
cd open-universe
```

### Шаг 5. Прогнать быстрые тесты (без Unreal Engine)
```powershell
powershell -ExecutionPolicy Bypass -File tools/test-all.ps1 -SkipUE
```
Ожидаемый итог: `cargo test`, `test-client`, `test-terrain` — **OK**;
`test-ue` и `test-shipping` — пропущены (движка нет).

### Шаг 6. (Только для UE-тестов) Установить Unreal Engine 5.5
- Установите **Epic Games Launcher**, добавьте **Unreal Engine 5.5**.
- Путь обычно `C:\Program Files\Epic Games\UE_5.5`.
- Установите **.NET Framework SDK** (нужен UnrealBuildTool):
```powershell
winget install --id Microsoft.DotNet.Framework.DeveloperPack_4 -e
```

### Шаг 7. Собрать UE-модуль клиента
```powershell
powershell -ExecutionPolicy Bypass -File tools/build-client.ps1 -Build
```
Если движок не в `D:\UE_5.5`, укажите путь:
```powershell
powershell -File tools/build-client.ps1 -Build -EngineRoot "C:\Program Files\Epic Games\UE_5.5"
```

### Шаг 8. Прогнать UE-тесты
```powershell
powershell -ExecutionPolicy Bypass -File tools/test-ue.ps1
```
Если движок нестандартный — добавьте `-EngineRoot`.

### Шаг 9. Полный прогон всего
```powershell
powershell -ExecutionPolicy Bypass -File tools/test-all.ps1
```
Сводка покажет все проверки сразу.

### Шаг 10. (Необязательно) Упаковать игру и проверить Shipping
```powershell
powershell -File tools/package-game.ps1     # сборка standalone (долго)
powershell -File tools/test-shipping.ps1    # запуск упакованной сборки
```

---

## 0. Быстрый путь (если всё уже стоит)

```powershell
git clone https://github.com/andreyshpak2016-lgtm/open-universe.git
cd open-universe
powershell -ExecutionPolicy Bypass -File tools/test-all.ps1
```

Скрипт покажет таблицу вида:

```
  cargo test --workspace     OK    код 0
  test-client (провод)       OK    код 0
  test-terrain (рельеф)      OK    код 0
  test-ue (редактор)         OK    код 0
  test-shipping (упакованная) -    пропущено: нет упакованной сборки
```

Без Unreal Engine добавляйте `-SkipUE`, чтобы не ждать тяжёлые шаги.

---

## 1. Что установить

### Обязательно для тестов ядра и провода

| Программа | Зачем | Как поставить | Проверка |
|---|---|---|---|
| **Rust (rustup)** | ядро, сервер, симуляция | `winget install Rustlang.Rustup` или https://rustup.rs | `cargo --version` |
| **Visual Studio 2022** с компонентом *Desktop development with C++* | C++-стенды провода и рельефа (`cl.exe`) | `winget install Microsoft.VisualStudio.2022.Community` затем в установщике отметить C++ | `cl` после `vcvars64.bat` |
| **Git** | клонирование | `winget install Git.Git` | `git --version` |

> Без VS с C++ тесты `core` и `terrain` будут **пропущены** (нужен `cl.exe`),
> а Rust-тесты пройдут.

### Для UE-тестов (графика, редактор, упаковка)

| Программа | Зачем | Проверка |
|---|---|---|
| **Unreal Engine 5.5** | движок клиента | `D:\UE_5.5\Engine\Binaries\Win64\UnrealEditor.exe` |
| **.NET Framework SDK 4.6+** | нужен UnrealBuildTool | `C:\Program Files (x86)\Windows Kits\NETFXSDK` |
| **Visual Studio 2022 с C++** | сборка UE-модуля | см. выше |

Установка .NET Framework SDK (частая ошибка `Could not find NetFxSDK install dir`):

```powershell
winget install Microsoft.DotNet.Framework.DeveloperPack_4
```

> UE требует ~100+ ГБ на диске и ставится долго. Если графические проверки на
> втором ПК не нужны, ставьте только Rust + VS — этого хватит для `core`,
> `test-client`, `test-terrain`.

---

## 2. Что делает каждый тест и что ему нужно

| Скрипт | Проверяет | Нужно |
|---|---|---|
| `tools/test-rust.ps1` | ядро (симуляция, рельеф, бой, экономика, протокол, шард) — 340+ тестов | Rust |
| `tools/test-client.ps1` | разбор реального пакета сервера тем же кодом, что в UE | Rust + MSVC |
| `tools/test-terrain.ps1` | бит-в-бит совпадение рельефа клиента и сервера | Rust + MSVC |
| `tools/test-ue.ps1` | редактор UE реально запускается и подключается к серверу | UE 5.5 |
| `tools/test-shipping.ps1` | упакованная Shipping-сборка запускается, кадр не пустой | упаковка + UE |
| `tools/capture.ps1 -Preset …` | сценарии фич клиента (скриншоты, графика) | UE 5.5 |
| `tools/test-all.ps1` | **всё сразу** с честными пометками о пропущенном | по возможностям |

---

## 3. Полный сценарий на втором ПК (с UE 5.5)

```powershell
git clone https://github.com/andreyshpak2016-lgtm/open-universe.git
cd open-universe

# 1. Тесты без движка — быстро (Rust + MSVC).
powershell -ExecutionPolicy Bypass -File tools/test-all.ps1 -SkipUE

# 2. Собрать UE-модуль клиента.
powershell -ExecutionPolicy Bypass -File tools/build-client.ps1 -Build

# 3. UE-тест редактора (запускает сервер и клиент, делает скриншот).
powershell -ExecutionPolicy Bypass -File tools/test-ue.ps1

# 4. (Необязательно) упаковать игру и проверить Shipping-сборку.
powershell -ExecutionPolicy Bypass -File tools/package-game.ps1
powershell -ExecutionPolicy Bypass -File tools/test-shipping.ps1

# 5. Один прогон всего доступного.
powershell -ExecutionPolicy Bypass -File tools/test-all.ps1
```

Если движок лежит не в `D:\UE_5.5`, укажите путь:

```powershell
powershell -File tools/test-all.ps1 -EngineRoot "C:\Program Files\Epic Games\UE_5.5"
powershell -File tools/build-client.ps1 -EngineRoot "C:\Program Files\Epic Games\UE_5.5"
```

---

## 4. Тесты в облаке (без своего второго ПК)

Всё, что **не требует UE**, гоняется автоматически на GitHub-раннерах:

- **`.github/workflows/ci.yml`** — `cargo test` + `test-client` + release-сборка
  при `push`/PR в `main` и при тегах `v*`.
- **`.github/workflows/health.yml`** — проверка «сервер+клиент» каждые 30 минут,
  публикация `status.json` на сайт.

Результаты — во вкладке **Actions** репозитория. UE-тесты в облако не вынести:
лицензия и размер движка.

---

## 5. Частые проблемы

| Симптом | Причина / решение |
|---|---|
| `cargo: command not found` | Rust не установлен или не в `PATH`; перезапустите терминал |
| `Не найден Visual Studio с C++` | В VS не отмечен компонент C++; доустановите его |
| `Could not find NetFxSDK install dir` | Нет .NET Framework SDK (см. зеро 1) |
| UE-тесты «ПРОПУЩЕНО» | Движок не найден — укажите `-EngineRoot` |
| `test-shipping` пропущен | Нет упакованной сборки — сначала `package-game.ps1` |
| `Unable to delete …UnrealEditor-OpenUniverse.dll` | Открыт редактор UE — закройте его и пересоберите |
