// Copyright Open Universe. Все права защищены.

#include "OUProjectile.h"
#include "Components/StaticMeshComponent.h"
#include "Engine/StaticMesh.h"
#include "Materials/MaterialInterface.h"
#include "UObject/ConstructorHelpers.h"

AOUProjectile::AOUProjectile()
{
	PrimaryActorTick.bCanEverTick = true;

	BoltMesh = CreateDefaultSubobject<UStaticMeshComponent>(TEXT("BoltMesh"));
	RootComponent = BoltMesh;
	BoltMesh->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	BoltMesh->SetRelativeScale3D(FVector(4.0f, 0.28f, 0.28f));   // вытянутый трассер

	static ConstructorHelpers::FObjectFinder<UStaticMesh> Sph(TEXT("/Engine/BasicShapes/Sphere.Sphere"));
	if (Sph.Succeeded())
	{
		BoltMesh->SetStaticMesh(Sph.Object);
	}
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> Mat(
		TEXT("/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"));
	if (Mat.Succeeded())
	{
		BoltMesh->SetMaterial(0, Mat.Object);
	}
}

void AOUProjectile::BeginPlay()
{
	Super::BeginPlay();
	// Снаряд — яркий трассер (жёлто-белый), свечение задаём и базовым цветом,
	// и эмиссией (если материал её поддерживает).
	if (BoltMesh)
	{
		BoltMesh->SetVectorParameterValueOnMaterials(TEXT("Color"), FVector(3.f, 2.4f, 0.6f));
		BoltMesh->SetVectorParameterValueOnMaterials(TEXT("Emissive"), FVector(3.f, 2.4f, 0.6f));
	}
}

void AOUProjectile::Launch(const FVector& Direction)
{
	Dir = Direction.GetSafeNormal();
	if (Dir.IsNearlyZero())
	{
		Dir = GetActorForwardVector();
	}
	bLaunched = true;
	// Ориентируем «болт» по направлению полёта (для удлинённой формы в будущем).
	if (!Dir.IsNearlyZero())
	{
		SetActorRotation(Dir.Rotation());
	}
}

void AOUProjectile::Tick(float DeltaSeconds)
{
	Super::Tick(DeltaSeconds);
	Age += DeltaSeconds;
	if (Age >= LifeSeconds)
	{
		Destroy();
		return;
	}
	// Летим сами (без физики UE): позиция += направление * скорость * dt.
	AddActorWorldOffset(Dir * Speed * DeltaSeconds, /*bSweep=*/false);
}
