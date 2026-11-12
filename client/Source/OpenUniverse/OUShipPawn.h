// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Pawn.h"
#include "OpenUniverseTypes.h"
#include "OUShipPawn.generated.h"

class UStaticMeshComponent;
class USpringArmComponent;
class UCameraComponent;
class UOUClientNet;
class AOUCharacterPawn;

/**
 * Корабль игрока.
 *
 * Повторяет **ту же кинематику**, что сервер (`crates/sim::integrate`):
 * ввод → углы → направление «вперёд» → тяга → демпфирование → позиция.
 * Благодаря этому клиент может предсказывать движение между пакетами, а
 * сервер — исправлять расхождения (реконсиляция).
 *
 * Математика намеренно детерминированная (см. `OUDeterministic`).
 */
UCLASS()
class OPENUNIVERSE_API AOUShipPawn : public APawn
{
	GENERATED_BODY()

public:
	AOUShipPawn();

	/** Параметры полёта (должны совпадать с сервером). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Flight")
	FOUShipParams Flight;

	/** Размер шага симуляции (совпадает с сервером: 1/30 с). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Flight")
	float FixedDeltaSeconds = 1.f / 30.f;

	/** Текущее состояние корабля (предсказанное). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Flight")
	FOUEntityState State;

	/** Сетевой клиент (создаётся автоматически, если не задан). */
	UPROPERTY(BlueprintReadWrite, Category = "Open Universe|Net")
	UOUClientNet* Net = nullptr;

	/** Текущий ввод (заполняется в Tick). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Flight")
	FOUInputFrame CurrentInput;

	/** Один шаг предсказания: применяет ввод к состоянию. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Flight")
	void StepPrediction(const FOUInputFrame& Input, float Dt);

	/** Реконсиляция: принять авторитетное состояние с сервера. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Flight")
	void ApplyAuthoritative(const FOUEntityState& Authoritative);

	/** Включить/выключить отправку ввода на сервер (для отладки без сервера). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Net")
	void SetInputEnabled(bool bEnabled) { bInputEnabled = bEnabled; }

	/** Вернуться в корабль (вызывает персонаж при зажатии F рядом с кораблём). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Flight")
	void EnterShip(AOUCharacterPawn* FromCharacter);

	/** Выйти из корабля: создать персонажа рядом и переключить управление. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Flight")
	void ExitShip();

protected:
	virtual void BeginPlay() override;
	virtual void Tick(float DeltaSeconds) override;
	virtual void SetupPlayerInputComponent(class UInputComponent* PlayerInputComponent) override;

	/** Обработчики осей управления (клавиатура). */
	void AxisThrottle(float Value);
	void AxisYaw(float Value);
	void AxisPitch(float Value);
	void StartFire();
	void StopFire();

	/** Зажата ли F (выход из корабля / возврат в кабину). */
	void OnExitPressed();
	void OnExitReleased();

	/** Маска зажатия F: сколько секунд держат (для «зажми, чтобы выйти»). */
	float ExitHoldTime = 0.f;

	/** Зажата ли сейчас F. */
	bool bExitHeld = false;

	/** Порог удержания F, с. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Flight")
	float ExitHoldThreshold = 0.6f;

	/** Текущий персонаж (когда вышли из корабля), иначе nullptr. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Flight")
	AOUCharacterPawn* Character = nullptr;

	/** Обновить визуализацию других игроков из последнего снимка. */
	void UpdateRemotePlayers();

	/** Найти или создать актор-заглушку для чужой сущности. */
	AActor* GetOrCreateRemoteActor(int32 EntityId);

	/** Реконсиляция как в Rust-клиенте: принять авторитетное состояние,
	 * затем переиграть ещё не подтверждённые кадры ввода. */
	void ReconcileTo(const FOUEntityState& Authoritative, int32 AckSeq);

	/** Применить State к актору (позиция + ориентация), с учётом floating origin. */
	void ApplyStateToActor();

	/** Контроллер игрока как меню (или nullptr), для гейта ввода. */
	class AOUPlayerMenuController* GetMenuController() const;

	/** Вектор «вперёд» в серверных осях по текущим углам. */
	FVector ForwardVector() const;

	/** Вектор «вперёд» в серверных осях для чужой сущности. */
	static FVector ForwardVectorOf(const FOUEntityState& E);

	/** Разосланные, но ещё не подтверждённые кадры ввода (для переигровки). */
	TArray<FOUInputFrame> PendingInputs;

	/** Меш корабля (блочная конструкция — временная заглушка до воксельной сборки). */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Flight")
	UStaticMeshComponent* ShipMesh = nullptr;

	/** Штанга камеры: камера смотрит на корабль сзади-сверху. */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Flight")
	USpringArmComponent* SpringArm = nullptr;

	/** Камера игрока (вид от третьего лица). */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Flight")
	UCameraComponent* Camera = nullptr;

	/** Ближайший менеджер начала координат (floating origin). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Flight")
	class AUOOriginManager* OriginManager = nullptr;

	/** Чужие корабли: сущность → актор-заглушка. */
	UPROPERTY(Transient)
	TMap<int32, TObjectPtr<AActor>> RemoteActors;

	/** Отправлять ли ввод на сервер (включается после подключения). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Net")
	bool bInputEnabled = true;

private:
	/** Меш-заглушка чужого корабля (загружается один раз). */
	UPROPERTY(Transient)
	TObjectPtr<class UStaticMesh> RemoteShipMesh = nullptr;
};
