// Copyright Open Universe. Все права защищены.

#include "OUCharacterPawn.h"
#include "OUShipPawn.h"
#include "OUClientNet.h"
#include "OUVoxelPlanet.h"
#include "OUSurfaceBase.h"
#include "OUPlayerMenuController.h"
#include "OUInterop.h"
#include "Components/CapsuleComponent.h"
#include "Components/StaticMeshComponent.h"
#include "Components/SceneComponent.h"
#include "Camera/CameraComponent.h"
#include "Components/InputComponent.h"
#include "Components/PointLightComponent.h"
#include "Engine/StaticMesh.h"
#include "Engine/OverlapResult.h"
#include "Materials/MaterialInterface.h"
#include "Kismet/GameplayStatics.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "OUProjectile.h"
#include "OUBuildBlock.h"
#include "OUBuildGrid.h"
#include "UObject/ConstructorHelpers.h"

DEFINE_LOG_CATEGORY_STATIC(LogOUChar, Log, All);

// Цвета фигурки (синий костюм — чтобы персонаж читался в кадре).
namespace
{
	const FLinearColor ColSuit(0.20f, 0.45f, 0.95f);   // костюм: торс и руки
	const FLinearColor ColSkin(0.95f, 0.78f, 0.60f);   // голова
	const FLinearColor ColBoots(0.16f, 0.18f, 0.26f);  // ноги/ботинки
}

AOUCharacterPawn::AOUCharacterPawn()
{
	PrimaryActorTick.bCanEverTick = true;

	Capsule = CreateDefaultSubobject<UCapsuleComponent>(TEXT("Capsule"));
	Capsule->InitCapsuleSize(CapsuleRadius, CapsuleHalfHeight);
	// Капсула — твёрдая: персонаж не проходит сквозь постройки и стоит на них.
	// Рельеф планеты ведёт серверная формула (аналитическая привязка ниже),
	// но здания базы и блоки редактора — обычные меши, и без коллизии игрок
	// «проваливается» через них. Включаем блокировку и свипаем движение.
	Capsule->SetCollisionEnabled(ECollisionEnabled::QueryOnly);
	Capsule->SetCollisionObjectType(ECC_Pawn);
	Capsule->SetCollisionResponseToAllChannels(ECR_Block);
	Capsule->SetGenerateOverlapEvents(false);
	SetRootComponent(Capsule);

	// Базовые меши движка и материал (параметрический цвет).
	static ConstructorHelpers::FObjectFinder<UStaticMesh> Cyl(TEXT("/Engine/BasicShapes/Cylinder.Cylinder"));
	static ConstructorHelpers::FObjectFinder<UStaticMesh> Sph(TEXT("/Engine/BasicShapes/Sphere.Sphere"));
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> Mat(
		TEXT("/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"));

	// --- Шарниры (пустые точки, от которых качаются конечности) ---
	ShoulderL = CreateDefaultSubobject<USceneComponent>(TEXT("ShoulderL"));
	ShoulderL->SetupAttachment(Capsule);
	ShoulderL->SetRelativeLocation(FVector(0.f, -28.f, 34.f));
	ShoulderR = CreateDefaultSubobject<USceneComponent>(TEXT("ShoulderR"));
	ShoulderR->SetupAttachment(Capsule);
	ShoulderR->SetRelativeLocation(FVector(0.f, 28.f, 34.f));
	HipL = CreateDefaultSubobject<USceneComponent>(TEXT("HipL"));
	HipL->SetupAttachment(Capsule);
	HipL->SetRelativeLocation(FVector(0.f, -14.f, -20.f));
	HipR = CreateDefaultSubobject<USceneComponent>(TEXT("HipR"));
	HipR->SetupAttachment(Capsule);
	HipR->SetRelativeLocation(FVector(0.f, 14.f, -20.f));

	// --- Торс (корпус) ---
	BodyMesh = CreateDefaultSubobject<UStaticMeshComponent>(TEXT("BodyMesh"));
	BodyMesh->SetupAttachment(Capsule);
	BodyMesh->SetRelativeLocation(FVector(0.f, 0.f, 10.f));
	BodyMesh->SetRelativeScale3D(FVector(0.5f, 0.5f, 0.7f));

	// --- Голова ---
	HeadMesh = CreateDefaultSubobject<UStaticMeshComponent>(TEXT("HeadMesh"));
	HeadMesh->SetupAttachment(Capsule);
	HeadMesh->SetRelativeLocation(FVector(0.f, 0.f, 72.f));
	HeadMesh->SetRelativeScale3D(FVector(0.36f));

	// --- Руки (висят ВНИЗ от плеч: меш сдвинут на -полувысоту под шарниром) ---
	ArmL = CreateDefaultSubobject<UStaticMeshComponent>(TEXT("ArmL"));
	ArmL->SetupAttachment(ShoulderL);
	ArmL->SetRelativeLocation(FVector(0.f, 0.f, -30.f));
	ArmL->SetRelativeScale3D(FVector(0.16f, 0.16f, 0.6f));
	ArmR = CreateDefaultSubobject<UStaticMeshComponent>(TEXT("ArmR"));
	ArmR->SetupAttachment(ShoulderR);
	ArmR->SetRelativeLocation(FVector(0.f, 0.f, -30.f));
	ArmR->SetRelativeScale3D(FVector(0.16f, 0.16f, 0.6f));

	// --- Ноги (висят ВНИЗ от бёдер) ---
	LegL = CreateDefaultSubobject<UStaticMeshComponent>(TEXT("LegL"));
	LegL->SetupAttachment(HipL);
	LegL->SetRelativeLocation(FVector(0.f, 0.f, -35.f));
	LegL->SetRelativeScale3D(FVector(0.2f, 0.2f, 0.7f));
	LegR = CreateDefaultSubobject<UStaticMeshComponent>(TEXT("LegR"));
	LegR->SetupAttachment(HipR);
	LegR->SetRelativeLocation(FVector(0.f, 0.f, -35.f));
	LegR->SetRelativeScale3D(FVector(0.2f, 0.2f, 0.7f));

	// Меши и материал для всех частей фигурки.
	if (Cyl.Succeeded())
	{
		BodyMesh->SetStaticMesh(Cyl.Object);
		ArmL->SetStaticMesh(Cyl.Object);
		ArmR->SetStaticMesh(Cyl.Object);
		LegL->SetStaticMesh(Cyl.Object);
		LegR->SetStaticMesh(Cyl.Object);
	}
	if (Sph.Succeeded())
	{
		HeadMesh->SetStaticMesh(Sph.Object);
	}
	auto Paint = [&](UStaticMeshComponent* Part, const FLinearColor& Color)
	{
		if (!Part)
		{
			return;
		}
		Part->SetCollisionEnabled(ECollisionEnabled::NoCollision);
		if (Mat.Succeeded())
		{
			Part->SetMaterial(0, Mat.Object);
		}
		Part->SetVectorParameterValueOnMaterials(TEXT("Color"), FVector(Color.R, Color.G, Color.B));
	};
	Paint(BodyMesh, ColSuit);
	Paint(HeadMesh, ColSkin);
	Paint(ArmL, ColSuit);
	Paint(ArmR, ColSuit);
	Paint(LegL, ColBoots);
	Paint(LegR, ColBoots);

	// --- Ручное оружие (в правой руке, смотрит вперёд) ---
	// Крепим к капсуле (не к качающейся руке), иначе на бегу оружие «уезжает»
	// из прицела. Позиция — у правой кисти, ориентация — вперёд по курсу.
	{
		static ConstructorHelpers::FObjectFinder<UStaticMesh> GunShape(
			TEXT("/Engine/BasicShapes/Cube.Cube"));
		GunMesh = CreateDefaultSubobject<UStaticMeshComponent>(TEXT("GunMesh"));
		GunMesh->SetupAttachment(Capsule);
		GunMesh->SetRelativeLocation(FVector(40.f, 34.f, 2.f));
		GunMesh->SetRelativeScale3D(FVector(0.85f, 0.16f, 0.16f));
		GunMesh->SetCollisionEnabled(ECollisionEnabled::NoCollision);
		if (GunShape.Succeeded())
		{
			GunMesh->SetStaticMesh(GunShape.Object);
		}
		if (Mat.Succeeded())
		{
			GunMesh->SetMaterial(0, Mat.Object);
		}
		GunMesh->SetVectorParameterValueOnMaterials(TEXT("Color"), FVector(0.15f, 0.16f, 0.2f));
	}

	// Вспышка у дула: точечный свет, загорается на кадр выстрела.
	MuzzleFlash = CreateDefaultSubobject<UPointLightComponent>(TEXT("MuzzleFlash"));
	MuzzleFlash->SetupAttachment(GunMesh);
	MuzzleFlash->SetRelativeLocation(FVector(60.f, 0.f, 0.f));
	MuzzleFlash->SetIntensity(6000.f);
	MuzzleFlash->SetLightColor(FLinearColor(1.f, 0.85f, 0.35f));
	MuzzleFlash->SetAttenuationRadius(400.f);
	MuzzleFlash->SetVisibility(false);

	// --- Призрак блока строительства (куда встанет следующий кубик) ---
	{
		static ConstructorHelpers::FObjectFinder<UStaticMesh> GhostCube(
			TEXT("/Engine/BasicShapes/Cube.Cube"));
		// Полупрозрачный материал — призрак виден как «превью», а не как блок.
		static ConstructorHelpers::FObjectFinder<UMaterialInterface> GhostMat(
			TEXT("/Engine/EngineDebugMaterials/M_SimpleUnlitTranslucent.M_SimpleUnlitTranslucent"));
		BuildGhost = CreateDefaultSubobject<UStaticMeshComponent>(TEXT("BuildGhost"));
		BuildGhost->SetupAttachment(Capsule);
		BuildGhost->SetCollisionEnabled(ECollisionEnabled::NoCollision);
		BuildGhost->SetCastShadow(false);
		BuildGhost->SetGenerateOverlapEvents(false);
		if (GhostCube.Succeeded())
		{
			BuildGhost->SetStaticMesh(GhostCube.Object);
		}
		if (GhostMat.Succeeded())
		{
			BuildGhost->SetMaterial(0, GhostMat.Object);
		}
		else if (Mat.Succeeded())
		{
			BuildGhost->SetMaterial(0, Mat.Object);
		}
		BuildGhost->SetVisibility(false);
	}

	// Камера — ВИД ОТ ПЕРВОГО ЛИЦА: камера прямо в голове, без штанги.
	// Тело фигурки своему игроку не показываем (иначе оно закрывало бы обзор),
	// но чужой игрок видит фигурку (клиент других игроков рисует её отдельно).
	Camera = CreateDefaultSubobject<UCameraComponent>(TEXT("Camera"));
	Camera->SetupAttachment(Capsule);
	Camera->SetRelativeLocation(FVector(18.f, 0.f, CapsuleHalfHeight * 0.85f));  // у глаз
	Camera->bUsePawnControlRotation = false;
	Camera->SetFieldOfView(CameraFov);   // угол обзора настраивается в меню

	// Свою фигурку и оружие владельцу не показываем — вид от первого лица.
	// (Чужие игроки видят фигурку: её рисует их собственный клиент отдельно.)
	auto HideFromOwner = [](UPrimitiveComponent* Part)
	{
		if (Part) { Part->SetOwnerNoSee(true); }
	};
	HideFromOwner(BodyMesh);
	HideFromOwner(HeadMesh);
	HideFromOwner(ArmL);
	HideFromOwner(ArmR);
	HideFromOwner(LegL);
	HideFromOwner(LegR);
	HideFromOwner(GunMesh);

	AutoPossessPlayer = EAutoReceiveInput::Disabled;
}

void AOUCharacterPawn::BeginPlay()
{
	Super::BeginPlay();

	UWorld* World = GetWorld();
	if (!World)
	{
		return;
	}

	// Тестовый автоход (для снимков анимации бега без ручного ввода).
	bAutoWalk = FParse::Param(FCommandLine::Get(), TEXT("OUAutoWalk"));
	// Тестовый автоматический огонь (для снимков оружия).
	bAutoFire = FParse::Param(FCommandLine::Get(), TEXT("OUAutoFire"));
	// Тестовое строительство (для снимка меню строительства и блока).
	bAutoBuild = FParse::Param(FCommandLine::Get(), TEXT("OUAutoBuild"));

	// Цвета фигурки задаём здесь (после регистрации компонентов — в конструкторе
	// параметры материала ещё не применяются).
	auto Paint = [](UStaticMeshComponent* Part, const FLinearColor& Color)
	{
		if (Part)
		{
			Part->SetVectorParameterValueOnMaterials(TEXT("Color"), FVector(Color.R, Color.G, Color.B));
		}
	};
	Paint(BodyMesh, ColSuit);
	Paint(HeadMesh, ColSkin);
	Paint(ArmL, ColSuit);
	Paint(ArmR, ColSuit);
	Paint(LegL, ColBoots);
	Paint(LegR, ColBoots);

	// Опора: точка появления на суше домашней планеты (как у сервера).
	const FVector BaseSpawn = AOUVoxelPlanet::HomeSpawn(2.0f);
	const FVector Up = BaseSpawn.GetSafeNormal();

	// Тангенциальный базис на поверхности: Fwd — «от базы наружу»,
	// Side — «вправо» от направления взгляда.
	FVector Fwd = FVector::VectorPlaneProject(FVector(1.f, 0.f, 0.f), Up).GetSafeNormal();
	if (Fwd.IsNearlyZero())
	{
		Fwd = FVector::VectorPlaneProject(FVector::ForwardVector, Up).GetSafeNormal();
	}
	FVector Side = FVector::CrossProduct(Up, Fwd).GetSafeNormal();

	// Персонаж — на посадочном поле, левее центральной разметки; корабль рядом.
	// Комплекс крупный (поле ~180 м), поэтому ставим игрока к его центру, а не
	// «наружу» — так он стоит на ровных плитах поля, а не на склоне.
	FVector CharPos = BaseSpawn - Fwd * 1800.f;
	// Корабль — рядом, на плите поля.
	FVector ShipPos = CharPos + Fwd * 900.f + Side * 700.f;
	if (AOUVoxelPlanet* Planet = AOUVoxelPlanet::FindHomePlanet(World))
	{
		// Плиты площадки лежат на земле (только визуал), поэтому стоим на
		// аналитической поверхности планеты.
		CharPos = CharPos.GetSafeNormal() * (Planet->SurfaceRadiusAt(CharPos.GetSafeNormal()) + 95.f);
		ShipPos = ShipPos.GetSafeNormal() * (Planet->SurfaceRadiusAt(ShipPos.GetSafeNormal()) + 140.f);
	}

	// Курс персонажа — «наружу» от базы (по Fwd).
	float LookYaw = FMath::Atan2(Fwd.Y, Fwd.X);
	// Диагностика: `-OUBaseOverview` — обзор комплекса сверху.
	const bool bOverview = FParse::Param(FCommandLine::Get(), TEXT("OUBaseOverview"));
	if (bOverview)
	{
		// Ставим ровно над центром базы и поднимаем камеру.
		const FVector Center = AOUVoxelPlanet::HomeSpawn(2.0f);
		CharPos = Center.GetSafeNormal() * (Center.Size() + 14000.f);
	}
	// Диагностика: `-OUBaseSide` — стоим ВНЕ террас и смотрим НА базу, чтобы
	// увидеть ступени сбоку.
	if (FParse::Param(FCommandLine::Get(), TEXT("OUBaseSide")))
	{
		FVector Center, UpB, T1, T2;
		AOUVoxelPlanet::PadFrame(Center, UpB, T1, T2);
		const float Dist = AOUVoxelPlanet::PadOuterRadiusCm + 2500.f;
		FVector Dir = (UpB * static_cast<float>(AOUVoxelPlanet::PadHomeRadiusCm) + T2 * Dist).GetSafeNormal();
		CharPos = Dir * (AOUVoxelPlanet::PadHomeRadiusCm + 300.f);
		LookYaw = FMath::Atan2(-T2.Y, -T2.X);   // смотрим внутрь, на базу
	}
	// Диагностика: `-OUDigOutside` — ставим игрока ВНЕ площадки базы (на чистый
	// рельеф), чтобы копание было видно (внутри площадки яма гасится ровным полом).
	if (FParse::Param(FCommandLine::Get(), TEXT("OUDigOutside")))
	{
		FVector Center, UpB, T1, T2;
		AOUVoxelPlanet::PadFrame(Center, UpB, T1, T2);
		const float Dist = AOUVoxelPlanet::PadOuterRadiusCm + 6000.f;
		FVector Dir = (UpB * static_cast<float>(AOUVoxelPlanet::PadHomeRadiusCm) + T2 * Dist).GetSafeNormal();
		CharPos = Dir * (AOUVoxelPlanet::PadHomeRadiusCm + 300.f);
		LookYaw = FMath::Atan2(-T2.Y, -T2.X);
	}
	PlaceAt(CharPos, LookYaw);
	// Якорь сетки строительства — в нескольких метрах ПЕРЕД точкой появления
	// (после привязки к поверхности). Точка детерминирована (её же видит любой
	// клиент), поэтому один серверный воксель кладётся в одно место у всех.
	{
		const FVector Spawn = GetActorLocation();
		const FVector UpS = Spawn.GetSafeNormal();
		FVector FwdS = FVector::VectorPlaneProject(FVector(1.f, 0.f, 0.f), UpS).GetSafeNormal();
		if (FwdS.IsNearlyZero())
		{
			FwdS = FVector::VectorPlaneProject(FVector::ForwardVector, UpS).GetSafeNormal();
		}
		// +5 вокселей вперёд (7.5 м) — стройка начинается перед игроком, а не в ногах.
		BuildAnchorUE = Spawn + FwdS * 750.f;
	}
	// Смотрим чуть вверх: в кадр попадают и земля, и небо с облаками.
	ViewPitch = 0.10f;
	// Диагностика: `-OULookDown` смотрит вниз — видно рельеф у ног.
	if (FParse::Param(FCommandLine::Get(), TEXT("OULookDown")))
	{
		ViewPitch = -0.55f;
	}
	if (bOverview)
	{
		ViewPitch = -1.15f;
	}

	// Корабль стоит рядом (между базой и персонажем).
	if (!Ship)
	{
		Ship = World->SpawnActor<AOUShipPawn>(
			AOUShipPawn::StaticClass(), FTransform(FRotator::ZeroRotator, ShipPos));
		if (Ship)
		{
			Ship->Character = this;
		}
	}

	// Плотный локальный участок вокруг игрока строим СРАЗУ: без него у ног
	// видна только крупная форма рельефа (глобальный меш грубый), и земля
	// выглядит плоской.
	if (AOUVoxelPlanet* Planet = AOUVoxelPlanet::FindHomePlanet(World))
	{
		LocalTerrainCenter = CharPos;
		Planet->RebuildLocal(CharPos);
	}

	// Диагностика/тесты: `-OUStartInShip` сразу сажает игрока в корабль (удобно
	// проверять управление кораблём без ручных нажатий).
	if (Ship && FParse::Param(FCommandLine::Get(), TEXT("OUStartInShip")))
	{
		Ship->EnterShip(this);
	}

	UE_LOG(LogOUChar, Log, TEXT("Персонаж создан: ходьба по поверхности, корабль рядом"));
}

float AOUCharacterPawn::SurfaceRadius(const FVector& DirUE) const
{
	if (AOUVoxelPlanet* Planet = AOUVoxelPlanet::FindHomePlanet(GetWorld()))
	{
		return Planet->SurfaceRadiusAt(DirUE);   // см UE
	}
	// Запасной вариант: радиус планеты в UE-сантиметрах.
	return 50000000.f;
}

void AOUCharacterPawn::PlaceAt(const FVector& WorldLocation, float LookYawWorld)
{
	SetActorLocation(WorldLocation);
	Velocity = FVector::ZeroVector;
	ViewPitch = 0.f;

	// ViewYaw хранится как угол в КАСАТЕЛЬНОЙ плоскости (вокруг радиуса), а не
	// мировой. Переведём желаемое мировое направление взгляда в этот базис:
	// так камера смотрит туда, куда нужно, а не в случайную сторону.
	const FVector Up = WorldLocation.GetSafeNormal();
	if (!Up.IsNearlyZero())
	{
		FVector Ref = FVector::CrossProduct(Up, FVector::UpVector);
		if (Ref.SizeSquared() < 1e-4f)
		{
			Ref = FVector::CrossProduct(Up, FVector::RightVector);
		}
		Ref = Ref.GetSafeNormal();
		const FVector Tang = FVector::CrossProduct(Ref, Up).GetSafeNormal();      // «вперёд» при yaw=0
		const FVector Bin = FVector::CrossProduct(Up, Tang).GetSafeNormal();      // «вправо»
		const FVector Wish = FVector(FMath::Cos(LookYawWorld), FMath::Sin(LookYawWorld), 0.f).GetSafeNormal();
		ViewYaw = FMath::Atan2(FVector::DotProduct(Wish, Bin), FVector::DotProduct(Wish, Tang));
	}
	else
	{
		ViewYaw = LookYawWorld;
	}
	UE_LOG(LogOUChar, Log, TEXT("Персонаж на позиции (%.0f, %.0f, %.0f), курс %.2f"),
		WorldLocation.X, WorldLocation.Y, WorldLocation.Z, ViewYaw);
}

void AOUCharacterPawn::SetupPlayerInputComponent(UInputComponent* PlayerInputComponent)
{
	Super::SetupPlayerInputComponent(PlayerInputComponent);
	if (!PlayerInputComponent)
	{
		return;
	}
	// Ходьба: W/S — вперёд/назад, A/D — стрейф вбок.
	PlayerInputComponent->BindAxis(TEXT("Move"), this, &AOUCharacterPawn::AxisForward);
	PlayerInputComponent->BindAxis(TEXT("Strafe"), this, &AOUCharacterPawn::AxisRight);
	// Обзор: мышь (и стрелки как запасной вариант).
	PlayerInputComponent->BindAxis(TEXT("LookYaw"), this, &AOUCharacterPawn::AxisLookYaw);
	PlayerInputComponent->BindAxis(TEXT("LookPitch"), this, &AOUCharacterPawn::AxisLookPitch);
	// Мышь: значение уже является дельтой за кадр, поэтому обрабатываем отдельно
	// (умножать его на время кадра нельзя — иначе обзор «плывёт» при просадке FPS).
	PlayerInputComponent->BindAxis(TEXT("LookYawMouse"), this, &AOUCharacterPawn::AxisLookYawMouse);
	PlayerInputComponent->BindAxis(TEXT("LookPitchMouse"), this, &AOUCharacterPawn::AxisLookPitchMouse);
	PlayerInputComponent->BindAction(TEXT("Jump"), IE_Pressed, this, &AOUCharacterPawn::OnJumpPressed);
	PlayerInputComponent->BindAction(TEXT("Jump"), IE_Released, this, &AOUCharacterPawn::OnJumpReleased);
	PlayerInputComponent->BindAction(TEXT("ExitShip"), IE_Pressed, this, &AOUCharacterPawn::OnExitPressed);
	PlayerInputComponent->BindAction(TEXT("ExitShip"), IE_Released, this, &AOUCharacterPawn::OnExitReleased);
	// Огонь: зажимание кнопки ведёт непрерывную стрельбу (окно оружия).
	PlayerInputComponent->BindAction(TEXT("FireWeapon"), IE_Pressed, this, &AOUCharacterPawn::FireWeapon);
	// Копание грунта (ЛКМ вне строительства).
	PlayerInputComponent->BindAction(TEXT("Dig"), IE_Pressed, this, &AOUCharacterPawn::DigTerrain);
	// Рынок и инвентарь: M — окно, 1 — продать руду, 2 — купить руду.
	PlayerInputComponent->BindAction(TEXT("MarketToggle"), IE_Pressed, this, &AOUCharacterPawn::ToggleMarket);
	PlayerInputComponent->BindAction(TEXT("MarketSell"), IE_Pressed, this, &AOUCharacterPawn::OnMarketSell);
	PlayerInputComponent->BindAction(TEXT("MarketBuy"), IE_Pressed, this, &AOUCharacterPawn::OnMarketBuy);
	// Инвентарь/трюм: I — отдельное окно (сетка предметов, объём и масса).
	PlayerInputComponent->BindAction(TEXT("InventoryToggle"), IE_Pressed, this, &AOUCharacterPawn::ToggleInventory);
	// Производство: Q — отдельное окно рецептов (крафт).
	PlayerInputComponent->BindAction(TEXT("CraftMenu"), IE_Pressed, this, &AOUCharacterPawn::ToggleCraft);
	// Карта: Tab — окно карты (территории, тела системы, зум).
	PlayerInputComponent->BindAction(TEXT("MapMenu"), IE_Pressed, this, &AOUCharacterPawn::ToggleMap);
	// Лог событий: ` — окно прокручиваемого лога.
	PlayerInputComponent->BindAction(TEXT("EventLogMenu"), IE_Pressed, this, &AOUCharacterPawn::ToggleEventLog);
	// Контакты: , — окно онлайн-игроков.
	PlayerInputComponent->BindAction(TEXT("ContactsMenu"), IE_Pressed, this, &AOUCharacterPawn::ToggleContacts);
	// Строительство: B открывает/закрывает меню, ЛКМ ставит выбранный блок.
	PlayerInputComponent->BindAction(TEXT("BuildMode"), IE_Pressed, this, &AOUCharacterPawn::ToggleBuildMenu);
	PlayerInputComponent->BindAction(TEXT("BuildPlace"), IE_Pressed, this, &AOUCharacterPawn::PlaceBuildBlock);
	// Удаление и покраска блока под прицелом (правая и средняя кнопки мыши).
	PlayerInputComponent->BindAction(TEXT("BuildRemove"), IE_Pressed, this, &AOUCharacterPawn::OnBuildRemove);
	PlayerInputComponent->BindAction(TEXT("BuildPaint"), IE_Pressed, this, &AOUCharacterPawn::OnBuildPaint);
	// Выбор блока в палитре: колесо мыши.
	PlayerInputComponent->BindAction(TEXT("BuildNext"), IE_Pressed, this, &AOUCharacterPawn::OnBuildNext);
	PlayerInputComponent->BindAction(TEXT("BuildPrev"), IE_Pressed, this, &AOUCharacterPawn::OnBuildPrev);
	// Размер кисти постановки: C — следующий, V — предыдущий.
	PlayerInputComponent->BindAction(TEXT("BuildBrush"), IE_Pressed, this, &AOUCharacterPawn::BuildBrushNext);
	PlayerInputComponent->BindAction(TEXT("BuildBrushPrev"), IE_Pressed, this, &AOUCharacterPawn::BuildBrushPrev);
	// Отдельное окно размеров вокселя: V — открыть/закрыть.
	PlayerInputComponent->BindAction(TEXT("VoxelMenu"), IE_Pressed, this, &AOUCharacterPawn::ToggleVoxelMenu);
	// Ядро: Y — переключить размер ядра (когда выбран слот «ЯДРО»).
	PlayerInputComponent->BindAction(TEXT("CoreSize"), IE_Pressed, this, &AOUCharacterPawn::CoreSizeNext);
	// Назначение ядра: статичная база → корабль → динамическое (клавиша U).
	PlayerInputComponent->BindAction(TEXT("CoreKind"), IE_Pressed, this, &AOUCharacterPawn::CoreKindNext);
	// Захват территории: T — объявить текущую зону своей.
	PlayerInputComponent->BindAction(TEXT("TerritoryCapture"), IE_Pressed, this, &AOUCharacterPawn::CaptureTerritory);
	// Совместная стройка: H — разрешить ближайшему игроку строить.
	PlayerInputComponent->BindAction(TEXT("BuildGrant"), IE_Pressed, this, &AOUCharacterPawn::GrantBuildToNearest);
	// Шаблоны постройки (blueprints): Z/X — выбрать, G — поставить.
	PlayerInputComponent->BindAction(TEXT("BlueprintNext"), IE_Pressed, this, &AOUCharacterPawn::BlueprintNext);
	PlayerInputComponent->BindAction(TEXT("BlueprintPrev"), IE_Pressed, this, &AOUCharacterPawn::BlueprintPrev);
	PlayerInputComponent->BindAction(TEXT("BlueprintPlace"), IE_Pressed, this, &AOUCharacterPawn::BlueprintPlace);
	// Скрипты: L — окно скриптов объекта.
	PlayerInputComponent->BindAction(TEXT("ScriptMenu"), IE_Pressed, this, &AOUCharacterPawn::ToggleScripts);
	// Мастерская: K — обмен шаблонами.
	PlayerInputComponent->BindAction(TEXT("WorkshopMenu"), IE_Pressed, this, &AOUCharacterPawn::ToggleWorkshop);
	// Организация: O — окно гильдии.
	PlayerInputComponent->BindAction(TEXT("OrgMenu"), IE_Pressed, this, &AOUCharacterPawn::ToggleOrg);
	// Персонаж: P — окно талантов.
	PlayerInputComponent->BindAction(TEXT("TalentMenu"), IE_Pressed, this, &AOUCharacterPawn::ToggleTalents);
	// Alt — отпустить/вернуть курсор (обзор мышью / работа с интерфейсом).
	PlayerInputComponent->BindAction(TEXT("CursorToggle"), IE_Pressed, this, &AOUCharacterPawn::OnCursorToggle);
}

void AOUCharacterPawn::AxisForward(float Value) { InputForward = FMath::Clamp(Value, -1.f, 1.f); }
void AOUCharacterPawn::AxisRight(float Value) { InputRight = FMath::Clamp(Value, -1.f, 1.f); }
void AOUCharacterPawn::AxisLookYaw(float Value) { InputLookYaw = FMath::Clamp(Value, -1.f, 1.f); }
void AOUCharacterPawn::AxisLookPitch(float Value) { InputLookPitch = FMath::Clamp(Value, -1.f, 1.f); }

void AOUCharacterPawn::AxisLookYawMouse(float Value)
{
	// Значение мыши в UE — это уже дельта в «пикселях» за кадр. Применяем сразу
	// (без умножения на DeltaSeconds), иначе чувствительность зависит от FPS.
	if (bCursorCaptured)
	{
		ViewYaw += Value * MouseSensitivity;
	}
}

void AOUCharacterPawn::AxisLookPitchMouse(float Value)
{
	if (bCursorCaptured)
	{
		// Мышь ВВЕРХ должна смотреть ВВЕРХ. Движок (`SceneViewport`) считает
		// MouseDelta.Y -= CursorDelta.Y, поэтому движению вверх соответствует
		// MouseY > 0; в ini ось задана Scale=+1 — инверсии нет.
		ViewPitch = FMath::Clamp(ViewPitch + Value * MouseSensitivity, -1.4f, 1.4f);
	}
}

void AOUCharacterPawn::OnCursorToggle()
{
	// Alt — как в Dual Universe: отпустить курсор для интерфейса и вернуть
	// обратно. Пока курсор свободен, обзор мышью не работает.
	bCursorCaptured = !bCursorCaptured;
	if (APlayerController* PC = Cast<APlayerController>(GetController()))
	{
		PC->bShowMouseCursor = !bCursorCaptured;
		if (bCursorCaptured)
		{
			PC->SetInputMode(FInputModeGameOnly());
		}
		else
		{
			FInputModeGameAndUI Mode;
			Mode.SetHideCursorDuringCapture(false);
			PC->SetInputMode(Mode);
		}
	}
}
void AOUCharacterPawn::OnJumpPressed() { bJumpQueued = true; }
void AOUCharacterPawn::OnJumpReleased() { }
void AOUCharacterPawn::OnExitPressed() { bExitHeld = true; ExitHoldTime = 0.f; }
void AOUCharacterPawn::OnExitReleased() { bExitHeld = false; ExitHoldTime = 0.f; }
void AOUCharacterPawn::OnBuildNext() { if (bBuildMenuOpen) { SelectBuildPart((BuildSelection + 1) % (int32)EOUBuildPart::Count); } }
void AOUCharacterPawn::OnBuildPrev() { if (bBuildMenuOpen) { SelectBuildPart((BuildSelection + (int32)EOUBuildPart::Count - 1) % (int32)EOUBuildPart::Count); } }

void AOUCharacterPawn::GrantBuildToNearest()
{
	// Разрешить ближайшему другому игроку строить в нашей конструкции (G в меню).
	Net = Net ? Net : FindComponentByClass<UOUClientNet>();
	if (!Net || !Net->IsConnected() || Net->OwnEntityId == 0)
	{
		UE_LOG(LogOUChar, Log, TEXT("Права: нет соединения с сервером"));
		return;
	}
	// Ищем ближайшего другого пешего персонажа и его сущность.
	const FVector MyPos = GetActorLocation();
	int32 BestEntity = 0;
	float BestDist = TNumericLimits<float>::Max();
	for (const FOUCharacterState& C : Net->RemoteCharacters)
	{
		if (C.Entity == Net->OwnEntityId)
		{
			continue;
		}
		// C.Position — в серверных осях/единицах: переводим в UE (как при отрисовке).
		const FVector Cm = OUInterop::ServerToUE(C.Position);
		const float D = FVector::Dist(MyPos, Cm);
		if (D < BestDist)
		{
			BestDist = D;
			BestEntity = C.Entity;
		}
	}
	if (BestEntity == 0)
	{
		UE_LOG(LogOUChar, Log, TEXT("Права: рядом нет других игроков"));
		return;
	}
	// В протоколе `player` = сущность игрока (на сервере они совпадают).
	Net->SendGrantBuild(Net->OwnEntityId, BestEntity);
	UE_LOG(LogOUChar, Log, TEXT("Права: разрешил строить сущности %d (%.0f см)"), BestEntity, BestDist);
}

void AOUCharacterPawn::PlaceBlueprint(int32 BlueprintId)
{
	// Поставить шаблон постройки: якорь — точка под прицелом (как для блока).
	// Сервер штампует воксели авторитетно; клиент лишь выбирает шаблон и место.
	if (!bBuildMenuOpen)
	{
		return;
	}
	if (!CanBuildHere())
	{
		return;
	}
	Net = Net ? Net : FindComponentByClass<UOUClientNet>();
	if (!Net || !Net->IsConnected() || Net->OwnEntityId == 0)
	{
		UE_LOG(LogOUChar, Log, TEXT("Шаблон: нет соединения с сервером"));
		return;
	}
	FIntVector Target(0, 0, 0);
	FVector Hit, Normal;
	if (TraceBuildTarget(Target, Hit, Normal))
	{
		Target += WorldDirToVoxelShift(Normal);
	}
	else
	{
		FVector GroundDir;
		if (AimTerrainDir(GroundDir))
		{
			Target = WorldToVoxel(GroundDir * (SurfaceRadius(GroundDir) + 75.f));
		}
	}
	const int32 TargetCore = (Net->ActiveCoreId != 0) ? Net->ActiveCoreId : Net->OwnEntityId;
	Net->SendPlaceBlueprint(BlueprintId, Target.X, Target.Y, Target.Z, TargetCore);
	UE_LOG(LogOUChar, Log, TEXT("Шаблон %d: поставлен в (%d,%d,%d) ядро %d"),
		BlueprintId, Target.X, Target.Y, Target.Z, TargetCore);
}

void AOUCharacterPawn::BlueprintNext()
{
	if (!bBuildMenuOpen) { return; }
	Net = Net ? Net : FindComponentByClass<UOUClientNet>();
	const int32 N = (Net && Net->Blueprints.Num() > 0) ? Net->Blueprints.Num() : 1;
	BlueprintSelection = (BlueprintSelection + 1) % N;
	UE_LOG(LogOUChar, Log, TEXT("Шаблон: выбран индекс %d"), BlueprintSelection);
}

void AOUCharacterPawn::BlueprintPrev()
{
	if (!bBuildMenuOpen) { return; }
	Net = Net ? Net : FindComponentByClass<UOUClientNet>();
	const int32 N = (Net && Net->Blueprints.Num() > 0) ? Net->Blueprints.Num() : 1;
	BlueprintSelection = (BlueprintSelection + N - 1) % N;
	UE_LOG(LogOUChar, Log, TEXT("Шаблон: выбран индекс %d"), BlueprintSelection);
}

void AOUCharacterPawn::BlueprintPlace()
{
	if (!bBuildMenuOpen) { return; }
	Net = Net ? Net : FindComponentByClass<UOUClientNet>();
	if (!Net || !Net->Blueprints.IsValidIndex(BlueprintSelection))
	{
		UE_LOG(LogOUChar, Log, TEXT("Шаблон: библиотека не получена"));
		return;
	}
	PlaceBlueprint(Net->Blueprints[BlueprintSelection].Id);
}

void AOUCharacterPawn::ToggleBuildMenu()
{
	bBuildMenuOpen = !bBuildMenuOpen;
	UE_LOG(LogOUChar, Log, TEXT("Меню строительства: %s (выбран %s)"),
		bBuildMenuOpen ? TEXT("открыто") : TEXT("закрыто"), *BuildPartName(BuildSelection));
}

void AOUCharacterPawn::ToggleVoxelMenu()
{
	// Отдельное окно «ВОКСЕЛИ» (клавиша V): выбор размера кисти квадратами.
	AOUPlayerMenuController* Menu = Cast<AOUPlayerMenuController>(
		GetController() ? GetController() : nullptr);
	if (!Menu)
	{
		Menu = Cast<AOUPlayerMenuController>(GetWorld() ? GetWorld()->GetFirstPlayerController() : nullptr);
	}
	if (!Menu)
	{
		return;
	}
	Menu->bVoxelsOpen = !Menu->bVoxelsOpen;
	// Пока окно открыто — курсор свободен, чтобы кликать по квадратам.
	Menu->SetMenuCursor(Menu->bVoxelsOpen);
	UE_LOG(LogOUChar, Log, TEXT("Воксели: окно %s"), Menu->bVoxelsOpen ? TEXT("открыто") : TEXT("закрыто"));
}

bool AOUCharacterPawn::CanBuildHere()
{
	// Права авторитетны на сервере: если он сообщил, что редактировать нельзя —
	// блокируем локально (посыл всё равно отклонится, но так честнее в UI).
	Net = Net ? Net : FindComponentByClass<UOUClientNet>();
	if (Net && Net->IsConnected() && !Net->bCanBuild)
	{
		return false;
	}
	return true;
}

void AOUCharacterPawn::SelectBuildPart(int32 Index)
{
	const int32 Count = (int32)EOUBuildPart::Count;
	if (Count > 0)
	{
		BuildSelection = ((Index % Count) + Count) % Count;
	}
}

void AOUCharacterPawn::BuildBrushNext()
{
	BuildBrush = (BuildBrush % 3) + 1;
	UE_LOG(LogOUChar, Log, TEXT("Кисть строительства: %s (%d)"), *BuildBrushName(), BuildBrush);
}

void AOUCharacterPawn::BuildBrushPrev()
{
	BuildBrush = ((BuildBrush + 1) % 3) + 1;
	UE_LOG(LogOUChar, Log, TEXT("Кисть строительства: %s (%d)"), *BuildBrushName(), BuildBrush);
}

FString AOUCharacterPawn::BuildBrushName() const
{
	switch (BuildBrush)
	{
	case 1: return TEXT("малый 0.5 м");
	case 2: return TEXT("средний 1 м");
	case 3: return TEXT("большой 1.5 м");
	default: return TEXT("малый 0.5 м");
	}
}

void AOUCharacterPawn::SetBuildBrush(int32 Size)
{
	BuildBrush = FMath::Clamp(Size, 1, 3);
	UE_LOG(LogOUChar, Log, TEXT("Кисть строительства: %s (%d)"), *BuildBrushName(), BuildBrush);
}

int32 AOUCharacterPawn::CoreSizeVoxels() const
{
	switch (CoreSizeSel)
	{
	case 1: return 32;
	case 2: return 64;
	default: return 16;
	}
}

FString AOUCharacterPawn::CoreSizeName() const
{
	switch (CoreSizeSel)
	{
	case 1: return TEXT("средний");
	case 2: return TEXT("большой");
	default: return TEXT("малый");
	}
}

FString AOUCharacterPawn::CoreKindName() const
{
	switch (CoreKindSel)
	{
	case 1: return TEXT("корабль");
	case 2: return TEXT("динамическое");
	default: return TEXT("статичная");
	}
}

int32 AOUCharacterPawn::CoreVoxelSize() const
{
	// Ядро — тоже воксель: малый (16) → 1, средний (32) → 2, большой (64) → 3.
	return CoreSizeSel + 1;
}

void AOUCharacterPawn::CoreSizeNext()
{
	CoreSizeSel = (CoreSizeSel + 1) % 3;
	UE_LOG(LogOUChar, Log, TEXT("Ядро: размер %s (%d)"), *CoreSizeName(), CoreSizeVoxels());
}

void AOUCharacterPawn::CoreKindNext()
{
	CoreKindSel = (CoreKindSel + 1) % 3;
	UE_LOG(LogOUChar, Log, TEXT("Ядро: назначение %s"), *CoreKindName());
}

void AOUCharacterPawn::PlaceCoreBlock()
{
	// Поставить ядро (слот «ЯДРО» в палитре): размер/назначение — по выбору,
	// ячейка — под прицелом (или перед игроком, если прицел в небо).
	Net = Net ? Net : FindComponentByClass<UOUClientNet>();
	if (!Net || !Net->IsConnected() || Net->OwnEntityId == 0)
	{
		UE_LOG(LogOUChar, Log, TEXT("Ядро: нет соединения с сервером"));
		return;
	}
	// Ядро ставится ПРЯМО под прицел (а не снапом к чужим блокам, как обычный
	// воксель): иначе оно появлялось бы внутри уже стоящей постройки и его не
	// было бы видно. Ищем ячейку на земле под прицелом.
	const FIntVector Target = CorePlacementCell();
	Net->SendPlaceCore(CoreSizeVoxels(), CoreKindSel, Target.X, Target.Y, Target.Z);
	UE_LOG(LogOUChar, Log, TEXT("Ядро: поставить %s %s в (%d,%d,%d)"),
		*CoreSizeName(), *CoreKindName(), Target.X, Target.Y, Target.Z);
}

FIntVector AOUCharacterPawn::CorePlacementCell() const
{
	// Ячейка, где встанет ядро из слота «ЯДРО»: под прицелом (на земле), а если
	// прицел в небо — перед игроком. Одна формула на постановку, призрак и
	// превью радиуса, чтобы они не расходились.
	FVector GroundDir;
	if (AimTerrainDir(GroundDir))
	{
		return WorldToVoxel(GroundDir * (SurfaceRadius(GroundDir) + 75.f));
	}
	return WorldToVoxel(GetActorLocation() + GetActorForwardVector() * 600.f);
}

FString AOUCharacterPawn::BuildPartName(int32 Index)
{
	switch ((EOUBuildPart)Index)
	{
	case EOUBuildPart::Hull:  return TEXT("Корпус");
	case EOUBuildPart::Panel: return TEXT("Панель");
	case EOUBuildPart::Beam:  return TEXT("Балка");
	case EOUBuildPart::Light: return TEXT("Свет");
	case EOUBuildPart::Glass: return TEXT("Стекло");
	case EOUBuildPart::Frame: return TEXT("Решётка");
	case EOUBuildPart::Slope: return TEXT("Скат");
	case EOUBuildPart::Rod:   return TEXT("Стержень");
	case EOUBuildPart::Trim:  return TEXT("Окантовка");
	case EOUBuildPart::Core:  return TEXT("ЯДРО");
	default: return TEXT("Блок");
	}
}

FLinearColor AOUCharacterPawn::BuildPartColor(int32 Index)
{
	switch ((EOUBuildPart)Index)
	{
	case EOUBuildPart::Hull:  return FLinearColor(0.45f, 0.55f, 0.70f);   // серо-стальной
	case EOUBuildPart::Panel: return FLinearColor(0.85f, 0.60f, 0.25f);   // тёплый
	case EOUBuildPart::Beam:  return FLinearColor(0.30f, 0.75f, 0.55f);   // зелёный
	case EOUBuildPart::Light: return FLinearColor(1.00f, 0.92f, 0.45f);   // светящийся
	case EOUBuildPart::Glass: return FLinearColor(0.55f, 0.80f, 0.95f);   // голубое стекло
	case EOUBuildPart::Frame: return FLinearColor(0.35f, 0.38f, 0.45f);   // тёмная решётка
	case EOUBuildPart::Slope: return FLinearColor(0.70f, 0.45f, 0.85f);   // фиолетовый скат
	case EOUBuildPart::Rod:   return FLinearColor(0.90f, 0.35f, 0.35f);   // красный стержень
	case EOUBuildPart::Trim:  return FLinearColor(0.95f, 0.95f, 0.98f);   // белая окантовка
	default: return FLinearColor(0.60f, 0.60f, 0.60f);
	}
}

void AOUCharacterPawn::PlaceBuildBlock()
{
	UWorld* World = GetWorld();
	if (!World || !bBuildMenuOpen)
	{
		return;
	}
	if (!CanBuildHere())
	{
		UE_LOG(LogOUChar, Log, TEXT("Строительство: нет прав на эту конструкцию"));
		return;
	}
	// Выбран слот «ЯДРО» — ставим ядро (оно задаёт размер и назначение постройки).
	if ((EOUBuildPart)BuildSelection == EOUBuildPart::Core)
	{
		PlaceCoreBlock();
		return;
	}

	const FLinearColor Col = BuildPartColor(BuildSelection);
	const int32 R = FMath::RoundToInt(Col.R * 255.f);
	const int32 G = FMath::RoundToInt(Col.G * 255.f);
	const int32 B = FMath::RoundToInt(Col.B * 255.f);

	// Онлайн: команду строительства отправляем на СЕРВЕР — он авторитетно
	// проверяет (целостность, права, лимиты) и рассылает правку всем. Блок
	// появится, когда придёт авторитетная правка (ApplyServerBuildEdits).
	if (Net && Net->IsConnected() && Net->OwnEntityId != 0)
	{
		// Куда ставим (как в DU): вплотную к постройке — см. ComputeBuildTarget.
		// Так блок всегда связен с ядром, и сервер его принимает.
		FIntVector Target;
		if (!ComputeBuildTarget(Target))
		{
			return;
		}
		// Один клик = один воксель-куб выбранного размера (кисть 1/2/3 ед.).
		// Воксели разного размера стыкуются по граням — сервер проверяет связность.
		// Ставим в АКТИВНОЕ ядро (у игрока их может быть несколько).
		const int32 TargetCore = (Net->ActiveCoreId != 0) ? Net->ActiveCoreId : Net->OwnEntityId;
		Net->SendBuild(TargetCore, /*Place=*/0, Target.X, Target.Y, Target.Z, R, G, B, BuildBrush);
		BuiltCoords.Add(Target);
		// Запоминаем размер ожидаемого вокселя: планировщик учтёт его, чтобы
		// следующие клики не наложились, пока сервер не ответил.
		PendingVoxelSize.Add(Target, BuildBrush);
		UE_LOG(LogOUChar, Log, TEXT("Строительство: воксель %s в (%d,%d,%d) ядро %d"),
			*BuildBrushName(), Target.X, Target.Y, Target.Z, TargetCore);
		// Обучение: строительство.
		if (AOUPlayerMenuController* MenuB = Cast<AOUPlayerMenuController>(GetController()))
		{
			MenuB->NotifyTutorial(TEXT("build"));
		}
		return;
	}

	// Оффлайн (без сервера) — ставим локально, как раньше, для одиночной игры.
	// Размер — как у кисти (0.5/1/1.5 м).
	const float BlockSize = static_cast<float>(OU_VOXEL_CM) * static_cast<float>(BuildBrushSide());
	const FVector Up = GetActorLocation().GetSafeNormal();
	FVector Fwd = GetActorForwardVector();
	Fwd = FVector::VectorPlaneProject(Fwd, Up).GetSafeNormal();
	if (Fwd.IsNearlyZero())
	{
		Fwd = FVector::CrossProduct(Up, FVector::RightVector).GetSafeNormal();
	}
	const FVector Right = FVector::CrossProduct(Up, Fwd).GetSafeNormal();
	FVector Pos = GetActorLocation() + Fwd * (BlockSize * 2.2f) + Right * (BlockSize * 1.2f);
	Pos += Fwd * (BlockSize * 1.1f * BuiltCount);
	const FVector Dir = Pos.GetSafeNormal();
	Pos = Dir * (SurfaceRadius(Dir) + BlockSize * 0.5f);

	AOUBuildBlock* Block = World->SpawnActor<AOUBuildBlock>(
		AOUBuildBlock::StaticClass(), FTransform(FRotationMatrix::MakeFromXZ(Fwd, Up).Rotator(), Pos));
	if (Block)
	{
		Block->BlockSize = BlockSize;
		Block->BlockColor = BuildPartColor(BuildSelection);
		Block->ApplyLook();
		++BuiltCount;
		UE_LOG(LogOUChar, Log, TEXT("Построен блок «%s» #%d (оффлайн)"),
			*BuildPartName(BuildSelection), BuiltCount);
	}
}

void AOUCharacterPawn::UpdateBuildGhost()
{
	if (!BuildGhost)
	{
		return;
	}
	// Призрак блока виден только когда открыто меню строительства и мы в сети.
	if (!bBuildMenuOpen)
	{
		BuildGhost->SetVisibility(false);
		return;
	}

	// Куда встанет следующий блок — та же логика, что у PlaceBuildBlock.
	// Для слота «ЯДРО» — ячейка постановки ядра (под прицелом); для блока — снап.
	FIntVector Target;
	if (((EOUBuildPart)BuildSelection == EOUBuildPart::Core))
	{
		Target = CorePlacementCell();
	}
	else if (!ComputeBuildTarget(Target))
	{
		BuildGhost->SetVisibility(false);
		return;
	}

	const bool bCoreSel = ((EOUBuildPart)BuildSelection == EOUBuildPart::Core);
	const int32 VoxSide = bCoreSel ? CoreVoxelSize() : BuildBrushSide();
	const FVector Pos = VoxelWorld(FVector(Target.X, Target.Y, Target.Z), VoxSide);
	// Размер призрака: у блока — размер кисти (1/2/3 → 1.5/3/4.5 м), у ядра —
	// размер вокселя-ядра. Призрак — ОДИН куб (клик ставит один воксель).
	BuildGhost->SetWorldLocation(Pos);
	// Ориентация призрака — как у реального блока (по осям сетки).
	BuildGhost->SetWorldRotation(BuildGridRotation());
	// Масштаб: базовый куб движка — 100 см; воксель — 150 см × размер.
	const float Scale = (static_cast<float>(OU_VOXEL_CM) * static_cast<float>(VoxSide)) / 100.f;
	BuildGhost->SetWorldScale3D(FVector(Scale));
	// Ядро — синеватый призрак (отличить от блоков).
	const FLinearColor Col = bCoreSel
		? FLinearColor(0.35f, 0.75f, 1.0f)
		: BuildPartColor(BuildSelection);
	BuildGhost->SetVectorParameterValueOnMaterials(TEXT("Color"),
		FVector(Col.R * 0.8f + 0.2f, Col.G * 0.8f + 0.2f, Col.B * 0.8f + 0.2f));
	BuildGhost->SetVisibility(true);
	// Диагностика `-OUGhostLog`: печатаем позицию призрака раз в ~секунду.
	if (FParse::Param(FCommandLine::Get(), TEXT("OUGhostLog")))
	{
		static double LastLog = 0.0;
		const double Now = GetWorld() ? GetWorld()->GetTimeSeconds() : 0.0;
		if (Now - LastLog > 1.0)
		{
			LastLog = Now;
			UE_LOG(LogOUChar, Log, TEXT("Призрак блока: coord (%d,%d,%d), мир %s, игрок %s, дист %.0f см, видим=%d"),
				Target.X, Target.Y, Target.Z, *Pos.ToCompactString(), *GetActorLocation().ToCompactString(),
				FVector::Dist(Pos, GetActorLocation()), BuildGhost->IsVisible() ? 1 : 0);
		}
	}
}

void AOUCharacterPawn::UpdateCoreGrid()
{
	// Сетка-радиус постройки (каркас области ядра). Показывается:
	//  * при выборе слота «ЯДРО» в палитре — превью в месте постановки;
	//  * когда прицел НАВЕДЁН на ядро (своё или чужое) — видно радиус постройки.
	// В остальное время скрыта (не мешает обзору).
	bool bShowGrid = false;
	FVector GridCenter = FVector::ZeroVector;
	double GridSideCm = 0.0;

	Net = Net ? Net : FindComponentByClass<UOUClientNet>();
	const bool bCoreSel = ((EOUBuildPart)BuildSelection == EOUBuildPart::Core);
	if (bBuildMenuOpen && bCoreSel)
	{
		// Превью: где встанет ядро выбранного размера (та же ячейка, что при ЛКМ).
		const FIntVector Target = CorePlacementCell();
		bShowGrid = true;
		GridCenter = VoxelWorld(FVector(Target.X, Target.Y, Target.Z), CoreSizeVoxels());
		GridSideCm = OU_VOXEL_CM * static_cast<double>(CoreSizeVoxels());
	}
	else if (Net)
	{
		// Наведён ли прицел на ядро (координата под прицелом == ячейка ядра)?
		FIntVector AimCoord;
		FVector AimHit, AimNormal;
		if (TraceBuildTarget(AimCoord, AimHit, AimNormal))
		{
			for (const FOUCoreInfo& C : Net->Cores)
			{
				if (C.Size > 0 && C.Coord == AimCoord)
				{
					bShowGrid = true;
					GridCenter = VoxelWorld(FVector(C.Coord.X, C.Coord.Y, C.Coord.Z), C.Size);
					GridSideCm = OU_VOXEL_CM * static_cast<double>(C.Size);
					break;
				}
			}
		}
	}

	if (bShowGrid)
	{
		UWorld* World = GetWorld();
		if (!BuildGrid && World)
		{
			BuildGrid = World->SpawnActor<AOUBuildGrid>(AOUBuildGrid::StaticClass(), FTransform::Identity);
		}
		if (BuildGrid)
		{
			BuildGrid->Setup(GridCenter, GridSideCm, BuildGridRotation());
		}
	}
	else if (BuildGrid)
	{
		BuildGrid->SetGridVisible(false);
	}
}

bool AOUCharacterPawn::AimTerrainDir(FVector& OutDir) const
{
	// Марш луча взгляда до поверхности планеты: из точки камеры вперёд, ищем
	// пересечение с рельефом. Возвращаем направление точки от центра планеты.
	AOUVoxelPlanet* Planet = AOUVoxelPlanet::FindHomePlanet(GetWorld());
	if (!Planet)
	{
		return false;
	}
	const FVector Start = Camera ? Camera->GetComponentLocation() : GetActorLocation();
	const FVector Dir = (Camera ? Camera->GetForwardVector() : GetActorForwardVector()).GetSafeNormal();
	if (Dir.IsNearlyZero())
	{
		return false;
	}
	for (int32 Step = 0; Step < 400; ++Step)
	{
		const FVector P = Start + Dir * (40.f * Step);   // шаг 40 см, до 160 м
		const FVector DirP = P.GetSafeNormal();
		if (DirP.IsNearlyZero())
		{
			break;
		}
		const float Surf = Planet->SurfaceRadiusAt(DirP);
		if (P.Size() <= Surf)
		{
			OutDir = DirP;
			return true;
		}
	}
	return false;
}

FIntVector AOUCharacterPawn::WorldDirToVoxelShift(const FVector& WorldDir) const
{
	// Проецируем мировое направление на локальный базис сетки: получаем
	// «насколько вперёд/вверх/вправо» смотрит луч в координатах вокселей.
	const FVector BaseUE = AOUVoxelPlanet::HomeSpawn(2.0f);
	const FVector UpUE = BaseUE.GetSafeNormal();
	FVector FwdUE = FVector::VectorPlaneProject(FVector(1.f, 0.f, 0.f), UpUE).GetSafeNormal();
	if (FwdUE.IsNearlyZero())
	{
		FwdUE = FVector::VectorPlaneProject(FVector::ForwardVector, UpUE).GetSafeNormal();
	}
	const FVector RightUE = FVector::CrossProduct(UpUE, FwdUE).GetSafeNormal();
	const FVector D = WorldDir.GetSafeNormal();
	const double FX = FVector::DotProduct(D, FwdUE);
	const double FY = FVector::DotProduct(D, UpUE);
	const double FZ = FVector::DotProduct(D, RightUE);

	// Сдвиг по оси с наибольшим вкладом — ставим блок в грань, куда смотрит луч.
	FIntVector Shift = FIntVector::ZeroValue;
	const double AX = FMath::Abs(FX), AY = FMath::Abs(FY), AZ = FMath::Abs(FZ);
	if (AX >= AY && AX >= AZ)      { Shift.X = FX >= 0.0 ? 1 : -1; }
	else if (AY >= AZ)             { Shift.Y = FY >= 0.0 ? 1 : -1; }
	else                           { Shift.Z = FZ >= 0.0 ? 1 : -1; }
	return Shift;
}

bool AOUCharacterPawn::TraceBuildTarget(FIntVector& OutCoord, FVector& OutHit, FVector& OutNormal) const
{
	UWorld* World = GetWorld();
	if (!World)
	{
		return false;
	}
	// Трассируем взгляд: ищем ближайший блок постройки в пределах 15 м
	// (блоки могут стоять и в ряду впереди, и рядом).
	const FVector Start = Camera ? Camera->GetComponentLocation() : GetActorLocation();
	const FVector Dir = Camera ? Camera->GetForwardVector() : GetActorForwardVector();
	const FVector End = Start + Dir.GetSafeNormal() * 1500.f;
	FHitResult Hit;
	FCollisionQueryParams Params;
	Params.AddIgnoredActor(this);
	if (!World->LineTraceSingleByChannel(Hit, Start, End, ECC_Visibility, Params))
	{
		return false;
	}
	AActor* HitActor = Hit.GetActor();
	if (!HitActor || !HitActor->IsA(AOUBuildBlock::StaticClass()))
	{
		return false;
	}
	// Координату берём из таблицы (запомнена при создании блока) — это точно,
	// в отличие от пересчёта по мировой точке (на границе ячеек округление
	// уводит на соседнюю ячейку, и сервер отвечает «нет связи с ядром»).
	OutHit = Hit.ImpactPoint;
	OutNormal = Hit.ImpactNormal;
	if (const FIntVector* Found = BlockCoords.Find(HitActor))
	{
		OutCoord = *Found;
		// Размер найденного блока (1/2/3) — нужен для сдвига на соседнюю грань:
		// у крупного блока соседняя ячейка на N, а не на 1.
		if (const AOUBuildBlock* BB = Cast<AOUBuildBlock>(HitActor))
		{
			LastHitBlockSize = FMath::Clamp(
				FMath::RoundToInt(BB->BlockSize / static_cast<float>(OU_VOXEL_CM)), 1, 3);
		}
		else
		{
			LastHitBlockSize = 1;
		}
		return true;
	}
	// Запасной путь: пересчёт по точке чуть внутри блока.
	const FVector Inside = Hit.ImpactPoint - Hit.ImpactNormal * 10.f;
	OutCoord = WorldToVoxel(Inside);
	LastHitBlockSize = 1;
	return true;
}

FIntVector AOUCharacterPawn::WorldToVoxel(const FVector& WorldPos) const
{
	const FVector F = WorldToVoxelF(WorldPos);
	// Округляем до целой ячейки (мин. угол куба со стороной 1).
	return FIntVector(FMath::RoundToInt(F.X), FMath::RoundToInt(F.Y), FMath::RoundToInt(F.Z));
}

FVector AOUCharacterPawn::WorldToVoxelF(const FVector& WorldPos) const
{
	// Мировая точка → НЕПРЕРЫВНАЯ координата сетки (в вокселях) по общему якорю.
	// Нужно для точной привязки нового куба к точке под прицелом (например, к
	// любому месту грани крупного вокселя), а не только к его минимальному углу.
	const double VoxelCm = OU_VOXEL_CM;
	const FVector AnchorUE = BuildAnchorUE;
	const FVector UpUE = AnchorUE.GetSafeNormal();
	FVector FwdUE = FVector::VectorPlaneProject(FVector(1.f, 0.f, 0.f), UpUE).GetSafeNormal();
	if (FwdUE.IsNearlyZero())
	{
		FwdUE = FVector::VectorPlaneProject(FVector::ForwardVector, UpUE).GetSafeNormal();
	}
	const FVector RightUE = FVector::CrossProduct(UpUE, FwdUE).GetSafeNormal();

	const FVector Rel = WorldPos - AnchorUE;
	return FVector(
		FVector::DotProduct(Rel, FwdUE) / VoxelCm,
		FVector::DotProduct(Rel, UpUE) / VoxelCm,
		FVector::DotProduct(Rel, RightUE) / VoxelCm);
}

void AOUCharacterPawn::RemoveBuildBlock()
{
	if (!bBuildMenuOpen) { return; }
	if (!CanBuildHere()) { return; }
	FIntVector Coord;
	FVector Hit, Normal;
	if (!TraceBuildTarget(Coord, Hit, Normal))
	{
		UE_LOG(LogOUChar, Log, TEXT("Удаление: впереди нет блока"));
		return;
	}
	if (Net && Net->IsConnected() && Net->OwnEntityId != 0)
	{
		const int32 TargetCore = (Net->ActiveCoreId != 0) ? Net->ActiveCoreId : Net->OwnEntityId;
		Net->SendBuild(TargetCore, /*Remove=*/1, Coord.X, Coord.Y, Coord.Z, 0, 0, 0);
		UE_LOG(LogOUChar, Log, TEXT("Строительство: удалить (%d,%d,%d) ядро %d"),
			Coord.X, Coord.Y, Coord.Z, TargetCore);
	}
}

void AOUCharacterPawn::PaintBuildBlock()
{
	if (!bBuildMenuOpen) { return; }
	if (!CanBuildHere()) { return; }
	FIntVector Coord;
	FVector Hit, Normal;
	if (!TraceBuildTarget(Coord, Hit, Normal))
	{
		UE_LOG(LogOUChar, Log, TEXT("Покраска: впереди нет блока"));
		return;
	}
	if (Net && Net->IsConnected() && Net->OwnEntityId != 0)
	{
		const FLinearColor Col = BuildPartColor(BuildSelection);
		const int32 TargetCore = (Net->ActiveCoreId != 0) ? Net->ActiveCoreId : Net->OwnEntityId;
		Net->SendBuild(TargetCore, /*Paint=*/2, Coord.X, Coord.Y, Coord.Z,
			FMath::RoundToInt(Col.R * 255.f),
			FMath::RoundToInt(Col.G * 255.f),
			FMath::RoundToInt(Col.B * 255.f));
		UE_LOG(LogOUChar, Log, TEXT("Строительство: покрасить (%d,%d,%d) ядро %d"),
			Coord.X, Coord.Y, Coord.Z, TargetCore);
	}
}

void AOUCharacterPawn::OnBuildRemove() { RemoveBuildBlock(); }
void AOUCharacterPawn::OnBuildPaint() { PaintBuildBlock(); }

FVector AOUCharacterPawn::VoxelWorld(const FVector& ServerCoord, int32 Size) const
{
	// Мировая точка ЦЕНТРА куба вокселя: якорь + координаты (мин. угол) + половина
	// куба. Размер учитывается: у вокселя N центру надо +N/2 ячейки, иначе крупный
	// куб «уходит» под землю / в текстуру.
	const double VoxelCm = OU_VOXEL_CM;   // 1.5 м на воксель в см UE
	const int32 N = FMath::Clamp(Size, 1, 3);
	const FVector AnchorUE = BuildAnchorUE;
	const FVector UpUE = AnchorUE.GetSafeNormal();
	FVector FwdUE = FVector::VectorPlaneProject(FVector(1.f, 0.f, 0.f), UpUE).GetSafeNormal();
	if (FwdUE.IsNearlyZero())
	{
		FwdUE = FVector::VectorPlaneProject(FVector::ForwardVector, UpUE).GetSafeNormal();
	}
	const FVector RightUE = FVector::CrossProduct(UpUE, FwdUE).GetSafeNormal();
	const double Half = 0.5 * static_cast<double>(N);
	return AnchorUE
		+ FwdUE * ((ServerCoord.X + Half) * VoxelCm)
		+ UpUE * ((ServerCoord.Y + Half) * VoxelCm)
		+ RightUE * ((ServerCoord.Z + Half) * VoxelCm);
}

FRotator AOUCharacterPawn::BuildGridRotation() const
{
	// Ориентация блоков и призрака — по осям сетки конструкции (как в DU):
	// блоки «стоят» по локальной поверхности, а не по мировым осям UE. Иначе на
	// сферической планете кубы выглядят повёрнутыми «криво».
	const FVector AnchorUE = BuildAnchorUE;
	const FVector UpUE = AnchorUE.GetSafeNormal();
	FVector FwdUE = FVector::VectorPlaneProject(FVector(1.f, 0.f, 0.f), UpUE).GetSafeNormal();
	if (FwdUE.IsNearlyZero())
	{
		FwdUE = FVector::VectorPlaneProject(FVector::ForwardVector, UpUE).GetSafeNormal();
	}
	return FRotationMatrix::MakeFromXZ(FwdUE, UpUE).Rotator();
}

bool AOUCharacterPawn::SpaceFree(int32 CoreId, const FIntVector& Min, int32 Size) const
{
	// Куб занят, если хотя бы одна его единичная ячейка уже принадлежит другому
	// вокселю (принятому сервером или только что отправленному). Учитываем размеры:
	// все кубы занимают свои s×s×s единичных ячеек.
	const int32 S = FMath::Clamp(Size, 1, 3);
	auto Overlaps = [&](const FIntVector& A, int32 As, const FIntVector& B, int32 Bs)
	{
		// Пересечение кубов [A, A+As) и [B, B+Bs) по всем осям.
		return A.X < B.X + Bs && B.X < A.X + As
			&& A.Y < B.Y + Bs && B.Y < A.Y + As
			&& A.Z < B.Z + Bs && B.Z < A.Z + As;
	};
	if (const TMap<FIntVector, int32>* Map = CoreBlocks.Find(CoreId))
	{
		for (const TPair<FIntVector, int32>& P : *Map)
		{
			if (Overlaps(Min, S, P.Key, FMath::Clamp(P.Value, 1, 3))) { return false; }
		}
	}
	for (const TPair<FIntVector, int32>& P : PendingVoxelSize)
	{
		if (Overlaps(Min, S, P.Key, FMath::Clamp(P.Value, 1, 3))) { return false; }
	}
	return true;
}

bool AOUCharacterPawn::ComputeBuildTarget(FIntVector& OutTarget) const
{
	const int32 NewSide = ((EOUBuildPart)BuildSelection == EOUBuildPart::Core)
		? CoreVoxelSize() : BuildBrushSide();
	const int32 CoreId = Net ? Net->ActiveCoreId : 0;

	// 1. Предпочтительная ячейка под новым кубом.
	//   * прицел в блок — ставим на его ГРАНЬ, ровно под прицелом: по двум
	//     «боковым» осям берём ячейку точки попадания (можно на любое место грани
	//     крупного вокселя), по оси нормали — сразу за гранью;
	//   * прицел в землю — берём ячейку на поверхности (постройку основало ядро).
	FIntVector Preferred;
	{
		FIntVector Coord;
		FVector Hit, Normal;
		if (TraceBuildTarget(Coord, Hit, Normal))
		{
			const FIntVector Shift = WorldDirToVoxelShift(Normal);
			const int32 HitSide = FMath::Clamp(LastHitBlockSize, 1, 3);
			// Точка попадания в непрерывных координатах сетки — по ней выбираем
			// ячейку под прицелом на грани (а не всегда минимальный угол куба).
			const FVector HitF = WorldToVoxelF(Hit);
			auto Axis = [&](int32 AxisIdx, int32 Min) -> int32
			{
				const int32 N = AxisIdx == 0 ? Shift.X : (AxisIdx == 1 ? Shift.Y : Shift.Z);
				const double HitCoord = AxisIdx == 0 ? HitF.X : (AxisIdx == 1 ? HitF.Y : HitF.Z);
				if (N > 0) { return Min + HitSide; }      // сразу за дальней гранью
				if (N < 0) { return Min - NewSide; }      // перед ближней гранью
				// Боковая ось: ячейка под прицелом, но не выходим за «пятно» грани.
				const int32 Cell = FMath::FloorToInt(HitCoord);
				const int32 Hi = Min + FMath::Max(0, HitSide - NewSide);
				return FMath::Clamp(Cell, Min, Hi);
			};
			Preferred = FIntVector(Axis(0, Coord.X), Axis(1, Coord.Y), Axis(2, Coord.Z));
		}
		else
		{
			FVector GroundDir;
			if (AimTerrainDir(GroundDir))
			{
				Preferred = WorldToVoxel(GroundDir * (SurfaceRadius(GroundDir) + 75.f));
			}
			else
			{
				Preferred = FIntVector(BuiltCount + 1, 0, 0);
			}
		}
	}

	// 2. Место под предпочтительной ячейкой свободно — ставим туда.
	if (SpaceFree(CoreId, Preferred, NewSide))
	{
		OutTarget = Preferred;
		return true;
	}

	// 3. Место занято. Ищем ближайшую СВОБОДНУЮ ячейку, примыкающую к постройке:
	//    перебираем грани всех известных кубов (принятых и ожидаемых) и выбираем
	//    ту, что ближе к предпочтительной. Так серия кликов продолжает постройку
	//    без наложений кубов разного размера.
	FIntVector Best = Preferred;
	int32 BestD = TNumericLimits<int32>::Max();
	bool bFound = false;
	auto Consider = [&](const TMap<FIntVector, int32>& Map)
	{
		const FIntVector Dirs[6] = { {1,0,0}, {-1,0,0}, {0,1,0}, {0,-1,0}, {0,0,1}, {0,0,-1} };
		for (const TPair<FIntVector, int32>& P : Map)
		{
			const int32 Ps = FMath::Clamp(P.Value, 1, 3);
			for (const FIntVector& D : Dirs)
			{
				const FIntVector Cand(
					P.Key.X + (D.X > 0 ? Ps : (D.X < 0 ? -NewSide : 0)),
					P.Key.Y + (D.Y > 0 ? Ps : (D.Y < 0 ? -NewSide : 0)),
					P.Key.Z + (D.Z > 0 ? Ps : (D.Z < 0 ? -NewSide : 0)));
				if (!SpaceFree(CoreId, Cand, NewSide)) { continue; }
				const int32 Dist = FMath::Abs(Cand.X - Preferred.X) + FMath::Abs(Cand.Y - Preferred.Y)
					+ FMath::Abs(Cand.Z - Preferred.Z);
				if (Dist < BestD)
				{
					BestD = Dist;
					Best = Cand;
					bFound = true;
				}
			}
		}
	};
	if (const TMap<FIntVector, int32>* Map = CoreBlocks.Find(CoreId)) { Consider(*Map); }
	Consider(PendingVoxelSize);
	OutTarget = bFound ? Best : Preferred;
	return true;
}

void AOUCharacterPawn::ApplyServerBuildEdits()
{
	if (!Net)
	{
		return;
	}
	// В модели «несколько ядер» постройки не пересоздаются: сервер выдаёт новому
	// ядру новый id, а прежние ядра со своими блоками остаются. Флаг сброса лишь
	// закрываем (совместимость), но глобально блоки не снимаем.
	if (Net->bCoreResetPending)
	{
		Net->bCoreResetPending = false;
		// Сбрасываем только «отправленные, но не подтверждённые» координаты:
		// при смене активного ядра планировщик больше не должен на них опираться.
		BuiltCoords.Reset();
		PendingVoxelSize.Reset();
	}
	if (Net->PendingBuildEdits.Num() == 0)
	{
		return;
	}
	UWorld* World = GetWorld();
	if (!World)
	{
		return;
	}
	const int32 ActiveCore = Net->ActiveCoreId;
	// Забираем очередь правок (сервер уже авторитетно их применил). Каждая правка
	// привязана к своему ядру (`E.Entity` = id ядра).
	TSet<int32> TouchedCores;
	for (const FOUBuildEdit& E : Net->PendingBuildEdits)
	{
		const FIntVector Key(E.X, E.Y, E.Z);
		const int32 CoreId = E.Entity;
		TouchedCores.Add(CoreId);
		if (E.Action == 0) // Place
		{
			TObjectPtr<AOUBuildBlock>* Found = ServerBlocks.Find(Key);
			if (!Found)
			{
				// Размер блока — из правки (1/2/3 → 1.5/3/4.5 м). Позицию считаем
				// по размеру: центр куба смещён на N/2 от минимального угла.
				const int32 VSize = FMath::Clamp(static_cast<int32>(E.Size), 1, 3);
				const FVector Pos = VoxelWorld(FVector(E.X, E.Y, E.Z), VSize);
				// Блоки ориентируем по осям сетки (BuildGridRotation) — тогда на
				// сферической планете постройка ровная, а не «криво».
				AOUBuildBlock* Block = World->SpawnActor<AOUBuildBlock>(
					AOUBuildBlock::StaticClass(), FTransform(BuildGridRotation(), Pos));
				if (Block)
				{
					Block->BlockSize = static_cast<float>(OU_VOXEL_CM) * static_cast<float>(VSize);
					// Цвет берём с сервера: у ядра он свой по назначению (корабль/
					// динамическое/статичная), у обычных блоков — по материалу.
					Block->BlockColor = FLinearColor(E.R / 255.f, E.G / 255.f, E.B / 255.f);
					Block->ApplyLook();
					ServerBlocks.Add(Key, Block);
					BlockCoords.Add(Block, Key);
					CoreBlocks.FindOrAdd(CoreId).Add(Key, VSize);
					// Сервер подтвердил этот воксель — он уже учтён в CoreBlocks.
					PendingVoxelSize.Remove(Key);
					// Ядро определяем по координате из сервера (для лога).
					const bool bIsCore = (CoreId == ActiveCore && Key == Net->CoreCoord);
					// Счётчик — только по ПРИНЯТЫМ сервером блокам АКТИВНОГО ядра
					// (не по кликам): так он честно показывает, что реально стоит.
					if (CoreId == ActiveCore)
					{
						++BuiltCount;
					}
					UE_LOG(LogOUChar, Log, TEXT("Авторитетный блок (%d,%d,%d) размера %d ядра %d%s от сервера"),
						E.X, E.Y, E.Z, VSize, CoreId, bIsCore ? TEXT(" [ЯДРО]") : TEXT(""));
					if (bIsCore)
					{
						const FVector CamPos = Camera ? Camera->GetComponentLocation() : GetActorLocation();
						UE_LOG(LogOUChar, Log, TEXT("Ядро: мир %s, игрок %s, дист до камеры %.0f см"),
							*Pos.ToCompactString(), *CamPos.ToCompactString(),
							FVector::Dist(Pos, CamPos));
					}
				}
			}
		}
		else if (E.Action == 1) // Remove
		{
			if (TObjectPtr<AOUBuildBlock>* Found = ServerBlocks.Find(Key))
			{
				if (*Found)
				{
					BlockCoords.Remove(*Found);
					(*Found)->Destroy();
				}
				ServerBlocks.Remove(Key);
				PendingVoxelSize.Remove(Key);
				if (TMap<FIntVector, int32>* Map = CoreBlocks.Find(CoreId)) { Map->Remove(Key); }
				if (CoreId == ActiveCore && BuiltCount > 0)
				{
					--BuiltCount;
				}
			}
		}
		else if (E.Action == 2) // Paint
		{
			if (TObjectPtr<AOUBuildBlock>* Found = ServerBlocks.Find(Key))
			{
				if (*Found)
				{
					(*Found)->BlockColor = FLinearColor(E.R / 255.f, E.G / 255.f, E.B / 255.f);
					(*Found)->ApplyLook();
				}
			}
		}
		LastRevisionByCore.FindOrAdd(CoreId) = E.Revision;
	}
	// Подтверждаем приём по каждому ядру — сервер не будет слать эти правки снова.
	for (int32 CoreId : TouchedCores)
	{
		const int64 Rev = LastRevisionByCore.FindRef(CoreId);
		Net->SendBuildAck(CoreId, Rev);
	}
	Net->PendingBuildEdits.Reset();
}

void AOUCharacterPawn::DigTerrain()
{
	UWorld* World = GetWorld();
	if (!World)
	{
		return;
	}
	// В режиме строительства ЛКМ ставит блок (BuildPlace), а не копает.
	if (bBuildMenuOpen)
	{
		return;
	}
	// Направление копания — НЕ курс камеры (курс — это касательная), а точка на
	// земле под прицелом. Иначе яма уходила бы «за горизонт»: направление
	// трактовалось как радиус от центра планеты. Ищем пересечение луча взгляда с
	// рельефом (маршем), затем берём направление этой точки от центра планеты.
	FVector DirUE = (Camera ? Camera->GetForwardVector() : GetActorForwardVector()).GetSafeNormal();
	AOUVoxelPlanet* Planet = AOUVoxelPlanet::FindHomePlanet(World);
	if (!Planet || DirUE.IsNearlyZero())
	{
		return;
	}
	{
		const FVector Origin = Camera ? Camera->GetComponentLocation() : GetActorLocation();
		const double R = 500000.0 * OUInterop::ServerUnitToCm;   // радиус планеты, см
		// Марш вдоль луча: ищем, где луч приблизится к поверхности на шаг.
		double T = 0.0;
		FVector HitDir = (Origin + DirUE * 100.f).GetSafeNormal();
		bool bHit = false;
		double PrevGap = 1e30;
		double PrevT = 0.0;
		for (int32 Step = 0; Step < 400; ++Step)
		{
			const double TNow = 40.0 * Step;               // шаг 40 см
			const FVector P = Origin + DirUE * TNow;
			const FVector DirP = P.GetSafeNormal();
			if (DirP.IsNearlyZero())
			{
				break;
			}
			const double Surf = static_cast<double>(Planet->SurfaceRadiusAt(DirP));
			const double Gap = P.Size() - Surf;            // >0 над землёй
			if (Gap <= 0.0)
			{
				// Пересекли поверхность между PrevT и TNow — берём эту точку.
				HitDir = DirP;
				bHit = true;
				T = TNow;
				break;
			}
			if (Gap > PrevGap && Step > 3)
			{
				// Луч пошёл вверх и не пересечёт землю — копаем в самой близкой точке.
				HitDir = (Origin + DirUE * PrevT).GetSafeNormal();
				bHit = true;
				break;
			}
			PrevGap = Gap;
			PrevT = TNow;
		}
		if (bHit && !HitDir.IsNearlyZero())
		{
			DirUE = HitDir;
		}
		(void)T;
	}
	// Направление и радиус — в СЕРВЕРНЫХ единицах (1 ед = 100 см).
	const FVector DirServer = OUInterop::UEDirToServer(DirUE).GetSafeNormal();
	const double RadiusServer = DigRadiusCm / OUInterop::ServerUnitToCm;
	const double DepthServer = DigDepthCm / OUInterop::ServerUnitToCm;
	const double R = 500000.0;   // радиус планеты в серверных единицах
	// Косинус углового радиуса воронки: чем больше радиус, тем меньше cos.
	const double CosRadius = FMath::Clamp(1.0 - (RadiusServer * RadiusServer) / (2.0 * R * R), -1.0, 1.0);

	Net = Net ? Net : FindComponentByClass<UOUClientNet>();
	if (Net && Net->IsConnected())
	{
		// Слот воронки — по направлению: повторный удар рядом углубляет ту же
		// яму. Слот ищется по «квантованному» направлению. Слот — низкие 6 бит.
		const int32 Slot = static_cast<int32>(FMath::Abs(
			FMath::RoundToInt(DirServer.X * 1000.f) * 73
			+ FMath::RoundToInt(DirServer.Y * 1000.f) * 19
			+ FMath::RoundToInt(DirServer.Z * 1000.f) * 7)) % 64;
		Net->SendDig(Slot, DirServer.X, DirServer.Y, DirServer.Z, CosRadius, DepthServer);
		UE_LOG(LogOUChar, Log, TEXT("Копание: слот %d, радиус %.0f см, глубина %.0f см"),
			Slot, DigRadiusCm, DigDepthCm);
		// Обучение: копание.
		if (AOUPlayerMenuController* MenuD = Cast<AOUPlayerMenuController>(GetController()))
		{
			MenuD->NotifyTutorial(TEXT("dig"));
		}
	}
}

void AOUCharacterPawn::ApplyServerDigs()
{
	// Забираем воронки, пришедшие от сервера, и применяем их к планете.
	if (!Net)
	{
		return;
	}
	if (Net->PendingDents.Num() == 0)
	{
		return;
	}
	if (AOUVoxelPlanet* Planet = AOUVoxelPlanet::FindHomePlanet(GetWorld()))
	{
		for (const FOUTerrainDent& D : Net->PendingDents)
		{
			Planet->ApplyDent(D.Slot, D.X, D.Y, D.Z, D.CosRadius, D.Depth);
		}
	}
	Net->PendingDents.Reset();
}

void AOUCharacterPawn::ToggleMarket()
{
	if (bBuildMenuOpen)
	{
		return;
	}
	// Обучение: посещение рынка.
	if (AOUPlayerMenuController* MenuM = Cast<AOUPlayerMenuController>(GetController()))
	{
		MenuM->NotifyTutorial(TEXT("market"));
	}
	AOUPlayerMenuController* Menu = Cast<AOUPlayerMenuController>(
		GetController() ? GetController() : nullptr);
	if (!Menu)
	{
		Menu = Cast<AOUPlayerMenuController>(GetWorld() ? GetWorld()->GetFirstPlayerController() : nullptr);
	}
	if (Menu)
	{
		Menu->bMarketOpen = !Menu->bMarketOpen;
		// Пока рынок открыт — курсор свободен (можно кликать по UI).
		Menu->SetMenuCursor(Menu->bMarketOpen);
		UE_LOG(LogOUChar, Log, TEXT("Рынок: %s"), Menu->bMarketOpen ? TEXT("открыт") : TEXT("закрыт"));
	}
}

void AOUCharacterPawn::ToggleInventory()
{
	// Окно инвентаря/трюма (клавиша I): сетка предметов, объём и масса.
	if (bBuildMenuOpen)
	{
		return;
	}
	AOUPlayerMenuController* Menu = Cast<AOUPlayerMenuController>(
		GetController() ? GetController() : nullptr);
	if (!Menu)
	{
		Menu = Cast<AOUPlayerMenuController>(GetWorld() ? GetWorld()->GetFirstPlayerController() : nullptr);
	}
	if (Menu)
	{
		const bool bWasOpen = Menu->bInventoryOpen;
		Menu->CloseAllWindows();
		Menu->bInventoryOpen = !bWasOpen;
		Menu->SetMenuCursor(Menu->bInventoryOpen);
		UE_LOG(LogOUChar, Log, TEXT("Инвентарь: %s"), Menu->bInventoryOpen ? TEXT("открыт") : TEXT("закрыт"));
	}
}

void AOUCharacterPawn::ToggleCraft()
{
	// Окно производства (клавиша Q): рецепты и заказ изготовления.
	if (bBuildMenuOpen)
	{
		return;
	}
	AOUPlayerMenuController* Menu = Cast<AOUPlayerMenuController>(
		GetController() ? GetController() : nullptr);
	if (!Menu)
	{
		Menu = Cast<AOUPlayerMenuController>(GetWorld() ? GetWorld()->GetFirstPlayerController() : nullptr);
	}
	if (Menu)
	{
		const bool bWasOpen = Menu->bCraftOpen;
		Menu->CloseAllWindows();
		Menu->bCraftOpen = !bWasOpen;
		Menu->SetMenuCursor(Menu->bCraftOpen);
		UE_LOG(LogOUChar, Log, TEXT("Производство: %s"), Menu->bCraftOpen ? TEXT("открыто") : TEXT("закрыто"));
	}
}

void AOUCharacterPawn::ToggleMap()
{
	// Окно карты (клавиша Tab): территории, тела системы, зум.
	if (bBuildMenuOpen)
	{
		return;
	}
	AOUPlayerMenuController* Menu = Cast<AOUPlayerMenuController>(
		GetController() ? GetController() : nullptr);
	if (!Menu)
	{
		Menu = Cast<AOUPlayerMenuController>(GetWorld() ? GetWorld()->GetFirstPlayerController() : nullptr);
	}
	if (Menu)
	{
		// Пока открыт ввод чата, Tab занят сменой канала — карту не трогаем.
		if (Menu->bChatInputOpen)
		{
			return;
		}
		const bool bWasOpen = Menu->bMapOpen;
		Menu->CloseAllWindows();
		Menu->bMapOpen = !bWasOpen;
		Menu->SetMenuCursor(Menu->bMapOpen);
		UE_LOG(LogOUChar, Log, TEXT("Карта: %s"), Menu->bMapOpen ? TEXT("открыта") : TEXT("закрыта"));
	}
}

void AOUCharacterPawn::ToggleEventLog()
{
	// Окно лога событий (клавиша `): прокручиваемый лог и уведомления.
	if (bBuildMenuOpen)
	{
		return;
	}
	AOUPlayerMenuController* Menu = Cast<AOUPlayerMenuController>(
		GetController() ? GetController() : nullptr);
	if (!Menu)
	{
		Menu = Cast<AOUPlayerMenuController>(GetWorld() ? GetWorld()->GetFirstPlayerController() : nullptr);
	}
	if (Menu)
	{
		const bool bWasOpen = Menu->bEventLogOpen;
		Menu->CloseAllWindows();
		Menu->bEventLogOpen = !bWasOpen;
		Menu->SetMenuCursor(Menu->bEventLogOpen);
		UE_LOG(LogOUChar, Log, TEXT("Лог событий: %s"), Menu->bEventLogOpen ? TEXT("открыт") : TEXT("закрыт"));
	}
}

void AOUCharacterPawn::ToggleContacts()
{
	// Окно контактов/онлайн-игроков (клавиша ,).
	if (bBuildMenuOpen)
	{
		return;
	}
	AOUPlayerMenuController* Menu = Cast<AOUPlayerMenuController>(
		GetController() ? GetController() : nullptr);
	if (!Menu)
	{
		Menu = Cast<AOUPlayerMenuController>(GetWorld() ? GetWorld()->GetFirstPlayerController() : nullptr);
	}
	if (Menu)
	{
		const bool bWasOpen = Menu->bContactsOpen;
		Menu->CloseAllWindows();
		Menu->bContactsOpen = !bWasOpen;
		Menu->SetMenuCursor(Menu->bContactsOpen);
		UE_LOG(LogOUChar, Log, TEXT("Контакты: %s"), Menu->bContactsOpen ? TEXT("открыты") : TEXT("закрыты"));
	}
}

void AOUCharacterPawn::CaptureTerritory()
{
	// Захват: просим сервер объявить текущую зону нашей (клавиша T).
	Net = Net ? Net : FindComponentByClass<UOUClientNet>();
	if (!Net || !Net->bTerritoriesReceived)
	{
		return;
	}
	if (Net->CurrentTerritoryId == 0)
	{
		UE_LOG(LogOUChar, Log, TEXT("Территория: игрок вне зон"));
		return;
	}
	Net->SendCapture(Net->CurrentTerritoryId);
	UE_LOG(LogOUChar, Log, TEXT("Территория: запрос захвата зоны %d"), Net->CurrentTerritoryId);
}

void AOUCharacterPawn::ToggleScripts()
{	// Окно скриптов объекта (клавиша L): автопилоты, турель, логистика.
	if (bBuildMenuOpen)
	{
		return;
	}
	AOUPlayerMenuController* Menu = Cast<AOUPlayerMenuController>(GetController());
	if (!Menu)
	{
		Menu = Cast<AOUPlayerMenuController>(GetWorld() ? GetWorld()->GetFirstPlayerController() : nullptr);
	}
	if (Menu)
	{
		// Взаимоисключающие окна: рынок и скрипты не открыты одновременно.
		Menu->bScriptsOpen = !Menu->bScriptsOpen;
		if (Menu->bScriptsOpen)
		{
			Menu->bMarketOpen = false;
			Menu->bWorkshopOpen = false;
			Menu->bOrgOpen = false;
		}
		Menu->SetMenuCursor(Menu->bScriptsOpen);
		UE_LOG(LogOUChar, Log, TEXT("Скрипты: %s"), Menu->bScriptsOpen ? TEXT("открыто") : TEXT("закрыто"));
	}
}

void AOUCharacterPawn::ToggleWorkshop()
{
	// Окно мастерской (клавиша K): обмен шаблонами построек.
	if (bBuildMenuOpen)
	{
		return;
	}
	AOUPlayerMenuController* Menu = Cast<AOUPlayerMenuController>(GetController());
	if (!Menu)
	{
		Menu = Cast<AOUPlayerMenuController>(GetWorld() ? GetWorld()->GetFirstPlayerController() : nullptr);
	}
	if (Menu)
	{
		Menu->bWorkshopOpen = !Menu->bWorkshopOpen;
		if (Menu->bWorkshopOpen)
		{
			Menu->bMarketOpen = false;
			Menu->bScriptsOpen = false;
			Menu->bOrgOpen = false;
		}
		Menu->SetMenuCursor(Menu->bWorkshopOpen);
		UE_LOG(LogOUChar, Log, TEXT("Мастерская: %s"), Menu->bWorkshopOpen ? TEXT("открыто") : TEXT("закрыто"));
	}
}

void AOUCharacterPawn::ToggleOrg()
{
	// Окно организации (клавиша O): состав, ранги, дипломатия.
	if (bBuildMenuOpen)
	{
		return;
	}
	AOUPlayerMenuController* Menu = Cast<AOUPlayerMenuController>(GetController());
	if (!Menu)
	{
		Menu = Cast<AOUPlayerMenuController>(GetWorld() ? GetWorld()->GetFirstPlayerController() : nullptr);
	}
	if (Menu)
	{
		Menu->bOrgOpen = !Menu->bOrgOpen;
		if (Menu->bOrgOpen)
		{
			Menu->bMarketOpen = false;
			Menu->bScriptsOpen = false;
			Menu->bWorkshopOpen = false;
		}
		Menu->SetMenuCursor(Menu->bOrgOpen);
		UE_LOG(LogOUChar, Log, TEXT("Организация: %s"), Menu->bOrgOpen ? TEXT("открыто") : TEXT("закрыто"));
	}
}

void AOUCharacterPawn::ToggleTalents()
{
	// Окно талантов (клавиша P): опыт, уровень, навыки.
	if (bBuildMenuOpen) { return; }
	AOUPlayerMenuController* Menu = Cast<AOUPlayerMenuController>(GetController());
	if (!Menu)
	{
		Menu = Cast<AOUPlayerMenuController>(GetWorld() ? GetWorld()->GetFirstPlayerController() : nullptr);
	}
	if (Menu)
	{
		Menu->bTalentsOpen = !Menu->bTalentsOpen;
		if (Menu->bTalentsOpen)
		{
			Menu->bMarketOpen = false;
			Menu->bScriptsOpen = false;
			Menu->bWorkshopOpen = false;
			Menu->bOrgOpen = false;
			// Обучение: открытие окна персонажа.
			Menu->NotifyTutorial(TEXT("talent"));
		}
		Menu->SetMenuCursor(Menu->bTalentsOpen);
		UE_LOG(LogOUChar, Log, TEXT("Таланты: %s"), Menu->bTalentsOpen ? TEXT("открыто") : TEXT("закрыто"));
	}
}

void AOUCharacterPawn::MarketQuickTrade(bool bSell)
{
	Net = Net ? Net : FindComponentByClass<UOUClientNet>();
	if (!Net || !Net->IsConnected())
	{
		return;
	}
	// Руда — предмет 1. Быстрая сделка по 20 кредитов за единицу.
	Net->SendMarketOrder(bSell ? 1 : 0, /*руда=*/1, 20, 1);
	UE_LOG(LogOUChar, Log, TEXT("Рынок: %s руды по 20"), bSell ? TEXT("продать") : TEXT("купить"));
}

void AOUCharacterPawn::FireWeapon()
{	UWorld* World = GetWorld();
	if (!World || !GunMesh || FireTimer > 0.f)
	{
		return;
	}
	FireTimer = FireCooldown;

	// Обучение: участие в бою.
	if (AOUPlayerMenuController* MenuF = Cast<AOUPlayerMenuController>(GetController()))
	{
		MenuF->NotifyTutorial(TEXT("combat"));
	}

	// Точка вылета — у дула оружия; направление — «вперёд» персонажа (по курсу).
	const FVector Muzzle = MuzzleFlash ? MuzzleFlash->GetComponentLocation() : GunMesh->GetComponentLocation();
	const FVector Dir = GetActorForwardVector().GetSafeNormal();

	if (AOUProjectile* Bolt = World->SpawnActor<AOUProjectile>(
		AOUProjectile::StaticClass(), FTransform(Dir.Rotation(), Muzzle)))
	{
		Bolt->Launch(Dir);
		LastBolt = Bolt;
	}
	// Вспышка у дула.
	if (MuzzleFlash)
	{
		MuzzleFlash->SetVisibility(true);
		MuzzleTimer = MuzzleFlashTime;
	}
}

void AOUCharacterPawn::AnimateLocomotion(float DeltaSeconds, float HorizontalSpeed)
{
	// Доля скорости от максимальной (0 — стоим, 1 — бежим на полной).
	const float Target = FMath::Clamp(HorizontalSpeed / FMath::Max(1.f, WalkSpeed), 0.f, 1.f);
	// Плавно подмешиваем, чтобы анимация не «дёргалась» на старте/стопе.
	RunBlend = FMath::FInterpTo(RunBlend, Target, DeltaSeconds, 8.f);

	// В воздухе конечности вытянуты, шаг замирает.
	if (!bGrounded)
	{
		RunBlend *= 0.35f;
	}

	if (RunBlend <= 0.01f)
	{
		// Стоим: плавно возвращаем конечности в нейтраль.
		auto Relax = [&](USceneComponent* P)
		{
			if (P) { P->SetRelativeRotation(FRotator(0.f, 0.f, 0.f)); }
		};
		Relax(ShoulderL); Relax(ShoulderR); Relax(HipL); Relax(HipR);
		if (BodyMesh)
		{
			BodyMesh->SetRelativeLocation(FVector(0.f, 0.f, 10.f));
			BodyMesh->SetRelativeRotation(FRotator(0.f, 0.f, 0.f));
		}
		return;
	}

	// Фаза шага растёт тем быстрее, чем выше скорость.
	StridePhase += DeltaSeconds * StrideFrequency * (0.6f + RunBlend) * 2.f * PI;
	// Ограничиваем фазу, чтобы не терять точность float.
	if (StridePhase > 2.f * PI) { StridePhase -= 2.f * PI; }

	const float Amp = StrideAmplitude * RunBlend;
	const float S = FMath::Sin(StridePhase);

	// Ноги: противоположные фазы (левая вперёд — правая назад).
	if (HipL) { HipL->SetRelativeRotation(FRotator(S * Amp, 0.f, 0.f)); }
	if (HipR) { HipR->SetRelativeRotation(FRotator(-S * Amp, 0.f, 0.f)); }

	// Руки: в противофазе к ногам (как при настоящем беге).
	if (ShoulderL) { ShoulderL->SetRelativeRotation(FRotator(-S * Amp * 0.8f, 0.f, 0.f)); }
	if (ShoulderR) { ShoulderR->SetRelativeRotation(FRotator(S * Amp * 0.8f, 0.f, 0.f)); }

	// Корпус слегка покачивается вверх-вниз в такт шагам.
	if (BodyMesh)
	{
		const float Bob = FMath::Sin(StridePhase * 2.f) * 3.5f * RunBlend;
		BodyMesh->SetRelativeLocation(FVector(0.f, 0.f, 10.f + Bob));
		BodyMesh->SetRelativeRotation(FRotator(0.f, 0.f, FMath::Sin(StridePhase) * 4.f * RunBlend));
	}
}

void AOUCharacterPawn::Tick(float DeltaSeconds)
{
	Super::Tick(DeltaSeconds);

	UWorld* World = GetWorld();
	if (!World)
	{
		return;
	}

	// Пауза-меню (ESC): персонаж не двигается и не смотрит, пока оно открыто.
	if (AOUPlayerMenuController* Menu = Cast<AOUPlayerMenuController>(GetController()))
	{
		if (Menu->IsPauseMenuOpen() || Menu->bChatInputOpen)
		{
			// Гасим накопленный ввод, чтобы при возврате не «рвануло».
			InputForward = 0.f;
			InputRight = 0.f;
			InputLookYaw = 0.f;
			InputLookPitch = 0.f;
			Velocity = FVector::ZeroVector;
			return;
		}
	}
	else if (AOUPlayerMenuController* Menu2 = Cast<AOUPlayerMenuController>(
		World->GetFirstPlayerController()))
	{
		if (Menu2->IsPauseMenuOpen() || Menu2->bChatInputOpen)
		{
			InputForward = 0.f;
			InputRight = 0.f;
			InputLookYaw = 0.f;
			InputLookPitch = 0.f;
			Velocity = FVector::ZeroVector;
			return;
		}
	}

	const FVector Pos = GetActorLocation();
	const FVector Up = Pos.GetSafeNormal();          // «вверх» — от центра планеты
	if (Up.IsNearlyZero())
	{
		return;
	}

	// Какая территория под ногами (для HUD и клавиши захвата T). Считаем
	// в начале тика, чтобы работало и в режиме обзора базы (ранний выход ниже).
	ResolveNet();
	if (Net && Net->IsConnected())
	{
		const FVector PosServerEarly = OUInterop::UEToServer(Pos);
		Net->UpdateTerritoryAt(PosServerEarly.X, PosServerEarly.Y, PosServerEarly.Z);
	}

	// Курс и тангаж (обзор мышью/стрелками).
	ViewYaw += InputLookYaw * 1.6f * DeltaSeconds;
	ViewPitch = FMath::Clamp(ViewPitch - InputLookPitch * 1.4f * DeltaSeconds, -1.4f, 1.4f);

	// Тестовый автоход: идём вперёд сами и понемногу поворачиваем — персонаж
	// описывает небольшой круг возле корабля, а на снимке видна анимация бега
	// (идти по прямой 45 с значило бы уйти за сотни метров от точки старта).
	// При авто-строительстве персонаж стоит на месте, чтобы блоки встали в ряд.
	const float FwdInput = (bAutoWalk && !bAutoBuild) ? 1.f : InputForward;
	if (bAutoWalk && !bAutoBuild)
	{
		ViewYaw += 1.5f * DeltaSeconds;
	}
	// Диагностика `-OUWalkInto`: идти строго вперёд (без поворота) — проверка,
	// что персонаж упирается в постройку и не проходит сквозь неё.
	const bool bWalkInto = FParse::Param(FCommandLine::Get(), TEXT("OUWalkInto"));
	// Развернуть курс НА ближайший блок, чтобы упереться именно в него.
	if (bWalkInto && ServerBlocks.Num() > 0)
	{
		AActor* Nearest = nullptr;
		float Best = TNumericLimits<float>::Max();
		for (const TPair<FIntVector, TObjectPtr<AOUBuildBlock>>& P : ServerBlocks)
		{
			if (!P.Value) { continue; }
			const float D = FVector::Dist(Pos, P.Value->GetActorLocation());
			if (D < Best) { Best = D; Nearest = P.Value; }
		}
		if (Nearest)
		{
			const FVector ToBlock = (Nearest->GetActorLocation() - Pos).GetSafeNormal();
			FVector TangDir = FVector::VectorPlaneProject(ToBlock, Up).GetSafeNormal();
			if (!TangDir.IsNearlyZero())
			{
				// Yaw в касательной плоскости относительно опорного направления.
				FVector RefW = FVector::CrossProduct(Up, FVector::UpVector);
				if (RefW.SizeSquared() < 1e-4f) { RefW = FVector::CrossProduct(Up, FVector::RightVector); }
				RefW = RefW.GetSafeNormal();
				const FVector TW = FVector::CrossProduct(RefW, Up).GetSafeNormal();
				const FVector BW = FVector::CrossProduct(Up, TW).GetSafeNormal();
				ViewYaw = FMath::Atan2(FVector::DotProduct(TangDir, BW), FVector::DotProduct(TangDir, TW));
			}
		}
	}
	const float FwdFinal = bWalkInto ? 1.f : FwdInput;
	// Обучение: первое движение (ходьба).
	if (FMath::Abs(FwdInput) > 0.1f || FMath::Abs(InputRight) > 0.1f)
	{
		if (AOUPlayerMenuController* MenuT = Cast<AOUPlayerMenuController>(GetController()))
		{
			MenuT->NotifyTutorial(TEXT("move"));
		}
	}

	// Базис на поверхности: forward — по касательной в сторону курса.
	FVector Ref = FVector::CrossProduct(Up, FVector::UpVector);
	if (Ref.SizeSquared() < 1e-4f)
	{
		Ref = FVector::CrossProduct(Up, FVector::RightVector);
	}
	Ref = Ref.GetSafeNormal();
	const FVector Tang = FVector::CrossProduct(Ref, Up).GetSafeNormal();

	const FVector Forward = (Tang * FMath::Cos(ViewYaw) + FVector::CrossProduct(Up, Tang) * FMath::Sin(ViewYaw)).GetSafeNormal();
	const FVector Right = FVector::CrossProduct(Up, Forward).GetSafeNormal();

	// Горизонтальная скорость от ввода (ходьба).
	const FVector Wish = (Forward * FwdFinal + Right * InputRight).GetSafeNormal();
	const FVector TargetHV = Wish * WalkSpeed;
	// Плавно тянем горизонтальную скорость к желаемой.
	FVector HV(0, 0, 0);
	HV = FVector::VectorPlaneProject(Velocity, Up);
	HV = FMath::VInterpTo(HV, FVector::VectorPlaneProject(TargetHV, Up), DeltaSeconds, 12.f);
	// Вертикальная (вдоль радиуса) составляющая сохраняется (падение/прыжок).
	const double VRadial = FVector::DotProduct(Velocity, Up);
	Velocity = HV + Up * VRadial;

	// Гравитация (вниз — к центру планеты).
	Velocity += -Up * Gravity * DeltaSeconds;

	// Прыжок.
	if (bJumpQueued && bGrounded)
	{
		Velocity += Up * JumpSpeed;
	}
	bJumpQueued = false;

	// Шаг позиции со свипом: персонаж не должен проходить сквозь постройки и
	// блоки редактора (у них есть коллизия). По рельефу ниже привязываемся
	// аналитически, но здания ловим обычным свипом UE.
	FVector NewPos = Pos + Velocity * DeltaSeconds;
	{
		FHitResult Hit;
		FCollisionQueryParams Params(SCENE_QUERY_STAT(OUCharMove), false, this);
		// ВАЖНО: не считаем «начальные перекрытия» стеной. Если капсула стоит на
		// площадке, свип каждый кадр видел бы перекрытие в момент 0 и обнулял бы
		// движение — персонаж «залипал». Нам нужны только реальные столкновения.
		Params.bFindInitialOverlaps = false;
		const bool bHit = World->SweepSingleByChannel(
			Hit, Pos, NewPos, FQuat::Identity, ECC_Pawn,
			FCollisionShape::MakeCapsule(CapsuleRadius, CapsuleHalfHeight), Params);
		if (bHit && Hit.Time > KINDA_SMALL_NUMBER)
		{
			NewPos = Pos + (NewPos - Pos) * Hit.Time;
			// Гасим скорость в направлении нормали стены (скользим вдоль неё).
			const FVector N = Hit.Normal;
			const double VN = FVector::DotProduct(Velocity, N);
			if (VN < 0.0)
			{
				Velocity -= N * VN;
			}
			// Стоим на полу (нормаль вверх): можно прыгать.
			if (FVector::DotProduct(N, Up) > 0.5)
			{
				bGrounded = true;
			}
			// Диагностика `-OUGhostLog`: столкновение с постройкой (персонаж
			// уперся в воксель и не проходит сквозь него).
			if (FParse::Param(FCommandLine::Get(), TEXT("OUGhostLog")))
			{
				static double LastColLog = 0.0;
				const double Now2 = World->GetTimeSeconds();
				if (Now2 - LastColLog > 0.5)
				{
					LastColLog = Now2;
					UE_LOG(LogOUChar, Log, TEXT("Столкновение: блокирует '%s' во времени %.2f"),
						*Hit.GetActor()->GetName(), Hit.Time);
				}
			}
		}
	}

	// Привязка к рельефу: не проваливаемся под поверхность. Плиты площадки —
	// только визуал, поэтому землёй остаётся аналитический рельеф планеты.
	const FVector Dir = NewPos.GetSafeNormal();
	const double Surface = static_cast<double>(SurfaceRadius(Dir)) + CapsuleHalfHeight;

	// Диагностика `-OUBaseOverview`: держим камеру над базой (обзор сверху),
	// иначе гравитация прижимает её к земле.
	if (FParse::Param(FCommandLine::Get(), TEXT("OUBaseOverview")))
	{
		const FVector Center = AOUVoxelPlanet::HomeSpawn(2.0f);
		const FVector Up2 = Center.GetSafeNormal();
		SetActorLocation(Up2 * (Center.Size() + 14000.f));
		SetActorRotation(FRotationMatrix::MakeFromXZ(FVector::CrossProduct(Up2, FVector::RightVector).GetSafeNormal(), Up2).Rotator());
		Velocity = FVector::ZeroVector;
		if (Camera)
		{
			Camera->SetRelativeRotation(FRotator(FMath::RadiansToDegrees(ViewPitch), 0.f, 0.f));
		}
		return;
	}

	const double Dist = NewPos.Size();
	if (Dist <= Surface)
	{
		NewPos = Dir * Surface;
		const double VRad = FVector::DotProduct(Velocity, Dir);
		if (VRad < 0.0)
		{
			Velocity -= Dir * VRad;   // гасим «в землю»
		}
		bGrounded = true;
	}
	else
	{
		bGrounded = false;
	}

	SetActorLocation(NewPos);

	// Гарантия непроходимости: если капсула всё же оказалась внутри вокселя
	// (например, блок поставили прямо на игрока), выдавливаем её по оси
	// наименьшего проникновения. Так персонаж никогда не «внутри» постройки.
	{
		const FBox Me = GetComponentsBoundingBox(/*bNonColliding=*/true);
		TArray<FOverlapResult> Overlaps;
		FCollisionQueryParams PP(SCENE_QUERY_STAT(OUDepenetrate), false, this);
		if (World->OverlapMultiByChannel(
			Overlaps, NewPos, FQuat::Identity, ECC_Pawn,
			FCollisionShape::MakeCapsule(CapsuleRadius, CapsuleHalfHeight), PP))
		{
			FVector Push = FVector::ZeroVector;
			for (const FOverlapResult& O : Overlaps)
			{
				const AActor* A = O.GetActor();
				if (!A || A == this || !A->IsA(AOUBuildBlock::StaticClass())) { continue; }
				const FBox B = A->GetComponentsBoundingBox(/*bNonColliding=*/true);
				// Проникновение по каждой оси: (Me+B)/2 центр, перекрытие [max(min),
				// min(max)]. Выбираем ось с наименьшим перекрытием и выталкиваем.
				const FVector Center = (Me.GetCenter() + B.GetCenter()) * 0.5;
				const FVector Overlap(
					FMath::Min(Me.Max.X, B.Max.X) - FMath::Max(Me.Min.X, B.Min.X),
					FMath::Min(Me.Max.Y, B.Max.Y) - FMath::Max(Me.Min.Y, B.Min.Y),
					FMath::Min(Me.Max.Z, B.Max.Z) - FMath::Max(Me.Min.Z, B.Min.Z));
				if (Overlap.X <= 0.f || Overlap.Y <= 0.f || Overlap.Z <= 0.f) { continue; }
				// Ось наименьшего проникновения.
				int32 Axis = 0;
				if (Overlap.Y < Overlap.X && Overlap.Y <= Overlap.Z) { Axis = 1; }
				else if (Overlap.Z < Overlap.X && Overlap.Z < Overlap.Y) { Axis = 2; }
				const double DirSign =
					((Axis == 0 ? Me.GetCenter().X : (Axis == 1 ? Me.GetCenter().Y : Me.GetCenter().Z))
					 >= (Axis == 0 ? B.GetCenter().X : (Axis == 1 ? B.GetCenter().Y : B.GetCenter().Z)))
					? 1.0 : -1.0;
				const double D = (Axis == 0 ? Overlap.X : (Axis == 1 ? Overlap.Y : Overlap.Z));
				Push += FVector(
					Axis == 0 ? DirSign * (D + 1.0) : 0.0,
					Axis == 1 ? DirSign * (D + 1.0) : 0.0,
					Axis == 2 ? DirSign * (D + 1.0) : 0.0);
			}
			if (!Push.IsNearlyZero())
			{
				SetActorLocation(NewPos + Push);
			}
		}
	}

	// Локальный детальный участок поверхности: перестраиваем, когда игрок ушёл
	// от центра прошлого участка (иначе мелкий рельеф не виден — глобальный
	// меш планеты слишком грубый).
	if (AOUVoxelPlanet* Planet = AOUVoxelPlanet::FindHomePlanet(World))
	{
		if (FVector::DistSquared(NewPos, LocalTerrainCenter) >
			FMath::Square(Planet->LocalExtentCm * 0.5f))
		{
			LocalTerrainCenter = NewPos;
			Planet->RebuildLocal(NewPos);
		}
	}

	// Диагностика `-OUGhostLog`: раз в секунду — проверка, есть ли блоки рядом с
	// персонажем (перекрытие по каналу Pawn). Прямое доказательство, что блоки
	// вообще имеют коллизию и персонаж упирается в них, а не проходит насквозь.
	if (FParse::Param(FCommandLine::Get(), TEXT("OUGhostLog")))
	{
		static double LastProbe = 0.0;
		const double Now3 = World->GetTimeSeconds();
		if (Now3 - LastProbe > 2.0)
		{
			LastProbe = Now3;
			FCollisionQueryParams PP(SCENE_QUERY_STAT(OUWallProbe), false, this);
			PP.bFindInitialOverlaps = true;
			TArray<FOverlapResult> Overlaps;
			World->OverlapMultiByChannel(
				Overlaps, NewPos, FQuat::Identity, ECC_Pawn,
				FCollisionShape::MakeSphere(3000.f), PP);
			int32 Blocks = 0;
			float Nearest = -1.f;
			for (const FOverlapResult& O : Overlaps)
			{
				if (O.GetActor() && O.GetActor()->GetClass()->GetName().Contains(TEXT("BuildBlock")))
				{
					++Blocks;
					const float D = FVector::Dist(NewPos, O.GetActor()->GetActorLocation());
					if (Nearest < 0.f || D < Nearest) { Nearest = D; }
				}
			}
			UE_LOG(LogOUChar, Log, TEXT("Блоков рядом (коллизия): %d, ближайший %.0f см"),
				Blocks, Nearest);
		}
	}

	// Зажатие F — возврат в корабль, если он рядом.
	if (bExitHeld)
	{
		ExitHoldTime += DeltaSeconds;
		if (ExitHoldTime >= 0.6f)
		{
			bExitHeld = false;
			if (Ship)
			{
				// Близко к кораблю? Тогда садимся обратно.
				const double DistToShip = FVector::Dist(GetActorLocation(), Ship->GetActorLocation());
				if (DistToShip < 700.0)
				{
					Ship->EnterShip(this);
					return;
				}
			}
		}
	}

	// Ориентация тела: «верх» — радиус, «вперёд» — по курсу.
	SetActorRotation(FRotationMatrix::MakeFromXZ(Forward, Up).Rotator());	// Камера: в ЛОКАЛЬНЫХ осях пешки (локальный X = курс, Z = вверх) достаточно
	// относительного поворота по тангажу — без мировой математики, которая
	// конфликтует с родительским трансформом.
	if (Camera)
	{
		Camera->SetRelativeRotation(FRotator(FMath::RadiansToDegrees(ViewPitch), 0.f, 0.f));
		// Угол обзора — из настроек (меняется в пауза-меню).
		Camera->SetFieldOfView(CameraFov);
	}

	// Анимация бега/ходьбы (процедурная, по горизонтальной скорости).
	const float HSpd = FVector::VectorPlaneProject(Velocity, Up).Size();
	AnimateLocomotion(DeltaSeconds, HSpd);

	// Авторитетное строительство: применяем правки, пришедшие от сервера.
	ResolveNet();
	ApplyServerBuildEdits();
	// Призрак блока под прицелом (меню строительства, как в Dual Universe).
	UpdateBuildGhost();
	// Радиус постройки (каркас области ядра) — виден при наведении на ядро.
	UpdateCoreGrid();
	// Авторитетное копание: применяем воронки, пришедшие от сервера.
	ApplyServerDigs();

	// Отправляем свою позу пешего персонажа другим игрокам.
	if (Net && Net->IsConnected())
	{
		const float HSpdNet = FVector::VectorPlaneProject(Velocity, Up).Size();
		// Поза — в серверных единицах (как позиция корабля).
		const FVector PosServer = OUInterop::UEToServer(GetActorLocation());
		Net->SendCharacter(PosServer, ViewYaw, HSpdNet > 50.f, /*bActive=*/true);
		// Какая территория под ногами (для HUD и клавиши захвата T).
		Net->UpdateTerritoryAt(PosServer.X, PosServer.Y, PosServer.Z);
	}

	// Оружие: кулдаун, угасание вспышки, автоогонь для тестов.
	FireTimer = FMath::Max(0.f, FireTimer - DeltaSeconds);
	if (MuzzleTimer > 0.f)
	{
		MuzzleTimer -= DeltaSeconds;
		if (MuzzleTimer <= 0.f && MuzzleFlash)
		{
			MuzzleFlash->SetVisibility(false);
		}
	}
	if (bAutoFire)
	{
		FireWeapon();
	}

	// Тестовое копание (`-OUAutoDig`): несколько ударов вперёд — проверка
	// сервер-авторитетного копания и перестройки рельефа.
	if (FParse::Param(FCommandLine::Get(), TEXT("OUAutoDig")))
	{
		DigTimer += DeltaSeconds;
		if (DigTimer >= 1.2f && DigCount < 6)
		{
			DigTimer = 0.f;
			++DigCount;
			DigTerrain();
		}
	}

	// Тестовая торговля (`-OUAutoTrade`): добыть руду, выставить продажу,
	// затем купить — проверка рынка вживую.
	if (FParse::Param(FCommandLine::Get(), TEXT("OUAutoTrade")))
	{
		TradeTimer += DeltaSeconds;
		if (TradeTimer >= 1.5f)
		{
			TradeTimer = 0.f;
			++TradeCount;
			// Открыть окно рынка, чтобы оно попало на отложенный снимок.
			if (TradeCount == 1)
			{
				ToggleMarket();
			}
			if (TradeCount <= 6)
			{
				DigTerrain();   // накопить руду
			}
			else if (TradeCount == 7)
			{
				MarketQuickTrade(true);   // продать руду по 20
			}
			else if (TradeCount == 9)
			{
				MarketQuickTrade(false);  // купить руду по 20
			}
		}
	}

	// Тестовый крафт (`-OUAutoCraft`): открыть рынок, накопить руду, изготовить
	// металл и показать результат — проверка производства вживую.
	if (FParse::Param(FCommandLine::Get(), TEXT("OUAutoCraft")))
	{
		CraftTimer += DeltaSeconds;
		if (CraftTimer >= 1.4f)
		{
			CraftTimer = 0.f;
			++CraftCount;
			if (CraftCount == 1)
			{
				ToggleMarket();
			}
			if (CraftCount <= 6)
			{
				DigTerrain();   // накопить руду для рецепта «выплавка металла» (2 руды → 1 металл)
			}
			else if (CraftCount >= 8 && Net && Net->Recipes.Num() > 0)
			{
				// Рецепт с выходом «металл» (обычно первый в цепочке).
				Net->SendCraft(Net->Recipes[0].Id);
			}
		}
	}

	// Тестовая привязка скрипта (`-OUAutoScript`): открыть окно скриптов и
	// привязать «крейсерский» автопилот, затем периодически логировать.
	if (FParse::Param(FCommandLine::Get(), TEXT("OUAutoScript")))	{
		ScriptTimer += DeltaSeconds;
		if (ScriptTimer >= 1.5f && ScriptCount < 20)
		{
			ScriptTimer = 0.f;
			++ScriptCount;
			if (ScriptCount == 1)
			{
				ToggleScripts();
			}
			if (ScriptCount == 3 && Net && Net->ScriptTemplates.Num() > 0)
			{
				// Выбираем «крейсерский» автопилот (id 2) — он пишет в log(),
				// так что на снимке виден и дисплей.
				int32 Pick = 0;
				for (int32 i = 0; i < Net->ScriptTemplates.Num(); ++i)
				{
					if (Net->ScriptTemplates[i].Id == 2) { Pick = i; break; }
				}
				Net->SendSetScript(Net->ScriptTemplates[Pick].Id);
			}
		}
	}

	// Тестовая постановка шаблона (`-OUAutoBlueprint`): открыть строительство и
	// поставить шаблон «Куб 5×5×5» рядом с базой — проверка blueprints вживую.
	if (FParse::Param(FCommandLine::Get(), TEXT("OUAutoBlueprint")))	{
		BlueprintTimer += DeltaSeconds;
		if (BlueprintTimer >= 2.f && BlueprintCount < 10)
		{
			BlueprintTimer = 0.f;
			++BlueprintCount;
			if (!bBuildMenuOpen)
			{
				ToggleBuildMenu();
			}
			if (BlueprintCount == 4 && Net && Net->Blueprints.Num() > 0)
			{
				// Ставим куб (id 2) в якорь сетки — прямо перед игроком.
				Net->SendPlaceBlueprint(2, 0, 0, 0, Net->OwnEntityId);
				UE_LOG(LogOUChar, Log, TEXT("Шаблон: поставлен куб 5×5×5"));
			}
		}
	}

	// Тест мастерской (`-OUAutoWorkshop`): открыть окно, опубликовать постройку,
	// затем показать витрину — проверка обмена вживую.
	if (FParse::Param(FCommandLine::Get(), TEXT("OUAutoWorkshop")))
	{
		WorkshopTimer += DeltaSeconds;
		if (WorkshopTimer >= 1.5f && WorkshopCount < 12)
		{
			WorkshopTimer = 0.f;
			++WorkshopCount;
			if (WorkshopCount == 1)
			{
				ToggleWorkshop();
			}
			// Сначала строим что-то, чтобы было что публиковать.
			if (WorkshopCount >= 3 && WorkshopCount <= 6)
			{
				if (!bBuildMenuOpen) { ToggleBuildMenu(); }
				PlaceBuildBlock();
			}
			if (WorkshopCount == 8 && Net && Net->OwnEntityId != 0)
			{
				Net->SendPublishBlueprint(TEXT("Шаблон TestPilot"), 600);
				UE_LOG(LogOUChar, Log, TEXT("Мастерская: публикую постройку"));
			}
		}
	}

	// Тест организации (`-OUAutoOrg`): открыть окно и создать гильдию.
	if (FParse::Param(FCommandLine::Get(), TEXT("OUAutoOrg")))
	{
		OrgTimer += DeltaSeconds;
		if (OrgTimer >= 1.5f && OrgCount < 8)
		{
			OrgTimer = 0.f;
			++OrgCount;
			if (OrgCount == 1)
			{
				ToggleOrg();
			}
			if (OrgCount == 3 && Net && Net->OwnEntityId != 0 && Net->OrgId == 0)
			{
				Net->SendCreateOrg(TEXT("Легион TestPilot"), TEXT("LEG"));
				UE_LOG(LogOUChar, Log, TEXT("Организация: создаю"));
			}
		}
	}

	// Тест чата (`-OUAutoChat`): отправить пару сообщений в глобальный чат.
	if (FParse::Param(FCommandLine::Get(), TEXT("OUAutoChat")))
	{
		ChatTimer += DeltaSeconds;
		if (ChatTimer >= 2.f && ChatCount < 6)
		{
			ChatTimer = 0.f;
			++ChatCount;
			if (Net && Net->IsConnected())
			{
				Net->SendChat(1, FString::Printf(TEXT("Привет, мир! Сообщение %d"), ChatCount));
			}
		}
	}

	// Тест талантов (`-OUAutoTalents`): копать (набрать опыт), открыть окно.
	if (FParse::Param(FCommandLine::Get(), TEXT("OUAutoTalents")))
	{
		TalentTimer += DeltaSeconds;
		if (TalentTimer >= 0.4f && TalentCount < 55)
		{
			TalentTimer = 0.f;
			++TalentCount;
			if (TalentCount <= 40)
			{
				DigTerrain();   // накопить опыт до уровня (100 опыта = ~34 копания)
			}
			else if (TalentCount == 45)
			{
				ToggleTalents();
			}
		}
	}

	if (FParse::Param(FCommandLine::Get(), TEXT("OUVoxels")))
	{
		// Диагностика: открыть окно размеров вокселя (снимок окна).
		if (AOUPlayerMenuController* MV = Cast<AOUPlayerMenuController>(GetController()))
		{
			MV->bVoxelsOpen = true;
			MV->SetMenuCursor(true);
		}
	}

	// Диагностика `-OUCore`: открыть стройку, выбрать слот «ЯДРО» и поставить его
	// под прицелом (проверка ядер и призрачной сетки одним прогоном).
	if (FParse::Param(FCommandLine::Get(), TEXT("OUCore")))
	{
		CoreTimer += DeltaSeconds;
		if (CoreTimer >= 2.0f && CoreCount < 3)
		{
			CoreTimer = 0.f;
			++CoreCount;
			if (CoreCount == 1)
			{
				if (!bBuildMenuOpen) { ToggleBuildMenu(); }
				SelectBuildPart((int32)EOUBuildPart::Core);
				// Диагностика `-OUCoreBig`: выбрать большой размер ядра (64).
				if (FParse::Param(FCommandLine::Get(), TEXT("OUCoreBig")))
				{
					CoreSizeSel = 2;
				}
			}
			else if (CoreCount == 2)
			{
				// Три ядра разного назначения — для снимка цветов. Ставим рядом,
				// вручную задавая ячейки (ядро при установке заменяет прежнее).
				Net = Net ? Net : FindComponentByClass<UOUClientNet>();
				FVector GroundDir;
				FIntVector Base;
				if (AimTerrainDir(GroundDir))
				{
					Base = WorldToVoxel(GroundDir * (SurfaceRadius(GroundDir) + 75.f));
				}
				else
				{
					Base = WorldToVoxel(GetActorLocation() + GetActorForwardVector() * 600.f);
				}
				if (Net && Net->IsConnected())
				{
					if (FParse::Param(FCommandLine::Get(), TEXT("OUCoreKinds")))
					{
						Net->SendPlaceCore(16, 0, Base.X, Base.Y, Base.Z);
						Net->SendPlaceCore(16, 1, Base.X + 4, Base.Y, Base.Z);
						Net->SendPlaceCore(16, 2, Base.X + 8, Base.Y, Base.Z);
					}
					else
					{
						PlaceCoreBlock();
					}
				}
			}
			else if (CoreCount == 3 && FParse::Param(FCommandLine::Get(), TEXT("OUCoreKinds")))
			{
				// Проверка стыковки: к ядру приставляем воксели разного размера
				// (малый/средний/большой) — все должны пристыковаться по граням.
				Net = Net ? Net : FindComponentByClass<UOUClientNet>();
				if (Net && Net->IsConnected() && Net->CoreSize > 0)
				{
					const int32 Core = (Net->ActiveCoreId != 0) ? Net->ActiveCoreId : Net->OwnEntityId;
					const FIntVector C = Net->CoreCoord;
					// Ядро занимает N ячеек от C. Приставляем к грани +X:
					// малый блок — за дальней гранью ядра.
					const int32 N = (Net->CoreSize == 64) ? 3 : (Net->CoreSize == 32 ? 2 : 1);
					Net->SendBuild(Core, 0, C.X + N, C.Y, C.Z, 80, 200, 120, 1);
					// Средний — вплотную по +Y.
					Net->SendBuild(Core, 0, C.X, C.Y + N, C.Z, 200, 160, 80, 2);
					// Большой — вплотную по +Z.
					Net->SendBuild(Core, 0, C.X, C.Y, C.Z + N, 140, 140, 220, 3);
				}
			}
		}
	}

	// Диагностика `-OUMix`: детерминированно ставим воксели разного размера через
	// тот же планировщик (ComputeBuildTarget + PlaceBuildBlock), чтобы проверить
	// отсутствие наложений и отказов сервера. Размер циклится 1→2→3→1…
	if (FParse::Param(FCommandLine::Get(), TEXT("OUMix")))
	{
		if (!bBuildMenuOpen) { ToggleBuildMenu(); }
		SelectBuildPart((int32)EOUBuildPart::Hull);
		MixTimer += DeltaSeconds;
		if (MixTimer >= 1.2f && MixCount < 9)
		{
			MixTimer = 0.f;
			BuildBrush = (MixCount % 3) + 1;   // 1, 2, 3, 1, …
			++MixCount;
			PlaceBuildBlock();
			UE_LOG(LogOUChar, Log, TEXT("Смесь: клик %d, размер %d (%s)"),
				MixCount, BuildBrush, *BuildBrushName());
		}
	}

	// Диагностика `-OUAutoMenus`: по очереди открыть все окна в игре, чтобы одним
	// прогоном проверить их работоспособность (в логе — строка открытия каждого).
	if (FParse::Param(FCommandLine::Get(), TEXT("OUAutoMenus")))
	{
		if (AOUPlayerMenuController* MM = Cast<AOUPlayerMenuController>(GetController()))
		{
			AutoMenuTimer += DeltaSeconds;
			if (AutoMenuTimer >= 2.5f && AutoMenuCount < 12)
			{
				AutoMenuTimer = 0.f;
				++AutoMenuCount;
				// Закрываем все окна, затем открываем одно (проверяем каждое).
				MM->CloseAllWindows();
				switch (AutoMenuCount)
				{
				case 1: ToggleMarket();          break;
				case 2: ToggleInventory();       break;
				case 3: ToggleCraft();           break;
				case 4: ToggleScripts();         break;
				case 5: ToggleWorkshop();        break;
				case 6: ToggleOrg();             break;
				case 7: ToggleTalents();         break;
				case 8: ToggleVoxelMenu();       break;
				case 9: ToggleMap();             break;
				case 10: ToggleEventLog();       break;
				case 11: ToggleContacts();       break;
				case 12: MM->SetMenuCursor(false); break;   // закрыть всё
				}
			}
		}
	}

	// Тестовое строительство: открыть меню, поставить блоки и проверить
	// удаление/покраску (всё через сервер — правки авторитетные).
	if (bAutoBuild)
	{
		if (!bBuildMenuOpen)
		{
			ToggleBuildMenu();
		}
		BuildTimer += DeltaSeconds;
		if (BuildTimer >= 1.0f)
		{
			BuildTimer = 0.f;
			// Первые три шага — ставим блоки разного типа.
			if (BuiltCount < 3)
			{
				SelectBuildPart(BuiltCount);
				PlaceBuildBlock();
			}
			// Дальше — разово: покрасить и удалить ОДИН из поставленных блоков
			// по его координате (прицелом это ненадёжно: камера авто-хода
			// крутится и попадает в ядро конструкции).
			else if (!bAutoTestedPaint && BuiltCoords.Num() > 0)
			{
				bAutoTestedPaint = true;
				const FIntVector C = BuiltCoords.Last();
				if (Net && Net->IsConnected() && Net->OwnEntityId != 0)
				{
					const int32 Core = (Net->ActiveCoreId != 0) ? Net->ActiveCoreId : Net->OwnEntityId;
					Net->SendBuild(Core, /*Paint=*/2, C.X, C.Y, C.Z, 255, 220, 80);
					UE_LOG(LogOUChar, Log, TEXT("Автотест: покрасить (%d,%d,%d)"), C.X, C.Y, C.Z);
				}
			}
			else if (!bAutoTestedRemove && BuiltCoords.Num() > 0)
			{
				bAutoTestedRemove = true;
				const FIntVector C = BuiltCoords.Last();
				if (Net && Net->IsConnected() && Net->OwnEntityId != 0)
				{
					const int32 Core = (Net->ActiveCoreId != 0) ? Net->ActiveCoreId : Net->OwnEntityId;
					Net->SendBuild(Core, /*Remove=*/1, C.X, C.Y, C.Z, 0, 0, 0);
					UE_LOG(LogOUChar, Log, TEXT("Автотест: удалить (%d,%d,%d)"), C.X, C.Y, C.Z);
				}
			}
		}
	}
}

void AOUCharacterPawn::ResolveNet()
{
	if (Net)
	{
		return;
	}
	// Сетевой компонент живёт на корабле (носителе соединения).
	if (Ship && Ship->Net)
	{
		Net = Ship->Net;
		return;
	}
	if (AOUShipPawn* S = Ship)
	{
		if (UOUClientNet* Found = S->FindComponentByClass<UOUClientNet>())
		{
			Net = Found;
		}
	}
}
