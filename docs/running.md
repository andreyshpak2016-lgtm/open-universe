# Как запустить Open Universe

Коротко: игра запускается **двумя командами** — сервер и клиент. Графического
клиента пока нет (каркас UE5 готов, но движок не установлен), поэтому клиент
сейчас консольный.

---

## Вариант 1. Проще всего — один скрипт

```powershell
# из корня репозитория
powershell -ExecutionPolicy Bypass -File tools/play.ps1
```

Скрипт сам:
1. соберёт release-версию,
2. запустит сервер `ou-netd`,
3. подключит двух игроков `ou-netclient`,
4. покажет итоги: тики, трафик, позиции.

Параметры:

```powershell
powershell -File tools/play.ps1 -Players 3 -Seconds 30      # три игрока, 30 секунд
powershell -File tools/play.ps1 -Port 7800 -World .\world   # свой порт, сохранение мира
powershell -File tools/play.ps1 -NoBuild                    # без пересборки
```

---

## Вариант 2. Вручную (два окна терминала)

### Шаг 1. Собрать

```powershell
cd server
cargo build --release
```

### Шаг 2. Запустить сервер (окно 1)

```powershell
cd server
cargo run -p ou-server --bin ou-netd -- 127.0.0.1:7777 60
```

Аргументы: `адрес:порт`, `секунды`, `[каталог мира]`.

Сервер печатает: токен сессии, частоту тиков (30 Гц), кто подключился.

### Шаг 3. Подключить игрока (окно 2)

```powershell
cd server
cargo run -p ou-client --bin ou-netclient -- 127.0.0.1:7777 50
```

Сколько окон — столько игроков. Каждый получит свой корабль.

---

## Вариант 3. Из готовой сборки (без Rust)

1. Скачайте архив со страницы релизов:
   https://github.com/andreyshpak2016-lgtm/open-universe/releases
   (или на сайте — кнопка **«⬇ Скачать»**).
2. Распакуйте.
3. В одном окне: `ou-netd.exe 127.0.0.1:7777 60`
4. В другом: `ou-netclient.exe 127.0.0.1:7777 50`

---

## Посмотреть мир «глазами»

Сетевой рендер-клиент рисует то, что сервер отправил по сети:

```powershell
# окно 1: сервер
ou-netd.exe 127.0.0.1:7777 20

# окно 2: рендер (сохранит 4 кадра в render-net.png)
cargo run -p ou-render --release --bin ou-netrender -- 127.0.0.1:7777 8 4
```

---

## Другие демонстрации (без сети)

| Что | Команда |
|---|---|
| Генерация звёздных систем | `cargo run -p ou-sim --bin ou-worldgen` |
| Посадка на планету | `cargo run -p ou-sim --bin ou-landing-demo` |
| Шардирование и миграции | `cargo run -p ou-shard --bin ou-shard-demo` |
| Сохранение и перезапуск мира | `cargo run -p ou-server --bin ou-persist-demo` |
| Рынок и сделки | `cargo run -p ou-economy --bin ou-market-demo` |
| Симуляция экономики | `cargo run -p ou-economy --bin ou-market-sim` |
| Бой, щиты, территории | `cargo run -p ou-combat --bin ou-combat-demo` |
| Воксельный редактор | `cargo run -p ou-build --bin ou-build-demo` |
| Скриптинг конструкций | `cargo run -p ou-script --bin ou-script-demo` |
| Рендер сцен в PNG | `cargo run -p ou-render --release -- all 1280 720` |
| Воксельная планета | `cargo run -p ou-render --release -- voxel 900 506` |

---

## Проверка, что всё работает

```powershell
cd server
cargo test --workspace              # 215 тестов Rust
```

```powershell
# проверка, что C++-клиент читает протокол сервера
powershell -File tools/test-client.ps1
```

---

## Порт занят?

Ошибка `не удалось занять UDP-порт` — выберите другой порт:

```powershell
ou-netd.exe 127.0.0.1:7801 60
ou-netclient.exe 127.0.0.1:7801 50
```

## Что дальше с графикой

Когда будет установлен **Unreal Engine 5.5+**, откроется проект `client/`:

```powershell
powershell -File tools/build-client.ps1
```

Сейчас графику заменяет собственный рендерер на Rust (`ou-render`), который
рисует настоящие кадры — их видно на сайте в разделе «Движок».
