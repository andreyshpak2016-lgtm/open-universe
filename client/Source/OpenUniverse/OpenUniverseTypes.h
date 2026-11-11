// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "OpenUniverseTypes.generated.h"

/**
 * Базовые типы мира Open Universe.
 *
 * Здесь живут данные, которые приходят по сети из нашего Rust-сервера. Типы
 * намеренно «плоские» (POD), чтобы совпадать с форматом провода один-к-одному.
 */

/** Идентификатор сущности (совпадает с NetId на сервере). */
USTRUCT(BlueprintType)
struct FOUEntityState
{
	GENERATED_BODY()

	/** Сетевой id. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe")
	int32 Id = 0;

	/** Позиция в локальных координатах (floating origin), сантиметры UE. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe")
	FVector Position = FVector::ZeroVector;

	/** Скорость, см/с. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe")
	FVector Velocity = FVector::ZeroVector;

	/** Курс (радианы). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe")
	float Yaw = 0.f;

	/** Тангаж (радианы). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe")
	float Pitch = 0.f;
};

/** Кадр управления игрока (уходит на сервер). */
USTRUCT(BlueprintType)
struct FOUInputFrame
{
	GENERATED_BODY()

	/** Монотонный номер кадра (для реконсиляции). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe")
	int32 Seq = 0;

	/** Тяга, −1..1. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe")
	float Throttle = 0.f;

	/** Рыскание, −1..1. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe")
	float Yaw = 0.f;

	/** Тангаж, −1..1. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe")
	float Pitch = 0.f;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe")
	bool bFire = false;
};

/**
 * Параметры полёта.
 *
 * ВАЖНО: значения должны совпадать с `ShipParams::default()` на сервере
 * (`crates/sim/src/lib.rs`). Иначе предсказание разойдётся с авторитетным
 * состоянием.
 */
USTRUCT(BlueprintType)
struct FOUShipParams
{
	GENERATED_BODY()

	/** Ускорение тяги, ед/с² (сервер: max_thrust = 45). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe")
	float MaxThrust = 45.f;

	/** Макс. угловая скорость рыскания, рад/с (сервер: 1.2). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe")
	float MaxYawRate = 1.2f;

	/** Макс. угловая скорость тангажа, рад/с (сервер: 1.0). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe")
	float MaxPitchRate = 1.0f;

	/** Демпфирование, 1/с (сервер: 0.02). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe")
	float LinearDamping = 0.02f;

	/** Ограничение скорости, ед/с (сервер: 400). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe")
	float MaxSpeed = 400.f;

	/** Скорость мягкой посадки, ед/с (сервер: 60). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe")
	float LandingSafeSpeed = 60.f;
};
