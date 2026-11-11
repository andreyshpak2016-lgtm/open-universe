# Сборка клиента Open Universe (UE5)

Практическое руководство: как собрать C++-модуль, чтобы Unreal перестал писать
**«Не удалось найти игровой модуль 'OpenUniverse'»**.

## Почему возникает эта ошибка

Unreal загружает игровой код из
`client/Binaries/Win64/UnrealEditor-OpenUniverse.dll`.
Пока файла нет, редактор отвечает: «Не удалось найти игровой модуль».

Это не поломка проекта, а признак того, что **C++-модуль не скомпилирован**.

## Чёрный экран при запуске

Если редактор открылся, модуль загрузился, но в окне **пусто и чёрно** — это
не поломка: у проекта ещё не было **карты** (`Content/` пуст), а `.ini`
указывает на `/Game/Maps/OpenUniverse`. Пустая карта = нет света, неба,
планеты и камеры → чёрный экран.

Лечится двумя способами (лучше обоими):

1. **Сгенерировать карту** — она создастся со светом, небом, туманом,
   точкой старта и воксельной планетой:
   ```powershell
   powershell -File tools\make-map.ps1
   ```
   Затем откройте `/Game/Maps/OpenUniverse` в редакторе.

2. **Страховка в коде.** `AOUGameMode::StartPlay` при запуске игры сам
   достраивает недостающие свет, небо, туман, планету и `AUOOriginManager`,
   создаёт `PlayerStart`, если его нет. Поэтому даже на пустой карте мир
   появится, а корабль получит камеру и меш.

> Если открываете не ту карту — в редакторе выберите
> `File → Open Level → /Game/Maps/OpenUniverse`. Проект также настроен на
> неё в `Config/DefaultEngine.ini` (`EditorStartupMap`).

## Что нужно установить

| Компонент | Зачем | Проверка |
|---|---|---|
| **Unreal Engine 5.5+** | сам движок | `D:\UE_5.5\Engine\Build\BatchFiles\Build.bat` |
| **Visual Studio 2022** с C++ | компилятор | MSVC 14.4x |
| **.NET Framework SDK 4.6+** | нужен UnrealBuildTool | `C:\Program Files (x86)\Windows Kits\NETFXSDK` |

> **Важно про .NET Framework SDK.** Без него UBT падает с
> `Could not find NetFxSDK install dir`. Устанавливается так:
> ```powershell
> winget install Microsoft.DotNet.Framework.DeveloperPack_4
> ```

## Сборка

```powershell
cd D:\Pro\open-universe
powershell -ExecutionPolicy Bypass -File tools\build-client.ps1
```

Скрипт сам:
1. находит движок (`D:\UE_5.5`, `D:\UE_5.8` и др.);
2. подставляет .NET, встроенный в UE (`Engine\Binaries\ThirdParty\DotNet`);
3. собирает цель `OpenUniverseEditor`;
4. печатает путь к собранному модулю.

Явно указать движок:

```powershell
powershell -File tools\build-client.ps1 -EngineRoot "D:\UE_5.5"
```

Только сборка без генерации файлов проекта:

```powershell
powershell -File tools\build-client.ps1 -Build
```

## Запуск редактора

```powershell
& "D:\UE_5.5\Engine\Binaries\Win64\UnrealEditor.exe" "D:\Pro\open-universe\client\OpenUniverse.uproject"
```

Если Unreal предложит **Rebuild** — соглашайтесь: это нормальное поведение для
C++-проекта с изменённым кодом.

## Известные ошибки и решения

### `Could not find NetFxSDK install dir`
Не хватает .NET Framework SDK. Установите Developer Pack (см. выше).

### `Type 'uint64' is not supported by blueprint`
Unreal не поддерживает `uint64` в `UPROPERTY`. Замените на `int64`:
```cpp
UPROPERTY(BlueprintReadOnly) int64 Tick = 0;   // правильно
```

### `FInternetAddr: cannot instantiate abstract class`
Класс абстрактный. Адрес создаётся фабрикой:
```cpp
ISocketSubsystem* S = ISocketSubsystem::Get(PLATFORM_SOCKETSUBSYSTEM);
TSharedRef<FInternetAddr> From = S->CreateInternetAddr();
Socket->RecvFrom(Buf, Len, Read, *From);
```

### `CreateMeshSection_LinearColor: cannot convert FColor to FLinearColor`
Используйте именно **LinearColor**:
```cpp
TArray<FLinearColor> Colors;   // а не TArray<FColor>
Mesh->CreateMeshSection_LinearColor(Idx, Verts, Tris, Normals, UVs, Colors, Tangents, false);
```

### `Visual Studio compiler version ... is not a preferred version`
Предупреждение, а не ошибка. Сборка продолжается.

### Не хватает памяти (`Delaying N processes due to memory pressure`)
UE просит ~1.5 ГБ на задачу. Закройте тяжёлые программы (игры, LLM-серверы)
или увеличьте файл подкачки. Сборка не падает — только замедляется.

## Подключение к Rust-серверу

Клиент работает с сервером `ou-netd` по тому же бинарному протоколу
(`OUWire.h`, `OUClientNet`) — внешних зависимостей нет.

```powershell
cd server
cargo run -p ou-server --bin ou-netd -- 127.0.0.1:7777 120   # окно 1
```

Затем в редакторе: **Play (Alt+P)**. В логе появится:

```
LogOUClient: Приветствие отправлено на 127.0.0.1:7777
LogOUClient: Сервер принял: моя сущность = 3
```

Протокол (совпадение с `crates/protocol`):

| Тег | Сообщение | Направление |
|---|---|---|
| 1 | `hello` | клиент → сервер |
| 8 | `welcome` (id своей сущности) | сервер → клиент |
| 2 | `input` (30 Гц) | клиент → сервер |
| 5 | `snapshot v2` | сервер → клиент |
| 4 | `ack` | сервер → клиент |

Токен сессии у обеих сторон выводится из seed `0x5EED`
(`new_session_token`), значение — `0x376085675394cb57…` (см. `OUClientNet.h`).

## Что собирается

| Файл | Что делает |
|---|---|
| `OpenUniverse.cpp` | точка входа модуля |
| `OUWire.h` | разбор протокола сервера (проверен стендом) |
| `OUClientNet.cpp` | UDP-клиент к Rust-серверу |
| `OUShipPawn.cpp` | корабль: предсказание, реконсиляция |
| `OUOriginManager.cpp` | floating origin |
| `OUVoxelPlanet.cpp` | воксельная планета (cube-to-sphere) |
| `OUGameMode.cpp` | режим игры |

## Проверка без движка

Логику протокола можно проверить, не имея UE — стендом на C++:

```powershell
powershell -File tools\test-client.ps1
```

Он берёт **реальный пакет Rust-сервера** и разбирает его тем же кодом `OUWire.h`.
