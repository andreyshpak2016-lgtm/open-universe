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

protected:
	virtual void BeginPlay() override;
	virtual void Tick(float DeltaSeconds) override;
	virtual void SetupPlayerInputComponent(class UInputComponent* PlayerInputComponent) override;

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
};
