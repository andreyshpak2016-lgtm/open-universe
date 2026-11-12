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
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "TimerManager.h"

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
	// Тик нужен для служебных задач GameMode (сейчас их нет, но тик включён
	// на случай динамики сцены).
	PrimaryActorTick.bCanEverTick = false;
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

	// Тестовый отложенный снимок (`-OULateShot`): кадр после закрытия меню и
	// авто-подключения. Нужен для проверок в режиме без окна, где ручной снимок
	// экрана невозможен.
	if (FParse::Param(FCommandLine::Get(), TEXT("OULateShot")))
	{
		UWorld* World = GetWorld();
		if (World)
		{
			FTimerHandle Handle;
			World->GetTimerManager().SetTimer(Handle, [this]()
			{
				RequestShot(TEXT("late"));
			}, 25.0f, false);
			UE_LOG(LogOUGameMode, Log, TEXT("Запланирован отложенный снимок (25 с)"));
		}
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
	// ВАЖНО: SkyAtmosphere по умолчанию рассчитан на планету радиусом 6360 км
	// (Земля). Наша планета — 500 км, поэтому дефолтная атмосфера строилась бы
	// «внутри» неё: игрок оказывался глубоко под уровнем атмосферного слоя, и
	// его граница была видна как косой клин в небе (казалось, что горизонт
	// наклонён). Привязываем атмосферу к ЦЕНТРУ планеты (мировое начало — центр
	// планеты, см. `GPlanetOffset`) и задаём её радиус в километрах.
	const bool bNoSky = FParse::Param(FCommandLine::Get(), TEXT("OUNoSky"));
	constexpr float PlanetRadiusKm = 500.0f;    // совпадает с радиусом планеты
	constexpr float AtmosphereHeightKm = 20.0f; // тонкий слой над поверхностью

	auto ConfigureSky = [](ASkyAtmosphere* Sky)
	{
		if (!Sky)
		{
			return;
		}
		if (USkyAtmosphereComponent* Comp = Sky->GetComponent())
		{
			// Центр планеты — в трансформе компонента (actor стоит в начале
			// координат, а это и есть центр нашей планеты).
			Comp->TransformMode = ESkyAtmosphereTransformMode::PlanetCenterAtComponentTransform;
			Comp->SetBottomRadius(PlanetRadiusKm);
			Comp->SetAtmosphereHeight(AtmosphereHeightKm);
		}
	};

	if (!bNoSky)
	{
		ASkyAtmosphere* Sky = Cast<ASkyAtmosphere>(
			UGameplayStatics::GetActorOfClass(World, ASkyAtmosphere::StaticClass()));
		if (!Sky)
		{
			Sky = World->SpawnActor<ASkyAtmosphere>(
				ASkyAtmosphere::StaticClass(), FTransform::Identity);
		}
		ConfigureSky(Sky);
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
		Home->Radius = 50000000.f;     // 500 км в UE-сантиметрах (как на сервере)
		Home->Amplitude = 1000000.f;   // 10 км рельефа
		Home->BuildPlanet();
	}

	// --- Других тел системы нет: в мире ровно одна планета ---
	// Раньше здесь строились солнце и ещё четыре планеты (декоративные тела
	// в небе). По решению проекта система сведена к одной большой планете,
	// поэтому небесные тела не создаются и не рассылаются.

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

	UE_LOG(LogOUGameMode, Log, TEXT("Сцена Open Universe собрана: свет, небо, одна планета, база, floating origin"));
}

