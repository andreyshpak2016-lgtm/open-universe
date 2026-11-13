// Copyright Open Universe. Все права защищены.

#include "OUVoxelPlanet.h"
#include "OUInterop.h"
#include "OUTerrain.h"
#include "OUDeterministic.h"
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
	Mesh->bUseComplexAsSimpleCollision = false;

	// Локальный детальный участок поверхности: отдельный меш, который
	// пристраивается вокруг игрока (глобальный меш слишком грубый для
	// мелкого рельефа). Коллизию ему не даём — физику ведёт сервер.
	LocalMesh = CreateDefaultSubobject<UProceduralMeshComponent>(TEXT("LocalTerrainMesh"));
	LocalMesh->SetupAttachment(Mesh);
	LocalMesh->bUseAsyncCooking = true;
	LocalMesh->SetCollisionEnabled(ECollisionEnabled::NoCollision);

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

	// --- Атмосфера и облака ---
	// Атмосфера — тонкая сфера чуть больше планеты, полупрозрачный материал.
	static ConstructorHelpers::FObjectFinder<UStaticMesh> Sphere(
		TEXT("/Engine/BasicShapes/Sphere.Sphere"));
	if (Sphere.Succeeded())
	{
		CloudSphereMesh = Sphere.Object;
	}
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> AtmoMat(
		TEXT("/Engine/EngineDebugMaterials/M_SimpleUnlitTranslucent.M_SimpleUnlitTranslucent"));
	if (AtmoMat.Succeeded())
	{
		AtmosphereMaterial = AtmoMat.Object;
	}
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> CloudMat(
		TEXT("/Engine/EngineDebugMaterials/M_SimpleUnlitTranslucent.M_SimpleUnlitTranslucent"));
	if (CloudMat.Succeeded())
	{
		CloudMaterial = CloudMat.Object;
	}

	// Слой атмосферы: сфера, по умолчанию скрыта (включается по типу планеты).
	AtmosphereMesh = CreateDefaultSubobject<UStaticMeshComponent>(TEXT("Atmosphere"));
	AtmosphereMesh->SetupAttachment(Mesh);
	AtmosphereMesh->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	AtmosphereMesh->SetCastShadow(false);
	if (CloudSphereMesh)
	{
		AtmosphereMesh->SetStaticMesh(CloudSphereMesh);
	}
	if (AtmosphereMaterial)
	{
		AtmosphereMesh->SetMaterial(0, AtmosphereMaterial);
	}
	AtmosphereMesh->SetVisibility(false);

	// Пул облачных «клубов»: видимость включается только у планет с облаками.
	CloudPuffs.Reserve(MaxCloudPuffs);
	for (int32 i = 0; i < MaxCloudPuffs; ++i)
	{
		UStaticMeshComponent* Puff = CreateDefaultSubobject<UStaticMeshComponent>(
			*FString::Printf(TEXT("Cloud%d"), i));
		Puff->SetupAttachment(Mesh);
		Puff->SetCollisionEnabled(ECollisionEnabled::NoCollision);
		Puff->SetCastShadow(false);
		if (CloudSphereMesh)
		{
			Puff->SetStaticMesh(CloudSphereMesh);
		}
		if (CloudMaterial)
		{
			Puff->SetMaterial(0, CloudMaterial);
		}
		Puff->SetVisibility(false);
		CloudPuffs.Add(Puff);
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
	case EOUVoxelLod::Far:   return 16;
	case EOUVoxelLod::Mid:   return 64;
	case EOUVoxelLod::Near:  return 256;
	case EOUVoxelLod::Close: return 512;   // крупная планета: ячейка ~1.5 км
	default:                 return 64;
	}
}

float AOUVoxelPlanet::HeightAtServerDetail(const FVector& DirServer, int32 Detail) const
{
	// Одна общая формула с сервером (см. OUTerrain.h): бит-в-бит тот же рельеф,
	// что считает физика. Все вычисления — в double, как в Rust. Воронки игроков
	// (копание) вычитаются из базовой высоты — тоже как на сервере.
	const double H = OULT::HeightWithDents(
		static_cast<double>(DirServer.X),
		static_cast<double>(DirServer.Y),
		static_cast<double>(DirServer.Z),
		static_cast<double>(Amplitude),
		0.545,                      // уровень океана (как на сервере)
		static_cast<uint64>(Seed),
		Detail,
		Dents.Num() > 0 ? Dents.GetData() : nullptr,
		Dents.Num());
	return static_cast<float>(H);
}

void AOUVoxelPlanet::ApplyDent(int32 Slot, double X, double Y, double Z, double CosRadius, double Depth)
{
	// Кладём воронку в слот (или углубляем существующую в этом слоте) и
	// перестраиваем рельеф: глобальный меш и локальный участок.
	while (Dents.Num() <= Slot)
	{
		Dents.Add(OULT::Dent());
	}
	OULT::Dent& D = Dents[Slot];
	const bool bSameDir = FMath::Abs(D.X - X) < 1e-6 && FMath::Abs(D.Y - Y) < 1e-6 && FMath::Abs(D.Z - Z) < 1e-6;
	if (bSameDir && D.Depth > 0.0)
	{
		D.Depth += Depth;
		D.CosR = FMath::Max(D.CosR, CosRadius);
	}
	else
	{
		D.X = X; D.Y = Y; D.Z = Z;
		D.CosR = CosRadius;
		D.Depth = Depth;
	}
	// Воронка мелкая по сравнению с глобальным мешем (ячейка ~1.5 км), поэтому
	// глобальную сферу не перестраиваем — это дорого и ничего не даёт. Яма видна
	// на ПЛОТНОМ локальном участке у игрока (ячейка ~21 м), его и обновляем.
	if (LocalDentCenter.IsNearlyZero() == false)
	{
		RebuildLocal(LocalDentCenter);
	}
	UE_LOG(LogOUPlanet, Log, TEXT("Воронка %d применена: глубина %.0f, всего воронок %d"),
		Slot, D.Depth, Dents.Num());
}

float AOUVoxelPlanet::HeightAtServer(const FVector& DirServer) const
{
	// По умолчанию — ПОЛНЫЙ рельеф (detail=1): он совпадает с физикой сервера
	// (посадка, столкновения) и с плотным локальным участком у игрока.
	return HeightAtServerDetail(DirServer, 1);
}

float AOUVoxelPlanet::HeightAtDetail(const FVector& Direction, int32 Detail) const
{
	// Направление приходит в UE-осях (вверх = Z). Серверный рельеф считаем в
	// серверных осях — переводим, иначе высота не совпадёт с сервером.
	return HeightAtServerDetail(OUInterop::UEDirToServer(Direction), Detail);
}

float AOUVoxelPlanet::HeightAt(const FVector& Direction) const
{
	return HeightAtDetail(Direction, 1);
}

float AOUVoxelPlanet::SurfaceRadiusAt(const FVector& Direction) const
{
	return Radius + HeightAt(Direction);
}

float AOUVoxelPlanet::VisualHeight(const FVector& DirUE) const
{
	return VisualHeightDetail(DirUE, 1);
}

float AOUVoxelPlanet::VisualHeightDetail(const FVector& DirUE, int32 Detail) const
{
	const float Base = HeightAtDetail(DirUE, Detail);
	// Домашняя планета: физика и картинка совпадают (по ней ходит игрок).
	if (bHomePlanet)
	{
		return Base;
	}
	const FVector D = DirUE.GetSafeNormal();
	const float T = FMath::Clamp(Base / (Amplitude * 2.f) + 0.5f, 0.f, 1.f);
	switch (PlanetType)
	{
	case EOUPlanetType::Desert:
	{
		// Дюны: направленные волны поверх обычного рельефа.
		const float Dune = FMath::Sin(D.X * 22.f + D.Z * 14.f) * 0.5f
			+ FMath::Sin(D.Y * 18.f - D.Z * 10.f) * 0.5f;
		return Base * 0.7f + Dune * Amplitude * 0.35f;
	}
	case EOUPlanetType::Airless:
	{
		// Луна: неровный «кратерный» рельеф с крутыми перепадами.
		const float Crat = FMath::Abs(FMath::Sin(D.X * 31.f) * FMath::Sin(D.Y * 27.f) * FMath::Sin(D.Z * 29.f));
		return Base * 0.6f + (Crat - 0.4f) * Amplitude * 0.9f;
	}
	case EOUPlanetType::GasGiant:
	{
		// Газовый гигант: почти гладкие полосы, без резких гор.
		return Base * 0.15f;
	}
	case EOUPlanetType::Volcanic:
	{
		// Вулканический: резкие пики и глубокие разломы.
		const float Ridge = 1.f - FMath::Abs(FMath::Sin(D.X * 17.f + D.Z * 13.f));
		return Base * 0.8f + Ridge * Amplitude * 0.6f;
	}
	default:
		return Base;
	}
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
	case EOUPlanetType::Airless:
	{
		// Безвоздушная луна: серый реголит и кратеры, без воды и трав.
		const FLinearColor Regolith(0.42f, 0.40f, 0.38f);
		const FLinearColor Basalt(0.22f, 0.21f, 0.22f);
		const FLinearColor Bright(0.62f, 0.60f, 0.56f);
		return T < 0.5f
			? FLinearColor::LerpUsingHSV(Basalt, Regolith, T / 0.5f)
			: FLinearColor::LerpUsingHSV(Regolith, Bright, (T - 0.5f) / 0.5f);
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

float AOUVoxelPlanet::LocalDetailFade(float DistCm) const
{
	// Полное сохранение мелкого рельефа в глубине участка, плавное гашение к
	// его краю (последние ~15%). Там глобальный меш показывает только крупный
	// рельеф, поэтому у границы мелочь должна сойти на нет — иначе виден обрыв.
	const float Edge = LocalExtentCm * 0.85f;
	if (DistCm <= Edge)
	{
		return 1.f;
	}
	const float T = FMath::Clamp((DistCm - Edge) / FMath::Max(1.f, LocalExtentCm - Edge), 0.f, 1.f);
	return 1.f - T * T * (3.f - 2.f * T);   // smoothstep вниз к 0
}

FVector AOUVoxelPlanet::SmoothSurfaceNormal(const FVector& DirUE, int32 Detail) const
{
	// Касательный базис в точке.
	const FVector Up = DirUE.GetSafeNormal();
	if (Up.IsNearlyZero())
	{
		return FVector::UpVector;
	}
	FVector T = FVector::CrossProduct(Up, FVector::UpVector);
	if (T.SizeSquared() < 1e-4f)
	{
		T = FVector::CrossProduct(Up, FVector::RightVector);
	}
	T = T.GetSafeNormal();
	const FVector B = FVector::CrossProduct(Up, T).GetSafeNormal();

	// Шаг конечных разностей: ~половина ячейки меша. Для полного рельефа
	// (локальный участок) шаг меньше — там видна мелочь.
	const float H = (Detail >= 1) ? (LocalExtentCm / FMath::Max(1, LocalCells) * 0.5f)
		: 150000.f;   // ~1.5 км для глобального меша
	auto At = [this, &DirUE, &T, &B, H, Detail](float SA, float SB)
	{
		// Смещаем вдоль касательных по сфере и берём высоту.
		const FVector N = (DirUE + T * (SA / Radius) + B * (SB / Radius)).GetSafeNormal();
		return VisualHeightDetail(N, Detail);
	};
	const float HL = At(-H, 0.f), HR = At(H, 0.f);
	const float HD = At(0.f, -H), HU = At(0.f, H);

	// Градиент высоты вдоль T и B.
	const float GT = (HR - HL) / (2.f * H);
	const float GB = (HU - HD) / (2.f * H);
	const FVector Normal = (Up - T * GT - B * GB).GetSafeNormal();
	return Normal.IsNearlyZero() ? Up : Normal;
}

FLinearColor AOUVoxelPlanet::ShadedVertexColor(float Height, const FVector& SmoothNormal) const
{
	FLinearColor Base = SurfaceColor(Height);
	// Вода — гладкая и блестящая: ровный цвет без резких теней рельефа.
	if (Height <= 0.f)
	{
		return Base;
	}
	// Направление «солнца» (совпадает с DirectionalLight в GameMode:
	// FRotator(-38, -30, 0) → вектор НА солнце).
	const FVector Sun = FVector(-0.682f, 0.394f, 0.616f).GetSafeNormal();
	const float NdotL = FMath::Clamp(FVector::DotProduct(SmoothNormal, Sun), 0.f, 1.f);
	// Диффузия с заметным контрастом: склоны к солнцу светлее, от солнца —
	// темнее. Так рельеф читается даже с неосвещаемым материалом.
	const float Diffuse = 0.28f + 0.72f * NdotL;
	// Лёгкий градиент по высоте: низины чуть темнее (читается объём).
	const float HeightTint = 0.90f + 0.10f * FMath::Clamp(Height / (Amplitude * 2.f) + 0.5f, 0.f, 1.f);
	const float K = Diffuse * HeightTint;
	return FLinearColor(Base.R * K, Base.G * K, Base.B * K, Base.A);
}

FVector AOUVoxelPlanet::HomeSpawnDirectionServer()
{
	// Общая формула (OUTerrain.h) — ОДИН источник истины с сервером: ищем самое
	// ровное место на суше. Возвращаем в серверных осях; сервер выберет то же
	// самое (сверено кросс-проверкой рельефа).
	double X = 0.0, Y = 1.0, Z = 0.0;
	OULT::HomeSpawnDirectionServer(10000.0, 0.545, 0xC0FFEEull, X, Y, Z);
	return FVector(X, Y, Z);
}

double AOUVoxelPlanet::SpawnRoughnessServer(const FVector& DirServer, double RadiusM) const
{
	return OULT::SpawnRoughness(
		DirServer.X, DirServer.Y, DirServer.Z,
		RadiusM, 10000.0, 0.545, 0xC0FFEEull);
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
	Probe->Radius = 500000.f;
	Probe->Amplitude = 10000.f;
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

	// 6 граней куба: (ось1, ось2, знак) — по одной на каждую ось в обе стороны.
	//
	// ВАЖНО: таблица должна покрывать ВСЕ шесть граней куба (+X,−X,+Y,−Y,+Z,−Z).
	// Раньше третья и четвёртая строки дублировали ±X вместо ±Y, и граней ±Z не
	// было вовсе — сфера получалась «разрезанной»: целая половина поверхности
	// отсутствовала, и игрок видел внутренность планеты («ядро»). Проверено
	// численно: старая таблица покрывала ~51% сферы, новая — 100%.
	const int32 Faces[6][3] = {
		{0, 1,  1}, {0, 1, -1},   // ±X
		{1, 0,  1}, {1, 0, -1},   // ±Y
		{2, 0,  1}, {2, 0, -1}    // ±Z
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
				// ГЛОБАЛЬНЫЙ меш: только КРУПНЫЙ рельеф (Detail=0). Мелкие волны
				// (сотни метров) ячейка ~1.5 км не разрешает — их «алиасит» в
				// плиты, и камера уходит под грань (серая изнанка поверхности).
				// Мелочь показывает плотный локальный участок (RebuildLocal).
				const float Height = VisualHeightDetail(Dir, 0);
				Verts.Add(Dir * (Radius + Height));

				// Нормаль — со сглаживанием по рельефу, иначе грань выглядит
				// плоской и свет не читается.
				const FVector Nrm = SmoothSurfaceNormal(Dir, 0);
				Normals.Add(Nrm);
				UVs.Add(FVector2D(U * 0.5f + 0.5f, Vv * 0.5f + 0.5f));

				// Цвет по высоте и типу планеты (океан/песок/трава/снег и т.д.)
				// с запечённым освещением — материал планеты не освещается движком.
				Colors.Add(ShadedVertexColor(Height, Nrm));
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

	// Атмосфера и облака зависят от типа планеты (у безвоздушных их нет).
	BuildAtmosphere();
}

void AOUVoxelPlanet::RebuildLocal(const FVector& AroundUE)
{
	if (!LocalMesh)
	{
		return;
	}
	LocalMesh->ClearAllMeshSections();

	// Точка на поверхности и её нормаль (в UE-осях).
	const FVector DirUE = AroundUE.GetSafeNormal();
	if (DirUE.IsNearlyZero())
	{
		return;
	}
	// У мелкого рельефа смысл есть только у планеты, по которой ходят.
	if (!bHomePlanet || Radius <= 0.f)
	{
		return;
	}
	// Диагностика: `-OUNoLocal` отключает локальный участок (сравнение кадров).
	if (FParse::Param(FCommandLine::Get(), TEXT("OUNoLocal")))
	{
		return;
	}
	// Запоминаем центр: если придут воронки (копание), перестроим участок тут же.
	LocalDentCenter = AroundUE;

	// Касательный базис в точке.
	FVector T1 = FVector::CrossProduct(DirUE, FVector::UpVector);
	if (T1.SizeSquared() < 1e-4f)
	{
		T1 = FVector::CrossProduct(DirUE, FVector::RightVector);
	}
	T1 = T1.GetSafeNormal();
	const FVector T2 = FVector::CrossProduct(DirUE, T1).GetSafeNormal();

	// Сетка в касательной плоскости; каждую вершину проецируем на сферу и
	// берём высоту из ТОЙ ЖЕ формулы, что глобальный меш и сервер.
	const int32 N = FMath::Clamp(LocalCells, 8, 512);
	const float Step = (2.f * LocalExtentCm) / N;

	TArray<FVector> Verts;
	TArray<int32> Tris;
	TArray<FVector> Normals;
	TArray<FVector2D> UVs;
	TArray<FLinearColor> Colors;
	TArray<struct FProcMeshTangent> Tangents;
	Verts.Reserve((N + 1) * (N + 1));

	for (int32 J = 0; J <= N; ++J)
	{
		for (int32 I = 0; I <= N; ++I)
		{
			const float A = -LocalExtentCm + I * Step;   // вдоль T1
			const float B = -LocalExtentCm + J * Step;   // вдоль T2
			// Точка в касательной плоскости, затем — на сферу планеты.
			const FVector Tangent = DirUE * Radius + T1 * A + T2 * B;
			const FVector Dir = Tangent.GetSafeNormal();
			// ЛОКАЛЬНЫЙ участок: ПОЛНЫЙ рельеф (Detail=1). Мелкие волны у края
			// ГАСИМ (fade), иначе на границе участка был бы обрыв: там глобальный
			// меш показывает только крупный рельеф (Detail=0). Внутри участка
			// (там, где ходит игрок) гашения нет — высота совпадает с физикой.
			const float Fade = LocalDetailFade(FMath::Max(FMath::Abs(A), FMath::Abs(B)));
			const float HCoarse = VisualHeightDetail(Dir, 0);
			const float HFull = VisualHeightDetail(Dir, 1);
			const float Height = HCoarse + (HFull - HCoarse) * Fade;
			Verts.Add(Dir * (Radius + Height));
			const FVector Nrm = SmoothSurfaceNormal(Dir, 1);
			Normals.Add(Nrm);
			UVs.Add(FVector2D(static_cast<float>(I) / N, static_cast<float>(J) / N));
			Colors.Add(ShadedVertexColor(Height, Nrm));
			Tangents.Add(FProcMeshTangent(T1.X, T1.Y, T1.Z));
		}
	}

	for (int32 J = 0; J < N; ++J)
	{
		for (int32 I = 0; I < N; ++I)
		{
			const int32 A = J * (N + 1) + I;
			const int32 B = A + 1;
			const int32 C = A + (N + 1);
			const int32 D = C + 1;
			Tris.Add(A); Tris.Add(C); Tris.Add(B);
			Tris.Add(B); Tris.Add(C); Tris.Add(D);
		}
	}
	// Двусторонняя намотка (как у глобального меша): участок виден с любой
	// стороны и не мерцает на стыке с глобальной сферой.
	TArray<int32> Back;
	Back.Reserve(Tris.Num());
	for (int32 T = 0; T + 2 < Tris.Num(); T += 3)
	{
		Back.Add(Tris[T + 0]);
		Back.Add(Tris[T + 2]);
		Back.Add(Tris[T + 1]);
	}
	Tris.Append(Back);

	LocalMesh->CreateMeshSection_LinearColor(0, Verts, Tris, Normals, UVs, Colors, Tangents, false);
	LocalMesh->SetMaterial(0, PlanetMaterial);
	UE_LOG(LogOUPlanet, Log, TEXT("Локальный участок вокруг игрока: ячеек %d, шаг %.0f см"), N, Step);
}

bool AOUVoxelPlanet::TypeHasAtmosphere(EOUPlanetType Type)
{
	// Воздух есть у всех типов, КРОМЕ безвоздушной луны.
	return Type != EOUPlanetType::Airless;
}

bool AOUVoxelPlanet::TypeHasClouds(EOUPlanetType Type)
{
	switch (Type)
	{
	case EOUPlanetType::Terran:  return true;    // плотные облака
	case EOUPlanetType::Ice:     return true;    // облака/туман
	case EOUPlanetType::GasGiant:return true;    // полосы и вихри
	case EOUPlanetType::Volcanic:return true;    // дымка из пепла
	case EOUPlanetType::Desert:  return false;   // тонкая пыль, облаков нет
	case EOUPlanetType::Airless: return false;   // вакуум
	default:                     return false;
	}
}

FLinearColor AOUVoxelPlanet::AtmosphereTint(EOUPlanetType Type)
{
	switch (Type)
	{
	case EOUPlanetType::Terran:  return FLinearColor(0.35f, 0.60f, 1.00f);   // голубое небо
	case EOUPlanetType::Desert:  return FLinearColor(0.85f, 0.65f, 0.40f);   // пыльная дымка
	case EOUPlanetType::Ice:     return FLinearColor(0.70f, 0.88f, 1.00f);   // морозная дымка
	case EOUPlanetType::Volcanic:return FLinearColor(0.70f, 0.30f, 0.15f);   // пепел
	case EOUPlanetType::GasGiant:return FLinearColor(0.95f, 0.85f, 0.65f);   // тёплые полосы
	case EOUPlanetType::Airless: return FLinearColor(0.0f, 0.0f, 0.0f);       // нет атмосферы
	default:                     return FLinearColor(0.5f, 0.6f, 0.8f);
	}
}

float AOUVoxelPlanet::CloudNoise(const FVector& Dir) const
{
	// Независимый от рельефа шум облаков: другое семя и масштаб.
	const FVector D = Dir.GetSafeNormal();
	const float U = D.X * 6.0f + D.Y * 3.0f;
	const float V = D.Z * 6.0f - D.Y * 2.0f;
	const float A = ValueNoise(U, V, Seed ^ 0xC10D);
	const float B = ValueNoise(U * 2.7f, V * 2.7f, Seed ^ 0xBEEF);
	return A * 0.6f + B * 0.4f;
}

void AOUVoxelPlanet::BuildAtmosphere()
{
	const bool bAtmo = TypeHasAtmosphere(PlanetType) && AtmosphereMesh != nullptr;
	const bool bClouds = TypeHasClouds(PlanetType);

	// Диагностика: `-OUNoClouds` полностью выключает атмосферу и облака.
	if (FParse::Param(FCommandLine::Get(), TEXT("OUNoClouds")))
	{
		for (UStaticMeshComponent* Puff : CloudPuffs)
		{
			if (Puff) { Puff->SetVisibility(false); }
		}
		if (AtmosphereMesh) { AtmosphereMesh->SetVisibility(false); }
		return;
	}

	UE_LOG(LogOUPlanet, Log, TEXT("Атмосфера: %s, облака: %s, тип %d"),
		bAtmo ? TEXT("да") : TEXT("нет"), bClouds ? TEXT("да") : TEXT("нет"),
		static_cast<int32>(PlanetType));

	// --- Атмосферный слой ---
	// Штатный материал оболочки (M_SimpleUnlitTranslucent) идёт без параметра
	// Opacity и рисуется непрозрачно — он «закрашивал» диск и прятал палитру
	// поверхности. Поэтому оболочку не рисуем: вид планеты дают палитра
	// поверхности (от сервера) и облака. Небо домашней планеты — SkyAtmosphere.
	if (AtmosphereMesh)
	{
		AtmosphereMesh->SetVisibility(false);
	}

	// --- Облака ---
	// Домашняя планета: облака высоко в небе (игрок смотрит вверх).
	// Дальние планеты: облачные пятна у самой поверхности — так они читаются
	// прямо на диске планеты.
	if (CloudPuffs.Num() == 0)
	{
		return;
	}
	if (!bClouds)
	{
		for (UStaticMeshComponent* Puff : CloudPuffs)
		{
			if (Puff) { Puff->SetVisibility(false); }
		}
		return;
	}

	const FLinearColor Tint = AtmosphereTint(PlanetType);
	const FLinearColor CloudColor = FLinearColor::LerpUsingHSV(
		FLinearColor(0.99f, 1.0f, 1.0f), Tint, 0.15f);
	const float CloudAlt = bHomePlanet
		? Radius + Radius * 0.02f                  // ~2 км над поверхностью
		: Radius + Amplitude * 0.15f;              // у поверхности — на диске планеты
	const float CloudSize = bHomePlanet
		? Radius * 0.006f                          // ~600 м: разрозненные облака
		: Radius * 0.05f;

	// Раскладываем облака детерминированно по направлениям, где шум выше
	// порога. Каждое «облако» — кластер из нескольких перекрывающихся сфер
	// (объёмная кучка, а не плоский диск).
	const int32 PuffCount = CloudPuffs.Num();
	int32 Placed = 0;
	const int32 GridN = 14;
	const int32 ClusterSize = 4;   // сфер на одно облако
	for (int32 GridI = 0; GridI < GridN && Placed < PuffCount; ++GridI)
	{
		for (int32 GridJ = 0; GridJ < GridN && Placed < PuffCount; ++GridJ)
		{
			// Направление по сетке (детерминированно, без ГПСЧ).
			// Оси UE: вверх = +Z (меш планеты тоже построен в UE-осях).
			const float A = (GridI + 0.5f) / GridN * 2.f * PI;
			const float B = (GridJ + 0.5f) / GridN * PI;
			const FVector Dir(
				FMath::Sin(B) * FMath::Cos(A),
				FMath::Sin(B) * FMath::Sin(A),
				FMath::Cos(B));
			const float N = CloudNoise(Dir);
			if (N < 0.46f)
			{
				continue;   // здесь ясно
			}
			// Касательный базис для разброса сфер внутри кластера.
			const FVector Up = Dir.GetSafeNormal();
			FVector T1 = FVector::CrossProduct(Up, FVector::UpVector);
			if (T1.SizeSquared() < 1e-4f) { T1 = FVector::CrossProduct(Up, FVector::RightVector); }
			T1 = T1.GetSafeNormal();
			const FVector T2 = FVector::CrossProduct(Up, T1).GetSafeNormal();
			const float Base = CloudSize * (0.9f + N * 1.6f);

			const int32 InCluster = FMath::Min(ClusterSize, PuffCount - Placed);
			for (int32 C = 0; C < InCluster; ++C)
			{
				UStaticMeshComponent* Puff = CloudPuffs[Placed];
				if (!Puff) { ++Placed; continue; }
				// Разброс внутри кластера — детерминированный (по индексу).
				const float FA = (C + 1) * 2.399f;   // золотой угол
				const float FR = Base * 0.6f * ((C == 0) ? 0.f : 1.f);
				const FVector Offset = (T1 * FMath::Cos(FA) + T2 * FMath::Sin(FA)) * FR;
				const FVector Pos = Up * (CloudAlt + Offset.Size() * 0.f) + Offset;
				Puff->SetRelativeLocation(Pos);
				const float Size = Base * (C == 0 ? 1.15f : 0.8f);
				Puff->SetRelativeScale3D(FVector(Size / 50.f));
				Puff->SetVectorParameterValueOnMaterials(TEXT("Color"),
					FVector(CloudColor.R, CloudColor.G, CloudColor.B));
				Puff->SetVectorParameterValueOnMaterials(TEXT("Emissive"),
					FVector(CloudColor.R, CloudColor.G, CloudColor.B));
				Puff->SetVisibility(true);
				Puff->SetCastShadow(false);
				++Placed;
			}
		}
	}
	// Остальные клубы — скрыть.
	for (int32 i = Placed; i < PuffCount; ++i)
	{
		if (CloudPuffs[i]) { CloudPuffs[i]->SetVisibility(false); }
	}
}
