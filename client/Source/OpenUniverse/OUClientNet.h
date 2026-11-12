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
struct FOUShotEvent
{
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

	/** Разбор пакета; false — пакет не наш или повреждён. */
	bool ParseSnapshot(const TArray<uint8>& Body, FOUSnapshot& Out);

	/** Отправить сырой пакет с токеном. */
	bool SendPacket(const TArray<uint8>& Body);

	/** Отправить приветствие серверу. */
	bool SendHello();
};
