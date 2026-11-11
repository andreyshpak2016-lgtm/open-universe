# Open Universe — клиент на Unreal Engine 5

Каркас клиента: C++-модуль, повторяющий серверную логику (протокол, физику,
рельеф) и добавляющий UE-специфику (рендер, ввод, floating origin).

Движок сам **не входит** в репозиторий — он ставится отдельно (Epic Games
Launcher, ~100 ГБ). Этот каталог откроется в UE5 сразу после установки.

## Что внутри

```
client/
├─ OpenUniverse.uproject            # проект (EngineAssociation 5.5)
├─ Config/
│  ├─ DefaultEngine.ini             # карта, GameMode, Lumen/Nanite
│  ├─ DefaultInput.ini              # Enhanced Input
│  └─ DefaultGame.ini               # размер сессии
└─ Source/
   ├─ OpenUniverse.Target.cs        # цель сборки игры
   ├─ OpenUniverseEditor.Target.cs  # цель сборки редактора
   └─ OpenUniverse/
      ├─ OpenUniverse.Build.cs      # зависимости модуля
      ├─ OpenUniverse.h/.cpp        # точка входа модуля
      ├─ OpenUniverseTypes.h        # типы провода (EntityState, InputFrame, ShipParams)
      ├─ OUDeterministic.h          # детерминированная тригонометрия (как на сервере)
      ├─ OUOriginManager.h/.cpp     # floating origin (double-координаты)
      ├─ OUClientNet.h/.cpp         # UDP-клиент к Rust-серверу, varint/zigzag
      ├─ OUShipPawn.h/.cpp          # корабль: предсказание и реконсиляция
      ├─ OUVoxelPlanet.h/.cpp       # воксельная планета (cube-to-sphere)
      └─ OUGameMode.h/.cpp          # режим игры
```

## Соответствие серверу (Rust)

| UE5 (C++) | Сервер (Rust) | Что повторяет |
|---|---|---|
| `FOUEntityState` | `protocol::EntityState` | состояние сущности на проводе |
| `OUDeterministic::DetSin/DetCos` | `sim::det_sin/det_cos` | та же формула, бит-в-бит |
| `OUShipPawn::StepPrediction` | `sim::integrate` | кинематика полёта |
| `OUClientNet::ParseSnapshot` | `protocol::decode` | формат снапшота v2 |
| `AOUVoxelPlanet::CubeToSphere` | `render::voxel::cube_to_sphere` | искривление сетки |
| `AOUVoxelPlanet::HeightAt` | `sim::terrain::height` | рельеф планеты |
| `AUOOriginManager` | `protocol::POS_SCALE` | работа с большими координатами |

## Сборка

Нужны **Unreal Engine 5.5+** и **Visual Studio 2022** с рабочей нагрузкой
«Разработка игр на C++» (есть: MSVC 17.14, Windows SDK 10.0.26100).

```powershell
# из корня репозитория
powershell -ExecutionPolicy Bypass -File tools/build-client.ps1
# или с явным путём к движку
powershell -ExecutionPolicy Bypass -File tools/build-client.ps1 -EngineRoot "D:\Epic Games\UE_5.5"
```

Скрипт найдёт движок, сгенерирует файлы проекта и соберёт цель `OpenUniverseEditor`.

## Архитектурные решения

**Почему не физика UE.** В открытом мире кинематику считает **авторитетный
сервер** (Rust) в `double`-координатах. Физика UE — `float` и недетерминирована,
поэтому предсказание на клиенте повторяет **формулу сервера** (`StepPrediction`),
а не `UCharacterMovementComponent`. Так античит проверяет движение, а не верит
клиенту.

**Почему floating origin.** `float` даёт ~7 значащих цифр: на дистанции 100 км
точность падает до сантиметров, дальше — «дрожание». `AUOOriginManager`
держит мир в `double` и переносит начало, когда игрок улетает (порог 5 км).

**Почему C++ повторяет шум.** Рельеф планеты нужен и физике (посадка), и
рендеру. Одна формула на сервере и клиенте гарантирует, что корабль сядет
именно на ту гору, которую видно.

## Что дальше в клиенте

- Материалы и текстуры поверхности (сейчас цвет по высоте в вершинах).
- LOD и стриминг чанков планеты (сейчас один меш на LOD).
- Enhanced Input: полноценная схема управления.
- HUD: скорость, высота над рельефом, статус сети.
- Сборка кораблей из вокселей игрока (редактор, Фаза 4).
