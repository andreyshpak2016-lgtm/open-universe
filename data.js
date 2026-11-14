/* Данные плана для сайта. Работает при открытии index.html напрямую (file://),
   без локального сервера. Синхронизируется с plan.json.

   Двуязычно: поле — русский текст, поле *_en — английский (язык по умолчанию).
   app.js выбирает язык через currentLang(). */
window.PLAN = {
  "project": "Open Universe",
  "tagline": "Один общий космос. Построен игроками.",
  "tagline_en": "One shared universe. Built by players.",
  "updated": "2026-11-14",
  "rule": "«Готово» ставится только тому, что игрок реально видит в игре (клиент UE5 + живой сервер). Написано и протестировано, но к игре не подключено — это «в работе».",
  "rule_en": "\"Done\" is only for what the player actually sees in the game (UE5 client + live server). Written and tested but not wired into the game counts as \"in progress\".",
  "phases": [
    {
      "id": "p0",
      "title": "Pre-production",
      "title_en": "Pre-production",
      "goal": "Определить игру, технологии и команду.",
      "goal_en": "Define the game, the technology and the team.",
      "tasks": [
        { "title": "Формулировка концепта и столпов игры", "title_en": "Concept and game pillars definition", "status": "done" },
        { "title": "Дизайн-документ (GDD) — черновик", "title_en": "Design document (GDD) — draft", "status": "done" },
        { "title": "Выбор игрового движка и стека (ADR-0001: UE5 + Rust)", "title_en": "Engine and stack choice (ADR-0001: UE5 + Rust)", "status": "done" },
        { "title": "Сайт-витрина с живым планом прогресса", "title_en": "Showcase site with a live progress plan", "status": "done" },
        { "title": "Схемы сетевого протокола клиент↔сервер", "title_en": "Client↔server network protocol schemas", "status": "done" },
        { "title": "Экономическая модель и симуляция рынка", "title_en": "Economic model and market simulation", "status": "done" },
        { "title": "Исследование рынка и конкурентов", "title_en": "Market and competitor research", "status": "done" },
        { "title": "Команда и распределение ролей", "title_en": "Team and role distribution", "status": "done" }
      ]
    },
    {
      "id": "p1",
      "title": "Ядро движка (UE5)",
      "title_en": "Engine core (UE5)",
      "goal": "Клиентский модуль на Unreal Engine 5: сферический рельеф планеты, floating origin.",
      "goal_en": "Client module on Unreal Engine 5: spherical planet terrain, floating origin.",
      "tasks": [
        { "title": "Каркас проекта на Unreal Engine 5 — модуль собран, открывается 3D-сцена", "title_en": "Unreal Engine 5 project skeleton — module builds, 3D scene opens", "status": "done" },
        { "title": "Стартовое меню персонажа и выбор сервера", "title_en": "Startup character menu and server selection", "status": "done" },
        { "title": "Floating origin / double-precision (AUOOriginManager)", "title_en": "Floating origin / double-precision (AUOOriginManager)", "status": "done" },
        { "title": "Сферическая планета с рельефом (общая формула с сервером)", "title_en": "Spherical planet with terrain (formula shared with the server)", "status": "done" },
        { "title": "Ландшафт: континенты, равнины, горы; биомы", "title_en": "Terrain: continents, plains, mountains; biomes", "status": "done" },
        { "title": "Землеподобный рельеф: зоны по высоте, камень на склонах, клиентские детали", "title_en": "Earth-like terrain: height zones, rock on slopes, client-side relief detail", "status": "done" },
        { "title": "Атмосфера, скайбокс и постобработка", "title_en": "Atmosphere, skybox and post-processing", "status": "done" },
        { "title": "Вид от первого лица (персонаж и корабль)", "title_en": "First-person view (character and ship)", "status": "done" },
        { "title": "Клиентский каркас: сетевой рендер мира", "title_en": "Client skeleton: networked world rendering", "status": "done" },
        { "title": "Копание грунта в игре (ЛКМ, сервер-авторитетно, с репликацией)", "title_en": "Terrain digging in game (LMB, server-authoritative, replicated)", "status": "done" },
        { "title": "Стартовый комплекс: площадка, лестницы, постройки, единый цвет пола", "title_en": "Start complex: pad, staircases, buildings, single-colour floor", "status": "done" },
        { "title": "Система небесных тел (генератор есть; в игре одна планета)", "title_en": "Celestial body system (generator exists; the game has one planet)", "status": "in_progress" }
      ]
    },
    {
      "id": "p2",
      "title": "Физика и сеть",
      "title_en": "Physics and networking",
      "goal": "Детерминированная физика и репликация для MMO.",
      "goal_en": "Deterministic physics and replication for an MMO.",
      "tasks": [
        { "title": "Детерминированная серверная физика", "title_en": "Deterministic server physics", "status": "done" },
        { "title": "Клиентский предсказатель и реконсиляция", "title_en": "Client prediction and reconciliation", "status": "done" },
        { "title": "Снапшот-интерполяция удалённых сущностей", "title_en": "Snapshot interpolation of remote entities", "status": "done" },
        { "title": "Ньютоновская модель полёта в космосе", "title_en": "Newtonian spaceflight model", "status": "done" },
        { "title": "Гравитация планет: обратные квадраты у поверхности, инерция в космосе", "title_en": "Planet gravity: inverse-square near the surface, inertia in space", "status": "done" },
        { "title": "Появление игрока на поверхности планеты (суша, ландшафт)", "title_en": "Player spawn on the planet surface (land, terrain)", "status": "done" },
        { "title": "Выход из корабля (удержание F) и пешие прогулки по планете", "title_en": "Exit ship (hold F) and walk on the planet", "status": "done" },
        { "title": "Interest management (репликация только нужного)", "title_en": "Interest management (replicate only what is needed)", "status": "done" },
        { "title": "Базовые сетевые протоколы (бинарный + дельты + varint)", "title_en": "Base network protocols (binary + deltas + varint)", "status": "done" },
        { "title": "UE5-клиент в сети: подключение, ввод, реконсиляция, другие игроки", "title_en": "UE5 client online: connect, input, reconciliation, other players", "status": "done" },
        { "title": "Реальная задержка игрока (ping/pong) в HUD", "title_en": "Real player latency (ping/pong) in the HUD", "status": "done" },
        { "title": "Меню ESC: продолжить / настройки / отлучиться от сервера", "title_en": "ESC menu: continue / settings / leave server", "status": "done" }
      ]
    },
    {
      "id": "p3",
      "title": "Single-shard бэкенд (Rust)",
      "title_en": "Single-shard backend (Rust)",
      "goal": "Единый непрерывный мир без «серверов-клонов», авторитетная симуляция на Rust.",
      "goal_en": "One continuous world without clone servers, authoritative simulation in Rust.",
      "tasks": [
        { "title": "Серверный каркас на Rust (tick loop, зоны интереса)", "title_en": "Rust server skeleton (tick loop, interest zones)", "status": "done" },
        { "title": "Пространственный индекс (grid) для зон интереса", "title_en": "Spatial index (grid) for interest zones", "status": "done" },
        { "title": "Персистентность мира в живой игре (автосейв, восстановление)", "title_en": "World persistence in the live game (autosave, recovery)", "status": "done" },
        { "title": "Мониторинг, логи, метрики", "title_en": "Monitoring, logs, metrics", "status": "done" },
        { "title": "Шардирование по системам (есть в crates/shard, к игре не подключено)", "title_en": "Sharding by system (exists in crates/shard, not wired into the game)", "status": "in_progress" },
        { "title": "Микросервисная архитектура (auth, мир, чат, рынок)", "title_en": "Microservice architecture (auth, world, chat, market)", "status": "in_progress" },
        { "title": "Оркестрация в Kubernetes, автомасштабирование", "title_en": "Kubernetes orchestration, autoscaling", "status": "todo" }
      ]
    },
    {
      "id": "p4",
      "title": "Строительство и редактор",
      "title_en": "Building and editor",
      "goal": "Полноценный воксельный редактор конструкций.",
      "goal_en": "A full voxel construction editor.",
      "tasks": [
        { "title": "Постановка блоков в игре (сервер-авторитетно)", "title_en": "Placing blocks in the game (server-authoritative)", "status": "done" },
        { "title": "Сетка деталей и «ядра» конструкций", "title_en": "Part and construction core grid", "status": "done" },
        { "title": "Валидация физической целостности", "title_en": "Physical integrity validation", "status": "done" },
        { "title": "Совместное строительство: репликация правок, права, подтверждения", "title_en": "Co-op building: edit replication, permissions, confirmations", "status": "done" },
        { "title": "Редактор в игре: ставить / удалять / красить (прицел, Е/R)", "title_en": "In-game editor: place / remove / paint (crosshair, E/R)", "status": "done" },
        { "title": "Призрак блока под прицелом и постановка «куда смотришь» (как в DU)", "title_en": "Ghost block under the crosshair, place where you look (like DU)", "status": "done" },
        { "title": "Дублирование и шаблоны (blueprints) — библиотека в редакторе (Z/X — выбрать, G — поставить)", "title_en": "Duplication and blueprints — library in the editor (Z/X to pick, G to place)", "status": "done" }
      ]
    },
    {
      "id": "p5",
      "title": "Экономика и производство",
      "title_en": "Economy and production",
      "goal": "Экономика, которую двигают сами игроки.",
      "goal_en": "An economy driven by the players themselves.",
      "note": "Петля замкнута в игре: копание даёт руду, рынок (M) и производство работают на живом сервере.",
      "note_en": "The loop is closed in-game: digging yields ore, and the market (M) and production run on the live server.",
      "tasks": [
        { "title": "Добыча ресурсов (копание даёт руду слоями по глубине)", "title_en": "Resource extraction (digging yields ore in depth layers)", "status": "done" },
        { "title": "Переработка и производственные цепочки (в игре, окно M)", "title_en": "Refining and production chains (in game, window M)", "status": "done" },
        { "title": "Игрок-рынок: ордера, стакан, сделки (в игре, клавиша M)", "title_en": "Player market: orders, order book, trades (in game, key M)", "status": "done" },
        { "title": "Отображение добычи и производства в HUD", "title_en": "Digging and production shown in the HUD", "status": "done" },
        { "title": "Модули контейнеров, топлива, энергии", "title_en": "Container, fuel and energy modules", "status": "in_progress" },
        { "title": "Экономическая симуляция и баланс", "title_en": "Economic simulation and balance", "status": "in_progress" }
      ]
    },
    {
      "id": "p6",
      "title": "Полёт и звёздные системы",
      "title_en": "Flight and star systems",
      "goal": "От поверхности планеты до межзвёздных перелётов.",
      "goal_en": "From a planet surface to interstellar travel.",
      "tasks": [
        { "title": "Атмосферный → орбитальный переход (посадка)", "title_en": "Atmospheric → orbital transition (landing)", "status": "done" },
        { "title": "Соседние тела системы: луна, планета, звезда, газовый гигант видны в небе", "title_en": "Neighbouring bodies: moon, planet, star and gas giant are visible in the sky", "status": "done" },
        { "title": "Сверхсвет (удержание Shift): сверхскоростной перелёт между телами", "title_en": "Supercruise (hold Shift): high-speed travel between bodies", "status": "done" },
        { "title": "Гравитация и посадка на соседних телах (луна/планета — с рельефом, гигант/звезда — сфера)", "title_en": "Gravity and landing on neighbouring bodies (moon/planet with terrain, giant/star as spheres)", "status": "done" },
        { "title": "Рельеф на соседних телах (процедурный меш по общей с сервером формуле)", "title_en": "Terrain on neighbouring bodies (procedural mesh from the shared height formula)", "status": "done" },
        { "title": "Генерация звёздных систем и поясов (генератор есть; обитаемая планета одна)", "title_en": "Star system and belt generation (generator exists; one habitable planet)", "status": "in_progress" },
        { "title": "Перемещения между системами (межзвёздные)", "title_en": "Travel between systems (interstellar)", "status": "todo" },
        { "title": "Орбитальные станции и шлюзы", "title_en": "Orbital stations and gateways", "status": "todo" },
        { "title": "Опасные зоны: чёрные дыры, радиация", "title_en": "Hazard zones: black holes, radiation", "status": "todo" }
      ]
    },
    {
      "id": "p7",
      "title": "Бой и территории",
      "title_en": "Combat and territory",
      "goal": "Конфликты за ресурсы и пространство.",
      "goal_en": "Conflict over resources and space.",
      "tasks": [
        { "title": "Щиты, броня, урон по секциям; смерть и возрождение (в игре)", "title_en": "Shields, armor, sectional damage; death and respawn (in game)", "status": "done" },
        { "title": "Дроны-цели: спавн, отстрел, возрождение (в игре)", "title_en": "Target drones: spawn, shoot, respawn (in game)", "status": "done" },
        { "title": "Взрывные воронки от боя: уничтожение у поверхности оставляет кратер", "title_en": "Blast craters from combat: destruction near the surface leaves a crater", "status": "done" },
        { "title": "Захват и удержание территорий (в игре: зоны у базы и соседних тел, клавиша T)", "title_en": "Territory capture and holding (in game: zones at the base and neighbours, key T)", "status": "done" },
        { "title": "Право собственности и налоги (налог с торговли идёт владельцу зоны)", "title_en": "Ownership and taxes (trade tax goes to the zone owner)", "status": "done" },
        { "title": "Орбитальные и планетарные сражения (орбитальные цели, награда за уничтожение, HUD боя)", "title_en": "Orbital and planetary battles (orbital targets, destroy bounty, combat HUD)", "status": "done" },
        { "title": "Античит и валидация на сервере (детерминированные исходы выстрелов)", "title_en": "Anti-cheat and server-side validation (deterministic shot outcomes)", "status": "done" }
      ]
    },
    {
      "id": "p8",
      "title": "Скриптинг и игрок-контент",
      "title_en": "Scripting and player content",
      "goal": "Игроки программируют свои конструкции.",
      "goal_en": "Players program their own constructions.",
      "tasks": [
        { "title": "Lua-подобный скриптинг (шаблоны привязываются в игре, клавиша L)", "title_en": "Lua-like scripting (templates are bound in game, key L)", "status": "done" },
        { "title": "Автопилоты, турели, логистика на скриптах (библиотека + авторитетный запуск)", "title_en": "Scripted autopilots, turrets and logistics (library + authoritative run)", "status": "done" },
        { "title": "Sandbox-ограничения и безопасность", "title_en": "Sandbox limits and security", "status": "done" },
        { "title": "Внутриигровые экраны и дисплеи (окно «Скрипты» с выводом log())", "title_en": "In-game screens and displays (Scripts window with log() output)", "status": "done" },
        { "title": "Мастерская и торговля шаблонами (публикация постройки за кредиты, покупка, окно K)", "title_en": "Workshop and blueprint trading (publish a build for credits, buy, key K)", "status": "done" }
      ]
    },
    {
      "id": "p9",
      "title": "Социальное и мета",
      "title_en": "Social and meta",
      "goal": "Организации, прогрессия и мета-системы.",
      "goal_en": "Organizations, progression and meta systems.",
      "tasks": [
        { "title": "Организации, ранги, дипломатия (гильдии: создание/вступление/выход, ранги, союзы/войны, окно O)", "title_en": "Organizations, ranks, diplomacy (guilds: create/join/leave, ranks, alliances/wars, key O)", "status": "done" },
        { "title": "Голосовой и текстовый чат", "title_en": "Voice and text chat", "status": "todo" },
        { "title": "Внутриигровая валюта и рынок предметов (MTX)", "title_en": "In-game currency and item market (MTX)", "status": "todo" },
        { "title": "Таланты/прогрессия персонажа", "title_en": "Character talents and progression", "status": "todo" },
        { "title": "Обучение для новичков (tutorial)", "title_en": "New player tutorial", "status": "todo" }
      ]
    },
    {
      "id": "p10",
      "title": "Запуск и live-ops",
      "title_en": "Launch and live-ops",
      "goal": "От альфы до живой постоянно развивающейся игры.",
      "goal_en": "From alpha to a living, continually evolving game.",
      "tasks": [
        { "title": "Закрытая альфа", "title_en": "Closed alpha", "status": "todo" },
        { "title": "Открытая бета и нагрузочные тесты", "title_en": "Open beta and load tests", "status": "todo" },
        { "title": "Издатель/маркетинг и трейлер", "title_en": "Publisher, marketing and trailer", "status": "todo" },
        { "title": "Релиз и сезонный контент", "title_en": "Release and seasonal content", "status": "todo" },
        { "title": "Поддержка, модерация, дорожная карта сообщества", "title_en": "Support, moderation, community roadmap", "status": "todo" }
      ]
    }
  ]
};
