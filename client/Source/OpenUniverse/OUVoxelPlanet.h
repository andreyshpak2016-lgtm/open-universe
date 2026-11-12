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
 * Тип планеты — задаёт палитру поверхности и характер рельефа.
 * Нужен, чтобы планеты в системе были визуально разными.
 */
UENUM(BlueprintType)
enum class EOUPlanetType : uint8
{
	/** Землеподобная: океан, трава, снег (домашняя планета игрока). */
	Terran UMETA(DisplayName = "Землеподобная"),
	/** Пустынная: пески, каньоны, без воды. */
	Desert UMETA(DisplayName = "Пустынная"),
	/** Ледяная: снег и голубой лёд. */
	Ice UMETA(DisplayName = "Ледяная"),
	/** Вулканическая: тёмная порода и раскалённые трещины. */
	Volcanic UMETA(DisplayName = "Вулканическая"),
	/** Газовая: без рельефа, полосы (юпитер-подобная). */
	GasGiant UMETA(DisplayName = "Газовый гигант")
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

	/** Радиус планеты (уровень океана), см UE. 8 км — как на сервере (8000 ед. × 100). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	float Radius = 800000.f;

	/** Размах рельефа, см. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	float Amplitude = 6000.f;

	/** Семя генерации (совпадает с сервером). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	int64 Seed = 0xC0FFEE;

	/** Уровень детализации. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	EOUVoxelLod Lod = EOUVoxelLod::Close;

	/** Тип планеты (палитра и характер рельефа). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	EOUPlanetType PlanetType = EOUPlanetType::Terran;

	/** Можно ли строить/приземляться на этой планете (домашняя — да). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	bool bHomePlanet = false;

	/** Построить меш планеты. */
	UFUNCTION(BlueprintCallable, CallInEditor, Category = "Open Universe|Planet")
	void BuildPlanet();

	/** Число сегментов на грань для выбранного LOD. */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	int32 Segments() const;

	/** Высота рельефа в направлении (для посадки и проверок). */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	float HeightAt(const FVector& Direction) const;

	/** Высота в СЕРВЕРНЫХ осях (совпадает с сервером бит-в-бит). */
	float HeightAtServer(const FVector& DirServer) const;

	/** Радиус поверхности в направлении (UE-оси). */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	float SurfaceRadiusAt(const FVector& Direction) const;

	/** Радиус поверхности в СЕРВЕРНЫХ осях. */
	float SurfaceRadiusAtServer(const FVector& DirServer) const
	{
		return Radius + HeightAtServer(DirServer);
	}

	/** Направление точки появления в UE-осях (для камеры и ориентации). */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	static FVector HomeSpawnDirection();

	/** Направление точки появления в СЕРВЕРНЫХ осях (как сервер). */
	static FVector HomeSpawnDirectionServer();

	/** Точка появления в СЕРВЕРНЫХ единицах (как хранит корабль). */
	static FVector HomeSpawnServer(float Clearance);

	/**
	 * Найти домашнюю планету (ту, где стоит игрок и база).
	 *
	 * В системе несколько планет (`AOUVoxelPlanet`), поэтому обычный
	 * `GetActorOfClass` вернул бы любую. Домашняя помечена `bHomePlanet`.
	 */
	static AOUVoxelPlanet* FindHomePlanet(UWorld* World);

	/** Точка появления над поверхностью (на `Clearance` ед. сервера), UE-оси. */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	static FVector HomeSpawn(float Clearance);

protected:
	virtual void BeginPlay() override;

	/** Строим меш и в редакторе, чтобы планета была видна сразу на карте. */
	virtual void OnConstruction(const FTransform& Transform) override;

	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Planet")
	UProceduralMeshComponent* Mesh = nullptr;

	/** Материал планеты (назначается на все секции меша). */
	UPROPERTY(Transient)
	TObjectPtr<class UMaterialInterface> PlanetMaterial = nullptr;

	/** Цвет поверхности по высоте и типу планеты. */
	FLinearColor SurfaceColor(float Height) const;

private:
	/** Детерминированный шум рельефа (совпадает с сервером). */
	float Noise(float U, float V) const;
};
