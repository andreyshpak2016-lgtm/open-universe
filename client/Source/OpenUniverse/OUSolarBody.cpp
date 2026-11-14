// Copyright Open Universe. Все права защищены.

#include "OUSolarBody.h"
#include "OUTerrain.h"
#include "OUInterop.h"
#include "ProceduralMeshComponent.h"
#include "Components/StaticMeshComponent.h"
#include "Engine/StaticMesh.h"
#include "Materials/MaterialInterface.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "UObject/ConstructorHelpers.h"

DEFINE_LOG_CATEGORY_STATIC(LogOUSolar, Log, All);

/** Уровень океана — тот же, что у сервера (`DEFAULT_SEA_LEVEL`). */
static constexpr double OU_NEIGHBOUR_SEA_LEVEL = 0.545;

AOUSolarBody::AOUSolarBody()
{
	PrimaryActorTick.bCanEverTick = false;

	// Меш рельефа тела (процедурный): строится по общей с сервером формуле.
	TerrainMesh = CreateDefaultSubobject<UProceduralMeshComponent>(TEXT("TerrainMesh"));
	RootComponent = TerrainMesh;
	TerrainMesh->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	TerrainMesh->bUseAsyncCooking = false;

	// Запасная сфера (для гладких тел: звёзд и т.п.).
	Mesh = CreateDefaultSubobject<UStaticMeshComponent>(TEXT("BodyMesh"));
	Mesh->SetupAttachment(RootComponent);
	Mesh->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	Mesh->SetVisibility(false);

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
	// Материал рельефа — вертексный цвет (как у домашней планеты).
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> VCol(
		TEXT("/Engine/EngineDebugMaterials/VertexColorMaterial.VertexColorMaterial"));
	if (VCol.Succeeded())
	{
		TerrainMaterial = VCol.Object;
	}
	else if (Mat.Succeeded())
	{
		TerrainMaterial = Mat.Object;
	}
}

void AOUSolarBody::OnConstruction(const FTransform& Transform)
{
	Super::OnConstruction(Transform);
	BuildBody();
}

/** Цвет поверхности по высоте над уровнем океана (упрощённая палитра). */
static FLinearColor OsmSurfaceColor(double Height, double Amplitude, const FLinearColor& Base)
{
	// Нормируем высоту в −1..1 (0 — уровень океана).
	const double A = (Amplitude > 1e-9) ? Amplitude : 1.0;
	const double H = Height / A;   // ~[-1..1]
	// Ниже уровня океана — тёмно-синяя «вода/впадина», выше — светлее базового.
	if (H < -0.05)
	{
		return FLinearColor(0.10f, 0.18f, 0.34f);
	}
	// Плавно от берега (базовый цвет) к вершинам (светлее).
	const float T = static_cast<float>(FMath::Clamp((H + 0.05) / 1.05, 0.0, 1.0));
	FLinearColor C = Base * (0.75f + 0.5f * T);
	C.A = 1.f;
	return C;
}

void AOUSolarBody::BuildTerrain()
{
	if (!TerrainMesh)
	{
		return;
	}
	TerrainMesh->ClearAllMeshSections();
	if (Amplitude <= 0.0)
	{
		// Гладкое тело (нет рельефа) — оставляем сфере.
		return;
	}

	// Сегментов на грань куба: мельче для крупных тел, но ограничиваем
	// разумно (сетка строится на клиенте один раз).
	int32 Seg = 24;
	if (BodyRadiusCm > 5000000.f)
	{
		Seg = 32;
	}

	// Амплитуда и радиус — в серверных единицах для общей формулы.
	const double AmpServer = Amplitude;
	const double RadiusServer = BodyRadiusCm / OUInterop::ServerUnitToCm;
	const uint64 Seed = static_cast<uint64>(TerrainSeed);

	const int32 Faces[6][3] = {
		{0, 1,  1}, {0, 1, -1},
		{1, 0,  1}, {1, 0, -1},
		{2, 0,  1}, {2, 0, -1}
	};

	int32 SectionIndex = 0;
	for (int32 F = 0; F < 6; ++F)
	{
		const int32 AI = Faces[F][0];
		const int32 BI = Faces[F][1];
		const float Sign = static_cast<float>(Faces[F][2]);

		TArray<FVector> Verts;
		TArray<int32> Tris;
		TArray<FVector> Normals;
		TArray<FVector2D> UVs;
		TArray<FLinearColor> Colors;
		TArray<struct FProcMeshTangent> Tangents;

		for (int32 J = 0; J <= Seg; ++J)
		{
			for (int32 I = 0; I <= Seg; ++I)
			{
				const float U = static_cast<float>(I) / Seg * 2.f - 1.f;
				const float Vv = static_cast<float>(J) / Seg * 2.f - 1.f;
				FVector Cube = FVector::ZeroVector;
				Cube[AI] = Sign;
				Cube[BI] = U;
				const int32 Third = 3 - AI - BI;
				Cube[Third] = Vv;

				FVector Dir = Cube.GetSafeNormal();

				// Общая с сервером формула высоты (серверные единицы), зеркало
				// `OULT::HeightDir` — та же функция, что у домашней планеты.
				const double H = OULT::HeightDir(
					Dir.X, Dir.Y, Dir.Z, AmpServer, OU_NEIGHBOUR_SEA_LEVEL, Seed, 0);
				// Высота в см UE (амплитуда в серверных ед. → см).
				const double HeightCm = H * OUInterop::ServerUnitToCm;
				Verts.Add(Dir * (BodyRadiusCm + static_cast<float>(HeightCm)));

				Normals.Add(Dir);
				UVs.Add(FVector2D(U * 0.5f + 0.5f, Vv * 0.5f + 0.5f));
				Colors.Add(OsmSurfaceColor(H, AmpServer, BodyColor));
				Tangents.Add(FProcMeshTangent(1.f, 0.f, 0.f));
			}
		}

		for (int32 J = 0; J < Seg; ++J)
		{
			for (int32 I = 0; I < Seg; ++I)
			{
				const int32 A = J * (Seg + 1) + I;
				const int32 B = A + 1;
				const int32 C = A + (Seg + 1);
				const int32 D = C + 1;
				Tris.Add(A); Tris.Add(C); Tris.Add(B);
				Tris.Add(B); Tris.Add(C); Tris.Add(D);
			}
		}
		// Обратные копии (двойная намотка) — тело видно с любой стороны.
		TArray<int32> BackTris;
		BackTris.Reserve(Tris.Num());
		for (int32 T = 0; T + 2 < Tris.Num(); T += 3)
		{
			BackTris.Add(Tris[T + 0]);
			BackTris.Add(Tris[T + 2]);
			BackTris.Add(Tris[T + 1]);
		}
		Tris.Append(BackTris);

		TerrainMesh->CreateMeshSection_LinearColor(
			SectionIndex, Verts, Tris, Normals, UVs, Colors, Tangents, false);
		if (TerrainMaterial)
		{
			TerrainMesh->SetMaterial(SectionIndex, TerrainMaterial);
		}
		++SectionIndex;
	}
	TerrainMesh->SetVisibility(true);
	Mesh->SetVisibility(false);
	UE_LOG(LogOUSolar, Log, TEXT("Рельеф тела: сегментов %d, секций %d"), Seg, SectionIndex);
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

	// Есть рельеф — строим процедурную поверхность и прячем сферу.
	if (Amplitude > 0.0 && Kind != EOUSolarKind::Star)
	{
		BuildTerrain();
	}
	else
	{
		if (TerrainMesh)
		{
			TerrainMesh->ClearAllMeshSections();
			TerrainMesh->SetVisibility(false);
		}
		Mesh->SetVisibility(true);
	}

	UE_LOG(LogOUSolar, Log, TEXT("Небесное тело: %s, радиус %.0f см, амплитуда %.0f"),
		Kind == EOUSolarKind::Star ? TEXT("звезда") : TEXT("планета"), BodyRadiusCm, Amplitude);
}
