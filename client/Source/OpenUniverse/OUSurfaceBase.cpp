// Copyright Open Universe. Все права защищены.

#include "OUSurfaceBase.h"
#include "OUInterop.h"
#include "OUTerrain.h"
#include "OUVoxelPlanet.h"
#include "Components/StaticMeshComponent.h"
#include "Engine/StaticMesh.h"
#include "Materials/MaterialInterface.h"
#include "UObject/ConstructorHelpers.h"
#include "Engine/World.h"

DEFINE_LOG_CATEGORY_STATIC(LogOUBase, Log, All);

namespace
{
	/** Радиус домашней планеты в серверных единицах (как на сервере). */
	constexpr double OUBigRadiusServer = 500000.0;
}

AOUSurfaceBase::AOUSurfaceBase()
{
	PrimaryActorTick.bCanEverTick = false;

	Root = CreateDefaultSubobject<USceneComponent>(TEXT("Root"));
	SetRootComponent(Root);

	static ConstructorHelpers::FObjectFinder<UStaticMesh> Cube(
		TEXT("/Engine/BasicShapes/Cube.Cube"));
	if (Cube.Succeeded())
	{
		CubeMesh = Cube.Object;
	}
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> Mat(
		TEXT("/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"));
	if (Mat.Succeeded())
	{
		BaseMaterial = Mat.Object;
	}
	// Собственный материал построек (панельные линии) — если сгенерирован
	// (tools/make-materials.ps1). Иначе остаётся базовый материал движка.
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> Panel(
		TEXT("/Game/Materials/M_BasePanel.M_BasePanel"));
	if (Panel.Succeeded())
	{
		BaseMaterial = Panel.Object;
	}
}

void AOUSurfaceBase::BeginPlay()
{
	Super::BeginPlay();
	BuildBase();
}

void AOUSurfaceBase::OnConstruction(const FTransform& Transform)
{
	Super::OnConstruction(Transform);
	BuildBase();
}

float AOUSurfaceBase::FloorHeightAt(float LocalX, float LocalY) const
{
	// Смещение земли (см) относительно центра комплекса в его плоскости (X,Y).
	// Берём ПОЛНУЮ поверхность планеты: обычный рельеф плюс выравнивание
	// площадки (`PadFlattenCm`) — ту самую, по которой ходит персонаж. Поэтому
	// постройки встают на землю (в том числе на пандус у края) без «парения».
	const FVector Up = GetActorUpVector();
	const FVector Fwd = GetActorForwardVector();
	const FVector Right = GetActorRightVector();
	const FVector World = GetActorLocation() + Fwd * LocalX + Right * LocalY;
	const FVector Dir = World.GetSafeNormal();
	if (Dir.IsNearlyZero())
	{
		return 0.f;
	}
	// Абсолютная земля (см над центром планеты) минус радиус в этой точке.
	const double Here = OUBigRadiusServer * OUInterop::ServerUnitToCm
		+ AOUVoxelPlanet::AnalyticHeightCm(Dir)
		+ AOUVoxelPlanet::PadFlattenCm(Dir);
	return static_cast<float>(Here - static_cast<double>(GetActorLocation().Size()));
}

void AOUSurfaceBase::AddBlock(const FVector& Offset, const FVector& Size, const FLinearColor& Color, const FString& Name, bool bSolid)
{
	if (!CubeMesh)
	{
		return;
	}
	UStaticMeshComponent* Comp = NewObject<UStaticMeshComponent>(this, *Name);
	Comp->SetupAttachment(Root);
	Comp->RegisterComponent();
	Comp->SetStaticMesh(CubeMesh);
	Comp->SetRelativeLocation(Offset);
	// Базовый куб движка — 100 см; масштаб задаём относительно него.
	Comp->SetRelativeScale3D(Size / 100.f);
	Comp->SetMobility(EComponentMobility::Movable);
	Comp->SetGenerateOverlapEvents(false);
	if (bSolid)
	{
		// Твёрдый блок (здания, фундаменты): персонаж не проходит сквозь.
		Comp->SetCollisionEnabled(ECollisionEnabled::QueryAndPhysics);
		Comp->SetCollisionObjectType(ECC_WorldStatic);
		Comp->SetCollisionResponseToAllChannels(ECR_Block);
	}
	else
	{
		// Плиты поля/разметка — только визуал (не мешают ходить по земле).
		Comp->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	}
	if (BaseMaterial)
	{
		Comp->SetMaterial(0, BaseMaterial);
	}
	Comp->SetVectorParameterValueOnMaterials(TEXT("Color"), FVector(Color.R, Color.G, Color.B));
	AddInstanceComponent(Comp);
}

void AOUSurfaceBase::BuildBase()
{
	if (!Root)
	{
		return;
	}
	// Очищаем прежние блоки (перестройка в редакторе).
	TArray<UActorComponent*> Existing;
	GetComponents(Existing);
	for (UActorComponent* C : Existing)
	{
		UStaticMeshComponent* SMC = Cast<UStaticMeshComponent>(C);
		if (SMC && SMC != Root)
		{
			SMC->DestroyComponent();
		}
	}

	// Комплекс больше НЕ строится: на площадке спавна оставлена только
	// выровненная поверхность (ровный пол + воксельные террасы по краю).
	// Это делает сама планета (`AOUVoxelPlanet::PadFlattenCm`), поэтому земля
	// ровная и проходимая, а игрок ставит своё ядро базы сам.
	UE_LOG(LogOUBase, Log, TEXT("Комплекс построен: поле %.0f м, элементов: 0 (только площадка)"),
		PadRadius * 2.f / 100.f);
}
