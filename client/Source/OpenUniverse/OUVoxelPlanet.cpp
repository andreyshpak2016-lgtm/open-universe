// Copyright Open Universe. Все права защищены.

#include "OUVoxelPlanet.h"
#include "ProceduralMeshComponent.h"
#include "Materials/MaterialInterface.h"
#include "UObject/ConstructorHelpers.h"

DEFINE_LOG_CATEGORY_STATIC(LogOUPlanet, Log, All);

namespace
{
	/** Хеш решётки в [0,1) — тот же, что на сервере. */
	float LatticeHash(int64 X, int64 Y, int64 Seed)
	{
		uint64 H = static_cast<uint64>(Seed);
		const int64 Coords[2] = { X, Y };
		for (int64 C : Coords)
		{
			H ^= static_cast<uint64>(C) * 0x9E3779B97F4A7C15ull;
			H = (H ^ (H >> 30)) * 0xBF58476D1CE4E5B9ull;
			H = (H ^ (H >> 27)) * 0x94D049BB133111EBull;
		}
		return static_cast<float>((H >> 11) * (1.0 / (1ull << 53)));
	}

	float Smooth(float T)
	{
		return T * T * (3.f - 2.f * T);
	}

	/** Кубическая интерполяция value noise. */
	float ValueNoise(float U, float V, int64 Seed)
	{
		const float IXf = FMath::FloorToFloat(U);
		const float IYf = FMath::FloorToFloat(V);
		const float FX = Smooth(U - IXf);
		const float FY = Smooth(V - IYf);
		const int64 IX = static_cast<int64>(IXf);
		const int64 IY = static_cast<int64>(IYf);
		const float C00 = LatticeHash(IX, IY, Seed);
		const float C10 = LatticeHash(IX + 1, IY, Seed);
		const float C01 = LatticeHash(IX, IY + 1, Seed);
		const float C11 = LatticeHash(IX + 1, IY + 1, Seed);
		const float X0 = FMath::Lerp(C00, C10, FX);
		const float X1 = FMath::Lerp(C01, C11, FX);
		return FMath::Lerp(X0, X1, FY);
	}

	/**
	 * Cube-to-sphere: равномерное отображение граней куба на сферу.
	 * Формула совпадает с `crates/render/src/voxel.rs::cube_to_sphere`.
	 */
	FVector CubeToSphere(const FVector& P)
	{
		const double X = P.X, Y = P.Y, Z = P.Z;
		const double X2 = X * X, Y2 = Y * Y, Z2 = Z * Z;
		return FVector(
			X * FMath::Sqrt(1.0 - Y2 * 0.5 - Z2 * 0.5 + Y2 * Z2 / 3.0),
			Y * FMath::Sqrt(1.0 - Z2 * 0.5 - X2 * 0.5 + Z2 * X2 / 3.0),
			Z * FMath::Sqrt(1.0 - X2 * 0.5 - Y2 * 0.5 + X2 * Y2 / 3.0));
	}
}

AOUVoxelPlanet::AOUVoxelPlanet()
{
	PrimaryActorTick.bCanEverTick = false;
	Mesh = CreateDefaultSubobject<UProceduralMeshComponent>(TEXT("VoxelMesh"));
	RootComponent = Mesh;
	Mesh->bUseAsyncCooking = true;

	// Материал обязателен: без него процедурный меш в UE не виден.
	// Берём материал, который рисует **цвет вершин** — иначе планета была бы
	// однотонно-белой и весь рельеф (вода/песок/трава/снег) потерялся бы.
	// Копию запоминаем — назначим его на КАЖДУЮ секцию (иначе видна лишь одна
	// грань: секция берёт материал из слота со своим номером).
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> VertexColorMat(
		TEXT("/Engine/EngineDebugMaterials/VertexColorMaterial.VertexColorMaterial"));
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> FallbackMat(
		TEXT("/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"));
	if (VertexColorMat.Succeeded())
	{
		PlanetMaterial = VertexColorMat.Object;
	}
	else if (FallbackMat.Succeeded())
	{
		PlanetMaterial = FallbackMat.Object;
	}
}

void AOUVoxelPlanet::BeginPlay()
{
	Super::BeginPlay();
	BuildPlanet();
}

void AOUVoxelPlanet::OnConstruction(const FTransform& Transform)
{
	Super::OnConstruction(Transform);
	// В редакторе BeginPlay не вызывается — строим меш здесь, чтобы планета
	// была видна на карте сразу.
	BuildPlanet();
}

int32 AOUVoxelPlanet::Segments() const
{
	switch (Lod)
	{
	case EOUVoxelLod::Far:   return 4;
	case EOUVoxelLod::Mid:   return 8;
	case EOUVoxelLod::Near:  return 16;
	case EOUVoxelLod::Close: return 24;
	default:                 return 8;
	}
}

float AOUVoxelPlanet::Noise(float U, float V) const
{
	// Две октавы, как на сервере: материки + детали.
	const float Coarse = ValueNoise(U * 2.6f, V * 2.6f, Seed);
	const float Fine = ValueNoise(U * 9.f, V * 9.f, Seed ^ 0xF1E);
	return Coarse * 0.72f + Fine * 0.28f;
}

float AOUVoxelPlanet::HeightAt(const FVector& Direction) const
{
	const FVector D = Direction.GetSafeNormal();
	// Octahedral mapping (сфера → квадрат), как в серверном terrain.rs.
	const float L = FMath::Abs(D.X) + FMath::Abs(D.Y) + FMath::Abs(D.Z);
	if (L < 1e-6f)
	{
		return 0.f;
	}
	float U = D.X / L;
	float V = D.Z / L;
	if (D.Y < 0.f)
	{
		const float UX = (1.f - FMath::Abs(V)) * (U >= 0.f ? 1.f : -1.f);
		const float VY = (1.f - FMath::Abs(U)) * (V >= 0.f ? 1.f : -1.f);
		U = UX;
		V = VY;
	}
	const float SeaLevel = 0.30f;
	return (Noise(U, V) - SeaLevel) * Amplitude * 2.f;
}

float AOUVoxelPlanet::SurfaceRadiusAt(const FVector& Direction) const
{
	return Radius + HeightAt(Direction);
}

void AOUVoxelPlanet::BuildPlanet()
{
	if (!Mesh)
	{
		return;
	}
	Mesh->ClearAllMeshSections();

	const int32 Seg = Segments();
	int32 SectionIndex = 0;

	// 6 граней куба: (ось, ось, знак).
	const int32 Faces[6][3] = {
		{0, 1,  1}, {0, 1, -1}, {0, 2,  1},
		{0, 2, -1}, {1, 2,  1}, {1, 2, -1}
	};

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

		// Углы грани (сетка (Seg+1)×(Seg+1)).
		Verts.Reserve((Seg + 1) * (Seg + 1));
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

				const FVector Dir = CubeToSphere(Cube);
				const float Height = HeightAt(Dir);
				Verts.Add(Dir * SurfaceRadiusAt(Dir));

				Normals.Add(Dir);
				UVs.Add(FVector2D(U * 0.5f + 0.5f, Vv * 0.5f + 0.5f));

				// Цвет по высоте: вода (синяя) → песок → трава → снег.
				// Используем FLinearColor: CreateMeshSection_LinearColor ждёт его.
				FLinearColor Color;
				if (Height <= 0.f)
				{
					Color = FLinearColor(0.03f, 0.12f, 0.42f); // океан
				}
				else
				{
					const float T = FMath::Clamp(Height / (Amplitude * 2.f), 0.f, 1.f);
					if (T < 0.35f)
					{
						Color = FLinearColor::LerpUsingHSV(FLinearColor(0.8f, 0.72f, 0.45f), FLinearColor(0.13f, 0.4f, 0.13f), T / 0.35f);
					}
					else
					{
						Color = FLinearColor::LerpUsingHSV(FLinearColor(0.13f, 0.4f, 0.13f), FLinearColor(0.95f, 0.96f, 0.99f), (T - 0.35f) / 0.65f);
					}
				}
				Colors.Add(Color);
				Tangents.Add(FProcMeshTangent(1.f, 0.f, 0.f));
			}
		}

		// Индексы двух треугольников на каждую ячейку.
		//
		// Намотка зависит от знака грани, поэтому вместо угадывания «лицевой»
		// стороны дублируем каждый треугольник обратным обходом. Отсечение
		// задних граней оставляет ровно один экземпляр из пары — планета
		// гарантированно видна с любой стороны, мерцания (z-fighting) нет.
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
		// Обратные копии считаем в отдельном массиве: нельзя читать Tris,
		// одновременно добавляя в него (массив перевыделяется — падение).
		TArray<int32> BackTris;
		BackTris.Reserve(Tris.Num());
		for (int32 T = 0; T + 2 < Tris.Num(); T += 3)
		{
			BackTris.Add(Tris[T + 0]);
			BackTris.Add(Tris[T + 2]);
			BackTris.Add(Tris[T + 1]);
		}
		Tris.Append(BackTris);

		Mesh->CreateMeshSection_LinearColor(SectionIndex, Verts, Tris, Normals, UVs, Colors, Tangents, false);
		// Материал на каждую секцию: иначе грань без материала не рисуется.
		Mesh->SetMaterial(SectionIndex, PlanetMaterial);
		++SectionIndex;
	}

	UE_LOG(LogOUPlanet, Log, TEXT("Планета построена: LOD сегментов %d, секций %d"), Seg, SectionIndex);
}
