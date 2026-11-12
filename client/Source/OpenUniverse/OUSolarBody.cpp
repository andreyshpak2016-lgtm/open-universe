// Copyright Open Universe. Все права защищены.

#include "OUSolarBody.h"
#include "Components/StaticMeshComponent.h"
#include "Engine/StaticMesh.h"
#include "Materials/MaterialInterface.h"
#include "UObject/ConstructorHelpers.h"

DEFINE_LOG_CATEGORY_STATIC(LogOUSolar, Log, All);

AOUSolarBody::AOUSolarBody()
{
	PrimaryActorTick.bCanEverTick = false;

	Mesh = CreateDefaultSubobject<UStaticMeshComponent>(TEXT("BodyMesh"));
	RootComponent = Mesh;
	Mesh->SetCollisionEnabled(ECollisionEnabled::NoCollision);

	// Сфера движка (радиус 50 см в юнитах — от неё считаем масштаб).
	static ConstructorHelpers::FObjectFinder<UStaticMesh> Sphere(
		TEXT("/Engine/BasicShapes/Sphere.Sphere"));
	if (Sphere.Succeeded())
	{
		SphereMesh = Sphere.Object;
		Mesh->SetStaticMesh(SphereMesh);
	}
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> Mat(
		TEXT("/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"));
	if (Mat.Succeeded())
	{
		BodyMaterial = Mat.Object;
		Mesh->SetMaterial(0, BodyMaterial);
	}
}

void AOUSolarBody::OnConstruction(const FTransform& Transform)
{
	Super::OnConstruction(Transform);
	BuildBody();
}

void AOUSolarBody::BuildBody()
{
	if (!Mesh || !SphereMesh)
	{
		return;
	}
	// Базовая сфера движка имеет радиус 50 см — масштабируем под нужный радиус.
	const float Scale = BodyRadiusCm / 50.f;
	Mesh->SetRelativeScale3D(FVector(Scale));
	if (BodyMaterial)
	{
		Mesh->SetMaterial(0, BodyMaterial);
	}
	Mesh->SetVectorParameterValueOnMaterials(TEXT("Color"),
		FVector(BodyColor.R, BodyColor.G, BodyColor.B));
	// Звезда светится.
	if (Kind == EOUSolarKind::Star && BodyMaterial)
	{
		Mesh->SetVectorParameterValueOnMaterials(TEXT("Emissive"),
			FVector(BodyColor.R, BodyColor.G, BodyColor.B));
	}
	UE_LOG(LogOUSolar, Log, TEXT("Небесное тело: %s, радиус %.0f см"),
		Kind == EOUSolarKind::Star ? TEXT("звезда") : TEXT("планета"), BodyRadiusCm);
}
