// Copyright Open Universe. Все права защищены.

#include "OUVoxelPlanet.h"
#include "OUInterop.h"
#include "ProceduralMeshComponent.h"
#include "EngineUtils.h"
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
	case EOUVoxelLod::Near:  return 48;
	case EOUVoxelLod::Close: return 128;   // крупная планета: мелкая ячейка у поверхности
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

float AOUVoxelPlanet::HeightAtServer(const FVector& DirServer) const
{
	// Октаэдральное отображение ровно как в серверном `terrain.rs`
	// (в серверных осях: вверх = Y). Только оно даёт ту же высоту, что сервер.
	const FVector D = DirServer.GetSafeNormal();
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

float AOUVoxelPlanet::HeightAt(const FVector& Direction) const
{
	// Направление приходит в UE-осях (вверх = Z). Серверный рельеф считаем в
	// серверных осях — переводим, иначе высота не совпадёт с сервером.
	return HeightAtServer(OUInterop::UEDirToServer(Direction));
}

float AOUVoxelPlanet::SurfaceRadiusAt(const FVector& Direction) const
{
	return Radius + HeightAt(Direction);
}

FLinearColor AOUVoxelPlanet::SurfaceColor(float Height) const
{
	const float T = FMath::Clamp(Height / (Amplitude * 2.f), 0.f, 1.f);
	const bool bWater = Height <= 0.f;
	switch (PlanetType)
	{
	case EOUPlanetType::Desert:
	{
		// Пески и каньоны: нет воды, от оранжевого к красноватому и светлому.
		const FLinearColor Sand(0.82f, 0.62f, 0.32f);
		const FLinearColor Canyon(0.55f, 0.28f, 0.16f);
		const FLinearColor Peak(0.90f, 0.82f, 0.62f);
		return T < 0.5f
			? FLinearColor::LerpUsingHSV(Canyon, Sand, T / 0.5f)
			: FLinearColor::LerpUsingHSV(Sand, Peak, (T - 0.5f) / 0.5f);
	}
	case EOUPlanetType::Ice:
	{
		// Лёд: голубой лёд в низинах, снег выше.
		if (bWater)
		{
			return FLinearColor(0.15f, 0.45f, 0.75f);
		}
		const FLinearColor Ice(0.55f, 0.78f, 0.92f);
		const FLinearColor Snow(0.97f, 0.99f, 1.0f);
		return FLinearColor::LerpUsingHSV(Ice, Snow, T);
	}
	case EOUPlanetType::Volcanic:
	{
		// Тёмная порода и раскалённые трещины: низины — «лава».
		if (bWater)
		{
			return FLinearColor(0.95f, 0.30f, 0.05f); // лава
		}
		const FLinearColor Basalt(0.12f, 0.10f, 0.11f);
		const FLinearColor Ash(0.35f, 0.30f, 0.30f);
		return FLinearColor::LerpUsingHSV(Basalt, Ash, T);
	}
	case EOUPlanetType::GasGiant:
	{
		// Газовый гигант: светлые полосы, без «земли».
		const FLinearColor Band1(0.86f, 0.74f, 0.55f);
		const FLinearColor Band2(0.62f, 0.48f, 0.34f);
		const float Band = FMath::Frac(Height * 0.08f) < 0.5f ? 0.f : 1.f;
		return Band < 0.5f ? Band1 : Band2;
	}
	case EOUPlanetType::Terran:
	default:
	{
		if (bWater)
		{
			return FLinearColor(0.03f, 0.12f, 0.42f); // океан
		}
		const FLinearColor Beach(0.80f, 0.72f, 0.45f);
		const FLinearColor Grass(0.13f, 0.40f, 0.13f);
		const FLinearColor Snow(0.95f, 0.96f, 0.99f);
		if (T < 0.35f)
		{
			return FLinearColor::LerpUsingHSV(Beach, Grass, T / 0.35f);
		}
		return FLinearColor::LerpUsingHSV(Grass, Snow, (T - 0.35f) / 0.65f);
	}
	}
}

FVector AOUVoxelPlanet::HomeSpawnDirectionServer()
{
	// Совпадает с сервером (`ou_sim::home_spawn_direction`): фиксированный
	// обход сферы в СЕРВЕРНЫХ осях, ищем первое «сухое» направление с заметной,
	// но не пиковой высотой. Результат — в серверных осях.
	AOUVoxelPlanet* Probe = NewObject<AOUVoxelPlanet>();
	Probe->Radius = 100000.f;   // серверные единицы (совпадает с ou_sim::HOME_PLANET_RADIUS)
	Probe->Amplitude = 2000.f;
	Probe->Seed = 0xC0FFEE;

	const float Step = 0.15f;
	for (float Phi = -1.2f; Phi <= 1.2f; Phi += Step)
	{
		for (float Theta = 0.f; Theta < 2.f * PI; Theta += Step)
		{
			// Серверные оси: вверх = Y. Формула ровно как в `home_spawn_direction`.
			const FVector D(
				FMath::Cos(Phi) * FMath::Cos(Theta),
				FMath::Sin(Phi),
				FMath::Cos(Phi) * FMath::Sin(Theta));
			const float H = Probe->HeightAtServer(D);
			if (H > Probe->Amplitude * 0.15f && H < Probe->Amplitude * 0.9f)
			{
				return D.GetSafeNormal();
			}
		}
	}
	return FVector(0.f, 1.f, 0.f); // в серверных осях «вверх» = +Y
}

FVector AOUVoxelPlanet::HomeSpawnDirection()
{
	// Направление точки появления в UE-осях (для камеры и ориентации).
	return OUInterop::ServerDirToUE(HomeSpawnDirectionServer()).GetSafeNormal();
}

FVector AOUVoxelPlanet::HomeSpawnServer(float Clearance)
{
	// Точка появления в СЕРВЕРНЫХ единицах (радиус 100 000, амплитуда 2000).
	// Без перевода в UE — чтобы не путаться с масштабом: корабль хранит
	// позицию именно в серверных единицах.
	const FVector DirS = HomeSpawnDirectionServer();
	AOUVoxelPlanet* Probe = NewObject<AOUVoxelPlanet>();
	Probe->Radius = 100000.f;
	Probe->Amplitude = 2000.f;
	Probe->Seed = 0xC0FFEE;
	const float Surface = Probe->SurfaceRadiusAtServer(DirS);
	return DirS * (Surface + Clearance);
}

FVector AOUVoxelPlanet::HomeSpawn(float Clearance)
{
	// Точка появления в UE-осях и UE-сантиметрах (для установки актора).
	return OUInterop::ServerToUE(HomeSpawnServer(Clearance));
}

AOUVoxelPlanet* AOUVoxelPlanet::FindHomePlanet(UWorld* World)
{
	if (!World)
	{
		return nullptr;
	}
	// Домашняя планета помечена bHomePlanet; если такой нет — берём первую.
	AOUVoxelPlanet* Fallback = nullptr;
	for (TActorIterator<AOUVoxelPlanet> It(World); It; ++It)
	{
		AOUVoxelPlanet* P = *It;
		if (!P)
		{
			continue;
		}
		if (P->bHomePlanet)
		{
			return P;
		}
		if (!Fallback)
		{
			Fallback = P;
		}
	}
	return Fallback;
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

				// Цвет по высоте и типу планеты (океан/песок/трава/снег и т.д.).
				Colors.Add(SurfaceColor(Height));
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
