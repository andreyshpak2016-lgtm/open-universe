// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "OUSurfaceBase.generated.h"

class UStaticMeshComponent;
class UStaticMesh;
class UMaterialInterface;

/**
 * База на поверхности планеты.
 *
 * Ставится в точке появления игрока (на суше домашней планеты). Это видимый
 * ориентир: посадочная площадка, командно-диспетчерский пункт и несколько
 * контейнеров. Собрана из простых блочных мешей — та же «воксельная» эстетика,
 * что и у кораблей.
 *
 * Актор строит элементы при старте и в редакторе (`OnConstruction`), поэтому
 * база видна сразу после открытия карты.
 */
UCLASS()
class OPENUNIVERSE_API AOUSurfaceBase : public AActor
{
	GENERATED_BODY()

public:
	AOUSurfaceBase();

	/** Построить базу (площадка, вышка, контейнеры). */
	UFUNCTION(BlueprintCallable, CallInEditor, Category = "Open Universe|Base")
	void BuildBase();

	/** Радиус посадочной площадки, см. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Base")
	float PadRadius = 1200.f;

protected:
	virtual void OnConstruction(const FTransform& Transform) override;
	virtual void BeginPlay() override;

	/** Корень базы. */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Base")
	USceneComponent* Root = nullptr;

private:
	/** Добавить блок (позиция и размер — в локальных осях базы, см). */
	void AddBlock(const FVector& Offset, const FVector& Size, const FLinearColor& Color, const FString& Name);

	/** Загруженные меш и материал по умолчанию. */
	UPROPERTY(Transient)
	TObjectPtr<UStaticMesh> CubeMesh = nullptr;
	UPROPERTY(Transient)
	TObjectPtr<UMaterialInterface> BaseMaterial = nullptr;
};
