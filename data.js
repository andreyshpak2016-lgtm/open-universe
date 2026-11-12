/* Данные плана для сайта. Работает при открытии index.html напрямую (file://),
   без локального сервера. Синхронизируется с plan.json.

   Двуязычно: поле — русский текст, поле *_en — английский (язык по умолчанию).
   app.js выбирает язык через currentLang(). */
window.PLAN = {
  "project": "Open Universe",
  "tagline": "Один общий космос. Построен игроками.",
  "tagline_en": "One shared universe. Built by players.",
  "updated": "2026-11-11",
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
      "goal": "Клиентский модуль на Unreal Engine 5: воксель, планеты, floating origin.",
      "goal_en": "Client module on Unreal Engine 5: voxels, planets, floating origin.",
      "tasks": [
        { "title": "Каркас проекта на Unreal Engine 5 — модуль собран, открывается 3D-сцена", "title_en": "Unreal Engine 5 project skeleton — module builds, 3D scene opens", "status": "done" },
        { "title": "Стартовое меню персонажа и выбор сервера", "title_en": "Startup character menu and server selection", "status": "done" },
        { "title": "Floating origin / double-precision (AUOOriginManager)", "title_en": "Floating origin / double-precision (AUOOriginManager)", "status": "done" },
        { "title": "Воксельный рендер с LOD (cube-to-sphere)", "title_en": "Voxel rendering with LOD (cube-to-sphere)", "status": "done" },
        { "title": "Планеты как сферы с искривлением воксель-сетки", "title_en": "Planets as spheres with a warped voxel grid", "status": "done" },
        { "title": "Система небесных тел (звёзды, луны, чёрные дыры)", "title_en": "Celestial body system (stars, moons, black holes)", "status": "done" },
        { "title": "Атмосферы, скайбокс и постобработка", "title_en": "Atmospheres, skybox and post-processing", "status": "done" },
        { "title": "Разрушение вокселей: копание и взрывы", "title_en": "Voxel destruction: digging and explosions", "status": "done" },
        { "title": "Клиентский каркас: сетевой рендер мира", "title_en": "Client skeleton: networked world rendering", "status": "done" }
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
        { "title": "UE5-клиент в сети: подключение, ввод, реконсиляция, другие игроки", "title_en": "UE5 client online: connect, input, reconciliation, other players", "status": "done" }
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
        { "title": "Микросервисная архитектура (auth, мир, чат, рынок)", "title_en": "Microservice architecture (auth, world, chat, market)", "status": "in_progress" },
        { "title": "Spatial-индекс и шардирование по системам", "title_en": "Spatial index and sharding by system", "status": "done" },
        { "title": "Персистентность мира (снимки, восстановление)", "title_en": "World persistence (snapshots, recovery)", "status": "done" },
        { "title": "Оркестрация в Kubernetes, автомасштабирование", "title_en": "Kubernetes orchestration, autoscaling", "status": "todo" },
        { "title": "Мониторинг, логи, метрики", "title_en": "Monitoring, logs, metrics", "status": "done" }
      ]
    },
    {
      "id": "p4",
      "title": "Строительство и редактор",
      "title_en": "Building and editor",
      "goal": "Полноценный воксельный редактор конструкций.",
      "goal_en": "A full voxel construction editor.",
      "tasks": [
        { "title": "Воксельный редактор (ставить/удалять/красить)", "title_en": "Voxel editor (place/remove/paint)", "status": "done" },
        { "title": "Сетка деталей и «ядра» конструкций", "title_en": "Part and construction core grid", "status": "done" },
        { "title": "Дублирование и шаблоны (blueprints)", "title_en": "Duplication and blueprints", "status": "done" },
        { "title": "Валидация физической целостности", "title_en": "Physical integrity validation", "status": "done" },
        { "title": "Совместное строительство (multiplayer edit): репликация правок, права, подтверждения", "title_en": "Co-op building (multiplayer edit): edit replication, permissions, confirmations", "status": "done" }
      ]
    },
    {
      "id": "p5",
      "title": "Экономика и производство",
      "title_en": "Economy and production",
      "goal": "Экономика, которую двигают сами игроки.",
      "goal_en": "An economy driven by the players themselves.",
      "tasks": [
        { "title": "Добыча ресурсов (ресурсы систем и поясов)", "title_en": "Resource extraction (system and belt resources)", "status": "done" },
        { "title": "Переработка и производственные цепочки", "title_en": "Refining and production chains", "status": "done" },
        { "title": "Игрок-рынок: ордера, стакан, логистика", "title_en": "Player market: orders, order book, logistics", "status": "done" },
        { "title": "Модули контейнеров, топлива, энергии", "title_en": "Container, fuel and energy modules", "status": "done" },
        { "title": "Экономическая симуляция и баланс", "title_en": "Economic simulation and balance", "status": "done" }
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
        { "title": "Сверхсветовые перемещения между системами", "title_en": "Superluminal travel between systems", "status": "todo" },
        { "title": "Генерация звёздных систем и поясов астероидов", "title_en": "Star system and asteroid belt generation", "status": "done" },
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
        { "title": "Турели, щиты, повреждения по секциям", "title_en": "Turrets, shields, sectional damage", "status": "done" },
        { "title": "Захват и удержание территорий", "title_en": "Territory capture and holding", "status": "done" },
        { "title": "Орбитальные и планетарные сражения", "title_en": "Orbital and planetary battles", "status": "in_progress" },
        { "title": "Право собственности и налоги", "title_en": "Ownership and taxes", "status": "done" },
        { "title": "Античит и валидация на сервере", "title_en": "Anti-cheat and server-side validation", "status": "in_progress" }
      ]
    },
    {
      "id": "p8",
      "title": "Скриптинг и игрок-контент",
      "title_en": "Scripting and player content",
      "goal": "Игроки программируют свои конструкции.",
      "goal_en": "Players program their own constructions.",
      "tasks": [
        { "title": "Встроенный Lua-подобный скриптинг", "title_en": "Built-in Lua-like scripting", "status": "done" },
        { "title": "Автопилоты, турели, логистика на скриптах", "title_en": "Scripted autopilots, turrets and logistics", "status": "done" },
        { "title": "Мастерская и торговля шаблонами", "title_en": "Workshop and blueprint trading", "status": "todo" },
        { "title": "Внутриигровые экраны и дисплеи", "title_en": "In-game screens and displays", "status": "in_progress" },
        { "title": "Sandbox-ограничения и безопасность", "title_en": "Sandbox limits and security", "status": "done" }
      ]
    },
    {
      "id": "p9",
      "title": "Социальное и мета",
      "title_en": "Social and meta",
      "goal": "Организации, прогрессия и мета-системы.",
      "goal_en": "Organizations, progression and meta systems.",
      "tasks": [
        { "title": "Организации, ранги, дипломатия", "title_en": "Organizations, ranks, diplomacy", "status": "todo" },
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
