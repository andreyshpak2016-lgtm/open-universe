// Copyright Open Universe. Все права защищены.

#include "OUShipPawn.h"
#include "OUClientNet.h"
#include "OUDeterministic.h"
#include "OUInterop.h"
#include "OUOriginManager.h"
#include "OUVoxelPlanet.h"
#include "Components/StaticMeshComponent.h"
#include "Components/InputComponent.h"
#include "Camera/CameraComponent.h"
#include "GameFramework/SpringArmComponent.h"
#include "Engine/StaticMesh.h"
#include "UObject/ConstructorHelpers.h"
#include "Kismet/GameplayStatics.h"

DEFINE_LOG_CATEGORY_STATIC(LogOUShip, Log, All);

namespace
{
	/** Радиус домашней планеты в серверных единицах (совпадает с сервером). */
	constexpr double OUPlanetRadius = 1000.0;
	/** Ускорение свободного падения у поверхности, ед/с² (как на сервере). */
	constexpr double OUSurfaceGravity = 18.0;
	/** До какого расстояния (в радиусах) действует гравитация. */
	constexpr double OUGravityRangeRadii = 30.0;

	/**
	 * Гравитационное ускорение в точке (серверные оси).
	 * Та же формула обратных квадратов, что в `ou_sim::gravity_at`.
	 */
	FVector GravityAt(const FVector& Pos)
	{
		const double Dist = Pos.Size();
		if (Dist < 1e-6)
		{
			return FVector::ZeroVector;
		}
		if (Dist > OUPlanetRadius * OUGravityRangeRadii)
		{
			return FVector::ZeroVector; // космос
		}
		const FVector Up = Pos / Dist;
		const double R = FMath::Max(Dist, OUPlanetRadius);
		const double Ratio = OUPlanetRadius / R;
		return Up * (-OUSurfaceGravity * Ratio * Ratio);
	}
}

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
	SpringArm->TargetArmLength = 2500.f;
	SpringArm->SetRelativeRotation(FRotator(-20.f, 0.f, 0.f));
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
	// Точка появления — на домашней планете (совпадает с сервером).
	// Пока сервер не подтвердил позицию, ставим корабль на сушу у поверхности.
	const FVector SpawnUE = AOUVoxelPlanet::HomeSpawn(80.f * OUInterop::ServerUnitToCm);
	State.Position = OUInterop::UEToServer(SpawnUE);
	SetActorLocation(SpawnUE);

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

	// Меш-заглушка для чужих кораблей (грузим один раз).
	if (!RemoteShipMesh)
	{
		RemoteShipMesh = LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Cube.Cube"));
	}

	// Менеджер начала координат (может отсутствовать — тогда работаем без него).
	OriginManager = Cast<AUOOriginManager>(
		UGameplayStatics::GetActorOfClass(GetWorld(), AUOOriginManager::StaticClass()));
}

void AOUShipPawn::SetupPlayerInputComponent(UInputComponent* PlayerInputComponent)
{
	Super::SetupPlayerInputComponent(PlayerInputComponent);
	if (!PlayerInputComponent)
	{
		return;
	}
	// Классический ввод (без ассетов): работает сразу, без Blueprint.
	PlayerInputComponent->BindAxis(TEXT("Throttle"), this, &AOUShipPawn::AxisThrottle);
	PlayerInputComponent->BindAxis(TEXT("Yaw"), this, &AOUShipPawn::AxisYaw);
	PlayerInputComponent->BindAxis(TEXT("Pitch"), this, &AOUShipPawn::AxisPitch);
	PlayerInputComponent->BindAction(TEXT("Fire"), IE_Pressed, this, &AOUShipPawn::StartFire);
	PlayerInputComponent->BindAction(TEXT("Fire"), IE_Released, this, &AOUShipPawn::StopFire);
}

void AOUShipPawn::AxisThrottle(float Value)
{
	CurrentInput.Throttle = FMath::Clamp(Value, -1.f, 1.f);
}

void AOUShipPawn::AxisYaw(float Value)
{
	CurrentInput.Yaw = FMath::Clamp(Value, -1.f, 1.f);
}

void AOUShipPawn::AxisPitch(float Value)
{
	CurrentInput.Pitch = FMath::Clamp(Value, -1.f, 1.f);
}

void AOUShipPawn::StartFire()
{
	CurrentInput.bFire = true;
}

void AOUShipPawn::StopFire()
{
	CurrentInput.bFire = false;
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
	const FVector Forward(
		DetCos(Pitch) * DetSin(Yaw),
		DetSin(Pitch),
		DetCos(Pitch) * DetCos(Yaw));

	// 3. Тяга + демпфирование + гравитация планеты (как на сервере:
	//    `integrate_with_accel` добавляет внешнее ускорение до шага).
	const FVector Thrust = Forward * (Input.Throttle * Flight.MaxThrust);
	const FVector Damp = State.Velocity * (-Flight.LinearDamping);
	const FVector Gravity = GravityAt(State.Position);
	State.Velocity += (Thrust + Damp + Gravity) * Dt;

	// 4. Ограничение скорости.
	const double Speed = State.Velocity.Size();
	if (Speed > Flight.MaxSpeed)
	{
		State.Velocity = State.Velocity * (Flight.MaxSpeed / Speed);
	}

	// 5. Позиция.
	State.Position += State.Velocity * Dt;
	ApplyStateToActor();
}

void AOUShipPawn::ApplyStateToActor()
{
	// Позиция: State.Position (серверные оси) → UE.
	const FVector PosUE = OUInterop::ServerToUE(State.Position);
	if (OriginManager)
	{
		OriginManager->SetWorldPosition(this, PosUE);
	}
	else
	{
		SetActorLocation(PosUE);
	}
	// Ориентация: куб-меш смотрит вдоль +X, поэтому направляем его
	// вектором «вперёд» (переведённым в оси UE, без масштаба).
	SetActorRotation(OUInterop::ServerDirToUE(ForwardVector()).Rotation());
}

FVector AOUShipPawn::ForwardVector() const
{
	using namespace OUDeterministic;
	// Вперёд в серверных осях (вверх = Y): (cp·sy, sp, cp·cy).
	const double CP = DetCos(State.Pitch);
	const double SP = DetSin(State.Pitch);
	const double SY = DetSin(State.Yaw);
	const double CY = DetCos(State.Yaw);
	return FVector(CP * SY, SP, CP * CY);
}

FVector AOUShipPawn::ForwardVectorOf(const FOUEntityState& E)
{
	using namespace OUDeterministic;
	const double CP = DetCos(E.Pitch);
	const double SP = DetSin(E.Pitch);
	const double SY = DetSin(E.Yaw);
	const double CY = DetCos(E.Yaw);
	return FVector(CP * SY, SP, CP * CY);
}

void AOUShipPawn::ApplyAuthoritative(const FOUEntityState& Authoritative)
{
	// Реконсиляция: встать на авторитетную точку. Полноценная версия
	// переигрывает неподтверждённые кадры (см. `ou_client::reconcile`).
	State.Position = Authoritative.Position;
	State.Velocity = Authoritative.Velocity;
	State.Yaw = Authoritative.Yaw;
	State.Pitch = Authoritative.Pitch;

	ApplyStateToActor();
}

void AOUShipPawn::Tick(float DeltaSeconds)
{
	Super::Tick(DeltaSeconds);

	// Фиксированный шаг: сеть и предсказание идут на 30 Гц, рендер — быстрее.
	const int32 Steps = FMath::Max(1, FMath::RoundToInt(DeltaSeconds / FixedDeltaSeconds));

	// Предсказываем движение нашим же шагом (сеть не требуется).
	CurrentInput.Seq += 1;
	for (int32 i = 0; i < Steps; ++i)
	{
		StepPrediction(CurrentInput, FixedDeltaSeconds);
	}

	if (Net)
	{
		// Опрашиваем сеть всегда: так мы узнаём свой id и состояние мира.
		Net->Poll();

		// Свой id берём у сервера (он сообщает его в welcome).
		if (Net->OwnEntityId != 0)
		{
			State.Id = Net->OwnEntityId;
		}

		if (Net->IsConnected())
		{
			if (bInputEnabled)
			{
				Net->SendInput(CurrentInput);
				// Запоминаем кадр: сервер подтвердит его seq, и мы отбросим.
				PendingInputs.Add(CurrentInput);
				if (PendingInputs.Num() > 128)
				{
					PendingInputs.RemoveAt(0);
				}
			}
			// Реконсиляция: принимаем авторитетное состояние своей сущности
			// и переигрываем ещё не подтверждённые кадры.
			for (const FOUEntityState& E : Net->LatestSnapshot.Entities)
			{
				if (E.Id == State.Id)
				{
					ReconcileTo(E, Net->LatestSnapshot.AckInputSeq);
					break;
				}
			}
			// Рисуем чужие корабли из последнего снимка.
			UpdateRemotePlayers();
		}
	}

	// Переносим начало координат, если улетели далеко.
	if (OriginManager)
	{
		OriginManager->UpdateOrigin(this);
	}
}

void AOUShipPawn::ReconcileTo(const FOUEntityState& Authoritative, int32 AckSeq)
{
	// 1. Встаём на авторитетное состояние.
	ApplyAuthoritative(Authoritative);
	// 2. Отбрасываем подтверждённые кадры.
	PendingInputs.RemoveAll([AckSeq](const FOUInputFrame& In) { return In.Seq <= AckSeq; });
	// 3. Переигрываем оставшиеся — так предсказание снова совпадёт с сервером.
	for (const FOUInputFrame& In : PendingInputs)
	{
		StepPrediction(In, FixedDeltaSeconds);
	}
}

AActor* AOUShipPawn::GetOrCreateRemoteActor(int32 EntityId)
{
	if (TObjectPtr<AActor>* Found = RemoteActors.Find(EntityId))
	{
		return *Found;
	}
	UWorld* World = GetWorld();
	if (!World)
	{
		return nullptr;
	}
	AActor* Actor = World->SpawnActor<AActor>(AActor::StaticClass(), FTransform::Identity);
	if (!Actor)
	{
		return nullptr;
	}
	UStaticMeshComponent* Comp = NewObject<UStaticMeshComponent>(Actor);
	Comp->SetupAttachment(Actor->GetRootComponent());
	Comp->RegisterComponent();
	if (RemoteShipMesh)
	{
		Comp->SetStaticMesh(RemoteShipMesh);
	}
	Comp->SetRelativeScale3D(FVector(3.f, 1.f, 0.6f));
	Comp->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	Actor->AddInstanceComponent(Comp);
	// Красноватый оттенок, чтобы отличать чужие корабли от своего.
	Comp->SetVectorParameterValueOnMaterials(TEXT("Color"), FVector(1.f, 0.35f, 0.35f));
	RemoteActors.Add(EntityId, Actor);
	return Actor;
}

void AOUShipPawn::UpdateRemotePlayers()
{
	const FOUSnapshot& Snap = Net->LatestSnapshot;
	TSet<int32> Seen;
	for (const FOUEntityState& E : Snap.Entities)
	{
		if (E.Id == State.Id)
		{
			continue; // свой корабль уже есть
		}
		Seen.Add(E.Id);
		AActor* Actor = GetOrCreateRemoteActor(E.Id);
		if (Actor)
		{
			// E.Position — в серверных осях: переводим в UE (с масштабом).
			Actor->SetActorLocation(OUInterop::ServerToUE(E.Position));
			// Чужие корабли смотрят туда же, куда летят.
			const FVector FwdUE = OUInterop::ServerDirToUE(ForwardVectorOf(E));
			Actor->SetActorRotation(FwdUE.Rotation());
		}
	}

	// Снимки — дельта: отсутствие сущности в дельте НЕ значит, что она ушла.
	// Полный список видимых сущностей приходит только в baseline, поэтому
	// удаляем «призраков» лишь тогда, когда это действительно достоверно.
	if (Snap.bBaseline)
	{
		for (auto It = RemoteActors.CreateIterator(); It; ++It)
		{
			if (!Seen.Contains(It.Key()) && It.Value())
			{
				It.Value()->Destroy();
				It.RemoveCurrent();
			}
		}
	}
}
