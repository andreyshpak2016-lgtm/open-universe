// Copyright Open Universe. Все права защищены.

#include "OUBuildBlock.h"
#include "Components/StaticMeshComponent.h"
#include "Engine/StaticMesh.h"
#include "Materials/MaterialInterface.h"
#include "UObject/ConstructorHelpers.h"

AOUBuildBlock::AOUBuildBlock()
{
	PrimaryActorTick.bCanEverTick = false;

	BlockMesh = CreateDefaultSubobject<UStaticMeshComponent>(TEXT("BlockMesh"));
	RootComponent = BlockMesh;
	BlockMesh->SetCollisionEnabled(ECollisionEnabled::NoCollision);

	static ConstructorHelpers::FObjectFinder<UStaticMesh> Cube(
		TEXT("/Engine/BasicShapes/Cube.Cube"));
	if (Cube.Succeeded())
	{
		BlockMesh->SetStaticMesh(Cube.Object);
	}
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> Mat(
		TEXT("/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"));
	if (Mat.Succeeded())
	{
		BlockMesh->SetMaterial(0, Mat.Object);
	}
	ApplyLook();
}

void AOUBuildBlock::BeginPlay()
{
	Super::BeginPlay();
	ApplyLook();
}

void AOUBuildBlock::ApplyLook()
{
	if (!BlockMesh)
	{
		return;
	}
	// Базовый куб движка имеет сторону 100 см — масштабируем под BlockSize.
	const float Scale = BlockSize / 100.f;
	BlockMesh->SetRelativeScale3D(FVector(Scale));
	BlockMesh->SetVectorParameterValueOnMaterials(TEXT("Color"),
		FVector(BlockColor.R, BlockColor.G, BlockColor.B));
}
