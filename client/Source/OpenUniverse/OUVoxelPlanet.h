// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "OpenUniverseTypes.h"
#include "OUVoxelPlanet.generated.h"

class UProceduralMeshComponent;
class UStaticMeshComponent;

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
	/** Землеподобная: океан, трава, снег, плотная атмосфера и облака. */
	Terran UMETA(DisplayName = "Землеподобная"),
	/** Пустынная: пески, дюны, каньоны; тонкая пыльная атмосфера, без облаков. */
	Desert UMETA(DisplayName = "Пустынная"),
	/** Ледяная: снег и голубой лёд; атмосфера и облака. */
	Ice UMETA(DisplayName = "Ледяная"),
	/** Вулканическая: тёмная порода и раскалённые трещины; плотная дымка. */
	Volcanic UMETA(DisplayName = "Вулканическая"),
	/** Газовая: без рельефа, полосы (юпитер-подобная); плотная атмосфера. */
	GasGiant UMETA(DisplayName = "Газовый гигант"),
	/** Безвоздушная (луна): голый камень и кратеры, НЕТ атмосферы и облаков. */
	Airless UMETA(DisplayName = "Безвоздушная")
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

	/** Радиус планеты (уровень океана), см UE. 500 км — как на сервере (500 000 ед. × 100). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	float Radius = 50000000.f;

	/** Размах рельефа, см (10 км — крупные горы и долины в масштабе планеты). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	float Amplitude = 1000000.f;

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

	// --- Атмосфера и облака ---

	/** Есть ли у этого типа планеты атмосфера (газовый слой и свечение). */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	static bool TypeHasAtmosphere(EOUPlanetType Type);

	/** Есть ли у этого типа планеты облака. */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	static bool TypeHasClouds(EOUPlanetType Type);

	/** Цвет атмосферы (даёт ореол вида планеты). */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	static FLinearColor AtmosphereTint(EOUPlanetType Type);

	/** Собрать/обновить атмосферу и облака по типу планеты. */
	UFUNCTION(BlueprintCallable, CallInEditor, Category = "Open Universe|Planet")
	void BuildAtmosphere();

	/** Максимум облачных «клубов» на планету (пул создаётся в конструкторе). */
	static constexpr int32 MaxCloudPuffs = 160;

protected:
	virtual void BeginPlay() override;

	/** Строим меш и в редакторе, чтобы планета была видна сразу на карте. */
	virtual void OnConstruction(const FTransform& Transform) override;

	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Planet")
	UProceduralMeshComponent* Mesh = nullptr;

	/** Слой атмосферы: чуть больше планеты, полупрозрачный ореол. */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Planet")
	UStaticMeshComponent* AtmosphereMesh = nullptr;

	/** Облака: пул сфер над поверхностью (показываются по типу планеты). */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Planet")
	TArray<UStaticMeshComponent*> CloudPuffs;

	/** Материал планеты (назначается на все секции меша). */
	UPROPERTY(Transient)
	TObjectPtr<class UMaterialInterface> PlanetMaterial = nullptr;

	/** Цвет поверхности по высоте и типу планеты. */
	FLinearColor SurfaceColor(float Height) const;

	/**
	 * Высота рельефа для ВИЗУАЛЬНОГО меша с учётом характера типа планеты.
	 * Для домашней планеты совпадает с физической высотой (иначе игрок
	 * «висел» бы над землёй); для декоративных — добавляет дюны/кратеры/полосы.
	 */
	float VisualHeight(const FVector& DirUE) const;

private:
	/** Детерминированный шум рельефа (совпадает с сервером). */
	float Noise(float U, float V) const;

	/** Детерминированный шум для облаков (независимый канал). */
	float CloudNoise(const FVector& Dir) const;

	/** Кэш базовых ассетов (сфера/материалы) — грузим лениво. */
	UPROPERTY(Transient)
	TObjectPtr<class UStaticMesh> CloudSphereMesh = nullptr;
	UPROPERTY(Transient)
	TObjectPtr<class UMaterialInterface> CloudMaterial = nullptr;
	UPROPERTY(Transient)
	TObjectPtr<class UMaterialInterface> AtmosphereMaterial = nullptr;
};
