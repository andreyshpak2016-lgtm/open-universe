// Copyright Open Universe. Все права защищены.

#include "OUShipPawn.h"
#include "OUClientNet.h"
#include "OUDeterministic.h"
#include "OUOriginManager.h"
#include "Components/StaticMeshComponent.h"
#include "Components/InputComponent.h"
#include "Camera/CameraComponent.h"
#include "GameFramework/SpringArmComponent.h"
#include "Engine/StaticMesh.h"
#include "UObject/ConstructorHelpers.h"
#include "Kismet/GameplayStatics.h"

DEFINE_LOG_CATEGORY_STATIC(LogOUShip, Log, All);

AOUShipPawn::AOUShipPawn()
{
	PrimaryActorTick.bCanEverTick = true;

	ShipMesh = CreateDefaultSubobject<UStaticMeshComponent>(TEXT("ShipMesh"));
	RootComponent = ShipMesh;

	// Видимый «корабль»: базовый куб движка. Настоящая блочная/воксельная
	// конструкция появится позже; сейчас важно, что игрок видит свой корабль.
	static ConstructorHelpers::FObjectFinder<UStaticMesh> ShipShape(
		TEXT("/Engine/BasicShapes/Cube.Cube"));
	if (ShipShape.Succeeded())
	{
		ShipMesh->SetStaticMesh(ShipShape.Object);
	}
	ShipMesh->SetRelativeScale3D(FVector(3.f, 1.f, 0.6f));

	// Физику UE **не используем** в открытом мире: кинематику считаем сами,
	// как на сервере. Это принципиально для детерминизма и античита.
	ShipMesh->SetSimulatePhysics(false);
	ShipMesh->SetCollisionEnabled(ECollisionEnabled::NoCollision);

	// Камера: штанга назад-вверх, вид от третьего лица.
	SpringArm = CreateDefaultSubobject<USpringArmComponent>(TEXT("SpringArm"));
	SpringArm->SetupAttachment(RootComponent);
	SpringArm->TargetArmLength = 1200.f;
	SpringArm->SetRelativeRotation(FRotator(-25.f, 0.f, 0.f));
	SpringArm->bDoCollisionTest = false;
	SpringArm->bUsePawnControlRotation = false;

	Camera = CreateDefaultSubobject<UCameraComponent>(TEXT("Camera"));
	Camera->SetupAttachment(SpringArm, USpringArmComponent::SocketName);
	Camera->bUsePawnControlRotation = false;

	AutoPossessPlayer = EAutoReceiveInput::Player0;
}

void AOUShipPawn::BeginPlay()
{
	Super::BeginPlay();

	State.Id = 1;
	State.Position = GetActorLocation();

	// Находим клиент сети: свой компонент или на уровне.
	if (!Net)
	{
		Net = FindComponentByClass<UOUClientNet>();
		if (!Net)
		{
			Net = NewObject<UOUClientNet>(this, TEXT("OUClientNet"));
			Net->RegisterComponent();
		}
	}

	// Менеджер начала координат (может отсутствовать — тогда работаем без него).
	OriginManager = Cast<AUOOriginManager>(
		UGameplayStatics::GetActorOfClass(GetWorld(), AUOOriginManager::StaticClass()));
}

void AOUShipPawn::SetupPlayerInputComponent(UInputComponent* PlayerInputComponent)
{
	Super::SetupPlayerInputComponent(PlayerInputComponent);
	// Управление настраивается в Blueprint/Enhanced Input; здесь каркас.
}

void AOUShipPawn::StepPrediction(const FOUInputFrame& Input, float Dt)
{
	using namespace OUDeterministic;

	// 1. Углы (как на сервере: yaw и pitch хранятся в состоянии).
	double Yaw = State.Yaw + Input.Yaw * Flight.MaxYawRate * Dt;
	double Pitch = State.Pitch + Input.Pitch * Flight.MaxPitchRate * Dt;
	Yaw = WrapPi(Yaw);
	Pitch = WrapPi(Pitch);
	const double PitchLimit = OUPi / 2.0 - 0.02;
	Pitch = FMath::Clamp(Pitch, -PitchLimit, PitchLimit);
	State.Yaw = static_cast<float>(Yaw);
	State.Pitch = static_cast<float>(Pitch);

	// 2. Направление «вперёд» из углов (та же формула, что на сервере).
	const double CP = DetCos(Pitch);
	const double SP = DetSin(Pitch);
	const double CY = DetCos(Yaw);
	const double SY = DetSin(Yaw);
	const FVector Forward(CP * SY, SP, CP * CY);

	// 3. Тяга + демпфирование.
	const FVector Thrust = Forward * (Input.Throttle * Flight.MaxThrust);
	const FVector Damp = State.Velocity * (-Flight.LinearDamping);
	State.Velocity += (Thrust + Damp) * Dt;

	// 4. Ограничение скорости.
	const double Speed = State.Velocity.Size();
	if (Speed > Flight.MaxSpeed)
	{
		State.Velocity = State.Velocity * (Flight.MaxSpeed / Speed);
	}

	// 5. Позиция.
	State.Position += State.Velocity * Dt;

	// 6. Применяем к актору (с учётом floating origin).
	if (OriginManager)
	{
		OriginManager->SetWorldPosition(this, State.Position);
	}
	else
	{
		SetActorLocation(State.Position);
	}
	SetActorRotation(FQuat(FRotator(State.Pitch * 180.0 / OUPi, State.Yaw * 180.0 / OUPi, 0.0)));
}

void AOUShipPawn::ApplyAuthoritative(const FOUEntityState& Authoritative)
{
	// Реконсиляция: встать на авторитетную точку. Полноценная версия
	// переигрывает неподтверждённые кадры (см. `ou_client::reconcile`).
	State.Position = Authoritative.Position;
	State.Velocity = Authoritative.Velocity;
	State.Yaw = Authoritative.Yaw;
	State.Pitch = Authoritative.Pitch;

	if (OriginManager)
	{
		OriginManager->SetWorldPosition(this, State.Position);
	}
	else
	{
		SetActorLocation(State.Position);
	}
}

void AOUShipPawn::Tick(float DeltaSeconds)
{
	Super::Tick(DeltaSeconds);

	// Фиксированный шаг: сеть и предсказание идут на 30 Гц, рендер — быстрее.
	const int32 Steps = FMath::Max(1, FMath::RoundToInt(DeltaSeconds / FixedDeltaSeconds));

	// Заполняем ввод из осей (настроит Blueprint) и предсказываем.
	CurrentInput.Seq += 1;
	for (int32 i = 0; i < Steps; ++i)
	{
		StepPrediction(CurrentInput, FixedDeltaSeconds);
	}

	if (Net && Net->IsConnected())
	{
		Net->SendInput(CurrentInput);
		Net->Poll();

		// Если пришёл снимок с нашей сущностью — реконсиляция.
		for (const FOUEntityState& E : Net->LatestSnapshot.Entities)
		{
			if (E.Id == State.Id)
			{
				ApplyAuthoritative(E);
				break;
			}
		}
	}

	// Переносим начало координат, если улетели далеко.
	if (OriginManager)
	{
		OriginManager->UpdateOrigin(this);
	}
}
