// Copyright Open Universe. Все права защищены.

#include "OUBuildGrid.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Engine/StaticMesh.h"
#include "Materials/MaterialInterface.h"
#include "UObject/ConstructorHelpers.h"

AOUBuildGrid::AOUBuildGrid()
{
	PrimaryActorTick.bCanEverTick = false;

	USceneComponent* Root = CreateDefaultSubobject<USceneComponent>(TEXT("Root"));
	SetRootComponent(Root);

	Lines = CreateDefaultSubobject<UInstancedStaticMeshComponent>(TEXT("Lines"));
	Lines->SetupAttachment(Root);
	// Сетка — только визуал: не ловит лучи и никого не блокирует.
	Lines->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	Lines->SetCastShadow(false);
	Lines->SetGenerateOverlapEvents(false);

	// Базовый куб движка (100 см) для тонких рёбер каркаса.
	static ConstructorHelpers::FObjectFinder<UStaticMesh> Cube(
		TEXT("/Engine/BasicShapes/Cube.Cube"));
	if (Cube.Succeeded())
	{
		Lines->SetStaticMesh(Cube.Object);
	}
	// Полупрозрачный материал — сетка «еле видная».
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> GhostMat(
		TEXT("/Engine/EngineDebugMaterials/M_SimpleUnlitTranslucent.M_SimpleUnlitTranslucent"));
	if (GhostMat.Succeeded())
	{
		Lines->SetMaterial(0, GhostMat.Object);
	}
	else
	{
		static ConstructorHelpers::FObjectFinder<UMaterialInterface> Mat(
			TEXT("/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"));
		if (Mat.Succeeded())
		{
			Lines->SetMaterial(0, Mat.Object);
		}
	}
}

void AOUBuildGrid::BeginPlay()
{
	Super::BeginPlay();
}

void AOUBuildGrid::SetGridVisible(bool bVisible)
{
	SetActorHiddenInGame(!bVisible);
	SetActorEnableCollision(false);
}

void AOUBuildGrid::Setup(const FVector& CenterWorld, double SideCm, const FRotator& Rotation)
{
	SetActorLocation(CenterWorld);
	SetActorRotation(Rotation);
	if (!Lines)
	{
		return;
	}
	Lines->ClearInstances();
	Lines->SetVectorParameterValueOnMaterials(TEXT("Color"), FVector(0.35f, 0.75f, 1.0f));

	const float H = static_cast<float>(SideCm) * 0.5f;
	// Толщина ребра — «еле видная» линия ~4 см.
	const float T = FMath::Max(3.0f, static_cast<float>(SideCm) * 0.006f);
	// Длина одного ребра (в единицах локального куба: базовый куб = 1, масштаб 100 см).
	const float Len = static_cast<float>(SideCm);

	// 12 рёбер куба: 4 вдоль X, 4 вдоль Y, 4 вдоль Z.
	// Локальные оси: X-вперёд, Y-вправо, Z-вверх от якоря сетки (совпадает с
	// ориентацией блоков), поэтому рёбра строим симметрично от центра.
	auto AddEdge = [&](const FVector& Pos, const FVector& Scale)
	{
		const FTransform Tr(FRotator::ZeroRotator, Pos, Scale);
		Lines->AddInstance(Tr);
	};
	// Проходим по 8 углам и соединяем соседей: делаем аккуратно 12 рёбер.
	for (int ex = -1; ex <= 1; ex += 2)
	{
		for (int ey = -1; ey <= 1; ey += 2)
		{
			// Рёбра вдоль Z (вертикальные) — 4 шт.
			AddEdge(FVector(ex * H, ey * H, 0.f),
				FVector(T / 100.f, T / 100.f, Len / 100.f));
		}
	}
	for (int ex = -1; ex <= 1; ex += 2)
	{
		for (int ez = -1; ez <= 1; ez += 2)
		{
			// Рёбра вдоль Y — 4 шт.
			AddEdge(FVector(ex * H, 0.f, ez * H),
				FVector(T / 100.f, Len / 100.f, T / 100.f));
		}
	}
	for (int ey = -1; ey <= 1; ey += 2)
	{
		for (int ez = -1; ez <= 1; ez += 2)
		{
			// Рёбра вдоль X — 4 шт.
			AddEdge(FVector(0.f, ey * H, ez * H),
				FVector(Len / 100.f, T / 100.f, T / 100.f));
		}
	}
	SetGridVisible(true);
}
