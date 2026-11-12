// Copyright Open Universe. Все права защищены.

#include "OUGameMode.h"
#include "OUShipPawn.h"
#include "OUCharacterPawn.h"
#include "OUOriginManager.h"
#include "OUVoxelPlanet.h"
#include "OUSurfaceBase.h"
#include "OUSolarBody.h"
#include "OUInterop.h"
#include "OUPlayerMenuController.h"
#include "OUPlayerMenuHUD.h"
#include "OUClientNet.h"

#include "Engine/World.h"
#include "Engine/DirectionalLight.h"
#include "Engine/SkyLight.h"
#include "Engine/ExponentialHeightFog.h"
#include "Engine/PostProcessVolume.h"
#include "GameFramework/PlayerStart.h"
#include "Components/DirectionalLightComponent.h"
#include "Components/SkyLightComponent.h"
#include "Components/SkyAtmosphereComponent.h"
#include "EngineUtils.h"
#include "Kismet/GameplayStatics.h"
#include "UnrealClient.h"
#include "Misc/DateTime.h"

DEFINE_LOG_CATEGORY_STATIC(LogOUGameMode, Log, All);

namespace
{
	/**
	 * Центр домашней планеты в осях UE.
	 *
	 * Совпадает с сервером: домашняя планета расположена в начале координат
	 * мира (`ou_sim::home_planet`). Игрок появляется на её поверхности.
	 */
	const FVector GPlanetOffset(0.f, 0.f, 0.f);
}

AOUGameMode::AOUGameMode()
{
	// Игрок появляется **персонажем** на поверхности планеты; корабль стоит
	// рядом отдельным объектом. В корабль можно сесть (удержание F).
	DefaultPawnClass = AOUCharacterPawn::StaticClass();
	// Стартовое меню: персонаж и выбор сервера.
	PlayerControllerClass = AOUPlayerMenuController::StaticClass();
	// HUD рисует само меню.
	HUDClass = AOUPlayerMenuHUD::StaticClass();
}

void AOUGameMode::StartPlay()
{
	// Достраиваем сцену до старта: даже на пустой карте мир появится целиком.
	BuildWorld();
	Super::StartPlay();
}

void AOUGameMode::BeginPlay()
{
	Super::BeginPlay();

	// Автоснимок для отчётов/витрины: через движок, чтобы не зависеть от фокуса.
	if (bAutoScreenshots)
	{
		RequestShot(TEXT("start"));
	}
}

void AOUGameMode::TakeShot(const FString& Label)
{
	RequestShot(Label);
}

void AOUGameMode::RequestShot(const FString& Label)
{
	PendingShotLabel = Label;
	UWorld* World = GetWorld();
	if (!World)
	{
		return;
	}
	FScreenshotRequest::RequestScreenshot(
		FString::Printf(TEXT("OU_%s_%s"), *Label, *FDateTime::Now().ToString(TEXT("%H%M%S"))),
		/*bShowUI=*/false, /*bAddFilenameSuffix=*/false);
	UE_LOG(LogOUGameMode, Log, TEXT("Запрошен скриншот: OU_%s"), *Label);
}

AActor* AOUGameMode::ChoosePlayerStart_Implementation(AController* Player)
{
	// На пустой карте PlayerStart отсутствует — тогда UE не создаёт Pawn и
	// камеры нет (чёрный экран). Создаём точку старта сами, **на поверхности
	// домашней планеты**, чтобы игрок появлялся на суше с ландшафтом.
	AActor* Existing = Super::ChoosePlayerStart_Implementation(Player);
	if (Existing)
	{
		return Existing;
	}

	UWorld* World = GetWorld();
	if (!World)
	{
		return nullptr;
	}
	const FVector Spawn = AOUVoxelPlanet::HomeSpawn(200.f);
	APlayerStart* Start = World->SpawnActor<APlayerStart>(
		APlayerStart::StaticClass(),
		FTransform(FRotator(-15.f, 0.f, 0.f), Spawn));
	UE_LOG(LogOUGameMode, Log, TEXT("Создана точка старта PlayerStart на планете: %s"), *Spawn.ToString());
	return Start;
}

void AOUGameMode::BuildWorld()
{
	UWorld* World = GetWorld();
	if (!World)
	{
		return;
	}

	/** Есть ли в мире актор указанного класса. */
	auto Has = [World](UClass* Class) -> bool
	{
		return UGameplayStatics::GetActorOfClass(World, Class) != nullptr;
	};

	// --- Небо и атмосфера ---
	if (!Has(ASkyAtmosphere::StaticClass()))
	{
		World->SpawnActor<ASkyAtmosphere>(ASkyAtmosphere::StaticClass(), FTransform::Identity);
	}

	// --- Экспозиция (пост-обработка) ---
	// Без фиксации экспозиции яркое небо «пересвечивает» кадр: автоподбор
	// вытягивает яркость до белого. Держим экспозицию ручной, чтобы цвета
	// планет и поверхности читались.
	if (!Has(APostProcessVolume::StaticClass()))
	{
		APostProcessVolume* PP = World->SpawnActor<APostProcessVolume>(
			APostProcessVolume::StaticClass(), FTransform::Identity);
		if (PP)
		{
			PP->bUnbound = true;   // действует на всю сцену
			PP->Settings.bOverride_AutoExposureMethod = true;
			PP->Settings.AutoExposureMethod = EAutoExposureMethod::AEM_Manual;
			PP->Settings.bOverride_AutoExposureBias = true;
			PP->Settings.AutoExposureBias = 10.0f;   // фиксированная EV100
			PP->Settings.bOverride_AutoExposureMinBrightness = true;
			PP->Settings.AutoExposureMinBrightness = 1.0f;
			PP->Settings.bOverride_AutoExposureMaxBrightness = true;
			PP->Settings.AutoExposureMaxBrightness = 1.0f;
		}
	}

	// --- Солнце (светило) ---
	if (!Has(ADirectionalLight::StaticClass()))
	{
		const FTransform SunT(FRotator(-38.f, -30.f, 0.f), FVector(0.f, 0.f, 200000.f));
		ADirectionalLight* Sun = World->SpawnActor<ADirectionalLight>(ADirectionalLight::StaticClass(), SunT);
		if (Sun)
		{
			Sun->GetLightComponent()->SetMobility(EComponentMobility::Movable);
			Sun->GetLightComponent()->SetIntensity(3.f);
			Sun->GetLightComponent()->SetLightColor(FLinearColor(1.f, 0.96f, 0.9f));
		}
	}

	// --- Рассеянный свет неба ---
	if (!Has(ASkyLight::StaticClass()))
	{
		ASkyLight* Sky = World->SpawnActor<ASkyLight>(ASkyLight::StaticClass(), FTransform::Identity);
		if (Sky)
		{
			USkyLightComponent* LC = Sky->GetLightComponent();
			LC->SetMobility(EComponentMobility::Movable);
			LC->SourceType = ESkyLightSourceType::SLS_CapturedScene;
			LC->bRealTimeCapture = true;
			LC->SetIntensity(1.6f);
			LC->RecaptureSky();
		}
	}

	// --- Домашняя планета (на ней появляется игрок и стоит база) ---
	// Сцену собираем детерминированно: убираем прежние тела (карта могла
	// содержать планету из редактора) и создаём ровно один набор.
	{
		TArray<AActor*> Old;
		for (TActorIterator<AOUVoxelPlanet> It(World); It; ++It) { Old.Add(*It); }
		for (TActorIterator<AOUSolarBody> It(World); It; ++It) { Old.Add(*It); }
		for (TActorIterator<AOUSurfaceBase> It(World); It; ++It) { Old.Add(*It); }
		// Старые корабли/персонажи с карты могли сохраниться и перехватывать
		// управление (у них мог быть AutoPossessPlayer=Player0) — убираем их.
		// ВАЖНО: не трогаем пешку, которой уже управляет игрок (локальный
		// игрок входит в мир до StartPlay, и его персонаж уже существует).
		auto IsPlayerControlled = [](AActor* A) -> bool
		{
			if (const APawn* P = Cast<APawn>(A))
			{
				return P->GetController() != nullptr;
			}
			return false;
		};
		for (TActorIterator<AOUShipPawn> It(World); It; ++It)
		{
			if (!IsPlayerControlled(*It)) { Old.Add(*It); }
		}
		for (TActorIterator<AOUCharacterPawn> It(World); It; ++It)
		{
			if (!IsPlayerControlled(*It)) { Old.Add(*It); }
		}
		for (TActorIterator<APlayerStart> It(World); It; ++It) { Old.Add(*It); }
		for (AActor* A : Old) { if (A) { A->Destroy(); } }
	}

	AOUVoxelPlanet* Home = World->SpawnActor<AOUVoxelPlanet>(
		AOUVoxelPlanet::StaticClass(),
		FTransform(FRotator::ZeroRotator, GPlanetOffset));
	if (Home)
	{
		Home->PlanetType = EOUPlanetType::Terran;
		Home->bHomePlanet = true;
		Home->Radius = 10000000.f;     // 100 км в UE-сантиметрах (как на сервере)
		Home->Amplitude = 200000.f;    // 2 км рельефа
		Home->BuildPlanet();
	}

	// --- Солнечная система: солнце и ещё три разные планеты ---
	// Раскладываем их далеко в НЕБЕ над игроком, чтобы в кадре читалась
	// система, а не «одна планета перед камерой». Это декоративные тела:
	// на них не садятся (домашняя — в центре, на ней стоит игрок).
	{
		// Базис «неба»: Up — от центра планеты к точке появления (UE-оси).
		FVector Up = AOUVoxelPlanet::HomeSpawnDirection();   // единичный, UE
		if (Up.IsNearlyZero())
		{
			Up = FVector::UpVector;
		}
		FVector T1 = FVector::CrossProduct(Up, FVector::UpVector);
		if (T1.SizeSquared() < 1e-4f)
		{
			T1 = FVector::CrossProduct(Up, FVector::RightVector);
		}
		T1 = T1.GetSafeNormal();
		const FVector T2 = FVector::CrossProduct(Up, T1).GetSafeNormal();

		// Направление «вперёд» игрока у поверхности: та же касательная, что
		// строит корабль (forward по умолчанию — мировой +X, спроецированный
		// на плоскость поверхности). Вокруг неё и раскладываем планеты.
		FVector Fwd = FVector::VectorPlaneProject(FVector(1.f, 0.f, 0.f), Up);
		if (Fwd.SizeSquared() < 1e-4f)
		{
			Fwd = T1;
		}
		Fwd = Fwd.GetSafeNormal();
		// Базис вокруг направления взгляда: B — «вправо» в плоскости горизонта.
		const FVector B = FVector::CrossProduct(Up, Fwd).GetSafeNormal();
		UE_LOG(LogOUGameMode, Log, TEXT("Взгляд игрока: fwd=(%.2f,%.2f,%.2f) up=(%.2f,%.2f,%.2f)"),
			Fwd.X, Fwd.Y, Fwd.Z, Up.X, Up.Y, Up.Z);

		// Планета на горизонте: горизонт — это касательная плоскость, поэтому
		// берём направление чуть выше неё (Elev) под углом Azim к взгляду.
		auto SkyAt = [&](float AzimDeg, float ElevDeg, float Dist) -> FVector
		{
			const float A = FMath::DegreesToRadians(AzimDeg);
			const float E = FMath::DegreesToRadians(ElevDeg);
			const FVector Horiz = (Fwd * FMath::Cos(A) + B * FMath::Sin(A)).GetSafeNormal();
			return (Horiz * FMath::Cos(E) + Up * FMath::Sin(E)).GetSafeNormal() * Dist;
		};

		// Солнце: крупная звезда высоко в небе.
		if (!Has(AOUSolarBody::StaticClass()))
		{
			AOUSolarBody* Star = World->SpawnActor<AOUSolarBody>(
				AOUSolarBody::StaticClass(),
				FTransform(FRotator::ZeroRotator, SkyAt(-25.f, 32.f, 90000000.f)));   // 900 км от центра
			if (Star)
			{
				Star->Kind = EOUSolarKind::Star;
				Star->BodyRadiusCm = 6000000.f;   // 60 км — крупное светило вдали
				Star->BodyColor = FLinearColor(1.0f, 0.86f, 0.35f);
				Star->BuildBody();
			}
		}

		// Четыре планеты системы: разные типы, у части есть атмосфера и облака,
		// у части — нет (пустынная с пыльной дымкой и безвоздушная луна).
		// Дистанции — сотни километров: за пределами домашней планеты (100 км).
		struct FPlanetDef { EOUPlanetType Type; float Azim; float Elev; float DistCm; float RadiusCm; };
		const FPlanetDef Defs[] = {
			{ EOUPlanetType::Desert,   18.f, 20.f, 50000000.f,  4000000.f },  // 500 км, R 40 км (пыль, без облаков)
			{ EOUPlanetType::Ice,      52.f, 14.f, 80000000.f,  3000000.f },  // 800 км, R 30 км (лёд + облака)
			{ EOUPlanetType::GasGiant, -48.f, 24.f, 150000000.f, 9000000.f }, // 1.5 Мкм, R 90 км (полосы)
			{ EOUPlanetType::Volcanic, 110.f, 18.f, 100000000.f, 3500000.f }, // 1.0 Мкм, R 35 км (пепел)
			{ EOUPlanetType::Airless, -120.f, 12.f, 60000000.f, 2000000.f },  // 600 км, R 20 км (луна без атмосферы)
		};
		for (const FPlanetDef& D : Defs)
		{
			const FVector Pos = SkyAt(D.Azim, D.Elev, D.DistCm);
			AOUVoxelPlanet* P = World->SpawnActor<AOUVoxelPlanet>(
				AOUVoxelPlanet::StaticClass(),
				FTransform(FRotator::ZeroRotator, Pos));
			if (P)
			{
				P->PlanetType = D.Type;
				P->bHomePlanet = false;
				P->Radius = D.RadiusCm;
				P->Amplitude = D.RadiusCm * 0.06f;
				P->Lod = EOUVoxelLod::Mid;   // дальние планеты — детализация ниже
				P->BuildPlanet();
			}
		}
	}

	// --- База на поверхности, в точке появления игрока ---
	if (!Has(AOUSurfaceBase::StaticClass()))
	{
		// Точка появления — на суше; «верх» базы направляем от центра планеты,
		// чтобы постройки стояли вертикально относительно поверхности.
		const FVector SpawnUE = AOUVoxelPlanet::HomeSpawn(3.0f);
		const FVector Up = SpawnUE.GetSafeNormal();
		const FVector Fwd = FVector::CrossProduct(Up, FVector::RightVector).GetSafeNormal();
		const FRotator Rot = FRotationMatrix::MakeFromXZ(Fwd, Up).Rotator();
		World->SpawnActor<AOUSurfaceBase>(
			AOUSurfaceBase::StaticClass(),
			FTransform(Rot, SpawnUE));
		UE_LOG(LogOUGameMode, Log, TEXT("База размещена в точке появления: %s"), *SpawnUE.ToString());
	}

	// --- Менеджер начала координат (floating origin) ---
	if (!Has(AUOOriginManager::StaticClass()))
	{
		World->SpawnActor<AUOOriginManager>(AUOOriginManager::StaticClass(), FTransform::Identity);
	}

	UE_LOG(LogOUGameMode, Log, TEXT("Сцена Open Universe собрана: свет, небо, планета, солнечная система, база, floating origin"));
}

void AOUGameMode::BuildSystemFromServer()
{
	UWorld* World = GetWorld();
	if (!World)
	{
		return;
	}
	// Находим сетевой компонент (он живёт на корабле игрока).
	UOUClientNet* Net = nullptr;
	for (TActorIterator<APawn> It(World); It; ++It)
	{
		if (UOUClientNet* Found = It->FindComponentByClass<UOUClientNet>())
		{
			Net = Found;
			break;
		}
	}
	if (!Net || !Net->bSystemReceived || Net->SystemBodies.Num() == 0)
	{
		return;
	}

	// Убираем прежние тела системы (солнце/декоративные планеты), чтобы не
	// смешивать серверные данные с прежней «нарисованной» системой.
	{
		TArray<AActor*> Old;
		for (TActorIterator<AOUSolarBody> It(World); It; ++It) { Old.Add(*It); }
		for (TActorIterator<AOUVoxelPlanet> It(World); It; ++It)
		{
			if (!It->bHomePlanet) { Old.Add(*It); }
		}
		for (AActor* A : Old) { if (A) { A->Destroy(); } }
	}

	for (const FOUSystemBody& B : Net->SystemBodies)
	{
		// Позиция в серверных осях → UE (с масштабом).
		const FVector PosUE = OUInterop::ServerToUE(B.Position);
		const float RadiusCm = static_cast<float>(B.Radius * OUInterop::ServerUnitToCm);

		if (B.Kind == 0 || B.Kind == 5)   // звезда или чёрная дыра
		{
			AOUSolarBody* Body = World->SpawnActor<AOUSolarBody>(
				AOUSolarBody::StaticClass(), FTransform(FRotator::ZeroRotator, PosUE));
			if (Body)
			{
				Body->Kind = EOUSolarKind::Star;
				Body->BodyRadiusCm = FMath::Max(RadiusCm, 100000.f);
				Body->BodyColor = B.Kind == 5
					? FLinearColor(0.08f, 0.05f, 0.12f)      // чёрная дыра
					: FLinearColor(1.0f, 0.86f, 0.35f);      // звезда
				Body->BuildBody();
			}
		}
		else if (B.Kind == 1 || B.Kind == 3 || B.Kind == 2)   // планета/газ/луна
		{
			if (RadiusCm < 10000.f) { continue; }   // слишком мелкое — не рисуем
			AOUVoxelPlanet* Planet = World->SpawnActor<AOUVoxelPlanet>(
				AOUVoxelPlanet::StaticClass(), FTransform(FRotator::ZeroRotator, PosUE));
			if (Planet)
			{
				Planet->bHomePlanet = false;
				Planet->Radius = RadiusCm;
				Planet->Amplitude = RadiusCm * 0.05f;
				Planet->PlanetType = (B.Kind == 3) ? EOUPlanetType::GasGiant
					: (B.Id % 2 == 0 ? EOUPlanetType::Ice : EOUPlanetType::Desert);
				Planet->Lod = EOUVoxelLod::Mid;
				Planet->BuildPlanet();
			}
		}
	}
	UE_LOG(LogOUGameMode, Log, TEXT("Система от сервера построена: тел %d"), Net->SystemBodies.Num());
}
