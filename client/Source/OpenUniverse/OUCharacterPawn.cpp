// Copyright Open Universe. Все права защищены.

#include "OUCharacterPawn.h"
#include "OUShipPawn.h"
#include "OUClientNet.h"
#include "OUVoxelPlanet.h"
#include "OUInterop.h"
#include "Components/CapsuleComponent.h"
#include "Components/StaticMeshComponent.h"
#include "GameFramework/SpringArmComponent.h"
#include "Camera/CameraComponent.h"
#include "Components/InputComponent.h"
#include "Engine/StaticMesh.h"
#include "Materials/MaterialInterface.h"
#include "Kismet/GameplayStatics.h"
#include "UObject/ConstructorHelpers.h"

DEFINE_LOG_CATEGORY_STATIC(LogOUChar, Log, All);

AOUCharacterPawn::AOUCharacterPawn()
{
	PrimaryActorTick.bCanEverTick = true;

	Capsule = CreateDefaultSubobject<UCapsuleComponent>(TEXT("Capsule"));
	Capsule->InitCapsuleSize(CapsuleRadius, CapsuleHalfHeight);
	Capsule->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	SetRootComponent(Capsule);

	// Видимая фигурка: капсула-туловище (до появления настоящей модели).
	// Цвет задаём через материал, иначе фигурка сливается с фоном.
	BodyMesh = CreateDefaultSubobject<UStaticMeshComponent>(TEXT("BodyMesh"));
	BodyMesh->SetupAttachment(Capsule);
	BodyMesh->SetRelativeScale3D(FVector(0.9f, 0.9f, 1.05f));
	BodyMesh->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	static ConstructorHelpers::FObjectFinder<UStaticMesh> BodyShape(
		TEXT("/Engine/BasicShapes/Cylinder.Cylinder"));
	if (BodyShape.Succeeded())
	{
		BodyMesh->SetStaticMesh(BodyShape.Object);
	}
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> BodyMat(
		TEXT("/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"));
	if (BodyMat.Succeeded())
	{
		BodyMesh->SetMaterial(0, BodyMat.Object);
	}

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

	// Цвет фигурки персонажа (синий) — чтобы она читалась в кадре.
	if (BodyMesh)
	{
		BodyMesh->SetVectorParameterValueOnMaterials(TEXT("Color"), FVector(0.20f, 0.45f, 0.95f));
	}

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
	// Слегка смотрим вниз, чтобы в кадр попадал и ландшафт/горизонт.
	ViewPitch = -0.10f;

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
	return 100000.f;
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
}

void AOUCharacterPawn::AxisForward(float Value) { InputForward = FMath::Clamp(Value, -1.f, 1.f); }
void AOUCharacterPawn::AxisRight(float Value) { InputRight = FMath::Clamp(Value, -1.f, 1.f); }
void AOUCharacterPawn::AxisLookYaw(float Value) { InputLookYaw = FMath::Clamp(Value, -1.f, 1.f); }
void AOUCharacterPawn::AxisLookPitch(float Value) { InputLookPitch = FMath::Clamp(Value, -1.f, 1.f); }
void AOUCharacterPawn::OnJumpPressed() { bJumpQueued = true; }
void AOUCharacterPawn::OnJumpReleased() { }
void AOUCharacterPawn::OnExitPressed() { bExitHeld = true; ExitHoldTime = 0.f; }
void AOUCharacterPawn::OnExitReleased() { bExitHeld = false; ExitHoldTime = 0.f; }

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
	const FVector Wish = (Forward * InputForward + Right * InputRight).GetSafeNormal();
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
}
