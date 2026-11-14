// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "OUSolarBody.generated.h"

class UStaticMeshComponent;
class UProceduralMeshComponent;
class UStaticMesh;
class UMaterialInterface;

/** Тип небесного тела для декоративных объектов системы. */
UENUM(BlueprintType)
enum class EOUSolarKind : uint8
{
	Star UMETA(DisplayName = "Звезда"),
	Planet UMETA(DisplayName = "Планета")
};

/**
 * Простое небесное тело солнечной системы: солнце или дальняя планета.
 *
 * Это лёгкий шар из сферы-меша (без воксельного рельефа) — нужен для
 * «картинки системы» в небе: игрок стоит на домашней планете, а вокруг видны
 * солнце и другие планеты. На эти тела не садятся, поэтому рельеф не нужен.
 */
UCLASS()
class OPENUNIVERSE_API AOUSolarBody : public AActor
{
	GENERATED_BODY()

public:
	AOUSolarBody();

	/** Звезда или планета. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|System")
	EOUSolarKind Kind = EOUSolarKind::Planet;

	/** Радиус тела в сантиметрах UE. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|System")
	float BodyRadiusCm = 60000.f;

	/** Амплитуда рельефа, СЕРВЕРНЫЕ единицы (0 — гладкая сфера). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|System")
	double Amplitude = 0.0;

	/** Сид рельефа (общая формула высоты с сервером). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|System")
	int64 TerrainSeed = 0;

	/** Цвет тела. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|System")
	FLinearColor BodyColor = FLinearColor(0.6f, 0.6f, 0.7f, 1.f);

	/** Построить тело (задать масштаб меша и цвет). */
	UFUNCTION(BlueprintCallable, CallInEditor, Category = "Open Universe|System")
	void BuildBody();

protected:
	virtual void OnConstruction(const FTransform& Transform) override;

	/** Процедурный меш рельефа (общая с сервером формула высоты). */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|System")
	UProceduralMeshComponent* TerrainMesh = nullptr;

	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|System")
	UStaticMeshComponent* Mesh = nullptr;

private:
	/** Построить процедурную поверхность тела по рельефу. */
	void BuildTerrain();

	UPROPERTY(Transient)
	TObjectPtr<UStaticMesh> SphereMesh = nullptr;
	UPROPERTY(Transient)
	TObjectPtr<UMaterialInterface> BodyMaterial = nullptr;
	UPROPERTY(Transient)
	TObjectPtr<UMaterialInterface> TerrainMaterial = nullptr;
};
