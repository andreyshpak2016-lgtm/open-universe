// Copyright Open Universe. Все права защищены.

#include "OUCharacterPawn.h"
#include "OUShipPawn.h"
#include "OUClientNet.h"
#include "OUVoxelPlanet.h"
#include "OUInterop.h"
#include "Components/CapsuleComponent.h"
#include "Components/StaticMeshComponent.h"
#include "Components/SceneComponent.h"
#include "GameFramework/SpringArmComponent.h"
#include "Camera/CameraComponent.h"
#include "Components/InputComponent.h"
#include "Components/PointLightComponent.h"
#include "Engine/StaticMesh.h"
#include "Materials/MaterialInterface.h"
#include "Kismet/GameplayStatics.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "OUProjectile.h"
#include "OUBuildBlock.h"
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
	Capsule->SetCollisionEnabled(ECollisionEnabled::NoCollision);
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

	// Камера от третьего лица через плечо: видно и персонажа, и корабль рядом.
	// Сдвиг вбок — чтобы собственная фигурка не закрывала обзор.
	SpringArm = CreateDefaultSubobject<USpringArmComponent>(TEXT("SpringArm"));
	SpringArm->SetupAttachment(Capsule);
	SpringArm->SetRelativeLocation(FVector(-20.f, 60.f, CapsuleHalfHeight * 0.7f));
	SpringArm->TargetArmLength = 900.f;
	SpringArm->bDoCollisionTest = false;
	SpringArm->bUsePawnControlRotation = false;

	Camera = CreateDefaultSubobject<UCameraComponent>(TEXT("Camera"));
	Camera->SetupAttachment(SpringArm, USpringArmComponent::SocketName);
	Camera->bUsePawnControlRotation = false;

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

	// Персонаж — впереди базы (наружу), корабль — впереди-справа, в кадре.
	// База остаётся ЗА камерой, поэтому в кадре: персонаж, корабль, горизонт.
	FVector CharPos = BaseSpawn + Fwd * 2200.f;
	// Корабль — рядом (5–7 м вперёд-вправо), виден в кадре как отдельный объект.
	FVector ShipPos = CharPos + Fwd * 420.f + Side * 320.f;
	if (AOUVoxelPlanet* Planet = AOUVoxelPlanet::FindHomePlanet(World))
	{
		CharPos = CharPos.GetSafeNormal() * (Planet->SurfaceRadiusAt(CharPos.GetSafeNormal()) + 95.f);
		ShipPos = ShipPos.GetSafeNormal() * (Planet->SurfaceRadiusAt(ShipPos.GetSafeNormal()) + 120.f);
	}

	// Курс персонажа — «наружу» от базы (по Fwd).
	const float LookYaw = FMath::Atan2(Fwd.Y, Fwd.X);
	PlaceAt(CharPos, LookYaw);
	// Смотрим чуть вверх: в кадр попадают и земля, и небо с облаками.
	ViewPitch = 0.10f;

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

	UE_LOG(LogOUChar, Log, TEXT("Персонаж создан: ходьба по поверхности, корабль рядом"));
}

float AOUCharacterPawn::SurfaceRadius(const FVector& DirUE) const
{
	if (AOUVoxelPlanet* Planet = AOUVoxelPlanet::FindHomePlanet(GetWorld()))
	{
		return Planet->SurfaceRadiusAt(DirUE);   // см UE
	}
	// Запасной вариант: радиус планеты в UE-сантиметрах.
	return 10000000.f;
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
	PlayerInputComponent->BindAxis(TEXT("Throttle"), this, &AOUCharacterPawn::AxisForward);
	PlayerInputComponent->BindAxis(TEXT("Yaw"), this, &AOUCharacterPawn::AxisRight);
	PlayerInputComponent->BindAxis(TEXT("LookYaw"), this, &AOUCharacterPawn::AxisLookYaw);
	PlayerInputComponent->BindAxis(TEXT("LookPitch"), this, &AOUCharacterPawn::AxisLookPitch);
	PlayerInputComponent->BindAction(TEXT("Jump"), IE_Pressed, this, &AOUCharacterPawn::OnJumpPressed);
	PlayerInputComponent->BindAction(TEXT("Jump"), IE_Released, this, &AOUCharacterPawn::OnJumpReleased);
	PlayerInputComponent->BindAction(TEXT("ExitShip"), IE_Pressed, this, &AOUCharacterPawn::OnExitPressed);
	PlayerInputComponent->BindAction(TEXT("ExitShip"), IE_Released, this, &AOUCharacterPawn::OnExitReleased);
	// Огонь: зажимание кнопки ведёт непрерывную стрельбу (окно оружия).
	PlayerInputComponent->BindAction(TEXT("FireWeapon"), IE_Pressed, this, &AOUCharacterPawn::FireWeapon);
	// Строительство: B открывает/закрывает меню, ЛКМ ставит выбранный блок.
	PlayerInputComponent->BindAction(TEXT("BuildMode"), IE_Pressed, this, &AOUCharacterPawn::ToggleBuildMenu);
	PlayerInputComponent->BindAction(TEXT("BuildPlace"), IE_Pressed, this, &AOUCharacterPawn::PlaceBuildBlock);
	// Выбор блока в палитре: колесо мыши.
	PlayerInputComponent->BindAction(TEXT("BuildNext"), IE_Pressed, this, &AOUCharacterPawn::OnBuildNext);
	PlayerInputComponent->BindAction(TEXT("BuildPrev"), IE_Pressed, this, &AOUCharacterPawn::OnBuildPrev);
}

void AOUCharacterPawn::AxisForward(float Value) { InputForward = FMath::Clamp(Value, -1.f, 1.f); }
void AOUCharacterPawn::AxisRight(float Value) { InputRight = FMath::Clamp(Value, -1.f, 1.f); }
void AOUCharacterPawn::AxisLookYaw(float Value) { InputLookYaw = FMath::Clamp(Value, -1.f, 1.f); }
void AOUCharacterPawn::AxisLookPitch(float Value) { InputLookPitch = FMath::Clamp(Value, -1.f, 1.f); }
void AOUCharacterPawn::OnJumpPressed() { bJumpQueued = true; }
void AOUCharacterPawn::OnJumpReleased() { }
void AOUCharacterPawn::OnExitPressed() { bExitHeld = true; ExitHoldTime = 0.f; }
void AOUCharacterPawn::OnExitReleased() { bExitHeld = false; ExitHoldTime = 0.f; }
void AOUCharacterPawn::OnBuildNext() { if (bBuildMenuOpen) { SelectBuildPart((BuildSelection + 1) % (int32)EOUBuildPart::Count); } }
void AOUCharacterPawn::OnBuildPrev() { if (bBuildMenuOpen) { SelectBuildPart((BuildSelection + (int32)EOUBuildPart::Count - 1) % (int32)EOUBuildPart::Count); } }

void AOUCharacterPawn::ToggleBuildMenu()
{
	bBuildMenuOpen = !bBuildMenuOpen;
	UE_LOG(LogOUChar, Log, TEXT("Меню строительства: %s (выбран %s)"),
		bBuildMenuOpen ? TEXT("открыто") : TEXT("закрыто"), *BuildPartName(BuildSelection));
}

void AOUCharacterPawn::SelectBuildPart(int32 Index)
{
	const int32 Count = (int32)EOUBuildPart::Count;
	if (Count > 0)
	{
		BuildSelection = ((Index % Count) + Count) % Count;
	}
}

FString AOUCharacterPawn::BuildPartName(int32 Index)
{
	switch ((EOUBuildPart)Index)
	{
	case EOUBuildPart::Hull:  return TEXT("Корпус");
	case EOUBuildPart::Panel: return TEXT("Панель");
	case EOUBuildPart::Beam:  return TEXT("Балка");
	case EOUBuildPart::Light: return TEXT("Свет");
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

	// Онлайн: команду строительства отправляем на СЕРВЕР — он авторитетно
	// проверяет (целостность, права, лимиты) и рассылает правку всем. Блок
	// появится, когда придёт авторитетная правка (ApplyServerBuildEdits).
	if (Net && Net->IsConnected() && Net->OwnEntityId != 0)
	{
		// Следующая ячейка в ряду (ядро конструкции — (0,0,0) на сервере).
		const int32 X = BuiltCount + 1;
		const FLinearColor Col = BuildPartColor(BuildSelection);
		Net->SendBuild(Net->OwnEntityId, /*Place=*/0, X, 0, 0,
			FMath::RoundToInt(Col.R * 255.f),
			FMath::RoundToInt(Col.G * 255.f),
			FMath::RoundToInt(Col.B * 255.f));
		++BuiltCount;
		UE_LOG(LogOUChar, Log, TEXT("Строительство: отправлена команда на сервер (ячейка %d,0,0)"), X);
		return;
	}

	// Оффлайн (без сервера) — ставим локально, как раньше, для одиночной игры.
	const float BlockSize = 120.f;
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

FVector AOUCharacterPawn::VoxelWorld(const FVector& ServerCoord) const
{
	// ВАЖНО: базис считаем в ОСЯХ UE (как и «вперёд» персонажа), иначе сетка
	// окажется повёрнутой относительно игрока: серверные оси (y — вверх) и
	// UE-оси (z — вверх) не совпадают, и проекция «мирового +X» даёт разное.
	const double VoxelCm = 150.0;   // 1.5 м на воксель в см UE
	const FVector BaseUE = AOUVoxelPlanet::HomeSpawn(2.0f);   // UE-см
	const FVector UpUE = BaseUE.GetSafeNormal();
	FVector FwdUE = FVector::VectorPlaneProject(FVector(1.f, 0.f, 0.f), UpUE).GetSafeNormal();
	if (FwdUE.IsNearlyZero())
	{
		FwdUE = FVector::VectorPlaneProject(FVector::ForwardVector, UpUE).GetSafeNormal();
	}
	const FVector RightUE = FVector::CrossProduct(UpUE, FwdUE).GetSafeNormal();
	// Якорь — сразу перед персонажем (он в 2200 см от базы, смотрит по FwdUE).
	const FVector CharUE = BaseUE + FwdUE * 2200.f;
	const FVector AnchorUE = CharUE + FwdUE * (6.0 * VoxelCm);
	return AnchorUE
		+ FwdUE * (ServerCoord.X * VoxelCm)
		+ UpUE * (ServerCoord.Y * VoxelCm + VoxelCm * 0.5)
		+ RightUE * (ServerCoord.Z * VoxelCm);
}

void AOUCharacterPawn::ApplyServerBuildEdits()
{
	if (!Net || Net->PendingBuildEdits.Num() == 0)
	{
		return;
	}
	UWorld* World = GetWorld();
	if (!World)
	{
		return;
	}
	// Забираем очередь правок (сервер уже авторитетно их применил).
	int64 MaxRev = LastAppliedRevision;
	for (const FOUBuildEdit& E : Net->PendingBuildEdits)
	{
		const FIntVector Key(E.X, E.Y, E.Z);
		if (E.Action == 0) // Place
		{
			TObjectPtr<AOUBuildBlock>* Found = ServerBlocks.Find(Key);
			if (!Found)
			{
				const FVector Pos = VoxelWorld(FVector(E.X, E.Y, E.Z));
				AOUBuildBlock* Block = World->SpawnActor<AOUBuildBlock>(
					AOUBuildBlock::StaticClass(), FTransform(FRotator::ZeroRotator, Pos));
				if (Block)
				{
					Block->BlockSize = 150.f;   // 1.5 м — совпадает с шагом сетки
					Block->BlockColor = FLinearColor(E.R / 255.f, E.G / 255.f, E.B / 255.f);
					Block->ApplyLook();
					ServerBlocks.Add(Key, Block);
					UE_LOG(LogOUChar, Log, TEXT("Авторитетный блок (%d,%d,%d) от сервера"), E.X, E.Y, E.Z);
				}
			}
		}
		else if (E.Action == 1) // Remove
		{
			if (TObjectPtr<AOUBuildBlock>* Found = ServerBlocks.Find(Key))
			{
				if (*Found) { (*Found)->Destroy(); }
				ServerBlocks.Remove(Key);
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
		MaxRev = FMath::Max(MaxRev, E.Revision);
	}
	LastAppliedRevision = MaxRev;
	// Подтверждаем приём — сервер не будет слать эти правки снова.
	const int32 Entity = (Net->OwnEntityId != 0) ? Net->OwnEntityId : 1;
	Net->SendBuildAck(Entity, MaxRev);
	Net->PendingBuildEdits.Reset();
}

void AOUCharacterPawn::FireWeapon()
{
	UWorld* World = GetWorld();
	if (!World || !GunMesh || FireTimer > 0.f)
	{
		return;
	}
	FireTimer = FireCooldown;

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
	const FVector Pos = GetActorLocation();
	const FVector Up = Pos.GetSafeNormal();          // «вверх» — от центра планеты
	if (Up.IsNearlyZero())
	{
		return;
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
	const FVector Wish = (Forward * FwdInput + Right * InputRight).GetSafeNormal();
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

	// Шаг позиции.
	FVector NewPos = Pos + Velocity * DeltaSeconds;

	// Привязка к рельефу: не проваливаемся под поверхность.
	const FVector Dir = NewPos.GetSafeNormal();
	const double Surface = SurfaceRadius(Dir) + CapsuleHalfHeight;
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
	SetActorRotation(FRotationMatrix::MakeFromXZ(Forward, Up).Rotator());
	// Камера: штанга в ЛОКАЛЬНЫХ осях пешки (локальный X = курс, Z = вверх),
	// поэтому достаточно относительного поворота по тангажу — без мировой
	// математики, которая конфликтует с родительским трансформом.
	if (SpringArm)
	{
		SpringArm->SetRelativeRotation(FRotator(FMath::RadiansToDegrees(ViewPitch), 0.f, 0.f));
	}

	// Анимация бега/ходьбы (процедурная, по горизонтальной скорости).
	const float HSpd = FVector::VectorPlaneProject(Velocity, Up).Size();
	AnimateLocomotion(DeltaSeconds, HSpd);

	// Авторитетное строительство: применяем правки, пришедшие от сервера.
	ResolveNet();
	ApplyServerBuildEdits();

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

	// Тестовое строительство: открыть меню и поставить пару блоков.
	if (bAutoBuild)
	{
		if (!bBuildMenuOpen)
		{
			ToggleBuildMenu();
		}
		BuildTimer += DeltaSeconds;
		if (BuildTimer >= 1.0f && BuiltCount < 3)
		{
			BuildTimer = 0.f;
			SelectBuildPart(BuiltCount);
			PlaceBuildBlock();
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
