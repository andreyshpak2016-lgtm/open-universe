// Copyright Open Universe. Все права защищены.

#include "OUGameMode.h"
#include "OUShipPawn.h"
#include "OUOriginManager.h"
#include "OUVoxelPlanet.h"

#include "Engine/World.h"
#include "Engine/DirectionalLight.h"
#include "Engine/SkyLight.h"
#include "Engine/ExponentialHeightFog.h"
#include "GameFramework/PlayerStart.h"
#include "Components/DirectionalLightComponent.h"
#include "Components/SkyLightComponent.h"
#include "Components/SkyAtmosphereComponent.h"
#include "EngineUtils.h"
#include "Kismet/GameplayStatics.h"

DEFINE_LOG_CATEGORY_STATIC(LogOUGameMode, Log, All);

namespace
{
	/**
	 * Планета ставится ниже нуля, чтобы начало координат (0,0,0) оказалось
	 * над поверхностью — иначе вид по умолчанию был бы «внутри» планеты.
	 */
	const FVector GPlanetOffset(0.f, 0.f, -130000.f);
}

AOUGameMode::AOUGameMode()
{
	// Игрок управляет кораблём; мир — общий, начало координат подвижное.
	DefaultPawnClass = AOUShipPawn::StaticClass();
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
}

AActor* AOUGameMode::ChoosePlayerStart_Implementation(AController* Player)
{
	// На пустой карте PlayerStart отсутствует — тогда UE не создаёт Pawn и
	// камеры нет (тот самый чёрный экран). Создаём точку старта сами.
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
	APlayerStart* Start = World->SpawnActor<APlayerStart>(
		APlayerStart::StaticClass(),
		FTransform(FRotator(-20.f, 0.f, 0.f), FVector::ZeroVector));
	UE_LOG(LogOUGameMode, Log, TEXT("Создана точка старта PlayerStart"));
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

	// --- Солнце ---
	if (!Has(ADirectionalLight::StaticClass()))
	{
		const FTransform SunT(FRotator(-45.f, -35.f, 0.f), FVector(0.f, 0.f, 200000.f));
		ADirectionalLight* Sun = World->SpawnActor<ADirectionalLight>(ADirectionalLight::StaticClass(), SunT);
		if (Sun)
		{
			Sun->GetLightComponent()->SetMobility(EComponentMobility::Movable);
			Sun->GetLightComponent()->SetIntensity(4.f);
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
			LC->SetIntensity(1.f);
			LC->RecaptureSky();
		}
	}

	// --- Атмосферный туман ---
	if (!Has(AExponentialHeightFog::StaticClass()))
	{
		World->SpawnActor<AExponentialHeightFog>(AExponentialHeightFog::StaticClass(), FTransform::Identity);
	}

	// --- Планета ---
	if (!Has(AOUVoxelPlanet::StaticClass()))
	{
		AOUVoxelPlanet* Planet = World->SpawnActor<AOUVoxelPlanet>(
			AOUVoxelPlanet::StaticClass(),
			FTransform(FRotator::ZeroRotator, GPlanetOffset));
		if (Planet)
		{
			Planet->BuildPlanet();
		}
	}

	// --- Менеджер начала координат (floating origin) ---
	if (!Has(AUOOriginManager::StaticClass()))
	{
		World->SpawnActor<AUOOriginManager>(AUOOriginManager::StaticClass(), FTransform::Identity);
	}

	UE_LOG(LogOUGameMode, Log, TEXT("Сцена Open Universe собрана: свет, небо, туман, планета, floating origin"));
}
