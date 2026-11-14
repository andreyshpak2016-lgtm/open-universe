// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "Components/ActorComponent.h"
#include "OpenUniverseTypes.h"
#include "OUInterop.h"
#include "OUClientNet.generated.h"

/** Статус подключения к серверу. */
UENUM(BlueprintType)
enum class EOUConnState : uint8
{
	Disconnected UMETA(DisplayName = "Отключён"),
	Connecting UMETA(DisplayName = "Подключение"),
	Connected UMETA(DisplayName = "Подключён"),
	Error UMETA(DisplayName = "Ошибка")
};

/** Снимок, пришедший от сервера (baseline или дельта). */
USTRUCT(BlueprintType)
struct FOUSnapshot
{
	GENERATED_BODY()

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe")
	int64 Tick = 0;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe")
	int32 AckInputSeq = 0;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe")
	bool bBaseline = false;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe")
	TArray<FOUEntityState> Entities;
};

/** Правка конструкции, пришедшая от сервера (авторитетное строительство). */
USTRUCT(BlueprintType)
struct FOUBuildEdit
{
	GENERATED_BODY()

	/** Конструкция, к которой относится правка. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	int32 Entity = 0;

	/** Кто применил. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	int32 Author = 0;

	/** Действие: 0 — поставить, 1 — удалить, 2 — покрасить. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	int32 Action = 0;

	/** Координаты вокселя в сетке конструкции. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	int32 X = 0;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	int32 Y = 0;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	int32 Z = 0;

	/** Цвет (0..255). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	int32 R = 0;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	int32 G = 0;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	int32 B = 0;

	/** Номер правки (для подтверждения). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	int64 Revision = 0;
};

/** Событие выстрела, полученное от сервера (для трассера и эффектов). */
USTRUCT(BlueprintType)
struct FOUShotEvent{
	GENERATED_BODY()

	/** Кто стрелял. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	int32 Attacker = 0;

	/** По кому (0 — промах без цели). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	int32 Target = 0;

	/** Попал ли. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	bool bHit = false;

	/** Прошло урона всего. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	int32 Damage = 0;

	/** Цель уничтожена. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	bool bDestroyed = false;
};

/** Состояние корпуса сущности от сервера (щит/броня). */
USTRUCT(BlueprintType)
struct FOUHullState
{
	GENERATED_BODY()

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	int32 Entity = 0;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	int32 Shield = 0;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	int32 Armor = 0;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	bool bDestroyed = false;
};

/** Запись инвентаря игрока (id предмета и количество). */
USTRUCT(BlueprintType)
struct FOUInventoryItem
{
	GENERATED_BODY()
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	int32 Item = 0;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	int64 Qty = 0;
};

/** Уровень стакана: цена и объём (для окна рынка). */
USTRUCT(BlueprintType)
struct FOUOrderLevel
{
	GENERATED_BODY()
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	int32 Item = 0;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	int64 Price = 0;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	int64 Qty = 0;
};

/** Предмет-вход рецепта (id и количество). */
USTRUCT(BlueprintType)
struct FOURecipeItem
{
	GENERATED_BODY()
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	int32 Item = 0;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	int64 Qty = 0;
};

/** Рецепт производства (для окна «Производство»). */
USTRUCT(BlueprintType)
struct FOURecipe
{
	GENERATED_BODY()
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	int32 Id = 0;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	FString Name;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	TArray<FOURecipeItem> Inputs;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	int32 OutItem = 0;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	int64 OutQty = 0;
};

/** Воронка (яма) на планете, применённая сервером. */USTRUCT(BlueprintType)
struct FOUTerrainDent
{
	GENERATED_BODY()

	/** Номер воронки (слот): повторный приход = углубление той же ямы. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Terrain")
	int32 Slot = 0;

	/** Направление центра воронки (серверные оси). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Terrain")
	double X = 0.0, Y = 0.0, Z = 0.0;

	/** Косинус углового радиуса воронки. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Terrain")
	double CosRadius = 1.0;

	/** Глубина (в единицах длины). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Terrain")
	double Depth = 0.0;
};

/** Поза чужого пешего персонажа от сервера. */
USTRUCT(BlueprintType)
struct FOUCharacterState
{
	GENERATED_BODY()

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Character")
	int32 Entity = 0;

	/** Позиция в СЕРВЕРНЫХ осях и сантиметрах UE (как у сущностей). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Character")
	FVector Position = FVector::ZeroVector;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Character")
	float Yaw = 0.f;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Character")
	bool bMoving = false;
};

/** Тело звёздной системы от сервера (для отрисовки). */
USTRUCT(BlueprintType)
struct FOUSystemBody
{
	GENERATED_BODY()

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|System")
	int32 Id = 0;

	/** Тип: 0 звезда, 1 планета, 2 луна, 3 газовый гигант, 4 пояс, 5 чёрная дыра. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|System")
	int32 Kind = 0;

	/** Тип поверхности (палитра): 0 Terran, 1 Desert, 2 Ice, 3 Volcanic, 4 GasGiant, 5 Airless. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|System")
	int32 Surface = 0;

	/** Центр орбиты в СЕРВЕРНЫХ единицах (0 — тело неподвижно). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|System")
	FVector Center = FVector::ZeroVector;

	/** Радиус орбиты, в СЕРВЕРНЫХ единицах (0 — неподвижно). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|System")
	float Orbit = 0.f;

	/** Начальная фаза орбиты, рад. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|System")
	float Phase = 0.f;

	/** Угловая скорость орбиты, рад/с. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|System")
	float Omega = 0.f;

	/** Радиус, в СЕРВЕРНЫХ единицах. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|System")
	float Radius = 0.f;

	/** Амплитуда рельефа, СЕРВЕРНЫЕ единицы (0 — гладкая сфера). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|System")
	double Amplitude = 0.0;

	/** Сид рельефа тела (общая формула высоты с сервером). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|System")
	int64 Seed = 0;
};

/** Шаблон постройки (blueprint) для редактора в игре. */
USTRUCT(BlueprintType)
struct FOUBlueprint
{
	GENERATED_BODY()

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	int32 Id = 0;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	FString Name;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	FString Description;
};

/** Шаблон скрипта из библиотеки (для окна «Скрипты»). */
USTRUCT(BlueprintType)
struct FOUScriptTemplate
{
	GENERATED_BODY()

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Script")
	int32 Id = 0;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Script")
	FString Name;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Script")
	FString Description;
};

/** Территория (зона) системы: владение, статус, налог — для HUD и миникарты. */
USTRUCT(BlueprintType)
struct FOUTerritory
{
	GENERATED_BODY()

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Territory")
	int32 Id = 0;

	/** Владелец: 0 — ничей, иначе id игрока-владельца. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Territory")
	int32 Owner = 0;

	/** Статус: 0 мирная (бой запрещён), 1 спорная, 2 укреплённая. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Territory")
	int32 Status = 0;

	/** Центр, в СЕРВЕРНЫХ единицах (для миникарты). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Territory")
	FVector Center = FVector::ZeroVector;

	/** Радиус зоны, в СЕРВЕРНЫХ единицах. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Territory")
	double Radius = 0.0;

	/** Щит контроля (0..100). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Territory")
	int32 ControlShield = 0;

	/** Налог с торговли, проценты. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Territory")
	int32 TaxPercent = 0;

	/** Сколько налога уже собрано владельцем. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Territory")
	int64 CollectedTax = 0;
};

/**
 * Сетевой клиент Open Universe.
 *
 * Говорит с нашим Rust-сервером (`ou-netd`) по UDP тем же бинарным протоколом.
 * Формат специально простой и совпадает с `crates/protocol`:
 *
 *   пакет: [token: u64 LE][len: u32 LE][body...]
 *   body:  [tag: u8][...]
 *
 * Теги: 1=hello, 2=input, 5=snapshot(v2), 4=ack.
 * Снимок v2: tick(varint), ack(varint), baseline(u8), count(varint),
 *            затем на сущность: id(varint), pos(3×zigzag varint), vel(там же),
 *            yaw(f32), pitch(f32).
 *
 * Реализация нарочно без сторонних зависимостей: только сокеты UE.
 */
UCLASS(ClassGroup = (OpenUniverse), meta = (BlueprintSpawnableComponent))
class OPENUNIVERSE_API UOUClientNet : public UActorComponent
{
	GENERATED_BODY()

public:
	UOUClientNet();

	/** Адрес сервера. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Net")
	FString ServerAddress = TEXT("127.0.0.1");

	/** Порт сервера (совпадает с ou-netd). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Net")
	int32 ServerPort = 7777;

	/** Токен сессии (должен совпадать с сервером). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Net")
	int64 SessionToken = 0x376085675394cb58;

	/** Имя в сети. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Net")
	FString PlayerName = TEXT("pilot");

	/** Текущее состояние подключения. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Net")
	EOUConnState State = EOUConnState::Disconnected;

	/** Id своей сущности на сервере (0 — сервер ещё не ответил). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Net")
	int32 OwnEntityId = 0;

	/** Последний полученный снимок. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Net")
	FOUSnapshot LatestSnapshot;

	/** Сколько снимков принято (для диагностики). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Net")
	int32 SnapshotsReceived = 0;

	/** Очередь авторитетных правок конструкций от сервера (забирается актором). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	TArray<FOUBuildEdit> PendingBuildEdits;

	/** Очередь воронок от сервера (забирается планетой для перестройки рельефа). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Terrain")
	TArray<FOUTerrainDent> PendingDents;

	/** Кредиты игрока (приходят от сервера). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	int64 Credits = 0;

	/** Инвентарь игрока. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	TArray<FOUInventoryItem> InventoryItems;

	/** Верх стакана покупок и продаж (для окна рынка). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	TArray<FOUOrderLevel> MarketBids;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	TArray<FOUOrderLevel> MarketAsks;

	/** Приходило ли хоть раз состояние экономики. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	bool bMarketReceived = false;

	/**
	 * Реальная задержка игрока до сервера (мс), измеренная по кругу.
	 *
	 * Считаем по подтверждениям ввода: сервер в снимке возвращает `ack_input_seq`,
	 * клиент знает, когда отправил этот кадр, — разница и есть RTT. Это настоящий
	 * пинг игрока (в отличие от числа из `status.json`, которое мерил CI-раннер).
	 */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Net")
	float PingMs = -1.f;

	/** Сглаженный пинг (мс) — для HUD, чтобы число не дёргалось. -1 — нет данных. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Net")
	float SmoothedPingMs = -1.f;

	/** Отправить запрос пинга серверу (эхо-ответ даст реальный RTT). */
	void SendPing();

	/** Последняя добыча при копании (id предмета, количество, слой, глубина ямы). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	int32 LastDigItem = 0;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	int64 LastDigQty = 0;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	int32 LastDigLayer = 0;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	float LastDigHoleDepth = 0.f;

	/** Время последней добычи (мир, сек) — для всплывающей подсказки в HUD. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	float LastDigTime = -100.f;

	/** Всего добыто за сессию по каждому слою (0 руда,1 лёд,2 кристалл,3 металл). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	TArray<int64> DigTotalByLayer;

	// --- Производство (крафт) ---

	/** Рецепты производства, присланные сервером (для окна «Производство»). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	TArray<FOURecipe> Recipes;

	/** Приходил ли каталог рецептов. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	bool bRecipesReceived = false;

	/** Последний результат крафта. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	bool bLastCraftOk = false;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	int32 LastCraftItem = 0;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	int64 LastCraftQty = 0;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	FString LastCraftReason;

	/** Время последнего крафта (мир) — для всплывающей подсказки. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	float LastCraftTime = -100.f;

	/** Отправить запрос на выполнение рецепта. */
	void SendCraft(int32 RecipeId);

	/** Последний результат команды строительства: принята ли. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	bool bLastBuildAccepted = false;

	/** Может ли этот клиент строить (права на свою конструкцию). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	bool bCanBuild = true;

	/** Очередь событий выстрела от сервера (для трассеров/эффектов). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	TArray<FOUShotEvent> PendingShots;

	/** Щит/броня своей сущности (для HUD). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	FOUHullState OwnHull;

	/** Своя сущность уничтожена (ждём возрождения). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	bool bOwnDestroyed = false;

	/** Тик сервера, на котором возродимся (0 — не ждём). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	int64 OwnRespawnTick = 0;

	/** Своя сущность неуязвима до этого тика (0 — нет). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	int64 OwnInvulnUntil = 0;

	/** Время последнего полученного тика (для отсчёта возрождения). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Net")
	int64 LastServerTick = 0;

	/** Позы чужих пеших персонажей, пришедшие за последний кадр. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Character")
	TArray<FOUCharacterState> RemoteCharacters;

	/** Описание системы от сервера (пусто — ещё не пришло). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|System")
	TArray<FOUSystemBody> SystemBodies;

	/** Имя системы от сервера. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|System")
	FString SystemName;

	/** Пришло ли описание системы (чтобы не перестраивать сцену зря). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|System")
	bool bSystemReceived = false;

	// --- Территории (владение и налоги) ---

	/** Территории системы от сервера (владение, статус, налог) для HUD. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Territory")
	TArray<FOUTerritory> Territories;

	/** Приходил ли список территорий. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Territory")
	bool bTerritoriesReceived = false;

	/** Время последнего изменения территорий (мир) — для всплывающей подсказки. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Territory")
	float LastTerritoryTime = -100.f;

	/** Территория, в которой сейчас игрок (id, 0 — вне зон). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Territory")
	int32 CurrentTerritoryId = 0;

	/** Игрок сейчас в мирной зоне (бой запрещён). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Territory")
	bool bInPeacefulZone = false;

	/**
	 * Обновить, в какой территории находится игрок. Вызывается из pawn'а,
	 * который знает свою позицию (в серверных единицах).
	 */
	void UpdateTerritoryAt(double ServerX, double ServerY, double ServerZ);

	/** Отправить запрос захвата территории (id). */
	void SendCapture(int32 Id);

	// --- Скрипты (автопилоты, турели, логистика) ---

	/** Библиотека шаблонов скриптов от сервера (для окна «Скрипты»). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Script")
	TArray<FOUScriptTemplate> ScriptTemplates;

	/** Приходила ли библиотека скриптов. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Script")
	bool bScriptsReceived = false;

	/** Id шаблона, привязанного к объекту игрока (0 — нет). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Script")
	int32 ScriptTemplate = 0;

	/** Имя привязанного скрипта. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Script")
	FString ScriptName;

	/** Сколько раз скрипт исполнился. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Script")
	int64 ScriptRuns = 0;

	/** Сколько раз скрипт ошибся. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Script")
	int64 ScriptErrors = 0;

	/** Последняя ошибка скрипта (пусто, если её нет). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Script")
	FString ScriptLastError;

	/** Свежий вывод `log()` скрипта (для внутриигрового дисплея). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Script")
	TArray<FString> ScriptLogs;

	/** Пришёл ли хоть раз статус скрипта. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Script")
	bool bScriptStatusReceived = false;

	/** Отправить запрос привязки скрипта-шаблона (0 — снять). */
	void SendSetScript(int32 Template);

	// --- Шаблоны постройки (blueprints) ---

	/** Библиотека шаблонов постройки от сервера (для редактора). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	TArray<FOUBlueprint> Blueprints;

	/** Приходила ли библиотека шаблонов постройки. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	bool bBlueprintsReceived = false;

	/** Поставить шаблон постройки по id с якорем (X,Y,Z) в конструкцию Entity. */
	void SendPlaceBlueprint(int32 Id, int32 X, int32 Y, int32 Z, int32 Entity);

	// --- Боевая статистика (HUD) ---

	/** Уничтожено целей игроком. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	int64 CombatKills = 0;

	/** Суммарный нанесённый урон. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	int64 CombatDamage = 0;

	/** Заработано награды (bounty) за уничтоженные цели. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	int64 CombatBounty = 0;

	/** Приходила ли боевая статистика. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Combat")
	bool bCombatReceived = false;

	/** Подключиться к серверу. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Net")
	bool Connect();

	/** Отключиться. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Net")
	void Disconnect();

	/** Отправить кадр управления. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Net")
	void SendInput(const FOUInputFrame& Input);

	/**
	 * Отправить позу своего пешего персонажа.
	 * `active = false` — игрок вернулся в корабль (позу снимаем).
	 */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Character")
	void SendCharacter(const FVector& PositionServer, float Yaw, bool bMoving, bool bActive);

	/** Обработать входящие пакеты (вызывать раз в кадр). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Net")
	void Poll();

	/** Отправить команду строительства (поставить/удалить/покрасить воксель). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Build")
	void SendBuild(int32 Entity, int32 Action, int32 X, int32 Y, int32 Z, int32 R, int32 G, int32 B);

	/** Отправить запрос копания грунта в направлении (серверные оси). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Terrain")
	void SendDig(int32 Slot, double X, double Y, double Z, double CosRadius, double Depth);

	/** Отправить ордер на рынок (0 — покупка, 1 — продажа). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Economy")
	void SendMarketOrder(int32 Side, int32 Item, int64 Price, int64 Qty);

	/** Подтвердить серверу приём правок до этой ревизии (чтобы не слал снова). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Build")
	void SendBuildAck(int32 Entity, int64 Revision);

	/** Подключён ли клиент. */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Net")
	bool IsConnected() const { return State == EOUConnState::Connected; }

protected:
	virtual void BeginPlay() override;
	virtual void EndPlay(const EEndPlayReason::Type Reason) override;

private:
	/** Сокет и адрес сервера. */
	class FSocket* Socket = nullptr;
	TSharedPtr<class FInternetAddr> ServerEndpoint;

	/** Счётчик отправленных кадров. */
	int32 InputSeq = 0;

	/** Время отправки кадров по их seq (для измерения RTT). Секунды мира. */
	TMap<int32, double> InputSentTime;

	/** Время отправки пингов по их nonce (для измерения RTT). */
	TMap<int32, double> PingSentTime;

	/** Счётчик пингов и время следующего. */
	int32 PingNonce = 0;
	double NextPingTime = 0.0;

	/** Разбор пакета; false — пакет не наш или повреждён. */
	bool ParseSnapshot(const TArray<uint8>& Body, FOUSnapshot& Out);

	/** Отправить сырой пакет с токеном. */
	bool SendPacket(const TArray<uint8>& Body);

	/** Отправить приветствие серверу. */
	bool SendHello();
};
