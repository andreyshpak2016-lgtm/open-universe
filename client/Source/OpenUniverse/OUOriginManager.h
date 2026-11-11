// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "OpenUniverseTypes.h"
#include "OUOriginManager.generated.h"

/** Событие смены начала координат (для перепривязки акторов). */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(FOUOriginShifted, const FVector&, Delta, AActor*, NewOrigin);

/**
 * Менеджер начала координат (floating origin).
 *
 * ## Проблема
 * Координаты в UE — `float` (24 бита мантиссы). На масштабе планеты (тысячи
 * километров) точность падает до сантиметров и хуже: объекты «дрожат», а при
 * удалении от нуля — рассыпаются. Это классическая проблема больших миров.
 *
 * ## Решение
 * Мир живёт в **двойных** координатах (`double`). Акторы рендерятся в
 * локальных `float`-координатах относительно подвижного начала. Когда игрок
 * уходит далеко от нуля, начало переносится, а все акторы сдвигаются на
 * дельту — визуально ничего не меняется, но точность восстанавливается.
 *
 * Наш сервер уже работает в double-координатах, поэтому этот класс — прямой
 * мост между сервером и UE.
 */
UCLASS(Blueprintable)
class OPENUNIVERSE_API AUOOriginManager : public AActor
{
	GENERATED_BODY()

public:
	AUOOriginManager();

	/**
	 * Текущее мировое начало в double-координатах.
	 * Мировая позиция актора = (double)актор + Origin.
	 */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Floating Origin")
	FVector Origin = FVector::ZeroVector;

	/**
	 * Насколько далеко от нуля допустимо уйти, прежде чем переносить начало.
	 * Значение в сантиметрах: 100_000 см = 1 км. Для мира с планетами
	 * разумный порог — десятки километров.
	 */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Floating Origin")
	float ShiftThreshold = 500000.f; // 5 км

	/** Событие: начало сместилось. UI и системы слушают его для обновления. */
	UPROPERTY(BlueprintAssignable, Category = "Open Universe|Floating Origin")
	FOUOriginShifted OnOriginShifted;

	/** Мировая (double) позиция актора с учётом начала. */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Floating Origin")
	FVector GetWorldPosition(const AActor* Actor) const;

	/** Поставить актор по мировой (double) позиции, переведя её в локальную. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Floating Origin")
	void SetWorldPosition(AActor* Actor, const FVector& WorldPosition);

	/**
	 * Проверить и при необходимости перенести начало.
	 * Вызывать раз в кадр (или по таймеру) для «якорного» актора — обычно
	 * это корабль игрока.
	 */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Floating Origin")
	void UpdateOrigin(const AActor* Focus);

	/** Принудительно перенести начало в заданную мировую точку. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Floating Origin")
	void RebaseTo(const FVector& WorldPosition);

private:
	/** Сдвинуть все акторы и начало на дельту. */
	void ApplyShift(const FVector& Delta);
};
