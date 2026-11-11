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

	/** Подключиться к серверу. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Net")
	bool Connect();

	/** Отключиться. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Net")
	void Disconnect();

	/** Отправить кадр управления. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Net")
	void SendInput(const FOUInputFrame& Input);

	/** Обработать входящие пакеты (вызывать раз в кадр). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Net")
	void Poll();

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
