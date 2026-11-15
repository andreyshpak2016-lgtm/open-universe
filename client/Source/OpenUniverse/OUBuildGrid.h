// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "OUBuildGrid.generated.h"

class UInstancedStaticMeshComponent;

/**
 * Призрачная сетка постройки: каркас куба (12 тонких рёбер) размера ядра.
 *
 * Показывает игроку будущий объём конструкции, когда он выбрал ядро (малый/
 * средний/большой). Полупрозрачный «еле видный» каркас: не мешает обзору, но
 * задаёт границы, в которых можно строить. Дополнительно рисует редкие
 * внутренние линии-подсетку, чтобы размер читался.
 */
UCLASS()
class OPENUNIVERSE_API AOUBuildGrid : public AActor
{
	GENERATED_BODY()

public:
	AOUBuildGrid();

	/** Задать куб сетки: центр (мир, см), сторона (см), ориентация по осям сетки. */
	void Setup(const FVector& CenterWorld, double SideCm, const FRotator& Rotation);

	/** Спрятать/показать сетку. */
	void SetGridVisible(bool bVisible);

protected:
	virtual void BeginPlay() override;

	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Build")
	UInstancedStaticMeshComponent* Lines = nullptr;
};
