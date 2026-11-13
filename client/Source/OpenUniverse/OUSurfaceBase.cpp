// Copyright Open Universe. Все права защищены.

#include "OUSurfaceBase.h"
#include "OUInterop.h"
#include "OUTerrain.h"
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
	/** Амплитуда рельефа в серверных единицах. */
	constexpr double OUBigAmpServer = 10000.0;
	constexpr double OUBigSeaLevel = 0.545;
	constexpr uint64 OUBigSeed = 0xC0FFEE;

	/** Высота рельефа (в см UE) в UE-направлении, общая формула планеты. */
	double AnalyticHeightCm(const FVector& DirUE)
	{
		const FVector S = OUInterop::UEDirToServer(DirUE);
		const double H = OULT::HeightDir(S.X, S.Y, S.Z, OUBigAmpServer, OUBigSeaLevel, OUBigSeed, 1);
		return H * OUInterop::ServerUnitToCm;
	}
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
	// Смещение пола здания относительно «нуля» актора: актор стоит на
	// поверхности в центре площадки, поэтому высота под точкой (X,Y) — это
	// разница высот рельефа между этой точкой и центром.
	const FVector Up = GetActorUpVector();
	const FVector Fwd = GetActorForwardVector();
	const FVector Right = GetActorRightVector();
	const FVector World = GetActorLocation() + Fwd * LocalX + Right * LocalY;
	const FVector Dir = World.GetSafeNormal();
	if (Dir.IsNearlyZero())
	{
		return 0.f;
	}
	const double Center = GetActorLocation().Size();
	const double Here = OUBigRadiusServer * OUInterop::ServerUnitToCm
		+ AnalyticHeightCm(Dir);
	return static_cast<float>(Here - Center);
}

float AOUSurfaceBase::DeckOffsetCm(const FVector& CenterUE, float InPadRadius)
{
	// Прогоняем ту же сетку замеров, что и `BuildBase`, и берём максимум земли
	// относительно центра. Поле поднимается до самой высокой точки площадки.
	const FVector Up = CenterUE.GetSafeNormal();
	FVector Ref = FVector::CrossProduct(Up, FVector::UpVector);
	if (Ref.SizeSquared() < 1e-4f)
	{
		Ref = FVector::CrossProduct(Up, FVector::RightVector);
	}
	Ref = Ref.GetSafeNormal();
	const FVector Fwd = FVector::CrossProduct(Ref, Up).GetSafeNormal();
	const FVector Right = FVector::CrossProduct(Up, Fwd).GetSafeNormal();
	const double Center = CenterUE.Size();
	const int32 NS = 12;
	double GMax = -1e18;
	for (int32 I = 0; I <= NS; ++I)
	{
		for (int32 J = 0; J <= NS; ++J)
		{
			const float sx = -InPadRadius + (2.f * InPadRadius) * I / NS;
			const float sy = -InPadRadius + (2.f * InPadRadius) * J / NS;
			const FVector World = CenterUE + Fwd * sx + Right * sy;
			const FVector Dir = World.GetSafeNormal();
			if (Dir.IsNearlyZero())
			{
				continue;
			}
			const double Here = OUBigRadiusServer * OUInterop::ServerUnitToCm + AnalyticHeightCm(Dir);
			GMax = FMath::Max(GMax, Here - Center);
		}
	}
	return static_cast<float>(GMax) + 40.f;   // верх плиты = макс. земля + 40 см
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
	// Блок — твёрдый: персонаж не должен «проваливаться» сквозь него. Блокируем
	// и мир (капсула персонажа — Pawn), и луч прицела (Visibility).
	Comp->SetCollisionEnabled(ECollisionEnabled::QueryAndPhysics);
	Comp->SetCollisionObjectType(ECC_WorldStatic);
	Comp->SetCollisionResponseToAllChannels(ECR_Block);
	Comp->SetMobility(EComponentMobility::Movable);
	Comp->SetGenerateOverlapEvents(false);
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

	// --- Палитра комплекса: «техно» серый-синий, тёплые акценты, техцвет ---
	const FLinearColor Concrete(0.42f, 0.45f, 0.50f);
	const FLinearColor Deck(0.34f, 0.37f, 0.42f);
	const FLinearColor Steel(0.56f, 0.60f, 0.66f);
	const FLinearColor Dark(0.18f, 0.19f, 0.22f);
	const FLinearColor Glass(0.16f, 0.62f, 0.96f);
	const FLinearColor Warm(1.00f, 0.58f, 0.14f);
	const FLinearColor Fuel(0.88f, 0.74f, 0.24f);
	const FLinearColor Cyan(0.10f, 0.92f, 0.86f);
	const FLinearColor Strip(0.30f, 0.88f, 1.00f);

	// --- Посадочное поле: РОВНАЯ плита-платформа с бортом вниз к земле ---
	// Место появления — самое ровное на планете, но перепад в десятки метров
	// всё равно есть. Поэтому поле делаем единой ГОРИЗОНТАЛЬНОЙ плитой на
	// высоте самой высокой точки площадки, а под ней — тёмный борт до земли.
	// Так площадка действительно плоская (как настоящий космодром), а не
	// ступеньками повторяет рельеф.
	const float PadR = PadRadius;
	const int32 NS = 12;   // сетка замеров земли под полем
	float GMin = 1e9f, GMax = -1e9f;
	for (int32 I = 0; I <= NS; ++I)
	{
		for (int32 J = 0; J <= NS; ++J)
		{
			const float sx = -PadR + (2.f * PadR) * I / NS;
			const float sy = -PadR + (2.f * PadR) * J / NS;
			const float g = FloorHeightAt(sx, sy);
			GMin = FMath::Min(GMin, g);
			GMax = FMath::Max(GMax, g);
		}
	}
	const float DeckTop = GMax + 40.f;                     // верх плиты
	const float DeckThick = 150.f;
	const float DeckCenterZ = DeckTop - DeckThick * 0.5f;
	// Сама плита (одним блоком) + тёмный борт по периметру до земли.
	AddBlock(FVector(0, 0, DeckCenterZ), FVector(2.f * PadR, 2.f * PadR, DeckThick), Deck, TEXT("PadDeck"));
	const float SkirtBottom = GMin - 300.f;
	const float SkirtH = DeckTop - SkirtBottom;
	const float SkirtT = 260.f;
	auto Skirt = [&](float CX, float CY, float SX, float SY, const FString& Nm)
	{
		AddBlock(FVector(CX, CY, SkirtBottom + SkirtH * 0.5f), FVector(SX, SY, SkirtH), Dark, Nm);
	};
	Skirt(0.f, PadR, 2.f * PadR + SkirtT, SkirtT, TEXT("SkirtN"));
	Skirt(0.f, -PadR, 2.f * PadR + SkirtT, SkirtT, TEXT("SkirtS"));
	Skirt(PadR, 0.f, SkirtT, 2.f * PadR, TEXT("SkirtE"));
	Skirt(-PadR, 0.f, SkirtT, 2.f * PadR, TEXT("SkirtW"));

	// --- Разметка поля: светящаяся рамка по периметру и «Н»-полоса ---
	const float MarkZ = DeckTop + 20.f;
	const float Lw = 220.f;   // ширина полосы разметки
	auto EdgeStrip = [&](float CX, float CY, float SX, float SY, const FString& Nm)
	{
		AddBlock(FVector(CX, CY, MarkZ), FVector(SX, SY, 40.f), Strip, Nm);
	};
	EdgeStrip(0.f, PadR - Lw * 0.5f, 2.f * PadR, Lw, TEXT("MarkN"));
	EdgeStrip(0.f, -PadR + Lw * 0.5f, 2.f * PadR, Lw, TEXT("MarkS"));
	EdgeStrip(PadR - Lw * 0.5f, 0.f, Lw, 2.f * PadR, TEXT("MarkE"));
	EdgeStrip(-PadR + Lw * 0.5f, 0.f, Lw, 2.f * PadR, TEXT("MarkW"));
	// Две полосы «посадочной H» в центре поля.
	EdgeStrip(-PadR * 0.35f, 0.f, 260.f, PadR * 0.7f, TEXT("H1"));
	EdgeStrip(PadR * 0.35f, 0.f, 260.f, PadR * 0.7f, TEXT("H2"));
	EdgeStrip(0.f, 0.f, PadR * 0.7f, 260.f, TEXT("H3"));

	// Все здания ставим на ЕДИНЫЙ уровень поля (верх плиты): так комплекс
	// читается как один объект. Под каждым — фундамент до земли, чтобы ничего
	// не парило на склоне.
	auto AddOnDeck = [&](float CX, float CY, const FVector& Size, const FLinearColor& Color, const FString& Nm)
	{
		const float G = FloorHeightAt(CX, CY);
		const float Base = DeckTop;                    // низ здания на уровне поля
		// Фундамент от земли до уровня поля (если земля ниже).
		const float FoundTop = Base;
		const float FoundBottom = FMath::Min(G, Base) - 200.f;
		if (FoundTop > FoundBottom)
		{
			const float FH = FoundTop - FoundBottom;
			AddBlock(FVector(CX, CY, FoundBottom + FH * 0.5f),
				FVector(Size.X * 1.05f, Size.Y * 1.05f, FH), Dark, Nm + TEXT("Found"));
		}
		AddBlock(FVector(CX, CY, Base + Size.Z * 0.5f), Size, Color, Nm);
	};

	// --- Командно-диспетчерская вышка (север поля) ---
	{
		const float TX = 0.f, TY = PadR + 2600.f;
		AddOnDeck(TX, TY, FVector(3400, 2800, 1000.f), Concrete, TEXT("TowerBase"));
		AddOnDeck(TX, TY, FVector(2400, 2000, 2200.f), Steel, TEXT("TowerMid"));
		AddOnDeck(TX, TY, FVector(2000, 2600, 3200.f), Glass, TEXT("TowerGlass"));
		AddOnDeck(TX, TY, FVector(2200, 2800, 1000.f), Dark, TEXT("TowerRoof"));
		// Маяк на крыше вышки.
		AddOnDeck(TX, TY, FVector(120, 120, 3800.f), Steel, TEXT("TowerMast"));
		AddOnDeck(TX, TY, FVector(240, 240, 300.f), Cyan, TEXT("TowerLight"));
	}

	// --- Два ангара (западнее и восточнее поля) ---
	auto Hangar = [&](float CX, float CY, const FString& P)
	{
		AddOnDeck(CX, CY, FVector(5200, 3600, 1400.f), Concrete, P + TEXT("Body"));
		AddOnDeck(CX, CY, FVector(5400, 3800, 500.f), Steel, P + TEXT("Roof"));
		// Ворота (тёмный проём) сбоку, обращённые к полю.
		const float Sign = (CY > 0.f) ? -1.f : 1.f;
		AddOnDeck(CX, CY + Sign * 1850.f, FVector(3200, 200.f, 1000.f), Dark, P + TEXT("Gate"));
	};
	Hangar(-PadR - 3200.f, PadR * 0.4f, TEXT("HangarW"));
	Hangar(PadR + 3200.f, -PadR * 0.4f, TEXT("HangarE"));

	// --- Топливный склад: два ряда «бочек» (из квадратных блоков) ---
	auto Tank = [&](float CX, float CY, const FString& P)
	{
		AddOnDeck(CX, CY, FVector(700, 700, 1800.f), Fuel, P + TEXT("Tank"));
		AddOnDeck(CX, CY, FVector(760, 760, 160.f), Steel, P + TEXT("Cap"));
	};
	for (int32 i = 0; i < 3; ++i)
	{
		const float X = -PadR + 900.f + i * 900.f;
		Tank(X, -PadR - 1600.f, FString::Printf(TEXT("FT%d"), i));
	}
	for (int32 i = 0; i < 3; ++i)
	{
		const float X = -PadR + 900.f + i * 900.f;
		Tank(X, -PadR - 2600.f, FString::Printf(TEXT("FT%d"), i + 3));
	}

	// --- Терминалы и стойки по южному краю поля ---
	for (int32 i = 0; i < 5; ++i)
	{
		const float X = -PadR + 1200.f + i * 1400.f;
		const float CY = PadR + 900.f;
		AddOnDeck(X, CY, FVector(700, 500, 800.f), Steel, FString::Printf(TEXT("Tr%d"), i));
		AddOnDeck(X, CY - 260.f, FVector(560, 60, 360.f), Warm, FString::Printf(TEXT("TrS%d"), i));
	}

	// --- Угловые маяки (4) с яркими огнями ---
	const float M = PadR * 0.82f;
	auto Corner = [&](float CX, float CY, const FString& P)
	{
		AddOnDeck(CX, CY, FVector(260, 260, 900.f), Steel, P + TEXT("Pole"));
		AddOnDeck(CX, CY, FVector(420, 420, 260.f), Warm, P + TEXT("Beacon"));
	};
	Corner(M, M, TEXT("C0"));
	Corner(-M, M, TEXT("C1"));
	Corner(M, -M, TEXT("C2"));
	Corner(-M, -M, TEXT("C3"));

	// --- Высокий маяк-ориентир (далеко виден) ---
	{
		const float BX = PadR + 1800.f, BY = PadR + 1800.f;
		AddOnDeck(BX, BY, FVector(300, 300, 3000.f), Steel, TEXT("BigMast"));
		AddOnDeck(BX, BY, FVector(500, 500, 500.f), Cyan, TEXT("BigLight"));
	}

	UE_LOG(LogOUBase, Log, TEXT("Комплекс построен: поле %.0f м, %d элементов"), PadR * 2.f / 100.f,
		GetComponents().Num());
}
