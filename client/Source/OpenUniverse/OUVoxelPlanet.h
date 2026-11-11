// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "OpenUniverseTypes.h"
#include "OUVoxelPlanet.generated.h"

class UProceduralMeshComponent;

/** Уровень детализации (совпадает с серверным LOD). */
UENUM(BlueprintType)
enum class EOUVoxelLod : uint8
{
	Far UMETA(DisplayName = "Далеко (4×4)"),
	Mid UMETA(DisplayName = "Средне (8×8)"),
	Near UMETA(DisplayName = "Близко (16×16)"),
	Close UMETA(DisplayName = "Вплотную (24×24)")
};

/**
 * Воксельная планета.
 *
 * Повторяет серверную геометрию (`crates/render/src/voxel.rs`):
 * кубическая сетка искривляется в сферу правильной формулой (cube-to-sphere),
 * высота берётся из шума. Это ровно тот же рельеф, что считает физика, —
 * поэтому посадка совпадает с картинкой.
 *
 * Меш строится процедурно (`UProceduralMeshComponent`) — по одной грани на
 * воксель. Для больших планет это заготовка: в реальном проекте нужен LOD и
 * стриминг чанков, что отражено в плане.
 */
UCLASS()
class OPENUNIVERSE_API AOUVoxelPlanet : public AActor
{
	GENERATED_BODY()

public:
	AOUVoxelPlanet();

	/** Радиус планеты (уровень океана), см UE. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	float Radius = 100000.f; // 1 км

	/** Размах рельефа, см. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	float Amplitude = 3000.f;

	/** Семя генерации (совпадает с сервером). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	int64 Seed = 0xC0FFEE;

	/** Уровень детализации. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	EOUVoxelLod Lod = EOUVoxelLod::Mid;

	/** Построить меш планеты. */
	UFUNCTION(BlueprintCallable, CallInEditor, Category = "Open Universe|Planet")
	void BuildPlanet();

	/** Число сегментов на грань для выбранного LOD. */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	int32 Segments() const;

	/** Высота рельефа в направлении (для посадки и проверок). */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	float HeightAt(const FVector& Direction) const;

	/** Радиус поверхности в направлении. */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	float SurfaceRadiusAt(const FVector& Direction) const;

protected:
	virtual void BeginPlay() override;

	/** Строим меш и в редакторе, чтобы планета была видна сразу на карте. */
	virtual void OnConstruction(const FTransform& Transform) override;

	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Planet")
	UProceduralMeshComponent* Mesh = nullptr;

private:
	/** Детерминированный шум рельефа (совпадает с сервером). */
	float Noise(float U, float V) const;
};
