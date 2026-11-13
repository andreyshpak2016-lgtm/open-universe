// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "OUSurfaceBase.generated.h"

class UStaticMeshComponent;
class UStaticMesh;
class UMaterialInterface;

/**
 * Стартовый комплекс на поверхности планеты.
 *
 * Ставится в **самом ровном месте** домашней планеты (см.
 * `ou_sim::home_spawn_direction`: ищется минимальный перепад высот в радиусе
 * базы). Это крупный видимый ориентир и «дом» игрока: посадочное поле с
 * разметкой, командно-диспетчерская вышка, ангары, топливные склады, терминалы
 * и маяк.
 *
 * Собран из простых блочных мешей — та же «воксельная» эстетика, что у кораблей
 * и редактора. Пол базы садится на рельеф по своей сетке (высоты берём из общей
 * формулы планеты), поэтому площадка не «висит» на склоне и не прорезает холм.
 *
 * Актор строит элементы при старте и в редакторе (`OnConstruction`), поэтому
 * комплекс виден сразу после открытия карты.
 */
UCLASS()
class OPENUNIVERSE_API AOUSurfaceBase : public AActor
{
	GENERATED_BODY()

public:
	AOUSurfaceBase();

	/** Построить комплекс. */
	UFUNCTION(BlueprintCallable, CallInEditor, Category = "Open Universe|Base")
	void BuildBase();

	/**
	 * Полуразмер посадочного поля, см (по умолчанию ~90 м).
	 *
	 * Всё остальное здание строится относительно него, поэтому увеличение
	 * поля увеличивает и весь комплекс. Должен совпадать с
	 * `AOUVoxelPlanet::PadFlatRadius` — иначе ровный пол планеты и здания
	 * разъедутся по размеру.
	 */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Base")
	float PadRadius = 9000.f;

	/** Высота меша-плиты поля, см (тонкая плита, чтобы не «парить» над землёй). */
	static constexpr float PadThicknessCm = 120.f;

protected:
	virtual void OnConstruction(const FTransform& Transform) override;
	virtual void BeginPlay() override;

	/** Корень базы. */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Base")
	USceneComponent* Root = nullptr;

private:
	/** Добавить блок (позиция и размер — в локальных осях базы, см). */
	void AddBlock(const FVector& Offset, const FVector& Size, const FLinearColor& Color, const FString& Name, bool bSolid = true);

	/**
	 * Высота пола базы над «нулём» актора в локальной точке (X,Y), см.
	 *
	 * Актор ставится в точке появления; мира «вниз» — от центра планеты. Функция
	 * берёт высоту рельефа в соответствующей точке поверхности и возвращает
	 * смещение пола. Нужна, чтобы здания стояли на земле, а не парили.
	 */
	float FloorHeightAt(float LocalX, float LocalY) const;

	/** Загруженные меш и материал по умолчанию. */
	UPROPERTY(Transient)
	TObjectPtr<UStaticMesh> CubeMesh = nullptr;
	UPROPERTY(Transient)
	TObjectPtr<UMaterialInterface> BaseMaterial = nullptr;
};
