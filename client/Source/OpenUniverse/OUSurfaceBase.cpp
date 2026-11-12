// Copyright Open Universe. Все права защищены.

#include "OUSurfaceBase.h"
#include "Components/StaticMeshComponent.h"
#include "Engine/StaticMesh.h"
#include "Materials/MaterialInterface.h"
#include "UObject/ConstructorHelpers.h"
#include "Engine/World.h"

DEFINE_LOG_CATEGORY_STATIC(LogOUBase, Log, All);

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

void AOUSurfaceBase::AddBlock(const FVector& Offset, const FVector& Size, const FLinearColor& Color, const FString& Name)
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
	Comp->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	if (BaseMaterial)
	{
		Comp->SetMaterial(0, BaseMaterial);
	}
	// Цвет через параметр материала (если он поддерживается) + вершинный оттенок.
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

	// --- Посадочная площадка: плоская плита под кораблём ---
	AddBlock(FVector(0, 0, 40), FVector(PadRadius * 2.f, PadRadius * 2.f, 80.f),
		FLinearColor(0.16f, 0.18f, 0.22f), TEXT("Pad"));

	// --- Разметка площадки: 4 угловых маркера (тёплые), цвет «техно» ---
	const float H = PadRadius * 0.9f;
	const FLinearColor Mark(1.0f, 0.55f, 0.1f);
	AddBlock(FVector( H,  H, 90), FVector(180, 180, 40), Mark, TEXT("Mark0"));
	AddBlock(FVector(-H,  H, 90), FVector(180, 180, 40), Mark, TEXT("Mark1"));
	AddBlock(FVector( H, -H, 90), FVector(180, 180, 40), Mark, TEXT("Mark2"));
	AddBlock(FVector(-H, -H, 90), FVector(180, 180, 40), Mark, TEXT("Mark3"));

	// --- Командно-диспетчерский пункт: основание + вышка + «стекло» ---
	AddBlock(FVector(0, PadRadius + 900, 300), FVector(1400, 1000, 600),
		FLinearColor(0.30f, 0.33f, 0.38f), TEXT("Tower0"));
	AddBlock(FVector(0, PadRadius + 900, 900), FVector(900, 700, 600),
		FLinearColor(0.30f, 0.33f, 0.38f), TEXT("Tower1"));
	AddBlock(FVector(0, PadRadius + 600, 1000), FVector(1000, 120, 400),
		FLinearColor(0.10f, 0.55f, 0.95f), TEXT("Glass"));

	// --- Топливные контейнеры вдоль края ---
	for (int32 i = 0; i < 3; ++i)
	{
		const float X = -PadRadius + 300.f + i * 500.f;
		AddBlock(FVector(X, -PadRadius - 700, 260), FVector(360, 360, 520),
			FLinearColor(0.85f, 0.70f, 0.20f), FString::Printf(TEXT("Fuel%d"), i));
	}

	// --- Маяк: тонкая колонна с яркой «лампой» ---
	AddBlock(FVector(PadRadius + 500, PadRadius + 500, 600), FVector(120, 120, 1200),
		FLinearColor(0.55f, 0.58f, 0.62f), TEXT("Beacon0"));
	AddBlock(FVector(PadRadius + 500, PadRadius + 500, 1260), FVector(220, 220, 220),
		FLinearColor(0.0f, 0.9f, 0.8f), TEXT("BeaconLight"));

	UE_LOG(LogOUBase, Log, TEXT("База построена в точке появления"));
}
